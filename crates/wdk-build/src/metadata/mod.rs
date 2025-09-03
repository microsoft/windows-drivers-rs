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

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::Wdk;
    use crate::{metadata::TryFromCargoMetadataError, DriverConfig, KmdfConfig};

    #[test]
    fn exactly_one_wdk_configuration() {
        let cwd = PathBuf::from("C:\\tmp");
        let driver_type = "KMDF";
        let driver_name = "sample-kmdf";
        let driver_version = "0.0.1";
        let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
        let (workspace_member, package) =
            get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(wdk_metadata));

        let cargo_toml_metadata =
            get_cargo_metadata(&cwd, vec![package], &[workspace_member], None);
        let cargo_toml_metadata =
            serde_json::from_str::<cargo_metadata::Metadata>(&cargo_toml_metadata)
                .expect("Failed to parse cargo metadata in set_up_standalone_driver_project");

        let wdk = Wdk::try_from(&cargo_toml_metadata);
        assert!(wdk.is_ok());
        assert!(matches!(
            wdk.unwrap().driver_model,
            DriverConfig::Kmdf(KmdfConfig {
                kmdf_version_major: 1,
                target_kmdf_version_minor: 33,
                minimum_kmdf_version_minor: None
            })
        ));
    }

    #[test]
    fn multiple_wdk_configurations() {
        let cwd = PathBuf::from("C:\\tmp");
        let driver_type = "KMDF";
        let driver_name = "sample-kmdf";
        let driver_version = "0.0.1";
        let wdk_metadata1 = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
        let (workspace_member1, package1) =
            get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(wdk_metadata1));

        let wdk_metadata2 = get_cargo_metadata_wdk_metadata(driver_type, 1, 35);
        let (workspace_member2, package2) =
            get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(wdk_metadata2));

        let cargo_toml_metadata = get_cargo_metadata(
            &cwd,
            vec![package1, package2],
            &[workspace_member1, workspace_member2],
            None,
        );
        let cargo_toml_metadata =
            serde_json::from_str::<cargo_metadata::Metadata>(&cargo_toml_metadata)
                .expect("Failed to parse cargo metadata in set_up_standalone_driver_project");

        let wdk = Wdk::try_from(&cargo_toml_metadata);
        assert!(matches!(
            wdk.expect_err("expected an error"),
            TryFromCargoMetadataError::MultipleWdkConfigurationsDetected {
                wdk_metadata_configurations: _
            }
        ));
    }

    #[test]
    fn no_wdk_configuration_detected() {
        let cwd = PathBuf::from("C:\\tmp");
        let driver_name = "sample-kmdf";
        let driver_version = "0.0.1";
        let (workspace_member, package) =
            get_cargo_metadata_package(&cwd, driver_name, driver_version, None);

        let cargo_toml_metadata =
            get_cargo_metadata(&cwd, vec![package], &[workspace_member], None);
        let cargo_toml_metadata =
            serde_json::from_str::<cargo_metadata::Metadata>(&cargo_toml_metadata)
                .expect("Failed to parse cargo metadata in set_up_standalone_driver_project");

        let wdk = Wdk::try_from(&cargo_toml_metadata);
        assert!(matches!(
            wdk.expect_err("expected an error"),
            TryFromCargoMetadataError::NoWdkConfigurationsDetected
        ));
    }

    #[test]
    fn invalid_wdk_metadata() {
        let cwd = PathBuf::from("C:\\tmp");
        let driver_name = "sample-kmdf";
        let driver_version = "0.0.1";
        let wdk_metadata = TestWdkMetadata(
            r#"
                {{
                    "wdk": {{
                        "driver-model": {{
                            "random-key": "random-value"
                        }}
                    }}
                }}
            "#
            .to_string(),
        );
        let (workspace_member, package) =
            get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(wdk_metadata));

        let cargo_toml_metadata =
            get_cargo_metadata(&cwd, vec![package], &[workspace_member], None);
        let cargo_toml_metadata =
            serde_json::from_str::<cargo_metadata::Metadata>(&cargo_toml_metadata)
                .expect("Failed to parse cargo metadata in set_up_standalone_driver_project");

        let wdk = Wdk::try_from(&cargo_toml_metadata);
        assert!(matches!(
            wdk.expect_err("expected an error"),
            TryFromCargoMetadataError::WdkMetadataDeserialization {
                metadata_source: _,
                error_source: _
            }
        ));
    }

    #[derive(Clone)]
    struct TestMetadataPackage(String);
    #[derive(Clone)]
    struct TestMetadataWorkspaceMemberId(String);
    #[derive(Clone)]
    struct TestWdkMetadata(String);

    fn get_cargo_metadata(
        root_dir: &Path,
        package_list: Vec<TestMetadataPackage>,
        workspace_member_list: &[TestMetadataWorkspaceMemberId],
        metadata: Option<TestWdkMetadata>,
    ) -> String {
        let metadata_section = match metadata {
            Some(metadata) => metadata.0,
            None => String::from("null"),
        };
        format!(
            r#"
    {{
        "target_directory": "{}",
        "workspace_root": "{}",
        "packages": [
            {}
            ],
        "workspace_members": [{}],
        "metadata": {},
        "version": 1
    }}"#,
            root_dir.join("target").to_string_lossy().escape_default(),
            root_dir.to_string_lossy().escape_default(),
            package_list
                .into_iter()
                .map(|p| p.0)
                .collect::<Vec<String>>()
                .join(", "),
            // Require quotes around each member
            workspace_member_list
                .iter()
                .map(|s| format!("\"{}\"", s.0))
                .collect::<Vec<String>>()
                .join(", "),
            metadata_section
        )
    }

    fn get_cargo_metadata_package(
        root_dir: &Path,
        default_package_name: &str,
        default_package_version: &str,
        metadata: Option<TestWdkMetadata>,
    ) -> (TestMetadataWorkspaceMemberId, TestMetadataPackage) {
        let package_id = format!(
            "path+file:///{}#{}@{}",
            root_dir.to_string_lossy().escape_default(),
            default_package_name,
            default_package_version
        );
        let metadata_section = match metadata {
            Some(metadata) => metadata.0,
            None => String::from("null"),
        };
        (
            TestMetadataWorkspaceMemberId(package_id),
            #[allow(clippy::format_in_format_args)]
            TestMetadataPackage(format!(
                r#"
            {{
            "name": "{}",
            "version": "{}",
            "id": "{}",
            "dependencies": [],
            "targets": [
                {{
                    "kind": [
                        "cdylib"
                    ],
                    "crate_types": [
                        "cdylib"
                    ],
                    "name": "{}",
                    "src_path": "{}",
                    "edition": "2021",
                    "doc": true,
                    "doctest": false,
                    "test": true
                }}
            ],
            "features": {{}},
            "manifest_path": "{}",
            "authors": [],
            "categories": [],
            "keywords": [],
            "edition": "2021",
            "metadata": {}
        }}
        "#,
                default_package_name,
                default_package_version,
                format!(
                    "path+file:///{}#{}@{}",
                    root_dir.to_string_lossy().escape_default(),
                    default_package_name,
                    default_package_version
                ),
                default_package_name,
                root_dir
                    .join("src")
                    .join("main.rs")
                    .to_string_lossy()
                    .escape_default(),
                root_dir
                    .join("Cargo.toml")
                    .to_string_lossy()
                    .escape_default(),
                metadata_section
            )),
        )
    }

    fn get_cargo_metadata_wdk_metadata(
        driver_type: &str,
        kmdf_version_major: u8,
        target_kmdf_version_minor: u8,
    ) -> TestWdkMetadata {
        TestWdkMetadata(format!(
            r#"
        {{
            "wdk": {{
                "driver-model": {{
                    "driver-type": "{}",
                    "{}-version-major": {},
                    "target-{}-version-minor": {}
                }}
            }}
        }}
    "#,
            driver_type,
            driver_type.to_ascii_lowercase(),
            kmdf_version_major,
            driver_type.to_ascii_lowercase(),
            target_kmdf_version_minor
        ))
    }
}
