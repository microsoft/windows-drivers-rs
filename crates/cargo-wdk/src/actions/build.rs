//! Module for building a package using cargo.
//!
//! This module defines the `BuildAction` struct and its associated methods for
//! building a package using the `cargo build` command. It provides
//! functionality to create a new build action and run the build process with
//! specified parameters.

use std::path::Path;

use anyhow::Result;
use mockall_double::double;
use tracing::{debug, info};

#[double]
use crate::providers::exec::CommandExec;
use crate::{providers::error::CommandError, trace};

/// Action that orchestrates building of driver project using cargo command.
pub struct BuildAction<'a> {
    package_name: &'a str,
    working_dir: &'a Path,
    verbosity_level: clap_verbosity_flag::Verbosity,
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
    pub const fn new(
        package_name: &'a str,
        working_dir: &'a Path,
        verbosity_level: clap_verbosity_flag::Verbosity,
        command_exec: &'a CommandExec,
    ) -> Self {
        Self {
            package_name,
            working_dir,
            verbosity_level,
            command_exec,
        }
    }

    /// Entry point method to run the build action
    /// # Returns
    /// * `Result<(), CommandError>` - Result indicating success or failure of
    ///   the build action
    /// # Errors
    /// * `CommandError` - If the command execution fails
    pub fn run(&self) -> Result<(), CommandError> {
        info!("Running cargo build for package: {}", self.package_name);
        let manifest_path = self
            .working_dir
            .join("Cargo.toml")
            .to_string_lossy()
            .trim_start_matches("\\\\?\\")
            .to_string();
        let args = trace::get_cargo_verbose_flags(self.verbosity_level).map_or_else(
            || {
                vec![
                    "build",
                    "--manifest-path",
                    &manifest_path,
                    "-p",
                    self.package_name,
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
                ]
            },
        );

        self.command_exec.run("cargo", &args, None)?;
        debug!("Done");
        Ok(())
    }
}
