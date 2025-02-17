use std::{path::PathBuf, str::FromStr};

use anyhow::Result;
use clap::Args;

use super::error::{InvalidDriverProjectNameError, NewProjectArgsError};
use crate::actions::{DriverType, Profile, TargetArch};

#[derive(Debug, Clone)]
pub struct ProjectNameArg(pub String);

impl FromStr for ProjectNameArg {
    type Err = NewProjectArgsError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(NewProjectArgsError::InvalidDriverProjectNameError(
                s.to_string(),
                InvalidDriverProjectNameError::EmptyProjectNameError,
            ));
        }
        if !s
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(NewProjectArgsError::InvalidDriverProjectNameError(
                s.to_string(),
                InvalidDriverProjectNameError::NonAlphanumericProjectNameError,
            ));
        }
        if !s.chars().next().unwrap().is_alphabetic() {
            return Err(NewProjectArgsError::InvalidDriverProjectNameError(
                s.to_string(),
                InvalidDriverProjectNameError::InvalidStartCharacter,
            ));
        }
        let invalid_names = vec!["crate", "self", "super", "extern", "_", "-", "new", "build"];
        if invalid_names.contains(&s) {
            return Err(NewProjectArgsError::InvalidDriverProjectNameError(
                s.to_string(),
                InvalidDriverProjectNameError::ReservedName(s.to_string()),
            ));
        }
        std::result::Result::Ok(ProjectNameArg(s.to_string()))
    }
}

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
            _ => Err(NewProjectArgsError::InvalidDriverTypeError(s.to_string()).to_string()),
        }
    }
}

#[derive(Debug, Args)]
pub struct NewProjectArgs {
    #[clap(help = "Driver Project Name")]
    pub driver_project_name: ProjectNameArg,
    #[clap(long, help = "Driver Type", index = 2)]
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
            "debug" => std::result::Result::Ok(ProfileArg::Debug),
            "release" => std::result::Result::Ok(ProfileArg::Release),
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
    #[clap(long, help = "Sample Class", default_value = "true")]
    pub sample_class: bool,
}
