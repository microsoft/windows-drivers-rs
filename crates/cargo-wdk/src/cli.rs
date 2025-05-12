// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! This module defines the top-level CLI layer, its argument types and
//! structures used for parsing and validating arguments for various
//! subcommands.
use std::{path::PathBuf, str::FromStr};

use anyhow::{Ok, Result};
use clap::{Args, Parser, Subcommand};
use mockall_double::double;
use wdk_build::{CpuArchitecture, DriverConfig};

use crate::actions::{
    build::{BuildAction, BuildActionParams},
    new::NewAction,
    Profile,
    TargetArch,
};
#[double]
use crate::providers::{exec::CommandExec, fs::Fs, metadata::Metadata, wdk_build::WdkBuild};

/// Validation errors for the driver project name arg passed to new project sub
/// command
#[derive(Debug, thiserror::Error)]
pub enum InvalidDriverProjectNameError {
    #[error("Project name cannot be empty")]
    EmptyProjectNameError,
    #[error("Project name can only contain alphanumeric characters, hyphens, and underscores")]
    NonAlphanumericProjectNameError,
    #[error("Project name must start with an alphabetic character")]
    InvalidStartCharacter,
    #[error("'{0}' is a reserved keyword or invalid name and cannot be used as a project name")]
    ReservedName(String),
}

/// Type for Driver Project Name Argument
#[derive(Debug, Clone)]
pub struct ProjectNameArg(pub String);

impl FromStr for ProjectNameArg {
    type Err = InvalidDriverProjectNameError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(InvalidDriverProjectNameError::EmptyProjectNameError);
        }
        if !s
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(InvalidDriverProjectNameError::InvalidStartCharacter);
        }
        if !s
            .chars()
            .next()
            .expect("Project name cannot be empty")
            .is_alphabetic()
        {
            return Err(InvalidDriverProjectNameError::NonAlphanumericProjectNameError);
        }
        let invalid_names = ["crate", "self", "super", "extern", "_", "-", "new", "build"];
        if invalid_names.contains(&s) {
            return Err(InvalidDriverProjectNameError::ReservedName(s.to_string()));
        }
        std::result::Result::Ok(Self(s.to_string()))
    }
}

/// Arguments for the `new` subcommand
#[derive(Debug, Args)]
pub struct NewProjectArgs {
    #[clap(help = "Driver Project Name")]
    pub driver_project_name: ProjectNameArg,
    #[clap(help = "Driver Type", index = 2, ignore_case = true)]
    pub driver_type: DriverConfig,
    #[clap(long, help = "Path to the project", default_value = ".")]
    pub cwd: PathBuf,
}

/// Arguments for the `build` subcommand
#[derive(Debug, Args)]
pub struct BuildProjectArgs {
    #[clap(long, help = "Path to the project", default_value = ".")]
    pub cwd: PathBuf,
    #[clap(long, help = "Build Profile/Configuration", ignore_case = true)]
    pub profile: Option<Profile>,
    #[clap(long, help = "Build Target", ignore_case = true)]
    pub target_arch: Option<CpuArchitecture>,
    #[clap(long, help = "Verify Signatures", default_value = "false")]
    pub verify_signature: bool,
    #[clap(long, help = "Sample Class", default_value = "false")]
    pub sample_class: bool,
}

/// Subcommands
#[derive(Debug, Subcommand)]
pub enum Subcmd {
    #[clap(name = "new", about = "Create a new Windows Driver Kit project")]
    New(NewProjectArgs),
    #[clap(name = "build", about = "Build the Windows Driver Kit project")]
    Build(BuildProjectArgs),
}

/// Top level command line interface for cargo wdk
#[derive(Debug, Parser)]
#[clap(
    name = "cargo wdk",
    version = "0.0.1",
    author = "Microsoft",
    about = "A tool for building Windows Driver Kit Rust projects",
    override_usage = "cargo wdk [SUBCOMMAND] [OPTIONS]"
)]
pub struct Cli {
    #[clap(name = "cargo command", default_value = "wdk")]
    pub cargo_command: String,
    #[clap(subcommand)]
    pub sub_cmd: Subcmd,
    #[command(flatten)]
    pub verbose: clap_verbosity_flag::Verbosity,
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
                let new_action = NewAction::new(
                    &cli_args.driver_project_name.0,
                    cli_args.driver_type,
                    &cli_args.cwd,
                    self.verbose,
                    &command_exec,
                    &fs,
                );
                new_action.run()?;
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
                let build_action = BuildAction::new(
                    &BuildActionParams {
                        working_dir: &cli_args.cwd,
                        profile: cli_args.profile.as_ref(),
                        target_arch,
                        verify_signature: cli_args.verify_signature,
                        is_sample_class: cli_args.sample_class,
                        verbosity_level: self.verbose,
                    },
                    &wdk_build,
                    &command_exec,
                    &fs,
                    &metadata,
                )?;
                build_action.run()?;
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
            .run("rustc", &["--print", "host-tuple"], None)
            .map_or_else(
                |e| Err(anyhow::anyhow!("Unable to read rustc host tuple: {e}")),
                |output| {
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    let stdout = stdout.trim();
                    match stdout.split_once('-') {
                        Some(("x86_64", _)) => Ok(CpuArchitecture::Amd64),
                        Some(("aarch64", _)) => Ok(CpuArchitecture::Arm64),
                        Some((..)) => Err(anyhow::anyhow!(
                            "CPU Architecture of the host is not supported: {} \n Please try \
                             selecting target by passing --target-arch option",
                            stdout
                        )),
                        None => Err(anyhow::anyhow!(
                            "Invalid format for host architecture: {}",
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

    use crate::cli::Cli;
    #[double]
    use crate::providers::exec::CommandExec;

    #[test]
    pub fn given_toolchain_host_tuple_is_x86_64_when_detect_default_arch_from_rustc_is_called_then_it_returns_arch(
    ) {
        let mut mock_command_exec = CommandExec::default();

        let expected_rustc_command = "rustc";
        let expected_rustc_args = vec!["--print", "host-tuple"];

        mock_command_exec
            .expect_run()
            .withf(
                move |command: &str,
                      args: &[&str],
                      _env_vars: &Option<&HashMap<&str, &str>>|
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
            .returning(move |_, _, _| {
                Ok(Output {
                    status: ExitStatus::default(),
                    stdout: b"x86_64-pc-windows-msvc".to_vec(),
                    stderr: vec![],
                })
            });

        let result = Cli::detect_default_target_arch_using_rustc(&mock_command_exec);

        assert_eq!(result.unwrap(), CpuArchitecture::Amd64);
    }

    #[test]
    pub fn given_toolchain_host_tuple_is_aarch64_when_detect_default_arch_from_rustc_is_called_then_it_returns_arch(
    ) {
        let mut mock_command_exec = CommandExec::default();

        let expected_rustc_command = "rustc";
        let expected_rustc_args = vec!["--print", "host-tuple"];

        mock_command_exec
            .expect_run()
            .withf(
                move |command: &str,
                      args: &[&str],
                      _env_vars: &Option<&HashMap<&str, &str>>|
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
            .returning(move |_, _, _| {
                Ok(Output {
                    status: ExitStatus::default(),
                    stdout: b"aarch64-pc-windows-msvc".to_vec(),
                    stderr: vec![],
                })
            });

        let result = Cli::detect_default_target_arch_using_rustc(&mock_command_exec);

        assert_eq!(result.unwrap(), CpuArchitecture::Arm64);
    }

    #[test]
    pub fn given_toolchain_host_tuple_is_unsupported_when_detect_default_arch_from_rustc_is_called_then_it_returns_error(
    ) {
        let mut mock_command_exec = CommandExec::default();

        let expected_rustc_command = "rustc";
        let expected_rustc_args = vec!["--print", "host-tuple"];

        mock_command_exec
            .expect_run()
            .withf(
                move |command: &str,
                      args: &[&str],
                      _env_vars: &Option<&HashMap<&str, &str>>|
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
            .returning(move |_, _, _| {
                Ok(Output {
                    status: ExitStatus::default(),
                    stdout: b"i686-pc-windows-msvc".to_vec(),
                    stderr: vec![],
                })
            });

        let result = Cli::detect_default_target_arch_using_rustc(&mock_command_exec);

        assert_eq!(
            result.err().unwrap().to_string(),
            format!(
                "CPU Architecture of the host is not supported: {} \n Please try selecting target \
                 by passing --target-arch option",
                "i686-pc-windows-msvc"
            )
        );
    }

    #[test]
    pub fn given_toolchain_host_tuple_is_invalid_when_detect_default_arch_from_rustc_is_called_then_it_returns_error(
    ) {
        let mut mock_command_exec = CommandExec::default();

        let expected_rustc_command = "rustc";
        let expected_rustc_args = vec!["--print", "host-tuple"];

        mock_command_exec
            .expect_run()
            .withf(
                move |command: &str,
                      args: &[&str],
                      _env_vars: &Option<&HashMap<&str, &str>>|
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
            .returning(move |_, _, _| {
                Ok(Output {
                    status: ExitStatus::default(),
                    stdout: b"somerandomvalue".to_vec(),
                    stderr: vec![],
                })
            });

        let result = Cli::detect_default_target_arch_using_rustc(&mock_command_exec);

        assert_eq!(
            result.err().unwrap().to_string(),
            format!(
                "Invalid format for host architecture: {}",
                "somerandomvalue"
            )
        );
    }

    #[test]
    pub fn given_rustc_command_fails_when_detect_default_arch_from_rustc_is_called_then_it_returns_error(
    ) {
        let mut mock_command_exec = CommandExec::default();

        let expected_rustc_command = "rustc";
        let expected_rustc_args = vec!["--print", "host-tuple"];

        mock_command_exec
            .expect_run()
            .withf(
                move |command: &str,
                      args: &[&str],
                      _env_vars: &Option<&HashMap<&str, &str>>|
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
            .returning(move |_, _, _| {
                Err(crate::providers::error::CommandError::CommandFailed {
                    command: "rustc".to_string(),
                    args: vec!["--print".to_string(), "host-tuple".to_string()],
                    stdout: "command error".to_string(),
                })
            });

        let result = Cli::detect_default_target_arch_using_rustc(&mock_command_exec);

        assert_eq!(
            result.err().unwrap().to_string(),
            format!(
                "Unable to read rustc host tuple: Command 'rustc' with args [\"--print\", \
                 \"host-tuple\"] failed \n STDOUT: {}",
                "command error"
            )
        );
    }
}
