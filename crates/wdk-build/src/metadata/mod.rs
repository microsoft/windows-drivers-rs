// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

pub mod error;
pub mod map;
pub mod ser;

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use camino::Utf8PathBuf;
use cargo_metadata::{Metadata, MetadataCommand};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::DriverConfig;

pub trait TryFromCargoMetadata {
    type Error;

    fn try_from_cargo_metadata(manifest_path: impl AsRef<Path>) -> Result<Self, Self::Error>
    where
        Self: Sized;
}

/// Metadata specified in the `package.metadata.wdk` section of the `Cargo.toml`
/// of a crate that depends on the WDK. This corresponds with the settings in
/// the `Driver Settings` property pages for WDK projects in Visual Studio
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(
    deny_unknown_fields,
    rename_all(serialize = "SCREAMING_SNAKE_CASE", deserialize = "kebab-case")
)]
pub struct WDKMetadata {
    // general: General,
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

// #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
// #[serde(deny_unknown_fields)]
// pub struct KMDFDriverModel {}

// Errors that could result from trying to convert a [`WDKMetadata`] to a
// [`DriverConfig`]
// #[derive(Debug, Error)]
// pub enum TryFromWDKMetadataError {
//     /// Error returned when the [`WDKMetadata`] is missing KMDF metadata
//     #[error(
//         "missing KMDF metadata needed to convert from wdk_build::WDKMetadata
// to \          wdk_build::DriverConfig::KMDF: {missing_metadata_field}"
//     )]
//     MissingKMDFMetadata {
//         /// Missing KMDF metadata
//         missing_metadata_field: String,
//     },
// }

/// Errors that could result from trying to construct a [`WDKMetadata`] from
/// information parsed by `cargo metadata`
#[derive(Debug, Error)]
pub enum TryFromCargoMetadataError {
    /// Error returned when `cargo_metadata` execution or parsing fails
    #[error(transparent)]
    CargoMetadata(#[from] cargo_metadata::Error),

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

impl TryFromCargoMetadata for WDKMetadata {
    type Error = TryFromCargoMetadataError;

    fn try_from_cargo_metadata(manifest_path: impl AsRef<Path>) -> Result<Self, Self::Error> {
        let manifest_path = manifest_path.as_ref();

        let Metadata {
            packages,
            workspace_metadata,
            workspace_root,
            ..
        } = MetadataCommand::new().manifest_path(manifest_path).exec()?;

        // Parse packages and workspace for Cargo manifest paths and WDKMetadata
        let ParsedData {
            mut wdk_metadata_configurations,
            mut cargo_manifest_paths,
        } = parse_packages_wdk_metadata(packages)?;
        if let Some(workspace_metadata) = parse_workspace_wdk_metadata(workspace_metadata)? {
            wdk_metadata_configurations.insert(workspace_metadata);
        }
        let workspace_manifest_path = {
            let mut path = workspace_root;
            path.push("Cargo.toml");
            path
        };
        cargo_manifest_paths.insert(workspace_manifest_path);
        cargo_manifest_paths.insert(manifest_path.to_owned().try_into()?);

        // Force rebuilds if any of the manifest files change (ex. if wdk metadata
        // section is modified)
        for path in cargo_manifest_paths {
            println!("cargo::rerun-if-changed={path}");
        }

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

struct ParsedData {
    wdk_metadata_configurations: HashSet<WDKMetadata>,
    cargo_manifest_paths: HashSet<Utf8PathBuf>,
}

fn parse_packages_wdk_metadata(
    packages: Vec<cargo_metadata::Package>,
) -> Result<ParsedData, TryFromCargoMetadataError> {
    let mut cargo_manifest_paths: HashSet<_> = HashSet::new();
    let wdk_metadata_configurations = packages
        .into_iter()
        .filter_map(|mut package| {
            // keep track of manifest paths for all packages, regardless if they have WDK
            // metadata. This is so that cargo::rerun-if-changed can be emitted for all
            // manifest files, so that when wdk metadata is added to a package that didn't
            // previosuly have it, it forces a rebuild
            cargo_manifest_paths.insert(package.manifest_path);

            // extract WDKMetadata information from all packages that have it
            match package.metadata["wdk"].take() {
                serde_json::Value::Null => None,
                wdk_metadata => Some(serde_json::from_value::<WDKMetadata>(wdk_metadata).map_err(
                    |err| TryFromCargoMetadataError::WDKMetadataDeserialization {
                        metadata_source: format!(
                            "{} for {} package",
                            stringify!(package.metadata["wdk"]),
                            package.name
                        ),
                        error_source: err,
                    },
                )),
            }
        })
        .collect::<Result<HashSet<_>, _>>()?;

    Ok(ParsedData {
        wdk_metadata_configurations,
        cargo_manifest_paths,
    })
}

fn parse_workspace_wdk_metadata(
    mut workspace_metadata: serde_json::Value,
) -> Result<Option<WDKMetadata>, TryFromCargoMetadataError> {
    Ok(match workspace_metadata["wdk"].take() {
        serde_json::Value::Null => None,
        wdk_metadata => Some(serde_json::from_value::<WDKMetadata>(wdk_metadata).map_err(
            |err| TryFromCargoMetadataError::WDKMetadataDeserialization {
                metadata_source: stringify!(workspace_metadata["wdk"]).to_string(),
                error_source: err,
            },
        )?),
    })
}
