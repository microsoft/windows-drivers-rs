// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Parsing and serializing metadata about WDK projects
//!
//! This module provides a [`Wdk`] struct that represents the cargo metadata
//! specified in the `metadata.wdk` section any `Cargo.toml`. This corresponds
//! with the settings in the `Driver Settings` property pages for WDK projects
//! in Visual Studio. This module also also provides [`serde`]-compatible
//! serialization and deserialization for the metadata.

pub use error::{Error, Result};
pub use map::Map;
pub use ser::{Serializer, to_map, to_map_with_prefix};

pub(crate) mod ser;

mod error;
mod map;

use std::collections::HashSet;

use camino::Utf8PathBuf;
use cargo_metadata::Metadata;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::DriverConfig;

/// Metadata specified in the `metadata.wdk` section of the `Cargo.toml`
/// of a crate that depends on the WDK, or in a cargo workspace.
///
/// This corresponds with the settings in the `Driver Settings` property pages
/// for WDK projects in Visual Studio
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(
    deny_unknown_fields,
    rename_all(serialize = "SCREAMING_SNAKE_CASE", deserialize = "kebab-case")
)]
pub struct Wdk {
    /// Metadata corresponding to the `Driver Model` property page in the WDK
    pub driver_model: DriverConfig,
}

/// Errors that could result from trying to construct a
/// [`metadata::Wdk`](crate::metadata::Wdk) from information parsed by `cargo
/// metadata`
#[derive(Debug, Error)]
pub enum TryFromCargoMetadataError {
    /// Error returned when no WDK configuration metadata is detected in the
    /// dependency graph
    #[error(
        "no WDK configuration metadata is detected in the dependency graph. This could happen \
         when building WDR itself, or building library crates that depend on the WDK but defer \
         WDK configuration to their consumers"
    )]
    NoWdkConfigurationsDetected,

    /// Error returned when multiple configurations of the WDK are detected
    /// across the dependency graph
    #[error(
        "multiple configurations of the WDK are detected across the dependency graph, but only \
         one configuration is allowed: {wdk_metadata_configurations:#?}"
    )]
    MultipleWdkConfigurationsDetected {
        /// [`HashSet`] of unique [`metadata::Wdk`](crate::metadata::Wdk)
        /// derived from detected WDK metadata
        wdk_metadata_configurations: HashSet<Wdk>,
    },

    /// Error returned when [`crate::metadata::Wdk`] fails to be deserialized
    /// from [`cargo_metadata::Metadata`] output
    #[error("failed to deserialize metadata::Wdk from {metadata_source}")]
    WdkMetadataDeserialization {
        /// `String` that describes what part of
        /// `cargo_metadata::Metadata` was used as the source for
        /// deserialization
        metadata_source: String,
        /// [`serde_json::Error`] that caused the deserialization to fail
        #[source]
        error_source: serde_json::Error,
    },
}

impl TryFrom<&Metadata> for Wdk {
    type Error = TryFromCargoMetadataError;

    fn try_from(metadata: &Metadata) -> std::result::Result<Self, Self::Error> {
        let wdk_metadata_configurations = {
            // Parse WDK metadata from workspace and all packages
            let mut configs = parse_packages_wdk_metadata(&metadata.packages)?;
            if let Some(workspace_metadata) =
                parse_workspace_wdk_metadata(&metadata.workspace_metadata)?
            {
                configs.insert(workspace_metadata);
            }
            configs
        };

        // Ensure that only one configuration of WDK is allowed per dependency graph
        match wdk_metadata_configurations.len() {
            1 => Ok(wdk_metadata_configurations.into_iter().next().expect(
                "wdk_metadata_configurations should have exactly one element because of the \
                 .len() check above",
            )),

            0 => Err(TryFromCargoMetadataError::NoWdkConfigurationsDetected),

            _ => Err(
                TryFromCargoMetadataError::MultipleWdkConfigurationsDetected {
                    wdk_metadata_configurations,
                },
            ),
        }
    }
}

fn parse_packages_wdk_metadata(
    packages: &[cargo_metadata::Package],
) -> std::result::Result<HashSet<Wdk>, TryFromCargoMetadataError> {
    let wdk_metadata_configurations = packages
        .iter()
        .filter_map(|package| match &package.metadata["wdk"] {
            serde_json::Value::Null => None,
            // When wdk section is empty, treat it as if it wasn't there. This is to allow for using
            // empty wdk metadata sections to mark the package as a driver (ex. for detection in
            // `package_driver_flow_condition_script`)
            serde_json::Value::Object(map) if map.is_empty() => None,
            wdk_metadata => Some(Wdk::deserialize(wdk_metadata).map_err(|err| {
                TryFromCargoMetadataError::WdkMetadataDeserialization {
                    metadata_source: format!(
                        "{} for {} package",
                        stringify!(package.metadata["wdk"]),
                        package.name
                    ),
                    error_source: err,
                }
            })),
        })
        .collect::<std::result::Result<HashSet<_>, _>>()?;
    Ok(wdk_metadata_configurations)
}

fn parse_workspace_wdk_metadata(
    workspace_metadata: &serde_json::Value,
) -> std::result::Result<Option<Wdk>, TryFromCargoMetadataError> {
    Ok(match &workspace_metadata["wdk"] {
        serde_json::Value::Null => None,
        wdk_metadata => Some(Wdk::deserialize(wdk_metadata).map_err(|err| {
            TryFromCargoMetadataError::WdkMetadataDeserialization {
                metadata_source: stringify!(workspace_metadata["wdk"]).to_string(),
                error_source: err,
            }
        })?),
    })
}

pub(crate) fn iter_manifest_paths(metadata: Metadata) -> impl IntoIterator<Item = Utf8PathBuf> {
    let mut cargo_manifest_paths = HashSet::new();

    // Add all package manifest paths
    for package in metadata.packages {
        cargo_manifest_paths.insert(package.manifest_path);
    }

    // Add workspace manifest path
    let workspace_manifest_path: Utf8PathBuf = {
        let mut path = metadata.workspace_root;
        path.push("Cargo.toml");
        path
    };
    cargo_manifest_paths.insert(workspace_manifest_path);

    cargo_manifest_paths
}
