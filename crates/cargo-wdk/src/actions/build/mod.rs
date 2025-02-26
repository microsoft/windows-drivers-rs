use std::path::PathBuf;

use anyhow::Result;
use log::{debug, info};

use crate::{
    log as logger,
    providers::{error::CommandError, exec::RunCommand},
};

/// Action to build a package using cargo
pub struct BuildAction<'a> {
    package_name: &'a str,
    working_dir: &'a PathBuf,
    verbosity_level: clap_verbosity_flag::Verbosity,
    command_exec: &'a dyn RunCommand,
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
        working_dir: &'a PathBuf,
        verbosity_level: clap_verbosity_flag::Verbosity,
        command_exec: &'a dyn RunCommand,
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
        let args = logger::get_cargo_verbose_flags(self.verbosity_level).map_or_else(
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
