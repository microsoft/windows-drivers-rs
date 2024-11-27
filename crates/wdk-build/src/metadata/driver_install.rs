// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Types for the `DriverInstall` section of the `metadata.wdk` section of the
//! `Cargo.toml`
//!
//! This section is used to specify files to be installed with the driver
//! package. This corresponds with the settings in the `Driver Install` property
//! pages

use serde::{Deserialize, Serialize};

/// The `DRIVER_INSTALL` section of the `metadata.wdk` section of the
/// `Cargo.toml`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(
    deny_unknown_fields,
    rename_all(serialize = "SCREAMING_SNAKE_CASE", deserialize = "kebab-case")
)]
pub struct DriverInstall {
    /// List of files to be installed with the driver package.
    pub package_files: Vec<String>,
}

impl DriverInstall {
    /// Creates a new [`DriverInstall`] with default values
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}
