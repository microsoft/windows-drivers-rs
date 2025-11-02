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
use cargo_metadata::{Message, Metadata as CargoMetadata, Package};
use error::BuildActionError;
use mockall_double::double;
use package_task::{PackageTask, PackageTaskParams};
use tracing::{debug, error as err, info, warn};
use wdk_build::{
    CpuArchitecture,
    metadata::{TryFromCargoMetadataError, Wdk},
};

use crate::actions::Profile;
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
                    self.build_and_package(&package_root_path, wdk_metadata.as_ref().ok(), package)
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

            self.build_and_package(working_dir, wdk_metadata.as_ref().ok(), package)?;

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
        wdk_metadata: Option<&Wdk>,
        package: &Package,
    ) -> Result<(), BuildActionError> {
        let package_name = package.name.as_str();
        info!("Building package {package_name}");

        let output_message_iter = BuildTask::new(
            package_name,
            working_dir,
            self.profile,
            self.target_arch,
            self.verbosity_level,
            self.command_exec,
        )
        .run()?;

        // Skip packaging if package does not have WDK metadata
        let Some(wdk_metadata) = wdk_metadata else {
            warn!("WDK metadata is not found for `{package_name}`; skipping packaging");
            return Ok(());
        };

        // Skip packaging if the package does not produce a cdylib (.dll)
        let emits_cdylib = package
            .targets
            .iter()
            .any(|target| target.crate_types.iter().any(|c| c.to_string() == "cdylib"));
        if !emits_cdylib {
            debug!("Package {package_name} does not produce a cdylib; skipping packaging");
            return Ok(());
        }

        // Resolve the target architecture for the packaging task
        let target_arch = if let Some(arch) = self.target_arch {
            arch
        } else {
            self.probe_target_arch_from_cargo_rustc(working_dir)?
        };

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
                target_dir: &Self::get_target_dir_for_packaging(package, output_message_iter)?,
                target_arch,
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

    // Extracts the driver DLL path from the Cargo build output
    fn get_target_dir_for_packaging(
        package: &Package,
        message_iter: impl Iterator<Item = Result<Message, std::io::Error>>,
    ) -> Result<PathBuf, BuildActionError> {
        let normalized_pkg_name = package.name.replace('-', "_");
        let driver_file_name = format!("{normalized_pkg_name}.dll");

        message_iter
            .filter_map(|message| match message {
                Ok(Message::CompilerArtifact(artifact)) => Some(artifact),
                Ok(_) => None,
                Err(err) => {
                    debug!("Skipping unparsable cargo message: {err}");
                    None
                }
            })
            .find_map(|artifact| {
                let package_matches = artifact.target.name == normalized_pkg_name
                    && artifact.manifest_path == package.manifest_path;
                let is_cdylib = artifact
                    .target
                    .crate_types
                    .iter()
                    .any(|t| t.to_string() == "cdylib")
                    && artifact
                        .target
                        .kind
                        .iter()
                        .any(|k| k.to_string() == "cdylib");

                if !(package_matches && is_cdylib) {
                    debug!(
                        "Skipping crate (name={:?}, kinds={:?}, crate_types={:?}, filenames={:?})",
                        artifact.target.name,
                        &artifact.target.kind,
                        &artifact.target.crate_types,
                        &artifact.filenames
                    );
                    return None;
                }

                artifact.filenames.iter().find_map(|path| {
                    if path.file_name() != Some(driver_file_name.as_str()) {
                        return None;
                    }

                    debug!(
                        "Matched driver crate (name={:?}, kinds={:?}, crate_types={:?}, \
                         filenames={:?})",
                        artifact.target.name,
                        &artifact.target.kind,
                        &artifact.target.crate_types,
                        &artifact.filenames
                    );

                    let dll_path = path.as_std_path();
                    let Some(parent) = dll_path.parent() else {
                        return Some(Err(BuildActionError::DriverBinaryMissingParent(
                            dll_path.to_path_buf(),
                        )));
                    };

                    match absolute(parent) {
                        Ok(artifacts_dir) => {
                            debug!(
                                "Driver artifacts parent directory: {}",
                                artifacts_dir.display()
                            );
                            Some(Ok(artifacts_dir))
                        }
                        Err(error) => Some(Err(BuildActionError::NotAbsolute(
                            parent.to_path_buf(),
                            error,
                        ))),
                    }
                })
            })
            .unwrap_or_else(|| Err(BuildActionError::DriverDllNotFound))
    }

    /// Invokes `cargo rustc -- --print cfg` and finds the `target_arch` value
    ///
    /// # Arguments
    /// * `command_exec` - A reference to the `CommandExec` struct that provides
    ///   methods for executing commands.
    ///
    /// # Returns
    /// * `CpuArchitecture`
    /// * `anyhow::Error` if the command fails to execute or the output is not
    ///   in the expected format.
    fn probe_target_arch_from_cargo_rustc(
        &self,
        working_dir: &Path,
    ) -> Result<&CpuArchitecture, BuildActionError> {
        let args = ["rustc", "--", "--print", "cfg"];
        let output = self
            .command_exec
            .run("cargo", &args, None, Some(working_dir))?;
        let arch = output.stdout.split(|b| *b == b'\n').find_map(|line| {
            line.strip_prefix(b"target_arch=\"")
                .and_then(|rest| rest.split(|b| *b == b'"').next())
        });

        match arch {
            Some(arch) if arch == b"x86_64" => Ok(&CpuArchitecture::Amd64),
            Some(arch) if arch == b"aarch64" => Ok(&CpuArchitecture::Arm64),
            Some(arch) => Err(BuildActionError::UnsupportedArchitecture(
                String::from_utf8_lossy(arch).into(),
            )),
            None => Err(BuildActionError::CannotDetectTargetArch),
        }
    }
}
