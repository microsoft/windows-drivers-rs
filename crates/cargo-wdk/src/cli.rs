use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use crate::actions::new::NewAction;
use crate::actions::package::PackageAction;
use crate::providers::exec::CommandExec;
use crate::providers::{fs, wdk_build};
use anyhow::Ok;
use clap::{Args, Parser, Subcommand};

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
        let wdk_build = wdk_build::WdkBuild {};
        let command_exec = CommandExec {};
        let fs_provider = fs::FS {};

        match self.sub_cmd {
            Subcmd::New(cli_args) => {
                let mut new_action = NewAction::new(
                    cli_args.driver_project_name,
                    cli_args.driver_type,
                    cli_args.cwd,
                    &command_exec,
                )?;
                new_action.create_new_project()
            }
            Subcmd::Build(cli_args) => {
                let package_action = PackageAction::new(
                    cli_args.cwd,
                    cli_args.profile,
                    cli_args.target_arch,
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

#[derive(Debug, Subcommand)]
pub enum Subcmd {
    #[clap(name = "new", about = "Create a new Windows Driver Kit project")]
    New(NewProjectArgs),
    #[clap(name = "build", about = "Build the Windows Driver Kit project")]
    Build(PackageProjectArgs),
}

#[derive(Debug, Clone)]
pub enum DriverType {
    KMDF,
    UMDF,
    WDM,
}

impl FromStr for DriverType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "kmdf" => std::result::Result::Ok(DriverType::KMDF),
            "umdf" => std::result::Result::Ok(DriverType::UMDF),
            "wdm" => std::result::Result::Ok(DriverType::WDM),
            _ => Err(format!("'{}' is not a valid driver type", s)),
        }
    }
}

impl fmt::Display for DriverType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            DriverType::KMDF => "kmdf",
            DriverType::UMDF => "umdf",
            DriverType::WDM => "wdm",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Args)]
pub struct NewProjectArgs {
    #[clap(help = "Driver Project Name", default_value = "")]
    pub driver_project_name: String,

    #[clap(long, help = "Driver Type", index = 2)]
    pub driver_type: DriverType,

    #[clap(long, help = "Path to the project", default_value = ".")]
    pub cwd: PathBuf,
}

#[derive(Debug, Clone)]
pub enum Profile {
    Debug,
    Release,
}

impl FromStr for Profile {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "debug" => std::result::Result::Ok(Profile::Debug),
            "release" => std::result::Result::Ok(Profile::Release),
            _ => Err(format!("'{}' is not a valid profile", s)),
        }
    }
}

impl fmt::Display for Profile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Profile::Debug => "debug",
            Profile::Release => "release",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone)]
pub enum TargetArch {
    X86_64,
    Aarch64
}

impl FromStr for TargetArch {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "x86_64" => std::result::Result::Ok(TargetArch::X86_64),
            "aarch64" => std::result::Result::Ok(TargetArch::Aarch64),
            _ => Err(format!("'{}' is not a valid target architecture", s)),
        }
    }
}

impl fmt::Display for TargetArch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            TargetArch::X86_64 => "x86_64",
            TargetArch::Aarch64 => "aarch64",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Args)]
pub struct PackageProjectArgs {
    #[clap(long, help = "Path to the project", default_value = ".")]
    pub cwd: PathBuf,

    #[clap(long, help = "Build Profile/Configuration", default_value = "debug")]
    pub profile: Profile,

    #[clap(long, help = "Build Target", default_value = "x86_64")]
    pub target_arch: TargetArch,

    // TODO: Deal with non-sample classes
    #[clap(long, help = "Sample Class", default_value = "true")]
    pub sample_class: bool,
}
