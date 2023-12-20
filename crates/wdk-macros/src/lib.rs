// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! A collection of macros that help make it easier to interact with
//! [`wdk-sys`]'s direct bindings to the Windows Driver Kit (WDK).
#![cfg_attr(feature = "nightly", feature(hint_must_use))]
#![deny(warnings)]
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

use cfg_if::cfg_if;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    parse2,
    Error,
    Expr,
    Ident,
    Token,
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
/// #![cfg_attr(feature = "nightly", feature(hint_must_use))]
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

struct CallUnsafeWDFFunctionInput {
    function_pointer_type: Ident,
    function_table_index: Ident,
    function_arguments: syn::punctuated::Punctuated<Expr, Token![,]>,
}

impl Parse for CallUnsafeWDFFunctionInput {
    fn parse(input: ParseStream) -> Result<Self, Error> {
        let c_function_name: String = input.parse::<Ident>()?.to_string();
        input.parse::<Token![,]>()?;
        let function_arguments = input.parse_terminated(Expr::parse, Token![,])?;

        Ok(Self {
            function_pointer_type: format_ident!(
                "PFN_{uppercase_c_function_name}",
                uppercase_c_function_name = c_function_name.to_uppercase()
            ),
            function_table_index: format_ident!("{c_function_name}TableIndex"),
            function_arguments,
        })
    }
}

fn call_unsafe_wdf_function_binding_impl(input_tokens: TokenStream2) -> TokenStream2 {
    let CallUnsafeWDFFunctionInput {
        function_pointer_type,
        function_table_index,
        function_arguments,
    } = match parse2::<CallUnsafeWDFFunctionInput>(input_tokens) {
        Ok(syntax_tree) => syntax_tree,
        Err(err) => return err.to_compile_error(),
    };

    // let inner_attribute_macros = proc_macro2::TokenStream::from_str(
    //     "#![allow(unused_unsafe)]\n\
    //      #![allow(clippy::multiple_unsafe_ops_per_block)]",
    // ).expect("inner_attribute_macros must be convertible to a valid
    // TokenStream");

    let wdf_function_call_tokens = quote! {
        {
            // Force the macro to require an unsafe block
            unsafe fn force_unsafe(){}
            force_unsafe();

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
                // `wdk_sys::WDF_FUNCTION_TABLE` indexed by `function_table_index` and guarded by the type-safety of
                // `function_pointer_type`. The passed arguments are also guaranteed to be of a compatible type due to
                // `function_pointer_type`.
                #[allow(unused_unsafe)]
                #[allow(clippy::multiple_unsafe_ops_per_block)]
                unsafe {
                    (wdf_function)(
                        wdk_sys::WdfDriverGlobals,
                        #function_arguments
                    )
                }
            } else {
                unreachable!("Option should never be None");
            }
        }
    };

    cfg_if! {
        if #[cfg(feature = "nightly")] {
            // FIXME: parse return type of function pointer and only emit
            // core::hint::must_use if the return type is not ()
            quote! {
                core::hint::must_use(#wdf_function_call_tokens)
            }
        } else {
            quote! {
                #wdf_function_call_tokens
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use cfg_if::cfg_if;
    use lazy_static::lazy_static;

    lazy_static! {
        static ref TESTS_FOLDER_PATH: PathBuf =
            [env!("CARGO_MANIFEST_DIR"), "tests"].iter().collect();
        static ref NIGHTLY_FOLDER_PATH: PathBuf = TESTS_FOLDER_PATH.join("nightly");
        static ref NON_NIGHTLY_FOLDER_PATH: PathBuf = TESTS_FOLDER_PATH.join("non-nightly");
        static ref NIGHTLY_FEATURE_MACROTEST_FOLDER_PATH: PathBuf =
            NIGHTLY_FOLDER_PATH.join("macrotest");
        static ref NO_NIGHTLY_FEATURE_MACROTEST_FOLDER_PATH: PathBuf =
            NON_NIGHTLY_FOLDER_PATH.join("macrotest");
        static ref TRYBUILD_FOLDER_PATH: PathBuf = {
            cfg_if! {
                if #[cfg(feature = "nightly")] {
                    NIGHTLY_FOLDER_PATH.join("trybuild")
                } else {
                    NON_NIGHTLY_FOLDER_PATH.join("trybuild")
                }
            }
        };
    }

    mod macro_expansion_and_compilation {
        use std::{io::Write, stringify};

        use colored::Colorize;
        use paste::paste;

        use super::*;

        /// Given a filename `f` which contains code utilizing macros in
        /// `wdk-macros`, generates a pair of tests to verify that code in `f`
        /// expands as expected, and compiles successfully. The test output will
        /// show `<f>_expansion` as the names of the expansion tests and
        /// `<f>_compilation` as the name of the compilation test. `f` must
        /// reside in the `tests/macrotest` folder, and may be a path to
        /// a file relative to the `tests/macrotest` folder.
        ///
        /// Note: Due to limitations in `trybuild`, a successful compilation
        /// test will include output that looks similar to the following:
        /// ```
        /// test \\?\D:\git-repos\windows-drivers-rs\crates\wdk-macros\tests\macrotest\wdf_driver_create.rs ... error
        /// Expected test case to fail to compile, but it succeeded.
        /// ```
        /// This is because `trybuild` will run `cargo check` when calling
        /// `TestCases::compile_fail`, but will run `cargo build` if calling
        /// `TestCases::pass`. `cargo build` will fail at link stage due to
        /// `trybuild` not allowing configuration to compile as a`cdylib`. To
        /// work around this, `compile_fail` is used, and we mark the test as
        /// expecting to panic with a specific message using the `should_panic`
        /// attribute macro.
        macro_rules! generate_macro_expansion_and_compilation_tests {
            ($($filename:ident),+) => {
                paste! {

                    // This module's tests are deliberately not feature-gated by #[cfg(feature = "nightly")] since macrotest can control whether to expand with the nightly feature or not
                    mod expansion_tests {
                        use super::*;

                        $(
                            #[test]
                            fn [<$filename _expansion>]() -> std::io::Result<()> {
                                macrotest::expand(&NO_NIGHTLY_FEATURE_MACROTEST_FOLDER_PATH.join(format!("{}.rs", stringify!($filename))).canonicalize()?);
                                Ok(())
                            }
                        )?

                        mod nightly_feature {
                            use super::*;

                            $(
                                #[test]
                                fn [<$filename _expansion>]() -> std::io::Result<()> {
                                    macrotest::expand_args(
                                        &NIGHTLY_FEATURE_MACROTEST_FOLDER_PATH.join(format!("{}.rs", stringify!($filename))).canonicalize()?, &["--features", "nightly"]);
                                    Ok(())
                                }
                            )?
                        }
                    }

                    mod compilation_tests {
                        use super::*;

                        pub trait TestCasesExt {
                            fn pass_cargo_check<P: AsRef<Path> + std::panic::UnwindSafe>(path: P);
                        }

                        impl TestCasesExt for trybuild::TestCases {
                            fn pass_cargo_check<P: AsRef<Path> + std::panic::UnwindSafe>(path: P) {
                                // "compile_fail" tests that pass cargo check result in this panic message
                                const SUCCESSFUL_CARGO_CHECK_STRING: &str = "1 of 1 tests failed";

                                let path = path.as_ref();

                                let failed_cargo_check = !std::panic::catch_unwind(|| {
                                    // A new TestCases is required because it relies on running the tests upon drop
                                    trybuild::TestCases::new().compile_fail(path);
                                })
                                .is_err_and(|cause| {
                                    if let Some(str) = cause.downcast_ref::<&str>() {
                                        *str == SUCCESSFUL_CARGO_CHECK_STRING
                                    } else if let Some(string) = cause.downcast_ref::<String>() {
                                        string == SUCCESSFUL_CARGO_CHECK_STRING
                                    } else {
                                        // Unexpected panic trait object type
                                        false
                                    }
                                });

                                if failed_cargo_check {
                                    let failed_cargo_check_msg = format!(
                                        "{}{}",
                                        path.to_string_lossy().bold().red(),
                                        " failed Cargo Check!".bold().red()
                                    );

                                    // Use writeln! to print even without passing --nocapture to the test harness
                                    writeln!(&mut std::io::stderr(), "{failed_cargo_check_msg}").unwrap();

                                    panic!("{failed_cargo_check_msg}");
                                } else {
                                    // Use writeln! to print even without passing --nocapture to the test harness
                                    writeln!(
                                        &mut std::io::stderr(),
                                        "{}{}{}{}{}",
                                        "Please ignore the above \"Expected test case to fail to compile, but it \
                                        succeeded.\" message (and its accompanying \"1 of 1 tests failed\" panic \
                                        message when run with --nocapture).\n"
                                            .italic()
                                            .yellow(),
                                        "test ".bold(),
                                        path.to_string_lossy().bold(),
                                        " ... ".bold(),
                                        "PASSED".bold().green()
                                    ).unwrap();
                                }
                            }
                        }

                        $(
                            #[cfg(not(feature = "nightly"))]
                            #[test]
                            fn [<$filename _compilation>]() {
                                trybuild::TestCases::pass_cargo_check(
                                    &NO_NIGHTLY_FEATURE_MACROTEST_FOLDER_PATH
                                        .join(format!("{}.rs", stringify!($filename)))
                                        .canonicalize()
                                        .expect(concat!(stringify!($filename), " should exist")),
                                );
                            }
                        )?

                        #[cfg(feature = "nightly")]
                        mod nightly_feature {
                            use super::*;

                            $(
                                #[test]
                                fn [<$filename _compilation>]() {
                                    trybuild::TestCases::pass_cargo_check(
                                        &NIGHTLY_FEATURE_MACROTEST_FOLDER_PATH
                                            .join(format!("{}.rs", stringify!($filename)))
                                            .canonicalize()
                                            .expect(concat!(stringify!($filename), " should exist")),
                                    );
                                }
                            )?
                        }
                    }
                }
            };
        }

        generate_macro_expansion_and_compilation_tests!(
            wdf_driver_create,
            wdf_device_create,
            wdf_device_create_device_interface
        );
    }

    mod macro_usage_errors {
        use super::*;

        /// This test leverages `trybuild` to ensure that developer misuse of
        /// the macro cause compilation failures, with an appropriate message
        #[test]
        fn trybuild() {
            trybuild::TestCases::new().compile_fail(
                // canonicalization of this path causes a bug in `glob`: https://github.com/rust-lang/glob/issues/132
                TRYBUILD_FOLDER_PATH // .canonicalize()?
                    .join("*.rs"),
            );
        }
    }
}
