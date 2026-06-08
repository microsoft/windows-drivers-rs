// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! This module contains the core functionality of the cargo-wdk utility. It
//! include modules which implement the business logic and common types that can
//! be shared across different actions. The `action` modules that implement the
//! business logic of the cargo-wdk utility are:
//! * `new` - New action module
//! * `build` - Build action module
//! * `clean` - Clean action module
pub mod build;
pub mod clean;
pub mod new;

use std::{
    fmt::{self, Display},
    str::FromStr,
};

use clap::Args;
use wdk_build::CpuArchitecture;

pub const KMDF_STR: &str = "kmdf";
pub const UMDF_STR: &str = "umdf";
pub const WDM_STR: &str = "wdm";
/// `x86_64/Amd64` target triple name
const X86_64_TARGET_TRIPLE_NAME: &str = "x86_64-pc-windows-msvc";
/// `aarch64/Arm64` target triple name
const AARCH64_TARGET_TRIPLE_NAME: &str = "aarch64-pc-windows-msvc";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
            KMDF_STR => Ok(Self::Kmdf),
            UMDF_STR => Ok(Self::Umdf),
            WDM_STR => Ok(Self::Wdm),
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

/// Cargo feature selection forwarded to `cargo` CLI commands.
///
/// Options:
///
/// * `--all-features` activates every feature in the resolved package(s).
/// * `--no-default-features` skips the `default` feature.
/// * `--features <FEATURES>` is a space- or comma-separated list (and may be
///   repeated) of features to activate.
#[derive(Args, Debug, Default, Clone)]
pub struct FeatureArgs {
    /// Activate all available features.
    #[arg(long)]
    pub all_features: bool,

    /// Do not activate the `default` feature.
    #[arg(long)]
    pub no_default_features: bool,

    /// Space- or comma-separated list of features to activate.
    #[arg(long, value_name = "FEATURES", value_delimiter = ',')]
    pub features: Vec<String>,
}

impl FeatureArgs {
    /// Returns the `cargo` CLI arguments equivalent to this selection, in the
    /// canonical order (`--all-features`, `--no-default-features`, then one
    /// `--features <name>` pair per feature).
    #[must_use]
    pub fn to_cargo_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        if self.all_features {
            args.push("--all-features".to_string());
        }
        if self.no_default_features {
            args.push("--no-default-features".to_string());
        }
        for feature in &self.features {
            args.push("--features".to_string());
            args.push(feature.clone());
        }
        args
    }
}
