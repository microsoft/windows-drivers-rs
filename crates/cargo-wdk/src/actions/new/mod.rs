// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! `Action` Module that creates new driver projects.
//!
//! This module defines the `NewAction` struct and its associated methods for
//! creating new driver projects. It runs `cargo new` with the provided options
//! and uses the pre-defined templates to setup the new project with the
//! necessary files and configurations.
mod error;

use std::{
    fs::create_dir_all,
    path::{Path, PathBuf},
};

use clap_verbosity_flag::Verbosity;
use error::NewActionError;
use include_dir::{include_dir, Dir};
use mockall_double::double;
use tracing::{debug, info};

#[double]
use crate::providers::{exec::CommandExec, fs::Fs};
use crate::{actions::DriverType, trace};

/// Directory containing the templates to be bundled with the utility
static TEMPLATES_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/templates");

/// `NewAction` struct and its methods orchestrates the creation of new driver
/// project based on the specified driver type.
pub struct NewAction<'a> {
    path: &'a Path,
    driver_type: DriverType,
    verbosity_level: Verbosity,
    command_exec: &'a CommandExec,
    fs: &'a Fs,
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
    /// * `verbosity_level` - The verbosity level for logging.
    /// * `command_exec` - The provider for command exection.
    /// * `fs` - The provider for file system operations.
    ///
    /// # Returns
    ///
    /// * `Self` - A new instance of `NewAction`.
    pub const fn new(
        path: &'a Path,
        driver_type: DriverType,
        verbosity_level: Verbosity,
        command_exec: &'a CommandExec,
        fs: &'a Fs,
    ) -> Self {
        Self {
            path,
            driver_type,
            verbosity_level,
            command_exec,
            fs,
        }
    }

    /// Entry point method to create a new driver project.
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
        info!(
            "Creating new {} driver crate at: {}",
            self.driver_type,
            self.path.display()
        );
        self.run_cargo_new()?;
        self.copy_lib_rs_template()?;
        self.update_cargo_toml()?;
        self.create_inx_file()?;
        self.copy_build_rs_template()?;
        self.copy_cargo_config()?;
        info!(
            "New {} driver crate created successfully at: {}",
            self.driver_type,
            self.path.display()
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
        debug!("Running cargo new command");
        let path_str = self.path.to_string_lossy().to_string();
        let mut args = vec!["new", "--lib", &path_str, "--vcs", "none"];
        if let Some(flag) = trace::get_cargo_verbose_flags(self.verbosity_level) {
            args.push(flag);
        }
        if let Err(e) = self.command_exec.run("cargo", &args, None) {
            return Err(NewActionError::CargoNewCommand(e));
        }
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
        let lib_rs_path = self.path.join("src").join("lib.rs");
        self.fs
            .write_to_file(&lib_rs_path, template_file.contents())?;
        Ok(())
    }

    /// Copies the `build.rs` template for the specified driver type to the
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
        let lib_rs_path = self.path.join("build.rs");
        self.fs
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
        let cargo_toml_path = self.path.join("Cargo.toml");
        let mut cargo_toml_content = self.fs.read_file_to_string(&cargo_toml_path)?;
        cargo_toml_content = cargo_toml_content.replace("[dependencies]\n", "");
        self.fs
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
        self.fs
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
        let driver_crate_name = self
            .path
            .file_name()
            .ok_or_else(|| {
                NewActionError::InvalidDriverCrateName(self.path.to_string_lossy().into_owned())
            })?
            .to_string_lossy()
            .to_string();
        debug!("Creating .inx file for: {}", driver_crate_name);
        let underscored_driver_crate_name = driver_crate_name.replace('-', "_");
        let inx_template_path =
            PathBuf::from(&self.driver_type.to_string()).join("driver_name.inx.tmp");
        let inx_template_file = TEMPLATES_DIR.get_file(&inx_template_path).ok_or_else(|| {
            NewActionError::TemplateNotFound(inx_template_path.to_string_lossy().into_owned())
        })?;
        let inx_content = String::from_utf8_lossy(inx_template_file.contents()).to_string();
        let substituted_inx_content = inx_content.replace(
            "##driver_name_placeholder##",
            &underscored_driver_crate_name,
        );
        let inx_output_path = self
            .path
            .join(format!("{underscored_driver_crate_name}.inx"));
        self.fs
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
        create_dir_all(self.path.join(".cargo"))?;
        let cargo_config_path = self.path.join(".cargo").join("config.toml");
        let cargo_config_template_path = PathBuf::from("config.toml.tmp");
        let cargo_config_template_file = TEMPLATES_DIR
            .get_file(&cargo_config_template_path)
            .ok_or_else(|| {
                NewActionError::TemplateNotFound(
                    cargo_config_template_path.to_string_lossy().into_owned(),
                )
            })?;
        self.fs
            .write_to_file(&cargo_config_path, cargo_config_template_file.contents())?;
        Ok(())
    }
}
