//! Module for handling the creation of new driver projects.
//!
//! This module defines the `NewAction` struct and its associated methods for
//! creating new driver projects using predefined templates and the `cargo new`
//! command.
mod error;

use std::{
    fs::create_dir_all,
    path::{Path, PathBuf},
};

use error::NewActionError;
use include_dir::{include_dir, Dir};
use mockall_double::double;
use tracing::{debug, info};

use crate::actions::DriverType;
#[double]
use crate::providers::{exec::CommandExec, fs::Fs};

/// Directory containing the templates to be bundled with the utility
static TEMPLATES_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/templates");

/// Action that orchestrates creation of new driver project based on the
/// specified driver type.
pub struct NewAction<'a> {
    driver_project_name: String,
    driver_type: DriverType,
    cwd: PathBuf,
    command_exec: &'a CommandExec,
    fs_provider: &'a Fs,
}

impl<'a> NewAction<'a> {
    /// Creates a new instance of `NewAction`.
    ///
    /// # Arguments
    ///
    /// * `driver_project_name` - The name of the driver project to be created.
    /// * `driver_type` - The type of the driver project to be created.
    /// * `cwd` - The current working directory inside which driver project will
    ///   be created.
    /// * `command_exec` - The provider for command exection.
    /// * `fs_provider` - The provider for file system operations.
    ///
    /// # Returns
    ///
    /// * `Self` - A new instance of `NewAction`.
    pub fn new(
        driver_project_name: &'a str,
        driver_type: DriverType,
        cwd: &'a Path,
        command_exec: &'a CommandExec,
        fs_provider: &'a Fs,
    ) -> Self {
        let cwd = cwd.join(driver_project_name);
        let driver_project_name = driver_project_name.replace('-', "_");
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
    /// * `Result<(), NewActionError>` - A result indicating success or failure
    ///   of the new driver project create action.
    ///
    /// # Errors
    ///
    /// * `NewActionError::CargoNewCommand` - If there is an error running the
    ///   `cargo new` command.
    /// * `NewActionError::TemplateNotFound` - If a template file matching the
    ///   driver type is not found
    /// * `NewActionError::FileSystem` - If there is an error with file system
    ///   operations.
    pub fn run(&self) -> Result<(), NewActionError> {
        debug!("Creating new project");
        self.run_cargo_new()?;
        self.copy_lib_rs_template()?;
        self.update_cargo_toml()?;
        self.create_inx_file()?;
        self.copy_build_rs_template()?;
        if matches!(self.driver_type, DriverType::Kmdf | DriverType::Wdm) {
            self.copy_cargo_config()?;
        }
        info!(
            "New Driver Project {} created at {}",
            self.driver_project_name,
            self.cwd.display()
        );
        Ok(())
    }

    /// Runs the `cargo new` command to create a new Rust library project.
    ///
    /// # Returns
    ///
    /// * `Result<(), NewActionError>` - A result indicating success or failure
    ///   of the command.
    ///
    /// # Errors
    ///
    /// * `NewActionError::CargoNewCommand` - If there is an error running the
    ///   `cargo new` command.
    fn run_cargo_new(&self) -> Result<(), NewActionError> {
        debug!(
            "Running cargo new for project: {}",
            self.driver_project_name
        );
        let args = ["new", "--lib", &self.cwd.to_string_lossy(), "--vcs", "none"];
        if let Err(e) = self.command_exec.run("cargo", &args, None) {
            return Err(NewActionError::CargoNewCommand(e));
        }
        debug!(
            "Successfully ran cargo new for project: {}",
            self.driver_project_name
        );
        Ok(())
    }

    /// Copies the `lib.rs` template for the specified driver type to the
    /// newly created driver project.
    ///
    /// # Returns
    ///
    /// * `Result<(), NewActionError>` - A result indicating success or failure
    ///   of the operation.
    ///
    /// # Errors
    ///
    /// * `NewActionError::TemplateNotFound` - If the matching `lib.rs` template
    ///   file is not bundled with the utility.
    /// * `NewActionError::FileSystem` - If there is an error writing lib.rs
    ///   template content to the destination lib.rs file.
    pub fn copy_lib_rs_template(&self) -> Result<(), NewActionError> {
        debug!(
            "Copying lib.rs template for driver type: {}",
            self.driver_type.to_string()
        );
        let template_path = PathBuf::from(&self.driver_type.to_string()).join("lib.rs.tmp");
        let template_file = TEMPLATES_DIR.get_file(&template_path).ok_or_else(|| {
            NewActionError::TemplateNotFound(template_path.to_string_lossy().into_owned())
        })?;
        let lib_rs_path = self.cwd.join("src/lib.rs");
        self.fs_provider
            .write_to_file(&lib_rs_path, template_file.contents())?;
        Ok(())
    }

    /// Copies the `build.rs` template for the specified driver typeto the
    /// newly created driver project.
    ///
    /// # Returns
    ///
    /// * `Result<(), NewActionError>` - A result indicating success or failure
    ///   of the operation.
    ///
    /// # Errors
    ///
    /// * `NewActionError::TemplateNotFound` - If the matching `build.rs`
    ///   template file is not bundled with the utility.
    /// * `NewActionError::FileSystem` - If there is an error writing build.rs
    ///   template content to the destination build.rs file.
    pub fn copy_build_rs_template(&self) -> Result<(), NewActionError> {
        debug!(
            "Copying build.rs template for driver type: {}",
            self.driver_type
        );
        let template_path = PathBuf::from("build.rs.tmp");
        let template_file = TEMPLATES_DIR.get_file(&template_path).ok_or_else(|| {
            NewActionError::TemplateNotFound(template_path.to_string_lossy().into_owned())
        })?;
        let lib_rs_path = self.cwd.join("build.rs");
        self.fs_provider
            .write_to_file(&lib_rs_path, template_file.contents())?;
        Ok(())
    }

    /// Updates the `Cargo.toml` file for the specified driver type.
    ///
    /// # Returns
    ///
    /// * `Result<(), NewActionError>` - A result indicating success or failure
    ///   of the operation.
    ///
    /// # Errors
    ///
    /// * `NewActionError::TemplateNotFound` - If the matching `Cargo.toml`
    ///   template file is not bundled with the utility.
    /// * `NewActionError::FileSystem` - If there is an error writing Cargo.toml
    ///   template content to the destination Cargo.toml file.
    pub fn update_cargo_toml(&self) -> Result<(), NewActionError> {
        debug!("Updating Cargo.toml for driver type: {}", self.driver_type);
        let cargo_toml_path = self.cwd.join("Cargo.toml");
        let mut cargo_toml_content = self.fs_provider.read_file_to_string(&cargo_toml_path)?;
        cargo_toml_content = cargo_toml_content.replace("[dependencies]\n", "");
        self.fs_provider
            .write_to_file(&cargo_toml_path, cargo_toml_content.as_bytes())?;

        let template_cargo_toml_path =
            PathBuf::from(&self.driver_type.to_string()).join("Cargo.toml.tmp");
        let template_cargo_toml_file = TEMPLATES_DIR
            .get_file(&template_cargo_toml_path)
            .ok_or_else(|| {
                NewActionError::TemplateNotFound(
                    template_cargo_toml_path.to_string_lossy().into_owned(),
                )
            })?;
        self.fs_provider
            .append_to_file(&cargo_toml_path, template_cargo_toml_file.contents())?;
        Ok(())
    }

    /// Creates the `.inx` file for the driver project.
    ///
    /// # Returns
    ///
    /// * `Result<(), NewActionError>` - A result indicating success or failure
    ///   of the operation.
    ///
    /// # Errors
    ///
    /// * `NewActionError::TemplateNotFound` - If the matching `.inx` template
    ///   file is not bundled with the utility.
    /// * `NewActionError::FileSystem` - If there is an error writing .inx
    ///   template content to the destination .inx file.
    pub fn create_inx_file(&self) -> Result<(), NewActionError> {
        debug!(
            "Creating .inx file for driver: {}",
            self.driver_project_name
        );
        let inx_template_path =
            PathBuf::from(&self.driver_type.to_string()).join("driver_name.inx.tmp");
        let inx_template_file = TEMPLATES_DIR.get_file(&inx_template_path).ok_or_else(|| {
            NewActionError::TemplateNotFound(inx_template_path.to_string_lossy().into_owned())
        })?;
        let inx_content = String::from_utf8_lossy(inx_template_file.contents()).to_string();
        let substituted_inx_content =
            inx_content.replace("##driver_name_placeholder##", &self.driver_project_name);
        let inx_output_path = self.cwd.join(format!("{}.inx", self.driver_project_name));
        self.fs_provider
            .write_to_file(&inx_output_path, substituted_inx_content.as_bytes())?;
        Ok(())
    }

    /// Copies the `.cargo/config.toml` file for the driver project.
    ///
    /// # Returns
    ///
    /// * `Result<(), NewActionError>` - A result indicating success or failure
    ///   of the operation.
    ///
    /// # Errors
    ///
    /// * `NewActionError::TemplateNotFound` - If the matching
    ///   `.cargo/config.toml` file is not bundled with the utility.
    /// * `NewActionError::FileSystem` - If there is an error writing
    ///   config.toml template content to the destination config.toml file.
    pub fn copy_cargo_config(&self) -> Result<(), NewActionError> {
        debug!("Copying .cargo/config.toml file");
        create_dir_all(self.cwd.join(".cargo"))?;
        let cargo_config_path = self.cwd.join(".cargo/config.toml");
        let cargo_config_template_path = PathBuf::from("config.toml.tmp");
        let cargo_config_template_file = TEMPLATES_DIR
            .get_file(&cargo_config_template_path)
            .ok_or_else(|| {
                NewActionError::TemplateNotFound(
                    cargo_config_template_path.to_string_lossy().into_owned(),
                )
            })?;
        self.fs_provider
            .write_to_file(&cargo_config_path, cargo_config_template_file.contents())?;
        Ok(())
    }
}
