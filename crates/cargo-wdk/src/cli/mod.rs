mod args;

use anyhow::Ok;
use args::{NewProjectArgs, PackageProjectArgs};
use clap::{Parser, Subcommand};

use crate::{
    actions::{new::NewAction, package::PackageAction},
    providers::{exec::CommandExec, fs, wdk_build},
};

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

#[derive(Debug, Subcommand)]
pub enum Subcmd {
    #[clap(name = "new", about = "Create a new Windows Driver Kit project")]
    New(NewProjectArgs),
    #[clap(name = "build", about = "Build the Windows Driver Kit project")]
    Build(PackageProjectArgs),
}

impl Cli {
    pub fn run(self) -> anyhow::Result<()> {
        let wdk_build = wdk_build::WdkBuild {};
        let command_exec = CommandExec {};
        let fs_provider = fs::FS {};

        match self.sub_cmd {
            Subcmd::New(cli_args) => {
                let mut new_action = NewAction::new(
                    cli_args.driver_project_name,
                    cli_args.driver_type.into(),
                    cli_args.cwd,
                    &command_exec,
                )?;
                new_action.create_new_project()
            }
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
