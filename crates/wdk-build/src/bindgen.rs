// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

use std::borrow::Borrow;

use bindgen::{
    callbacks::{ItemInfo, ItemKind, ParseCallbacks},
    Builder,
};
use grep::{
    regex::RegexMatcher,
    searcher::{sinks::UTF8, SearcherBuilder},
};
use walkdir::WalkDir;

use crate::{Config, ConfigError};

/// An extension trait that provides a way to create a [`bindgen::Builder`]
/// configured for generating bindings to the wdk
pub trait BuilderExt {
    /// Returns a `bindgen::Builder` with the default configuration for
    /// generation of bindings to the WDK
    ///
    /// # Errors
    ///
    /// Implementation may return `wdk_build::ConfigError` if it fails to create
    /// a builder
    fn wdk_default(config: impl Borrow<Config>) -> Result<Builder, ConfigError>;

    /// Returns self (`Builder`) with opaque types for Windows handle types
    /// added to the bindgen configuration. This is necessary as the bindgen
    /// resolution for the `DECLARE_HANDLE` macro does not properly detect these
    /// generated handle types as opaque.
    ///
    /// # Errors
    ///
    /// Implementation may return a `wdk_build::ConfigError` if unable to
    /// resolve include paths
    fn opaque_windows_handle_types(
        self,
        config: impl Borrow<Config>,
    ) -> Result<Builder, ConfigError>;
}

#[derive(Debug)]
struct WdkCallbacks {
    wdf_function_table_symbol_name: Option<String>,
}

impl BuilderExt for Builder {
    /// Returns a `bindgen::Builder` with the default configuration for
    /// generation of bindings to the WDK
    ///
    /// # Errors
    ///
    /// Will return `wdk_build::ConfigError` if any of the resolved include or
    /// library paths do not exist
    fn wdk_default(config: impl Borrow<Config>) -> Result<Self, ConfigError> {
        let config = config.borrow();

        let builder = Self::default()
            .use_core() // Can't use std for kernel code
            .derive_default(true) // allows for default initializing structs
            // CStr types are safer and easier to work with when interacting with string constants
            // from C
            .generate_cstr(true)
            // Building in eWDK can pollute system search path when clang-sys tries to detect
            // c_search_paths
            .detect_include_paths(false)
            .clang_args(config.include_paths()?.map(|include_path| {
                format!(
                    "--include-directory={}",
                    include_path
                        .to_str()
                        .expect("Non Unicode paths are not supported")
                )
            }))
            .clang_args(
                config
                    .preprocessor_definitions()
                    .map(|(key, value)| {
                        format!(
                            "--define-macro={key}{}",
                            value.map(|v| format!("={v}")).unwrap_or_default()
                        )
                    })
                    .chain(Config::wdk_bindgen_compiler_flags()),
            )
            .blocklist_item("ExAllocatePoolWithTag") // Deprecated
            .blocklist_item("ExAllocatePoolWithQuotaTag") // Deprecated
            .blocklist_item("ExAllocatePoolWithTagPriority") // Deprecated
            .blocklist_item("ExAllocatePool") // Deprecated
            .blocklist_item("USBD_CalculateUsbBandwidth") // Deprecated
            .blocklist_item("USBD_CreateConfigurationRequest") // Deprecated
            .blocklist_item("USBD_Debug_LogEntry") // Deprecated
            .blocklist_item("USBD_GetUSBDIVersion") // Deprecated
            .blocklist_item("USBD_ParseConfigurationDescriptor") // Deprecated
            .blocklist_item("USBD_QueryBusTime") // Deprecated
            .blocklist_item("USBD_RegisterHcFilter") // Deprecated
            .blocklist_item("IOCTL_USB_DIAG_IGNORE_HUBS_OFF") // Deprecated/Internal-Use-Only
            .blocklist_item("IOCTL_USB_DIAG_IGNORE_HUBS_ON") // Deprecated/Internal-Use-Only
            .blocklist_item("IOCTL_USB_DIAGNOSTIC_MODE_OFF") // Deprecated/Internal-Use-Only
            .blocklist_item("IOCTL_USB_DIAGNOSTIC_MODE_ON") // Deprecated/Internal-Use-Only
            .blocklist_item("IOCTL_USB_GET_HUB_CAPABILITIES") // Deprecated/Internal-Use-Only
            .blocklist_item("IOCTL_USB_HCD_DISABLE_PORT") // Deprecated/Internal-Use-Only
            .blocklist_item("IOCTL_USB_HCD_ENABLE_PORT") // Deprecated/Internal-Use-Only
            .blocklist_item("IOCTL_USB_HCD_GET_STATS_1") // Deprecated/Internal-Use-Only
            .blocklist_item("IOCTL_USB_HCD_GET_STATS_2") // Deprecated/Internal-Use-Only
            .blocklist_item("IOCTL_USB_RESET_HUB") // Deprecated/Internal-Use-Only
            .opaque_type("_KGDTENTRY64") // No definition in WDK
            .opaque_type("_KIDTENTRY64") // No definition in WDK
            // FIXME: bitfield generated with non-1byte alignment in _MCG_CAP
            .blocklist_item(".*MCG_CAP(?:__bindgen.*)?")
            .blocklist_item(".*WHEA_XPF_MCA_SECTION")
            .blocklist_item(".*WHEA_ARM_BUS_ERROR(?:__bindgen.*)?")
            .blocklist_item(".*WHEA_ARM_PROCESSOR_ERROR")
            .blocklist_item(".*WHEA_ARM_CACHE_ERROR")
            // FIXME: arrays with more than 32 entries currently fail to generate a `Default`` impl: https://github.com/rust-lang/rust-bindgen/issues/2803
            .no_default(".*tagMONITORINFOEXA")
            .must_use_type("NTSTATUS")
            .must_use_type("HRESULT")
            // Defaults enums to generate as a set of constants contained in a module (default value
            // is EnumVariation::Consts which generates enums as global constants)
            .default_enum_style(bindgen::EnumVariation::ModuleConsts)
            .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
            .parse_callbacks(Box::new(WdkCallbacks::new(config)))
            .formatter(bindgen::Formatter::Prettyplease);
        Ok(builder)
    }

    /// Returns self (`Builder`) with opaque types for Windows handle types
    /// added to the bindgen configuration. This is necessary as the bindgen
    /// resolution for the `DECLARE_HANDLE` macro does not properly detect these
    /// generated handle types as opaque.
    ///
    /// # Errors
    ///
    /// Implementation may return a `wdk_build::ConfigError` if unable to
    /// resolve include paths
    fn opaque_windows_handle_types(
        mut self,
        config: impl Borrow<Config>,
    ) -> Result<Self, ConfigError> {
        let config: &Config = config.borrow();

        // We create a new matcher with the pattern as follows:
        // From the beginning of the string, we want any amount of whitespace followed
        // by the `DECLARE_HANDLE` string, which should be followed by any amount
        // of whitespace before an opening parentheses. The pattern is then
        // repeated to be inclusive of the string DECLARE_NDIS_HANDLE, which is a
        // function macro that expands to `DECLARE_HANDLE`.
        let matcher = RegexMatcher::new_line_matcher(
            "(^\\s*DECLARE_HANDLE\\s*\\()|(^\\s*DECLARE_NDIS_HANDLE\\s*\\()",
        )
        .expect("Failed to create matcher for grep.");

        // For each include path, we recursively step through the directory using
        // `WalkDir`, then search for matches to our `RegexMatcher` pattern. We push all
        // results to the vector `results`.
        let mut results = Vec::new();
        for include_path in config.include_paths()? {
            for entry in WalkDir::new(include_path)
                .into_iter()
                .filter_map(std::result::Result::ok)
            {
                if !entry.file_type().is_file() {
                    continue;
                }
                let _ = SearcherBuilder::new().build().search_path(
                    &matcher,
                    entry.path(),
                    UTF8(|_lnum, line| {
                        // We find the open and closed parentheses in each line, and push the
                        // trimmed substring between them to the results vector. The
                        // `DECLARE_HANDLE` macro adds two underscores to
                        // the end of the handle type, so we add them
                        // to match.
                        let open_paren_index = line.find('(');
                        let close_paren_index = line.find(')');
                        results.push(format!(
                            "{}__",
                            line[open_paren_index.unwrap() + 1..close_paren_index.unwrap()].trim()
                        ));
                        Ok(true)
                    }),
                );
            }
        }
        for result in results {
            self = self
                .clone()
                .opaque_type(result.as_str())
                .no_copy(result.as_str())
                .no_default(result);
        }
        Ok(self)
    }
}

impl ParseCallbacks for WdkCallbacks {
    fn generated_name_override(&self, item_info: ItemInfo) -> Option<String> {
        // Override the generated name for the WDF function table symbol, since bindgen is unable to currently translate the #define automatically: https://github.com/rust-lang/rust-bindgen/issues/2544
        if let Some(wdf_function_table_symbol_name) = &self.wdf_function_table_symbol_name {
            if let ItemInfo {
                name: item_name,
                kind: ItemKind::Var,
                ..
            } = item_info
            {
                if item_name == wdf_function_table_symbol_name {
                    return Some("WdfFunctions".to_string());
                }
            }
        }
        None
    }
}

impl WdkCallbacks {
    fn new(config: &Config) -> Self {
        Self {
            wdf_function_table_symbol_name: config.compute_wdffunctions_symbol_name(),
        }
    }
}
