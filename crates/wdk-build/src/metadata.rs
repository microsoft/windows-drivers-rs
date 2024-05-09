use std::{
    borrow::Borrow,
    collections::HashSet,
    path::{Path, PathBuf},
};

use cargo_metadata::{Metadata, MetadataCommand};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{ConfigError, DriverConfig, DriverType, KMDFConfig, UMDFConfig};

/// Metadata specified in the `package.metadata.wdk` section of the `Cargo.toml`
/// of a crate that depends on the WDK. This corresponds with the settings in
/// the `Driver Settings` property pages for WDK projects in Visual Studio
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct WDKMetadata {
    #[serde(rename = "driver-model")]
    driver_model: DriverModel,
}

// TODO!
// pub struct General {
//     target_version,
//     driver_target_platform,
//     _NT_TARGET_VERSION
// }

/// Metadata corresponding to the driver model page property page for WDK
/// projects in Visual Studio
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct DriverModel {
    #[serde(rename = "driver-type")]
    driver_type: DriverType,

    // KMDF-specific metadata
    #[serde(rename = "kmdf-version-major")]
    kmdf_version_major: Option<u8>,
    #[serde(rename = "target-kmdf-version-minor")]
    target_kmdf_version_minor: Option<u8>,
    #[serde(rename = "minimum-kmdf-version-minor")]
    minimum_kmdf_version_minor: Option<u8>,

    // UMDF-specific metadata
    #[serde(rename = "umdf-version-major")]
    umdf_version_major: Option<u8>,
    #[serde(rename = "target-umdf-version-minor")]
    target_umdf_version_minor: Option<u8>,
    #[serde(rename = "minimum-umdf-version-minor")]
    minimum_umdf_version_minor: Option<u8>,
}

// #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
// #[serde(deny_unknown_fields)]
// pub struct KMDFDriverModel {}

/// Errors that could result from trying to convert a [`WDKMetadata`] to a
/// [`DriverConfig`]
#[derive(Debug, Error)]
pub enum TryFromWDKMetadataError {
    /// Error returned when the [`WDKMetadata`] is missing KMDF metadata
    #[error(
        "missing KMDF metadata needed to convert from wdk_build::WDKMetadata to \
         wdk_build::DriverConfig::KMDF: {missing_metadata_field}"
    )]
    MissingKMDFMetadata {
        /// Missing KMDF metadata
        missing_metadata_field: String,
    },
}

impl TryFrom<WDKMetadata> for DriverConfig {
    type Error = TryFromWDKMetadataError;

    fn try_from(wdk_metadata: WDKMetadata) -> Result<Self, Self::Error> {
        Ok(match wdk_metadata.driver_model.driver_type {
            DriverType::WDM => Self::WDM(),
            DriverType::KMDF => Self::KMDF(KMDFConfig {
                kmdf_version_major: wdk_metadata.driver_model.kmdf_version_major.ok_or_else(
                    || TryFromWDKMetadataError::MissingKMDFMetadata {
                        missing_metadata_field: stringify!(WDKMetadata.d).to_string(),
                    },
                )?,
                kmdf_version_minor: wdk_metadata
                    .driver_model
                    .target_kmdf_version_minor
                    // tODO: should error if not present
                    .unwrap_or(33),
            }),
            DriverType::UMDF => Self::UMDF(UMDFConfig {
                // tODO: should error if not present
                umdf_version_major: wdk_metadata.driver_model.kmdf_version_major.unwrap_or(2),
                umdf_version_minor: wdk_metadata
                    .driver_model
                    .target_kmdf_version_minor
                    // tODO: should error if not present
                    .unwrap_or(33),
            }),
        })
    }
}

/// TODO: add docs
/// # Panics
///
/// todo
///
/// # Errors
///
/// todo
pub fn detect_driver_config(manifest_path: impl AsRef<Path>) -> Result<DriverConfig, ConfigError> {
    // TODO: check that if this auto reruns if cargo.toml's change
    let manifest_path = manifest_path.as_ref();

    let metadata = MetadataCommand::new()
        .manifest_path(&manifest_path)
        .exec()?;

    let driver_config_from_workspace_manifest =
        parse_workspace_metadata_for_driver_config(&metadata);
    let driver_config_from_package_manifests = parse_package_metadata_for_driver_config(&metadata);

    // TODO: add ws level test:
    //////////////ws level tests: https://stackoverflow.com/a/71461114/10173605

    match (
        driver_config_from_workspace_manifest,
        driver_config_from_package_manifests,
    ) {
        // Either the workspace or package manifest has a driver configuration
        (Ok(driver_config), Err(ConfigError::NoWDKConfigurationsDetected))
        | (Err(ConfigError::NoWDKConfigurationsDetected), Ok(driver_config)) => Ok(driver_config),

        // Both the workspace and package manifest have a driver configuration. This is only allowed
        // if they are the same
        (Ok(workspace_driver_config), Ok(packages_driver_config)) => {
            if workspace_driver_config != packages_driver_config {
                return Err(ConfigError::MultipleWDKConfigurationsDetected {
                    wdk_configurations: [workspace_driver_config, packages_driver_config]
                        .into_iter()
                        .collect(),
                });
            }

            Ok(workspace_driver_config)
        }

        // Workspace has a driver configuration, and multiple conflicting driver configurations were
        // detected in the package manifests. This is a special case so that the error can list all
        // the offending driver configurations
        (
            Ok(workspace_driver_config),
            Err(ConfigError::MultipleWDKConfigurationsDetected {
                mut wdk_configurations,
            }),
        ) => {
            wdk_configurations.insert(workspace_driver_config);
            Err(ConfigError::MultipleWDKConfigurationsDetected { wdk_configurations })
        }

        (unhandled_error @ Err(_), _) | (_, unhandled_error @ Err(_)) => unhandled_error,
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
pub fn find_top_level_cargo_manifest() -> PathBuf {
    let out_dir =
        PathBuf::from(std::env::var("OUT_DIR").expect(
            "Cargo should have set the OUT_DIR environment variable when executing build.rs",
        ));
    // TODO need rerun on OUT_DIR and cargo.toml changes?

    out_dir
        .ancestors()
        .skip_while(|path| !path.join("Cargo.lock").exists())
        .next()
        .unwrap()
        .join("Cargo.toml")
    // TODO: error handling
}

/// todo
fn parse_package_metadata_for_driver_config(
    metadata: impl Borrow<Metadata>,
) -> Result<DriverConfig, ConfigError> {
    let metadata = metadata.borrow();

    let wdk_configurations = metadata
        .packages
        .iter()
        .filter_map(|package| {
            if let Some(wdk_metadata) = package.metadata.get("wdk") {
                // TODO: error handling
                return Some(
                    serde_json::from_value::<WDKMetadata>(wdk_metadata.clone())
                        .unwrap()
                        .try_into()
                        .unwrap(),
                );
            };
            None
        })
        .collect::<HashSet<_>>();

    // Only one configuration of WDK is allowed per dependency graph
    match wdk_configurations.len() {
        1 => {
            return Ok(wdk_configurations.into_iter().next().expect(
                "wdk_configurations should have exactly one element because of the .len() check \
                 above",
            ));
        }

        0 => {
            // TODO: add a test for this
            return Err(ConfigError::NoWDKConfigurationsDetected {});
        }

        _ => {
            // TODO: add a test for this
            return Err(ConfigError::MultipleWDKConfigurationsDetected {
                wdk_configurations: wdk_configurations,
            });
        }
    }
}

fn parse_workspace_metadata_for_driver_config(
    metadata: impl Borrow<Metadata>,
) -> Result<DriverConfig, ConfigError> {
    let metadata = metadata.borrow();

    if let Some(wdk_metadata) = metadata.workspace_metadata.get("wdk") {
        // TODO: error handling for this unwrap when failure to parse json value into
        // wdkmetadata
        return Ok(serde_json::from_value::<WDKMetadata>(wdk_metadata.clone())
            .unwrap()
            .try_into()?);
    }

    return Err(ConfigError::NoWDKConfigurationsDetected);
}
