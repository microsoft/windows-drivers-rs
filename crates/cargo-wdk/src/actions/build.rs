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
use crate::{
    actions::{Profile, AARCH64_TARGET_TRIPLE_NAME, X86_64_TARGET_TRIPLE_NAME},
    providers::error::CommandError,
    trace,
};

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
    target_arch: &'a TargetArch,
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
    /// * `Self` - A new instance of `BuildAction`
    pub fn new(
        package_name: &'a str,
        working_dir: &'a Path,
        profile: &'a Profile,
        target_arch: &'a TargetArch,
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
        let target_triple = match self.target_arch {
            TargetArch::X64 => X86_64_TARGET_TRIPLE_NAME,
            TargetArch::Arm64 => AARCH64_TARGET_TRIPLE_NAME,
            _ => "",
        };
        let mut args = trace::get_cargo_verbose_flags(self.verbosity_level).map_or_else(
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
        if !target_triple.is_empty() {
            args.push("--target");
            args.push(target_triple);
        }
        self.command_exec.run("cargo", &args, None)?;
        debug!("Done");
        Ok(())
    }
}
