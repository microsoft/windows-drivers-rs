mod args;
mod error;

use anyhow::{Ok, Result};
use args::{NewProjectArgs, PackageProjectArgs};
use clap::{Parser, Subcommand};

use crate::{
    actions::{new::NewAction, package::PackageAction},
    providers::{exec::CommandExec, fs::FS, wdk_build},
};

/// Top level command line interface for cargo wdk
#[derive(Debug, Parser)]
#[clap(
    name = "cargo wdk",
    version = "0.0.1",
    author = "Rust for Drivers",
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

/// Subcommands for wdk
#[derive(Debug, Subcommand)]
pub enum Subcmd {
    #[clap(name = "new", about = "Create a new Windows Driver Kit project")]
    New(NewProjectArgs),
    #[clap(name = "build", about = "Build the Windows Driver Kit project")]
    Build(PackageProjectArgs),
}

impl Cli {
    /// Entry point method to construct and call actions based on the subcommand
    /// and arguments provided by the user.
    pub fn run(self) -> Result<()> {
        let wdk_build = wdk_build::WdkBuild {};
        let command_exec = CommandExec {};
        let fs_provider = FS {};

        match self.sub_cmd {
            Subcmd::New(cli_args) => NewAction::new(
                &cli_args.driver_project_name.0,
                cli_args.driver_type.into(),
                cli_args.cwd,
                &command_exec,
                &fs_provider,
            )?
            .run(),
            Subcmd::Build(cli_args) => {
                let package_action = PackageAction::new(
                    cli_args.cwd,
                    cli_args.profile.into(),
                    cli_args.target_arch.into(),
                    cli_args.sample_class,
                    self.verbose,
                    &wdk_build,
                    &command_exec,
                    &fs_provider,
                )?;
                package_action.run()?;
                Ok(())
            }
        }
    }
}
