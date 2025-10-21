// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Build script for the `wdk-sys` crate.
//!
//! This parses the WDK configuration from metadata provided in the build tree,
//! and generates the relevant bindings to WDK APIs.

use std::{
    env,
    fs::File,
    io::Write,
    panic,
    path::{Path, PathBuf},
    sync::LazyLock,
    thread,
};

use anyhow::Context;
use bindgen::CodegenConfig;
use tracing::{Span, info, info_span, trace};
use tracing_subscriber::{
    EnvFilter,
    filter::{LevelFilter, ParseError},
};
use wdk_build::{
    ApiSubset,
    BuilderExt,
    Config,
    ConfigError,
    DriverConfig,
    IoError,
    KmdfConfig,
    UmdfConfig,
    configure_wdk_library_build_and_then,
};

const OUT_DIR_PLACEHOLDER: &str =
    "<PLACEHOLDER FOR LITERAL VALUE CONTAINING OUT_DIR OF wdk-sys CRATE>";
const WDFFUNCTIONS_SYMBOL_NAME_PLACEHOLDER: &str =
    "<PLACEHOLDER FOR LITERAL VALUE CONTAINING WDFFUNCTIONS SYMBOL NAME>";
const WDF_FUNCTION_COUNT_PLACEHOLDER: &str =
    "<PLACEHOLDER FOR EXPRESSION FOR NUMBER OF WDF FUNCTIONS IN `wdk_sys::WdfFunctions`";

const WDF_FUNCTION_COUNT_DECLARATION_EXTERNAL_SYMBOL: &str =
    "// SAFETY: `crate::WdfFunctionCount` is generated as a mutable static, but is not supposed \
     to be ever mutated by WDF.
    (unsafe { crate::WdfFunctionCount }) as usize";

const WDF_FUNCTION_COUNT_DECLARATION_TABLE_INDEX: &str =
    "crate::_WDFFUNCENUM::WdfFunctionTableNumEntries as usize";

static WDF_FUNCTION_COUNT_FUNCTION_TEMPLATE: LazyLock<String> = LazyLock::new(|| {
    format!(
        r"#[allow(clippy::must_use_candidate)]
/// Returns the number of functions available in the WDF function table.
/// Should not be used in public API.
pub fn get_wdf_function_count() -> usize {{
    {WDF_FUNCTION_COUNT_PLACEHOLDER}
}}"
    )
});

static CALL_UNSAFE_WDF_BINDING_TEMPLATE: LazyLock<String> = LazyLock::new(|| {
    format!(
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
    )
});

static TEST_STUBS_TEMPLATE: LazyLock<String> = LazyLock::new(|| {
    format!(
        r"
use crate::WDFFUNC;

/// Stubbed version of the symbol that `WdfFunctions` links to so that test targets will compile
// SAFETY: Generated WDF symbol name is required for test compilation and is unique per build.
// No other symbols in this crate export this name, preventing linker conflicts.
#[unsafe(no_mangle)]
pub static mut {WDFFUNCTIONS_SYMBOL_NAME_PLACEHOLDER}: *const WDFFUNC = core::ptr::null();
",
    )
});

/// Enabled API subsets based off of cargo-features
const ENABLED_API_SUBSETS: &[ApiSubset] = &[
    ApiSubset::Base,
    ApiSubset::Wdf,
    #[cfg(feature = "gpio")]
    ApiSubset::Gpio,
    #[cfg(feature = "hid")]
    ApiSubset::Hid,
    #[cfg(feature = "parallel-ports")]
    ApiSubset::ParallelPorts,
    #[cfg(feature = "spb")]
    ApiSubset::Spb,
    #[cfg(feature = "storage")]
    ApiSubset::Storage,
    #[cfg(feature = "usb")]
    ApiSubset::Usb,
];

type GenerateFn = fn(&Path, &Config) -> Result<(), ConfigError>;
const BINDGEN_FILE_GENERATORS_TUPLES: &[(&str, GenerateFn)] = &[
    ("constants.rs", generate_constants),
    ("types.rs", generate_types),
    ("base.rs", generate_base),
    ("wdf.rs", generate_wdf),
    #[cfg(feature = "gpio")]
    ("gpio.rs", generate_gpio),
    #[cfg(feature = "hid")]
    ("hid.rs", generate_hid),
    #[cfg(feature = "parallel-ports")]
    ("parallel_ports.rs", generate_parallel_ports),
    #[cfg(feature = "spb")]
    ("spb.rs", generate_spb),
    #[cfg(feature = "storage")]
    ("storage.rs", generate_storage),
    #[cfg(feature = "usb")]
    ("usb.rs", generate_usb),
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

    let header_contents = config.bindgen_header_contents(ENABLED_API_SUBSETS.iter().copied())?;
    trace!(header_contents = ?header_contents);

    let bindgen_builder = bindgen::Builder::wdk_default(config)?
        .with_codegen_config(CodegenConfig::VARS)
        .header_contents("constants-input.h", &header_contents);
    trace!(bindgen_builder = ?bindgen_builder);

    let output_file_path = out_path.join("constants.rs");
    Ok(bindgen_builder
        .generate()
        .expect("Bindings should succeed to generate")
        .write_to_file(&output_file_path)
        .map_err(|source| IoError::with_path(output_file_path, source))?)
}

fn generate_types(out_path: &Path, config: &Config) -> Result<(), ConfigError> {
    info!("Generating bindings to WDK: types.rs");

    let header_contents = config.bindgen_header_contents(ENABLED_API_SUBSETS.iter().copied())?;
    trace!(header_contents = ?header_contents);

    let bindgen_builder = bindgen::Builder::wdk_default(config)?
        .with_codegen_config(CodegenConfig::TYPES)
        .header_contents("types-input.h", &header_contents);
    trace!(bindgen_builder = ?bindgen_builder);

    let output_file_path = out_path.join("types.rs");
    Ok(bindgen_builder
        .generate()
        .expect("Bindings should succeed to generate")
        .write_to_file(&output_file_path)
        .map_err(|source| IoError::with_path(output_file_path, source))?)
}

fn generate_base(out_path: &Path, config: &Config) -> Result<(), ConfigError> {
    let outfile_name = match &config.driver_config {
        DriverConfig::Wdm | DriverConfig::Kmdf(_) => "ntddk",
        DriverConfig::Umdf(_) => "windows",
    };
    info!("Generating bindings to WDK: {outfile_name}.rs");

    let header_contents = config.bindgen_header_contents([ApiSubset::Base])?;
    trace!(header_contents = ?header_contents);

    let bindgen_builder = bindgen::Builder::wdk_default(config)?
        .with_codegen_config((CodegenConfig::TYPES | CodegenConfig::VARS).complement())
        .header_contents(&format!("{outfile_name}-input.h"), &header_contents);
    trace!(bindgen_builder = ?bindgen_builder);

    let output_file_path = out_path.join(format!("{outfile_name}.rs"));
    Ok(bindgen_builder
        .generate()
        .expect("Bindings should succeed to generate")
        .write_to_file(&output_file_path)
        .map_err(|source| IoError::with_path(output_file_path, source))?)
}

fn generate_wdf(out_path: &Path, config: &Config) -> Result<(), ConfigError> {
    if let DriverConfig::Kmdf(_) | DriverConfig::Umdf(_) = config.driver_config {
        info!("Generating bindings to WDK: wdf.rs");

        let header_contents = config.bindgen_header_contents([ApiSubset::Base, ApiSubset::Wdf])?;
        trace!(header_contents = ?header_contents);

        let bindgen_builder = bindgen::Builder::wdk_default(config)?
            .with_codegen_config((CodegenConfig::TYPES | CodegenConfig::VARS).complement())
            .header_contents("wdf-input.h", &header_contents)
            // Only generate for files that are prefixed with (case-insensitive) wdf (ie.
            // /some/path/WdfSomeHeader.h), to prevent duplication of code in ntddk.rs
            .allowlist_file("(?i).*wdf.*");
        trace!(bindgen_builder = ?bindgen_builder);

        let output_file_path = out_path.join("wdf.rs");
        Ok(bindgen_builder
            .generate()
            .expect("Bindings should succeed to generate")
            .write_to_file(&output_file_path)
            .map_err(|source| IoError::with_path(output_file_path, source))?)
    } else {
        info!(
            "Skipping wdf.rs generation since driver_config is {:#?}",
            config.driver_config
        );
        Ok(())
    }
}

#[cfg(feature = "gpio")]
fn generate_gpio(out_path: &Path, config: &Config) -> Result<(), ConfigError> {
    info!("Generating bindings to WDK: gpio.rs");

    let header_contents =
        config.bindgen_header_contents([ApiSubset::Base, ApiSubset::Wdf, ApiSubset::Gpio])?;
    trace!(header_contents = ?header_contents);

    let bindgen_builder = {
        let mut builder = bindgen::Builder::wdk_default(config)?
            .with_codegen_config((CodegenConfig::TYPES | CodegenConfig::VARS).complement())
            .header_contents("gpio-input.h", &header_contents);

        // Only allowlist files in the gpio-specific files to avoid
        // duplicate definitions
        for header_file in config.headers(ApiSubset::Gpio)? {
            builder = builder.allowlist_file(format!("(?i).*{header_file}.*"));
        }
        builder
    };
    trace!(bindgen_builder = ?bindgen_builder);

    let output_file_path = out_path.join("gpio.rs");
    Ok(bindgen_builder
        .generate()
        .expect("Bindings should succeed to generate")
        .write_to_file(&output_file_path)
        .map_err(|source| IoError::with_path(output_file_path, source))?)
}

#[cfg(feature = "hid")]
fn generate_hid(out_path: &Path, config: &Config) -> Result<(), ConfigError> {
    info!("Generating bindings to WDK: hid.rs");

    let header_contents =
        config.bindgen_header_contents([ApiSubset::Base, ApiSubset::Wdf, ApiSubset::Hid])?;
    trace!(header_contents = ?header_contents);

    let bindgen_builder = {
        let mut builder = bindgen::Builder::wdk_default(config)?
            .with_codegen_config((CodegenConfig::TYPES | CodegenConfig::VARS).complement())
            .header_contents("hid-input.h", &header_contents);

        // Only allowlist files in the hid-specific files to avoid
        // duplicate definitions
        for header_file in config.headers(ApiSubset::Hid)? {
            builder = builder.allowlist_file(format!("(?i).*{header_file}.*"));
        }
        builder
    };
    trace!(bindgen_builder = ?bindgen_builder);

    let output_file_path = out_path.join("hid.rs");
    Ok(bindgen_builder
        .generate()
        .expect("Bindings should succeed to generate")
        .write_to_file(&output_file_path)
        .map_err(|source| IoError::with_path(output_file_path, source))?)
}

#[cfg(feature = "parallel-ports")]
fn generate_parallel_ports(out_path: &Path, config: &Config) -> Result<(), ConfigError> {
    info!("Generating bindings to WDK: parallel_ports.rs");

    let header_contents = config.bindgen_header_contents([
        ApiSubset::Base,
        ApiSubset::Wdf,
        ApiSubset::ParallelPorts,
    ])?;
    trace!(header_contents = ?header_contents);

    let bindgen_builder = {
        let mut builder = bindgen::Builder::wdk_default(config)?
            .with_codegen_config((CodegenConfig::TYPES | CodegenConfig::VARS).complement())
            .header_contents("parallel-ports-input.h", &header_contents);

        // Only allowlist files in the parallel-ports-specific files to
        // avoid duplicate definitions
        for header_file in config.headers(ApiSubset::ParallelPorts)? {
            builder = builder.allowlist_file(format!("(?i).*{header_file}.*"));
        }
        builder
    };
    trace!(bindgen_builder = ?bindgen_builder);

    let output_file_path = out_path.join("parallel_ports.rs");
    Ok(bindgen_builder
        .generate()
        .expect("Bindings should succeed to generate")
        .write_to_file(&output_file_path)
        .map_err(|source| IoError::with_path(output_file_path, source))?)
}

#[cfg(feature = "spb")]
fn generate_spb(out_path: &Path, config: &Config) -> Result<(), ConfigError> {
    info!("Generating bindings to WDK: spb.rs");

    let header_contents =
        config.bindgen_header_contents([ApiSubset::Base, ApiSubset::Wdf, ApiSubset::Spb])?;
    trace!(header_contents = ?header_contents);

    let bindgen_builder = {
        let mut builder = bindgen::Builder::wdk_default(config)?
            .with_codegen_config((CodegenConfig::TYPES | CodegenConfig::VARS).complement())
            .header_contents("spb-input.h", &header_contents);

        // Only allowlist files in the spb-specific files to avoid
        // duplicate definitions
        for header_file in config.headers(ApiSubset::Spb)? {
            builder = builder.allowlist_file(format!("(?i).*{header_file}.*"));
        }
        builder
    };
    trace!(bindgen_builder = ?bindgen_builder);

    let output_file_path = out_path.join("spb.rs");
    Ok(bindgen_builder
        .generate()
        .expect("Bindings should succeed to generate")
        .write_to_file(&output_file_path)
        .map_err(|source| IoError::with_path(output_file_path, source))?)
}

#[cfg(feature = "storage")]
fn generate_storage(out_path: &Path, config: &Config) -> Result<(), ConfigError> {
    info!("Generating bindings to WDK: storage.rs");

    let header_contents =
        config.bindgen_header_contents([ApiSubset::Base, ApiSubset::Wdf, ApiSubset::Storage])?;
    trace!(header_contents = ?header_contents);

    let bindgen_builder = {
        let mut builder = bindgen::Builder::wdk_default(config)?
            .with_codegen_config((CodegenConfig::TYPES | CodegenConfig::VARS).complement())
            .header_contents("storage-input.h", &header_contents);

        // Only allowlist files in the storage-specific files to avoid
        // duplicate definitions
        for header_file in config.headers(ApiSubset::Storage)? {
            builder = builder.allowlist_file(format!("(?i).*{header_file}.*"));
        }
        builder
    };
    trace!(bindgen_builder = ?bindgen_builder);

    let output_file_path = out_path.join("storage.rs");
    Ok(bindgen_builder
        .generate()
        .expect("Bindings should succeed to generate")
        .write_to_file(&output_file_path)
        .map_err(|source| IoError::with_path(output_file_path, source))?)
}

#[cfg(feature = "usb")]
fn generate_usb(out_path: &Path, config: &Config) -> Result<(), ConfigError> {
    info!("Generating bindings to WDK: usb.rs");

    let header_contents =
        config.bindgen_header_contents([ApiSubset::Base, ApiSubset::Wdf, ApiSubset::Usb])?;
    trace!(header_contents = ?header_contents);

    let bindgen_builder = {
        let mut builder = bindgen::Builder::wdk_default(config)?
            .with_codegen_config((CodegenConfig::TYPES | CodegenConfig::VARS).complement())
            .header_contents("usb-input.h", &header_contents);

        // Only allowlist files in the usb-specific files to avoid
        // duplicate definitions
        for header_file in config.headers(ApiSubset::Usb)? {
            builder = builder.allowlist_file(format!("(?i).*{header_file}.*"));
        }
        builder
    };
    trace!(bindgen_builder = ?bindgen_builder);

    let output_file_path = out_path.join("usb.rs");
    Ok(bindgen_builder
        .generate()
        .expect("Bindings should succeed to generate")
        .write_to_file(&output_file_path)
        .map_err(|source| IoError::with_path(output_file_path, source))?)
}

/// Generates a `wdf_function_count.rs` file in `OUT_DIR` which contains the
/// definition of the function `get_wdf_function_count()`. This is required to
/// be generated here since the size of the table is derived from either a
/// global symbol that newer WDF versions expose, or an enum that older versions
/// use.
fn generate_wdf_function_count(out_path: &Path, config: &Config) -> Result<(), IoError> {
    const MINIMUM_MINOR_VERSION_TO_GENERATE_WDF_FUNCTION_COUNT: u8 = 25;

    let generated_file_path = out_path.join("wdf_function_count.rs");
    let mut generated_file = File::create(&generated_file_path)
        .map_err(|source| IoError::with_path(&generated_file_path, source))?;

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

    let wdf_function_table_count_snippet = WDF_FUNCTION_COUNT_FUNCTION_TEMPLATE.replace(
        WDF_FUNCTION_COUNT_PLACEHOLDER,
        if is_wdf_function_count_generated {
            WDF_FUNCTION_COUNT_DECLARATION_EXTERNAL_SYMBOL
        } else {
            WDF_FUNCTION_COUNT_DECLARATION_TABLE_INDEX
        },
    );

    generated_file
        .write_all(wdf_function_table_count_snippet.as_bytes())
        .map_err(|source| IoError::with_path(generated_file_path, source))?;
    Ok(())
}

/// Generates a `macros.rs` file in `OUT_DIR` which contains a
/// `call_unsafe_wdf_function_binding!` macro that redirects to the
/// `wdk_macros::call_unsafe_wdf_function_binding` `proc_macro` . This is
/// required in order to add an additional argument with the path to the file
/// containing generated types. There is currently no other way to pass
/// `OUT_DIR` of `wdk-sys` to the `proc_macro`.
fn generate_call_unsafe_wdf_function_binding_macro(out_path: &Path) -> Result<(), IoError> {
    let generated_file_path = out_path.join("call_unsafe_wdf_function_binding.rs");
    let mut generated_file = File::create(&generated_file_path)
        .map_err(|source| IoError::with_path(&generated_file_path, source))?;
    generated_file
        .write_all(
            CALL_UNSAFE_WDF_BINDING_TEMPLATE
                .replace(
                    OUT_DIR_PLACEHOLDER,
                    out_path.join("types.rs").to_str().expect(
                        "path to file with generated type information should successfully convert \
                         to a str",
                    ),
                )
                .as_bytes(),
        )
        .map_err(|source| IoError::with_path(generated_file_path, source))?;
    Ok(())
}

/// Generates a `test_stubs.rs` file in `OUT_DIR` which contains stubs required
/// for tests to compile. This should only generate the stubs whose names are
/// dependent on the WDK configuration, and would otherwise be impossible to
/// just include in `src/test_stubs.rs` directly.
fn generate_test_stubs(out_path: &Path, config: &Config) -> Result<(), IoError> {
    let stubs_file_path = out_path.join("test_stubs.rs");
    let mut stubs_file = File::create(&stubs_file_path)
        .map_err(|source| IoError::with_path(&stubs_file_path, source))?;
    stubs_file
        .write_all(
            TEST_STUBS_TEMPLATE
                .replace(
                    WDFFUNCTIONS_SYMBOL_NAME_PLACEHOLDER,
                    &config.compute_wdffunctions_symbol_name().expect(
                        "KMDF and UMDF configs should always have a computable WdfFunctions \
                         symbol name",
                    ),
                )
                .as_bytes(),
        )
        .map_err(|source| IoError::with_path(stubs_file_path, source))?;
    Ok(())
}

/// Starts parallel bindgen tasks for generating binding files.
fn start_bindgen_tasks<'scope>(
    thread_scope: &'scope thread::Scope<'scope, '_>,
    out_path: &'scope Path,
    config: &'scope Config,
    thread_join_handles: &mut Vec<thread::ScopedJoinHandle<'scope, Result<(), ConfigError>>>,
) {
    info_span!("bindgen generation").in_scope(|| {
        for (file_name, generate_function) in BINDGEN_FILE_GENERATORS_TUPLES {
            let current_span = Span::current();

            thread_join_handles.push(
                thread::Builder::new()
                    .name(format!("bindgen {file_name} generator"))
                    .spawn_scoped(thread_scope, move || {
                        // Parent span must be manually set since spans do not persist across thread boundaries: https://github.com/tokio-rs/tracing/issues/1391
                        info_span!(parent: &current_span, "worker thread", generated_file_name = file_name).in_scope(|| generate_function(out_path, config))
                    })
                    .expect("Scoped Thread should spawn successfully"),
            );
        }
    });
}

/// Starts a task that compiles a C shim to expose WDF symbols hidden by
/// `__declspec(selectany)`.
fn start_wdf_symbol_export_tasks<'scope>(
    thread_scope: &'scope thread::Scope<'scope, '_>,
    out_path: &'scope Path,
    config: &'scope Config,
    thread_join_handles: &mut Vec<thread::ScopedJoinHandle<'scope, Result<(), ConfigError>>>,
) {
    let current_span = Span::current();

    // Compile a c library to expose symbols that are not exposed because of
    // __declspec(selectany)
    thread_join_handles.push(
        thread::Builder::new()
            .name("wdf.c cc compilation".to_string())
            .spawn_scoped(thread_scope, move || {
                // Parent span must be manually set since spans do not persist across thread boundaries: https://github.com/tokio-rs/tracing/issues/1391
                info_span!(parent: current_span, "cc").in_scope(|| {
                    info!("Compiling wdf.c");

                    // Write all included headers into wdf.c (existing file, if present
                    // (i.e. incremental rebuild), is truncated)
                    let wdf_c_file_path = out_path.join("wdf.c");
                    {
                        let mut wdf_c_file = File::create(&wdf_c_file_path)
                            .map_err(|source| IoError::with_path(&wdf_c_file_path, source))?;
                        wdf_c_file
                            .write_all(
                                config
                                    // This should include the entirety of the `ENABLED_API_SUBSETS`, but this is currently blocked by issues with mutually exclusive headers: https://github.com/microsoft/windows-drivers-rs/issues/515
                                    .bindgen_header_contents([
                                        ApiSubset::Base,
                                        ApiSubset::Wdf,
                                        #[cfg(feature = "hid")]
                                        ApiSubset::Hid,
                                        #[cfg(feature = "spb")]
                                        ApiSubset::Spb,
                                    ])?
                                    .as_bytes(),
                            )
                            .map_err(|source| IoError::with_path(&wdf_c_file_path, source))?;

                        // Explicitly sync_all to surface any IO errors (File::drop
                        // silently ignores close errors)
                        wdf_c_file
                            .sync_all()
                            .map_err(|source| IoError::with_path(&wdf_c_file_path, source))?;
                    }

                    let mut cc_builder = cc::Build::new();
                    for (key, value) in config.preprocessor_definitions() {
                        cc_builder.define(&key, value.as_deref());
                    }

                    cc_builder
                        .includes(config.include_paths()?)
                        .file(wdf_c_file_path)
                        .compile("wdf");
                    Ok::<(), ConfigError>(())
                })
            })
            .expect("Scoped Thread should spawn successfully"),
    );
}

/// Starts generation/compilation tasks for WDF-specific artifacts for driver
/// configurations.
///
/// Uses the `start_*_tasks` naming convention: dispatches work to scoped
/// threads and returns after scheduling.
fn start_wdf_artifact_tasks<'scope>(
    thread_scope: &'scope thread::Scope<'scope, '_>,
    out_path: &'scope Path,
    config: &'scope Config,
    thread_join_handles: &mut Vec<thread::ScopedJoinHandle<'scope, Result<(), ConfigError>>>,
) -> anyhow::Result<()> {
    if let DriverConfig::Kmdf(_) | DriverConfig::Umdf(_) = config.driver_config {
        start_wdf_symbol_export_tasks(thread_scope, out_path, config, thread_join_handles);

        info_span!("wdf_function_count.rs generation")
            .in_scope(|| generate_wdf_function_count(out_path, config))?;

        info_span!("call_unsafe_wdf_function_binding.rs generation")
            .in_scope(|| generate_call_unsafe_wdf_function_binding_macro(out_path))?;

        info_span!("test_stubs.rs generation")
            .in_scope(|| generate_test_stubs(out_path, config))?;
    }
    Ok(())
}

/// Joins all worker threads and collects their results
fn join_worker_threads(
    thread_join_handles: Vec<thread::ScopedJoinHandle<'_, Result<(), ConfigError>>>,
) -> anyhow::Result<()> {
    for join_handle in thread_join_handles {
        let thread_name = join_handle.thread().name().unwrap_or("UNNAMED").to_string();

        match join_handle.join() {
            // Forward panics to the main thread
            Err(panic_payload) => {
                panic::resume_unwind(panic_payload);
            }

            Ok(thread_result) => {
                thread_result.with_context(|| {
                    format!(r#""{thread_name}" thread failed to exit successfully"#)
                })?;
            }
        }
    }
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

            start_bindgen_tasks(thread_scope, &out_path, &config, &mut thread_join_handles);
            start_wdf_artifact_tasks(thread_scope, &out_path, &config, &mut thread_join_handles)?;

            join_worker_threads(thread_join_handles)
        })?;

        Ok::<(), anyhow::Error>(())
    })?;

    Ok(())
}
