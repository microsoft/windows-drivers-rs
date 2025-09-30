// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! This module defines the top-level CLI layer, its argument types and
//! structures used for parsing and validating arguments for various
//! subcommands.
use std::path::{Path, PathBuf};

use anyhow::{Ok, Result};
use clap::{ArgGroup, Args, Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use mockall_double::double;
use wdk_build::CpuArchitecture;

use crate::actions::{
    DriverType,
    KMDF_STR,
    Profile,
    TargetArch,
    UMDF_STR,
    WDM_STR,
    build::{BuildAction, BuildActionParams},
    new::NewAction,
};
#[double]
use crate::providers::{exec::CommandExec, fs::Fs, metadata::Metadata, wdk_build::WdkBuild};

const ABOUT_STRING: &str = "cargo-wdk is a cargo extension that can be used to create and build \
                            Windows Rust driver projects.";
const CARGO_WDK_BIN_NAME: &str = "cargo wdk";

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

    /// Verify the signature
    #[arg(long)]
    pub verify_signature: bool,

    /// Build Sample Class Driver Project
    #[arg(long)]
    pub sample: bool,
}

/// Subcommands
#[derive(Debug, Subcommand)]
pub enum Subcmd {
    #[clap(name = "new", about = "Create a new Windows Driver Kit project")]
    New(NewArgs),
    #[clap(name = "build", about = "Build the Windows Driver Kit project")]
    Build(BuildArgs),
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
                let target_arch = if let Some(arch) = cli_args.target_arch {
                    TargetArch::Selected(arch)
                } else {
                    // Detect the default target architecture using rustc
                    let detected_arch =
                        Self::detect_default_target_arch_using_rustc(&command_exec)?;
                    TargetArch::Default(detected_arch)
                };
                BuildAction::new(
                    &BuildActionParams {
                        working_dir: Path::new("."), // Using current dir as working dir
                        profile: cli_args.profile.as_ref(),
                        target_arch,
                        verify_signature: cli_args.verify_signature,
                        is_sample_class: cli_args.sample,
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
        }
    }

    /// Returns the default architecture of the host machine by running `rustc
    /// --print host-tuple` command.
    ///
    /// # Arguments
    /// * `command_exec` - A reference to the `CommandExec` struct that provides
    ///   methods for executing commands.
    /// # Returns
    /// * `CpuArchitecture`
    /// * `anyhow::Error` if the command fails to execute or the output is not
    ///   in the expected format.
    fn detect_default_target_arch_using_rustc(
        command_exec: &CommandExec,
    ) -> Result<CpuArchitecture> {
        command_exec
            .run("rustc", &["--print", "host-tuple"], None, None)
            .map_or_else(
                |e| Err(anyhow::anyhow!("Unable to read rustc host tuple: {e}")),
                |output| {
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    match stdout.trim() {
                        "x86_64-pc-windows-msvc" => Ok(CpuArchitecture::Amd64),
                        "aarch64-pc-windows-msvc" => Ok(CpuArchitecture::Arm64),
                        _ => Err(anyhow::anyhow!(
                            "Unsupported default target: {}. Only x86_64-pc-windows-msvc and \
                             aarch64-pc-windows-msvc are supported.\n Make sure you're on Windows \
                             and switch the default target to one of the above two using \
                             `rustup.exe`. You can also use the --target-arch option to \
                             explicitly specify a CPU architecture instead of relying on the \
                             default target.",
                            stdout
                        )),
                    }
                },
            )
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::ref_option_ref)] // This is suppressed for mockall as it generates mocks with env_vars: &Option
    use std::{
        collections::HashMap,
        process::{ExitStatus, Output},
    };

    use mockall_double::double;
    use wdk_build::CpuArchitecture;

    #[double]
    use crate::providers::exec::CommandExec;
    use crate::{
        actions::DriverType,
        cli::{Cli, NewArgs},
    };

    #[test]
    pub fn arch_detection_works_for_supported_toolchains() {
        fn run_test(toolchain: &str, arch: CpuArchitecture) {
            let result = run_arch_detection(Ok(Output {
                status: ExitStatus::default(),
                stdout: toolchain.bytes().collect::<Vec<_>>(),
                stderr: vec![],
            }));

            assert!(result.is_ok());
            assert_eq!(result.unwrap(), arch);
        }

        run_test("x86_64-pc-windows-msvc", CpuArchitecture::Amd64);
        run_test("aarch64-pc-windows-msvc", CpuArchitecture::Arm64);
    }

    #[test]
    pub fn arch_detection_fails_for_unsupported_toolchains() {
        fn run_test(toolchain: &str) {
            let result = run_arch_detection(Ok(Output {
                status: ExitStatus::default(),
                stdout: toolchain.bytes().collect::<Vec<_>>(),
                stderr: vec![],
            }));

            assert!(result.is_err());
            assert_eq!(
                result.err().unwrap().to_string(),
                format!(
                    "Unsupported default target: {toolchain}. Only x86_64-pc-windows-msvc and \
                     aarch64-pc-windows-msvc are supported.\n Make sure you're on Windows and \
                     switch the default target to one of the above two using `rustup.exe`. You \
                     can also use the --target-arch option to explicitly specify a CPU \
                     architecture instead of relying on the default target."
                )
            );
        }

        run_test("i686-pc-windows-msvc");
        run_test("x86_64-win7-windows-msvc");
    }

    #[test]
    pub fn arch_detection_fails_if_rustc_fails() {
        let result =
            run_arch_detection(Err(crate::providers::error::CommandError::CommandFailed {
                command: "rustc".to_string(),
                args: vec!["--print".to_string(), "host-tuple".to_string()],
                stdout: "command error".to_string(),
            }));

        assert!(result.is_err());
        assert_eq!(
            result.err().unwrap().to_string(),
            "Unable to read rustc host tuple: Command 'rustc' with args [\"--print\", \
             \"host-tuple\"] failed \n STDOUT: command error",
        );
    }

    pub fn run_arch_detection(
        expected_cli_result: Result<Output, crate::providers::error::CommandError>,
    ) -> core::result::Result<CpuArchitecture, anyhow::Error> {
        let mut mock_command_exec = CommandExec::default();

        let expected_rustc_command = "rustc";
        let expected_rustc_args = vec!["--print", "host-tuple"];

        mock_command_exec
            .expect_run()
            .withf(
                move |command: &str,
                      args: &[&str],
                      _env_vars: &Option<&HashMap<&str, &str>>,
                      _working_dir: &Option<&std::path::Path>|
                      -> bool {
                    println!("command: {command}, args: {args:?}");
                    println!(
                        "expected_command: {expected_rustc_command}, expected_args: \
                         {expected_rustc_args:?}"
                    );
                    command == expected_rustc_command && args == expected_rustc_args
                },
            )
            .once()
            .return_once(|_, _, _, _| expected_cli_result);

        Cli::detect_default_target_arch_using_rustc(&mock_command_exec)
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
}
