// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! Module that handles low level build operations for driver packages
//! This module defines the `BuildTask` struct and its associated methods for
//! building a driver package with the provided options using the `cargo build`
//! command.

use std::path::{Path, PathBuf};

use anyhow::Result;
use mockall_double::double;
use tracing::{debug, info};

#[double]
use crate::providers::exec::CommandExec;
use crate::{
    actions::{build::error::BuildTaskError, to_target_triple, Profile, TargetArch},
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
}

impl<'a> BuildTask<'a> {
    /// Factory method for `BuildTask`.
    ///
    /// Arguments:
    /// * `package_name`  – Name of the package (used for `-p <name>`).
    /// * `working_dir`   – Absolute path to the package root directory.
    /// * `profile`       – Optional cargo profile (e.g. `Release`).
    /// * `target_arch`   – Selected or default target architecture wrapper.
    /// * `verbosity_level` – Verbosity flags propagated to cargo.
    /// * `command_exec`  – Command execution provider.
    ///
    /// Returns:
    /// * `Self` - A new instance of `BuildTask`.
    pub fn new(
        package_name: &'a str,
        working_dir: &'a Path,
        profile: Option<&'a Profile>,
        target_arch: TargetArch,
        verbosity_level: clap_verbosity_flag::Verbosity,
        command_exec: &'a CommandExec,
    ) -> Self {
        debug_assert!(working_dir.is_absolute(), "working_dir should be absolute");
        Self {
            package_name,
            profile,
            target_arch,
            verbosity_level,
            manifest_path: working_dir.join("Cargo.toml"),
            command_exec,
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
        info!("Running cargo build for package: {}", self.package_name);
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
        self.command_exec.run("cargo", &args, None)?;
        debug!("Done");
        Ok(())
    }
}
