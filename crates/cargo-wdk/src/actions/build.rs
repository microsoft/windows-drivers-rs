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
    profile: &'a Profile,
    verbosity_level: clap_verbosity_flag::Verbosity,
    manifest_path: PathBuf,
    command_exec: &'a CommandExec,
}

impl<'a> BuildAction<'a> {
    /// Creates a new instance of `BuildAction`
    /// # Arguments
    /// * `package_name` - The name of the package to build
    /// * `working_dir` - The working directory for the build
    /// * `verbosity_level` - The verbosity level for logging
    /// * `command_exec` - The command execution provider
    /// # Returns
    /// * `Result<Self>` - A result containing the new instance of `BuildAction`
    ///   or an error
    /// # Errors
    /// * `BuildActionError::IoError` - If there is an IO error while
    ///   canonicalizing the working dir
    pub fn new(
        package_name: &'a str,
        working_dir: &'a Path,
        profile: &'a Profile,
        verbosity_level: clap_verbosity_flag::Verbosity,
        command_exec: &'a CommandExec,
        fs_provider: &'a Fs,
    ) -> Result<Self, BuildActionError> {
        let manifest_path = fs_provider.canonicalize_path(&working_dir.join("Cargo.toml"))?;
        match manifest_path.strip_extended_length_path_prefix() {
            Ok(path) => Ok(Self {
                package_name,
                profile,
                verbosity_level,
                manifest_path: path,
                command_exec,
            }),
            Err(StripExtendedPathPrefixError::NoExtendedPathPrefix) => Ok(Self {
                package_name,
                profile,
                verbosity_level,
                manifest_path,
                command_exec,
            }),
            Err(StripExtendedPathPrefixError::EmptyPath) => {
                Err(BuildActionError::EmptyManifestPath)
            }
        }
    }

    /// Entry point method to run the build action
    /// # Returns
    /// * `Result<(), CommandError>` - Result indicating success or failure of
    ///   the build action
    /// # Errors
    /// * `CommandError` - If the command execution fails
    pub fn run(&self) -> Result<(), BuildActionError> {
        info!(
            "Running cargo build for package: {}, profile: {}",
            self.package_name, self.profile
        );
        let manifest_path = self.manifest_path.to_string_lossy().to_string();
        let profile = &self.profile.to_string();
        let args = trace::get_cargo_verbose_flags(self.verbosity_level).map_or_else(
            || {
                vec![
                    "build",
                    "--manifest-path",
                    &manifest_path,
                    "-p",
                    self.package_name,
                    "--profile",
                    profile,
                ]
            },
            |flag| {
                vec![
                    "build",
                    flag,
                    "--manifest-path",
                    &manifest_path,
                    "-p",
                    self.package_name,
                    "--profile",
                    profile,
                ]
            },
        );

        self.command_exec.run("cargo", &args, None)?;
        debug!("Done");
        Ok(())
    }
}
