// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

pub mod error;
pub mod map;
pub mod ser;

use std::{collections::HashSet, path::PathBuf};

use camino::Utf8PathBuf;
use cargo_metadata::Metadata;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::DriverConfig;

/// Metadata specified in the `metadata.wdk` section of the `Cargo.toml`
/// of a crate that depends on the WDK, or in a cargo workspace. This corresponds with the settings in
/// the `Driver Settings` property pages for WDK projects in Visual Studio
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(
    deny_unknown_fields,
    rename_all(serialize = "SCREAMING_SNAKE_CASE", deserialize = "kebab-case")
)]
pub struct WDKMetadata {
    // general: General,
    /// Metadata corresponding to the `Driver Model` property page in the WDK
    pub driver_model: DriverConfig,
}

// TODO: move all metadata to one source of truth

// #[derive(Debug, Clone, Serialize, Deserialize)]
// #[serde(deny_unknown_fields, rename_all = "kebab-case")]
// pub struct General {
//     //       <PreprocessorDefinitions
// Condition="'$(OverrideTargetVersionDefines)' !=
// 'true'">_WIN32_WINNT=$(WIN32_WINNT_VERSION);WINVER=$(WINVER_VERSION);WINNT=1;
// NTDDI_VERSION=$(NTDDI_VERSION);%(ClCompile.PreprocessorDefinitions)</
// PreprocessorDefinitions> //       <PreprocessorDefinitions
// Condition="'$(IsKernelModeToolset)' !=
// 'true'">WIN32_LEAN_AND_MEAN=1;%(ClCompile.PreprocessorDefinitions)</
// PreprocessorDefinitions>

//     t_os_version,
//     driver_target_platform:
//     nt_target_version: u32
// }

// Metadata corresponding to the driver model page property page for WDK
// projects in Visual Studio
// #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
// #[serde(deny_unknown_fields, rename_all = "kebab-case")]
// pub struct DriverModel {

//     driver_type: DriverType,

//     // KMDF-specific metadata
//     kmdf_version_major: Option<u8>,
//     target_kmdf_version_minor: Option<u8>,
//     minimum_kmdf_version_minor: Option<u8>,

//     // UMDF-specific metadata
//     umdf_version_major: Option<u8>,
//     target_umdf_version_minor: Option<u8>,
//     minimum_umdf_version_minor: Option<u8>,
// }

/// Errors that could result from trying to construct a [`WDKMetadata`] from
/// information parsed by `cargo metadata`
#[derive(Debug, Error)]
pub enum TryFromCargoMetadataError {
    /// Error returned when no WDK configuration metadata is detected in the
    /// dependency graph
    #[error(
        "no WDK configuration metadata is detected in the dependency graph. This could happen \
         when building WDR itself, building library crates that depend on the WDK but defer wdk \
         configuration to their consumers, or when building a driver that has a path dependency \
         on WDR"
    )]
    NoWDKConfigurationsDetected,

    /// Error returned when multiple configurations of the WDK are detected
    /// across the dependency graph
    #[error(
        "multiple configurations of the WDK are detected across the dependency graph, but only \
         one configuration is allowed: {wdk_metadata_configurations:#?}"
    )]
    MultipleWDKConfigurationsDetected {
        /// [`HashSet`] of unique [`WDKMetadata`] derived from detected WDK
        /// metadata
        wdk_metadata_configurations: HashSet<WDKMetadata>,
    },

    /// Error returned when [`WDKMetadata`] fails to be deserialized from
    /// [`cargo_metadata::Metadata`] output
    #[error("failed to deserialize WDKMetadata from {metadata_source}")]
    WDKMetadataDeserialization {
        /// `String` that describes what part of
        /// `cargo_metadata::Metadata` was used as the source for
        /// deserialization
        metadata_source: String,
        /// [`serde_json::Error`] that caused the deserialization to fail
        #[source]
        error_source: serde_json::Error,
    },

    /// Error returned when the `try_from_cargo_metadata` is called with a
    /// `manifest_path` that contains invalid UTF-8
    #[error("manifest path contains invalid UTF-8: {0}")]
    NonUtf8ManifestPath(#[from] camino::FromPathBufError),
}

impl TryFrom<&Metadata> for WDKMetadata {
    type Error = TryFromCargoMetadataError;

    fn try_from(metadata: &Metadata) -> Result<Self, Self::Error> {
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
        // TODO: add ws level test:
        //////////////ws level tests: https://stackoverflow.com/a/71461114/10173605
        match wdk_metadata_configurations.len() {
            1 => Ok(wdk_metadata_configurations.into_iter().next().expect(
                "wdk_metadata_configurations should have exactly one element because of the \
                 .len() check above",
            )),

            // TODO: add a test for this
            0 => Err(TryFromCargoMetadataError::NoWDKConfigurationsDetected),

            // TODO: add a test for this
            _ => Err(
                TryFromCargoMetadataError::MultipleWDKConfigurationsDetected {
                    wdk_metadata_configurations,
                },
            ),
        }
    }
}

/// Find the path the the toplevel Cargo manifest of the currently executing
/// Cargo subcommand. This should resolve to either:
/// 1. the `Cargo.toml` of the package where the Cargo subcommand (build, check,
///    etc.) was run
/// 2. the `Cargo.toml` provided to the `--manifest-path` argument to the Cargo
///    subcommand
/// 3. the `Cargo.toml` of the workspace that contains the package pointed to by
///    1 or 2
///
/// The returned path should be a manifest in the same directory of the
/// lockfile. This does not support invokations that use non-default target
/// directories (ex. via `--target-dir`). This function only works when called
/// from a `build.rs` file
#[must_use]
pub fn find_top_level_cargo_manifest() -> PathBuf {
    let out_dir =
        PathBuf::from(std::env::var("OUT_DIR").expect(
            "Cargo should have set the OUT_DIR environment variable when executing build.rs",
        ));

    out_dir
        .ancestors()
        .find(|path| path.join("Cargo.lock").exists())
        .expect("a Cargo.lock file should exist in the same directory as the top-level Cargo.toml")
        .join("Cargo.toml")
}

fn parse_packages_wdk_metadata(
    packages: &Vec<cargo_metadata::Package>,
) -> Result<HashSet<WDKMetadata>, TryFromCargoMetadataError> {
    let wdk_metadata_configurations = packages
        .into_iter()
        .filter_map(|package| match &package.metadata["wdk"] {
            serde_json::Value::Null => None,
            wdk_metadata => Some(WDKMetadata::deserialize(wdk_metadata).map_err(|err| {
                TryFromCargoMetadataError::WDKMetadataDeserialization {
                    metadata_source: format!(
                        "{} for {} package",
                        stringify!(package.metadata["wdk"]),
                        package.name
                    ),
                    error_source: err,
                }
            })),
        })
        .collect::<Result<HashSet<_>, _>>()?;

    Ok(wdk_metadata_configurations)
}

fn parse_workspace_wdk_metadata(
    workspace_metadata: &serde_json::Value,
) -> Result<Option<WDKMetadata>, TryFromCargoMetadataError> {
    Ok(match &workspace_metadata["wdk"] {
        serde_json::Value::Null => None,
        wdk_metadata => Some(WDKMetadata::deserialize(wdk_metadata).map_err(|err| {
            TryFromCargoMetadataError::WDKMetadataDeserialization {
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
