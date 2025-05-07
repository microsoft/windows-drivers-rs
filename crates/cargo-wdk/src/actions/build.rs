// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! Module for building a package using cargo.
//!
//! This module defines the `BuildAction` struct and its associated methods for
//! building a package using the `cargo build` command. It provides
//! functionality to create a new build action and run the build process with
//! specified parameters.

use std::path::{Path, PathBuf};

use anyhow::Result;
use mockall_double::double;
use thiserror::Error;
use tracing::{debug, info};
use wdk_build::utils::{PathExt, StripExtendedPathPrefixError};

use super::TargetArch;
#[double]
use crate::providers::{exec::CommandExec, fs::Fs};
use crate::{actions::Profile, providers::error::CommandError, trace};

#[derive(Error, Debug)]
pub enum BuildActionError {
    #[error("Error getting canonicalized path for manifest file: {0}")]
    CanonicalizeManifestPath(#[from] std::io::Error),
    #[error("Empty manifest path found error")]
    EmptyManifestPath,
    #[error("Error running cargo build command: {0}")]
    CargoBuild(#[from] CommandError),
}

/// Action that orchestrates building of driver project using cargo command.
pub struct BuildAction<'a> {
    package_name: &'a str,
    profile: Option<&'a Profile>,
    target_arch: TargetArch,
    verbosity_level: clap_verbosity_flag::Verbosity,
    manifest_path: PathBuf,
    command_exec: &'a CommandExec,
}

impl<'a> BuildAction<'a> {
    /// Creates a new instance of `BuildAction`
    /// # Arguments
    /// * `package_name` - The name of the package to build
    /// * `working_dir` - The working directory for the build
    /// * `profile` - An optional profile for the build
    /// * `target_arch` - The target architecture for the build
    /// * `verbosity_level` - The verbosity level for logging
    /// * `command_exec` - The command execution provider
    /// * `fs_provider` - The file system provider
    /// # Returns
    /// * `Result<Self>` - A result containing the new instance of `BuildAction`
    ///   or an error
    /// # Errors
    /// * `BuildActionError::IoError` - If there is an IO error while
    ///   canonicalizing the working dir
    pub fn new(
        package_name: &'a str,
        working_dir: &'a Path,
        profile: Option<&'a Profile>,
        target_arch: TargetArch,
        verbosity_level: clap_verbosity_flag::Verbosity,
        command_exec: &'a CommandExec,
        fs_provider: &'a Fs,
    ) -> Result<Self, BuildActionError> {
        let mut manifest_path = fs_provider.canonicalize_path(&working_dir.join("Cargo.toml"))?;
        manifest_path = match manifest_path.strip_extended_length_path_prefix() {
            Ok(path) => path,
            Err(StripExtendedPathPrefixError::NoExtendedPathPrefix) => manifest_path,
            Err(StripExtendedPathPrefixError::EmptyPath) => {
                return Err(BuildActionError::EmptyManifestPath);
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
    /// * `Result<(), BuildActionError>` - Result indicating success or failure
    ///   of the build action
    /// # Errors
    /// * `CommandError` - If the command execution fails
    pub fn run(&self) -> Result<(), BuildActionError> {
        info!("Running cargo build for package: {}", self.package_name);
        let mut args = vec!["build".to_string()];
        args.push("-p".to_string());
        args.push(self.package_name.to_string());
        if let Some(path) = self.manifest_path.to_str() {
            args.push("--manifest-path".to_string());
            args.push(path.to_string());
        } else {
            return Err(BuildActionError::EmptyManifestPath);
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
