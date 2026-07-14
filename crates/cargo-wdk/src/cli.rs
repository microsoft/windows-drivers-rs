// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! This module defines the top-level CLI layer, its argument types and
//! structures used for parsing and validating arguments for various
//! subcommands.
use std::path::{Path, PathBuf};

use anyhow::{Ok, Result};
use clap::{ArgGroup, Args, Parser, Subcommand, ValueEnum};
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

    /// Driver signing mode.
    #[arg(long, value_enum, ignore_case = true, default_value_t = SignModeArg::Test)]
    pub sign_mode: SignModeArg,

    /// Verify the signature
    #[arg(long)]
    pub verify_signature: bool,

    /// Additional arguments to forward to `inf2cat`
    #[arg(
        long,
        value_name = "ARGS",
        value_parser = parse_inf2cat_args,
        help_heading = "Inf2Cat Options"
    )]
    pub inf2cat_args: Option<Inf2catArgs>,

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

impl BuildArgs {
    /// Maps the `--sign-mode` and `--verify-signature` combination to the
    /// respective [`SignMode`] variant, or returns an error.
    ///
    /// # Errors
    ///
    /// Returns an error if `--verify-signature` is used together with
    /// `--sign-mode=off`.
    fn sign_mode(&self) -> Result<SignMode> {
        match (self.sign_mode, self.verify_signature) {
            (SignModeArg::Off, true) => Err(anyhow::anyhow!(
                "`--verify-signature` cannot be used with `--sign-mode=off`."
            )),
            (SignModeArg::Off, false) => Ok(SignMode::Off),
            (SignModeArg::Test, verify_signature) => Ok(SignMode::Test { verify_signature }),
        }
    }

    fn inf2cat_arg_tokens(&self) -> Vec<String> {
        self.inf2cat_args
            .clone()
            .map(|parsed| parsed.0)
            .unwrap_or_default()
    }
}

/// Arguments forwarded verbatim to `inf2cat` (after cargo-wdk's own `/driver:`
/// argument).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inf2catArgs(pub Vec<String>);

/// `value_parser` for `--inf2cat-args`: tokenizes the raw string into
/// individual `inf2cat` arguments.
///
/// Rules:
/// - Whitespace separates arguments
/// - Quoted spans (single or double quotes) are preserved as a single argument
/// - Unterminated quotes are rejected with an error
/// - The `/driver:` (or its `/drv:` alias) switch is rejected because
///   `cargo-wdk` supplies that argument itself
fn parse_inf2cat_args(raw: &str) -> std::result::Result<Inf2catArgs, String> {
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
            "unterminated `{q}` quote in `--inf2cat-args`; make sure every quote is closed"
        ));
    }
    if in_arg {
        args.push(current);
    }

    // reject a user-supplied `/driver:` (or `/drv:` alias) early.
    if let Some(driver_arg) = args.iter().find(|arg| {
        let lower = arg.to_ascii_lowercase();
        lower.starts_with("/driver") || lower.starts_with("/drv")
    }) {
        return Err(format!(
            "`--inf2cat-args` must not contain `{driver_arg}`: cargo-wdk supplies the `/driver:` \
             argument itself"
        ));
    }

    std::result::Result::Ok(Inf2catArgs(args))
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
                let sign_mode = cli_args.sign_mode()?;
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
                        inf2cat_args: cli_args.inf2cat_arg_tokens(),
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
    use clap_cargo::Features;

    use crate::{
        actions::DriverType,
        cli::{BuildArgs, Cli, Inf2catArgs, NewArgs, SignModeArg, Subcmd, parse_inf2cat_args},
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

    #[test]
    fn build_rejects_verify_signature_when_sign_mode_is_off() {
        let cli = Cli {
            cargo_command: "wdk".to_string(),
            sub_cmd: Subcmd::Build(BuildArgs {
                profile: None,
                target_arch: None,
                verify_signature: true,
                sign_mode: SignModeArg::Off,
                inf2cat_args: None,
                sample: false,
                locked: false,
                features: Features::default(),
            }),
            verbose: clap_verbosity_flag::Verbosity::default(),
        };

        let result = cli.run();
        assert!(result.is_err());
        assert_eq!(
            result.err().unwrap().to_string(),
            "`--verify-signature` cannot be used with `--sign-mode=off`."
        );
    }

    #[test]
    fn parse_inf2cat_args_tokenizes_on_whitespace() {
        assert_eq!(
            parse_inf2cat_args("/os:10_x64 /uselocaltime").unwrap(),
            Inf2catArgs(vec!["/os:10_x64".to_string(), "/uselocaltime".to_string()])
        );
    }

    #[test]
    fn parse_inf2cat_args_preserves_quoted_spans() {
        assert_eq!(
            parse_inf2cat_args("/os:10_x64 \"a b\" /verbose").unwrap(),
            Inf2catArgs(vec![
                "/os:10_x64".to_string(),
                "a b".to_string(),
                "/verbose".to_string(),
            ])
        );
    }

    #[test]
    fn parse_inf2cat_args_rejects_unterminated_quote() {
        let err = parse_inf2cat_args("/os:10_x64 \"unterminated")
            .expect_err("unterminated quote should be rejected");
        assert!(err.contains("unterminated"), "unexpected error: {err}");
    }

    #[test]
    fn parse_inf2cat_args_treats_empty_or_whitespace_as_no_tokens() {
        for value in ["", "   ", "\t"] {
            assert_eq!(
                parse_inf2cat_args(value).unwrap(),
                Inf2catArgs(Vec::new()),
                "input {value:?} should yield no tokens"
            );
        }
    }

    #[test]
    fn parse_inf2cat_args_rejects_driver_switch() {
        for value in [
            "/driver:C:\\pkg",
            "/os:10_x64 /driver:C:\\pkg",
            "/DRIVER:C:\\pkg",
            "/drv:C:\\pkg",
        ] {
            let err = parse_inf2cat_args(value)
                .expect_err("a user-supplied /driver: switch should be rejected");
            assert!(
                err.contains("cargo-wdk supplies the `/driver:`"),
                "unexpected error for {value:?}: {err}"
            );
        }
    }

    #[test]
    fn build_inf2cat_args_maps_none_to_empty_and_some_to_tokens() {
        let mut args = BuildArgs {
            profile: None,
            target_arch: None,
            verify_signature: false,
            sign_mode: SignModeArg::Test,
            inf2cat_args: None,
            sample: false,
            locked: false,
            features: Features::default(),
        };
        assert!(args.inf2cat_arg_tokens().is_empty());

        args.inf2cat_args = Some(Inf2catArgs(vec![
            "/os:10_x64".to_string(),
            "/uselocaltime".to_string(),
        ]));
        assert_eq!(
            args.inf2cat_arg_tokens(),
            vec!["/os:10_x64".to_string(), "/uselocaltime".to_string()]
        );
    }
}
