pub mod new_driver;
mod error;

use std::path::PathBuf;

use anyhow::{Ok, Result};
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
    ) -> Result<Self> {
        Ok(Self {
            driver_project_name,
            driver_type,
            cwd,
            command_exec,
            fs_provider,
        })
    }

    pub fn run(&self) -> Result<()> {
        NewDriver::new(
            self.driver_project_name.to_string(),
            self.driver_type.clone(),
            self.cwd.clone(),
            self.command_exec,
            self.fs_provider,
        )?
        .run()?;
        Ok(())
    }
}
