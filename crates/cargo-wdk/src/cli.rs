// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! This module defines the top-level CLI layer, its argument types and
//! structures used for parsing and validating arguments for various
//! subcommands.
use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::{ArgGroup, Args, CommandFactory, Parser, Subcommand, ValueEnum, error::ErrorKind};
use clap_cargo::Features;
use clap_verbosity_flag::Verbosity;
use mockall_double::double;
use wdk_build::CpuArchitecture;

use crate::actions::{
    build::{BuildAction, BuildActionParams, Profile, SignMode, TargetPlatform},
    clean::CleanAction,
    new::{DriverType, KMDF_STR, NewAction, UMDF_STR, WDM_STR},
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

/// Arguments passed through to downstream tools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassthroughArgs(pub Vec<String>);

/// Platform at which the device driver is targeted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum TargetPlatformArg {
    /// Validates that the INF meets Universal driver requirements.
    Universal,
    /// Validates that the INF meets Desktop driver requirements.
    Desktop,
    /// Validates that the INF meets Windows driver requirements.
    Windows,
}

impl From<TargetPlatformArg> for TargetPlatform {
    fn from(value: TargetPlatformArg) -> Self {
        match value {
            TargetPlatformArg::Universal => Self::Universal,
            TargetPlatformArg::Desktop => Self::Desktop,
            TargetPlatformArg::Windows => Self::Windows,
        }
    }
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

    /// Driver target platform
    #[arg(long, value_enum, ignore_case = true, default_value_t = TargetPlatformArg::Universal)]
    pub target_platform: TargetPlatformArg,

    /// Build sample class driver project
    #[arg(long)]
    pub sample: bool,

    /// Signing mode
    #[arg(
        long,
        value_enum,
        ignore_case = true,
        default_value_t = SignModeArg::Test,
        help_heading = "Driver Signing"
    )]
    pub sign_mode: SignModeArg,

    /// Custom arguments to pass to `signtool sign` when signing the driver
    /// binary and the catalog file, e.g.
    /// `--signtool-args '/fd SHA512 /n "CN=WDRLocalTestCert, O=Foo"'`.
    #[arg(
        long,
        value_name = "ARGS",
        value_parser = parse_passthrough_args,
        help_heading = "Driver Signing"
    )]
    pub signtool_args: Option<PassthroughArgs>,

    /// Verify the signatures of the driver binary and catalog file after
    /// signing.
    #[arg(long, help_heading = "Driver Signing")]
    pub verify_signature: bool,

    /// Custom arguments to pass to `inf2cat` when generating the catalog file,
    /// e.g. `--inf2cat-args '/os:10_x64,10_GE_X64 /uselocaltime'`. `/driver:`
    /// and `/drv:` are not allowed because cargo-wdk passes them by default.
    #[arg(
        long,
        value_name = "ARGS",
        value_parser = parse_passthrough_args,
        help_heading = "Inf2Cat Options"
    )]
    pub inf2cat_args: Option<PassthroughArgs>,

    /// Custom arguments to pass to `stampinf` when generating the INF file,
    /// e.g. `--stampinf-args '-d 01/01/2026 -v 1.2.3.4 -p "Contoso Ltd"'`.
    #[arg(
        long,
        value_name = "ARGS",
        // `stampinf` args can be `-` prefixed.
        allow_hyphen_values = true,
        value_parser = parse_passthrough_args,
        help_heading = "Stampinf Options"
    )]
    pub stampinf_args: Option<PassthroughArgs>,

    /// Custom arguments to pass to `infverif` when validating the INF,
    /// e.g. `--infverif-args '/rulever 10.0.22621 /info'`.
    #[arg(
        long,
        value_name = "ARGS",
        // `infverif` args can be `-` prefixed.
        allow_hyphen_values = true,
        value_parser = parse_passthrough_args,
        help_heading = "InfVerif Options"
    )]
    pub infverif_args: Option<PassthroughArgs>,

    /// Assert that `Cargo.lock` will remain unchanged
    #[arg(long)]
    pub locked: bool,

    #[command(flatten)]
    #[clap(next_help_heading = "Feature Selection")]
    pub features: Features,
}

impl BuildArgs {
    /// Resolves a typed, fully-validated [`SignMode`] from the parsed build
    /// arguments. Rules that clap cannot express declaratively are enforced
    /// here and surfaced as `clap::Error` for consistent CLI UX.
    fn sign_mode(&self) -> Result<SignMode, clap::Error> {
        fn build_error(message: impl std::fmt::Display) -> clap::Error {
            Cli::command().error(ErrorKind::ArgumentConflict, message)
        }

        match self.sign_mode {
            SignModeArg::Off => {
                if self.verify_signature {
                    return Err(build_error(
                        "`--verify-signature` cannot be used with `--sign-mode=off`.",
                    ));
                }
                if self.signtool_args.is_some() {
                    return Err(build_error(
                        "`--signtool-args` cannot be used with `--sign-mode=off`.",
                    ));
                }
                Ok(SignMode::Off)
            }
            SignModeArg::Test => Ok(SignMode::Test {
                verify_signature: self.verify_signature,
                signtool_args: self
                    .signtool_args
                    .clone()
                    .map_or_else(Vec::new, |parsed| parsed.0),
            }),
        }
    }

    /// Resolves the arguments to forward to `inf2cat`. Rejects a
    /// caller-supplied `/driver:` (or its `/drv:` alias).
    /// Returns a `clap::Error` if the caller-supplied arguments are invalid.
    fn inf2cat_args(&self) -> Result<Option<Vec<String>>, clap::Error> {
        let Some(args) = self.inf2cat_args.clone().map(|parsed| parsed.0) else {
            return Ok(None);
        };
        for arg in &args {
            let lower = arg.to_ascii_lowercase();
            if lower.starts_with("/driver:") || lower.starts_with("/drv:") {
                return Err(Cli::command().error(
                    ErrorKind::ArgumentConflict,
                    format!(
                        "`--inf2cat-args` must not contain `{arg}`; cargo-wdk supplies the \
                         `/driver:` switch itself"
                    ),
                ));
            }
        }
        Ok(Some(args))
    }

    /// Resolves the arguments to forward to `stampinf`. Rejects
    /// the args cargo-wdk derives from the build itself: `-f`, `-a`, `-c`,
    /// `-k` and `-u`.
    /// Returns a `clap::Error` if the caller-supplied arguments are invalid.
    fn stampinf_args(&self) -> Result<Option<Vec<String>>, clap::Error> {
        const RESERVED_ARGS: [&str; 5] = ["f", "a", "c", "k", "u"];
        let Some(args) = self.stampinf_args.clone().map(|parsed| parsed.0) else {
            return Ok(None);
        };
        for arg in &args {
            // `stampinf` accepts both `-x` and `/x`, case-insensitively.
            let Some(arg_name) = arg.strip_prefix(['-', '/']) else {
                continue;
            };
            if RESERVED_ARGS
                .iter()
                .any(|reserved| arg_name.eq_ignore_ascii_case(reserved))
            {
                let reserved_args = RESERVED_ARGS
                    .iter()
                    .map(|arg| format!("`-{arg}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(Cli::command().error(
                    ErrorKind::ArgumentConflict,
                    format!(
                        "`--stampinf-args` must not contain `{arg}`; cargo-wdk supplies the \
                         {reserved_args} args itself"
                    ),
                ));
            }
        }
        Ok(Some(args))
    }

    /// Resolves the arguments to forward to `infverif`. Rejects the arguments
    /// cargo-wdk supplies itself: the mode flags derived from the
    /// `--target-platform` option and the INF file path.
    fn infverif_args(&self) -> Result<Option<Vec<String>>, clap::Error> {
        const MODE_FLAGS: [&str; 3] = ["h", "w", "u"];

        let Some(args) = self.infverif_args.clone().map(|parsed| parsed.0) else {
            return Ok(None);
        };
        for arg in &args {
            let mode_flag = arg.trim_start_matches(['/', '-']).to_ascii_lowercase();
            let reason = if MODE_FLAGS.contains(&mode_flag.as_str()) {
                format!(
                    "cargo-wdk derives the mode flag `{arg}` from the `--target-platform` option"
                )
            } else if Path::new(arg)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("inf"))
            {
                "cargo-wdk supplies the INF file path itself".to_string()
            } else {
                continue;
            };
            return Err(Cli::command().error(
                ErrorKind::ArgumentConflict,
                format!("`--infverif-args` must not contain `{arg}`; {reason}"),
            ));
        }
        Ok(Some(args))
    }
}

/// `value_parser` for passthrough tool arguments: tokenizes the raw string
/// into individual arguments for an external tool.
///
/// Rules:
/// - Whitespace separates arguments
/// - Quoted spans (single or double quotes) are preserved as a single argument
/// - Unterminated quotes are rejected with an error
fn parse_passthrough_args(raw: &str) -> Result<PassthroughArgs, String> {
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
                    let token = std::mem::take(&mut current);
                    args.push(token);
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
            "unterminated `{q}` quote in passthrough arguments; make sure every quote is closed"
        ));
    }
    if in_arg {
        args.push(current);
    }

    Ok(PassthroughArgs(args))
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
            }
            Subcmd::Build(cli_args) => {
                let sign_mode = cli_args.sign_mode()?;
                let inf2cat_args = cli_args.inf2cat_args()?;
                let stampinf_args = cli_args.stampinf_args()?;
                let infverif_args = cli_args.infverif_args()?;
                BuildAction::new(
                    &BuildActionParams {
                        working_dir: Path::new("."), // Using current dir as working dir
                        profile: cli_args.profile.as_ref(),
                        target_arch: cli_args.target_arch,
                        sign_mode,
                        inf2cat_args,
                        stampinf_args,
                        infverif_args,
                        is_sample_class: cli_args.sample,
                        locked: cli_args.locked,
                        target_platform: cli_args.target_platform.into(),
                        features: &cli_args.features,
                        verbosity_level: self.verbose,
                    },
                    &wdk_build,
                    &command_exec,
                    &fs,
                    &metadata,
                )?
                .run()?;
            }
            Subcmd::Clean => {
                CleanAction::new(Path::new("."), self.verbose, &command_exec, &fs)?.run()?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use crate::{
        actions::{build::SignMode, new::DriverType},
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

        fn parse_build_args(extra: &[&str]) -> Result<BuildArgs, clap::Error> {
            let mut command_line = vec!["cargo-wdk", "wdk", "build"];
            command_line.extend_from_slice(extra);
            match Cli::try_parse_from(command_line)?.sub_cmd {
                Subcmd::Build(build_args) => Ok(build_args),
                _ => unreachable!("build subcommand was requested"),
            }
        }

        #[test]
        fn rejects_verify_signature_when_sign_mode_is_off() {
            let args = parse_build_args(&["--sign-mode", "off", "--verify-signature"])
                .expect("args parse");
            let err = args.sign_mode().expect_err("should be rejected");
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
            let err = args.sign_mode().expect_err("should be rejected");
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
                let err = args.sign_mode().expect_err("should be rejected");
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
                args.sign_mode().expect("mapping should succeed"),
                SignMode::Off
            );
        }

        #[test]
        fn default_options_maps_to_test_sign_mode_with_no_signtool_args() {
            let args = parse_build_args(&[]).expect("args should parse");
            assert_eq!(
                args.sign_mode().expect("mapping should succeed"),
                SignMode::Test {
                    verify_signature: false,
                    signtool_args: Vec::new(),
                }
            );
        }

        #[test]
        fn verify_signature_works_with_signtool_args() {
            let args = parse_build_args(&["--verify-signature", "--signtool-args", "/fd SHA256"])
                .expect("args should parse");
            assert_eq!(
                args.sign_mode().expect("mapping should succeed"),
                SignMode::Test {
                    verify_signature: true,
                    signtool_args: vec!["/fd".to_string(), "SHA256".to_string()],
                }
            );
        }

        #[test]
        fn inf2cat_args_rejects_driver_switch() {
            for value in ["/driver:x", "/DRIVER:x", "/drv:x", "/os:10_x64 /driver:y"] {
                let args = parse_build_args(&["--inf2cat-args", value]).expect("args should parse");
                let err = args
                    .inf2cat_args()
                    .expect_err("driver switch should be rejected");
                assert!(
                    err.to_string()
                        .contains("cargo-wdk supplies the `/driver:` switch itself"),
                    "unexpected error for {value:?}: {err}"
                );
            }
        }

        #[test]
        fn inf2cat_args_allows_other_switches() {
            let args = parse_build_args(&["--inf2cat-args", "/os:10_x64 /uselocaltime"])
                .expect("args should parse");
            assert_eq!(
                args.inf2cat_args().expect("should resolve"),
                Some(vec!["/os:10_x64".to_string(), "/uselocaltime".to_string()])
            );
        }

        #[test]
        fn stampinf_args_rejects_args_reserved_by_cargo_wdk() {
            for value in [
                "-f other.inf",
                "-a arm64",
                "-c other.cat",
                "-k 1.15",
                "-u 2.33.0",
                "/c other.cat",
                "-C other.cat",
                "-d 01/01/2026 /A arm64",
            ] {
                let args =
                    parse_build_args(&["--stampinf-args", value]).expect("args should parse");
                let err = args
                    .stampinf_args()
                    .expect_err("reserved arg should be rejected");
                assert!(
                    err.to_string()
                        .contains("cargo-wdk supplies the `-f`, `-a`, `-c`, `-k`, `-u` args"),
                    "unexpected error for {value:?}: {err}"
                );
            }
        }

        fn assert_infverif_args_rejected(value: &str, expected_reason: &str) {
            let args = parse_build_args(&["--infverif-args", value]).expect("args should parse");
            let err = args
                .infverif_args()
                .expect_err("reserved argument should be rejected");
            assert!(
                err.to_string().contains(expected_reason),
                "unexpected error for {value:?}: {err}"
            );
        }

        #[test]
        fn infverif_args_rejects_mode_flags() {
            for (value, mode_flag) in [
                ("/h", "/h"),
                ("-w", "-w"),
                ("/U", "/U"),
                ("/info -w", "-w"),
                ("-info /w", "/w"),
                ("/rulever 10.0.22621 -h /info /w pkg.inf", "-h"),
            ] {
                assert_infverif_args_rejected(
                    value,
                    &format!(
                        "cargo-wdk derives the mode flag `{mode_flag}` from the \
                         `--target-platform` option"
                    ),
                );
            }
        }

        #[test]
        fn stampinf_args_allows_args_not_reserved_by_cargo_wdk() {
            let args = parse_build_args(&[
                "--stampinf-args",
                "-d 01/01/2026 /v 1.2.3.4 -p \"Contoso Ltd\"",
            ])
            .expect("args should parse");
            assert_eq!(
                args.stampinf_args().expect("should resolve"),
                Some(vec![
                    "-d".to_string(),
                    "01/01/2026".to_string(),
                    "/v".to_string(),
                    "1.2.3.4".to_string(),
                    "-p".to_string(),
                    "Contoso Ltd".to_string(),
                ])
            );
        }

        #[test]
        fn infverif_args_rejects_inf_paths() {
            for value in [
                "extra.inf",
                "/info C:\\pkg\\other.INF",
                "-info C:\\pkg\\other.INF",
            ] {
                assert_infverif_args_rejected(value, "cargo-wdk supplies the INF file path itself");
            }
        }

        #[test]
        fn infverif_args_allows_other_args() {
            for (value, expected) in [
                (
                    "/rulever 10.0.22621 -info",
                    ["/rulever", "10.0.22621", "-info"],
                ),
                (
                    "-rulever 10.0.22621 /info",
                    ["-rulever", "10.0.22621", "/info"],
                ),
            ] {
                let args =
                    parse_build_args(&["--infverif-args", value]).expect("args should parse");
                assert_eq!(
                    args.infverif_args().expect("should resolve"),
                    Some(expected.map(str::to_string).to_vec()),
                    "unexpected args for {value:?}"
                );
            }
        }
    }

    mod parse_passthrough_args {
        use super::super::parse_passthrough_args;

        #[test]
        fn tokenizes_whitespace_separated_args() {
            let parsed = parse_passthrough_args("/fd SHA384 /f cert.pfx").expect("should parse");
            assert_eq!(parsed.0, vec!["/fd", "SHA384", "/f", "cert.pfx"]);
        }

        #[test]
        fn preserves_quoted_spans() {
            let parsed =
                parse_passthrough_args("/n \"CN=Contoso Root\" /fd SHA256").expect("should parse");
            assert_eq!(parsed.0, vec!["/n", "CN=Contoso Root", "/fd", "SHA256"]);
        }

        #[test]
        fn rejects_unterminated_quote() {
            let err = parse_passthrough_args("/n \"CN=Contoso")
                .expect_err("unterminated quote should be rejected");
            assert!(
                err.contains(
                    "unterminated `\"` quote in passthrough arguments; make sure every quote is \
                     closed"
                ),
                "unexpected error: {err}"
            );
        }

        #[test]
        fn treats_empty_or_whitespace_as_no_args() {
            for value in ["", "   ", "\t"] {
                let parsed = parse_passthrough_args(value).expect("should parse");
                assert!(
                    parsed.0.is_empty(),
                    "value {value:?} should parse to no args"
                );
            }
        }

        #[test]
        fn preserves_quoted_empty_args() {
            for value in ["/p \"\"", "/p ''"] {
                let parsed = parse_passthrough_args(value).expect("should parse");
                assert_eq!(parsed.0, vec!["/p", ""]);
            }
        }
    }

    #[test]
    fn target_platform_arg_maps_to_target_platform() {
        use crate::{actions::build::TargetPlatform, cli::TargetPlatformArg};

        assert_eq!(
            TargetPlatform::from(TargetPlatformArg::Universal),
            TargetPlatform::Universal
        );
        assert_eq!(
            TargetPlatform::from(TargetPlatformArg::Desktop),
            TargetPlatform::Desktop
        );
        assert_eq!(
            TargetPlatform::from(TargetPlatformArg::Windows),
            TargetPlatform::Windows
        );
    }
}
