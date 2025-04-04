// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! This module contains the `PackageAction` struct and its associated methods
//! for orchestrating the packaging of a driver project. It includes the build
//! step as a prerequisite for packaging. It consists the logic to build and
//! package standalone projects, workspaces, individual members in a workspace
//! and emulated workspaces.
#[cfg(test)]
mod tests;

mod error;
use cargo_metadata::{Metadata as CargoMetadata, Package, TargetKind};
use error::PackageProjectError;
use mockall_double::double;
mod package_task;

use std::{
    fs::read_dir,
    io,
    path::{Path, PathBuf},
    result::Result::Ok,
};

use anyhow::Result;
use package_task::{PackageTask, PackageTaskParams};
use tracing::{debug, error as log_error, info, warn};
use wdk_build::metadata::{TryFromCargoMetadataError, Wdk};

use super::{build::BuildAction, CpuArchitecture, Profile};
use crate::actions::{AARCH64_TARGET_TRIPLE_NAME, X86_64_TARGET_TRIPLE_NAME};
#[double]
use crate::providers::{exec::CommandExec, fs::Fs, metadata::Metadata, wdk_build::WdkBuild};

pub struct PackageActionParams<'a> {
    pub working_dir: &'a Path,
    pub profile: Profile,
    pub host_arch: CpuArchitecture,
    pub target_arch: Option<CpuArchitecture>,
    pub verify_signature: bool,
    pub is_sample_class: bool,
    pub verbosity_level: clap_verbosity_flag::Verbosity,
}

/// Action that orchestrates the packaging of a driver project
/// This also includes the build step as pre-requisite for packaging
pub struct PackageAction<'a> {
    working_dir: PathBuf,
    profile: Profile,
    host_arch: CpuArchitecture,
    target_arch: Option<CpuArchitecture>,
    verify_signature: bool,
    is_sample_class: bool,
    verbosity_level: clap_verbosity_flag::Verbosity,

    // Injected deps
    wdk_build_provider: &'a WdkBuild,
    command_exec: &'a CommandExec,
    fs_provider: &'a Fs,
    metadata: &'a Metadata,
}

impl<'a> PackageAction<'a> {
    /// Creates a new instance of `PackageAction`
    /// # Arguments
    /// * `working_dir` - The working directory to operate on
    /// * `profile` - The profile to be used for cargo build and package target
    ///   dir
    /// * `target_arch` - The target architecture
    /// * `is_sample_class` - Indicates if the driver is a sample class driver
    /// * `verbosity_level` - The verbosity level for logging
    /// * `wdk_build_provider` - The WDK build provider instance
    /// * `command_exec` - The command execution provider instance
    /// * `fs_provider` - The file system provider instance
    /// # Returns
    /// * `Result<Self>` - A result containing the new instance of
    ///   `PackageAction` or an error
    /// # Errors
    /// * `PackageProjectError::IoError` - If there is an IO error while
    ///   canonicalizing the working dir
    pub fn new(
        params: &PackageActionParams<'a>,
        wdk_build_provider: &'a WdkBuild,
        command_exec: &'a CommandExec,
        fs_provider: &'a Fs,
        metadata: &'a Metadata,
    ) -> Result<Self> {
        // TODO: validate and init attrs here
        let working_dir = fs_provider.canonicalize_path(params.working_dir)?;
        Ok(Self {
            working_dir,
            profile: params.profile,
            host_arch: params.host_arch,
            target_arch: params.target_arch,
            verify_signature: params.verify_signature,
            is_sample_class: params.is_sample_class,
            verbosity_level: params.verbosity_level,
            wdk_build_provider,
            command_exec,
            fs_provider,
            metadata,
        })
    }

    /// Entry point method to execute the packaging action flow
    /// # Returns
    /// * `Result<Self>` - A result containing an empty tuple or an error of
    ///   type `PackageProjectError`
    /// # Errors
    /// * `PackageProjectError::NotAWorkspaceMemberError` - If the working
    ///   directory is not a workspace member
    /// * `PackageProjectError::PackageDriverInitError` - If there is an error
    ///   initializing the package driver
    /// * `PackageProjectError::PackageDriverError` - If there is an error
    ///   during the package driver process
    /// * `PackageProjectError::CargoMetadataParseError` - If there is an error
    ///   parsing the Cargo metadata
    /// * `PackageProjectError::WdkMetadataParseError` - If there is an error
    ///   parsing the WDK metadata
    /// * `PackageProjectError::WdkBuildConfigError` - If there is an error with
    ///   the WDK build config
    /// * `PackageProjectError::IoError` - Wraps all possible IO errors
    /// * `PackageProjectError::CommandExecutionError` - If there is an error
    ///   executing a command
    /// * `PackageProjectError::NoValidRustProjectsInTheDirectory` - If no valid
    ///   Rust projects are found in the directory
    /// * `PackageProjectError::OneOrMoreRustProjectsFailedToBuild` - If one or
    ///   more Rust projects fail to build
    pub fn run(&self) -> Result<(), PackageProjectError> {
        wdk_build::cargo_make::setup_path()?;
        debug!("PATH env variable is set with WDK bin and tools paths");
        debug!(
            "Initializing packaging for project at: {}",
            self.working_dir.display()
        );
        let build_number = self.wdk_build_provider.detect_wdk_build_number()?;
        debug!("WDK build number: {}", build_number);
        // Standalone driver/driver workspace support
        if self
            .fs_provider
            .exists(&self.working_dir.join("Cargo.toml"))
        {
            let cargo_metadata = self.get_cargo_metadata(&self.working_dir)?;
            return self.run_from_workspace_root(&self.working_dir, &cargo_metadata);
        }

        // Emulated workspaces support
        let dirs = read_dir(&self.working_dir)?.collect::<Result<Vec<_>, io::Error>>()?;
        info!(
            "Checking for valid Rust projects in the working directory: {}",
            self.working_dir.display()
        );

        let mut is_valid_dir_with_rust_projects = false;
        for dir in &dirs {
            if dir.file_type()?.is_dir() && self.fs_provider.exists(&dir.path().join("Cargo.toml"))
            {
                debug!(
                    "Found atleast one valid Rust project directory: {}, continuing with the \
                     package flow",
                    dir.path()
                        .file_name()
                        .expect("error reading the folder name")
                        .to_string_lossy()
                );
                is_valid_dir_with_rust_projects = true;
                break;
            }
        }

        if !is_valid_dir_with_rust_projects {
            return Err(PackageProjectError::NoValidRustProjectsInTheDirectory(
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
            if dir.file_type()?.is_dir() && self.fs_provider.exists(&dir.path().join("Cargo.toml"))
            {
                info!(
                    "Processing Rust(possibly driver) project: {}",
                    dir.path()
                        .file_name()
                        .expect("error reading the folder name")
                        .to_string_lossy()
                );
                match self.get_cargo_metadata(&dir.path()) {
                    Ok(cargo_metadata) => {
                        if let Err(e) = self.run_from_workspace_root(&dir.path(), &cargo_metadata) {
                            failed_atleast_one_project = true;
                            log_error!(
                                "Error packaging the child project: {}, error: {}",
                                dir.path()
                                    .file_name()
                                    .expect("error reading the folder name")
                                    .to_string_lossy(),
                                e
                            );
                        }
                    }
                    Err(e) => {
                        failed_atleast_one_project = true;
                        log_error!("Error reading cargo metadata: {}", e);
                    }
                }
            } else {
                debug!("Skipping the dir entry as it is not a valid Rust project");
            }
        }

        debug!("Done checking for valid Rust(possibly driver) projects in the working director");
        if failed_atleast_one_project {
            return Err(PackageProjectError::OneOrMoreRustProjectsFailedToBuild(
                self.working_dir.clone(),
            ));
        }

        info!("Building and packaging completed successfully");
        Ok(())
    }

    fn run_from_workspace_root(
        &self,
        working_dir: &PathBuf,
        cargo_metadata: &CargoMetadata,
    ) -> Result<(), PackageProjectError> {
        let target_directory = cargo_metadata.target_directory.as_std_path().to_path_buf();
        let wdk_metadata = Wdk::try_from(cargo_metadata);
        let workspace_packages = cargo_metadata.workspace_packages();
        let workspace_root = self
            .fs_provider
            .canonicalize_path(cargo_metadata.workspace_root.clone().as_std_path())?;
        if workspace_root.eq(working_dir) {
            debug!("Running from workspace root");
            for package in workspace_packages {
                let package_root_path: PathBuf = package
                    .manifest_path
                    .parent()
                    .expect("Unable to find package path from Cargo manifest path")
                    .into();

                let package_root_path = self
                    .fs_provider
                    .canonicalize_path(package_root_path.as_path())?;
                debug!(
                    "Processing workspace driver package: {}",
                    package_root_path.display()
                );
                self.build_and_package(
                    &package_root_path,
                    &wdk_metadata,
                    package,
                    package.name.clone(),
                    &target_directory,
                )?;
            }
            if let Err(e) = wdk_metadata {
                return Err(PackageProjectError::WdkMetadataParse(e));
            }
            return Ok(());
        }
        info!("Running from workspace member directory");
        let package = workspace_packages.iter().find(|p| {
            let package_root_path: PathBuf = p
                .manifest_path
                .parent()
                .expect("Unable to find package path from Cargo manifest path")
                .into();
            self.fs_provider
                .canonicalize_path(package_root_path.as_path())
                .is_ok_and(|package_root_path| {
                    debug!(
                        "Processing workspace driver package: {}",
                        package_root_path.display()
                    );
                    package_root_path.eq(working_dir)
                })
        });

        if package.is_none() {
            return Err(PackageProjectError::NotAWorkspaceMember(
                working_dir.clone(),
            ));
        }

        let package = package.expect("Package cannot be empty");
        self.build_and_package(
            working_dir,
            &wdk_metadata,
            package,
            package.name.clone(),
            &target_directory,
        )?;

        if let Err(e) = wdk_metadata {
            return Err(PackageProjectError::WdkMetadataParse(e));
        }

        info!("Building and packaging completed successfully");

        Ok(())
    }

    fn get_cargo_metadata(&self, working_dir: &Path) -> Result<CargoMetadata, PackageProjectError> {
        let working_dir_path_trimmed: PathBuf = working_dir
            .to_string_lossy()
            .trim_start_matches("\\\\?\\")
            .into();
        let cargo_metadata = self
            .metadata
            .get_cargo_metadata_at_path(&working_dir_path_trimmed)?;
        Ok(cargo_metadata)
    }

    fn build_and_package(
        &self,
        working_dir: &Path,
        wdk_metadata: &Result<Wdk, TryFromCargoMetadataError>,
        package: &Package,
        package_name: String,
        target_dir: &Path,
    ) -> Result<(), PackageProjectError> {
        info!("Processing package: {}", package_name);
        BuildAction::new(
            &package_name,
            working_dir,
            &self.profile,
            self.target_arch,
            self.verbosity_level,
            self.command_exec,
            self.fs_provider,
        )?
        .run()?;

        if package.metadata.get("wdk").is_none() {
            warn!(
                "No package.metadata.wdk section found. Skipping driver package workflow for \
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
                "No cdylib target found. Skipping driver package workflow for package: {}",
                package_name
            );
            return Ok(());
        }

        debug!("Found wdk metadata in package: {}", package_name);

        let mut target_dir = match self.target_arch {
            Some(CpuArchitecture::Amd64) => target_dir.join(X86_64_TARGET_TRIPLE_NAME),
            Some(CpuArchitecture::Arm64) => target_dir.join(AARCH64_TARGET_TRIPLE_NAME),
            None => target_dir.to_path_buf(),
        };
        target_dir = target_dir.join(self.profile.target_folder_name());
        debug!(
            "Target directory for package: {} is: {}",
            package_name,
            target_dir.display()
        );
        let target_arch = self.target_arch.unwrap_or(self.host_arch);
        debug!(
            "Target architecture for package: {} is: {}",
            package_name, target_arch
        );
        debug!("Creating package driver for package: {}", package_name);
        if wdk_metadata.is_err() {
            return Ok(());
        }

        let wdk_metadata = wdk_metadata.as_ref().expect("WDK metadata cannot be empty");

        debug!("Found wdk metadata in package: {}", package_name);

        let mut target_dir = match self.target_arch {
            Some(CpuArchitecture::Amd64) => target_dir.join(X86_64_TARGET_TRIPLE_NAME),
            Some(CpuArchitecture::Arm64) => target_dir.join(AARCH64_TARGET_TRIPLE_NAME),
            None => target_dir.to_path_buf(),
        };
        target_dir = target_dir.join(self.profile.target_folder_name());
        debug!(
            "Target directory for package: {} is: {}",
            package_name,
            target_dir.display()
        );

        let target_arch = if self.target_arch.is_some() {
            self.target_arch.expect("Target architecture should be set")
        } else {
            self.host_arch
        };
        debug!(
            "Target architecture for package: {} is: {}",
            package_name, target_arch
        );
        debug!("Creating package driver for package: {}", package_name);
        let package_driver = PackageTask::new(
            PackageTaskParams {
                package_name: &package_name,
                working_dir,
                target_dir: &target_dir,
                target_arch,
                verify_signature: self.verify_signature,
                sample_class: self.is_sample_class,
                driver_model: wdk_metadata.driver_model.clone(),
            },
            self.wdk_build_provider,
            self.command_exec,
            self.fs_provider,
        );
        if let Err(e) = package_driver {
            return Err(PackageProjectError::PackageTaskInit(package_name, e));
        }

        if let Err(e) = package_driver
            .expect("PackageDriver failed to initialize")
            .run()
        {
            return Err(PackageProjectError::PackageTask(package_name, e));
        }
        info!("Processing completed for package: {}", package_name);
        Ok(())
    }
}
