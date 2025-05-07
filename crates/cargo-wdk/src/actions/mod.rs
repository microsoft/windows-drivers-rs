// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! This module provides the core functionality for the cargo-wdk CLI tool.
//! It includes submodules for handling actions such as creating new driver
//! projects, building them, and packaging them.
//! This module also defines common types to be shared across the action layer.
use std::{fmt, str::FromStr};

/// Business logic is divided into the following action modules
/// * `new` - New action module
/// * `build` - Build action module
/// * `package` - Package action module
pub mod build;
pub mod new;
pub mod package;

use wdk_build::CpuArchitecture;

#[derive(Debug, Clone, Copy)]
pub enum Profile {
    Dev,
    Release,
}

impl FromStr for Profile {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "dev" => std::result::Result::Ok(Self::Dev),
            "release" => std::result::Result::Ok(Self::Release),
            _ => Err(format!("'{s}' is not a valid profile")),
        }
    }
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

/// Enum is used to determine the architecture for which the driver is being
/// built. It can be either a selected architecture passed via CLI or a default
/// host architecture.
#[derive(Debug, Clone, Copy)]
pub enum TargetArch {
    Selected(CpuArchitecture),
    Default(CpuArchitecture),
}
