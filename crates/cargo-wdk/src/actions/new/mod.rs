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

pub struct NewAction<'a> {
    driver_project_name: &'a str,
    driver_type: DriverType,
    cwd: PathBuf,
    command_exec: &'a dyn RunCommand,
    fs_provider: &'a dyn FSProvider,
}

impl<'a> NewAction<'a> {
    pub fn new(
        driver_project_name: &'a str,
        driver_type: DriverType,
        cwd: PathBuf,
        command_exec: &'a dyn RunCommand,
        fs_provider: &'a dyn FSProvider,
    ) -> Self {
        Self {
            driver_project_name,
            driver_type,
            cwd,
            command_exec,
            fs_provider,
        }
    }

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
