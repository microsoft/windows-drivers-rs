#[cfg(test)]
mod tests;

// Module imports
mod error;
use cargo_metadata::Metadata;
use error::PackageProjectError;
mod package_driver;

// Non local imports
use std::{fs::read_dir, io, path::PathBuf, result::Result::Ok};

use anyhow::Result;
use log::{debug, error as log_error, info};
use package_driver::PackageDriver;
use wdk_build::metadata::Wdk;

use super::{build::BuildAction, Profile, TargetArch};
use crate::providers::{exec::RunCommand, fs::FSProvider, wdk_build::WdkBuildProvider};

// #[derive(Debug)]
pub struct PackageAction<'a> {
    working_dir: PathBuf,
    profile: Profile,
    target_arch: TargetArch,
    is_sample_class: bool,
    verbosity_level: clap_verbosity_flag::Verbosity,

    // Injected deps
    wdk_build_provider: &'a dyn WdkBuildProvider,
    command_exec: &'a dyn RunCommand,
    fs_provider: &'a dyn FSProvider,
}

impl<'a> PackageAction<'a> {
    pub fn new(
        working_dir: PathBuf,
        profile: Profile,
        target_arch: TargetArch,
        is_sample_class: bool,
        verbosity_level: clap_verbosity_flag::Verbosity,
        wdk_build_provider: &'a dyn WdkBuildProvider,
        command_exec: &'a dyn RunCommand,
        fs_provider: &'a dyn FSProvider,
    ) -> Result<Self> {
        // TODO: validate and init attrs here
        wdk_build::cargo_make::setup_path()?;
        debug!("PATH env variable is set with WDK bin and tools paths");

        debug!(
            "Initializing packaging for project at: {}",
            working_dir.display()
        );
        // FIXME: Canonicalizing here leads to a cargo_metadata error. Probably because
        // it is already canonicalized, * (wild chars) won't be resolved to actual paths
        let working_dir = fs_provider.canonicalize_path(working_dir)?;
        Ok(Self {
            working_dir,
            profile,
            target_arch,
            is_sample_class,
            verbosity_level,
            command_exec,
            wdk_build_provider,
            fs_provider,
        })
    }

    pub fn run(&self) -> Result<(), PackageProjectError> {
        // Standalone driver/driver workspace support
        if self
            .fs_provider
            .exists(&self.working_dir.join("Cargo.toml"))
        {
            let cargo_metadata = self.get_cargo_metadata(self.working_dir.clone())?;
            return self.run_from_workspace_root(self.working_dir.clone(), cargo_metadata);
        }

        // Emulated workspaces support
        let dirs = read_dir(&self.working_dir)?
            .map(|entry| entry)
            .collect::<Result<Vec<_>, io::Error>>()?;
        info!(
            "Checking for valid Rust projects in the working directory: {}",
            self.working_dir.display()
        );
        debug!(
            "Found {} entries in the working directory: {:?}",
            dirs.len(),
            dirs
        );

        let mut is_valid_dir_with_rust_projects = false;
        for dir in dirs.iter() {
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
        let mut did_fail_atleast_one_project = false;
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
                match self.get_cargo_metadata(dir.path()) {
                    Ok(cargo_metadata) => {
                        if let Err(e) = self.run_from_workspace_root(dir.path(), cargo_metadata) {
                            did_fail_atleast_one_project = true;
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
                        did_fail_atleast_one_project = true;
                        log_error!("Error reading cargo metadata: {}", e);
                    }
                }
            } else {
                debug!("Skipping the dir entry as it is not a valid Rust project");
            }
        }

        debug!("Done checking for valid Rust(possibly driver) projects in the working director");
        if did_fail_atleast_one_project {
            return Err(PackageProjectError::OneOrMoreRustProjectsFailedToBuild(
                self.working_dir.clone(),
            ));
        }
        Ok(())
    }

    fn run_from_workspace_root(
        &self,
        working_dir: PathBuf,
        cargo_metadata: Metadata,
    ) -> Result<(), PackageProjectError> {
        let target_directory = cargo_metadata
            .target_directory
            .join(&self.profile.to_string());
        let wdk_metadata = Wdk::try_from(&cargo_metadata)?;
        let workspace_packages = cargo_metadata.workspace_packages();
        let workspace_root = self
            .fs_provider
            .canonicalize_path(cargo_metadata.workspace_root.clone().into())?;
        if workspace_root.eq(&working_dir) {
            debug!("Running from workspace root");
            let target_directory: PathBuf = target_directory.into();
            for package in workspace_packages {
                let package_root_path: PathBuf = package
                    .manifest_path
                    .parent()
                    .expect("Unable to find package path from Cargo manifest path")
                    .into();

                let package_root_path = self.fs_provider.canonicalize_path(package_root_path)?;
                debug!(
                    "Processing workspace driver package: {}",
                    package_root_path.display()
                );
                self.build_and_package(
                    &package_root_path,
                    &wdk_metadata,
                    &package.metadata,
                    package.name.clone(),
                    &target_directory,
                )?;
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
            if let Ok(package_root_path) = self.fs_provider.canonicalize_path(package_root_path) {
                debug!(
                    "Processing workspace driver package: {}",
                    package_root_path.display()
                );
                package_root_path.eq(&working_dir)
            } else {
                false
            }
        });

        if package.is_none() {
            return Err(PackageProjectError::NotAWorkspaceMemberError(
                working_dir.clone(),
            ));
        }

        let package = package.unwrap();
        self.build_and_package(
            &working_dir,
            &wdk_metadata,
            &package.metadata,
            package.name.clone(),
            &target_directory.into(),
        )?;

        info!("Building and packaging completed successfully");

        Ok(())
    }

    fn get_cargo_metadata(
        &self,
        working_dir: PathBuf,
    ) -> Result<cargo_metadata::Metadata, PackageProjectError> {
        let working_dir_path_trimmed: PathBuf = working_dir
            .to_string_lossy()
            .trim_start_matches("\\\\?\\")
            .into();
        let cargo_metadata = self
            .wdk_build_provider
            .get_cargo_metadata_at_path(&working_dir_path_trimmed)?;
        Ok(cargo_metadata)
    }

    fn build_and_package(
        &self,
        working_dir: &PathBuf,
        wdk_metadata: &Wdk,
        metadata: &serde_json::Value,
        package_name: String,
        target_dir: &PathBuf,
    ) -> Result<(), PackageProjectError> {
        info!("Processing package: {}", package_name);
        BuildAction::new(
            &package_name,
            &working_dir,
            self.verbosity_level,
            self.command_exec,
        )
        .run()?;
        if metadata.get("wdk").is_none() {
            debug!(
                "No wdk metadata found. Skipping driver package workflow for package: {}",
                package_name
            );
            return Ok(());
        }

        debug!("Found wdk metadata in package: {}", package_name);
        let package_driver = PackageDriver::new(
            &package_name,
            &working_dir,
            target_dir,
            &self.target_arch,
            self.is_sample_class,
            wdk_metadata.driver_model.clone(),
            self.wdk_build_provider,
            self.command_exec,
            self.fs_provider,
        );
        if let Err(e) = package_driver {
            return Err(PackageProjectError::PackageDriverInitError(package_name, e));
        }

        if let Err(e) = package_driver.unwrap().run() {
            return Err(PackageProjectError::PackageDriverError(package_name, e));
        }
        info!("Processing completed for package: {}", package_name);
        Ok(())
    }
}
