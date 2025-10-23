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
use cargo_metadata::Metadata as CargoMetadata;
use error::BuildActionError;
use mockall_double::double;
use package_task::{PackageTask, PackageTaskParams};
use tracing::{debug, error as err, info, warn};
use wdk_build::{
    CpuArchitecture,
    metadata::{TryFromCargoMetadataError, Wdk},
};

use crate::actions::{Profile, build::error::BuildTaskError};
#[double]
use crate::providers::{exec::CommandExec, fs::Fs, metadata::Metadata, wdk_build::WdkBuild};

pub struct BuildActionParams<'a> {
    pub working_dir: &'a Path,
    pub profile: Option<&'a Profile>,
    pub target_arch: Option<&'a CpuArchitecture>,
    pub verify_signature: bool,
    pub is_sample_class: bool,
    pub verbosity_level: clap_verbosity_flag::Verbosity,
}

/// Action that orchestrates the build and package of a driver project. Build is
/// a pre-requisite for packaging.
pub struct BuildAction<'a> {
    working_dir: PathBuf,
    profile: Option<&'a Profile>,
    target_arch: Option<&'a CpuArchitecture>,
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
    ///
    /// # Returns
    /// * `Result<(), BuildActionError>` - A result containing an empty tuple or
    ///   an error of type `BuildActionError`.
    ///
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
        debug!(
            "WDK build number: {}",
            self.wdk_build.detect_wdk_build_number()?
        );

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
                if let Err(e) =
                    self.build_and_package(&package_root_path, &wdk_metadata, &package.name)
                {
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

            self.build_and_package(working_dir, &wdk_metadata, &package.name)?;

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
        package_name: &str,
    ) -> Result<(), BuildActionError> {
        info!("Building package {package_name}");
        let (dll_path, wdk_metadata) = match BuildTask::new(
            package_name,
            working_dir,
            self.profile,
            self.target_arch,
            self.verbosity_level,
            self.command_exec,
        )
        .run()
        {
            Ok(dll_path) => {
                debug!("Found driver binary(.dll) at {}", dll_path.display());
                if let Ok(meta) = wdk_metadata {
                    info!("Found wdk metadata in `{package_name}` package");
                    (dll_path, meta)
                } else {
                    warn!("WDK metadata is not found for `{package_name}`; skipping packaging",);
                    return Ok(());
                }
            }
            Err(BuildTaskError::DllNotFound) => {
                if wdk_metadata.is_ok() {
                    warn!(
                        "WDK metadata is present in workspace manifest. But `{package_name}` may \
                         not be a driver, no cdylib (.dll) artifact found; skipping packaging"
                    );
                } else {
                    info!("WDK metadata not found for `{package_name}`; skipping packaging");
                }
                return Ok(());
            }
            Err(e) => return Err(BuildActionError::BuildTask(e)),
        };

        let (target_arch, artifacts_dir) =
            self.determine_target_arch_and_artifacts_dir(working_dir, &dll_path)?;

        // Set up the `PATH` system environment variable with WDK/SDK bin and tools
        // paths.
        wdk_build::cargo_make::setup_path().map_err(|e| {
            debug!("Failed to set up PATH for WDK/SDK tools");
            BuildActionError::WdkBuildConfig(e)
        })?;
        debug!("PATH env variable is set with WDK bin and tools paths");

        PackageTask::new(
            &PackageTaskParams {
                package_name,
                working_dir,
                artifacts_dir: &artifacts_dir,
                target_arch: &target_arch,
                verify_signature: self.verify_signature,
                sample_class: self.is_sample_class,
                driver_model: &wdk_metadata.driver_model,
            },
            self.wdk_build,
            self.command_exec,
            self.fs,
        )
        .run()?;

        info!("Finished building {package_name}");
        Ok(())
    }

    /// Determine the effective target architecture for packaging.
    fn determine_target_arch_and_artifacts_dir(
        &self,
        working_dir: &Path,
        dll_path: &Path,
    ) -> Result<(CpuArchitecture, PathBuf), BuildActionError> {
        let artifacts_dir =
            dll_path
                .parent()
                .ok_or(BuildActionError::DriverBinaryMissingParent(
                    dll_path.to_path_buf(),
                ))?;
        let artifacts_dir = absolute(artifacts_dir)
            .map_err(|e| BuildActionError::NotAbsolute(artifacts_dir.to_path_buf(), e))?;
        debug!(
            "Driver artifacts parent directory: {}",
            artifacts_dir.display()
        );
        if let Some(explicit) = self.target_arch {
            return Ok((*explicit, artifacts_dir));
        }
        let expected_profile_dir = if matches!(self.profile, Some(Profile::Release)) {
            "release"
        } else {
            "debug"
        };
        let components: Vec<String> = artifacts_dir
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect();
        if let Some(tgt_idx) = components.iter().rposition(|c| c == "target")
            && components.len() > tgt_idx + 2
            && components[tgt_idx + 2] == expected_profile_dir
        {
            let triple = &components[tgt_idx + 1];
            if let Ok(a) = Self::arch_from_triple(triple) {
                debug!("Inferred architecture {:?} from artifact layout", a);
                return Ok((a, artifacts_dir));
            }
        }
        Ok((
            self.detect_target_arch_using_cargo_rustc(working_dir)?,
            artifacts_dir,
        ))
    }

    // Maps a target triple string to a CPU architecture.
    fn arch_from_triple(triple: &str) -> Result<CpuArchitecture, BuildActionError> {
        let arch = triple.split('-').next().unwrap_or(triple);
        match arch {
            "x86_64" => Ok(CpuArchitecture::Amd64),
            "aarch64" => Ok(CpuArchitecture::Arm64),
            _ => Err(BuildActionError::UnsupportedArchitecture(
                triple.to_string(),
            )),
        }
    }

    /// Detects the effective target architecture Cargo will build for this
    /// package by invoking `cargo rustc -- --print cfg` inside the package
    /// directory and parsing the emitted `target_arch="..."` cfg value.
    ///
    /// # Arguments
    /// * `command_exec` - A reference to the `CommandExec` struct that provides
    ///   methods for executing commands.
    ///
    /// # Returns
    /// * `CpuArchitecture`
    /// * `anyhow::Error` if the command fails to execute or the output is not
    ///   in the expected format.
    fn detect_target_arch_using_cargo_rustc(
        &self,
        working_dir: &Path,
    ) -> Result<CpuArchitecture, BuildActionError> {
        let args = ["rustc", "--", "--print", "cfg"];
        let output = self
            .command_exec
            .run("cargo", &args, None, Some(working_dir))?;
        for line in output.stdout.split(|b| *b == b'\n') {
            if let Some(rest) = line.strip_prefix(b"target_arch=\"")
                && let Some(end_quote) = rest.iter().position(|b| *b == b'"')
            {
                let arch = &rest[..end_quote];
                return match arch {
                    b"x86_64" => Ok(CpuArchitecture::Amd64),
                    b"aarch64" => Ok(CpuArchitecture::Arm64),
                    _ => {
                        return Err(BuildActionError::UnsupportedArchitecture(
                            String::from_utf8_lossy(arch).into(),
                        ));
                    }
                };
            }
        }

        Err(BuildActionError::CannotDetectTargetArch)
    }
}
