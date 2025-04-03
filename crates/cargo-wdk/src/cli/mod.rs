mod args;
mod error;

use anyhow::{Ok, Result};
use args::{NewProjectArgs, PackageProjectArgs};
use clap::{Parser, Subcommand};
use error::UnsupportedHostArchError;
use mockall_double::double;
use tracing::debug;

use crate::actions::{
    new::NewAction,
    package::{PackageAction, PackageActionParams},
    CpuArchitecture,
};
#[double]
use crate::providers::{exec::CommandExec, fs::Fs, metadata::Metadata, wdk_build::WdkBuild};

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
        let wdk_build = WdkBuild::default();
        let command_exec = CommandExec::default();
        let fs_provider = Fs::default();
        let metadata = Metadata::default();

        match self.sub_cmd {
            Subcmd::New(cli_args) => {
                let new_action = NewAction::new(
                    &cli_args.driver_project_name.0,
                    cli_args.driver_type.into(),
                    &cli_args.cwd,
                    &command_exec,
                    &fs_provider,
                );
                new_action.run()?;
                Ok(())
            }
            Subcmd::Build(cli_args) => {
                let host_arch = detect_host_architecture()?;
                let package_action = PackageAction::new(
                    &PackageActionParams {
                        working_dir: &cli_args.cwd,
                        profile: cli_args.profile.into(),
                        host_arch,
                        target_arch: cli_args.target_arch,
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

fn detect_host_architecture() -> Result<CpuArchitecture> {
    debug!("Detecting host architecture from ARCH environment variable");
    let host_arch_str = std::env::consts::ARCH;
    let host_arch = match host_arch_str {
        "x86_64" => {
            debug!("Host architecture is x86_64");
            CpuArchitecture::Amd64
        }
        "aarch64" => {
            debug!("Host architecture is aarch64");
            CpuArchitecture::Arm64
        }
        _ => {
            return Err(UnsupportedHostArchError {
                arch: host_arch_str.to_string(),
            }
            .into());
        }
    };
    Ok(host_arch)
}
