use std::path::PathBuf;

use anyhow::Ok;
use clap::{Args, Parser, Subcommand};
use crate::actions::package::PackageAction;
use crate::actions::new::NewAction;
use crate::providers::exec::CommandExec;
use crate::providers::{fs, wdk_build};

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

impl Cli {
    pub fn run(self) -> anyhow::Result<()> {
        let wdk_build = wdk_build::WdkBuild{};
        let command_exec= CommandExec{};
        let fs_provider = fs::FS{};

        match self.sub_cmd {
            Subcmd::New(cli_args) => {
                let mut new_action = NewAction::new(cli_args.driver_project_name, cli_args.driver_type, cli_args.wdk_version, cli_args.cwd, &command_exec)?;
                new_action.create_new_project()
            }
            Subcmd::Build(cli_args) => {
                let package_action = PackageAction::new(cli_args.cwd, 
                    cli_args.profile, 
                    cli_args.target_arch, 
                    cli_args.sample_class, 
                    self.verbose,
                    &wdk_build,
                    &command_exec,
                    &fs_provider
                )?;
                package_action.run()?;
                Ok(())
            }
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum Subcmd {
    #[clap(name = "new", about = "Create a new Windows Driver Kit project")]
    New(NewProjectArgs),
    #[clap(name = "build", about = "Build the Windows Driver Kit project")]
    Build(PackageProjectArgs)
}

// TODO: Implement workspaces
#[derive(Debug, Args)]
pub struct NewProjectArgs {
    #[clap(help = "Driver Project Name", default_value = "")]
    pub driver_project_name: String,

    #[clap(long, help = "Driver Type", index = 2)]
    pub driver_type: String,

    #[clap(long, help = "Windows Driver Kit Version", default_value = "*")]
    pub wdk_version: String,

    #[clap(long, help = "Path to the project", default_value = ".")]
    pub cwd: PathBuf,

    #[clap(long, help = "Device class", default_value = "Sample")]
    pub device_class: String,
}

#[derive(Debug, Args)]
pub struct PackageProjectArgs {
    #[clap(long, help = "Path to the project", default_value = ".")]
    pub cwd: PathBuf,

    #[clap(long, help = "Build Profile/Configuration", default_value = "debug")]
    pub profile: String,

    #[clap(long, help = "Build Target", default_value = "x86_64")]
    pub target_arch: String,

    // TODO: Deal with non-sample classes
    #[clap(long, help = "Sample Class", default_value = "true")]
    pub sample_class: bool,
}
