// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! Module that handles low level build tasks for driver packages
//! This module defines the `BuildTask` struct and its associated methods for
//! building a driver package with the provided options using the `cargo build`
//! command.

use std::path::{Path, PathBuf};

use anyhow::Result;
use mockall_double::double;
use tracing::{debug, info};
use wdk_build::utils::{PathExt, StripExtendedPathPrefixError};

use super::{error::BuildTaskError, TargetArch};
#[double]
use crate::providers::{exec::CommandExec, fs::Fs};
use crate::{actions::Profile, trace};

/// Suports low level driver build operations
pub struct BuildTask<'a> {
    package_name: &'a str,
    profile: Option<&'a Profile>,
    target_arch: TargetArch,
    verbosity_level: clap_verbosity_flag::Verbosity,
    manifest_path: PathBuf,
    command_exec: &'a CommandExec,
}

impl<'a> BuildTask<'a> {
    /// Creates a new instance of `BuildTask`
    /// # Arguments
    /// * `package_name` - The name of the package to build
    /// * `working_dir` - The working directory for the build
    /// * `profile` - An optional profile for the build
    /// * `target_arch` - The target architecture for the build
    /// * `verbosity_level` - The verbosity level for logging
    /// * `command_exec` - The command execution provider
    /// * `fs_provider` - The file system provider
    /// # Returns
    /// * `Result<Self>` - A result containing the new instance of `BuildTask`
    ///   or an error
    /// # Errors
    /// * `BuildTaskError::IoError` - If there is an IO error while
    ///   canonicalizing the working dir
    pub fn new(
        package_name: &'a str,
        working_dir: &'a Path,
        profile: Option<&'a Profile>,
        target_arch: TargetArch,
        verbosity_level: clap_verbosity_flag::Verbosity,
        command_exec: &'a CommandExec,
        fs_provider: &'a Fs,
    ) -> Result<Self, BuildTaskError> {
        let mut manifest_path = fs_provider.canonicalize_path(&working_dir.join("Cargo.toml"))?;
        manifest_path = match manifest_path.strip_extended_length_path_prefix() {
            Ok(path) => path,
            Err(StripExtendedPathPrefixError::NoExtendedPathPrefix) => manifest_path,
            Err(StripExtendedPathPrefixError::EmptyPath) => {
                return Err(BuildTaskError::EmptyManifestPath);
            }
        };
        Ok(Self {
            package_name,
            profile,
            target_arch,
            verbosity_level,
            manifest_path,
            command_exec,
        })
    }

    /// Entry point method to run the build action
    /// # Returns
    /// * `Result<(), BuildTaskError>` - Result indicating success or failure of
    ///   the build action
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
            args.push(target_arch.to_target_triple());
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
