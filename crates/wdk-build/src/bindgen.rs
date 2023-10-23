// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

use bindgen::Builder;

use crate::{CPUArchitecture, Config, ConfigError, DriverConfig};

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
            println!("cargo:rerun-if-changed={c_header}");
            builder = builder.header(c_header);
        }

        builder = builder
            .use_core() // Can't use std for kernel code
            .derive_default(true) // allows for default initializing structs
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
                match config.cpu_architecture {
                    // Definitions sourced from `Program Files\Windows
                    // Kits\10\build\10.0.22621.0\WindowsDriver.x64.props`
                    CPUArchitecture::AMD64 => {
                        vec!["_WIN64", "_AMD64_", "AMD64"]
                    }
                    // Definitions sourced from `Program Files\Windows
                    // Kits\10\build\10.0.22621.0\WindowsDriver.arm64.props`
                    CPUArchitecture::ARM64 => {
                        vec!["_ARM64_", "ARM64", "_USE_DECLSPECS_FOR_SAL=1", "STD_CALL"]
                    }
                }
                .iter()
                .map(|preprocessor_definition| format!("--define-macro={preprocessor_definition}")),
            )
            .clang_args(
                match config.driver_config {
                    // FIXME: Add support for KMDF_MINIMUM_VERSION_REQUIRED and
                    // UMDF_MINIMUM_VERSION_REQUIRED
                    DriverConfig::WDM() => {
                        vec![]
                    }
                    DriverConfig::KMDF(kmdf_config) => {
                        vec![
                            format!("KMDF_VERSION_MAJOR={}", kmdf_config.kmdf_version_major),
                            format!("KMDF_VERSION_MINOR={}", kmdf_config.kmdf_version_minor),
                        ]
                    }
                    DriverConfig::UMDF(umdf_config) => {
                        let mut umdf_definitions = vec![
                            format!("UMDF_VERSION_MAJOR={}", umdf_config.umdf_version_major),
                            format!("UMDF_VERSION_MINOR={}", umdf_config.umdf_version_minor),
                        ];

                        if umdf_config.umdf_version_major >= 2 {
                            umdf_definitions.push("UMDF_USING_NTSTATUS".to_string());
                            umdf_definitions.push("_UNICODE".to_string());
                            umdf_definitions.push("UNICODE".to_string());
                        }

                        umdf_definitions
                    }
                }
                .iter()
                .map(|preprocessor_definition| format!("--define-macro={preprocessor_definition}")),
            )
            // Windows SDK & DDK have non-portable paths (ex. #include "DriverSpecs.h" but the file
            // is actually driverspecs.h)
            .clang_arg("--warn-=no-nonportable-include-path")
            // Windows SDK & DDK use pshpack and poppack headers to change packing
            .clang_arg("--warn-=no-pragma-pack")
            .clang_arg("--warn-=no-ignored-attributes")
            .clang_arg("--warn-=no-ignored-pragma-intrinsic")
            .clang_arg("--warn-=no-visibility")
            .clang_arg("--warn-=no-microsoft-anon-tag")
            .clang_arg("--warn-=no-microsoft-enum-forward-reference")
            // Don't warn for deprecated declarations. deprecated items are already blocklisted
            // below and if there are any non-blocklisted function definitions, it will throw a
            // -WDeprecated warning
            .clang_arg("--warn-=no-deprecated-declarations")
            // Windows SDK & DDK contain unnecessary token pasting (ex. &##_variable: `&` and
            // `_variable` are seperate tokens already, and don't need `##` to concatenate them)
            .clang_arg("--warn-=no-invalid-token-paste")
            .clang_arg("-fms-extensions")
            .blocklist_item("ExAllocatePoolWithTag") // Deprecated
            .blocklist_item("ExAllocatePoolWithQuotaTag") // Deprecated
            .blocklist_item("ExAllocatePoolWithTagPriority") // Deprecated
            // FIXME: Types containing 32-bit pointers (via __ptr32) are not generated properly and cause bindgen layout tests to fail: https://github.com/rust-lang/rust-bindgen/issues/2636
            .blocklist_item(".*EXTENDED_CREATE_INFORMATION_32")
            // FIXME: bitfield generated with non-1byte alignment in _MCG_CAP
            .blocklist_item(".*MCG_CAP(?:__bindgen.*)?")
            .blocklist_item(".*WHEA_XPF_MCA_SECTION")
            .blocklist_item(".*WHEA_ARM_BUS_ERROR(?:__bindgen.*)?")
            .blocklist_item(".*WHEA_ARM_PROCESSOR_ERROR")
            .blocklist_item(".*WHEA_ARM_CACHE_ERROR")
            .must_use_type("NTSTATUS")
            .must_use_type("HRESULT")
            // Defaults enums to generate as a set of constants contained in a module (default value
            // is EnumVariation::Consts which generates enums as global constants)
            .default_enum_style(bindgen::EnumVariation::ModuleConsts)
            .parse_callbacks(Box::new(bindgen::CargoCallbacks))
            .formatter(bindgen::Formatter::Prettyplease);

        Ok(builder)
    }
}
