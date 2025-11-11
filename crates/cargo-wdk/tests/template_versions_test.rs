// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! Test to verify that template Cargo.toml files use the correct versions
//! from the workspace configuration.

use std::{collections::HashMap, path::PathBuf};

use cargo_metadata::MetadataCommand;
use toml::Value;

/// Get the workspace versions from the root Cargo.toml
fn get_workspace_versions() -> HashMap<String, String> {
    let metadata = MetadataCommand::new()
        .manifest_path("../../Cargo.toml")
        .exec()
        .expect("Failed to get cargo metadata");

    let mut versions = HashMap::new();

    // Get workspace dependencies
    let workspace_root = metadata.workspace_root.as_std_path();
    let cargo_toml_path = workspace_root.join("Cargo.toml");
    let cargo_toml_content =
        std::fs::read_to_string(&cargo_toml_path).expect("Failed to read workspace Cargo.toml");
    let cargo_toml: Value =
        toml::from_str(&cargo_toml_content).expect("Failed to parse workspace Cargo.toml");

    if let Some(workspace) = cargo_toml.get("workspace") {
        if let Some(deps) = workspace.get("dependencies") {
            if let Some(deps_table) = deps.as_table() {
                for (name, value) in deps_table {
                    if name.starts_with("wdk") {
                        if let Some(version) = value.get("version") {
                            if let Some(version_str) = version.as_str() {
                                versions.insert(name.clone(), version_str.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    versions
}

/// Parse a template Cargo.toml and extract dependency versions
fn get_template_versions(template_path: PathBuf) -> HashMap<String, String> {
    let template_content =
        std::fs::read_to_string(&template_path).expect("Failed to read template file");
    let template_toml: Value =
        toml::from_str(&template_content).expect("Failed to parse template TOML");

    let mut versions = HashMap::new();

    // Check build-dependencies
    if let Some(build_deps) = template_toml.get("build-dependencies") {
        if let Some(build_deps_table) = build_deps.as_table() {
            for (name, value) in build_deps_table {
                if name.starts_with("wdk") {
                    if let Some(version_str) = value.as_str() {
                        versions.insert(name.clone(), version_str.to_string());
                    }
                }
            }
        }
    }

    // Check dependencies
    if let Some(deps) = template_toml.get("dependencies") {
        if let Some(deps_table) = deps.as_table() {
            for (name, value) in deps_table {
                if name.starts_with("wdk") {
                    if let Some(version_str) = value.as_str() {
                        versions.insert(name.clone(), version_str.to_string());
                    }
                }
            }
        }
    }

    versions
}

#[test]
fn kmdf_template_versions_match_workspace() {
    let workspace_versions = get_workspace_versions();
    let template_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("templates")
        .join("kmdf")
        .join("Cargo.toml.tmp");
    let template_versions = get_template_versions(template_path);

    // KMDF template should have: wdk-build, wdk, wdk-alloc, wdk-panic, wdk-sys
    for (dep_name, template_version) in &template_versions {
        let workspace_version = workspace_versions
            .get(dep_name)
            .unwrap_or_else(|| panic!("Dependency {dep_name} not found in workspace"));
        assert_eq!(
            template_version, workspace_version,
            "KMDF template: {dep_name} version mismatch. Template has {template_version}, \
             workspace has {workspace_version}"
        );
    }
}

#[test]
fn umdf_template_versions_match_workspace() {
    let workspace_versions = get_workspace_versions();
    let template_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("templates")
        .join("umdf")
        .join("Cargo.toml.tmp");
    let template_versions = get_template_versions(template_path);

    // UMDF template should have: wdk-build, wdk, wdk-sys
    for (dep_name, template_version) in &template_versions {
        let workspace_version = workspace_versions
            .get(dep_name)
            .unwrap_or_else(|| panic!("Dependency {dep_name} not found in workspace"));
        assert_eq!(
            template_version, workspace_version,
            "UMDF template: {dep_name} version mismatch. Template has {template_version}, \
             workspace has {workspace_version}"
        );
    }
}

#[test]
fn wdm_template_versions_match_workspace() {
    let workspace_versions = get_workspace_versions();
    let template_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("templates")
        .join("wdm")
        .join("Cargo.toml.tmp");
    let template_versions = get_template_versions(template_path);

    // WDM template should have: wdk-build, wdk, wdk-alloc, wdk-panic, wdk-sys
    for (dep_name, template_version) in &template_versions {
        let workspace_version = workspace_versions
            .get(dep_name)
            .unwrap_or_else(|| panic!("Dependency {dep_name} not found in workspace"));
        assert_eq!(
            template_version, workspace_version,
            "WDM template: {dep_name} version mismatch. Template has {template_version}, workspace \
             has {workspace_version}"
        );
    }
}
