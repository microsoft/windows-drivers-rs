mod error;
mod map;
mod ser;

use std::{
    borrow::Borrow,
    collections::HashSet,
    path::{Path, PathBuf},
};

use cargo_metadata::{Metadata, MetadataCommand};
pub use ser::{to_map, to_map_with_prefix};
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
    CargoMetadataError(#[from] cargo_metadata::Error),

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
}

impl From<WDKMetadata> for DriverConfig {
    fn from(wdk_metadata: WDKMetadata) -> Self {
        // Ok(match wdk_metadata.driver_model.driver_type {
        //     DriverType::WDM => Self::WDM(),
        //     DriverType::KMDF => Self::KMDF(KMDFConfig {
        //         kmdf_version_major:
        // wdk_metadata.driver_model.kmdf_version_major.ok_or_else(             ||
        // TryFromWDKMetadataError::MissingKMDFMetadata {                 //
        // TODO: fix population                 missing_metadata_field:
        // stringify!(WDKMetadata.d).to_string(),             },
        //         )?,
        //         kmdf_version_minor: wdk_metadata
        //             .driver_model
        //             .target_kmdf_version_minor
        //             // tODO: should error if not present
        //             .unwrap_or(33),
        //     }),
        //     DriverType::UMDF => Self::UMDF(UMDFConfig {
        //         // tODO: should error if not present
        //         umdf_version_major:
        // wdk_metadata.driver_model.kmdf_version_major.unwrap_or(2),
        //         umdf_version_minor: wdk_metadata
        //             .driver_model
        //             .target_kmdf_version_minor
        //             // tODO: should error if not present
        //             .unwrap_or(33),
        //     }),
        // })
        wdk_metadata.driver_model
    }
}

impl TryFromCargoMetadata for WDKMetadata {
    type Error = TryFromCargoMetadataError;

    /// TODO: add docs
    /// # Panics
    ///
    /// todo
    ///
    /// # Errors
    ///
    /// todo
    fn try_from_cargo_metadata(manifest_path: impl AsRef<Path>) -> Result<Self, Self::Error> {
        let manifest_path = manifest_path.as_ref();

        // TODO: this works for the top level manifest, but it needs to be emitted for
        // any toml in the workspace
        println!("cargo::rerun-if-changed={}", manifest_path.display());

        let metadata = MetadataCommand::new().manifest_path(manifest_path).exec()?;

        let wdk_metadata_from_workspace_manifest = parse_workspace_wdk_metadata(&metadata);
        let wdk_metadata_from_package_manifests = parse_packages_wdk_metadata(&metadata);

        // TODO: add ws level test:
        //////////////ws level tests: https://stackoverflow.com/a/71461114/10173605

        match (
            wdk_metadata_from_workspace_manifest,
            wdk_metadata_from_package_manifests,
        ) {
            // Either the workspace or package manifest has a driver configuration
            (Ok(wdk_metadata), Err(TryFromCargoMetadataError::NoWDKConfigurationsDetected))
            | (Err(TryFromCargoMetadataError::NoWDKConfigurationsDetected), Ok(wdk_metadata)) => {
                Ok(wdk_metadata)
            }

            // Both the workspace and package manifest have a driver configuration. This is only
            // allowed if they are the same
            (Ok(workspace_wdk_metadata), Ok(packages_wdk_metadata)) => {
                if workspace_wdk_metadata != packages_wdk_metadata {
                    return Err(
                        TryFromCargoMetadataError::MultipleWDKConfigurationsDetected {
                            wdk_metadata_configurations: [
                                workspace_wdk_metadata,
                                packages_wdk_metadata,
                            ]
                            .into_iter()
                            .collect(),
                        },
                    );
                }

                Ok(workspace_wdk_metadata)
            }

            // Workspace has a driver configuration, and multiple conflicting driver configurations
            // were detected in the package manifests. This is a special case so that
            // the error can list all the offending driver configurations
            (
                Ok(workspace_wdk_metadata),
                Err(TryFromCargoMetadataError::MultipleWDKConfigurationsDetected {
                    mut wdk_metadata_configurations,
                }),
            ) => {
                wdk_metadata_configurations.insert(workspace_wdk_metadata);
                Err(
                    TryFromCargoMetadataError::MultipleWDKConfigurationsDetected {
                        wdk_metadata_configurations,
                    },
                )
            }

            (unhandled_error @ Err(_), _) | (_, unhandled_error @ Err(_)) => unhandled_error,
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
    // TODO need rerun on OUT_DIR and cargo.toml changes?

    out_dir
        .ancestors()
        .find(|path| path.join("Cargo.lock").exists())
        .unwrap()
        .join("Cargo.toml")
    // TODO: error handling
}

/// todo
fn parse_packages_wdk_metadata(
    metadata: impl Borrow<Metadata>,
) -> Result<WDKMetadata, TryFromCargoMetadataError> {
    let metadata = metadata.borrow();

    let wdk_metadata_configurations = metadata
        .packages
        .iter()
        .filter_map(|package| {
            if let Some(wdk_metadata) = package.metadata.get("wdk") {
                // TODO: error handling for unwrap
                return Some(serde_json::from_value::<WDKMetadata>(wdk_metadata.clone()).unwrap());
            };
            None
        })
        .collect::<HashSet<_>>();

    // Only one configuration of WDK is allowed per dependency graph
    match wdk_metadata_configurations.len() {
        1 => Ok(wdk_metadata_configurations.into_iter().next().expect(
            "wdk_metadata_configurations should have exactly one element because of the .len() \
             check above",
        )),

        0 => {
            // TODO: add a test for this
            Err(TryFromCargoMetadataError::NoWDKConfigurationsDetected {})
        }

        _ => {
            // TODO: add a test for this
            Err(
                TryFromCargoMetadataError::MultipleWDKConfigurationsDetected {
                    wdk_metadata_configurations,
                },
            )
        }
    }
}

fn parse_workspace_wdk_metadata(
    metadata: impl Borrow<Metadata>,
) -> Result<WDKMetadata, TryFromCargoMetadataError> {
    let metadata = metadata.borrow();

    if let Some(wdk_metadata) = metadata.workspace_metadata.get("wdk") {
        // TODO: error handling for this unwrap when failure to parse json value into
        // wdkmetadata
        return Ok(serde_json::from_value::<WDKMetadata>(wdk_metadata.clone()).unwrap());
    }

    Err(TryFromCargoMetadataError::NoWDKConfigurationsDetected)
}
