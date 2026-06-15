// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Build script for the `wdk-sys` crate.
//!
//! This parses the WDK configuration from metadata provided in the build tree,
//! and generates the relevant bindings to WDK APIs.

use std::{
    collections::HashSet,
    env,
    fmt::Write as _,
    fs::{self, File},
    io::Write,
    panic,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex},
    thread,
};

use bindgen::{
    CodegenConfig,
    callbacks::{self, DiscoveredItem},
};
use tracing::{Span, info, info_span, trace, warn};
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
    derives::{BaseDerivesCallback, DerivesMap},
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

type GenerateFn = fn(&Path, &Config) -> Result<(), ConfigError>;
const BASE_BINDGEN_FILE_GENERATORS: &[(&str, GenerateFn)] = &[
    ("base_constants.rs", generate_base_constants),
    ("base_functions.rs", generate_base_functions),
    ("wdf.rs", generate_wdf),
];

const BASE_API_SUBSETS: &[ApiSubset] = &[ApiSubset::Base, ApiSubset::Wdf];

const FUNCTIONS_CODEGEN: CodegenConfig =
    (CodegenConfig::TYPES.union(CodegenConfig::VARS)).complement();

const API_SUBSET_CONFIGS: &[(&str, ApiSubset)] = &[
    #[cfg(feature = "gpio")]
    ("gpio", ApiSubset::Gpio),
    #[cfg(feature = "hid")]
    ("hid", ApiSubset::Hid),
    #[cfg(feature = "parallel-ports")]
    ("parallel_ports", ApiSubset::ParallelPorts),
    #[cfg(feature = "spb")]
    ("spb", ApiSubset::Spb),
    #[cfg(feature = "storage")]
    ("storage", ApiSubset::Storage),
    #[cfg(feature = "usb")]
    ("usb", ApiSubset::Usb),
];

#[derive(Debug, Default, Clone)]
struct IncludeTracker {
    files: Arc<Mutex<HashSet<String>>>,
}

impl callbacks::ParseCallbacks for IncludeTracker {
    fn include_file(&self, filename: &str) {
        // `bindgen`'s `blocklist_file` matches against `libclang`'s raw
        // `clang_getFileName` output without any normalization (see
        // `bindgen`'s `ir/item.rs::can_be_rendered`). Store the path
        // verbatim here so the regex we later feed to `blocklist_file`
        // matches whatever casing/format `libclang` reports.
        //
        // `bindgen` fires `include_file` once per `#include` site, so
        // the same header path appears many times across a translation
        // unit; the set keeps only one entry per file.
        self.files
            .lock()
            .expect("Mutex should not be poisoned")
            .insert(filename.to_string());
    }
}

#[derive(Debug, Default, Clone)]
struct ApiSubsetCallbacks {
    discovered_type_names: Arc<Mutex<Vec<String>>>,
}

/// Scoped join handle for an API subset's types-pass thread. Each one
/// returns the API subset name paired with the type names that pass
/// emitted, for the aggregator to re-export.
type TypesPassHandle<'scope> =
    thread::ScopedJoinHandle<'scope, Result<(String, Vec<String>), ConfigError>>;

/// Each API subset produces three independent bindgen passes — types,
/// constants, and functions — that emit `{api_subset_name}_types.rs`,
/// `{api_subset_name}_constants.rs`, and `{api_subset_name}.rs` respectively.
/// The variants differ in codegen config, callbacks installed, and allowlist
/// behavior; encoding them as an enum makes the per-pass differences
/// explicit at the call site.
enum ApiSubsetPass {
    /// Emits type definitions only. Collects the discovered type names for
    /// the aggregator and consults `derive_map` so API-subset structs can
    /// derive traits even when their fields reference blocklisted base
    /// types.
    Types { derive_map: Arc<DerivesMap> },
    /// Emits `pub const` items only.
    Constants,
    /// Emits extern function bindings only. Restricts the allowlist to
    /// API-subset-owned header files so unrelated transitively-included
    /// functions don't leak into the API subset's module.
    Functions,
}

impl callbacks::ParseCallbacks for ApiSubsetCallbacks {
    fn new_item_found(&self, _id: callbacks::DiscoveredItemId, item: callbacks::DiscoveredItem) {
        let name = match &item {
            DiscoveredItem::Struct { final_name, .. }
            | DiscoveredItem::Union { final_name, .. }
            | DiscoveredItem::Enum { final_name, .. } => final_name,
            DiscoveredItem::Alias { alias_name, .. } => alias_name,
            DiscoveredItem::Function { .. } | DiscoveredItem::Method { .. } => return,
        };
        self.discovered_type_names
            .lock()
            .expect("Mutex should not be poisoned")
            .push(name.clone());
    }
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
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .init();

    Ok(())
}

/// Joins all worker threads and collects their results
fn join_worker_threads<T, E>(
    thread_join_handles: Vec<thread::ScopedJoinHandle<'_, Result<T, E>>>,
) -> anyhow::Result<Vec<T>>
where
    E: Into<anyhow::Error> + Send + 'static,
    T: Send + 'static,
{
    let mut results = Vec::with_capacity(thread_join_handles.len());
    for join_handle in thread_join_handles {
        let thread_name = join_handle.thread().name().unwrap_or("UNNAMED").to_string();

        match join_handle.join() {
            // Forward panics to the main thread
            Err(panic_payload) => {
                panic::resume_unwind(panic_payload);
            }

            Ok(Err(thread_error)) => {
                return Err(thread_error.into().context(format!(
                    r#""{thread_name}" thread failed to exit successfully"#
                )));
            }

            Ok(Ok(value)) => results.push(value),
        }
    }
    Ok(results)
}

/// Generates `base_types.rs` and harvests the include set visited along the
/// way.
///
/// The returned path feeds `DeriveMap::from_file` so API-subset passes can
/// answer bindgen's `blocklisted_type_implements_trait` queries. The returned
/// `HashSet` is the set of headers bindgen actually visited while emitting
/// base types, used by API-subset passes to `blocklist_file` items already
/// emitted under `crate::types::*`.
fn generate_base_types(
    out_path: &Path,
    config: &Config,
) -> Result<(PathBuf, HashSet<String>), ConfigError> {
    info!("Generating bindings to WDK: base_types.rs");

    let header_contents = config.bindgen_header_contents(BASE_API_SUBSETS.iter().copied())?;
    trace!(header_contents = ?header_contents);

    let include_tracker = IncludeTracker::default();
    let bindgen_builder = bindgen::Builder::wdk_default(config)?
        .with_codegen_config(CodegenConfig::TYPES)
        .header_contents("base-types-input.h", &header_contents)
        .parse_callbacks(Box::new(include_tracker.clone()));
    trace!(bindgen_builder = ?bindgen_builder);

    let bindings = bindgen_builder
        .generate()
        .expect("Bindings should succeed to generate");

    let output_file_path = out_path.join("base_types.rs");
    fs::write(&output_file_path, bindings.to_string())
        .map_err(|source| IoError::with_path(output_file_path.clone(), source))?;

    // Clone the set out under the lock rather than trying to take ownership of
    // the `Arc` — bindgen may retain internal references to the callback
    // (e.g. via libclang-sys), so `Arc::into_inner` is not guaranteed to
    // succeed. The set is small and this runs once per build.
    let base_files = include_tracker
        .files
        .lock()
        .expect("Mutex should not be poisoned")
        .clone();
    info!("Discovered {} base files", base_files.len());

    Ok((output_file_path, base_files))
}

fn generate_base_constants(out_path: &Path, config: &Config) -> Result<(), ConfigError> {
    info!("Generating bindings to WDK: base_constants.rs");

    let header_contents = config.bindgen_header_contents(BASE_API_SUBSETS.iter().copied())?;
    trace!(header_contents = ?header_contents);

    let bindgen_builder = bindgen::Builder::wdk_default(config)?
        .with_codegen_config(CodegenConfig::VARS)
        .header_contents("constants-input.h", &header_contents);
    trace!(bindgen_builder = ?bindgen_builder);

    let output_file_path = out_path.join("base_constants.rs");
    bindgen_builder
        .generate()
        .expect("Bindings should succeed to generate")
        .write_to_file(&output_file_path)
        .map_err(|source| IoError::with_path(output_file_path, source))?;
    Ok(())
}

fn generate_base_functions(out_path: &Path, config: &Config) -> Result<(), ConfigError> {
    let outfile_name = match &config.driver_config {
        DriverConfig::Wdm | DriverConfig::Kmdf(_) => "ntddk",
        DriverConfig::Umdf(_) => "windows",
    };
    info!("Generating bindings to WDK: {outfile_name}.rs");

    let header_contents = config.bindgen_header_contents([ApiSubset::Base])?;
    trace!(header_contents = ?header_contents);

    let bindgen_builder = bindgen::Builder::wdk_default(config)?
        .with_codegen_config(FUNCTIONS_CODEGEN)
        .header_contents(&format!("{outfile_name}-input.h"), &header_contents);
    trace!(bindgen_builder = ?bindgen_builder);

    let output_file_path = out_path.join(format!("{outfile_name}.rs"));
    bindgen_builder
        .generate()
        .expect("Bindings should succeed to generate")
        .write_to_file(&output_file_path)
        .map_err(|source| IoError::with_path(output_file_path, source))?;
    Ok(())
}

fn generate_wdf(out_path: &Path, config: &Config) -> Result<(), ConfigError> {
    if let DriverConfig::Kmdf(_) | DriverConfig::Umdf(_) = config.driver_config {
        info!("Generating bindings to WDK: wdf.rs");

        let header_contents = config.bindgen_header_contents(BASE_API_SUBSETS.iter().copied())?;
        trace!(header_contents = ?header_contents);

        let bindgen_builder = bindgen::Builder::wdk_default(config)?
            .with_codegen_config(FUNCTIONS_CODEGEN)
            .header_contents("wdf-input.h", &header_contents)
            .allowlist_file("(?i).*wdf.*");
        trace!(bindgen_builder = ?bindgen_builder);

        let output_file_path = out_path.join("wdf.rs");
        bindgen_builder
            .generate()
            .expect("Bindings should succeed to generate")
            .write_to_file(&output_file_path)
            .map_err(|source| IoError::with_path(output_file_path, source))?;
    } else {
        info!(
            "Skipping wdf.rs generation since driver_config is {:#?}",
            config.driver_config
        );
    }
    Ok(())
}

/// Generates bindings for an API subset by blocklisting base header files. This
/// prevents base items from being re-emitted while allowing bindgen to
/// recursively follow type references through sub-headers. Base items are
/// resolved via `use crate::types::*;` in the generated output.
///
/// When `pass` is `ApiSubsetPass::Types`, installs a `ParseCallbacks` that
/// records every type bindgen emits and returns them as `Some(names)`. The
/// aggregator uses this list to emit targeted `pub use` statements instead
/// of a wildcard.
///
/// When `pass` is not `ApiSubsetPass::Types`, returns `None`.
fn generate_api_subset_bindings(
    out_path: &Path,
    config: &Config,
    api_subset_name: &str,
    api_subset: ApiSubset,
    base_files: &HashSet<String>,
    pass: &ApiSubsetPass,
) -> Result<Vec<String>, ConfigError> {
    let (stem, codegen) = match pass {
        ApiSubsetPass::Types { .. } => (format!("{api_subset_name}_types"), CodegenConfig::TYPES),
        ApiSubsetPass::Constants => (format!("{api_subset_name}_constants"), CodegenConfig::VARS),
        ApiSubsetPass::Functions => (api_subset_name.to_string(), FUNCTIONS_CODEGEN),
    };
    let filename = format!("{stem}.rs");
    info!("Generating bindings to WDK: {filename}");

    let header_contents = config.bindgen_header_contents(
        BASE_API_SUBSETS
            .iter()
            .copied()
            .chain(std::iter::once(api_subset)),
    )?;
    trace!(header_contents = ?header_contents);

    let api_subset_callbacks =
        matches!(pass, ApiSubsetPass::Types { .. }).then(ApiSubsetCallbacks::default);
    let mut builder = bindgen::Builder::wdk_default(config)?
        .with_codegen_config(codegen)
        .header_contents(&format!("{stem}-input.h"), &header_contents)
        // Per-API-subset prefix on anonymous fields prevents `_bindgen_ty_N`
        // collisions across API subsets.
        .anon_fields_prefix(format!("__{api_subset_name}_bindgen_anon_"))
        .raw_line(
            r#"#[allow(clippy::wildcard_imports, reason = "the underlying c code relies on all type definitions being in scope, which results in the bindgen generated code relying on the generated types being in scope as well")]"#,
        )
        .raw_line("#[allow(unused_imports)]")
        .raw_line("use crate::types::*;");
    if let Some(callbacks) = &api_subset_callbacks {
        builder = builder.parse_callbacks(Box::new(callbacks.clone()));
    }
    // Answers `blocklisted_type_implements_trait` for API-subset type passes
    // by consulting the derive set parsed from the base-types source. Without
    // this, bindgen treats every blocklisted base type as "unknown" and
    // suppresses derives on any API-subset struct that transitively contains
    // one — a regression of ~1,674 types vs. the single-TU build.
    if let ApiSubsetPass::Types { derive_map } = pass {
        builder =
            builder.parse_callbacks(Box::new(BaseDerivesCallback::new(Arc::clone(derive_map))));
    }

    // `regex::escape` turns path metacharacters (., \, (, )) into literal
    // matchers. Bindgen treats blocklist patterns as anchored regex (^...$), so
    // un-escaped metachars in Windows paths would match the wrong files. `(?i)`
    // enables case-insensitive matching for the whole pattern. Libclang may
    // return path casings that don't match what IncludeTracker recorded.
    for file in base_files {
        builder = builder.blocklist_file(format!("(?i){}", regex::escape(file)));
    }

    // Only restrict to API-subset-owned header files for the functions pass.
    // Types and constants are allowed to include transitively-pulled items
    // (e.g. types from poclass.h reached via ufxproprietarycharger.h) that
    // would otherwise be filtered out by an explicit allowlist.
    //
    // TODO: Re-evaluate whether this allowlist is still needed now that the
    // blocklist above suppresses every base-emitted item. The allowlist
    // pre-dates the blocklist mechanism (carried over from the pre-multi-pass
    // implementation) and may be redundant for any function declared in a
    // base file. It still guards against a non-base, non-API-subset-owned
    // header being transitively reached by two API subsets and producing
    // duplicate `extern` declarations — verify experimentally before
    // removing.
    if matches!(pass, ApiSubsetPass::Functions) {
        for header_file in config.headers(api_subset)? {
            builder = builder.allowlist_file(format!("(?i).*{}.*", regex::escape(&header_file)));
        }
    }
    trace!(bindgen_builder = ?builder);

    let output_file_path = out_path.join(&filename);
    builder
        .generate()
        .expect("API subset bindings should succeed to generate")
        .write_to_file(&output_file_path)
        .map_err(|source| IoError::with_path(output_file_path, source))?;

    // Same as the `IncludeTracker` callback above: clone under the lock rather
    // than trying to take ownership of the `Arc`, since bindgen may retain
    // internal references to the callback.
    Ok(api_subset_callbacks
        .map(|callbacks| {
            callbacks
                .discovered_type_names
                .lock()
                .expect("Mutex should not be poisoned")
                .clone()
        })
        .unwrap_or_default())
}

/// Returns true for bindgen's anonymous compound type names of the form
/// `_bindgen_ty_<digits>`.
///
/// Bindgen assigns these to anonymous C compounds (e.g. ntdef.h's
/// `TYPE_ALIGNMENT` macro expanding to `struct { char x; t test; }`) using a
/// per-invocation counter. The same name from different bindgen runs
/// therefore refers to unrelated types, so re-exporting one from an API subset
/// would silently shadow the base pass's name with a same-named-but-different
/// type. Skipping is safe because user code never references these by name.
fn is_bare_anonymous_type(name: &str) -> bool {
    name.strip_prefix("_bindgen_ty_")
        .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
}

/// Writes the aggregated `types.rs`, by composing base and API-subset type
/// bindings into a unified namespace. Base types are re-exported via a
/// wildcard, and API-subset types are re-exported individually so duplicates
/// across API subsets are dropped instead of colliding. Collisions between
/// base and API-subset types are resolved because Rust's named `pub use`
/// re-exports take precedence over `pub use *` glob re-exports in the same
/// scope, so the per-name API-subset entry wins over the base wildcard.
fn write_aggregated_types(
    out_path: &Path,
    api_subset_types: &[(String, Vec<String>)],
) -> Result<(), IoError> {
    let mut types_aggregator = String::from(
        r#"#[allow(unused)]
mod base_types { include!("base_types.rs"); }
#[allow(clippy::wildcard_imports)]
pub use base_types::*;

"#,
    );

    let mut exported_types = HashSet::<String>::new();
    for (api_subset_name, type_names) in api_subset_types {
        writeln!(
            types_aggregator,
            r#"#[allow(unused)]
mod {api_subset_name}_types {{ include!("{api_subset_name}_types.rs"); }}"#
        )
        .expect("writing to String is infallible");
        for type_name in type_names {
            if is_bare_anonymous_type(type_name) {
                warn!(
                    "Skipping bare anonymous type {type_name} from {api_subset_name}_types to \
                     avoid shadowing a base anon type with the same generated name"
                );
                continue;
            }
            if exported_types.insert(type_name.clone()) {
                writeln!(
                    types_aggregator,
                    "pub use {api_subset_name}_types::{type_name};"
                )
                .expect("writing to String is infallible");
            } else {
                trace!(
                    "Skipping duplicate type {type_name} from {api_subset_name}_types (already \
                     re-exported)"
                );
            }
        }
        types_aggregator.push('\n');
    }

    let types_file_path = out_path.join("types.rs");
    fs::write(&types_file_path, types_aggregator)
        .map_err(|source| IoError::with_path(types_file_path, source))?;
    Ok(())
}

/// Writes the aggregated `constants.rs`, which composes base and API-subset
/// constant bindings into a unified namespace. All constants are re-exported
/// via wildcards.
fn write_aggregated_constants(out_path: &Path, api_subset_names: &[String]) -> Result<(), IoError> {
    let mut constants_aggregator = String::from(
        r#"#[allow(unused, clippy::wildcard_imports)]
mod base_constants {
    use crate::types::*;
    include!("base_constants.rs");
}
#[allow(clippy::wildcard_imports)]
pub use base_constants::*;

"#,
    );

    for api_subset_name in api_subset_names {
        writeln!(
            constants_aggregator,
            r#"#[allow(unused)]
mod {api_subset_name}_constants {{ include!("{api_subset_name}_constants.rs"); }}
#[allow(clippy::wildcard_imports)]
pub use {api_subset_name}_constants::*;
"#
        )
        .expect("writing to String is infallible");
    }

    let constants_file_path = out_path.join("constants.rs");
    fs::write(&constants_file_path, constants_aggregator)
        .map_err(|source| IoError::with_path(constants_file_path, source))?;
    Ok(())
}

/// Runs the full bindgen pipeline: `generate_base_types` runs first
/// sequentially because API-subset type passes need its derive information
/// (via `BaseDerivesCallback`) and its include set (for `blocklist_file`),
/// then base + API-subset generators run in parallel, then aggregation.
fn bindgen_pipeline(out_path: &Path, config: &Config) -> anyhow::Result<()> {
    let (base_types_path, base_files) =
        info_span!("generate base_types.rs").in_scope(|| generate_base_types(out_path, config))?;

    let derive_map = Arc::new(DerivesMap::from_file(&base_types_path)?);
    let base_files = &base_files;

    let mut api_subset_types: Vec<(String, Vec<String>)> = Vec::new();
    thread::scope(|scope| -> anyhow::Result<()> {
        let mut types_handles: Vec<TypesPassHandle> = Vec::new();
        let mut non_types_handles = Vec::new();

        // Base passes — produce `base_constants.rs`, `base_functions.rs`
        // (`ntddk.rs` for WDM/KMDF, `windows.rs` for UMDF), and `wdf.rs`.
        for (file_name, generate_function) in BASE_BINDGEN_FILE_GENERATORS {
            non_types_handles.push(
                thread::Builder::new()
                    .name(format!("bindgen base: {file_name}"))
                    .spawn_scoped(scope, move || {
                        info_span!(parent: &Span::current(), "base generator", generated_file_name = file_name)
                            .in_scope(|| generate_function(out_path, config))
                    })
                    .expect("Scoped Thread should spawn successfully"),
            );
        }

        // API-subset passes — three per API subset. Types passes return their
        // discovered type names through the join handle so the aggregator
        // can emit selective `pub use` re-exports.
        for (api_subset_name, api_subset) in API_SUBSET_CONFIGS {
            let api_subset = *api_subset;

            // Types pass
            let current_span = Span::current();
            let derive_map = Arc::clone(&derive_map);
            types_handles.push(
                thread::Builder::new()
                    .name(format!("bindgen {api_subset_name} types"))
                    .spawn_scoped(scope, move || {
                        info_span!(parent: &current_span, "worker thread", generated_file_name = format!("{api_subset_name}_types.rs"))
                            .in_scope(|| {
                                let discovered_type_names = generate_api_subset_bindings(
                                    out_path, config, api_subset_name, api_subset,
                                    base_files, &ApiSubsetPass::Types { derive_map },
                                )?;
                                Ok((api_subset_name.to_string(), discovered_type_names))
                            })
                    })
                    .expect("Scoped Thread should spawn successfully"),
            );

            // Constants pass
            let current_span = Span::current();
            non_types_handles.push(
                thread::Builder::new()
                    .name(format!("bindgen {api_subset_name} constants"))
                    .spawn_scoped(scope, move || {
                        info_span!(parent: &current_span, "worker thread", generated_file_name = format!("{api_subset_name}_constants.rs"))
                            .in_scope(|| {
                                generate_api_subset_bindings(
                                    out_path, config, api_subset_name, api_subset,
                                    base_files, &ApiSubsetPass::Constants,
                                )?;
                                Ok(())
                            })
                    })
                    .expect("Scoped Thread should spawn successfully"),
            );

            // Functions pass
            let current_span = Span::current();
            non_types_handles.push(
                thread::Builder::new()
                    .name(format!("bindgen {api_subset_name} functions"))
                    .spawn_scoped(scope, move || {
                        info_span!(parent: &current_span, "worker thread", generated_file_name = format!("{api_subset_name}.rs"))
                            .in_scope(|| {
                                generate_api_subset_bindings(
                                    out_path, config, api_subset_name, api_subset,
                                    base_files, &ApiSubsetPass::Functions,
                                )?;
                                Ok(())
                            })
                    })
                    .expect("Scoped Thread should spawn successfully"),
            );
        }

        join_worker_threads(non_types_handles)?;
        api_subset_types = join_worker_threads(types_handles)?;
        Ok(())
    })?;
    api_subset_types.sort_by(|(a, _), (b, _)| a.cmp(b));

    write_aggregated_types(out_path, &api_subset_types)?;

    let api_subset_names: Vec<String> = api_subset_types.into_iter().map(|(s, _)| s).collect();
    write_aggregated_constants(out_path, &api_subset_names)?;
    Ok(())
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
                    out_path.join("base_types.rs").to_str().expect(
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

/// Starts a task that compiles a C shim to expose WDF symbols hidden by
/// `__declspec(selectany)`.
fn start_wdf_symbol_export_tasks<'scope>(
    thread_scope: &'scope thread::Scope<'scope, '_>,
    out_path: &'scope Path,
    config: &'scope Config,
    thread_join_handles: &mut Vec<thread::ScopedJoinHandle<'scope, anyhow::Result<()>>>,
) {
    let current_span = Span::current();

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
                                    // This should include all enabled API subsets, but is currently blocked by mutually exclusive headers in the cc (C compiler) pass: https://github.com/microsoft/windows-drivers-rs/issues/515
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
                    Ok::<(), anyhow::Error>(())
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
    thread_join_handles: &mut Vec<thread::ScopedJoinHandle<'scope, anyhow::Result<()>>>,
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

fn main() -> anyhow::Result<()> {
    initialize_tracing()?;

    configure_wdk_library_build_and_then(|config| {
        let out_path = PathBuf::from(
            env::var("OUT_DIR").expect("OUT_DIR should be exist in Cargo build environment"),
        );

        thread::scope(|scope| {
            let mut thread_join_handles = Vec::new();

            start_wdf_artifact_tasks(scope, &out_path, &config, &mut thread_join_handles)?;
            info_span!("bindgen pipeline").in_scope(|| bindgen_pipeline(&out_path, &config))?;

            join_worker_threads(thread_join_handles)?;
            Ok::<(), anyhow::Error>(())
        })?;

        Ok::<(), anyhow::Error>(())
    })?;

    Ok(())
}
