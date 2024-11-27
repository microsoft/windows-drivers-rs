// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Types for the `DriverConfig` section of the `metadata.wdk` section of the
//! `Cargo.toml`
//!
//! This section is used to specify the driver type and its associated
//! configuration parameters. This corresponds with the settings in the `Driver
//! Model` property pages

use serde::{Deserialize, Serialize};

/// The driver type with its associated configuration parameters
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(
    tag = "DRIVER_TYPE",
    deny_unknown_fields,
    rename_all = "UPPERCASE",
    from = "DeserializableDriverConfig"
)]
pub enum DriverConfig {
    /// Windows Driver Model
    Wdm,
    /// Kernel Mode Driver Framework
    Kmdf(KmdfConfig),
    /// User Mode Driver Framework
    Umdf(UmdfConfig),
    /// INF only drivers e.g. null drivers and extension INFs
    Package,
}

/// Private enum identical to [`DriverConfig`] but with different tag name to
/// deserialize from.
///
/// [`serde_derive`] doesn't support different tag names for serialization vs.
/// deserialization, and also doesn't support aliases for tag names, so the
/// `from` attribute is used in conjunction with this type to facilitate a
/// different tag name for deserialization.
///
/// Relevant Github Issues:
/// * <https://github.com/serde-rs/serde/issues/2776>
/// * <https://github.com/serde-rs/serde/issues/2324>
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "driver-type", deny_unknown_fields, rename_all = "UPPERCASE")]
enum DeserializableDriverConfig {
    Wdm,
    Kmdf(KmdfConfig),
    Umdf(UmdfConfig),
    Package,
}

/// The configuration parameters for KMDF drivers
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(
    deny_unknown_fields,
    rename_all(serialize = "SCREAMING_SNAKE_CASE", deserialize = "kebab-case")
)]
pub struct KmdfConfig {
    /// Major KMDF Version
    pub kmdf_version_major: u8,
    /// Minor KMDF Version (Target Version)
    pub target_kmdf_version_minor: u8,
    /// Minor KMDF Version (Minimum Required)
    pub minimum_kmdf_version_minor: Option<u8>,
}

/// The configuration parameters for UMDF drivers
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(
    deny_unknown_fields,
    rename_all(serialize = "SCREAMING_SNAKE_CASE", deserialize = "kebab-case")
)]
pub struct UmdfConfig {
    /// Major UMDF Version
    pub umdf_version_major: u8,
    /// Minor UMDF Version (Target Version)
    pub target_umdf_version_minor: u8,
    /// Minor UMDF Version (Minimum Required)
    pub minimum_umdf_version_minor: Option<u8>,
}

impl From<DeserializableDriverConfig> for DriverConfig {
    fn from(config: DeserializableDriverConfig) -> Self {
        match config {
            DeserializableDriverConfig::Wdm => Self::Wdm,
            DeserializableDriverConfig::Kmdf(kmdf_config) => Self::Kmdf(kmdf_config),
            DeserializableDriverConfig::Umdf(umdf_config) => Self::Umdf(umdf_config),
            DeserializableDriverConfig::Package => Self::Package,
        }
    }
}

impl Default for KmdfConfig {
    #[must_use]
    fn default() -> Self {
        // FIXME: determine default values from TargetVersion and _NT_TARGET_VERSION
        Self {
            kmdf_version_major: 1,
            target_kmdf_version_minor: 33,
            minimum_kmdf_version_minor: None,
        }
    }
}

impl KmdfConfig {
    /// Creates a new [`KmdfConfig`] with default values
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for UmdfConfig {
    #[must_use]
    fn default() -> Self {
        // FIXME: determine default values from TargetVersion and _NT_TARGET_VERSION
        Self {
            umdf_version_major: 2,
            target_umdf_version_minor: 33,
            minimum_umdf_version_minor: None,
        }
    }
}

impl UmdfConfig {
    /// Creates a new [`UmdfConfig`] with default values
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}
