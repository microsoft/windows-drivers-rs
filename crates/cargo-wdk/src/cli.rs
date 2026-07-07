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
    build::{
        BuildAction,
        BuildActionParams,
        CertSource,
        FileDigestAlgorithm,
        SecretString,
        SignMode,
        SignOptions,
    },
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
#[command(group = ArgGroup::new("cert_source").args(["cert_store", "cert_file"]).multiple(false))]
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

    /// File digest algorithm passed to signtool (`/fd`, and `/td` when a
    /// timestamp server is used).
    #[arg(
        long,
        value_enum,
        ignore_case = true,
        default_value_t = FileDigestAlgorithm::Sha256,
        help_heading = "Driver Signing"
    )]
    pub file_digest_algorithm: FileDigestAlgorithm,

    /// Certificate store name to select the signing certificate from. Must be
    /// used together with `--cert-name`.
    #[arg(
        long,
        value_name = "STORE",
        requires = "cert_name",
        help_heading = "Driver Signing"
    )]
    pub cert_store: Option<String>,

    /// Subject name of the certificate to select from the store. Must be used
    /// together with `--cert-store`.
    #[arg(
        long,
        value_name = "NAME",
        requires = "cert_store",
        help_heading = "Driver Signing"
    )]
    pub cert_name: Option<String>,

    /// Path to a PFX certificate file to sign with. Mutually exclusive with
    /// `--cert-store`/`--cert-name`.
    #[arg(
        long,
        value_name = "PATH",
        value_parser = existing_file,
        help_heading = "Driver Signing"
    )]
    pub cert_file: Option<PathBuf>,

    /// Name of the environment variable holding the PFX password. The password
    /// is read from the environment (never passed as a plaintext CLI value).
    /// Must be used together with `--cert-file`.
    #[arg(
        long,
        value_name = "ENV_VAR",
        requires = "cert_file",
        help_heading = "Driver Signing"
    )]
    pub cert_password_env: Option<String>,

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
    /// Returns `true` if any certificate or timestamp signing option was
    /// provided on the command line. The defaulted `--file-digest-algorithm`
    /// is intentionally not counted (it is ignored when nothing is signed).
    const fn has_signing_options(&self) -> bool {
        self.cert_store.is_some()
            || self.cert_name.is_some()
            || self.cert_file.is_some()
            || self.cert_password_env.is_some()
    }

    /// Resolves the certificate selection from the (already clap-validated)
    /// certificate flags, reading the PFX password from the environment when
    /// requested.
    fn cert_source(&self) -> Result<CertSource, clap::Error> {
        match (&self.cert_file, &self.cert_store, &self.cert_name) {
            (Some(path), ..) => {
                let password = match &self.cert_password_env {
                    Some(var) => Some(resolve_password_env(var)?),
                    None => None,
                };
                std::result::Result::Ok(CertSource::File {
                    path: path.clone(),
                    password,
                })
            }
            (None, Some(store), Some(name)) => std::result::Result::Ok(CertSource::Store {
                store: store.clone(),
                name: name.clone(),
            }),
            _ => std::result::Result::Ok(CertSource::AutoTestCert),
        }
    }
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
                if args.has_signing_options() {
                    return Err(build_error(
                        "Signing options (certificate/timestamp) cannot be used with \
                         `--sign-mode=off`; nothing would be signed.",
                    ));
                }
                std::result::Result::Ok(Self::Off)
            }
            SignModeArg::Test => std::result::Result::Ok(Self::Test {
                verify_signature: args.verify_signature,
                options: SignOptions {
                    cert: args.cert_source()?,
                    file_digest_algorithm: args.file_digest_algorithm,
                },
            }),
        }
    }
}

/// `clap` value parser that accepts a path only if it points at an existing
/// file.
fn existing_file(value: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if path.is_file() {
        std::result::Result::Ok(path)
    } else {
        Err(format!("file does not exist: {value}"))
    }
}

/// Reads the PFX password from the named environment variable, wrapping it in a
/// redacting [`SecretString`]. Returns a `clap::Error` when the variable is
/// unset or empty.
fn resolve_password_env(var: &str) -> Result<SecretString, clap::Error> {
    match std::env::var(var) {
        std::result::Result::Ok(value) if !value.is_empty() => {
            std::result::Result::Ok(SecretString::new(value))
        }
        _ => Err(build_error(format!(
            "environment variable `{var}` referenced by `--cert-password-env` is unset or empty"
        ))),
    }
}

/// Builds a `clap::Error` with the given message, rendered with the standard
/// `cargo wdk build` usage for a consistent CLI experience.
fn build_error(message: impl std::fmt::Display) -> clap::Error {
    Cli::command().error(ErrorKind::ArgumentConflict, message)
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
        actions::{
            DriverType,
            build::{CertSource, FileDigestAlgorithm, SignMode, SignOptions},
        },
        cli::{BuildArgs, Cli, NewArgs, Subcmd},
    };

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
        let args =
            parse_build_args(&["--sign-mode", "off", "--verify-signature"]).expect("args parse");
        let err = SignMode::try_from(&args).expect_err("should be rejected");
        assert!(
            err.to_string()
                .contains("`--verify-signature` cannot be used with `--sign-mode=off`."),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn build_rejects_signing_options_with_sign_mode_off() {
        let args = parse_build_args(&[
            "--sign-mode",
            "off",
            "--cert-store",
            "S",
            "--cert-name",
            "N",
        ])
        .expect("args parse");
        let err = SignMode::try_from(&args).expect_err("should be rejected");
        assert!(
            err.to_string().contains("`--sign-mode=off`"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn build_rejects_cert_file_with_store_and_name() {
        assert!(
            parse_build_args(&[
                "--cert-file",
                "cert.pfx",
                "--cert-store",
                "MyStore",
                "--cert-name",
                "MyCert",
            ])
            .is_err()
        );
    }

    #[test]
    fn build_rejects_cert_store_without_cert_name() {
        assert!(parse_build_args(&["--cert-store", "MyStore"]).is_err());
    }

    #[test]
    fn build_rejects_cert_name_without_cert_store() {
        assert!(parse_build_args(&["--cert-name", "MyCert"]).is_err());
    }

    #[test]
    fn build_rejects_cert_password_env_without_cert_file() {
        assert!(parse_build_args(&["--cert-password-env", "PFX_PW"]).is_err());
    }

    #[test]
    fn build_rejects_invalid_file_digest_algorithm() {
        assert!(parse_build_args(&["--file-digest-algorithm", "MD5"]).is_err());
    }

    #[test]
    fn build_rejects_missing_cert_file_path() {
        assert!(parse_build_args(&["--cert-file", "definitely_not_a_real_file.pfx"]).is_err());
    }

    #[test]
    fn build_maps_store_cert_and_digest() {
        let args = parse_build_args(&[
            "--sign-mode",
            "test",
            "--cert-store",
            "MyStore",
            "--cert-name",
            "MyCert",
            "--file-digest-algorithm",
            "SHA384",
        ])
        .expect("args should parse");

        assert_eq!(
            SignMode::try_from(&args).expect("mapping should succeed"),
            SignMode::Test {
                verify_signature: false,
                options: SignOptions {
                    cert: CertSource::Store {
                        store: "MyStore".to_string(),
                        name: "MyCert".to_string(),
                    },
                    file_digest_algorithm: FileDigestAlgorithm::Sha384,
                },
            }
        );
    }

    #[test]
    fn build_maps_pfx_cert_with_password_env() {
        let cert = assert_fs::NamedTempFile::new("cert.pfx").expect("temp file");
        std::fs::write(cert.path(), b"pfx").expect("write temp cert");
        let cert_path = cert.path().to_path_buf();

        let args = parse_build_args(&[
            "--cert-file",
            cert_path.to_str().expect("utf8 path"),
            "--cert-password-env",
            "CARGO_WDK_TEST_PFX_PW",
        ])
        .expect("args should parse");

        let sign_mode =
            crate::test_utils::with_env(&[("CARGO_WDK_TEST_PFX_PW", Some("secret"))], || {
                SignMode::try_from(&args)
            })
            .expect("mapping should succeed");

        assert_eq!(
            sign_mode,
            SignMode::Test {
                verify_signature: false,
                options: SignOptions {
                    cert: CertSource::File {
                        path: cert_path,
                        password: Some(crate::actions::build::SecretString::new(
                            "secret".to_string()
                        )),
                    },
                    file_digest_algorithm: FileDigestAlgorithm::Sha256,
                },
            }
        );
    }

    #[test]
    fn build_rejects_unset_cert_password_env() {
        let cert = assert_fs::NamedTempFile::new("cert.pfx").expect("temp file");
        std::fs::write(cert.path(), b"pfx").expect("write temp cert");

        let args = parse_build_args(&[
            "--cert-file",
            cert.path().to_str().expect("utf8 path"),
            "--cert-password-env",
            "CARGO_WDK_TEST_UNSET_PW",
        ])
        .expect("args should parse");

        let result =
            crate::test_utils::with_env(&[("CARGO_WDK_TEST_UNSET_PW", None::<&str>)], || {
                SignMode::try_from(&args)
            });
        assert!(result.is_err(), "unset password env should be rejected");
    }

    #[test]
    fn build_off_mode_maps_to_off() {
        let args = parse_build_args(&["--sign-mode", "off"]).expect("args should parse");
        assert_eq!(
            SignMode::try_from(&args).expect("mapping should succeed"),
            SignMode::Off
        );
    }
}
