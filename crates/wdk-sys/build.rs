// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

use std::{
    env,
    path::{Path, PathBuf},
};

use bindgen::CodegenConfig;
use wdk_build::{BuilderExt, CPUArchitecture, Config, ConfigError, DriverConfig, KMDFConfig};

// FIXME: feature gate the WDF version
// FIXME: check that the features are exclusive
// const KMDF_VERSIONS: &'static [&'static str] = &[
//     "1.9", "1.11", "1.13", "1.15", "1.17", "1.19", "1.21", "1.23", "1.25",
// "1.27", "1.31", "1.33", ];
// const UMDF_VERSIONS: &'static [&'static str] = &[
//     "2.0", "2.15", "2.17", "2.19", "2.21", "2.23", "2.25", "2.27", "2.31",
// "2.33", ];

fn generate_constants(out_path: &Path, config: Config) -> Result<(), ConfigError> {
    Ok(
        bindgen::Builder::wdk_default(vec!["src/ntddk-input.h", "src/wdf-input.h"], config)?
            .with_codegen_config(CodegenConfig::VARS)
            .generate()
            .expect("Bindings should succeed to generate")
            .write_to_file(out_path.join("constants.rs"))?,
    )
}

fn generate_types(out_path: &Path, config: Config) -> Result<(), ConfigError> {
    Ok(
        bindgen::Builder::wdk_default(vec!["src/ntddk-input.h", "src/wdf-input.h"], config)?
            .with_codegen_config(CodegenConfig::TYPES)
            .generate()
            .expect("Bindings should succeed to generate")
            .write_to_file(out_path.join("types.rs"))?,
    )
}

fn generate_ntddk(out_path: &Path, config: Config) -> Result<(), ConfigError> {
    Ok(
        bindgen::Builder::wdk_default(vec!["src/ntddk-input.h"], config)?
            .with_codegen_config((CodegenConfig::TYPES | CodegenConfig::VARS).complement())
            .generate()
            .expect("Bindings should succeed to generate")
            .write_to_file(out_path.join("ntddk.rs"))?,
    )
}

fn generate_wdf(out_path: &Path, config: Config) -> Result<(), ConfigError> {
    // As of NI WDK, this may generate an empty file due to no non-type and non-var
    // items in the wdf headers(i.e. functions are all inlined). This step is
    // intentionally left here in case older WDKs have non-inlined functions or new
    // WDKs may introduce non-inlined functions.
    Ok(
        bindgen::Builder::wdk_default(vec!["src/wdf-input.h"], config)?
            .clang_arg("-fkeep-inline-functions")
            .generate_inline_functions(true)
            .with_codegen_config((CodegenConfig::TYPES | CodegenConfig::VARS).complement())
            .allowlist_file("(?i).*wdf.*") // Only generate for files that are prefixed with (case-insensitive) wdf (ie.
            // /some/path/WdfSomeHeader.h), to prevent duplication of code in ntddk.rs
            .generate()
            .expect("Bindings should succeed to generate")
            .write_to_file(out_path.join("wdf.rs"))?,
    )
}

fn main() -> Result<(), ConfigError> {
    tracing_subscriber::fmt::init();

    let config = Config {
        // FIXME: this should be based off of Cargo feature version
        driver_config: DriverConfig::KMDF(KMDFConfig::new()),
        ..Config::default()
    };

    let out_paths = vec![
        // FIXME: gate the generations of the generated_bindings folder behind a feature flag that
        // is disabled in crates.io builds (modifying source is illegal when distributing
        // crates)

        // Generate a copy of the bindings to the generated_bindings so that its easier to see
        // diffs in the output due to bindgen settings changes
        PathBuf::from("./generated_bindings/"),
        // This is the actual bindings that get consumed via !include in this library's modules
        PathBuf::from(
            env::var("OUT_DIR").expect("OUT_DIR should be exist in Cargo build environment"),
        ),
    ];

    for out_path in out_paths {
        generate_constants(&out_path, config.clone())?;
        generate_types(&out_path, config.clone())?;
        generate_ntddk(&out_path, config.clone())?;
        generate_wdf(&out_path, config.clone())?;
    }

    // FIXME: This is mostly duplicated from wdk-build/src/bindgen.rs.
    let args = config
        .get_include_paths()?
        .iter()
        .map(|include_path| {
            format!(
                "--include-directory={}",
                include_path
                    .to_str()
                    .expect("Non Unicode paths are not supported")
            )
        })
        .chain([format!(
            "--define-macro={}",
            match config.cpu_architecture {
                CPUArchitecture::AMD64 => "_AMD64_",
                CPUArchitecture::ARM64 => "_ARM64EC_",
            }
        )])
        .chain(
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
        .chain(["--warn-=no-nonportable-include-path".to_string()])
        // Windows SDK & DDK use pshpack and poppack headers to change packing
        .chain(["--warn-=no-pragma-pack".to_string()])
        .chain(["--warn-=no-ignored-attributes".to_string()])
        .chain(["--warn-=no-ignored-pragma-intrinsic".to_string()])
        .chain(["--warn-=no-visibility".to_string()])
        .chain(["--warn-=no-microsoft-anon-tag".to_string()])
        .chain(["--warn-=no-microsoft-enum-forward-reference".to_string()])
        // Don't warn for deprecated declarations. deprecated items are already blocklisted
        // below and if there are any non-blocklisted function definitions, it will throw a
        // -WDeprecated warning
        .chain(["--warn-=no-deprecated-declarations".to_string()])
        .chain(["-fms-extensions".to_string()])
        .collect::<Vec<_>>();

    // Generate the inline `Wdf*` functions for linking.
    let mut cc = cc::Build::new();
    for flag in args {
        cc.flag(&flag);
    }

    cc.compiler("clang")
        .warnings(false)
        .file("src/wdf.c")
        .compile("wdf");

    config.configure_library_build()?;
    Ok(config.export_config()?)
}
