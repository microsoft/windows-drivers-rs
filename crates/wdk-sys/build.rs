// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

use std::{
    env,
    path::{Path, PathBuf},
};

use bindgen::CodegenConfig;
use tracing_subscriber::{filter::LevelFilter, EnvFilter};
use wdk_build::{BuilderExt, Config, ConfigError, DriverConfig, KMDFConfig};

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
            .with_codegen_config((CodegenConfig::TYPES | CodegenConfig::VARS).complement())
            .allowlist_file("(?i).*wdf.*") // Only generate for files that are prefixed with (case-insensitive) wdf (ie.
            // /some/path/WdfSomeHeader.h), to prevent duplication of code in ntddk.rs
            .generate()
            .expect("Bindings should succeed to generate")
            .write_to_file(out_path.join("wdf.rs"))?,
    )
}

fn main() -> anyhow::Result<()> {
    let tracing_filter = EnvFilter::default()
        // Show errors and warnings by default
        .add_directive(LevelFilter::WARN.into())
        // Silence various warnings originating from bindgen that are not currently actionable
        // FIXME: this currently sets the minimum log level to error for the listed modules. It should actually be turning off logging (level=off) for specific warnings in these modules, but a bug in the tracing crate's filtering is preventing this from working as expected. See https://github.com/tokio-rs/tracing/issues/2843.
        .add_directive("bindgen::codegen::helpers[{message}]=error".parse()?)
        .add_directive("bindgen::codegen::struct_layout[{message}]=error".parse()?)
        .add_directive("bindgen::ir::comp[{message}]=error".parse()?)
        .add_directive("bindgen::ir::context[{message}]=error".parse()?)
        .add_directive("bindgen::ir::ty[{message}]=error".parse()?)
        .add_directive("bindgen::ir::var[{message}]=error".parse()?);

    // Allow overriding tracing behaviour via `EnvFilter::DEFAULT_ENV` env var
    let tracing_filter =
        if let Ok(filter_directives_from_env_var) = env::var(EnvFilter::DEFAULT_ENV) {
            // Append each directive from the env var to the filter
            filter_directives_from_env_var.split(',').fold(
                tracing_filter,
                |tracing_filter, filter_directive| {
                    match filter_directive.parse() {
                        Ok(parsed_filter_directive) => {
                            tracing_filter.add_directive(parsed_filter_directive)
                        }
                        Err(parsing_error) => {
                            // Must use eprintln!() here as tracing is not yet initialized
                            eprintln!(
                                "Skipping filter directive, {}, which failed to be parsed from {} \
                                 obtained from {} with the following error: {}",
                                filter_directive,
                                filter_directives_from_env_var,
                                EnvFilter::DEFAULT_ENV,
                                parsing_error
                            );
                            tracing_filter
                        }
                    }
                },
            )
        } else {
            tracing_filter
        };

    tracing_subscriber::fmt()
        .pretty()
        .with_env_filter(tracing_filter)
        .init();

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

    Ok(config.export_config()?)
}
