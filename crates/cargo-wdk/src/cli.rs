//! This module defines the top-level CLI layer, its argument types and
//! structures used for parsing and validating arguments for various
//! subcommands.
use std::{path::PathBuf, str::FromStr};

use anyhow::{Ok, Result};
use clap::{Args, Parser, Subcommand};
use mockall_double::double;
use wdk_build::{CpuArchitecture, DriverConfig};

use crate::actions::{
    new::NewAction,
    package::{PackageAction, PackageActionParams},
    Profile,
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
        let fs_provider = Fs::default();
        let metadata = Metadata::default();

        match self.sub_cmd {
            Subcmd::New(cli_args) => {
                let new_action = NewAction::new(
                    &cli_args.driver_project_name.0,
                    cli_args.driver_type,
                    &cli_args.cwd,
                    self.verbose,
                    &command_exec,
                    &fs_provider,
                );
                new_action.run()?;
                Ok(())
            }
            Subcmd::Build(cli_args) => {
                let package_action = PackageAction::new(
                    &PackageActionParams {
                        working_dir: &cli_args.cwd,
                        profile: cli_args.profile.as_ref(),
                        target_arch: cli_args.target_arch.as_ref(),
                        verify_signature: cli_args.verify_signature,
                        is_sample_class: cli_args.sample_class,
                        verbosity_level: self.verbose,
                    },
                    &wdk_build,
                    &command_exec,
                    &fs_provider,
                    &metadata,
                )?;
                package_action.run()?;
                Ok(())
            }
        }
    }
}
