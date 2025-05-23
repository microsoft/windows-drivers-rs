// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! This module contains the core functionality of the cargo-wdk utility. It
//! include modules which implement the business logic and common types that can
//! be shared across different actions. The `action` modules that implement the
//! business logic of the cargo-wdk utility are:
//! * `new` - New action module
//! * `build` - Build action module
pub mod build;
pub mod new;

use std::{
    fmt::{self, Display},
    str::FromStr,
};

use wdk_build::CpuArchitecture;

pub const KMDF_STR: &str = "kmdf";
pub const UMDF_STR: &str = "umdf";
pub const WDM_STR: &str = "wdm";

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
impl Display for Profile {
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

/// `x86_64/Amd64` target triple name
const X86_64_TARGET_TRIPLE_NAME: &str = "x86_64-pc-windows-msvc";
/// `aarch64/Arm64` target triple name
const AARCH64_TARGET_TRIPLE_NAME: &str = "aarch64-pc-windows-msvc";

/// Converts `CpuArchitecture` to its corresponding target triple name.
#[must_use]
pub fn to_target_triple(cpu_arch: CpuArchitecture) -> String {
    match cpu_arch {
        CpuArchitecture::Amd64 => X86_64_TARGET_TRIPLE_NAME.to_string(),
        CpuArchitecture::Arm64 => AARCH64_TARGET_TRIPLE_NAME.to_string(),
    }
}

/// Enum of driver types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverType {
    Kmdf,
    Umdf,
    Wdm,
}

impl FromStr for DriverType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            KMDF_STR => std::result::Result::Ok(Self::Kmdf),
            UMDF_STR => std::result::Result::Ok(Self::Umdf),
            WDM_STR => std::result::Result::Ok(Self::Wdm),
            _ => Err(format!("'{s}' is not a valid driver type")),
        }
    }
}

impl Display for DriverType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Kmdf => KMDF_STR,
            Self::Umdf => UMDF_STR,
            Self::Wdm => WDM_STR,
        };
        write!(f, "{s}")
    }
}
