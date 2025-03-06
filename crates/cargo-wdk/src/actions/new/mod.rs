//! Module that initializes creating a new driver project.
//!
//! This module defines the `NewAction` struct and its associated methods for
//! creating a new driver project.

mod error;
pub mod new_driver;

use std::path::PathBuf;

use anyhow::Result;
use error::NewProjectError;
use new_driver::NewDriver;

use crate::{
    actions::DriverType,
    providers::{exec::RunCommand, fs::FSProvider},
};

/// Represents the action to create a new driver project.
pub struct NewAction<'a> {
    driver_project_name: &'a str,
    driver_type: DriverType,
    cwd: PathBuf,
    command_exec: &'a dyn RunCommand,
    fs_provider: &'a dyn FSProvider,
}

impl<'a> NewAction<'a> {
    /// Creates a new instance of `NewAction`.
    ///
    /// # Arguments
    ///
    /// * `driver_project_name` - The name of the driver project.
    /// * `driver_type` - The type of the driver.
    /// * `cwd` - The current working directory.
    /// * `command_exec` - The command execution provider.
    /// * `fs_provider` - The file system provider.
    pub fn new(
        driver_project_name: &'a str,
        driver_type: DriverType,
        cwd: PathBuf,
        command_exec: &'a impl RunCommand,
        fs_provider: &'a impl FSProvider,
    ) -> Self {
        Self {
            driver_project_name,
            driver_type,
            cwd,
            command_exec,
            fs_provider,
        }
    }

    /// Runs the action to create a new driver project.
    ///
    /// # Returns
    ///
    /// * `Result<(), NewProjectError>` - A result indicating success or failure
    ///   of the action.
    /// # Errors
    ///
    /// * `NewProjectError::NewDriver` - If there is an error creating the new
    ///   driver.
    pub fn run(&self) -> Result<(), NewProjectError> {
        let new_driver = NewDriver::new(
            self.driver_project_name,
            self.driver_type.clone(),
            &self.cwd,
            self.command_exec,
            self.fs_provider,
        );

        if let Err(e) = new_driver.run() {
            return Err(NewProjectError::NewDriver(e));
        }
        Ok(())
    }
}
