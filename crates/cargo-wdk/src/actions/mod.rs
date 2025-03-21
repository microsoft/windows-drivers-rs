/// This module defines various actions for the cargo-wdk CLI tool.
/// It includes modules for creating new projects, building projects, and
/// packaging projects.
use std::fmt;

/// Business logic is divided into the following action modules
/// * `new` - New action module
/// * `build` - Build action module
/// * `package` - Package action module
pub mod build;
pub mod new;
pub mod package;

pub(crate) const X86_TARGET_TRIPLE_NAME: &str = "x86_64-pc-windows-msvc";
pub(crate) const ARM64_TARGET_TRIPLE_NAME: &str = "aarch64-pc-windows-msvc";

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
pub enum TargetArch {
    X64,
    Arm64,
    Host,
}
