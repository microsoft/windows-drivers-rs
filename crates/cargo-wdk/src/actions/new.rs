use std::{fs::create_dir_all, path::PathBuf};

use crate::{
    cli::Cli,
    errors::NewProjectError,
    providers::exec::RunCommand,
    utils::{append_to_file, read_file_to_string, write_to_file},
};
use anyhow::{Ok, Result};
use clap::{error::ErrorKind, CommandFactory, Error as ClapError};
use include_dir::{include_dir, Dir};
use log::{debug, error};

pub static TEMPLATES_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/templates");

pub struct NewAction<'a> {
    driver_project_name: String,
    driver_type: String,
    wdk_version: String,
    cwd: PathBuf,
    command_exec: &'a dyn RunCommand,
}

impl<'a> NewAction<'a> {
    pub fn new(driver_project_name: String, driver_type: String, wdk_version: String, cwd: PathBuf, command_exec: &'a dyn RunCommand) -> Result<Self>{
        // TODO: Pre-validation checks
        Ok(Self {
            driver_project_name,
            driver_type,
            wdk_version,
            cwd,
            command_exec
        })
    }
    pub fn create_new_project(&mut self) -> Result<()> {
        debug!("Creating new project");
        self.check_driver_project_name()?;
        self.run_cargo_new()?;
        self.cwd.push(&self.driver_project_name);
        self.driver_project_name = self.driver_project_name.replace("-", "_");
        self.copy_lib_rs_template()?;
        self.update_cargo_toml()?;
        self.create_inx_file()?;
        self.copy_build_rs_template()?;
        if matches!(self.driver_type.as_str(), "KMDF" | "WDM") {
            self.copy_cargo_config()?;
        }
        debug!(
            "Project {} created successfully at {}!",
            self.driver_project_name,
            self.cwd.display()
        );
        Ok(())
    }

    fn check_driver_project_name(&mut self) -> Result<()> {
        debug!("Checking driver project name");
        Ok(if self.driver_project_name.is_empty() {
            let mut cmd = Cli::command();
            let err = ClapError::raw(
                ErrorKind::MissingRequiredArgument,
                "Driver project name must be provided and cannot be empty.",
            );
            error!("Driver project name is missing");
            err.format(&mut cmd).exit();
        })
    }

    fn run_cargo_new(&self) -> Result<()> {
        debug!(
            "Running cargo new for project: {}",
            self.driver_project_name
        );

        let args = ["new", "--lib", &self.driver_project_name, "--vcs", "none"];

        self.command_exec.run("cargo", &args, None)?;

        debug!(
            "Successfully ran cargo new for project: {}",
            self.driver_project_name
        );

        Ok(())
    }

    pub fn copy_lib_rs_template(&self) -> Result<()> {
        debug!(
            "Copying lib.rs template for driver type: {}",
            self.driver_type
        );
        let template_path = PathBuf::from(&self.driver_type).join("lib.rs.tmp");
        let template_file = TEMPLATES_DIR
            .get_file(template_path.to_str().unwrap())
            .ok_or_else(|| {
                NewProjectError::TemplateNotFoundError(template_path.to_string_lossy().into_owned())
            })?;
        let lib_rs_path = self.cwd.join("src/lib.rs");
        write_to_file(&lib_rs_path, template_file.contents())?;
        Ok(())
    }

    pub fn copy_build_rs_template(&self) -> Result<()> {
        debug!(
            "Copying build.rs template for driver type: {}",
            self.driver_type
        );
        let template_path = PathBuf::from("build.rs.tmp");
        let template_file = TEMPLATES_DIR
            .get_file(template_path.to_str().unwrap())
            .ok_or_else(|| {
                NewProjectError::TemplateNotFoundError(template_path.to_string_lossy().into_owned())
            })?;
        let lib_rs_path = self.cwd.join("build.rs");
        write_to_file(&lib_rs_path, template_file.contents())?;
        Ok(())
    }

    pub fn update_cargo_toml(&self) -> Result<()> {
        debug!("Updating Cargo.toml for driver type: {}", self.driver_type);
        let cargo_toml_path = self.cwd.join("Cargo.toml");
        let mut cargo_toml_content = read_file_to_string(&cargo_toml_path)?;
        cargo_toml_content = cargo_toml_content.replace("[dependencies]\n", "");
        write_to_file(&cargo_toml_path, cargo_toml_content.as_bytes())?;

        let template_cargo_toml_path = PathBuf::from(&self.driver_type).join("Cargo.toml.tmp");
        let template_cargo_toml_file = TEMPLATES_DIR
            .get_file(template_cargo_toml_path.to_str().unwrap())
            .ok_or_else(|| {
                NewProjectError::TemplateNotFoundError(
                    template_cargo_toml_path.to_string_lossy().into_owned(),
                )
            })?;
        append_to_file(&cargo_toml_path, template_cargo_toml_file.contents())?;
        Ok(())
    }

    pub fn create_inx_file(&self) -> Result<()> {
        debug!(
            "Creating .inx file for driver: {}",
            self.driver_project_name
        );
        let inx_template_path = PathBuf::from(&self.driver_type).join("driver_name.inx.tmp");
        let inx_template_file = TEMPLATES_DIR
            .get_file(inx_template_path.to_str().unwrap())
            .ok_or_else(|| {
                NewProjectError::TemplateNotFoundError(
                    inx_template_path.to_string_lossy().into_owned(),
                )
            })?;
        let inx_content = String::from_utf8_lossy(inx_template_file.contents()).to_string();
        let substituted_inx_content =
            inx_content.replace("##driver_name_placeholder##", &self.driver_project_name);
        let inx_output_path = self.cwd.join(format!("{}.inx", self.driver_project_name));
        write_to_file(&inx_output_path, substituted_inx_content.as_bytes())?;
        Ok(())
    }

    pub fn copy_cargo_config(&self) -> Result<()> {
        debug!("Copying .cargo/config.toml file");
        create_dir_all(self.cwd.join(".cargo"))?;
        let cargo_config_path = self.cwd.join(".cargo/config.toml");
        let cargo_config_template_path = PathBuf::from("config.toml.tmp");
        let cargo_config_template_file = TEMPLATES_DIR
            .get_file(cargo_config_template_path.to_str().unwrap())
            .ok_or_else(|| {
                NewProjectError::TemplateNotFoundError(
                    cargo_config_template_path.to_string_lossy().into_owned(),
                )
            })?;
        write_to_file(&cargo_config_path, cargo_config_template_file.contents())?;
        Ok(())
    }
}
