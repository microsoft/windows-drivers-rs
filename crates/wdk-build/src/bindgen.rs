// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

use bindgen::{
    callbacks::{ItemInfo, ItemKind, ParseCallbacks},
    Builder,
};

use crate::{Config, ConfigError, DriverConfig};

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
    fn wdk_default(c_header_files: Vec<&str>, config: Config) -> Result<Builder, ConfigError>;
}

#[derive(Debug)]
struct WDKCallbacks {
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
    fn wdk_default(c_header_files: Vec<&str>, config: Config) -> Result<Self, ConfigError> {
        let mut builder = Self::default();

        for c_header in c_header_files {
            builder = builder.header(c_header);
        }

        builder = builder
            .use_core() // Can't use std for kernel code
            .derive_default(true) // allows for default initializing structs
            // CStr types are safer and easier to work with when interacting with string constants
            // from C
            .generate_cstr(true)
            // Building in eWDK can pollute system search path when clang-sys tries to detect
            // c_search_paths
            .detect_include_paths(false)
            .clang_args(config.get_include_paths()?.iter().map(|include_path| {
                format!(
                    "--include-directory={}",
                    include_path
                        .to_str()
                        .expect("Non Unicode paths are not supported")
                )
            }))
            .clang_args(
                config
                    .get_preprocessor_definitions_iter()
                    .map(|(key, value)| {
                        format!(
                            "--define-macro={key}{}",
                            value.map(|v| format!("={v}")).unwrap_or_default()
                        )
                    })
                    .chain(config.get_compiler_flags_iter()),
            )
            .blocklist_item("ExAllocatePoolWithTag") // Deprecated
            .blocklist_item("ExAllocatePoolWithQuotaTag") // Deprecated
            .blocklist_item("ExAllocatePoolWithTagPriority") // Deprecated
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
            .parse_callbacks(Box::new(WDKCallbacks::new(&config)))
            .formatter(bindgen::Formatter::Prettyplease);

        Ok(builder)
    }
}

impl WDKCallbacks {
    fn new(config: &Config) -> Self {
        Self {
            wdf_function_table_symbol_name: Self::compute_wdf_function_table_symbol_name(config),
        }
    }

    fn compute_wdf_function_table_symbol_name(config: &Config) -> Option<String> {
        let (wdf_major_version, wdf_minor_version) = match config.driver_config {
            DriverConfig::KMDF(config) => (config.kmdf_version_major, config.kmdf_version_minor),
            DriverConfig::UMDF(config) => (config.umdf_version_major, config.umdf_version_minor),
            DriverConfig::WDM() => return None,
        };

        Some(format!(
            "WdfFunctions_{:02}0{:02}",
            wdf_major_version, wdf_minor_version
        ))
    }
}

impl ParseCallbacks for WDKCallbacks {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::with_env;

    mod compute_wdf_function_table_symbol_name {
        use super::*;
        use crate::{KMDFConfig, UMDFConfig};

        #[test]
        fn kmdf() {
            let config = with_env(&[("CARGO_CFG_TARGET_ARCH", "x86_64")], || Config {
                driver_config: DriverConfig::KMDF(KMDFConfig {
                    kmdf_version_major: 1,
                    kmdf_version_minor: 15,
                }),
                ..Default::default()
            });

            let result = WDKCallbacks::compute_wdf_function_table_symbol_name(&config);

            assert_eq!(result, Some("WdfFunctions_01015".to_string()));
        }

        #[test]
        fn umdf() {
            let config = with_env(&[("CARGO_CFG_TARGET_ARCH", "aarch64")], || Config {
                driver_config: DriverConfig::UMDF(UMDFConfig {
                    umdf_version_major: 2,
                    umdf_version_minor: 33,
                }),
                ..Default::default()
            });

            let result = WDKCallbacks::compute_wdf_function_table_symbol_name(&config);

            assert_eq!(result, Some("WdfFunctions_02033".to_string()));
        }

        #[test]
        fn wdm() {
            let config = with_env(&[("CARGO_CFG_TARGET_ARCH", "x86_64")], || Config {
                driver_config: DriverConfig::WDM(),
                ..Default::default()
            });

            let result = WDKCallbacks::compute_wdf_function_table_symbol_name(&config);

            assert_eq!(result, None);
        }
    }
}
