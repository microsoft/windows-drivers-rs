//! This module provides the core functionality for the cargo-wdk CLI tool.
//! It includes submodules for handling actions such as creating new driver
//! projects, building them, and packaging them.
//! This module also defines common types to be shared across the action layer.
use std::fmt;

/// Business logic is divided into the following action modules
/// * `new` - New action module
/// * `build` - Build action module
/// * `package` - Package action module
pub mod build;
pub mod new;
pub mod package;

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
#[derive(Debug, Clone, Copy)]
pub enum TargetArch {
    X64,
    Arm64,
}
