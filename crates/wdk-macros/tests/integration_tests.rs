// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

use std::path::{Path, PathBuf};

use lazy_static::lazy_static;

lazy_static! {
    static ref TESTS_FOLDER_PATH: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests"].iter().collect();
    static ref MACROTEST_FOLDER_PATH: PathBuf = TESTS_FOLDER_PATH.join("macrotest");
    static ref TRYBUILD_FOLDER_PATH: PathBuf = TESTS_FOLDER_PATH.join("trybuild");
}

use std::{io::Write, stringify};

use owo_colors::OwoColorize;
use paste::paste;

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
                mod macro_expansion {
                    use super::*;

                    $(
                        #[test]
                        fn [<$filename _expansion>]() -> std::io::Result<()> {
                            macrotest::expand(&MACROTEST_FOLDER_PATH.join(format!("{}.rs", stringify!($filename))).canonicalize()?);
                            Ok(())
                        }
                    )?

                    mod nightly_feature {
                        use super::*;

                        $(
                            #[test]
                            fn [<$filename _expansion>]() -> std::io::Result<()> {
                                macrotest::expand_args(
                                    &MACROTEST_FOLDER_PATH.join(format!("{}.rs", stringify!($filename))).canonicalize()?, &["--features", "nightly"]);
                                Ok(())
                            }
                        )?
                    }
                }

                mod macro_compilation {
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
                                &MACROTEST_FOLDER_PATH
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
                                    &MACROTEST_FOLDER_PATH
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
    wdf_device_create_device_interface,
    wdf_spin_lock_acquire
);

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
