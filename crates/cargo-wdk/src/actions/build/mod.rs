// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! This module contains the `BuildAction` struct and its associated methods
//! for orchestrating the build and packaging of a driver project. It consists
//! the logic to build and package standalone projects, workspaces, individual
//! members in a workspace and emulated workspaces. It consists of two tasks -
//! `BuildTask` that handles the build phase and the `PackageTask` that handles
//! the package phase.

mod build_task;
mod error;
mod package_task;
#[cfg(test)]
mod tests;
use std::{
    path::{Path, PathBuf, absolute},
    result::Result::Ok,
};

use anyhow::Result;
use build_task::BuildTask;
use cargo_metadata::{Metadata as CargoMetadata, Package, TargetKind};
use error::BuildActionError;
use mockall_double::double;
use package_task::{PackageTask, PackageTaskParams};
use tracing::{debug, error as err, info, warn};
use wdk_build::metadata::{TryFromCargoMetadataError, Wdk};

use crate::actions::{Profile, TargetArch, to_target_triple};
#[double]
use crate::providers::{exec::CommandExec, fs::Fs, metadata::Metadata, wdk_build::WdkBuild};

pub struct BuildActionParams<'a> {
    pub working_dir: &'a Path,
    pub profile: Option<&'a Profile>,
    pub target_arch: TargetArch,
    pub verify_signature: bool,
    pub is_sample_class: bool,
    pub verbosity_level: clap_verbosity_flag::Verbosity,
}

/// Action that orchestrates the build and package of a driver project. Build is
/// a pre-requisite for packaging.
pub struct BuildAction<'a> {
    working_dir: PathBuf,
    profile: Option<&'a Profile>,
    target_arch: TargetArch,
    verify_signature: bool,
    is_sample_class: bool,
    verbosity_level: clap_verbosity_flag::Verbosity,

    // Injected deps
    wdk_build: &'a WdkBuild,
    command_exec: &'a CommandExec,
    fs: &'a Fs,
    metadata: &'a Metadata,
}

impl<'a> BuildAction<'a> {
    /// Creates a new instance of `BuildAction`.
    ///
    /// # Arguments:
    /// * `params` - The `BuildActionParams` struct containing the parameters
    ///   for the build action
    /// * `wdk_build` - The WDK build provider instance
    /// * `command_exec` - The command execution provider instance
    /// * `fs` - The file system provider instance
    /// * `metadata` - The metadata provider instance
    ///
    /// # Returns
    /// * `Result<Self>` - A result containing either a new instance of
    ///   `BuildAction` on success, or an `anyhow::Error`.
    ///
    /// # Errors
    /// * [`anyhow::Error`] -  If `params.working_dir` is not a syntactically
    ///   valid path, e.g. it is empty
    pub fn new(
        params: &BuildActionParams<'a>,
        wdk_build: &'a WdkBuild,
        command_exec: &'a CommandExec,
        fs: &'a Fs,
        metadata: &'a Metadata,
    ) -> Result<Self> {
        // TODO: validate params
        Ok(Self {
            working_dir: absolute(params.working_dir)?,
            profile: params.profile,
            target_arch: params.target_arch,
            verify_signature: params.verify_signature,
            is_sample_class: params.is_sample_class,
            verbosity_level: params.verbosity_level,
            wdk_build,
            command_exec,
            fs,
            metadata,
        })
    }

    /// Entry point method to execute the packaging action flow.
    /// # Returns
    /// * `Result<Self>` - A result containing an empty tuple or an error of
    ///   type `BuildActionError`.
    /// # Errors
    /// * `BuildActionError::NotAWorkspaceMember` - If the working directory is
    ///   not a workspace member.
    /// * `BuildActionError::PackageTaskInit` - If there is an error
    ///   initializing the package task.
    /// * `BuildActionError::PackageTask` - If there is an error during the
    ///   package task process.
    /// * `BuildActionError::CargoMetadataParse` - If it is not a valid rust
    ///   project/workspace and error parsing Cargo.toml.
    /// * `BuildActionError::WdkMetadataParse` - Error Parsing WDK metadata from
    ///   Cargo.toml, not a valid driver project/workspace.
    /// * `BuildActionError::WdkBuildConfig` - If there is an error setting up
    ///   Path for the tools or when failed to detect WDK build number.
    /// * `BuildActionError::Io` - Wraps all possible IO errors.
    /// * `BuildActionError::CommandExecution` - If there is an error executing
    ///   a command.
    /// * `BuildActionError::NoValidRustProjectsInTheDirectory` - If no valid
    ///   Rust projects are found in the working directory.
    /// * `BuildActionError::OneOrMoreRustProjectsFailedToBuild` - If one or
    ///   more Rust projects fail to build in an emulated workspace.
    /// * `BuildActionError::OneOrMoreWorkspaceMembersFailedToBuild` - If one or
    ///   more workspace members fail to build inside a workspace.
    /// * `BuildActionError::BuildTask` - If there is an error during the build
    ///   task process.
    pub fn run(&self) -> Result<(), BuildActionError> {
        debug!(
            "Initialized build for project at: {}",
            self.working_dir.display()
        );
        let build_number = self.wdk_build.detect_wdk_build_number()?;
        debug!("WDK build number: {}", build_number);
        wdk_build::cargo_make::setup_path()?;
        debug!("PATH env variable is set with WDK bin and tools paths");

        // Standalone driver/driver workspace support
        if self.fs.exists(&self.working_dir.join("Cargo.toml")) {
            return self.run_from_workspace_root(&self.working_dir);
        }

        // Emulated workspaces support
        let dirs = self.fs.read_dir_entries(&self.working_dir)?;
        debug!(
            "Checking for valid Rust projects in the working directory: {}",
            self.working_dir.display()
        );

        let mut is_valid_dir_with_rust_projects = false;
        for dir in &dirs {
            if self.fs.dir_file_type(dir)?.is_dir()
                && self.fs.exists(&dir.path().join("Cargo.toml"))
            {
                debug!(
                    "Found atleast one valid Rust project directory: {}, continuing with the \
                     build flow",
                    dir.path()
                        .file_name()
                        .expect(
                            "package sub directory name ended with \"..\" which is not expected"
                        )
                        .to_string_lossy()
                );
                is_valid_dir_with_rust_projects = true;
                break;
            }
        }

        if !is_valid_dir_with_rust_projects {
            return Err(BuildActionError::NoValidRustProjectsInTheDirectory(
                self.working_dir.clone(),
            ));
        }

        info!("Building packages in {}", self.working_dir.display());

        let mut failed_atleast_one_project = false;
        for dir in dirs {
            debug!("Checking dir entry: {}", dir.path().display());
            if !self.fs.dir_file_type(&dir)?.is_dir()
                || !self.fs.exists(&dir.path().join("Cargo.toml"))
            {
                debug!("Dir entry is not a valid Rust package");
                continue;
            }

            let working_dir_path = dir.path(); // Avoids a short-lived temporary
            let sub_dir = working_dir_path
                .file_name()
                .expect("package sub directory name ended with \"..\" which is not expected")
                .to_string_lossy();

            debug!("Building package(s) in dir {sub_dir}");
            if let Err(e) = self.run_from_workspace_root(&dir.path()) {
                failed_atleast_one_project = true;
                err!(
                    "Error building project: {sub_dir}, error: {:?}",
                    anyhow::Error::new(e)
                );
            }
        }

        debug!("Done building packages in {}", self.working_dir.display());
        if failed_atleast_one_project {
            return Err(BuildActionError::OneOrMoreRustProjectsFailedToBuild(
                self.working_dir.clone(),
            ));
        }

        info!(
            "Build completed successfully for packages in {}",
            self.working_dir.display()
        );
        Ok(())
    }

    // Runs build for the given working directory and the cargo metadata
    fn run_from_workspace_root(&self, working_dir: &Path) -> Result<(), BuildActionError> {
        let cargo_metadata = &self.get_cargo_metadata(working_dir)?;
        let wdk_metadata = Wdk::try_from(cargo_metadata);
        let workspace_packages = cargo_metadata.workspace_packages();
        let workspace_root =
            absolute(cargo_metadata.workspace_root.as_std_path()).map_err(|e| {
                BuildActionError::NotAbsolute(cargo_metadata.workspace_root.clone().into(), e)
            })?;
        let target_directory =
            absolute(cargo_metadata.target_directory.as_std_path()).map_err(|e| {
                BuildActionError::NotAbsolute(cargo_metadata.target_directory.clone().into(), e)
            })?;
        if workspace_root.eq(&working_dir) {
            // If the working directory is root of a standalone project or a
            // workspace
            debug!(
                "Running from standalone project or from a root of a workspace: {}",
                working_dir.display()
            );
            let mut failed_atleast_one_workspace_member = false;
            for package in workspace_packages {
                let package_root_path: PathBuf = package
                    .manifest_path
                    .parent()
                    .expect("Unable to find package path from Cargo manifest path")
                    .into();

                let package_root_path = absolute(package_root_path.as_path())
                    .map_err(|e| BuildActionError::NotAbsolute(package_root_path.clone(), e))?;
                debug!(
                    "Building workspace member package: {}",
                    package_root_path.display()
                );

                if let Err(e) = self.build_and_package(
                    &package_root_path,
                    &wdk_metadata,
                    package,
                    &package.name,
                    &target_directory,
                ) {
                    failed_atleast_one_workspace_member = true;
                    err!(
                        "Error building the workspace member project: {}, error: {:?}",
                        package_root_path.display(),
                        anyhow::Error::new(e)
                    );
                }
            }
            if let Err(e) = wdk_metadata {
                // Ignore NoWdkConfigurationsDetected but propagate any other error
                if !matches!(e, TryFromCargoMetadataError::NoWdkConfigurationsDetected) {
                    return Err(BuildActionError::WdkMetadataParse(e));
                }
            }

            if failed_atleast_one_workspace_member {
                return Err(BuildActionError::OneOrMoreWorkspaceMembersFailedToBuild(
                    working_dir.to_owned(),
                ));
            }
        } else {
            // If the working directory is a workspace member directory
            debug!(
                "Running from a workspace member directory: {}",
                working_dir.display()
            );
            let package = workspace_packages.iter().find(|p| {
                let package_root_path: PathBuf = p
                    .manifest_path
                    .parent()
                    .expect("Unable to find package path from Cargo manifest path")
                    .into();
                absolute(package_root_path.as_path()).is_ok_and(|p| {
                    debug!("Processing workspace member package: {}", p.display());
                    p.eq(&working_dir)
                })
            });

            let package = package
                .ok_or_else(|| BuildActionError::NotAWorkspaceMember(working_dir.to_owned()))?;

            self.build_and_package(
                working_dir,
                &wdk_metadata,
                package,
                &package.name,
                &target_directory,
            )?;

            if let Err(e) = wdk_metadata {
                // Ignore NoWdkConfigurationsDetected but propagate any other error
                if !matches!(e, TryFromCargoMetadataError::NoWdkConfigurationsDetected) {
                    return Err(BuildActionError::WdkMetadataParse(e));
                }
            }
        }

        debug!(
            "Build completed successfully for path: {}",
            working_dir.display()
        );

        Ok(())
    }

    fn get_cargo_metadata(&self, working_dir: &Path) -> Result<CargoMetadata, BuildActionError> {
        let working_dir_path_trimmed: PathBuf = working_dir
            .to_string_lossy()
            .trim_start_matches("\\\\?\\")
            .into();
        let cargo_metadata = self
            .metadata
            .get_cargo_metadata_at_path(&working_dir_path_trimmed)?;
        Ok(cargo_metadata)
    }

    // Method to perform the build and package tasks on the given package
    fn build_and_package(
        &self,
        working_dir: &Path,
        wdk_metadata: &Result<Wdk, TryFromCargoMetadataError>,
        package: &Package,
        package_name: &str,
        target_dir: &Path,
    ) -> Result<(), BuildActionError> {
        info!("Building package {package_name}");
        BuildTask::new(
            package_name,
            working_dir,
            self.profile,
            self.target_arch,
            self.verbosity_level,
            self.command_exec,
        )
        .run()?;

        let wdk_metadata = if let Ok(wdk_metadata) = wdk_metadata {
            debug!("Found wdk metadata in package: {}", package_name);
            wdk_metadata
        } else {
            debug!("Invalid WDK metadata. Skipping package task");
            return Ok(());
        };

        // Identifying non driver packages
        if package.metadata.get("wdk").is_none() {
            debug!("Packaging task skipped for non-driver package");
            return Ok(());
        }

        if !package
            .targets
            .iter()
            .any(|t| t.kind.contains(&TargetKind::CDyLib))
        {
            warn!("No cdylib target found. Skipping package task");
            return Ok(());
        }

        debug!("Creating the driver package in the target directory");
        let driver_model = wdk_metadata.driver_model.clone();
        let target_arch = match self.target_arch {
            TargetArch::Default(arch) | TargetArch::Selected(arch) => arch,
        };
        debug!("Target architecture for package: {package_name} is: {target_arch}");
        let mut target_dir = target_dir.to_path_buf();
        if let TargetArch::Selected(arch) = self.target_arch {
            target_dir = target_dir.join(to_target_triple(arch));
        }
        target_dir = match self.profile {
            Some(Profile::Release) => target_dir.join("release"),
            _ => target_dir.join("debug"),
        };
        debug!(
            "Target directory for package: {} is: {}",
            package_name,
            target_dir.display()
        );

        PackageTask::new(
            PackageTaskParams {
                package_name,
                working_dir,
                target_dir: &target_dir,
                target_arch: &target_arch,
                verify_signature: self.verify_signature,
                sample_class: self.is_sample_class,
                driver_model,
            },
            self.wdk_build,
            self.command_exec,
            self.fs,
        )
        .run()?;

        info!("Finished building {package_name}");
        Ok(())
    }
}
