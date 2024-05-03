use std::{
    collections::{HashMap, HashSet},
    env,
    path::{Path, PathBuf},
};

use cargo_metadata::{CargoOpt, MetadataCommand};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{ConfigError, DriverConfig, DriverType, KMDFConfig, UMDFConfig};

// "This is a false-positive of this lint since metadata is a private module and WDKMetadata is re-exported to be at the crate root. See https://github.com/rust-lang/rust-clippy/issues/8524"
#[allow(clippy::module_name_repetitions)]

// pub trait MetadataExt {
//     type MetadataError;

//     fn get_package_id(&self, package_name: impl AsRef<str>) -> Result<cargo_metadata::PackageId, Self::MetadataError>;

// }

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

// impl MetadataExt for cargo_metadata::Metadata {
//     type MetadataError = ();
    
//     fn get_package_id(&self, package_name: impl AsRef<str>) -> Result<cargo_metadata::PackageId, Self::MetadataError> {
//         todo!("Implement this")
//     }
// }

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

    // TODO: should all metadata commands be operating on the resolved section
    // instead of the packages?
    let cargo_metadata_packages_list: Vec<cargo_metadata::Package> = MetadataCommand::new()
        .manifest_path(&manifest_path)
        .features(CargoOpt::SomeFeatures(detect_enabled_cargo_features(
            manifest_path,
        )?))
        .exec()?
        .packages;

    // TODO: handle workspace metadata

    // Only one configuration of WDK is allowed per dependency graph
    let wdk_metadata_configurations = cargo_metadata_packages_list
        .into_iter()
        .filter_map(|package| {
            println!("{:?}", package.name);
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

/// Find the path the the toplevel Cargo manifest of the currently executing
/// Cargo subcommand. This should resolve to either:
/// 1. the `Cargo.toml` of the package where the Cargo subcommand (build, check, etc.) was run
/// 2. the `Cargo.toml` provided to the `--manifest-path` argument to the Cargo subcommand
/// 3. the `Cargo.toml` of the workspace that contains the package pointed to by
///    1 or 2
/// 
/// The returned path should be a manifest in the same directory of the
/// lockfile. This does not support invokations that use non-default target
/// directories (ex. via `--target-dir`).
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

/// Detect the currently enabled cargo features of the crate that is being built
/// and return them as a list of strings
///
/// # Panics
///
/// This function will panic if it cannot determine the name of the feature
/// being enabled. This is due to the non-unique mapping of
/// `CARGO_FEATURE_<FEATURE_NAME>` to the feature name: https://github.com/rust-lang/cargo/issues/3702
///
/// This function will also panic if called from outside a Cargo build script.
pub fn detect_enabled_cargo_features<P>(manifest_path: P) -> Result<Vec<String>, ConfigError>
where
    P: AsRef<Path>,
{
    let current_package = env::var("CARGO_PKG_NAME")
        .expect("CARGO_PKG_NAME should be set by Cargo and be valid UTF-8");

    let cargo_metadata_packages_list: Vec<cargo_metadata::Package> = MetadataCommand::new()
        .manifest_path(manifest_path.as_ref())
        .exec()?
        .packages;

    let current_package_features = cargo_metadata_packages_list
        .into_iter()
        .find_map(|package| {
            if package.name == current_package {
                return Some(package.features.into_keys());
            }
            None
        })
        .unwrap_or_else(|| {
            panic!(
                "Could not find {} package in Cargo Metadata output",
                current_package
            )
        });

    let env_var_to_feature_name_hashmap =
        create_env_var_to_feature_name_hashmap(current_package_features);

    Ok(env::vars()
        .filter_map(|(env_var, _)| {
            dbg!(&env_var);
            if let Some(feature_name) = env_var_to_feature_name_hashmap.get(&env_var) {
                dbg!(feature_name);
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
