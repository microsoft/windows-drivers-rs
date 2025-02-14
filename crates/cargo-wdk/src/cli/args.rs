use std::{path::PathBuf, str::FromStr};

use clap::Args;

use crate::actions::{DriverType, Profile, TargetArch};

#[derive(Debug, Clone)]
pub enum DriverTypeArg {
    KMDF,
    UMDF,
    WDM,
}

impl Into<DriverType> for DriverTypeArg {
    fn into(self) -> DriverType {
        match self {
            DriverTypeArg::KMDF => DriverType::KMDF,
            DriverTypeArg::UMDF => DriverType::UMDF,
            DriverTypeArg::WDM => DriverType::WDM,
        }
    }
}

impl FromStr for DriverTypeArg {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "kmdf" => std::result::Result::Ok(DriverTypeArg::KMDF),
            "umdf" => std::result::Result::Ok(DriverTypeArg::UMDF),
            "wdm" => std::result::Result::Ok(DriverTypeArg::WDM),
            _ => Err(format!("'{}' is not a valid driver type", s)),
        }
    }
}

#[derive(Debug, Args)]
pub struct NewProjectArgs {
    #[clap(help = "Driver Project Name")]
    pub driver_project_name: String,

    #[clap(long, help = "Driver Type")]
    pub driver_type: DriverTypeArg,

    #[clap(long, help = "Path to the project", default_value = ".")]
    pub cwd: PathBuf,
}

#[derive(Debug, Clone)]
pub enum ProfileArg {
    Debug,
    Release,
}

impl Into<Profile> for ProfileArg {
    fn into(self) -> Profile {
        match self {
            ProfileArg::Debug => Profile::Debug,
            ProfileArg::Release => Profile::Release,
        }
    }
}

impl FromStr for ProfileArg {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "debug" => Ok(ProfileArg::Debug),
            "release" => Ok(ProfileArg::Release),
            _ => Err(format!("'{}' is not a valid profile", s)),
        }
    }
}

#[derive(Debug, Clone)]
pub enum TargetArchArg {
    X86_64,
    Aarch64,
}

impl Into<TargetArch> for TargetArchArg {
    fn into(self) -> TargetArch {
        match self {
            TargetArchArg::X86_64 => TargetArch::X86_64,
            TargetArchArg::Aarch64 => TargetArch::Aarch64,
        }
    }
}

impl FromStr for TargetArchArg {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "x86_64" => std::result::Result::Ok(TargetArchArg::X86_64),
            "aarch64" => std::result::Result::Ok(TargetArchArg::Aarch64),
            _ => Err(format!("'{}' is not a valid target architecture", s)),
        }
    }
}

#[derive(Debug, Args)]
pub struct PackageProjectArgs {
    #[clap(long, help = "Path to the project", default_value = ".")]
    pub cwd: PathBuf,

    #[clap(long, help = "Build Profile/Configuration", default_value = "debug")]
    pub profile: ProfileArg,

    #[clap(long, help = "Build Target", default_value = "x86_64")]
    pub target_arch: TargetArchArg,

    // TODO: Deal with non-sample classes
    #[clap(long, help = "Sample Class", default_value = "true")]
    pub sample_class: bool,
}
