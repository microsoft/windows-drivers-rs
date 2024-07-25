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
    thread,
};

use anyhow::Context;
use bindgen::CodegenConfig;
use lazy_static::lazy_static;
use tracing::{info, info_span, Span};
use tracing_subscriber::{
    filter::{LevelFilter, ParseError},
    EnvFilter,
};
use wdk_build::{
    configure_wdk_library_build_and_then,
    BuilderExt,
    Config,
    ConfigError,
    DriverConfig,
    KmdfConfig,
    UmdfConfig,
};

const NUM_WDF_FUNCTIONS_PLACEHOLDER: &str =
    "<PLACEHOLDER FOR IDENTIFIER FOR VARIABLE CORRESPONDING TO NUMBER OF WDF FUNCTIONS>";
const WDF_FUNCTION_COUNT_DECLARATION_PLACEHOLDER: &str =
    "<PLACEHOLDER FOR DECLARATION OF wdf_function_count VARIABLE>";
const OUT_DIR_PLACEHOLDER: &str =
    "<PLACEHOLDER FOR LITERAL VALUE CONTAINING OUT_DIR OF wdk-sys CRATE>";
const WDFFUNCTIONS_SYMBOL_NAME_PLACEHOLDER: &str =
    "<PLACEHOLDER FOR LITERAL VALUE CONTAINING WDFFUNCTIONS SYMBOL NAME>";

const WDF_FUNCTION_COUNT_DECLARATION_EXTERNAL_SYMBOL: &str = "
        // SAFETY: `crate::WdfFunctionCount` is generated as a mutable static, but is not supposed \
                                                              to be ever mutated by WDF.
        let wdf_function_count = unsafe { crate::WdfFunctionCount } as usize;";
const WDF_FUNCTION_COUNT_DECLARATION_TABLE_INDEX: &str = "
        let wdf_function_count = crate::_WDFFUNCENUM::WdfFunctionTableNumEntries as usize;";

// FIXME: replace lazy_static with std::Lazy once available: https://github.com/rust-lang/rust/issues/109736
lazy_static! {
    static ref WDF_FUNCTION_TABLE_TEMPLATE: String = format!(
        r#"
// FIXME: replace lazy_static with std::Lazy once available: https://github.com/rust-lang/rust/issues/109736
#[cfg(any(driver_model__driver_type = "KMDF", driver_model__driver_type = "UMDF"))]
lazy_static::lazy_static! {{
    #[allow(missing_docs)]
    pub static ref WDF_FUNCTION_TABLE: &'static [crate::WDFFUNC] = {{
        // SAFETY: `WdfFunctions` is generated as a mutable static, but is not supposed to be ever mutated by WDF.
        let wdf_function_table = unsafe {{ crate::WdfFunctions }};
{WDF_FUNCTION_COUNT_DECLARATION_PLACEHOLDER}

        // SAFETY: This is safe because:
        //         1. `WdfFunctions` is valid for reads for `{NUM_WDF_FUNCTIONS_PLACEHOLDER}` * `core::mem::size_of::<WDFFUNC>()`
        //            bytes, and is guaranteed to be aligned and it must be properly aligned.
        //         2. `WdfFunctions` points to `{NUM_WDF_FUNCTIONS_PLACEHOLDER}` consecutive properly initialized values of
        //            type `WDFFUNC`.
        //         3. WDF does not mutate the memory referenced by the returned slice for for its entire `'static' lifetime.
        //         4. The total size, `{NUM_WDF_FUNCTIONS_PLACEHOLDER}` * `core::mem::size_of::<WDFFUNC>()`, of the slice must be no
        //            larger than `isize::MAX`. This is proven by the below `debug_assert!`.
        unsafe {{
            debug_assert!(isize::try_from(wdf_function_count * core::mem::size_of::<crate::WDFFUNC>()).is_ok());
            core::slice::from_raw_parts(wdf_function_table, wdf_function_count)
        }}
    }};
}}"#
    );
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

type GenerateFn = fn(&Path, &Config) -> Result<(), ConfigError>;

const BINDGEN_FILE_GENERATORS_TUPLES: &[(&str, GenerateFn)] = &[
    ("constants.rs", generate_constants),
    ("types.rs", generate_types),
    ("base.rs", generate_base),
    ("wdf.rs", generate_wdf),
];

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
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .init();

    Ok(())
}

fn generate_constants(out_path: &Path, config: &Config) -> Result<(), ConfigError> {
    info!("Generating bindings to WDK: constants.rs");

    Ok(bindgen::Builder::wdk_default(vec!["src/input.h"], config)?
        .with_codegen_config(CodegenConfig::VARS)
        .generate()
        .expect("Bindings should succeed to generate")
        .write_to_file(out_path.join("constants.rs"))?)
}

fn generate_types(out_path: &Path, config: &Config) -> Result<(), ConfigError> {
    info!("Generating bindings to WDK: types.rs");

    Ok(bindgen::Builder::wdk_default(vec!["src/input.h"], config)?
        .with_codegen_config(CodegenConfig::TYPES)
        .generate()
        .expect("Bindings should succeed to generate")
        .write_to_file(out_path.join("types.rs"))?)
}

fn generate_base(out_path: &Path, config: &Config) -> Result<(), ConfigError> {
    let outfile_name = match &config.driver_config {
        DriverConfig::Wdm | DriverConfig::Kmdf(_) => "ntddk.rs",
        DriverConfig::Umdf(_) => "windows.rs",
    };
    info!("Generating bindings to WDK: {outfile_name}.rs");

    Ok(bindgen::Builder::wdk_default(vec!["src/input.h"], config)?
        .with_codegen_config((CodegenConfig::TYPES | CodegenConfig::VARS).complement())
        .generate()
        .expect("Bindings should succeed to generate")
        .write_to_file(out_path.join(outfile_name))?)
}

fn generate_wdf(out_path: &Path, config: &Config) -> Result<(), ConfigError> {
    if let DriverConfig::Kmdf(_) | DriverConfig::Umdf(_) = &config.driver_config {
        info!("Generating bindings to WDK: wdf.rs");

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
    } else {
        info!(
            "Skipping wdf.rs generation since driver_config is {:#?}",
            config.driver_config
        );
        Ok(())
    }
}

/// Generates a `wdf_function_table.rs` file in `OUT_DIR` which contains the
/// definition of `WDF_FUNCTION_TABLE`. This is required to be generated here
/// since the size of the table is derived from either a global symbol
/// (`WDF_FUNCTION_COUNT`) that newer WDF versions expose, or an enum that older
/// versions use.
fn generate_wdf_function_table(out_path: &Path, config: &Config) -> std::io::Result<()> {
    const MINIMUM_MINOR_VERSION_TO_GENERATE_WDF_FUNCTION_COUNT: u8 = 25;

    let generated_file_path = out_path.join("wdf_function_table.rs");
    let mut generated_file = std::fs::File::create(generated_file_path)?;

    let is_wdf_function_count_generated = match *config {
        Config {
            driver_config:
                DriverConfig::Kmdf(KmdfConfig {
                    kmdf_version_major,
                    target_kmdf_version_minor,
                    ..
                }),
            ..
        } => {
            kmdf_version_major >= 1
                && target_kmdf_version_minor >= MINIMUM_MINOR_VERSION_TO_GENERATE_WDF_FUNCTION_COUNT
        }

        Config {
            driver_config:
                DriverConfig::Umdf(UmdfConfig {
                    umdf_version_major,
                    target_umdf_version_minor,
                    ..
                }),
            ..
        } => {
            umdf_version_major >= 2
                && target_umdf_version_minor >= MINIMUM_MINOR_VERSION_TO_GENERATE_WDF_FUNCTION_COUNT
        }

        _ => {
            unreachable!(
                "generate_wdf_function_table is only called with WDF driver configurations"
            )
        }
    };

    let wdf_function_table_code_snippet = if is_wdf_function_count_generated {
        WDF_FUNCTION_TABLE_TEMPLATE
            .replace(NUM_WDF_FUNCTIONS_PLACEHOLDER, "crate::WdfFunctionCount")
            .replace(
                WDF_FUNCTION_COUNT_DECLARATION_PLACEHOLDER,
                WDF_FUNCTION_COUNT_DECLARATION_EXTERNAL_SYMBOL,
            )
    } else {
        WDF_FUNCTION_TABLE_TEMPLATE
            .replace(
                NUM_WDF_FUNCTIONS_PLACEHOLDER,
                "crate::_WDFFUNCENUM::WdfFunctionTableNumEntries",
            )
            .replace(
                WDF_FUNCTION_COUNT_DECLARATION_PLACEHOLDER,
                WDF_FUNCTION_COUNT_DECLARATION_TABLE_INDEX,
            )
    };

    generated_file.write_all(wdf_function_table_code_snippet.as_bytes())?;
    Ok(())
}

/// Generates a `macros.rs` file in `OUT_DIR` which contains a
/// `call_unsafe_wdf_function_binding!` macro that redirects to the
/// `wdk_macros::call_unsafe_wdf_function_binding` `proc_macro` . This is
/// required in order to add an additional argument with the path to the file
/// containing generated types. There is currently no other way to pass
/// `OUT_DIR` of `wdk-sys` to the `proc_macro`.
fn generate_call_unsafe_wdf_function_binding_macro(out_path: &Path) -> std::io::Result<()> {
    let generated_file_path = out_path.join("call_unsafe_wdf_function_binding.rs");
    let mut generated_file = std::fs::File::create(generated_file_path)?;
    generated_file.write_all(
        CALL_UNSAFE_WDF_BINDING_TEMPLATE
            .replace(
                OUT_DIR_PLACEHOLDER,
                out_path.join("types.rs").to_str().expect(
                    "path to file with generated type information should successfully convert to \
                     a str",
                ),
            )
            .as_bytes(),
    )?;
    Ok(())
}

/// Generates a `test_stubs.rs` file in `OUT_DIR` which contains stubs required
/// for tests to compile. This should only generate the stubs whose names are
/// dependent on the WDK configuration, and would otherwise be impossible to
/// just include in `src/test_stubs.rs` directly.
fn generate_test_stubs(out_path: &Path, config: &Config) -> std::io::Result<()> {
    let stubs_file_path = out_path.join("test_stubs.rs");
    let mut stubs_file = std::fs::File::create(stubs_file_path)?;
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

fn main() -> anyhow::Result<()> {
    initialize_tracing()?;

    configure_wdk_library_build_and_then(|config| {
        let out_path = PathBuf::from(
            env::var("OUT_DIR").expect("OUT_DIR should be exist in Cargo build environment"),
        );

        thread::scope(|thread_scope| {
            let mut thread_join_handles = Vec::new();

            info_span!("bindgen generation").in_scope(|| {
                let out_path = &out_path;
                let config = &config;

                for (file_name, generate_function) in BINDGEN_FILE_GENERATORS_TUPLES {
                    let current_span = Span::current();

                    thread_join_handles.push(
                        thread::Builder::new()
                            .name(format!("bindgen {file_name} generator"))
                            .spawn_scoped(thread_scope, move || {
                                // Parent span must be manually set since spans do not persist across thread boundaries: https://github.com/tokio-rs/tracing/issues/1391
                                info_span!(parent: current_span, "worker thread", generated_file_name = file_name).in_scope(|| generate_function(out_path, config))
                            })
                            .expect("Scoped Thread should spawn successfully"),
                    );
                }
            });

            if let DriverConfig::Kmdf(_) | DriverConfig::Umdf(_) = config.driver_config {
                let current_span = Span::current();
                // Compile a c library to expose symbols that are not exposed because of
                // __declspec(selectany)
                thread_join_handles.push(
                    thread::Builder::new()
                        .name("wdf.c cc compilation".to_string())
                        .spawn_scoped(thread_scope, || {
                            // Parent span must be manually set since spans do not persist across thread boundaries: https://github.com/tokio-rs/tracing/issues/1391
                            info_span!(parent: current_span, "cc").in_scope(|| {
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
                            })
                        })
                        .expect("Scoped Thread should spawn successfully"),
                );

                info_span!("wdf_function_table.rs generation").in_scope(|| {
                    generate_wdf_function_table(&out_path, &config)?;
                    Ok::<(), std::io::Error>(())
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

            for join_handle in thread_join_handles {
                let thread_name = join_handle.thread().name().unwrap_or("UNNAMED").to_string();
                join_handle
                    .join()
                    .expect("Thread should complete without panicking")
                    .with_context(|| {
                        format!(r#""{thread_name}" thread failed to exit successfully"#)
                    })?;
            }
            Ok::<(), anyhow::Error>(())
        })?;

        Ok::<(), anyhow::Error>(())
    })?;

    Ok(())
}
