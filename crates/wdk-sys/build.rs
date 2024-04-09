// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Build script for the `wdk-sys` crate.

use std::{
    env,
    path::{Path, PathBuf},
};

use bindgen::CodegenConfig;
use tracing::{info, info_span};
use tracing_subscriber::{filter::LevelFilter, EnvFilter};
use wdk_build::{detect_driver_config, BuilderExt, Config, ConfigError};

fn generate_constants(out_path: &Path, config: Config) -> Result<(), ConfigError> {
    Ok(bindgen::Builder::wdk_default(vec!["src/input.h"], config)?
        .with_codegen_config(CodegenConfig::VARS)
        .generate()
        .expect("Bindings should succeed to generate")
        .write_to_file(out_path.join("constants.rs"))?)
}

fn generate_types(out_path: &Path, config: Config) -> Result<(), ConfigError> {
    Ok(bindgen::Builder::wdk_default(vec!["src/input.h"], config)?
        .with_codegen_config(CodegenConfig::TYPES)
        .generate()
        .expect("Bindings should succeed to generate")
        .write_to_file(out_path.join("types.rs"))?)
}

fn generate_ntddk(out_path: &Path, config: Config) -> Result<(), ConfigError> {
    Ok(bindgen::Builder::wdk_default(vec!["src/input.h"], config)?
        .with_codegen_config((CodegenConfig::TYPES | CodegenConfig::VARS).complement())
        .generate()
        .expect("Bindings should succeed to generate")
        .write_to_file(out_path.join("ntddk.rs"))?)
}

fn generate_wdf(out_path: &Path, config: Config) -> Result<(), ConfigError> {
    // As of NI WDK, this may generate an empty file due to no non-type and non-var
    // items in the wdf headers(i.e. functions are all inlined). This step is
    // intentionally left here in case older WDKs have non-inlined functions or new
    // WDKs may introduce non-inlined functions.
    Ok(bindgen::Builder::wdk_default(vec!["src/input.h"], config)?
        .with_codegen_config((CodegenConfig::TYPES | CodegenConfig::VARS).complement())
        .allowlist_file("(?i).*wdf.*") // Only generate for files that are prefixed with (case-insensitive) wdf (ie.
        // /some/path/WdfSomeHeader.h), to prevent duplication of code in ntddk.rs
        .generate()
        .expect("Bindings should succeed to generate")
        .write_to_file(out_path.join("wdf.rs"))?)
}

fn main() -> anyhow::Result<()> {
    let tracing_filter = EnvFilter::default()
        // Show up to INFO level by default
        .add_directive(LevelFilter::INFO.into())
        // Silence various warnings originating from bindgen that are not currently actionable
        // FIXME: this currently sets the minimum log level to error for the listed modules. It
        // should actually be turning off logging (level=off) for specific warnings in these
        // modules, but a bug in the tracing crate's filtering is preventing this from working as expected. See https://github.com/tokio-rs/tracing/issues/2843.
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
        driver_config: match detect_driver_config() {
            Ok(driver_config) => driver_config,
            Err(ConfigError::NoWDKConfigurationsDetected) => {
                // When building wdk-sys standalone, skip binding generation
                tracing::warn!("No WDK configurations detected. Skipping WDK binding generation.");
                return Ok(());
            }
            Err(error) => {
                return Err(error.into());
            }
        },
        ..Config::default()
    };

    let out_path = PathBuf::from(
        env::var("OUT_DIR").expect("OUT_DIR should be exist in Cargo build environment"),
    );

    // TODO: consider using references here to avoid cloning
    info_span!("bindgen").in_scope(|| {
        info!("Generating bindings to WDK");
        generate_constants(&out_path, config.clone())?;
        generate_types(&out_path, config.clone())?;
        generate_ntddk(&out_path, config.clone())?;

        if let wdk_build::DriverConfig::KMDF(_) | wdk_build::DriverConfig::UMDF(_) =
            config.driver_config
        {
            generate_wdf(&out_path, config.clone())?;
        }
        Ok::<(), ConfigError>(())
    })?;

    if let wdk_build::DriverConfig::KMDF(_) | wdk_build::DriverConfig::UMDF(_) =
        config.driver_config
    {
        // Compile a c library to expose symbols that are not exposed because of
        // __declspec(selectany)
        info_span!("cc").in_scope(|| {
            info!("Compiling wdf.c");
            let mut cc_builder = cc::Build::new();
            for (key, value) in config.get_preprocessor_definitions_iter() {
                cc_builder.define(&key, value.as_deref());
            }

            cc_builder
                .includes(config.get_include_paths()?)
                .file("src/wdf.c")
                .compile("wdf");
            Ok::<(), ConfigError>(())
        })?;
    }

    config.configure_library_build()?;
    Ok(config.export_config()?)
}
