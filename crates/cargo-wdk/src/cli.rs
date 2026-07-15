// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! This module defines the top-level CLI layer, its argument types and
//! structures used for parsing and validating arguments for various
//! subcommands.
use std::path::{Path, PathBuf};

use anyhow::{Ok, Result};
use clap::{ArgGroup, Args, CommandFactory, Parser, Subcommand, ValueEnum, error::ErrorKind};
use clap_cargo::Features;
use clap_verbosity_flag::Verbosity;
use mockall_double::double;
use wdk_build::CpuArchitecture;

use crate::actions::{
    DriverType,
    KMDF_STR,
    Profile,
    UMDF_STR,
    WDM_STR,
    build::{BuildAction, BuildActionParams, SignMode},
    clean::CleanAction,
    new::NewAction,
};
#[double]
use crate::providers::{exec::CommandExec, fs::Fs, metadata::Metadata, wdk_build::WdkBuild};

const ABOUT_STRING: &str = "cargo-wdk is a cargo extension that can be used to create and build \
                            Windows Rust driver projects.";
const CARGO_WDK_BIN_NAME: &str = "cargo wdk";

/// Driver signing mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
#[value(rename_all = "lower")]
pub enum SignModeArg {
    /// Skip signing.
    Off,
    /// Sign with an auto-generated self-signed certificate.
    #[default]
    Test,
}

/// Arguments to `signtool sign` for signing the driver binary and catalog file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigntoolArgs(pub Vec<String>);

/// Arguments for the `new` subcommand
#[derive(Debug, Args)]
#[clap(
    group(
        ArgGroup::new("driver_type")
            .required(true)
            .args([KMDF_STR, UMDF_STR, WDM_STR])
    ),
)]
pub struct NewArgs {
    /// Create a KMDF driver crate
    #[arg(long)]
    pub kmdf: bool,

    /// Create a UMDF driver crate
    #[arg(long)]
    pub umdf: bool,

    /// Create a WDM driver crate
    #[arg(long)]
    pub wdm: bool,

    /// Path at which the new driver crate should be created
    #[arg(required = true)]
    pub path: Option<PathBuf>,
}

impl NewArgs {
    /// Returns the variant of `DriverType` based on which of the `driver_type`
    /// flags, `--kmdf`, `--umdf` or `--wdm` was passed to the `new` command.
    ///
    /// # Returns
    ///
    /// * `DriverType`
    const fn driver_type(&self) -> DriverType {
        // `ArgGroup` setting on `NewArgs` ensures
        // exactly one of these flags is set
        if self.kmdf {
            DriverType::Kmdf
        } else if self.umdf {
            DriverType::Umdf
        } else {
            DriverType::Wdm
        }
    }
}

/// Arguments for the `build` subcommand
#[derive(Debug, Args)]
pub struct BuildArgs {
    /// Build artifacts with the specified profile
    #[arg(long, ignore_case = true)]
    pub profile: Option<Profile>,

    /// Build for the target architecture
    #[arg(long, ignore_case = true)]
    pub target_arch: Option<CpuArchitecture>,

    /// Signing mode.
    #[arg(
        long,
        value_enum,
        ignore_case = true,
        default_value_t = SignModeArg::Test,
        help_heading = "Driver Signing"
    )]
    pub sign_mode: SignModeArg,

    /// Additional arguments to pass to `signtool sign` when signing the driver
    /// binary and the catalog file, e.g.
    /// `--signtool-args '/fd SHA512 /n "CN=WDRLocalTestCert, O=Foo"'`.
    #[arg(
        long,
        value_name = "ARGS",
        value_parser = parse_signtool_args,
        help_heading = "Driver Signing"
    )]
    pub signtool_args: Option<SigntoolArgs>,

    /// Verify the signatures of the driver binary and catalog file after
    /// signing.
    #[arg(long, help_heading = "Driver Signing")]
    pub verify_signature: bool,

    /// Build sample class driver project
    #[arg(long)]
    pub sample: bool,

    /// Assert that `Cargo.lock` will remain unchanged
    #[arg(long)]
    pub locked: bool,

    #[command(flatten)]
    #[clap(next_help_heading = "Feature Selection")]
    pub features: Features,
}

/// Resolves a typed, fully-validated [`SignMode`] from the parsed build
/// arguments. Rules that clap cannot express declaratively are enforced here
/// and surfaced as `clap::Error` for consistent CLI UX.
impl TryFrom<&BuildArgs> for SignMode {
    type Error = clap::Error;

    fn try_from(args: &BuildArgs) -> Result<Self, clap::Error> {
        match args.sign_mode {
            SignModeArg::Off => {
                if args.verify_signature {
                    return Err(build_error(
                        "`--verify-signature` cannot be used with `--sign-mode=off`.",
                    ));
                }
                if args.signtool_args.is_some() {
                    return Err(build_error(
                        "`--signtool-args` cannot be used with `--sign-mode=off`.",
                    ));
                }
                std::result::Result::Ok(Self::Off)
            }
            SignModeArg::Test => std::result::Result::Ok(Self::Test {
                verify_signature: args.verify_signature,
                signtool_args: args
                    .signtool_args
                    .clone()
                    .map(|parsed| parsed.0)
                    .unwrap_or_default(),
            }),
        }
    }
}

/// Builds a `clap::Error` with the given message, rendered with the standard
/// `cargo wdk build` usage for a consistent CLI experience.
fn build_error(message: impl std::fmt::Display) -> clap::Error {
    Cli::command().error(ErrorKind::ArgumentConflict, message)
}

/// `value_parser` for `--signtool-args`: tokenizes the raw string into
/// individual `signtool` arguments.
///
/// Rules:
/// - Whitespace separates arguments
/// - Quoted spans (single or double quotes) are preserved as a single argument
/// - Unterminated quotes are rejected with an error
fn parse_signtool_args(raw: &str) -> std::result::Result<SigntoolArgs, String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_arg = false;
    let mut quote: Option<char> = None;

    for c in raw.chars() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                } else {
                    current.push(c);
                }
            }
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                in_arg = true;
            }
            None if c.is_whitespace() => {
                if in_arg {
                    args.push(std::mem::take(&mut current));
                    in_arg = false;
                }
            }
            None => {
                current.push(c);
                in_arg = true;
            }
        }
    }

    if let Some(q) = quote {
        return Err(format!(
            "unterminated `{q}` quote in `--signtool-args`; make sure every quote is closed"
        ));
    }
    if in_arg {
        args.push(current);
    }

    std::result::Result::Ok(SigntoolArgs(args))
}

/// Subcommands
#[derive(Debug, Subcommand)]
pub enum Subcmd {
    #[clap(name = "new", about = "Create a new Windows Driver Kit project")]
    New(NewArgs),
    #[clap(name = "build", about = "Build the Windows Driver Kit project")]
    Build(BuildArgs),
    #[clap(
        name = "clean",
        about = "Clean build artifacts of the Windows Driver Kit project"
    )]
    Clean,
}

/// Top level command line interface for cargo wdk
#[derive(Debug, Parser)]
#[clap(
    name = env!("CARGO_PKG_NAME"),
    version = env!("CARGO_PKG_VERSION"),
    bin_name = CARGO_WDK_BIN_NAME,
    display_name = CARGO_WDK_BIN_NAME,
    author = env!("CARGO_PKG_AUTHORS"),
    about = ABOUT_STRING,
)]
#[command(styles = clap_cargo::style::CLAP_STYLING)]
pub struct Cli {
    #[clap(name = "cargo command", default_value = "wdk", hide = true)]
    pub cargo_command: String,
    #[clap(subcommand)]
    pub sub_cmd: Subcmd,
    #[command(flatten)]
    #[clap(next_help_heading = "Verbosity")]
    pub verbose: Verbosity,
}

impl Cli {
    /// Entry point method to construct and call actions based on the subcommand
    /// and arguments provided by the user.
    pub fn run(self) -> Result<()> {
        let wdk_build = WdkBuild::default();
        let command_exec = CommandExec::default();
        let fs = Fs::default();
        let metadata = Metadata::default();

        match self.sub_cmd {
            Subcmd::New(cli_args) => {
                // TODO: Support extended path as cargo supports it
                if let Some(path) = &cli_args.path {
                    const EXTENDED_PATH_PREFIX: &str = r"\\?\";
                    if path
                        .as_os_str()
                        .to_string_lossy()
                        .starts_with(EXTENDED_PATH_PREFIX)
                    {
                        return Err(anyhow::anyhow!(
                            "Extended/Verbatim paths (i.e. paths starting with '\\?') are not \
                             currently supported"
                        ));
                    }
                }

                NewAction::new(
                    cli_args.path.as_ref().unwrap_or(&std::env::current_dir()?),
                    cli_args.driver_type(),
                    self.verbose,
                    &command_exec,
                    &fs,
                )
                .run()?;
                Ok(())
            }
            Subcmd::Build(cli_args) => {
                let sign_mode = SignMode::try_from(&cli_args)?;
                BuildAction::new(
                    &BuildActionParams {
                        working_dir: Path::new("."), // Using current dir as working dir
                        profile: cli_args.profile.as_ref(),
                        target_arch: cli_args.target_arch,
                        sign_mode,
                        is_sample_class: cli_args.sample,
                        locked: cli_args.locked,
                        features: &cli_args.features,
                        verbosity_level: self.verbose,
                    },
                    &wdk_build,
                    &command_exec,
                    &fs,
                    &metadata,
                )?
                .run()?;
                Ok(())
            }
            Subcmd::Clean => {
                CleanAction::new(Path::new("."), self.verbose, &command_exec, &fs)?.run()?;
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use crate::{
        actions::{DriverType, build::SignMode},
        cli::{BuildArgs, Cli, NewArgs, Subcmd},
    };

    #[test]
    fn new_args_driver_type_kmdf() {
        let args = NewArgs {
            kmdf: true,
            umdf: false,
            wdm: false,
            path: None,
        };
        assert_eq!(args.driver_type(), DriverType::Kmdf);
    }

    #[test]
    fn new_args_driver_type_umdf() {
        let args = NewArgs {
            kmdf: false,
            umdf: true,
            wdm: false,
            path: None,
        };
        assert_eq!(args.driver_type(), DriverType::Umdf);
    }

    #[test]
    fn new_args_driver_type_wdm() {
        let args = NewArgs {
            kmdf: false,
            umdf: false,
            wdm: true,
            path: None,
        };
        assert_eq!(args.driver_type(), DriverType::Wdm);
    }

    #[test]
    fn verbatim_path_is_rejected() {
        use std::path::PathBuf;

        let cli = Cli {
            cargo_command: "wdk".to_string(),
            sub_cmd: crate::cli::Subcmd::New(NewArgs {
                kmdf: true,
                umdf: false,
                wdm: false,
                path: Some(PathBuf::from(r"\\?\C:\some\path")),
            }),
            verbose: clap_verbosity_flag::Verbosity::default(),
        };

        let result = cli.run();
        assert!(result.is_err());
        assert_eq!(
            result.err().unwrap().to_string(),
            "Extended/Verbatim paths (i.e. paths starting with '\\?') are not currently supported"
        );
    }

    mod build {
        use super::*;

        // Parses `cargo wdk build <extra args>` and returns the parsed `BuildArgs`,
        // or the clap error if parsing/validation fails.
        fn parse_build_args(extra: &[&str]) -> Result<BuildArgs, clap::Error> {
            let mut command_line = vec!["cargo-wdk", "wdk", "build"];
            command_line.extend_from_slice(extra);
            match Cli::try_parse_from(command_line)?.sub_cmd {
                Subcmd::Build(build_args) => std::result::Result::Ok(build_args),
                _ => unreachable!("build subcommand was requested"),
            }
        }

        #[test]
        fn rejects_verify_signature_when_sign_mode_is_off() {
            let args = parse_build_args(&["--sign-mode", "off", "--verify-signature"])
                .expect("args parse");
            let err = SignMode::try_from(&args).expect_err("should be rejected");
            assert!(
                err.to_string()
                    .contains("`--verify-signature` cannot be used with `--sign-mode=off`."),
                "unexpected error: {err}"
            );
        }

        #[test]
        fn rejects_signtool_args_with_sign_mode_off() {
            let args = parse_build_args(&["--sign-mode", "off", "--signtool-args", "/fd SHA256"])
                .expect("args parse");
            let err = SignMode::try_from(&args).expect_err("should be rejected");
            assert!(
                err.to_string()
                    .contains("`--signtool-args` cannot be used with `--sign-mode=off`."),
                "unexpected error: {err}"
            );
        }

        #[test]
        fn rejects_empty_signtool_args_with_sign_mode_off() {
            for value in ["", "   ", "\t"] {
                let args = parse_build_args(&["--sign-mode", "off", "--signtool-args", value])
                    .expect("args should parse");
                let err = SignMode::try_from(&args).expect_err("should be rejected");
                assert!(
                    err.to_string()
                        .contains("`--signtool-args` cannot be used with `--sign-mode=off`."),
                    "value {value:?} should be rejected, got: {err}"
                );
            }
        }

        #[test]
        fn sign_mode_off_maps_correctly() {
            let args = parse_build_args(&["--sign-mode", "off"]).expect("args should parse");
            assert_eq!(
                SignMode::try_from(&args).expect("mapping should succeed"),
                SignMode::Off
            );
        }

        #[test]
        fn default_options_maps_to_test_sign_mode_with_no_signtool_args() {
            let args = parse_build_args(&[]).expect("args should parse");
            assert_eq!(
                SignMode::try_from(&args).expect("mapping should succeed"),
                SignMode::Test {
                    verify_signature: false,
                    signtool_args: Vec::new(),
                }
            );
        }

        #[test]
        fn tokenizes_signtool_args() {
            let args = parse_build_args(&["--signtool-args", "/fd SHA384 /f cert.pfx"])
                .expect("args should parse");
            assert_eq!(
                SignMode::try_from(&args).expect("mapping should succeed"),
                SignMode::Test {
                    verify_signature: false,
                    signtool_args: vec![
                        "/fd".to_string(),
                        "SHA384".to_string(),
                        "/f".to_string(),
                        "cert.pfx".to_string(),
                    ],
                }
            );
        }

        #[test]
        fn tokenizes_signtool_args_preserving_quoted_spans() {
            let args = parse_build_args(&["--signtool-args", "/n \"CN=Contoso Root\" /fd SHA256"])
                .expect("args should parse");
            assert_eq!(
                SignMode::try_from(&args).expect("mapping should succeed"),
                SignMode::Test {
                    verify_signature: false,
                    signtool_args: vec![
                        "/n".to_string(),
                        "CN=Contoso Root".to_string(),
                        "/fd".to_string(),
                        "SHA256".to_string(),
                    ],
                }
            );
        }

        #[test]
        fn rejects_unterminated_quote_in_signtool_args() {
            let err = parse_build_args(&["--signtool-args", "/n \"CN=Contoso"])
                .expect_err("unterminated quote should be rejected");
            assert!(
                err.to_string().contains(
                    "unterminated `\"` quote in `--signtool-args`; make sure every quote is closed"
                ),
                "unexpected error: {err}"
            );
        }

        #[test]
        fn treats_empty_or_whitespace_signtool_args_as_not_provided() {
            for value in ["", "   ", "\t"] {
                let args =
                    parse_build_args(&["--signtool-args", value]).expect("args should parse");
                assert_eq!(
                    SignMode::try_from(&args).expect("mapping should succeed"),
                    SignMode::Test {
                        verify_signature: false,
                        signtool_args: Vec::new(),
                    },
                    "value {value:?} should map to no signtool args"
                );
            }
        }

        #[test]
        fn verify_signature_works_with_signtool_args() {
            let args = parse_build_args(&["--verify-signature", "--signtool-args", "/fd SHA256"])
                .expect("args should parse");
            assert_eq!(
                SignMode::try_from(&args).expect("mapping should succeed"),
                SignMode::Test {
                    verify_signature: true,
                    signtool_args: vec!["/fd".to_string(), "SHA256".to_string()],
                }
            );
        }
    }
}
