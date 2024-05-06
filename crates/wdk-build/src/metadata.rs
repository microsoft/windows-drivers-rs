use std::{
    borrow::Borrow,
    collections::{HashMap, HashSet},
    env,
    path::{Path, PathBuf},
};

use cargo_metadata::{CargoOpt, Metadata, MetadataCommand};
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
pub fn detect_driver_config() -> Result<DriverConfig, ConfigError> {
    // TODO: check that if this auto reruns if cargo.toml's change
    let manifest_path = find_top_level_cargo_manifest();

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

        // No driver configurations were detected in the workspace or package manifests. In this
        // situation, detect driver configurations enabled by features(i.e. a feature brings in a
        // crate which contains a wdk metadata section). This is supported to enable scenerios where
        // a wdk-dependent library author may want to use features to enable running builds/tests
        // with different WDK configurations. Note: bringing in a wdk configuration via a feature
        // will affect the entire build graph

        // (Err(ConfigError::NoWDKConfigurationsDetected),
        // Err(ConfigError::NoWDKConfigurationsDetected)) => { if metadata
        //     .workspace_packages()
        //     .iter()
        //     .find(|package| package.name == current_package_name)
        //     .is_some()
        // {
        //     let enabled_features = get_enabled_cargo_features_in_current_package(
        //         current_package_name,
        //         metadata,
        //     )?;

        //     if !enabled_features.is_empty() {
        //         info!(
        //             "0 driver configurations found. Attempting to find driver \
        //                 configurations brought in by features. Currently enable features: \
        //                 {enabled_features:#?}"
        //         );

        //         let metadata = MetadataCommand::new()
        //             .manifest_path(&manifest_path)
        //             .features(CargoOpt::SomeFeatures(enabled_features))
        //             .exec()?;

        //         return parse_metadata_for_driver_config(metadata, manifest_path);
        //     }
        // }
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
/// directories (ex. via `--target-dir`).
fn find_top_level_cargo_manifest() -> PathBuf {
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

/// Returns a `Vec<String>` of all the feature names that are enabled for the
/// currently compiling crate. This function relies on the
/// `CARGO_FEATURE_<FEATURE_NAME>` environment variables that Cargo exposes in
/// build scripts, so it only functions when `current_package_name` is the same
/// as the package currently being compiled.
///
/// # Panics
///
/// This function will panic if it cannot determine the name of the feature
/// being enabled. This is due to the non-unique mapping of
/// `CARGO_FEATURE_<FEATURE_NAME>` to the feature name: https://github.com/rust-lang/cargo/issues/3702
///
/// This function will also panic if called from outside a Cargo build script.
fn get_enabled_cargo_features_in_current_package(
    current_package_name: impl AsRef<str>,
    metadata: impl Borrow<Metadata>,
) -> Result<Vec<String>, ConfigError> {
    let current_package_name = current_package_name.as_ref();
    let metadata = metadata.borrow();

    let current_package_features = metadata
        .packages
        .iter()
        .find_map(|package| {
            if package.name == current_package_name {
                return Some(package.features.keys());
            }
            None
        })
        .unwrap_or_else(|| {
            panic!(
                "Could not find {} package in Cargo Metadata output",
                current_package_name
            )
        });

    let env_var_to_feature_name_hashmap =
        create_env_var_to_feature_name_hashmap(current_package_features);

    Ok(env::vars()
        .filter_map(|(env_var, _)| {
            if let Some(feature_name) = env_var_to_feature_name_hashmap.get(&env_var) {
                return Some(feature_name.clone());
            }
            None
        })
        .collect())
}

/// Creates a HashMap that maps Cargo's `CARGO_FEATURE_<FEATURE NAME>`
/// environment variable names to Cargo Feature names
///
/// # Panics
///
/// This function will panic if two or more feature names resolve to the same
/// environment variable name
fn create_env_var_to_feature_name_hashmap(
    features: impl IntoIterator<Item = impl Into<String>>,
) -> HashMap<String, String> {
    let mut hashmap = HashMap::new();
    for feature_name in features.into_iter().map(|feature| feature.into()) {
        let env_var_name = format!(
            "CARGO_FEATURE_{}",
            feature_name.to_uppercase().replace('-', "_")
        );
        if let Some(existing_feature_name) =
            hashmap.insert(env_var_name.clone(), feature_name.clone())
        {
            panic!(
                "Two or more feature names resolve to the same env var:\nenv_var: \
                 {env_var_name}\noffending feature names: [{existing_feature_name}, \
                 {feature_name}]"
            );
        }
    }
    hashmap
}

#[cfg(test)]
mod tests {
    use super::*;

    mod create_env_var_to_feature_name_hashmap {
        use super::*;

        #[test]
        fn unique_feature_names() {
            let feature_names = vec!["feature-name-1", "feature-name-2", "feature-name-3"];
            let env_var_to_feature_name_hashmap: HashMap<String, String> =
                create_env_var_to_feature_name_hashmap(feature_names);

            assert_eq!(env_var_to_feature_name_hashmap.len(), 3);
            assert_eq!(
                env_var_to_feature_name_hashmap["CARGO_FEATURE_FEATURE_NAME_1"],
                "feature-name-1".to_string()
            );
            assert_eq!(
                env_var_to_feature_name_hashmap["CARGO_FEATURE_FEATURE_NAME_2"],
                "feature-name-2".to_string()
            );
            assert_eq!(
                env_var_to_feature_name_hashmap["CARGO_FEATURE_FEATURE_NAME_3"],
                "feature-name-3".to_string()
            );
        }

        #[test]
        #[should_panic(expected = "Two or more feature names resolve to the same env \
                                   var:\nenv_var: CARGO_FEATURE_FEATURE_NAME\noffending feature \
                                   names: [feature-name, feature_name]")]
        fn duplicate_feature_names_because_of_hyphen_conversion() {
            let feature_names = vec!["feature-name", "feature_name"];
            let _env_var_to_feature_name_hashmap =
                create_env_var_to_feature_name_hashmap(feature_names);
        }

        #[test]
        #[should_panic(expected = "Two or more feature names resolve to the same env \
                                   var:\nenv_var: CARGO_FEATURE_FEAT_FOO\noffending feature \
                                   names: [FEAT_FOO, feat_foo]")]
        fn duplicate_feature_names_because_of_case_conversion() {
            let feature_names = vec!["FEAT_FOO", "feat_foo"];
            let _env_var_to_feature_name_hashmap =
                create_env_var_to_feature_name_hashmap(feature_names);
        }
    }
}
