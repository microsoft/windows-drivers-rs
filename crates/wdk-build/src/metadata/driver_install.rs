// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

use serde::{Deserialize, Serialize};

/// The `DRIVER_INSTALL` section of the `metadata.wdk` section of the `Cargo.toml`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(
    deny_unknown_fields,
    rename_all(serialize = "SCREAMING_SNAKE_CASE", deserialize = "kebab-case")
)]
pub struct DriverInstall {
    /// List of files to be installed with the driver package.
    pub package_files: Vec<String>
}

impl Default for DriverInstall {
    fn default() -> Self {
        Self {
            package_files: Vec::new()
        }
    }
}

impl DriverInstall {
   /// Creates a new [`DriverInstall`] with default values
   #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}