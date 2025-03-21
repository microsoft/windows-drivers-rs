/// This module defines various actions for the cargo-wdk CLI tool.
/// It includes modules for creating new projects, building projects, and
/// packaging projects.
use std::{fmt, str::FromStr};

/// Business logic is divided into the following action modules
/// * `new` - New action module
/// * `build` - Build action module
/// * `package` - Package action module
pub mod build;
pub mod new;
pub mod package;

pub const X86_64_TARGET_TRIPLE_NAME: &str = "x86_64-pc-windows-msvc";
pub const AARCH64_TARGET_TRIPLE_NAME: &str = "aarch64-pc-windows-msvc";

/// `DriverType` for the action layer
#[derive(Debug, Clone)]
pub enum DriverType {
    Kmdf,
    Umdf,
    Wdm,
}

impl fmt::Display for DriverType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Kmdf => "kmdf",
            Self::Umdf => "umdf",
            Self::Wdm => "wdm",
        };
        write!(f, "{s}")
    }
}

/// `Profile` for the action layer
#[derive(Debug, Clone, Copy)]
pub enum Profile {
    Dev,
    Release,
}

impl fmt::Display for Profile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Dev => "dev",
            Self::Release => "release",
        };
        write!(f, "{s}")
    }
}

impl Profile {
    pub fn target_folder_name(self) -> String {
        match self {
            Self::Dev => "debug".to_string(),
            Self::Release => "release".to_string(),
        }
    }
}

/// `TargetArch` for the action layer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuArchitecture {
    Amd64,
    Arm64,
    Host,
}

impl FromStr for CpuArchitecture {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "amd64" => std::result::Result::Ok(Self::Amd64),
            "arm64" => std::result::Result::Ok(Self::Arm64),
            _ => Err(format!("'{s}' is not a valid target architecture")),
        }
    }
}

impl fmt::Display for CpuArchitecture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Amd64 => "amd64",
            Self::Arm64 => "arm64",
        };
        write!(f, "{s}")
    }
}
