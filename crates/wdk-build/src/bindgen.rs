// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

use std::{borrow::Borrow, fmt};

use bindgen::{
    Builder,
    callbacks::{ItemInfo, ItemKind, ParseCallbacks},
};
use cargo_metadata::MetadataCommand;
use tracing::debug;

use crate::{Config, ConfigError, DriverConfig, find_top_level_cargo_manifest};

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
    fn wdk_default(config: impl Borrow<Config> + fmt::Debug) -> Result<Builder, ConfigError>;
}

#[derive(Debug)]
struct WdkCallbacks {
    wdf_function_table_symbol_name: Option<String>,
}

struct BindgenRustEditionWrapper(bindgen::RustEdition);

impl TryFrom<cargo_metadata::Edition> for BindgenRustEditionWrapper {
    type Error = ConfigError;

    fn try_from(edition: cargo_metadata::Edition) -> Result<Self, Self::Error> {
        match edition {
            cargo_metadata::Edition::E2015 => Err(ConfigError::UnsupportedRustEdition {
                edition: "2015".to_string(),
            }),
            cargo_metadata::Edition::E2018 => Ok(Self(bindgen::RustEdition::Edition2018)),
            cargo_metadata::Edition::E2021 => Ok(Self(bindgen::RustEdition::Edition2021)),
            cargo_metadata::Edition::E2024 => Ok(Self(bindgen::RustEdition::Edition2024)),
            cargo_metadata::Edition::_E2027 => Err(ConfigError::UnsupportedRustEdition {
                edition: "2027".to_string(),
            }),
            cargo_metadata::Edition::_E2030 => Err(ConfigError::UnsupportedRustEdition {
                edition: "2030".to_string(),
            }),
            _ => Err(ConfigError::UnsupportedRustEdition {
                edition: "unknown".to_string(),
            }),
        }
    }
}

impl BuilderExt for Builder {
    /// Returns a `bindgen::Builder` with the default configuration for
    /// generation of bindings to the WDK
    ///
    /// # Errors
    ///
    /// Will return `wdk_build::ConfigError` if any of the resolved include or
    /// library paths do not exist
    #[tracing::instrument(level = "debug")]
    fn wdk_default(config: impl Borrow<Config> + fmt::Debug) -> Result<Self, ConfigError> {
        let config = config.borrow();

        let mut builder = Self::default()
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
            // FIXME: bindgen unable to generate for anonymous structs
            // https://github.com/rust-lang/rust-bindgen/issues/3177
            .blocklist_item(".*ADDRESS0_OWNERSHIP_ACQUIRE")
            .blocklist_item(".*USBDEVICE_ABORTIO")
            .blocklist_item(".*USBDEVICE_STARTIO")
            .blocklist_item(".*USBDEVICE_TREE_PURGEIO")
            // FIXME: arrays with more than 32 entries currently fail to generate a `Default`` impl: https://github.com/rust-lang/rust-bindgen/issues/2803
            .no_default(".*tagMONITORINFOEXA")
            .must_use_type("NTSTATUS")
            .must_use_type("HRESULT")
            // Defaults enums to generate as a set of constants contained in a module (default value
            // is EnumVariation::Consts which generates enums as global constants)
            .default_enum_style(bindgen::EnumVariation::ModuleConsts)
            .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
            .parse_callbacks(Box::new(WdkCallbacks::new(config)))
            .formatter(bindgen::Formatter::Prettyplease)
            .rust_target(get_rust_target()?)
            .rust_edition(get_rust_edition()?);

        // The `_USBPM_CLIENT_CONFIG_EXTRA_INFO` struct only has members when
        // _KERNEL_MODE flag is defined. We need to mark this type as opaque to avoid
        // generating an empty struct, since  they are not currently supported by
        // bindgen: https://github.com/rust-lang/rust-bindgen/issues/1683
        if let DriverConfig::Umdf(_) = config.driver_config {
            builder = builder.opaque_type("_USBPM_CLIENT_CONFIG_EXTRA_INFO");
        }

        Ok(builder)
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
    #[tracing::instrument(level = "trace")]
    fn new(config: &Config) -> Self {
        Self {
            wdf_function_table_symbol_name: config.compute_wdffunctions_symbol_name(),
        }
    }
}

// Retrieves the Rust version as a `bindgen::RustTarget` for the current build
// configuration.
//
// If the `nightly` feature is enabled and the current toolchain is `nightly`,
// returns a value allowing `bindgen` to generate code with supported `nightly`
// features. Otherwise, queries the MSRV from the `CARGO_PKG_RUST_VERSION`
// environment variable and uses it to create a `bindgen::RustTarget::stable`
// value.
//
// # Errors
//
// Returns `ConfigError::MsrvNotSupportedByBindgen` if the MSRV is not supported
// by bindgen, or `ConfigError::SemverError` if the MSRV cannot be parsed as a
// semver version.
#[tracing::instrument(level = "trace")]
fn get_rust_target() -> Result<bindgen::RustTarget, ConfigError> {
    let nightly_feature = cfg!(feature = "nightly");
    let nightly_toolchain = rustversion::cfg!(nightly);

    match (nightly_feature, nightly_toolchain) {
        (true, true) => Ok(bindgen::RustTarget::nightly()),
        (false, false) => get_stable_rust_target(),
        (true, false) => {
            tracing::warn!(
                "A non-nightly toolchain has been detected. Nightly bindgen features are only \
                 enabled with both nightly feature enablement and nightly toolchain use. "
            );
            get_stable_rust_target()
        }
        (false, true) => {
            tracing::warn!(
                "The nightly feature for wdk-build is disabled. Nightly bindgen features are only \
                 enabled with both nightly feature enablement and nightly toolchain use. "
            );
            get_stable_rust_target()
        }
    }
}

// Retrieves the stable Rust target for the current build configuration.
// Queries the MSRV from the `CARGO_PKG_RUST_VERSION` environment variable and
// uses it to create a `bindgen::RustTarget::stable` value.
#[tracing::instrument(level = "trace")]
fn get_stable_rust_target() -> Result<bindgen::RustTarget, ConfigError> {
    let package_msrv = semver::Version::parse(env!("CARGO_PKG_RUST_VERSION"))
        .map_err(|e| ConfigError::RustVersionParseError { error_source: e })?;

    let bindgen_msrv = bindgen::RustTarget::stable(package_msrv.minor, package_msrv.patch)
        .map_err(|e| ConfigError::MsrvNotSupportedByBindgen {
            msrv: package_msrv.to_string(),
            reason: e.to_string(),
        })?;
    Ok(bindgen_msrv)
}

// Retrieves the Rust edition from `cargo metadata` and returns the appropriate
// `bindgen::RustEdition` value.
//
// # Errors
//
// Returns `ConfigError::CargoMetadataPackageNotFound` if the `wdk-build`
// package is not found, or `ConfigError::UnsupportedRustEdition` if the edition
// is not supported.
#[tracing::instrument(level = "trace")]
fn get_rust_edition() -> Result<bindgen::RustEdition, ConfigError> {
    const WDK_BUILD_PACKAGE_NAME: &str = "wdk-build";
    // Run `cargo_metadata` in the same working directory as the top level manifest
    // in order to respect `config.toml` overrides
    let top_level_cargo_manifest_path = find_top_level_cargo_manifest();
    debug!(
        "Top level Cargo manifest path: {:?}",
        top_level_cargo_manifest_path
    );
    let cwd = top_level_cargo_manifest_path
        .parent()
        .expect("Cargo manifest should have a valid parent directory");
    let wdk_sys_cargo_metadata = MetadataCommand::new().current_dir(cwd).exec()?;

    let wdk_sys_package_metadata = wdk_sys_cargo_metadata
        .packages
        .iter()
        .find(|package| package.name == WDK_BUILD_PACKAGE_NAME)
        .ok_or_else(|| ConfigError::WdkBuildPackageNotFoundInCargoMetadata)?;

    let rust_edition: BindgenRustEditionWrapper = wdk_sys_package_metadata.edition.try_into()?;
    Ok(rust_edition.0)
}
