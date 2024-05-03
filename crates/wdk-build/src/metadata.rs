use std::collections::HashSet;

use cargo_metadata::MetadataCommand;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{ConfigError, DriverConfig, DriverType, KMDFConfig, UMDFConfig};

// "This is a false-positive of this lint since metadata is a private module and WDKMetadata is re-exported to be at the crate root. See https://github.com/rust-lang/rust-clippy/issues/8524"
#[allow(clippy::module_name_repetitions)]
/// Metadata specified in the `package.metadata.wdk` section of the `Cargo.toml`
/// of a crate that depends on the WDK. This corresponds with the settings in
/// the `Driver Settings` property pages for WDK projects in Visual Studio
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct WDKMetadata {
    #[serde(rename = "driver-model")]
    driver_model: DriverModel,
}

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct KMDFDriverModel {}

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
                    .unwrap_or(33),
            }),
            DriverType::UMDF => Self::UMDF(UMDFConfig {
                umdf_version_major: wdk_metadata.driver_model.kmdf_version_major.unwrap_or(2),
                umdf_version_minor: wdk_metadata
                    .driver_model
                    .target_kmdf_version_minor
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
pub fn detect_driver_config() -> Result<DriverConfig, ConfigError> {
    // TODO: check that if this auto reruns if cargo.toml's change
    let cargo_metadata_packages_list = MetadataCommand::new().exec()?.packages;

    // Only one version of wdk-build should be present in the dependency graph
    let wdk_build_package_matches = cargo_metadata_packages_list
        .iter()
        .filter(|package| package.name == "wdk-build")
        .collect::<Vec<_>>();
    if wdk_build_package_matches.len() != 1 {
        // TODO: add test for this
        return Err(ConfigError::MultipleWDKBuildCratesDetected {
            package_ids: wdk_build_package_matches
                .iter()
                .map(|package_info| package_info.id.clone())
                .collect(),
        });
    }

    // Only one configuration of WDK is allowed per dependency graph
    let wdk_metadata_configurations = cargo_metadata_packages_list
        .into_iter()
        .filter_map(|package| {
            if let Some(wdk_metadata) = package.metadata.get("wdk") {
                // TODO: error handling
                return Some(serde_json::from_value::<WDKMetadata>(wdk_metadata.clone()).unwrap());
            };
            None
        })
        .collect::<HashSet<_>>();

    Ok(match wdk_metadata_configurations.len() {
        0 => {
            // TODO: add a test for this
            return Err(ConfigError::NoWDKConfigurationsDetected {});
        }
        1 => wdk_metadata_configurations
            .into_iter()
            .next()
            .expect(
                "wdk_metadata_configurations should have exactly one element because of the \
                 .len() check above",
            )
            .try_into()?,
        _ => {
            // TODO: add a test for this
            return Err(ConfigError::MultipleWDKConfigurationsDetected {
                wdk_configurations: wdk_metadata_configurations,
            });
        }
    })
}
