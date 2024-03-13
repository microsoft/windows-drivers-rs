// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! A collection of macros that help make it easier to interact with
//! [`wdk-sys`]'s direct bindings to the Windows Driver Kit (WDK).
#![deny(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::cargo)]
#![deny(clippy::multiple_unsafe_ops_per_block)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![deny(clippy::unnecessary_safety_doc)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![deny(rustdoc::missing_crate_level_docs)]
#![deny(rustdoc::invalid_codeblock_attributes)]
#![deny(rustdoc::invalid_html_tags)]
#![deny(rustdoc::invalid_rust_codeblocks)]
#![deny(rustdoc::bare_urls)]
#![deny(rustdoc::unescaped_backticks)]
#![deny(rustdoc::redundant_explicit_links)]

use std::{
    io::{BufReader, Read},
    path::PathBuf,
    process::{Command, Stdio},
};

use cargo_metadata::{Message, MetadataCommand, PackageId};
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    parse2,
    parse_file,
    punctuated::Punctuated,
    spanned::Spanned,
    AngleBracketedGenericArguments,
    BareFnArg,
    Error,
    Expr,
    File,
    GenericArgument,
    Ident,
    Item,
    ItemType,
    Path,
    PathArguments,
    PathSegment,
    ReturnType,
    Token,
    Type,
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

// TODO: for every place where an error can be returned, forward a span that
// makes sense
struct CallUnsafeWDFFunctionParseOutputs {
    function_pointer_type: Ident,
    function_table_index: Ident,
    parameters: Punctuated<BareFnArg, Token![,]>,
    return_type: ReturnType,
    arguments: Punctuated<Expr, Token![,]>,
    inline_fn_impl_name: Ident,
}

impl Parse for CallUnsafeWDFFunctionParseOutputs {
    fn parse(input: ParseStream) -> Result<Self, Error> {
        // parse inputs
        let c_function_identifier = input.parse::<Ident>()?;
        input.parse::<Token![,]>()?;
        let arguments = input.parse_terminated(Expr::parse, Token![,])?;

        // compute parse outputs
        let function_pointer_type = format_ident!(
            "PFN_{uppercase_c_function_name}",
            uppercase_c_function_name = c_function_identifier.to_string().to_uppercase(),
            span = c_function_identifier.span()
        );
        let function_table_index = format_ident!("{c_function_identifier}TableIndex");
        let (parameters, return_type) =
            compute_parse_outputs_from_generated_code(&function_pointer_type)?;
        let inline_fn_impl_name = format_ident!(
            "{c_function_name_snake_case}_impl",
            c_function_name_snake_case = {
                // convert c_function_name to snake case
                let c_function_name = c_function_identifier.to_string();
                let mut snake_case_name = String::with_capacity(c_function_name.len());
                for (i, char) in c_function_name.chars().enumerate() {
                    if char.is_uppercase() {
                        if i != 0 {
                            snake_case_name.push('_');
                        }
                        snake_case_name.push_str(char.to_lowercase().collect::<String>().as_str());
                    } else {
                        snake_case_name.push(char);
                    }
                }
                snake_case_name
            }
        );

        Ok(Self {
            function_pointer_type,
            function_table_index,
            parameters,
            return_type,
            arguments,
            inline_fn_impl_name,
        })
    }
}

fn call_unsafe_wdf_function_binding_impl(input_tokens: TokenStream2) -> TokenStream2 {
    let parse_outputs = match parse2::<CallUnsafeWDFFunctionParseOutputs>(input_tokens) {
        Ok(syntax_tree) => syntax_tree,
        Err(err) => return err.to_compile_error(),
    };

    let must_use_attribute = if matches!(parse_outputs.return_type, ReturnType::Type(..)) {
        quote! { #[must_use] }
    } else {
        TokenStream2::new()
    };

    let wdf_function_call_tokens = generate_wdf_function_call_tokens(&parse_outputs);

    quote! {
        {
            #must_use_attribute
            #wdf_function_call_tokens
        }
    }
}

/// Compute the function parameters and return type corresponding to the
/// function signature of the `function_pointer_type` type alias in the AST for
/// types.rs
fn compute_parse_outputs_from_generated_code(
    function_pointer_type: &Ident,
) -> Result<(Punctuated<BareFnArg, Token![,]>, ReturnType), Error> {
    let types_rs_ast = get_abstract_syntax_tree_from_types_rs()?;
    let type_alias_definition = find_type_alias_definition(&types_rs_ast, function_pointer_type)?;
    let fn_pointer_definition = extract_fn_pointer_definition(type_alias_definition)?;
    compute_parse_outputs_from_fn_pointer_definition(fn_pointer_definition)
}

fn get_abstract_syntax_tree_from_types_rs() -> Result<File, Error> {
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

fn find_wdk_sys_out_dir() -> Result<PathBuf, Error> {
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
            "target/wdk-macros-target",
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
                    "Expected exactly one instance of wdk-sys in dependency graph, found {}",
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
fn find_wdk_sys_pkg_id() -> Result<PackageId, Error> {
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
                "Expected exactly one instance of wdk-sys in dependency graph, found {}",
                wdk_sys_package_matches.len()
            ),
        ));
    }
    Ok(wdk_sys_package_matches[0].id.clone())
}

/// Find type alias definition that matches the Ident of `function_pointer_type`
/// in `syn::File` AST
///
/// For example, passing the `PFN_WDFDRIVERCREATE` [`Ident`] as
/// `function_pointer_type` would return a [`ItemType`] representation of:
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
) -> Result<&'a ItemType, Error> {
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

fn extract_fn_pointer_definition(type_alias: &ItemType) -> Result<&TypePath, Error> {
    if let Type::Path(fn_pointer) = type_alias.ty.as_ref() {
        Ok(fn_pointer)
    } else {
        Err(Error::new(type_alias.ty.span(), "Expected Type::Path"))
    }
}

fn compute_parse_outputs_from_fn_pointer_definition(
    fn_pointer_typepath: &TypePath,
) -> Result<(Punctuated<BareFnArg, Token![,]>, ReturnType), Error> {
    let bare_fn_type = extract_bare_fn_type(fn_pointer_typepath)?;
    let fn_parameters = compute_fn_parameters(bare_fn_type)?;
    let return_type = compute_return_type(bare_fn_type)?;

    Ok((fn_parameters, return_type))
}

fn extract_bare_fn_type(fn_pointer_typepath: &TypePath) -> Result<&syn::TypeBareFn, Error> {
    let option_path_segment = fn_pointer_typepath.path.segments.last().ok_or_else(|| {
        Error::new(
            fn_pointer_typepath.path.segments.span(),
            "Expected PathSegments",
        )
    })?;
    if option_path_segment.ident != "Option" {
        return Err(Error::new(
            option_path_segment.ident.span(),
            "Expected Option",
        ));
    }
    let PathArguments::AngleBracketed(AngleBracketedGenericArguments {
        args: ref option_angle_bracketed_args,
        ..
    }) = option_path_segment.arguments
    else {
        return Err(Error::new(
            option_path_segment.arguments.span(),
            "Expected AngleBracketed PathArguments",
        ));
    };
    let bracketed_argument = option_angle_bracketed_args.first().ok_or_else(|| {
        Error::new(
            option_angle_bracketed_args.span(),
            "Expected exactly one generic argument",
        )
    })?;
    let GenericArgument::Type(Type::BareFn(bare_fn_type)) = bracketed_argument else {
        return Err(Error::new(bracketed_argument.span(), "Expected TypeBareFn"));
    };
    Ok(bare_fn_type)
}

fn compute_fn_parameters(
    bare_fn_type: &syn::TypeBareFn,
) -> Result<Punctuated<BareFnArg, Token![,]>, Error> {
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
            bare_fn_type.inputs.span(),
            "Expected at least one input parameter",
        ));
    };
    let Some(last_path_segment) = first_parameter_type_path.last() else {
        return Err(Error::new(
            first_parameter_type_path.span(),
            "Expected at least one segment in type path",
        ));
    };
    if last_path_segment.ident != "PWDF_DRIVER_GLOBALS" {
        return Err(Error::new(
            last_path_segment.ident.span(),
            "Expected PWDF_DRIVER_GLOBALS",
        ));
    }

    // drop PWDF_DRIVER_GLOBALS parameter and prepend wdk_sys to the rest of the
    // parameters
    let parameters = bare_fn_type
        .inputs
        .iter()
        .skip(1)
        .cloned()
        .map(|mut bare_fn_arg| {
            let parameter_type_path_segments = match &mut bare_fn_arg.ty {
                Type::Path(TypePath {
                    path: Path {
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
                            ty.span(),
                            "Failed to parse TypePath from TypePtr",
                        ));
                    };
                    segments
                }

                _ => {
                    return Err(Error::new(
                        bare_fn_arg.ty.span(),
                        format!(
                            "Unepected Type encountered when parsing: {:#?}",
                            bare_fn_arg.ty
                        ),
                    ));
                }
            };

            parameter_type_path_segments.insert(
                0,
                syn::PathSegment::from(Ident::new("wdk_sys", Span::call_site())),
            );
            Ok(bare_fn_arg)
        })
        .collect::<Result<_, Error>>()?;

    Ok(parameters)
}

fn compute_return_type(bare_fn_type: &syn::TypeBareFn) -> Result<ReturnType, Error> {
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
                                ty.span(),
                                "Failed to parse ReturnType TypePath",
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

fn generate_wdf_function_call_tokens(
    parse_outputs: &CallUnsafeWDFFunctionParseOutputs,
) -> TokenStream2 {
    let CallUnsafeWDFFunctionParseOutputs {
        function_pointer_type,
        function_table_index,
        parameters,
        return_type,
        arguments,
        inline_fn_impl_name,
    } = parse_outputs;

    let parameter_identifiers = match parameters
        .iter()
        .cloned()
        .map(|bare_fn_arg| {
            if let Some((identifier, _)) = bare_fn_arg.name {
                return Ok(identifier);
            }
            Err(Error::new(
                bare_fn_arg.span(),
                "Expected parameter to have a name",
            ))
        })
        .collect::<Result<Punctuated<Ident, Token![,]>, Error>>()
    {
        Ok(identifiers) => identifiers,
        Err(err) => return err.to_compile_error(),
    };

    quote! {
        #[inline(always)]
        unsafe fn #inline_fn_impl_name(#parameters) #return_type {
            // Get handle to WDF function from the function table
            let wdf_function: wdk_sys::#function_pointer_type = Some(
                // SAFETY: This `transmute` from a no-argument function pointer to a function pointer with the correct
                //         arguments for the WDF function is safe befause WDF maintains the strict mapping between the
                //         function table index and the correct function pointer type.
                #[allow(unused_unsafe)]
                #[allow(clippy::multiple_unsafe_ops_per_block)]
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
                #[allow(unused_unsafe)]
                #[allow(clippy::multiple_unsafe_ops_per_block)]
                unsafe {
                    (wdf_function)(
                        wdk_sys::WdfDriverGlobals,
                        #parameter_identifiers
                    )
                }
            } else {
                unreachable!("Option should never be None");
            }
        }

        #inline_fn_impl_name(#arguments)
    }
}

#[cfg(test)]
mod tests {
 
}
