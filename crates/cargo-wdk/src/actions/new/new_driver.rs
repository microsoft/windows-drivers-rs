use std::{fs::create_dir_all, path::PathBuf};

use anyhow::{Ok, Result};
use include_dir::{include_dir, Dir};
use log::{debug, info};

use crate::{
    actions::DriverType,
    errors::NewProjectError,
    providers::{exec::RunCommand, fs::FSProvider},
};

pub static TEMPLATES_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/templates");

pub struct NewDriver<'a> {
    driver_project_name: String,
    driver_type: DriverType,
    cwd: PathBuf,
    command_exec: &'a dyn RunCommand,
    fs_provider: &'a dyn FSProvider,
}

impl<'a> NewDriver<'a> {
    pub fn new(
        driver_project_name: String,
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

    pub fn run(&mut self) -> Result<()> {
        debug!("Creating new project");
        self.run_cargo_new()?;
        self.cwd.push(&self.driver_project_name);
        self.driver_project_name = self.driver_project_name.replace("-", "_");
        self.copy_lib_rs_template()?;
        self.update_cargo_toml()?;
        self.create_inx_file()?;
        self.copy_build_rs_template()?;
        if matches!(self.driver_type, DriverType::KMDF | DriverType::WDM) {
            self.copy_cargo_config()?;
        }
        info!(
            "New Driver Project {} created at {}",
            self.driver_project_name,
            self.cwd.display()
        );
        Ok(())
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
            self.driver_type.to_string()
        );
        let template_path = PathBuf::from(&self.driver_type.to_string()).join("lib.rs.tmp");
        let template_file = TEMPLATES_DIR
            .get_file(template_path.to_str().unwrap())
            .ok_or_else(|| {
                NewProjectError::TemplateNotFoundError(template_path.to_string_lossy().into_owned())
            })?;
        let lib_rs_path = self.cwd.join("src/lib.rs");
        self.fs_provider
            .write_to_file(&lib_rs_path, template_file.contents())?;
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
        self.fs_provider
            .write_to_file(&lib_rs_path, template_file.contents())?;
        Ok(())
    }

    pub fn update_cargo_toml(&self) -> Result<()> {
        debug!("Updating Cargo.toml for driver type: {}", self.driver_type);
        let cargo_toml_path = self.cwd.join("Cargo.toml");
        let mut cargo_toml_content = self.fs_provider.read_file_to_string(&cargo_toml_path)?;
        cargo_toml_content = cargo_toml_content.replace("[dependencies]\n", "");
        self.fs_provider
            .write_to_file(&cargo_toml_path, cargo_toml_content.as_bytes())?;

        let template_cargo_toml_path =
            PathBuf::from(&self.driver_type.to_string()).join("Cargo.toml.tmp");
        let template_cargo_toml_file = TEMPLATES_DIR
            .get_file(template_cargo_toml_path.to_str().unwrap())
            .ok_or_else(|| {
                NewProjectError::TemplateNotFoundError(
                    template_cargo_toml_path.to_string_lossy().into_owned(),
                )
            })?;
        self.fs_provider
            .append_to_file(&cargo_toml_path, template_cargo_toml_file.contents())?;
        Ok(())
    }

    pub fn create_inx_file(&self) -> Result<()> {
        debug!(
            "Creating .inx file for driver: {}",
            self.driver_project_name
        );
        let inx_template_path =
            PathBuf::from(&self.driver_type.to_string()).join("driver_name.inx.tmp");
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
        self.fs_provider
            .write_to_file(&inx_output_path, substituted_inx_content.as_bytes())?;
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
        self.fs_provider
            .write_to_file(&cargo_config_path, cargo_config_template_file.contents())?;
        Ok(())
    }
}
