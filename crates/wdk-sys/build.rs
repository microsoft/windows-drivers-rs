// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Build script for the `wdk-sys` crate.
//!
//! This parses the WDK configuration from metadata provided in the build tree,
//! and generates the relevant bindings to WDK APIs.

use std::{
    env,
    io::Write,
    path::{Path, PathBuf},
};

use bindgen::CodegenConfig;
use lazy_static::lazy_static;
use tracing::{info, info_span};
use tracing_subscriber::{
    filter::{LevelFilter, ParseError},
    EnvFilter,
};
use wdk_build::{
    detect_driver_config,
    find_top_level_cargo_manifest,
    BuilderExt,
    Config,
    ConfigError,
    DriverConfig,
};

const OUT_DIR_PLACEHOLDER: &str =
    "<PLACEHOLDER FOR LITERAL VALUE CONTAINING OUT_DIR OF wdk-sys CRATE>";
const WDFFUNCTIONS_SYMBOL_NAME_PLACEHOLDER: &str =
    "<PLACEHOLDER FOR LITERAL VALUE CONTAINING WDFFUNCTIONS SYMBOL NAME>";

// FIXME: replace lazy_static with std::Lazy once available: https://github.com/rust-lang/rust/issues/109736
lazy_static! {
    static ref CALL_UNSAFE_WDF_BINDING_TEMPLATE: String = format!(
        r#"
/// A procedural macro that allows WDF functions to be called by name.
///
/// This function parses the name of the WDF function, finds it function
/// pointer from the WDF function table, and then calls it with the
/// arguments passed to it
///
/// # Safety
/// Function arguments must abide by any rules outlined in the WDF
/// documentation. This macro does not perform any validation of the
/// arguments passed to it., beyond type validation.
///
/// # Examples
///
/// ```rust, no_run
/// use wdk_sys::*;
/// 
/// pub unsafe extern "system" fn driver_entry(
///     driver: &mut DRIVER_OBJECT,
///     registry_path: PCUNICODE_STRING,
/// ) -> NTSTATUS {{
/// 
///     let mut driver_config = WDF_DRIVER_CONFIG {{
///         Size: core::mem::size_of::<WDF_DRIVER_CONFIG>() as ULONG,
///         ..WDF_DRIVER_CONFIG::default()
///     }};
///     let driver_handle_output = WDF_NO_HANDLE as *mut WDFDRIVER;
///
///     unsafe {{
///         call_unsafe_wdf_function_binding!(
///             WdfDriverCreate,
///             driver as PDRIVER_OBJECT,
///             registry_path,
///             WDF_NO_OBJECT_ATTRIBUTES,
///             &mut driver_config,
///             driver_handle_output,
///         )
///     }}
/// }}
/// ```
#[macro_export]
macro_rules! call_unsafe_wdf_function_binding {{
    ( $($tt:tt)* ) => {{
        $crate::__proc_macros::call_unsafe_wdf_function_binding! (
            r"{OUT_DIR_PLACEHOLDER}",
            $($tt)*
        )
    }}
}}"#
    );
    static ref TEST_STUBS_TEMPLATE: String = format!(
        r#"
use crate::WDFFUNC;

/// Stubbed version of the symbol that [`WdfFunctions`] links to so that test targets will compile
#[no_mangle]
pub static mut {WDFFUNCTIONS_SYMBOL_NAME_PLACEHOLDER}: *const WDFFUNC = core::ptr::null();
"#,
    );
}

fn initialize_tracing() -> Result<(), ParseError> {
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

    Ok(())
}

fn generate_constants(out_path: &Path, config: &Config) -> Result<(), ConfigError> {
    Ok(bindgen::Builder::wdk_default(vec!["src/input.h"], config)?
        .with_codegen_config(CodegenConfig::VARS)
        .generate()
        .expect("Bindings should succeed to generate")
        .write_to_file(out_path.join("constants.rs"))?)
}

fn generate_types(out_path: &Path, config: &Config) -> Result<(), ConfigError> {
    Ok(bindgen::Builder::wdk_default(vec!["src/input.h"], config)?
        .with_codegen_config(CodegenConfig::TYPES)
        .generate()
        .expect("Bindings should succeed to generate")
        .write_to_file(out_path.join("types.rs"))?)
}

fn generate_base(out_path: &Path, config: &Config) -> Result<(), ConfigError> {
    let outfile_name = match &config.driver_config {
        DriverConfig::WDM() | DriverConfig::KMDF(_) => "ntddk.rs",
        DriverConfig::UMDF(_) => "windows.rs",
    };

    Ok(bindgen::Builder::wdk_default(vec!["src/input.h"], config)?
        .with_codegen_config((CodegenConfig::TYPES | CodegenConfig::VARS).complement())
        .generate()
        .expect("Bindings should succeed to generate")
        .write_to_file(out_path.join(outfile_name))?)
}

fn generate_wdf(out_path: &Path, config: &Config) -> Result<(), ConfigError> {
    // As of NI WDK, this may generate an empty file due to no non-type and non-var
    // items in the wdf headers(i.e. functions are all inlined). This step is
    // intentionally left here in case older/newer WDKs have non-inlined functions
    // or new WDKs may introduce non-inlined functions.
    Ok(bindgen::Builder::wdk_default(vec!["src/input.h"], config)?
        .with_codegen_config((CodegenConfig::TYPES | CodegenConfig::VARS).complement())
        // Only generate for files that are prefixed with (case-insensitive) wdf (ie.
        // /some/path/WdfSomeHeader.h), to prevent duplication of code in ntddk.rs
        .allowlist_file("(?i).*wdf.*")
        .generate()
        .expect("Bindings should succeed to generate")
        .write_to_file(out_path.join("wdf.rs"))?)
}

/// Generates a `macros.rs` file in `OUT_DIR` which contains a
/// `call_unsafe_wdf_function_binding!` macro redirects to the
/// `wdk_macros::call_unsafe_wdf_function_binding` macro . This is required
/// in order to add an additional argument with the path to the file containing
/// generated types
fn generate_call_unsafe_wdf_function_binding_macro(out_path: &Path) -> std::io::Result<()> {
    let generated_file_path = out_path.join("call_unsafe_wdf_function_binding.rs");
    let mut generated_file = std::fs::File::create(&generated_file_path)?;
    generated_file.write_all(
        CALL_UNSAFE_WDF_BINDING_TEMPLATE
            .replace(
                OUT_DIR_PLACEHOLDER,
                out_path.join("types.rs").to_str().expect(
                    "path to file with generated type information should succesfully convert to a \
                     str",
                ),
            )
            .as_bytes(),
    )?;
    Ok(())
}

fn generate_test_stubs(out_path: &Path, config: &Config) -> std::io::Result<()> {
    let stubs_file_path = out_path.join("test_stubs.rs");
    let mut stubs_file = std::fs::File::create(&stubs_file_path)?;
    stubs_file.write_all(
        TEST_STUBS_TEMPLATE
            .replace(
                WDFFUNCTIONS_SYMBOL_NAME_PLACEHOLDER,
                &config.compute_wdffunctions_symbol_name().expect(
                    "KMDF and UMDF configs should always have a computable WdfFunctions symbol \
                     name",
                ),
            )
            .as_bytes(),
    )?;
    Ok(())
}

// TODO: most wdk-sys specific code should move to wdk-build in a wdk-sys
// specific module so it can be unit tested.

fn main() -> anyhow::Result<()> {
    initialize_tracing()?;

    let config = Config {
        driver_config: match detect_driver_config(find_top_level_cargo_manifest()) {
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

    info_span!("bindgen").in_scope(|| {
        info!("Generating bindings to WDK");
        generate_constants(&out_path, &config)?;
        generate_types(&out_path, &config)?;
        generate_base(&out_path, &config)?;

        if let DriverConfig::KMDF(_) | DriverConfig::UMDF(_) = &config.driver_config {
            generate_wdf(&out_path, &config)?;
        }
        Ok::<(), ConfigError>(())
    })?;

    if let DriverConfig::KMDF(_) | DriverConfig::UMDF(_) = config.driver_config {
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

        info_span!("call_unsafe_wdf_function_binding.rs generation").in_scope(|| {
            generate_call_unsafe_wdf_function_binding_macro(&out_path)?;
            Ok::<(), std::io::Error>(())
        })?;

        info_span!("test_stubs.rs generation").in_scope(|| {
            generate_test_stubs(&out_path, &config)?;
            Ok::<(), std::io::Error>(())
        })?;
    }

    config.configure_library_build()?;
    Ok(config.export_config()?)
}
