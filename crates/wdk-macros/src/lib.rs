// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! A collection of macros that help make it easier to interact with
//! [`wdk-sys`]'s direct bindings to the Windows Driver Kit (WDK).

use std::{
    io::{BufReader, Read},
    path::PathBuf,
    process::{Command, Stdio},
};

use cargo_metadata::{Message, MetadataCommand, PackageId};
use itertools::Itertools;
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
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
    TypePtr,
};

/// A procedural macro that allows WDF functions to be called by name.
///
/// This function parses the name of the WDF function, finds it function pointer
/// from the WDF function table, and then calls it with the arguments passed to
/// it
///
/// # Safety
/// Function arguments must abide by any rules outlined in the WDF
/// documentation. This macro does not perform any validation of the arguments
/// passed to it., beyond type validation.
///
/// # Examples
///
/// ```rust, no_run
/// use wdk_sys::*;
///
/// #[export_name = "DriverEntry"]
/// pub extern "system" fn driver_entry(
///     driver: &mut DRIVER_OBJECT,
///     registry_path: PCUNICODE_STRING,
/// ) -> NTSTATUS {
///     let mut driver_config = WDF_DRIVER_CONFIG {
///         Size: core::mem::size_of::<WDF_DRIVER_CONFIG>() as ULONG,
///         ..WDF_DRIVER_CONFIG::default()
///     };
///     let driver_handle_output = WDF_NO_HANDLE as *mut WDFDRIVER;
///
///     unsafe {
///         wdk_macros::call_unsafe_wdf_function_binding!(
///             WdfDriverCreate,
///             driver as PDRIVER_OBJECT,
///             registry_path,
///             WDF_NO_OBJECT_ATTRIBUTES,
///             &mut driver_config,
///             driver_handle_output,
///         )
///     }
/// }
/// ```
#[allow(clippy::unnecessary_safety_doc)]
#[proc_macro]
pub fn call_unsafe_wdf_function_binding(input_tokens: TokenStream) -> TokenStream {
    call_unsafe_wdf_function_binding_impl(TokenStream2::from(input_tokens)).into()
}

/// A trait to provide additional functionality to the `String` type
trait StringExt {
    /// Convert a string to `snake_case`
    fn to_snake_case(&self) -> String;
}

/// Struct storing the input tokens directly parsed from calls to
/// `call_unsafe_wdf_function_binding` macro
#[derive(Debug, PartialEq)]
struct Inputs {
    /// The name of the WDF function to call. This matches the name of the
    /// function in C/C++.
    wdf_function_identifier: Ident,
    /// The arguments to pass to the WDF function. These should match the
    /// function signature of the WDF function.
    wdf_function_arguments: Punctuated<Expr, Token![,]>,
}

/// Struct storing all the AST fragments derived from `Inputs`. This represents
/// all the derived ASTs depend on `Inputs` that ultimately get used in the
/// final generated code that.
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
/// generated code. These sections are derived from `DerivedASTFragments`.
struct IntermediateOutputASTFragments {
    must_use_attribute: Option<Attribute>,
    inline_wdf_fn_signature: Signature,
    inline_wdf_fn_body_statments: Vec<Stmt>,
    inline_wdf_fn_invocation: ExprCall,
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

impl Parse for Inputs {
    fn parse(input: ParseStream) -> Result<Self> {
        let c_wdf_function_identifier = input.parse::<Ident>()?;

        // Support WDF apis with no arguments
        if input.is_empty() {
            return Ok(Self {
                wdf_function_identifier: c_wdf_function_identifier,
                wdf_function_arguments: Punctuated::new(),
            });
        }

        input.parse::<Token![,]>()?;
        let wdf_function_arguments = input.parse_terminated(Expr::parse, Token![,])?;

        Ok(Self {
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
        let (parameters, return_type) =
            generate_parameters_and_return_type(&function_pointer_type)?;
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
                    core::mem::transmute(
                        // FIXME: investigate why _WDFFUNCENUM does not have a generated type alias without the underscore prefix
                        wdk_sys::WDF_FUNCTION_TABLE[wdk_sys::_WDFFUNCENUM::#function_table_index as usize],
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
                #conditional_must_use_attribute
                #[inline(always)]
                #inline_wdf_fn_signature {
                    #(#inline_wdf_fn_body_statments)*
                }

                #inline_wdf_fn_invocation
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

/// Generate the function parameters and return type corresponding to the
/// function signature of the `function_pointer_type` type alias in the AST for
/// types.rs
///
/// # Examples
///
/// Passing the `PFN_WDFDRIVERCREATE` [`Ident`] as `function_pointer_type` would
/// return a [`Punctuated`] representation of
///
/// ```rust, compile_fail
/// DriverObject: wdk_sys::PDRIVER_OBJECT,
/// RegistryPath: wdk_sys::PCUNICODE_STRING,
/// DriverAttributes: wdk_sys::WDF_OBJECT_ATTRIBUTES,
/// DriverConfig: wdk_sys::PWDF_DRIVER_CONFIG,
/// Driver: *mut wdk_sys::WDFDRIVER
/// ```
///
/// and return type as the [`ReturnType`] representation of `wdk_sys::NTSTATUS`
fn generate_parameters_and_return_type(
    function_pointer_type: &Ident,
) -> Result<(Punctuated<BareFnArg, Token![,]>, ReturnType)> {
    let types_rs_ast = get_type_rs_ast()?;
    let type_alias_definition = find_type_alias_definition(&types_rs_ast, function_pointer_type)?;
    let fn_pointer_definition =
        extract_fn_pointer_definition(type_alias_definition, function_pointer_type.span())?;
    parse_fn_pointer_definition(fn_pointer_definition, function_pointer_type.span())
}

/// Finds the `types.rs` file generated by `wdk-sys` and parses it into an AST
fn get_type_rs_ast() -> Result<File> {
    let types_rs_path = find_wdk_sys_out_dir()?.join("types.rs");
    let types_rs_contents = match std::fs::read_to_string(&types_rs_path) {
        Ok(contents) => contents,
        Err(err) => {
            return Err(Error::new(
                Span::call_site(),
                format!(
                    "Failed to read wdk-sys types.rs at {}: {}",
                    types_rs_path.display(),
                    err
                ),
            ));
        }
    };

    match parse_file(&types_rs_contents) {
        Ok(wdk_sys_types_rs_abstract_syntax_tree) => Ok(wdk_sys_types_rs_abstract_syntax_tree),
        Err(err) => Err(Error::new(
            Span::call_site(),
            format!(
                "Failed to parse wdk-sys types.rs into AST at {}: {}",
                types_rs_path.display(),
                err
            ),
        )),
    }
}

/// Find the `OUT_DIR` of wdk-sys crate by running `cargo check` with
/// `--message-format=json` and parsing its output using [`cargo_metadata`]
fn find_wdk_sys_out_dir() -> Result<PathBuf> {
    let scratch_path = scratch::path(env!("CARGO_PKG_NAME"));
    let mut cargo_check_process_handle = match Command::new("cargo")
        .args([
            "check",
            "--message-format=json",
            "--package",
            "wdk-sys",
            // must have a seperate target directory to prevent deadlock from cargo holding a
            // file lock on build output directory since this proc_macro causes
            // cargo build to invoke cargo check
            "--target-dir",
            scratch_path
                .as_os_str()
                .to_str()
                .expect("scratch::path should be valid UTF-8"),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(process) => process,
        Err(err) => {
            return Err(Error::new(
                Span::call_site(),
                format!("Failed to start cargo check process successfully: {err}"),
            ));
        }
    };

    let wdk_sys_pkg_id = find_wdk_sys_pkg_id()?;
    let wdk_sys_out_dir = cargo_metadata::Message::parse_stream(BufReader::new(
        cargo_check_process_handle
            .stdout
            .take()
            .expect("cargo check process should have valid stdout handle"),
    ))
    .filter_map(|message| {
        if let Ok(Message::BuildScriptExecuted(build_script_message)) = message {
            if build_script_message.package_id == wdk_sys_pkg_id {
                return Some(build_script_message.out_dir);
            }
        }
        None
    })
    .collect::<Vec<_>>();
    let wdk_sys_out_dir = match wdk_sys_out_dir.len() {
        1 => &wdk_sys_out_dir[0],
        _ => {
            return Err(Error::new(
                Span::call_site(),
                format!(
                    "Expected exactly one instance of wdk-sys in dependency graph when running \
                     `cargo check`, found {}",
                    wdk_sys_out_dir.len()
                ),
            ));
        }
    };
    match cargo_check_process_handle.wait() {
        Ok(exit_status) => {
            if !exit_status.success() {
                let mut stderr_output = String::new();
                BufReader::new(
                    cargo_check_process_handle
                        .stderr
                        .take()
                        .expect("cargo check process should have valid stderr handle"),
                )
                .read_to_string(&mut stderr_output)
                .expect("cargo check process' stderr should be valid UTF-8");
                return Err(Error::new(
                    Span::call_site(),
                    format!(
                        "cargo check failed to execute to get OUT_DIR for wdk-sys: \
                         \n{stderr_output}"
                    ),
                ));
            }
        }
        Err(err) => {
            return Err(Error::new(
                Span::call_site(),
                format!("cargo check process handle should sucessfully be waited on: {err}"),
            ));
        }
    }

    Ok(wdk_sys_out_dir.to_owned().into())
}

/// find wdk-sys `package_id`. WDR places a limitation that only one instance of
/// wdk-sys is allowed in the dependency graph
fn find_wdk_sys_pkg_id() -> Result<PackageId> {
    let cargo_metadata_packages_list = match MetadataCommand::new().exec() {
        Ok(metadata) => metadata.packages,
        Err(err) => {
            return Err(Error::new(
                Span::call_site(),
                format!("cargo metadata failed to run successfully: {err}"),
            ));
        }
    };
    let wdk_sys_package_matches = cargo_metadata_packages_list
        .iter()
        .filter(|package| package.name == "wdk-sys")
        .collect::<Vec<_>>();

    if wdk_sys_package_matches.len() != 1 {
        return Err(Error::new(
            Span::call_site(),
            format!(
                "Expected exactly one instance of wdk-sys in dependency graph when running `cargo \
                 metadata`, found {}",
                wdk_sys_package_matches.len()
            ),
        ));
    }
    Ok(wdk_sys_package_matches[0].id.clone())
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
    file_ast: &'a File,
    function_pointer_type: &Ident,
) -> Result<&'a ItemType> {
    file_ast
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
/// DriverObject: wdk_sys::PDRIVER_OBJECT,
/// RegistryPath: wdk_sys::PCUNICODE_STRING,
/// DriverAttributes: wdk_sys::WDF_OBJECT_ATTRIBUTES,
/// DriverConfig: wdk_sys::PWDF_DRIVER_CONFIG,
/// Driver: *mut wdk_sys::WDFDRIVER
/// ```
///
/// and return type as the [`ReturnType`] representation of `wdk_sys::NTSTATUS`
fn parse_fn_pointer_definition(
    fn_pointer_typepath: &TypePath,
    error_span: Span,
) -> Result<(Punctuated<BareFnArg, Token![,]>, ReturnType)> {
    let bare_fn_type = extract_bare_fn_type(fn_pointer_typepath, error_span)?;
    let fn_parameters = compute_fn_parameters(bare_fn_type, error_span)?;
    let return_type = compute_return_type(bare_fn_type, error_span)?;

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

/// Compute the function parameters based on the function definition. Prepends
/// `wdk_sys::` to the parameter types
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
/// DriverObject: wdk_sys::PDRIVER_OBJECT,
/// RegistryPath: wdk_sys::PCUNICODE_STRING,
/// DriverAttributes: wdk_sys::WDF_OBJECT_ATTRIBUTES,
/// DriverConfig: wdk_sys::PWDF_DRIVER_CONFIG,
/// Driver: *mut wdk_sys::WDFDRIVER
/// ```
fn compute_fn_parameters(
    bare_fn_type: &syn::TypeBareFn,
    error_span: Span,
) -> Result<Punctuated<BareFnArg, Token![,]>> {
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

    // discard the PWDF_DRIVER_GLOBALS parameter and prepend wdk_sys to the rest of
    // the parameters
    let parameters = bare_fn_type
        .inputs
        .iter()
        .skip(1)
        .cloned()
        .map(|mut bare_fn_arg| {
            let parameter_type_path_segments: &mut Punctuated<PathSegment, syn::token::PathSep> =
                match &mut bare_fn_arg.ty {
                    Type::Path(TypePath {
                        path:
                            Path {
                                ref mut segments, ..
                            },
                        ..
                    }) => segments,

                    Type::Ptr(TypePtr { elem: ty, .. }) => {
                        let Type::Path(TypePath {
                            path:
                                Path {
                                    ref mut segments, ..
                                },
                            ..
                        }) = **ty
                        else {
                            return Err(Error::new(
                                error_span,
                                format!(
                                    "Failed to parse PathSegments out of TypePtr function \
                                     parameter:\n{bare_fn_arg:#?}"
                                ),
                            ));
                        };
                        segments
                    }

                    _ => {
                        return Err(Error::new(
                            error_span,
                            format!(
                                "Unexpected Type encountered when parsing function \
                                 parameter:\n{bare_fn_arg:#?}",
                            ),
                        ));
                    }
                };

            parameter_type_path_segments
                .insert(0, syn::PathSegment::from(format_ident!("wdk_sys")));
            Ok(bare_fn_arg)
        })
        .collect::<Result<_>>()?;

    Ok(parameters)
}

/// Compute the return type based on the function defintion. Prepends the return
/// type with `wdk_sys::`
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
fn compute_return_type(bare_fn_type: &syn::TypeBareFn, error_span: Span) -> Result<ReturnType> {
    let return_type = match &bare_fn_type.output {
        ReturnType::Default => ReturnType::Default,
        ReturnType::Type(right_arrow_token, ty) => ReturnType::Type(
            *right_arrow_token,
            Box::new(Type::Path(TypePath {
                qself: None,
                path: Path {
                    leading_colon: None,
                    segments: {
                        // prepend wdk_sys to existing TypePath
                        let Type::Path(TypePath {
                            path: Path { ref segments, .. },
                            ..
                        }) = **ty
                        else {
                            return Err(Error::new(
                                error_span,
                                format!("Failed to parse ReturnType TypePath:\n{ty:#?}"),
                            ));
                        };
                        let mut segments = segments.clone();
                        segments.insert(
                            0,
                            PathSegment {
                                ident: format_ident!("wdk_sys"),
                                arguments: PathArguments::None,
                            },
                        );
                        segments
                    },
                },
            })),
        ),
    };
    Ok(return_type)
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
    use pretty_assertions::assert_eq as pretty_assert_eq;
    use quote::ToTokens;

    use super::*;

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
                let input_tokens = quote! { WdfDriverCreate, driver, registry_path, WDF_NO_OBJECT_ATTRIBUTES, &mut driver_config, driver_handle_output };
                let expected = Inputs {
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
                let input_tokens = quote! { WdfDriverCreate, driver, registry_path, WDF_NO_OBJECT_ATTRIBUTES, &mut driver_config, driver_handle_output, };
                let expected = Inputs {
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
                let input_tokens = quote! { WdfVerifierDbgBreakPoint };
                let expected = Inputs {
                    wdf_function_identifier: format_ident!("WdfVerifierDbgBreakPoint"),
                    wdf_function_arguments: Punctuated::new(),
                };

                pretty_assert_eq!(parse2::<Inputs>(input_tokens).unwrap(), expected);
            }

            #[test]
            fn wdf_function_with_no_arguments_and_trailing_comma() {
                let input_tokens = quote! { WdfVerifierDbgBreakPoint, };
                let expected = Inputs {
                    wdf_function_identifier: format_ident!("WdfVerifierDbgBreakPoint"),
                    wdf_function_arguments: Punctuated::new(),
                };

                pretty_assert_eq!(parse2::<Inputs>(input_tokens).unwrap(), expected);
            }

            #[test]
            fn invalid_ident() {
                let input_tokens = quote! { 123InvalidIdent, driver, registry_path, WDF_NO_OBJECT_ATTRIBUTES, &mut driver_config, driver_handle_output, };
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
                let inputs = Inputs {
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
                        DriverObject: wdk_sys::PDRIVER_OBJECT,
                        RegistryPath: wdk_sys::PCUNICODE_STRING,
                        DriverAttributes: wdk_sys::PWDF_OBJECT_ATTRIBUTES,
                        DriverConfig: wdk_sys::PWDF_DRIVER_CONFIG,
                        Driver: *mut wdk_sys::WDFDRIVER
                    },
                    parameter_identifiers: parse_quote! {
                        DriverObject,
                        RegistryPath,
                        DriverAttributes,
                        DriverConfig,
                        Driver
                    },
                    return_type: parse_quote! { -> wdk_sys::NTSTATUS },
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
            }

            #[test]
            fn valid_input_with_no_arguments() {
                let inputs = Inputs {
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
            }
        }
    }

    mod generate_parameters_and_return_type {
        use super::*;

        #[test]
        fn valid_input() {
            let function_pointer_type = format_ident!("PFN_WDFIOQUEUEPURGESYNCHRONOUSLY");
            let expected = (
                parse_quote! {
                    Queue: wdk_sys::WDFQUEUE
                },
                ReturnType::Default,
            );

            pretty_assert_eq!(
                generate_parameters_and_return_type(&function_pointer_type).unwrap(),
                expected
            );
        }
    }

    mod find_type_alias_definition {
        use super::*;

        #[test]
        fn valid_input() {
            // This is just a snippet of a generated types.rs file
            let types_rs_ast = parse_quote! {
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
                find_type_alias_definition(&types_rs_ast, &function_pointer_type).unwrap(),
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
                    DriverObject: wdk_sys::PDRIVER_OBJECT,
                    RegistryPath: wdk_sys::PCUNICODE_STRING,
                    DriverAttributes: wdk_sys::PWDF_OBJECT_ATTRIBUTES,
                    DriverConfig: wdk_sys::PWDF_DRIVER_CONFIG,
                    Driver: *mut wdk_sys::WDFDRIVER
                },
                ReturnType::Type(
                    Token![->](Span::call_site()),
                    Box::new(Type::Path(parse_quote! { wdk_sys::NTSTATUS })),
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
                DriverObject: wdk_sys::PDRIVER_OBJECT,
                RegistryPath: wdk_sys::PCUNICODE_STRING,
                DriverAttributes: wdk_sys::PWDF_OBJECT_ATTRIBUTES,
                DriverConfig: wdk_sys::PWDF_DRIVER_CONFIG,
                Driver: *mut wdk_sys::WDFDRIVER
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
                Box::new(Type::Path(parse_quote! { wdk_sys::NTSTATUS })),
            );

            pretty_assert_eq!(
                compute_return_type(&bare_fn_type, Span::call_site()).unwrap(),
                expected
            );
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

            pretty_assert_eq!(
                compute_return_type(&bare_fn_type, Span::call_site()).unwrap(),
                expected
            );
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
