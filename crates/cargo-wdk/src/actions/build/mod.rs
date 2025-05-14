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
    fs::read_dir,
    io,
    path::{Path, PathBuf},
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

use super::TargetArch;
use crate::actions::Profile;
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
    /// Creates a new instance of `BuildAction`
    /// # Arguments
    /// * `params` - The `BuildActionParams` struct containing the parameters
    ///   for the build action
    /// * `wdk_build` - The WDK build provider instance
    /// * `command_exec` - The command execution provider instance
    /// * `fs` - The file system provider instance
    /// * `metadata` - The metadata provider instance
    /// # Returns
    /// * `Result<Self>` - A result containing the new instance of `BuildAction`
    ///   or an error
    /// # Errors
    /// * `BuildActionError::IoError` - If there is an IO error while
    ///   canonicalizing the working dir
    pub fn new(
        params: &BuildActionParams<'a>,
        wdk_build: &'a WdkBuild,
        command_exec: &'a CommandExec,
        fs: &'a Fs,
        metadata: &'a Metadata,
    ) -> Result<Self> {
        // TODO: validate and init attrs here
        let working_dir = fs.canonicalize_path(params.working_dir)?;
        Ok(Self {
            working_dir,
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
        wdk_build::cargo_make::setup_path()?;
        debug!("PATH env variable is set with WDK bin and tools paths");
        let build_number = self.wdk_build.detect_wdk_build_number()?;
        debug!("WDK build number: {}", build_number);

        // Standalone driver/driver workspace support
        if self.fs.exists(&self.working_dir.join("Cargo.toml")) {
            return self.run_from_workspace_root(&self.working_dir);
        }

        // Emulated workspaces support
        let dirs = read_dir(&self.working_dir)?.collect::<Result<Vec<_>, io::Error>>()?;
        info!(
            "Checking for valid Rust projects in the working directory: {}",
            self.working_dir.display()
        );

        let mut is_valid_dir_with_rust_projects = false;
        for dir in &dirs {
            if dir.file_type()?.is_dir() && self.fs.exists(&dir.path().join("Cargo.toml")) {
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

        debug!("Iterating over each dir entry and process valid Rust(possibly driver) projects");
        let mut failed_atleast_one_project = false;
        for dir in dirs {
            debug!(
                "Verifying the dir entry if it is a valid Rust project: {}",
                dir.path().display()
            );
            if !dir.file_type()?.is_dir() || !self.fs.exists(&dir.path().join("Cargo.toml")) {
                debug!("Skipping the dir entry as it is not a valid Rust project");
                continue;
            }

            info!(
                "Processing Rust(possibly driver) project: {}",
                dir.path()
                    .file_name()
                    .expect("package sub directory name ended with \"..\" which is not expected")
                    .to_string_lossy()
            );
            if let Err(e) = self.run_from_workspace_root(&dir.path()) {
                failed_atleast_one_project = true;
                err!(
                    "Error building the child project: {}, error: {}",
                    dir.path()
                        .file_name()
                        .expect(
                            "package sub directory name ended with \"..\" which is not expected"
                        )
                        .to_string_lossy(),
                    e
                );
            }
        }

        debug!("Done checking for valid Rust(possibly driver) projects in the working directory");
        if failed_atleast_one_project {
            return Err(BuildActionError::OneOrMoreRustProjectsFailedToBuild(
                self.working_dir.clone(),
            ));
        }

        info!("Build completed successfully");
        Ok(())
    }

    // Method to initiate the packaging process for the given working directory
    // and the cargo metadata
    fn run_from_workspace_root(&self, working_dir: &Path) -> Result<(), BuildActionError> {
        let cargo_metadata = &self.get_cargo_metadata(working_dir)?;
        let target_directory = cargo_metadata.target_directory.as_std_path().to_path_buf();
        let wdk_metadata = Wdk::try_from(cargo_metadata);
        let workspace_packages = cargo_metadata.workspace_packages();
        let workspace_root = self
            .fs
            .canonicalize_path(cargo_metadata.workspace_root.clone().as_std_path())?;
        if workspace_root.eq(working_dir) {
            debug!("Running from workspace root");
            let mut failed_atleast_one_workspace_member = false;
            for package in workspace_packages {
                let package_root_path: PathBuf = package
                    .manifest_path
                    .parent()
                    .expect("Unable to find package path from Cargo manifest path")
                    .into();

                let package_root_path = self.fs.canonicalize_path(package_root_path.as_path())?;
                debug!(
                    "Processing workspace member package: {}",
                    package_root_path.display()
                );
                if let Err(e) = self.build_and_package(
                    &package_root_path,
                    &wdk_metadata,
                    package,
                    package.name.clone(),
                    &target_directory,
                ) {
                    failed_atleast_one_workspace_member = true;
                    err!(
                        "Error packaging the workspace member project: {}, error: {}",
                        package_root_path.display(),
                        e
                    );
                }
            }
            if let Err(e) = wdk_metadata {
                return Err(BuildActionError::WdkMetadataParse(e));
            }

            if failed_atleast_one_workspace_member {
                return Err(BuildActionError::OneOrMoreWorkspaceMembersFailedToBuild(
                    working_dir.to_owned(),
                ));
            }
            return Ok(());
        }
        info!("Running from standalone/workspace member directory");
        let package = workspace_packages.iter().find(|p| {
            let package_root_path: PathBuf = p
                .manifest_path
                .parent()
                .expect("Unable to find package path from Cargo manifest path")
                .into();
            self.fs
                .canonicalize_path(package_root_path.as_path())
                .is_ok_and(|package_root_path| {
                    debug!(
                        "Processing standalone/workspace member package: {}",
                        package_root_path.display()
                    );
                    package_root_path.eq(working_dir)
                })
        });

        let package =
            package.ok_or_else(|| BuildActionError::NotAWorkspaceMember(working_dir.to_owned()))?;

        self.build_and_package(
            working_dir,
            &wdk_metadata,
            package,
            package.name.clone(),
            &target_directory,
        )?;

        if let Err(e) = wdk_metadata {
            return Err(BuildActionError::WdkMetadataParse(e));
        }

        info!(
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
        package_name: String,
        target_dir: &Path,
    ) -> Result<(), BuildActionError> {
        info!("Processing package: {}", package_name);
        BuildTask::new(
            &package_name,
            working_dir,
            self.profile,
            self.target_arch,
            self.verbosity_level,
            self.command_exec,
            self.fs,
        )?
        .run()?;
        if package.metadata.get("wdk").is_none() {
            warn!(
                "No package.metadata.wdk section found. Skipping driver build workflow for \
                 package: {}",
                package_name
            );
            return Ok(());
        }
        if !package
            .targets
            .iter()
            .any(|t| t.kind.contains(&TargetKind::CDyLib))
        {
            warn!(
                "No cdylib target found. Skipping driver build workflow for package: {}",
                package_name
            );
            return Ok(());
        }

        let wdk_metadata = if let Ok(wdk_metadata) = wdk_metadata {
            debug!("Found wdk metadata in package: {}", package_name);
            wdk_metadata
        } else {
            warn!(
                "WDK metadata is not available. Skipping driver build workflow for package: {}",
                package_name
            );
            return Ok(());
        };

        debug!("Creating the driver package in the target directory");
        let driver_model = wdk_metadata.driver_model.clone();
        let target_arch = match self.target_arch {
            TargetArch::Default(arch) | TargetArch::Selected(arch) => arch,
        };
        debug!(
            "Target architecture for package: {} is: {}",
            package_name, target_arch
        );
        let mut target_dir = target_dir.to_path_buf();
        if let TargetArch::Selected(arch) = self.target_arch {
            target_dir = target_dir.join(arch.to_target_triple());
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

        let package_task = PackageTask::new(
            PackageTaskParams {
                package_name: &package_name,
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
        );

        match package_task {
            Ok(package_task) => {
                if let Err(e) = package_task.run() {
                    return Err(BuildActionError::PackageTask(package_name, e));
                }
            }
            Err(e) => {
                return Err(BuildActionError::PackageTaskInit(package_name, e));
            }
        }

        info!("Processing completed for package: {}", package_name);
        Ok(())
    }
}
