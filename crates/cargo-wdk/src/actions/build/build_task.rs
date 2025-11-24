// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! Module that handles low level build operations for driver packages
//! This module defines the `BuildTask` struct and its associated methods for
//! building a driver package with the provided options using the `cargo build`
//! command.

use std::path::{Path, PathBuf};

use anyhow::Result;
use mockall_double::double;
use tracing::debug;

#[double]
use crate::providers::exec::CommandExec;
use crate::{
    actions::{Profile, TargetArch, build::error::BuildTaskError, to_target_triple},
    trace,
};

/// Builds specified package by running `cargo build`  
pub struct BuildTask<'a> {
    package_name: &'a str,
    profile: Option<&'a Profile>,
    target_arch: TargetArch,
    verbosity_level: clap_verbosity_flag::Verbosity,
    manifest_path: PathBuf,
    command_exec: &'a CommandExec,
    working_dir: &'a Path,
}

impl<'a> BuildTask<'a> {
    /// Creates a new instance of `BuildTask`.
    ///
    /// # Arguments
    /// * `package_name` - The name of the package to build
    /// * `working_dir` - The working directory for the build
    /// * `profile` - An optional profile for the build
    /// * `target_arch` - The target architecture for the build
    /// * `verbosity_level` - The verbosity level for logging
    /// * `command_exec` - The command execution provider
    ///
    /// # Returns
    /// * `Self` - A new instance of `BuildTask`.
    ///
    /// # Panics
    /// * If `working_dir` is not absolute
    pub fn new(
        package_name: &'a str,
        working_dir: &'a Path,
        profile: Option<&'a Profile>,
        target_arch: TargetArch,
        verbosity_level: clap_verbosity_flag::Verbosity,
        command_exec: &'a CommandExec,
    ) -> Self {
        assert!(
            working_dir.is_absolute(),
            "Working directory path must be absolute. Input path: {}",
            working_dir.display()
        );
        Self {
            package_name,
            profile,
            target_arch,
            verbosity_level,
            manifest_path: working_dir.join("Cargo.toml"),
            command_exec,
            working_dir,
        }
    }

    /// Entry point method to run the build task
    /// # Returns
    /// * `Result<(), BuildTaskError>` - Result indicating success or failure of
    ///   the build task
    /// # Errors
    /// * `BuildTaskError::CargoBuild` - If there is an error running the cargo
    ///   build command
    pub fn run(&self) -> Result<(), BuildTaskError> {
        debug!("Running cargo build");
        let mut args = vec!["build".to_string()];
        args.push("-p".to_string());
        args.push(self.package_name.to_string());
        if let Some(path) = self.manifest_path.to_str() {
            args.push("--manifest-path".to_string());
            args.push(path.to_string());
        } else {
            return Err(BuildTaskError::EmptyManifestPath);
        }
        if let Some(profile) = self.profile {
            args.push("--profile".to_string());
            args.push(profile.to_string());
        }
        if let TargetArch::Selected(target_arch) = self.target_arch {
            args.push("--target".to_string());
            args.push(to_target_triple(target_arch));
        }
        if let Some(flag) = trace::get_cargo_verbose_flags(self.verbosity_level) {
            args.push(flag.to_string());
        }
        let args = args
            .iter()
            .map(std::string::String::as_str)
            .collect::<Vec<&str>>();

        // Run cargo build from the provided working directory so that config.toml
        // is respected
        self.command_exec
            .run("cargo", &args, None, Some(self.working_dir))?;
        debug!("cargo build done");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use wdk_build::CpuArchitecture;

    use super::*;
    use crate::actions::{Profile, TargetArch};

    #[test]
    fn new_succeeds_for_valid_args() {
        let working_dir = PathBuf::from("C:/absolute/path/to/working/dir");
        let package_name = "test_package";
        let profile = Profile::Dev;
        let target_arch = TargetArch::Selected(CpuArchitecture::Amd64);
        let verbosity_level = clap_verbosity_flag::Verbosity::default();
        let command_exec = CommandExec::new();

        let build_task = BuildTask::new(
            package_name,
            &working_dir,
            Some(&profile),
            target_arch,
            verbosity_level,
            &command_exec,
        );

        assert_eq!(build_task.package_name, package_name);
        assert_eq!(build_task.profile, Some(&profile));
        assert_eq!(build_task.target_arch, target_arch);
        assert_eq!(build_task.manifest_path, working_dir.join("Cargo.toml"));
        assert_eq!(
            std::ptr::from_ref(build_task.command_exec),
            &raw const command_exec,
            "CommandExec instances are not the same"
        );
        // TODO: Add assert for verbosity_level once `clap-verbosity-flag` crate
        // is updated to 3.0.4
    }

    #[test]
    #[should_panic(expected = "Working directory path must be absolute. Input path: \
                               relative/path/to/working/dir")]
    fn new_panics_when_working_dir_is_not_absolute() {
        let working_dir = PathBuf::from("relative/path/to/working/dir");
        let package_name = "test_package";
        let profile = Some(Profile::Dev);
        let target_arch = TargetArch::Selected(CpuArchitecture::Amd64);
        let verbosity_level = clap_verbosity_flag::Verbosity::default();
        let command_exec = CommandExec::new();

        BuildTask::new(
            package_name,
            &working_dir,
            profile.as_ref(),
            target_arch,
            verbosity_level,
            &command_exec,
        );
    }
}
