// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! A collection of macros that help make it easier to interact with
//! [`wdk_sys`](../wdk_sys/index.html)'s direct bindings to the Windows Driver
//! Kit (WDK).

use std::{collections::BTreeMap, path::PathBuf, str::FromStr};

use fs4::fs_std::FileExt;
use itertools::Itertools;
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote, ToTokens};
use serde::{Deserialize, Serialize};
use syn::{
    parse::{Parse, ParseStream, Parser},
    parse2,
    parse_file,
    parse_quote,
    punctuated::Punctuated,
    AngleBracketedGenericArguments,
    Attribute,
    BareFnArg,
    Error,
    Expr,
    ExprCall,
    File,
    GenericArgument,
    Ident,
    Item,
    ItemType,
    LitStr,
    Path,
    PathArguments,
    PathSegment,
    Result,
    ReturnType,
    Signature,
    Stmt,
    Token,
    Type,
    TypeBareFn,
    TypePath,
};

/// Name of the `bindgen`-generated Rust module that contains the `TableIndex`
/// constants for the `WDF`'s function table
const WDF_FUNC_ENUM_MOD_NAME: &str = "_WDFFUNCENUM";

/// A procedural macro that allows WDF functions to be called by name.
///
/// This macro is only intended to be used in the
/// [`wdk_sys`](../wdk_sys/index.html) crate. Users wanting to call WDF
/// [`wdk_sys`](../wdk_sys/index.html) as an argument to the macro.
/// macro differs from the one in [`wdk_sys`](../wdk_sys/index.html) in that it
/// must pass in the generated types from `wdk-sys` as an argument to the macro.
#[proc_macro]
pub fn call_unsafe_wdf_function_binding(input_tokens: TokenStream) -> TokenStream {
    call_unsafe_wdf_function_binding_impl(TokenStream2::from(input_tokens)).into()
}

/// A trait to provide additional functionality to the [`String`] type
trait StringExt {
    /// Convert a string to `snake_case`
    fn to_snake_case(&self) -> String;
}

/// A trait to provide additional functionality to `std::result::Result`
trait ResultExt<T, E> {
    fn to_syn_result(self, span: Span, error: &str) -> syn::Result<T>;
}

/// Struct storing string representations of the information we want to cache
/// from `types.rs`.
#[derive(Debug, Deserialize, PartialEq, Serialize)]
struct CachedFunctionInfo {
    parameters: String,
    return_type: String,
}

/// Struct storing the input tokens directly parsed from calls to
/// `call_unsafe_wdf_function_binding` macro
#[derive(Debug, PartialEq)]
struct Inputs {
    /// Path to file where generated type information resides.
    types_path: LitStr,
    /// The name of the WDF function to call. This matches the name of the
    /// function in C/C++.
    wdf_function_identifier: Ident,
    /// The arguments to pass to the WDF function. These should match the
    /// function signature of the WDF function.
    wdf_function_arguments: Punctuated<Expr, Token![,]>,
}

/// Struct storing all the AST fragments derived from [`Inputs`]. This
/// represents all the ASTs derived from [`Inputs`]. These ultimately get used
/// in the final generated code.
#[derive(Debug, PartialEq)]
struct DerivedASTFragments {
    function_pointer_type: Ident,
    function_table_index: Ident,
    parameters: Punctuated<BareFnArg, Token![,]>,
    parameter_identifiers: Punctuated<Ident, Token![,]>,
    return_type: ReturnType,
    arguments: Punctuated<Expr, Token![,]>,
    inline_wdf_fn_name: Ident,
}

/// Struct storing the AST fragments that form distinct sections of the final
/// generated code. Each field is derived from [`DerivedASTFragments`].
struct IntermediateOutputASTFragments {
    must_use_attribute: Option<Attribute>,
    inline_wdf_fn_signature: Signature,
    inline_wdf_fn_body_statments: Vec<Stmt>,
    inline_wdf_fn_invocation: ExprCall,
}

/// Struct to represent a file lock guard. This struct enforces RAII, ensuring
/// that the file lock is released when the guard goes out of scope.
struct FileLockGuard {
    file: std::fs::File,
}

impl FileLockGuard {
    fn new(file: std::fs::File, span: Span) -> Result<Self> {
        FileExt::lock_exclusive(&file).to_syn_result(span, "unable to obtain file lock")?;
        Ok(Self { file })
    }
}

impl Drop for FileLockGuard {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

impl StringExt for String {
    fn to_snake_case(&self) -> String {
        // There will be, at max, 2 characters unhandled by the 3-char windows. It is
        // only less than 2 when the string has length less than 2
        const MAX_PADDING_NEEDED: usize = 2;

        let mut snake_case_string = Self::with_capacity(self.len());

        for (current_char, next_char, next_next_char) in self
            .chars()
            .map(Some)
            .chain([None; MAX_PADDING_NEEDED])
            .tuple_windows()
            .filter_map(|(c1, c2, c3)| Some((c1?, c2, c3)))
        {
            // Handle camelCase or PascalCase word boundary (e.g. lC in camelCase)
            if current_char.is_lowercase() && next_char.is_some_and(|c| c.is_ascii_uppercase()) {
                snake_case_string.push(current_char);
                snake_case_string.push('_');
            }
            // Handle UPPERCASE acronym word boundary (e.g. ISt in ASCIIString)
            else if current_char.is_uppercase()
                && next_char.is_some_and(|c| c.is_ascii_uppercase())
                && next_next_char.is_some_and(|c| c.is_ascii_lowercase())
            {
                snake_case_string.push(current_char.to_ascii_lowercase());
                snake_case_string.push('_');
            } else {
                snake_case_string.push(current_char.to_ascii_lowercase());
            }
        }

        snake_case_string
    }
}

impl<T, E: std::error::Error> ResultExt<T, E> for std::result::Result<T, E> {
    fn to_syn_result(self, span: Span, error_description: &str) -> syn::Result<T> {
        self.map_err(|err| Error::new(span, format!("{error_description}, {err}")))
    }
}

impl From<(Punctuated<BareFnArg, Token![,]>, ReturnType)> for CachedFunctionInfo {
    fn from((parameters, return_type): (Punctuated<BareFnArg, Token![,]>, ReturnType)) -> Self {
        Self {
            parameters: parameters.to_token_stream().to_string(),
            return_type: return_type.to_token_stream().to_string(),
        }
    }
}

impl Parse for Inputs {
    fn parse(input: ParseStream) -> Result<Self> {
        let types_path = input.parse::<LitStr>()?;

        input.parse::<Token![,]>()?;
        let c_wdf_function_identifier = input.parse::<Ident>()?;

        // Support WDF apis with no arguments
        if input.is_empty() {
            return Ok(Self {
                types_path,
                wdf_function_identifier: c_wdf_function_identifier,
                wdf_function_arguments: Punctuated::new(),
            });
        }

        input.parse::<Token![,]>()?;
        let wdf_function_arguments = input.parse_terminated(Expr::parse, Token![,])?;

        Ok(Self {
            types_path,
            wdf_function_identifier: c_wdf_function_identifier,
            wdf_function_arguments,
        })
    }
}

impl Inputs {
    fn generate_derived_ast_fragments(self) -> Result<DerivedASTFragments> {
        let function_pointer_type = format_ident!(
            "PFN_{uppercase_c_function_name}",
            uppercase_c_function_name = self.wdf_function_identifier.to_string().to_uppercase(),
            span = self.wdf_function_identifier.span()
        );
        let function_table_index = format_ident!(
            "{wdf_function_identifier}TableIndex",
            wdf_function_identifier = self.wdf_function_identifier,
            span = self.wdf_function_identifier.span()
        );

        let function_name_to_info_map: BTreeMap<String, CachedFunctionInfo> =
            get_wdf_function_info_map(&self.types_path, self.wdf_function_identifier.span())?;
        let function_info = function_name_to_info_map
            .get(&self.wdf_function_identifier.to_string())
            .ok_or_else(|| {
                Error::new(
                    self.wdf_function_identifier.span(),
                    format!(
                        "Failed to find function info for {}",
                        self.wdf_function_identifier
                    ),
                )
            })?;
        let parameters_tokens = TokenStream2::from_str(&function_info.parameters).to_syn_result(
            self.wdf_function_identifier.span(),
            "unable to parse parameter tokens",
        )?;
        let return_type_tokens = TokenStream2::from_str(&function_info.return_type).to_syn_result(
            self.wdf_function_identifier.span(),
            "unable to parse return type tokens",
        )?;
        let parameters =
            Punctuated::<BareFnArg, Token![,]>::parse_terminated.parse2(parameters_tokens)?;
        let return_type = ReturnType::parse.parse2(return_type_tokens)?;

        let parameter_identifiers = parameters
            .iter()
            .cloned()
            .map(|bare_fn_arg| {
                if let Some((identifier, _)) = bare_fn_arg.name {
                    return Ok(identifier);
                }
                Err(Error::new(
                    function_pointer_type.span(),
                    format!("Expected fn parameter to have a name: {bare_fn_arg:#?}"),
                ))
            })
            .collect::<Result<_>>()?;
        let inline_wdf_fn_name = format_ident!(
            "{c_function_name_snake_case}_impl",
            c_function_name_snake_case = self.wdf_function_identifier.to_string().to_snake_case()
        );

        Ok(DerivedASTFragments {
            function_pointer_type,
            function_table_index,
            parameters,
            parameter_identifiers,
            return_type,
            arguments: self.wdf_function_arguments,
            inline_wdf_fn_name,
        })
    }
}

impl DerivedASTFragments {
    fn generate_intermediate_output_ast_fragments(self) -> IntermediateOutputASTFragments {
        let Self {
            function_pointer_type,
            function_table_index,
            parameters,
            parameter_identifiers,
            return_type,
            arguments,
            inline_wdf_fn_name,
        } = self;

        let must_use_attribute = generate_must_use_attribute(&return_type);

        let inline_wdf_fn_signature = parse_quote! {
            unsafe fn #inline_wdf_fn_name(#parameters) #return_type
        };

        let inline_wdf_fn_body_statments = parse_quote! {
            // Get handle to WDF function from the function table
            let wdf_function: wdk_sys::#function_pointer_type = Some(
                // SAFETY: This `transmute` from a no-argument function pointer to a function pointer with the correct
                //         arguments for the WDF function is safe befause WDF maintains the strict mapping between the
                //         function table index and the correct function pointer type.
                unsafe {
                    let wdf_function_table = wdk_sys::WdfFunctions;
                    let wdf_function_count = wdk_sys::wdf::__private::get_wdf_function_count();

                    // SAFETY: This is safe because:
                    //         1. `WdfFunctions` is valid for reads for `{NUM_WDF_FUNCTIONS_PLACEHOLDER}` * `core::mem::size_of::<WDFFUNC>()`
                    //            bytes, and is guaranteed to be aligned and it must be properly aligned.
                    //         2. `WdfFunctions` points to `{NUM_WDF_FUNCTIONS_PLACEHOLDER}` consecutive properly initialized values of
                    //            type `WDFFUNC`.
                    //         3. WDF does not mutate the memory referenced by the returned slice for for its entire `'static' lifetime.
                    //         4. The total size, `{NUM_WDF_FUNCTIONS_PLACEHOLDER}` * `core::mem::size_of::<WDFFUNC>()`, of the slice must be no
                    //            larger than `isize::MAX`. This is proven by the below `const_assert!`.

                    debug_assert!(isize::try_from(wdf_function_count * core::mem::size_of::<wdk_sys::WDFFUNC>()).is_ok());
                    let wdf_function_table = core::slice::from_raw_parts(wdf_function_table, wdf_function_count);

                    core::mem::transmute(
                        // FIXME: investigate why _WDFFUNCENUM does not have a generated type alias without the underscore prefix
                        wdf_function_table[wdk_sys::_WDFFUNCENUM::#function_table_index as usize],
                    )
                }
            );

            // Call the WDF function with the supplied args. This mirrors what happens in the inlined WDF function in
            // the various wdf headers(ex. wdfdriver.h)
            if let Some(wdf_function) = wdf_function {
                // SAFETY: The WDF function pointer is always valid because its an entry in
                // `wdk_sys::WDF_FUNCTION_TABLE` indexed by `table_index` and guarded by the type-safety of
                // `pointer_type`. The passed arguments are also guaranteed to be of a compatible type due to
                // `pointer_type`.
                unsafe {
                    (wdf_function)(
                        wdk_sys::WdfDriverGlobals,
                        #parameter_identifiers
                    )
                }
            } else {
                unreachable!("Option should never be None");
            }
        };

        let inline_wdf_fn_invocation = parse_quote! {
            #inline_wdf_fn_name(#arguments)
        };

        IntermediateOutputASTFragments {
            must_use_attribute,
            inline_wdf_fn_signature,
            inline_wdf_fn_body_statments,
            inline_wdf_fn_invocation,
        }
    }
}

impl IntermediateOutputASTFragments {
    fn assemble_final_output(self) -> TokenStream2 {
        let Self {
            must_use_attribute,
            inline_wdf_fn_signature,
            inline_wdf_fn_body_statments,
            inline_wdf_fn_invocation,
        } = self;

        let conditional_must_use_attribute =
            must_use_attribute.map_or_else(TokenStream2::new, quote::ToTokens::into_token_stream);

        quote! {
            {
                // Use a private module to prevent leaking of glob import into inline_wdf_fn_invocation's parameters
                mod private__ {
                    // Glob import types from wdk_sys. glob importing is done instead of blindly prepending the
                    // paramters types with wdk_sys:: because bindgen generates some paramters as native rust types
                    use wdk_sys::*;

                    // If the function returns a value, add a `#[must_use]` attribute to the function
                    #conditional_must_use_attribute
                    // Encapsulate the code in an inline functions to allow for condition must_use attribute.
                    //  core::hint::must_use is not stable yet: https://github.com/rust-lang/rust/issues/94745
                    #[inline(always)]
                    pub #inline_wdf_fn_signature {
                        #(#inline_wdf_fn_body_statments)*
                    }
                }

                private__::#inline_wdf_fn_invocation
            }
        }
    }
}

fn call_unsafe_wdf_function_binding_impl(input_tokens: TokenStream2) -> TokenStream2 {
    let inputs = match parse2::<Inputs>(input_tokens) {
        Ok(syntax_tree) => syntax_tree,
        Err(err) => return err.to_compile_error(),
    };

    let derived_ast_fragments = match inputs.generate_derived_ast_fragments() {
        Ok(derived_ast_fragments) => derived_ast_fragments,
        Err(err) => return err.to_compile_error(),
    };

    derived_ast_fragments
        .generate_intermediate_output_ast_fragments()
        .assemble_final_output()
}

/// Fetch the function table information from the cache, if
/// it exists. If not, create the cache by reading the
/// `types.rs` file. Returns a `BTreeMap`, where
/// `key` is the function name and `value` is the cached function table
/// information.
///
/// Instead of parsing `types.rs` for relevant data on
/// every macro invocation, all relevant function
/// table information is extracted during the first `proc-macro` invocation and
/// serialized to a location accessible by all proc-macro invocations.
/// Subsequent invocations fetching from the cache significantly reduces
/// compilation time.
fn get_wdf_function_info_map(
    types_path: &LitStr,
    span: Span,
) -> Result<BTreeMap<String, CachedFunctionInfo>> {
    cfg_if::cfg_if! {
        if #[cfg(test)] {
            let scratch_dir = scratch::path(concat!(env!("CARGO_CRATE_NAME"), "_ast_fragments_test"));
        } else {
            let scratch_dir = scratch::path(concat!(env!("CARGO_CRATE_NAME"), "_ast_fragments"));
        }
    }

    let cached_function_info_map_path = scratch_dir.join("cached_function_info_map.json");

    if !cached_function_info_map_path.exists() {
        let flock = std::fs::File::create(scratch_dir.join(".lock"))
            .to_syn_result(span, "unable to create file lock")?;

        // When _flock_guard goes out of scope, the file lock is released
        let _flock_guard = FileLockGuard::new(flock, span)
            .to_syn_result(span, "unable to create file lock guard")?;

        // Before this thread acquires the lock, it's possible that a concurrent thread
        // already created the cache. If so, this thread skips cache generation.
        if !cached_function_info_map_path.exists() {
            let function_info_map = create_wdf_function_info_file_cache(
                types_path,
                cached_function_info_map_path.as_path(),
                span,
            )?;
            return Ok(function_info_map);
        }
    }
    let function_info_map =
        read_wdf_function_info_file_cache(cached_function_info_map_path.as_path(), span)?;
    Ok(function_info_map)
}

/// Reads the cache of function information, then deserializes it into a
/// `BTreeMap`.
fn read_wdf_function_info_file_cache(
    cached_function_info_map_path: &std::path::Path,
    span: Span,
) -> Result<BTreeMap<String, CachedFunctionInfo>> {
    let generated_map_string = std::fs::read_to_string(cached_function_info_map_path)
        .to_syn_result(span, "unable to read cache to string")?;
    let map: BTreeMap<String, CachedFunctionInfo> = serde_json::from_str(&generated_map_string)
        .to_syn_result(span, "unable to parse cache to BTreeMap")?;
    Ok(map)
}

/// Generates the cache of function information, then
/// serializes it into a JSON string and writes it to a designated location.
/// Must obtain an exclusive file lock prior to calling this function to prevent
/// concurrent threads from reading and writing to the same file.
fn create_wdf_function_info_file_cache(
    types_path: &LitStr,
    cached_function_info_map_path: &std::path::Path,
    span: Span,
) -> Result<BTreeMap<String, CachedFunctionInfo>> {
    let generated_map = generate_wdf_function_info_file_cache(types_path, span)?;
    let generated_map_string = serde_json::to_string(&generated_map)
        .to_syn_result(span, "unable to parse cache to JSON string")?;
    std::fs::write(cached_function_info_map_path, generated_map_string)
        .to_syn_result(span, "unable to write cache to file")?;
    Ok(generated_map)
}

/// Parses file from `types_path` to generate a `BTreeMap` of
/// function information, where `key` is the function name and `value` is
/// the cached function table information.
fn generate_wdf_function_info_file_cache(
    types_path: &LitStr,
    span: Span,
) -> Result<BTreeMap<String, CachedFunctionInfo>> {
    let types_ast = parse_types_ast(types_path)?;
    let func_enum_mod = types_ast
        .items
        .iter()
        .find_map(|item| {
            if let Item::Mod(mod_alias) = item {
                if mod_alias.ident == WDF_FUNC_ENUM_MOD_NAME {
                    return Some(mod_alias);
                }
            }
            None
        })
        .ok_or_else(|| {
            Error::new(
                span,
                format!("Failed to find {WDF_FUNC_ENUM_MOD_NAME} module in types.rs file",),
            )
        })?;

    let (_brace, func_enum_mod_contents) = &func_enum_mod.content.as_ref().ok_or_else(|| {
        Error::new(
            span,
            format!("Failed to find {WDF_FUNC_ENUM_MOD_NAME} module contents in types.rs file",),
        )
    })?;

    func_enum_mod_contents
        .iter()
        .filter_map(|item| {
            if let Item::Const(const_alias) = item {
                return const_alias
                    .ident
                    .to_string()
                    .strip_suffix("TableIndex")
                    .and_then(|function_name| {
                        let function_pointer_type = format_ident!(
                            "PFN_{uppercase_c_function_name}",
                            uppercase_c_function_name = function_name.to_uppercase(),
                            span = span
                        );
                        generate_cached_function_info(&types_ast, &function_pointer_type)
                            .transpose()
                            .map(|generate_cached_function_info_result| {
                                generate_cached_function_info_result.map(|cached_function_info| {
                                    (function_name.to_string(), cached_function_info)
                                })
                            })
                    });
            }
            None
        })
        .collect()
}

fn parse_types_ast(path: &LitStr) -> Result<File> {
    let types_path = PathBuf::from(path.value());
    let types_path = match types_path.canonicalize() {
        Ok(types_path) => types_path,
        Err(err) => {
            return Err(Error::new(
                path.span(),
                format!(
                    "Failed to canonicalize types_path ({}): {err}",
                    types_path.display()
                ),
            ));
        }
    };

    let types_file_contents = match std::fs::read_to_string(&types_path) {
        Ok(contents) => contents,
        Err(err) => {
            return Err(Error::new(
                path.span(),
                format!(
                    "Failed to read wdk-sys types information from {}: {err}",
                    types_path.display(),
                ),
            ));
        }
    };

    match parse_file(&types_file_contents) {
        Ok(wdk_sys_types_rs_abstract_syntax_tree) => Ok(wdk_sys_types_rs_abstract_syntax_tree),
        Err(err) => Err(Error::new(
            path.span(),
            format!(
                "Failed to parse wdk-sys types information from {} into AST: {err}",
                types_path.display(),
            ),
        )),
    }
}

/// Generate the function parameters and return type corresponding to the
/// function signature of the `function_pointer_type` type alias found in
/// bindgen-generated types information
///
/// # Examples
///
/// Passing the `PFN_WDFDRIVERCREATE` [`Ident`] as `function_pointer_type` would
/// return a [`Punctuated`] representation of
///
/// ```rust, compile_fail
/// DriverObject: PDRIVER_OBJECT,
/// RegistryPath: PCUNICODE_STRING,
/// DriverAttributes: WDF_OBJECT_ATTRIBUTES,
/// DriverConfig: PWDF_DRIVER_CONFIG,
/// Driver: *mut WDFDRIVER
/// ```
///
/// and return type as the [`ReturnType`] representation of `wdk_sys::NTSTATUS`
fn generate_cached_function_info(
    types_ast: &File,
    function_pointer_type: &Ident,
) -> Result<Option<CachedFunctionInfo>> {
    match find_type_alias_definition(types_ast, function_pointer_type) {
        Ok(type_alias_definition) => {
            let fn_pointer_definition =
                extract_fn_pointer_definition(type_alias_definition, function_pointer_type.span())?;
            Ok(Some(
                parse_fn_pointer_definition(fn_pointer_definition, function_pointer_type.span())?
                    .into(),
            ))
        }
        // `types.rs` includes only a subset of types listed in _WDFFUNCENUM. Therefore, not finding
        // a type alias definition is expected behavior.
        Err(_err) => Ok(None),
    }
}

/// Find type alias declaration and definition that matches the Ident of
/// `function_pointer_type` in `syn::File` AST
///
/// # Examples
///
/// Passing the `PFN_WDFDRIVERCREATE` [`Ident`] as `function_pointer_type` would
/// return a [`ItemType`] representation of:
///
/// ```rust, compile_fail
/// pub type PFN_WDFDRIVERCREATE = ::core::option::Option<
///     unsafe extern "C" fn(
///         DriverGlobals: PWDF_DRIVER_GLOBALS,
///         DriverObject: PDRIVER_OBJECT,
///         RegistryPath: PCUNICODE_STRING,
///         DriverAttributes: PWDF_OBJECT_ATTRIBUTES,
///         DriverConfig: PWDF_DRIVER_CONFIG,
///         Driver: *mut WDFDRIVER,
///     ) -> NTSTATUS,
/// >;
/// ```
fn find_type_alias_definition<'a>(
    types_ast: &'a File,
    function_pointer_type: &Ident,
) -> Result<&'a ItemType> {
    types_ast
        .items
        .iter()
        .find_map(|item| {
            if let Item::Type(type_alias) = item {
                if type_alias.ident == *function_pointer_type {
                    return Some(type_alias);
                }
            }
            None
        })
        .ok_or_else(|| {
            Error::new(
                function_pointer_type.span(),
                format!("Failed to find type alias definition for {function_pointer_type}"),
            )
        })
}

/// Extract the [`TypePath`] representing the function pointer definition from
/// the [`ItemType`]
///
/// # Examples
///
/// The [`ItemType`] representation of
///
/// ```rust, compile_fail
/// pub type PFN_WDFDRIVERCREATE = ::core::option::Option<
///     unsafe extern "C" fn(
///         DriverGlobals: PWDF_DRIVER_GLOBALS,
///         DriverObject: PDRIVER_OBJECT,
///         RegistryPath: PCUNICODE_STRING,
///         DriverAttributes: PWDF_OBJECT_ATTRIBUTES,
///         DriverConfig: PWDF_DRIVER_CONFIG,
///         Driver: *mut WDFDRIVER,
///     ) -> NTSTATUS,
/// >;
/// ```
///
/// would return the [`TypePath`] representation of
///
/// ```rust, compile_fail
/// ::core::option::Option<
///     unsafe extern "C" fn(
///         DriverGlobals: PWDF_DRIVER_GLOBALS,
///         DriverObject: PDRIVER_OBJECT,
///         RegistryPath: PCUNICODE_STRING,
///         DriverAttributes: PWDF_OBJECT_ATTRIBUTES,
///         DriverConfig: PWDF_DRIVER_CONFIG,
///         Driver: *mut WDFDRIVER,
///     ) -> NTSTATUS,
/// >
/// ```
fn extract_fn_pointer_definition(type_alias: &ItemType, error_span: Span) -> Result<&TypePath> {
    if let Type::Path(fn_pointer) = type_alias.ty.as_ref() {
        Ok(fn_pointer)
    } else {
        Err(Error::new(
            error_span,
            format!("Expected Type::Path when parsing  ItemType.ty:\n{type_alias:#?}"),
        ))
    }
}

/// Parse the parameter list (both names and types) and the return type from the
/// [`TypePath`] representing the function pointer definition
///
/// # Examples
///
/// The [`TypePath`] representation of
///
/// ```rust, compile_fail
/// ::core::option::Option<
///     unsafe extern "C" fn(
///         DriverGlobals: PWDF_DRIVER_GLOBALS,
///         DriverObject: PDRIVER_OBJECT,
///         RegistryPath: PCUNICODE_STRING,
///         DriverAttributes: PWDF_OBJECT_ATTRIBUTES,
///         DriverConfig: PWDF_DRIVER_CONFIG,
///         Driver: *mut WDFDRIVER,
///     ) -> NTSTATUS,
/// >
/// ```
///
/// would return the parsed parameter list as the [`Punctuated`] representation
/// of
///
/// ```rust, compile_fail
/// DriverObject: PDRIVER_OBJECT,
/// RegistryPath: PCUNICODE_STRING,
/// DriverAttributes: WDF_OBJECT_ATTRIBUTES,
/// DriverConfig: PWDF_DRIVER_CONFIG,
/// Driver: *mut WDFDRIVER
/// ```
///
/// and return type as the [`ReturnType`] representation of `wdk_sys::NTSTATUS`
fn parse_fn_pointer_definition(
    fn_pointer_typepath: &TypePath,
    error_span: Span,
) -> Result<(Punctuated<BareFnArg, Token![,]>, ReturnType)> {
    let bare_fn_type = extract_bare_fn_type(fn_pointer_typepath, error_span)?;
    let fn_parameters = compute_fn_parameters(bare_fn_type, error_span)?;
    let return_type = compute_return_type(bare_fn_type);

    Ok((fn_parameters, return_type))
}

/// Extract the [`TypeBareFn`] (i.e. function definition) from the [`TypePath`]
/// (i.e. the function pointer option) representing the function
///
/// # Examples
///
/// The [`TypePath`] representation of
///
/// ```rust, compile_fail
/// ::core::option::Option<
///     unsafe extern "C" fn(
///         DriverGlobals: PWDF_DRIVER_GLOBALS,
///         DriverObject: PDRIVER_OBJECT,
///         RegistryPath: PCUNICODE_STRING,
///         DriverAttributes: PWDF_OBJECT_ATTRIBUTES,
///         DriverConfig: PWDF_DRIVER_CONFIG,
///         Driver: *mut WDFDRIVER,
///     ) -> NTSTATUS,
/// >
/// ```
///
/// would return the [`TypeBareFn`] representation of
///
/// ```rust, compile_fail
/// unsafe extern "C" fn(
///     DriverGlobals: PWDF_DRIVER_GLOBALS,
///     DriverObject: PDRIVER_OBJECT,
///     RegistryPath: PCUNICODE_STRING,
///     DriverAttributes: PWDF_OBJECT_ATTRIBUTES,
///     DriverConfig: PWDF_DRIVER_CONFIG,
///     Driver: *mut WDFDRIVER,
/// ) -> NTSTATUS,
/// ```
fn extract_bare_fn_type(fn_pointer_typepath: &TypePath, error_span: Span) -> Result<&TypeBareFn> {
    let option_path_segment: &PathSegment =
        fn_pointer_typepath.path.segments.last().ok_or_else(|| {
            Error::new(
                error_span,
                format!("Expected at least one PathSegment in TypePath:\n{fn_pointer_typepath:#?}"),
            )
        })?;
    if option_path_segment.ident != "Option" {
        return Err(Error::new(
            error_span,
            format!("Expected Option as last PathSegment in TypePath:\n{fn_pointer_typepath:#?}"),
        ));
    }
    let PathArguments::AngleBracketed(AngleBracketedGenericArguments {
        args: ref option_angle_bracketed_args,
        ..
    }) = option_path_segment.arguments
    else {
        return Err(Error::new(
            error_span,
            format!(
                "Expected AngleBracketed PathArguments in Option \
                 PathSegment:\n{option_path_segment:#?}"
            ),
        ));
    };
    let bracketed_argument = option_angle_bracketed_args.first().ok_or_else(|| {
        Error::new(
            error_span,
            format!(
                "Expected exactly one GenericArgument in AngleBracketedGenericArguments:\n{:#?}",
                option_path_segment.arguments
            ),
        )
    })?;
    let GenericArgument::Type(Type::BareFn(bare_fn_type)) = bracketed_argument else {
        return Err(Error::new(
            error_span,
            format!("Expected TypeBareFn in GenericArgument:\n{bracketed_argument:#?}"),
        ));
    };
    Ok(bare_fn_type)
}

/// Compute the function parameters based on the function definition
///
/// # Examples
///
/// The [`TypeBareFn`] representation of
///
/// ```rust, compile_fail
/// unsafe extern "C" fn(
///     DriverGlobals: PWDF_DRIVER_GLOBALS,
///     DriverObject: PDRIVER_OBJECT,
///     RegistryPath: PCUNICODE_STRING,
///     DriverAttributes: PWDF_OBJECT_ATTRIBUTES,
///     DriverConfig: PWDF_DRIVER_CONFIG,
///     Driver: *mut WDFDRIVER,
/// ) -> NTSTATUS,
/// ```
///
/// would return the [`Punctuated`] representation of
/// ```rust, compile_fail
/// DriverObject: PDRIVER_OBJECT,
/// RegistryPath: PCUNICODE_STRING,
/// DriverAttributes: WDF_OBJECT_ATTRIBUTES,
/// DriverConfig: PWDF_DRIVER_CONFIG,
/// Driver: *mut WDFDRIVER
/// ```
fn compute_fn_parameters(
    bare_fn_type: &syn::TypeBareFn,
    error_span: Span,
) -> Result<Punctuated<BareFnArg, Token![,]>> {
    // Validate that the first parameter is PWDF_DRIVER_GLOBALS
    let Some(BareFnArg {
        ty:
            Type::Path(TypePath {
                path:
                    Path {
                        segments: first_parameter_type_path,
                        ..
                    },
                ..
            }),
        ..
    }) = bare_fn_type.inputs.first()
    else {
        return Err(Error::new(
            error_span,
            format!(
                "Expected at least one input parameter of type Path in \
                 BareFnType:\n{bare_fn_type:#?}"
            ),
        ));
    };
    let Some(last_path_segment) = first_parameter_type_path.last() else {
        return Err(Error::new(
            error_span,
            format!("Expected at least one PathSegment in TypePath:\n{bare_fn_type:#?}"),
        ));
    };
    if last_path_segment.ident != "PWDF_DRIVER_GLOBALS" {
        return Err(Error::new(
            error_span,
            format!(
                "Expected PWDF_DRIVER_GLOBALS as last PathSegment in TypePath of first BareFnArg \
                 input:\n{bare_fn_type:#?}"
            ),
        ));
    }

    Ok(bare_fn_type
        .inputs
        .iter()
        .skip(1)
        // transform argument names to snake_case with trailing underscores to lessen likelihood
        // of shadowing issues
        .map(|fn_arg| {
            let arg_name = fn_arg.name.as_ref().map(|(ident, colon_token)| {
                let modified_name = {
                    let mut name = ident.to_string().to_snake_case();
                    name.push_str("__");
                    name
                };
                (Ident::new(&modified_name, ident.span()), *colon_token)
            });

            BareFnArg {
                name: arg_name,
                ..fn_arg.clone()
            }
        })
        .collect())
}

/// Compute the return type based on the function defintion
///
/// # Examples
///
/// The [`TypeBareFn`] representation of
///
/// ```rust, compile_fail
/// unsafe extern "C" fn(
///     DriverGlobals: PWDF_DRIVER_GLOBALS,
///     DriverObject: PDRIVER_OBJECT,
///     RegistryPath: PCUNICODE_STRING,
///     DriverAttributes: PWDF_OBJECT_ATTRIBUTES,
///     DriverConfig: PWDF_DRIVER_CONFIG,
///     Driver: *mut WDFDRIVER,
/// ) -> NTSTATUS,
/// ```
///
/// would return the [`ReturnType`] representation of `wdk_sys::NTSTATUS`
fn compute_return_type(bare_fn_type: &syn::TypeBareFn) -> ReturnType {
    bare_fn_type.output.clone()
}

/// Generate the `#[must_use]` attribute if the return type is not `()`
fn generate_must_use_attribute(return_type: &ReturnType) -> Option<Attribute> {
    if matches!(return_type, ReturnType::Type(..)) {
        Some(parse_quote! { #[must_use] })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::sync::LazyLock;

    use pretty_assertions::assert_eq as pretty_assert_eq;
    use quote::ToTokens;

    use super::*;

    static SCRATCH_DIR: LazyLock<PathBuf> =
        LazyLock::new(|| scratch::path(concat!(env!("CARGO_CRATE_NAME"), "_ast_fragments_test")));
    const CACHE_FILE_NAME: &str = "cached_function_info_map.json";

    fn with_file_lock_clean_env<F>(f: F)
    where
        F: FnOnce(),
    {
        let test_flock: std::fs::File =
            std::fs::File::create(SCRATCH_DIR.join("test.lock")).unwrap();
        FileExt::lock_exclusive(&test_flock).unwrap();

        let cached_function_info_map_path = SCRATCH_DIR.join(CACHE_FILE_NAME);

        pretty_assert_eq!(
            cached_function_info_map_path.exists(),
            false,
            "could not remove file {}",
            cached_function_info_map_path.display()
        );

        f();

        if cached_function_info_map_path.exists() {
            std::fs::remove_file(cached_function_info_map_path).unwrap();
        }

        FileExt::unlock(&test_flock).unwrap();
    }

    mod to_snake_case {
        use super::*;

        #[test]
        fn camel_case() {
            let input = "camelCaseString".to_string();
            let expected = "camel_case_string";

            pretty_assert_eq!(input.to_snake_case(), expected);
        }

        #[test]
        fn short_camel_case() {
            let input = "aB".to_string();
            let expected = "a_b";

            pretty_assert_eq!(input.to_snake_case(), expected);
        }

        #[test]
        fn pascal_case() {
            let input = "PascalCaseString".to_string();
            let expected = "pascal_case_string";

            pretty_assert_eq!(input.to_snake_case(), expected);
        }

        #[test]
        fn pascal_case_with_leading_acronym() {
            let input = "ASCIIEncodedString".to_string();
            let expected = "ascii_encoded_string";

            pretty_assert_eq!(input.to_snake_case(), expected);
        }

        #[test]
        fn pascal_case_with_trailing_acronym() {
            let input = "IsASCII".to_string();
            let expected = "is_ascii";

            pretty_assert_eq!(input.to_snake_case(), expected);
        }

        #[test]
        fn screaming_snake_case() {
            let input = "PFN_WDF_DRIVER_DEVICE_ADD".to_string();
            let expected = "pfn_wdf_driver_device_add";

            pretty_assert_eq!(input.to_snake_case(), expected);
        }

        #[test]
        fn screaming_snake_case_with_leading_acronym() {
            let input = "ASCII_STRING".to_string();
            let expected = "ascii_string";

            pretty_assert_eq!(input.to_snake_case(), expected);
        }

        #[test]
        fn screaming_snake_case_with_leading_underscore() {
            let input = "_WDF_DRIVER_INIT_FLAGS".to_string();
            let expected = "_wdf_driver_init_flags";

            pretty_assert_eq!(input.to_snake_case(), expected);
        }

        #[test]
        fn snake_case() {
            let input = "snake_case_string".to_string();
            let expected = "snake_case_string";

            pretty_assert_eq!(input.to_snake_case(), expected);
        }

        #[test]
        fn snake_case_with_leading_underscore() {
            let input = "_snake_case_with_leading_underscore".to_string();
            let expected = "_snake_case_with_leading_underscore";

            pretty_assert_eq!(input.to_snake_case(), expected);
        }
    }

    mod inputs {
        use super::*;

        mod parse {
            use super::*;

            #[test]
            fn valid_input() {
                let input_tokens = quote! { "/path/to/generated/types/file.rs", WdfDriverCreate, driver, registry_path, WDF_NO_OBJECT_ATTRIBUTES, &mut driver_config, driver_handle_output };
                let expected = Inputs {
                    types_path: parse_quote! { "/path/to/generated/types/file.rs" },
                    wdf_function_identifier: format_ident!("WdfDriverCreate"),
                    wdf_function_arguments: parse_quote! {
                        driver,
                        registry_path,
                        WDF_NO_OBJECT_ATTRIBUTES,
                        &mut driver_config,
                        driver_handle_output
                    },
                };

                pretty_assert_eq!(parse2::<Inputs>(input_tokens).unwrap(), expected);
            }

            #[test]
            fn valid_input_with_trailing_comma() {
                let input_tokens = quote! { "/path/to/generated/types/file.rs" , WdfDriverCreate, driver, registry_path, WDF_NO_OBJECT_ATTRIBUTES, &mut driver_config, driver_handle_output, };
                let expected = Inputs {
                    types_path: parse_quote! { "/path/to/generated/types/file.rs" },
                    wdf_function_identifier: format_ident!("WdfDriverCreate"),
                    wdf_function_arguments: parse_quote! {
                        driver,
                        registry_path,
                        WDF_NO_OBJECT_ATTRIBUTES,
                        &mut driver_config,
                        driver_handle_output,
                    },
                };

                pretty_assert_eq!(parse2::<Inputs>(input_tokens).unwrap(), expected);
            }

            #[test]
            fn wdf_function_with_no_arguments() {
                let input_tokens =
                    quote! { "/path/to/generated/types/file.rs", WdfVerifierDbgBreakPoint };
                let expected = Inputs {
                    types_path: parse_quote! { "/path/to/generated/types/file.rs" },
                    wdf_function_identifier: format_ident!("WdfVerifierDbgBreakPoint"),
                    wdf_function_arguments: Punctuated::new(),
                };

                pretty_assert_eq!(parse2::<Inputs>(input_tokens).unwrap(), expected);
            }

            #[test]
            fn wdf_function_with_no_arguments_and_trailing_comma() {
                let input_tokens =
                    quote! { "/path/to/generated/types/file.rs", WdfVerifierDbgBreakPoint, };
                let expected = Inputs {
                    types_path: parse_quote! { "/path/to/generated/types/file.rs" },
                    wdf_function_identifier: format_ident!("WdfVerifierDbgBreakPoint"),
                    wdf_function_arguments: Punctuated::new(),
                };

                pretty_assert_eq!(parse2::<Inputs>(input_tokens).unwrap(), expected);
            }

            #[test]
            fn invalid_ident() {
                let input_tokens = quote! { "/path/to/generated/types/file.rs", 23InvalidIdent, driver, registry_path, WDF_NO_OBJECT_ATTRIBUTES, &mut driver_config, driver_handle_output, };
                let expected = Error::new(Span::call_site(), "expected identifier");

                pretty_assert_eq!(
                    parse2::<Inputs>(input_tokens).unwrap_err().to_string(),
                    expected.to_string()
                );
            }
        }

        mod generate_derived_ast_fragments {
            use super::*;

            #[test]
            fn valid_input() {
                with_file_lock_clean_env(|| {
                    let inputs = Inputs {
                        types_path: parse_quote! { "tests/unit-tests-input/generated-types.rs" },
                        wdf_function_identifier: format_ident!("WdfDriverCreate"),
                        wdf_function_arguments: parse_quote! {
                            driver,
                            registry_path,
                            WDF_NO_OBJECT_ATTRIBUTES,
                            &mut driver_config,
                            driver_handle_output,
                        },
                    };
                    let expected = DerivedASTFragments {
                        function_pointer_type: format_ident!("PFN_WDFDRIVERCREATE"),
                        function_table_index: format_ident!("WdfDriverCreateTableIndex"),
                        parameters: parse_quote! {
                            driver_object__: PDRIVER_OBJECT,
                            registry_path__: PCUNICODE_STRING,
                            driver_attributes__: PWDF_OBJECT_ATTRIBUTES,
                            driver_config__: PWDF_DRIVER_CONFIG,
                            driver__: *mut WDFDRIVER
                        },
                        parameter_identifiers: parse_quote! {
                            driver_object__,
                            registry_path__,
                            driver_attributes__,
                            driver_config__,
                            driver__
                        },
                        return_type: parse_quote! { -> NTSTATUS },
                        arguments: parse_quote! {
                            driver,
                            registry_path,
                            WDF_NO_OBJECT_ATTRIBUTES,
                            &mut driver_config,
                            driver_handle_output,
                        },
                        inline_wdf_fn_name: format_ident!("wdf_driver_create_impl"),
                    };

                    pretty_assert_eq!(inputs.generate_derived_ast_fragments().unwrap(), expected);
                });
            }

            #[test]
            fn valid_input_with_no_arguments() {
                with_file_lock_clean_env(|| {
                    let inputs = Inputs {
                        types_path: parse_quote! { "tests/unit-tests-input/generated-types.rs" },
                        wdf_function_identifier: format_ident!("WdfVerifierDbgBreakPoint"),
                        wdf_function_arguments: Punctuated::new(),
                    };
                    let expected = DerivedASTFragments {
                        function_pointer_type: format_ident!("PFN_WDFVERIFIERDBGBREAKPOINT"),
                        function_table_index: format_ident!("WdfVerifierDbgBreakPointTableIndex"),
                        parameters: Punctuated::new(),
                        parameter_identifiers: Punctuated::new(),
                        return_type: ReturnType::Default,
                        arguments: Punctuated::new(),
                        inline_wdf_fn_name: format_ident!("wdf_verifier_dbg_break_point_impl"),
                    };

                    pretty_assert_eq!(inputs.generate_derived_ast_fragments().unwrap(), expected);
                });
            }
        }
    }

    mod get_wdf_function_info_map {
        use super::*;

        #[test]
        fn valid_input_no_cache() {
            with_file_lock_clean_env(|| {
                let inputs = Inputs {
                    types_path: parse_quote! { "tests/unit-tests-input/generated-types.rs" },
                    wdf_function_identifier: format_ident!("WdfVerifierDbgBreakPoint"),
                    wdf_function_arguments: Punctuated::new(),
                };

                let mut expected: BTreeMap<String, CachedFunctionInfo> = BTreeMap::new();
                expected.insert(
                    "WdfDriverCreate".into(),
                    CachedFunctionInfo {
                        parameters: "driver_object__ : PDRIVER_OBJECT , registry_path__ : \
                                     PCUNICODE_STRING , driver_attributes__ : \
                                     PWDF_OBJECT_ATTRIBUTES , driver_config__ : \
                                     PWDF_DRIVER_CONFIG , driver__ : * mut WDFDRIVER"
                            .into(),
                        return_type: "-> NTSTATUS".into(),
                    },
                );

                expected.insert(
                    "WdfVerifierDbgBreakPoint".into(),
                    CachedFunctionInfo {
                        parameters: String::new(),
                        return_type: String::new(),
                    },
                );
                pretty_assert_eq!(
                    get_wdf_function_info_map(
                        &inputs.types_path,
                        inputs.wdf_function_identifier.span()
                    )
                    .unwrap(),
                    expected
                );

                pretty_assert_eq!(SCRATCH_DIR.join(CACHE_FILE_NAME).exists(), true);
            });
        }

        #[test]
        fn valid_input_cache_exists() {
            with_file_lock_clean_env(|| {
                let inputs = Inputs {
                    types_path: parse_quote! { "tests/unit-tests-input/generated-types.rs" },
                    wdf_function_identifier: format_ident!("WdfVerifierDbgBreakPoint"),
                    wdf_function_arguments: Punctuated::new(),
                };
                // create cache with first call to get_wdf_function_info_map

                get_wdf_function_info_map(
                    &inputs.types_path,
                    inputs.wdf_function_identifier.span(),
                )
                .unwrap();

                // make sure cache exists
                pretty_assert_eq!(SCRATCH_DIR.join(CACHE_FILE_NAME).exists(), true);

                let mut expected: BTreeMap<String, CachedFunctionInfo> = BTreeMap::new();
                expected.insert(
                    "WdfDriverCreate".into(),
                    CachedFunctionInfo {
                        parameters: "driver_object__ : PDRIVER_OBJECT , registry_path__ : \
                                     PCUNICODE_STRING , driver_attributes__ : \
                                     PWDF_OBJECT_ATTRIBUTES , driver_config__ : \
                                     PWDF_DRIVER_CONFIG , driver__ : * mut WDFDRIVER"
                            .into(),
                        return_type: "-> NTSTATUS".into(),
                    },
                );

                expected.insert(
                    "WdfVerifierDbgBreakPoint".into(),
                    CachedFunctionInfo {
                        parameters: String::new(),
                        return_type: String::new(),
                    },
                );
                pretty_assert_eq!(
                    get_wdf_function_info_map(
                        &inputs.types_path,
                        inputs.wdf_function_identifier.span()
                    )
                    .unwrap(),
                    expected
                );
            });
        }
    }

    mod generate_wdf_function_info_file_cache {
        use super::*;

        #[test]
        fn valid_input() {
            let inputs = Inputs {
                types_path: parse_quote! { "tests/unit-tests-input/generated-types.rs" },
                wdf_function_identifier: format_ident!("WdfVerifierDbgBreakPoint"),
                wdf_function_arguments: Punctuated::new(),
            };

            let mut expected: BTreeMap<String, CachedFunctionInfo> = BTreeMap::new();
            expected.insert(
                "WdfDriverCreate".into(),
                CachedFunctionInfo {
                    parameters: "driver_object__ : PDRIVER_OBJECT , registry_path__ : \
                                 PCUNICODE_STRING , driver_attributes__ : PWDF_OBJECT_ATTRIBUTES \
                                 , driver_config__ : PWDF_DRIVER_CONFIG , driver__ : * mut \
                                 WDFDRIVER"
                        .into(),
                    return_type: "-> NTSTATUS".into(),
                },
            );

            expected.insert(
                "WdfVerifierDbgBreakPoint".into(),
                CachedFunctionInfo {
                    parameters: String::new(),
                    return_type: String::new(),
                },
            );

            pretty_assert_eq!(
                generate_wdf_function_info_file_cache(
                    &inputs.types_path,
                    inputs.wdf_function_identifier.span()
                )
                .unwrap(),
                expected
            );
        }

        #[test]
        fn invalid_input_missing_wdf_func_enum() {
            let inputs = Inputs {
                types_path: parse_quote! { "tests/unit-tests-input/missing-wdf-func-enum.rs" },
                wdf_function_identifier: format_ident!("WdfVerifierDbgBreakPoint"),
                wdf_function_arguments: Punctuated::new(),
            };

            let expected = Error::new(
                Span::call_site(),
                "Failed to find _WDFFUNCENUM module in types.rs file",
            );

            pretty_assert_eq!(
                generate_wdf_function_info_file_cache(
                    &inputs.types_path,
                    inputs.wdf_function_identifier.span()
                )
                .unwrap_err()
                .to_string(),
                expected.to_string()
            );
        }

        #[test]
        fn invalid_input_missing_wdf_func_enum_contents() {
            let inputs = Inputs {
                types_path: parse_quote! { "tests/unit-tests-input/missing-wdf-func-enum-contents.rs" },
                wdf_function_identifier: format_ident!("WdfVerifierDbgBreakPoint"),
                wdf_function_arguments: Punctuated::new(),
            };

            let expected = Error::new(
                Span::call_site(),
                "Failed to find _WDFFUNCENUM module contents in types.rs file",
            );

            pretty_assert_eq!(
                generate_wdf_function_info_file_cache(
                    &inputs.types_path,
                    inputs.wdf_function_identifier.span()
                )
                .unwrap_err()
                .to_string(),
                expected.to_string()
            );
        }
    }

    mod generate_cached_function_info {
        use super::*;

        #[test]
        fn valid_input() {
            // This is a snippet of a bindgen-generated file containing types information
            // used by tests for [`wdk_macros::call_unsafe_wdf_function_binding!`]
            let types_ast = parse_quote! {
                pub type PFN_WDFIOQUEUEPURGESYNCHRONOUSLY = ::core::option::Option<
                    unsafe extern "C" fn(DriverGlobals: PWDF_DRIVER_GLOBALS, Queue: WDFQUEUE),
                >;
            };
            let function_pointer_type = format_ident!("PFN_WDFIOQUEUEPURGESYNCHRONOUSLY");
            let expected: Option<CachedFunctionInfo> = Some(
                (
                    parse_quote! {
                        queue__: WDFQUEUE
                    },
                    ReturnType::Default,
                )
                    .into(),
            );

            pretty_assert_eq!(
                generate_cached_function_info(&types_ast, &function_pointer_type).unwrap(),
                expected
            );
        }
    }

    mod find_type_alias_definition {
        use super::*;

        #[test]
        fn valid_input() {
            // This is a snippet of a bindgen-generated file containing types information
            // used by tests for [`wdk_macros::call_unsafe_wdf_function_binding!`]
            let types_ast = parse_quote! {
                pub type WDF_DRIVER_GLOBALS = _WDF_DRIVER_GLOBALS;
                pub type PWDF_DRIVER_GLOBALS = *mut _WDF_DRIVER_GLOBALS;
                pub mod _WDFFUNCENUM {
                    pub type Type = ::core::ffi::c_int;
                    pub const WdfChildListCreateTableIndex: Type = 0;
                    pub const WdfChildListGetDeviceTableIndex: Type = 1;
                    pub const WdfChildListRetrievePdoTableIndex: Type = 2;
                    pub const WdfChildListRetrieveAddressDescriptionTableIndex: Type = 3;
                    pub const WdfChildListBeginScanTableIndex: Type = 4;
                    pub const WdfChildListEndScanTableIndex: Type = 5;
                    pub const WdfChildListBeginIterationTableIndex: Type = 6;
                    pub const WdfChildListRetrieveNextDeviceTableIndex: Type = 7;
                    pub const WdfChildListEndIterationTableIndex: Type = 8;
                    pub const WdfChildListAddOrUpdateChildDescriptionAsPresentTableIndex: Type = 9;
                    pub const WdfChildListUpdateChildDescriptionAsMissingTableIndex: Type = 10;
                    pub const WdfChildListUpdateAllChildDescriptionsAsPresentTableIndex: Type = 11;
                    pub const WdfChildListRequestChildEjectTableIndex: Type = 12;
                }
                pub type PFN_WDFGETTRIAGEINFO = ::core::option::Option<
                    unsafe extern "C" fn(DriverGlobals: PWDF_DRIVER_GLOBALS) -> PVOID,
                >;
            };
            let function_pointer_type = format_ident!("PFN_WDFGETTRIAGEINFO");
            let expected = parse_quote! {
                pub type PFN_WDFGETTRIAGEINFO = ::core::option::Option<
                    unsafe extern "C" fn(DriverGlobals: PWDF_DRIVER_GLOBALS) -> PVOID,
                >;
            };

            pretty_assert_eq!(
                find_type_alias_definition(&types_ast, &function_pointer_type).unwrap(),
                &expected
            );
        }
    }

    mod extract_fn_pointer_definition {
        use super::*;

        #[test]
        fn valid_input() {
            let fn_type_alias = parse_quote! {
                pub type PFN_WDFDRIVERCREATE = ::core::option::Option<
                    unsafe extern "C" fn(
                        DriverGlobals: PWDF_DRIVER_GLOBALS,
                        DriverObject: PDRIVER_OBJECT,
                        RegistryPath: PCUNICODE_STRING,
                        DriverAttributes: PWDF_OBJECT_ATTRIBUTES,
                        DriverConfig: PWDF_DRIVER_CONFIG,
                        Driver: *mut WDFDRIVER,
                    ) -> NTSTATUS
                >;
            };
            let expected = parse_quote! {
                ::core::option::Option<
                    unsafe extern "C" fn(
                        DriverGlobals: PWDF_DRIVER_GLOBALS,
                        DriverObject: PDRIVER_OBJECT,
                        RegistryPath: PCUNICODE_STRING,
                        DriverAttributes: PWDF_OBJECT_ATTRIBUTES,
                        DriverConfig: PWDF_DRIVER_CONFIG,
                        Driver: *mut WDFDRIVER,
                    ) -> NTSTATUS
                >
            };

            pretty_assert_eq!(
                extract_fn_pointer_definition(&fn_type_alias, Span::call_site()).unwrap(),
                &expected
            );
        }

        #[test]
        fn valid_input_with_no_arguments() {
            let fn_type_alias = parse_quote! {
                pub type PFN_WDFVERIFIERDBGBREAKPOINT = ::core::option::Option<unsafe extern "C" fn(DriverGlobals: PWDF_DRIVER_GLOBALS)>;
            };
            let expected = parse_quote! {
                ::core::option::Option<unsafe extern "C" fn(DriverGlobals: PWDF_DRIVER_GLOBALS)>
            };

            pretty_assert_eq!(
                extract_fn_pointer_definition(&fn_type_alias, Span::call_site()).unwrap(),
                &expected
            );
        }
    }

    mod parse_fn_pointer_definition {
        use super::*;

        #[test]
        fn valid_input() {
            // WdfDriverCreate has the following generated signature:
            let fn_pointer_typepath = parse_quote! {
                ::core::option::Option<unsafe extern "C" fn(
                    DriverGlobals: PWDF_DRIVER_GLOBALS,
                    DriverObject: PDRIVER_OBJECT,
                    RegistryPath: PCUNICODE_STRING,
                    DriverAttributes: PWDF_OBJECT_ATTRIBUTES,
                    DriverConfig: PWDF_DRIVER_CONFIG,
                    Driver: *mut WDFDRIVER,
                ) -> NTSTATUS>
            };
            let expected = (
                parse_quote! {
                    driver_object__: PDRIVER_OBJECT,
                    registry_path__: PCUNICODE_STRING,
                    driver_attributes__: PWDF_OBJECT_ATTRIBUTES,
                    driver_config__: PWDF_DRIVER_CONFIG,
                    driver__: *mut WDFDRIVER
                },
                ReturnType::Type(
                    Token![->](Span::call_site()),
                    Box::new(Type::Path(parse_quote! { NTSTATUS })),
                ),
            );

            pretty_assert_eq!(
                parse_fn_pointer_definition(&fn_pointer_typepath, Span::call_site()).unwrap(),
                expected
            );
        }

        #[test]
        fn valid_input_with_no_arguments() {
            // WdfVerifierDbgBreakPoint has the following generated signature:
            let fn_pointer_typepath = parse_quote! {
                ::core::option::Option<unsafe extern "C" fn(DriverGlobals: PWDF_DRIVER_GLOBALS)>
            };
            let expected = (Punctuated::new(), ReturnType::Default);

            pretty_assert_eq!(
                parse_fn_pointer_definition(&fn_pointer_typepath, Span::call_site()).unwrap(),
                expected
            );
        }
    }

    mod extract_bare_fn_type {
        use super::*;

        #[test]
        fn valid_input() {
            // WdfDriverCreate has the following generated signature:
            let fn_pointer_typepath = parse_quote! {
                ::core::option::Option<
                    unsafe extern "C" fn(
                        DriverGlobals: PWDF_DRIVER_GLOBALS,
                        DriverObject: PDRIVER_OBJECT,
                        RegistryPath: PCUNICODE_STRING,
                        DriverAttributes: PWDF_OBJECT_ATTRIBUTES,
                        DriverConfig: PWDF_DRIVER_CONFIG,
                        Driver: *mut WDFDRIVER,
                    ) -> NTSTATUS,
                >
            };
            let expected: TypeBareFn = parse_quote! {
                unsafe extern "C" fn(
                    DriverGlobals: PWDF_DRIVER_GLOBALS,
                    DriverObject: PDRIVER_OBJECT,
                    RegistryPath: PCUNICODE_STRING,
                    DriverAttributes: PWDF_OBJECT_ATTRIBUTES,
                    DriverConfig: PWDF_DRIVER_CONFIG,
                    Driver: *mut WDFDRIVER,
                ) -> NTSTATUS
            };

            pretty_assert_eq!(
                extract_bare_fn_type(&fn_pointer_typepath, Span::call_site()).unwrap(),
                &expected
            );
        }
    }

    mod compute_fn_parameters {
        use super::*;

        #[test]
        fn valid_input() {
            // WdfDriverCreate has the following generated signature:
            let bare_fn_type = parse_quote! {
                unsafe extern "C" fn(
                    DriverGlobals: PWDF_DRIVER_GLOBALS,
                    DriverObject: PDRIVER_OBJECT,
                    RegistryPath: PCUNICODE_STRING,
                    DriverAttributes: PWDF_OBJECT_ATTRIBUTES,
                    DriverConfig: PWDF_DRIVER_CONFIG,
                    Driver: *mut WDFDRIVER,
                ) -> NTSTATUS
            };
            let expected = parse_quote! {
                driver_object__: PDRIVER_OBJECT,
                registry_path__: PCUNICODE_STRING,
                driver_attributes__: PWDF_OBJECT_ATTRIBUTES,
                driver_config__: PWDF_DRIVER_CONFIG,
                driver__: *mut WDFDRIVER
            };

            pretty_assert_eq!(
                compute_fn_parameters(&bare_fn_type, Span::call_site()).unwrap(),
                expected
            );
        }

        #[test]
        fn valid_input_with_no_arguments() {
            // WdfVerifierDbgBreakPoint has the following generated signature:
            let bare_fn_type = parse_quote! {
                unsafe extern "C" fn(DriverGlobals: PWDF_DRIVER_GLOBALS)
            };
            let expected = Punctuated::new();

            pretty_assert_eq!(
                compute_fn_parameters(&bare_fn_type, Span::call_site()).unwrap(),
                expected
            );
        }
    }

    mod compute_return_type {
        use super::*;

        #[test]
        fn ntstatus() {
            // WdfDriverCreate has the following generated signature:
            let bare_fn_type = parse_quote! {
                unsafe extern "C" fn(
                    DriverGlobals: PWDF_DRIVER_GLOBALS,
                    DriverObject: PDRIVER_OBJECT,
                    RegistryPath: PCUNICODE_STRING,
                    DriverAttributes: PWDF_OBJECT_ATTRIBUTES,
                    DriverConfig: PWDF_DRIVER_CONFIG,
                    Driver: *mut WDFDRIVER,
                ) -> NTSTATUS
            };
            let expected = ReturnType::Type(
                Token![->](Span::call_site()),
                Box::new(Type::Path(parse_quote! { NTSTATUS })),
            );

            pretty_assert_eq!(compute_return_type(&bare_fn_type), expected);
        }

        #[test]
        fn unit() {
            // WdfSpinLockAcquire has the following generated signature:
            let bare_fn_type = parse_quote! {
                unsafe extern "C" fn(
                    DriverGlobals: PWDF_DRIVER_GLOBALS,
                    SpinLock: WDFSPINLOCK
                )
            };
            let expected = ReturnType::Default;

            pretty_assert_eq!(compute_return_type(&bare_fn_type), expected);
        }
    }

    mod generate_must_use_attribute {
        use super::*;

        #[test]
        fn unit_return_type() {
            let return_type = ReturnType::Default;
            let generated_must_use_attribute_tokens = generate_must_use_attribute(&return_type);

            pretty_assert_eq!(generated_must_use_attribute_tokens, None);
        }

        #[test]
        fn ntstatus_return_type() {
            let return_type: ReturnType = parse_quote! { -> NTSTATUS };
            let expected_tokens = quote! { #[must_use] };
            let generated_must_use_attribute_tokens = generate_must_use_attribute(&return_type);

            pretty_assert_eq!(
                generated_must_use_attribute_tokens
                    .unwrap()
                    .into_token_stream()
                    .to_string(),
                expected_tokens.to_string(),
            );
        }
    }
}
