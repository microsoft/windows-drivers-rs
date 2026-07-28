// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! This module contains the core functionality of the cargo-wdk utility. It
//! include modules which implement the business logic and common types that can
//! be shared across different actions. The `action` modules that implement the
//! business logic of the cargo-wdk utility are:
//! * `new` - New action module
//! * `build` - Build action module
//! * `clean` - Clean action module
pub mod build;
pub mod clean;
pub mod new;

use std::{
    fmt::{self, Display},
    path::{Path, PathBuf, absolute},
    str::FromStr,
};

use clap_cargo::Features;
use mockall_double::double;
use wdk_build::CpuArchitecture;

use crate::providers::fs::DirEntryInfo;
#[double]
use crate::providers::{fs::Fs, metadata::Metadata};

pub const KMDF_STR: &str = "kmdf";
pub const UMDF_STR: &str = "umdf";
pub const WDM_STR: &str = "wdm";
/// `x86_64/Amd64` target triple name
const X86_64_TARGET_TRIPLE_NAME: &str = "x86_64-pc-windows-msvc";
/// `aarch64/Arm64` target triple name
const AARCH64_TARGET_TRIPLE_NAME: &str = "aarch64-pc-windows-msvc";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Dev,
    Release,
}
impl FromStr for Profile {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "dev" => std::result::Result::Ok(Self::Dev),
            "release" => std::result::Result::Ok(Self::Release),
            _ => Err(format!("'{s}' is not a valid profile")),
        }
    }
}
impl Display for Profile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Dev => "dev",
            Self::Release => "release",
        };
        write!(f, "{s}")
    }
}

/// Converts `CpuArchitecture` to its corresponding target triple name.
#[must_use]
pub fn to_target_triple(cpu_arch: CpuArchitecture) -> String {
    match cpu_arch {
        CpuArchitecture::Amd64 => X86_64_TARGET_TRIPLE_NAME.to_string(),
        CpuArchitecture::Arm64 => AARCH64_TARGET_TRIPLE_NAME.to_string(),
    }
}

/// Enum of driver types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverType {
    Kmdf,
    Umdf,
    Wdm,
}

impl FromStr for DriverType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            KMDF_STR => Ok(Self::Kmdf),
            UMDF_STR => Ok(Self::Umdf),
            WDM_STR => Ok(Self::Wdm),
            _ => Err(format!("'{s}' is not a valid driver type")),
        }
    }
}

impl Display for DriverType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Kmdf => KMDF_STR,
            Self::Umdf => UMDF_STR,
            Self::Wdm => WDM_STR,
        };
        write!(f, "{s}")
    }
}

/// Resolves the root of the workspace for a working directory that has no
/// `Cargo.toml` of its own.
fn find_workspace_root(
    metadata: &Metadata,
    fs: &Fs,
    working_dir: &Path,
    dirs: &[DirEntryInfo],
    locked: bool,
    features: &Features,
) -> Option<PathBuf> {
    let working_dir_trimmed: PathBuf = working_dir
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .into();
    let other_options = if locked {
        vec!["--locked".to_string()]
    } else {
        Vec::new()
    };
    let cargo_metadata = metadata
        .get_cargo_metadata_at_path(&working_dir_trimmed, other_options, features)
        .ok()?;

    let member_dirs: Vec<PathBuf> = cargo_metadata
        .workspace_packages()
        .iter()
        .filter_map(|p| {
            p.manifest_path
                .parent()
                .and_then(|path| absolute(path.as_std_path()).ok())
        })
        .collect();

    let is_emulated_workspace = dirs.iter().any(|entry| {
        entry.is_dir
            && fs.exists(&entry.path.join("Cargo.toml"))
            && absolute(&entry.path).is_ok_and(|child_dir| {
                !member_dirs
                    .iter()
                    .any(|member| member.starts_with(&child_dir))
            })
    });
    if is_emulated_workspace {
        return None;
    }

    absolute(cargo_metadata.workspace_root.as_std_path()).ok()
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use clap_cargo::Features;
    use mockall::predicate::eq;
    use mockall_double::double;

    use super::find_workspace_root;
    use crate::providers::fs::DirEntryInfo;
    #[double]
    use crate::providers::{fs::Fs, metadata::Metadata};

    fn dir_entry(path: PathBuf, is_dir: bool) -> DirEntryInfo {
        DirEntryInfo { path, is_dir }
    }

    /// A `Metadata` mock that reports the directory is not inside any
    /// workspace.
    fn metadata_not_in_workspace() -> Metadata {
        let mut metadata = Metadata::default();
        metadata
            .expect_get_cargo_metadata_at_path()
            .returning(|_, _, _| {
                Err(cargo_metadata::Error::CargoMetadata {
                    stderr: "not a workspace".to_string(),
                })
            });
        metadata
    }

    /// A `Metadata` mock that reports a single workspace member at
    /// `member_dir`, rooted at `workspace_root`.
    fn metadata_owning_dir(workspace_root: &Path, member_dir: &Path) -> Metadata {
        let member_fwd = member_dir.to_string_lossy().replace('\\', "/");
        let member_fwd = member_fwd.trim_start_matches("//?/").to_string();
        let id = format!("path+file:///{member_fwd}#pkg@0.1.0");
        let json = serde_json::json!({
            "target_directory": workspace_root.join("target").to_string_lossy(),
            "workspace_root": workspace_root.to_string_lossy(),
            "packages": [{
                "name": "pkg",
                "version": "0.1.0",
                "id": id,
                "dependencies": [],
                "targets": [{
                    "kind": ["lib"],
                    "crate_types": ["lib"],
                    "name": "pkg",
                    "src_path": member_dir.join("src").join("lib.rs").to_string_lossy(),
                    "edition": "2021",
                    "doc": true,
                    "doctest": false,
                    "test": true
                }],
                "features": {},
                "manifest_path": member_dir.join("Cargo.toml").to_string_lossy(),
                "authors": [],
                "categories": [],
                "keywords": [],
                "edition": "2021",
                "metadata": null
            }],
            "workspace_members": [id],
            "metadata": null,
            "version": 1
        });
        let parsed: cargo_metadata::Metadata =
            serde_json::from_value(json).expect("valid cargo metadata");
        let mut metadata = Metadata::default();
        metadata
            .expect_get_cargo_metadata_at_path()
            .returning(move |_, _, _| Ok(parsed.clone()));
        metadata
    }

    mod find_workspace_root {
        use super::*;

        #[test]
        fn returns_none_when_not_in_a_workspace() {
            // A loose directory not inside any Cargo workspace.
            let metadata = metadata_not_in_workspace();
            let fs = Fs::default();
            let working_dir = PathBuf::from("C:\\tmp\\loose");
            assert_eq!(
                find_workspace_root(
                    &metadata,
                    &fs,
                    &working_dir,
                    &[],
                    false,
                    &Features::default()
                ),
                None
            );
        }

        #[test]
        fn returns_workspace_root_from_intermediate_subdirectory() {
            // An intermediate directory that is an ancestor of a workspace member.
            let workspace_root = PathBuf::from("C:\\tmp\\ws");
            let group_dir = workspace_root.join("group");
            let member_dir = group_dir.join("pkg");
            let metadata = metadata_owning_dir(&workspace_root, &member_dir);
            let mut fs = Fs::default();
            fs.expect_exists()
                .with(eq(member_dir.join("Cargo.toml")))
                .returning(|_| true);
            let dirs = [dir_entry(member_dir, true)];
            assert_eq!(
                find_workspace_root(
                    &metadata,
                    &fs,
                    &group_dir,
                    &dirs,
                    false,
                    &Features::default()
                ),
                Some(workspace_root)
            );
        }

        #[test]
        fn returns_workspace_root_from_dir_without_child_projects() {
            // A directory inside the workspace with no child projects (e.g. `docs/` or a
            // member's `src/`).
            let workspace_root = PathBuf::from("C:\\tmp\\ws");
            let docs_dir = workspace_root.join("docs");
            let member_dir = workspace_root.join("pkg");
            let metadata = metadata_owning_dir(&workspace_root, &member_dir);
            let fs = Fs::default();
            assert_eq!(
                find_workspace_root(&metadata, &fs, &docs_dir, &[], false, &Features::default()),
                Some(workspace_root)
            );
        }

        #[test]
        fn returns_none_for_emulated_workspace_with_non_member_children() {
            // An emulated-workspace directory whose children are independent projects
            // excluded from the workspace.
            let workspace_root = PathBuf::from("C:\\tmp\\ws");
            let emulated_dir = workspace_root.join("examples");
            let member_dir = workspace_root.join("crates").join("pkg");
            let child = emulated_dir.join("proj-a");
            let metadata = metadata_owning_dir(&workspace_root, &member_dir);
            let mut fs = Fs::default();
            fs.expect_exists()
                .with(eq(child.join("Cargo.toml")))
                .returning(|_| true);
            let dirs = [dir_entry(child, true)];
            assert_eq!(
                find_workspace_root(
                    &metadata,
                    &fs,
                    &emulated_dir,
                    &dirs,
                    false,
                    &Features::default()
                ),
                None
            );
        }
    }
}
