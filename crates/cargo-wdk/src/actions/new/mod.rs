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
use include_dir::{Dir, include_dir};
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
    /// * `path` - The path to the new driver project. The last part of the path
    ///   is used as the package name.
    /// * `driver_type` - The type of the driver project to be created.
    /// * `verbosity_level` - The verbosity level for logging.
    /// * `command_exec` - The provider for command execution.
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
            "Trying to create new {} driver package at: {}",
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
        if let Err(e) = self.command_exec.run("cargo", &args, None, None) {
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
        let build_rs_path = self.path.join("build.rs");
        self.fs
            .write_to_file(&build_rs_path, template_file.contents())?;
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

#[cfg(test)]
mod tests {
    use std::{
        io::Error,
        os::windows::process::ExitStatusExt,
        path::{Path, PathBuf},
        process::{ExitStatus, Output},
    };

    use clap_verbosity_flag::Verbosity;

    use super::error::NewActionError;
    use crate::{
        actions::{new::NewAction, DriverType},
        providers::{
            error::{CommandError, FileError},
            exec::MockCommandExec,
            fs::MockFs,
        },
    };

    // Helper struct for setting up different failure scenarios in the
    // `update_cargo_toml` step.
    struct UpdateCargoTomlCaseSetupHelper {
        is_read_success: bool,
        is_dep_removal_success: bool,
        is_template_append_success: bool,
        assert_fn: fn(Result<(), NewActionError>),
    }

    #[test]
    fn new_project_created_successfully() {
        let path = Path::new("test_driver");
        let driver_type = DriverType::Kmdf;

        let cases = vec![
            (Verbosity::default(), None),                   // Default
            (Verbosity::new(0, 1), Some("-q".to_string())), // Quiet
            (Verbosity::new(1, 0), Some("-v".to_string())), // Verbose
        ];

        for (verbosity_level, expected_flag) in cases {
            let test_new_action = TestSetup::new(path)
                .expect_cargo_new(None, expected_flag)
                .expect_copy_lib_rs_template(true)
                .expect_update_cargo_toml(true, true, true)
                .expect_create_inx_file(true)
                .expect_copy_build_rs_template(true)
                .expect_copy_cargo_config(true);
            let result = NewAction::new(
                path,
                driver_type,
                verbosity_level,
                &test_new_action.mock_exec,
                &test_new_action.mock_fs,
            )
            .run();
            assert!(result.is_ok());
        }
    }

    #[test]
    fn when_cargo_new_fails_then_returns_cargo_new_command_error() {
        let path = Path::new("test_driver_fail_cargo_new");
        let driver_type = DriverType::Kmdf;
        let verbosity_level = Verbosity::default();

        // Set up mocks with failure at cargo new step
        let test_new_action = TestSetup::new(path).expect_cargo_new(
            Some(Output {
                status: ExitStatus::from_raw(1),
                stdout: vec![],
                stderr: "some error".into(),
            }),
            None,
        );

        let result = NewAction::new(
            path,
            driver_type,
            verbosity_level,
            &test_new_action.mock_exec,
            &test_new_action.mock_fs,
        )
        .run();
        assert!(
            matches!(result, Err(NewActionError::CargoNewCommand(_))),
            "Expected CargoNewCommand error"
        );
    }

    #[test]
    fn when_copy_lib_rs_template_fails_then_returns_filesystem_error() {
        let path = Path::new("test_driver_fail_lib");
        let driver_type = DriverType::Kmdf;
        let verbosity_level = Verbosity::default();

        // Set up mocks with failure at copy lib rs template to driver project step
        let test_new_action = TestSetup::new(path)
            .expect_cargo_new(None, None)
            .expect_copy_lib_rs_template(false); // Force failure here

        let result = NewAction::new(
            path,
            driver_type,
            verbosity_level,
            &test_new_action.mock_exec,
            &test_new_action.mock_fs,
        )
        .run();
        assert!(
            matches!(
                result,
                Err(NewActionError::FileSystem(FileError::WriteError(_, _)))
            ),
            "Expected FileSystem WriteError from copy_lib_rs_template"
        );
    }

    #[test]
    fn when_update_cargo_toml_fails_for_multiple_cases_then_returns_filesystem_error() {
        let path = Path::new("test_driver_fail_cargo_toml_read");
        let driver_type = DriverType::Kmdf;
        let verbosity_level = Verbosity::default();

        let cases: [UpdateCargoTomlCaseSetupHelper; 3] = [
            UpdateCargoTomlCaseSetupHelper {
                is_read_success: false,
                is_dep_removal_success: true,
                is_template_append_success: true,
                assert_fn: |result: Result<(), NewActionError>| {
                    assert!(
                        matches!(
                            result,
                            Err(NewActionError::FileSystem(FileError::NotFound(_)))
                        ),
                        "Expected FileSystem NotFound error from update_cargo_toml read step"
                    );
                },
            }, // Fail on reading the generated Cargo.toml
            UpdateCargoTomlCaseSetupHelper {
                is_read_success: true,
                is_dep_removal_success: false,
                is_template_append_success: true,
                assert_fn: |result: Result<(), NewActionError>| {
                    assert!(
                        matches!(
                            result,
                            Err(NewActionError::FileSystem(FileError::WriteError(_, _)))
                        ),
                        "Expected FileSystem WriteError from update_cargo_toml dependency section \
                         removal step"
                    );
                },
            }, // Fail on updating the cargo toml with default dependencies section removed
            UpdateCargoTomlCaseSetupHelper {
                is_read_success: true,
                is_dep_removal_success: true,
                is_template_append_success: false,

                assert_fn: |result: Result<(), NewActionError>| {
                    assert!(
                        matches!(
                            result,
                            Err(NewActionError::FileSystem(FileError::AppendError(_, _)))
                        ),
                        "Expected FileSystem AppendError from update_cargo_toml template append \
                         step"
                    );
                },
            }, // Fail on appending cargo toml template to the Cargo.toml
        ];

        // Set up mocks with different failure cases for update_cargo_toml
        for UpdateCargoTomlCaseSetupHelper {
            is_read_success,
            is_dep_removal_success,
            is_template_append_success,
            assert_fn,
        } in cases
        {
            let test_new_action = TestSetup::new(path)
                .expect_cargo_new(None, None)
                .expect_copy_lib_rs_template(true)
                .expect_update_cargo_toml(
                    is_read_success,
                    is_dep_removal_success,
                    is_template_append_success,
                ); // Force failure here

            let result = NewAction::new(
                path,
                driver_type,
                verbosity_level,
                &test_new_action.mock_exec,
                &test_new_action.mock_fs,
            )
            .run();

            assert_fn(result);
        }
    }

    #[test]
    fn when_create_inx_file_called_with_invalid_path_then_returns_invalid_driver_crate_name() {
        // Use an empty path component so file_name returns None
        let empty_path = Path::new("");
        let driver_type = DriverType::Kmdf;
        let verbosity_level = Verbosity::default();

        // Set up mocks with failure at parsing driver crate name step
        let test_new_action = TestSetup::new(empty_path)
            .expect_cargo_new(None, None)
            .expect_copy_lib_rs_template(true)
            .expect_update_cargo_toml(true, true, true);
        let new_action = NewAction::new(
            empty_path,
            driver_type,
            verbosity_level,
            &test_new_action.mock_exec,
            &test_new_action.mock_fs,
        );
        let result = new_action.run();
        assert!(
            matches!(result, Err(NewActionError::InvalidDriverCrateName(_))),
            "Expected InvalidDriverCrateName error"
        );
    }

    #[test]
    fn when_copy_build_rs_template_fails_then_returns_filesystem_error() {
        let path = Path::new("test_driver_fail_build_rs");
        let driver_type = DriverType::Kmdf;
        let verbosity_level = Verbosity::default();

        // Set up mocks with failure at copy build rs template to driver project step
        let test_new_action = TestSetup::new(path)
            .expect_cargo_new(None, None)
            .expect_copy_lib_rs_template(true)
            .expect_update_cargo_toml(true, true, true)
            .expect_create_inx_file(true)
            .expect_copy_build_rs_template(false); // Force failure here

        let result = NewAction::new(
            path,
            driver_type,
            verbosity_level,
            &test_new_action.mock_exec,
            &test_new_action.mock_fs,
        )
        .run();
        assert!(
            matches!(
                result,
                Err(NewActionError::FileSystem(FileError::WriteError(_, _)))
            ),
            "Expected FileSystem WriteError from copy_build_rs_template step"
        );
    }

    #[test]
    fn when_copy_cargo_config_fails_then_returns_filesystem_error() {
        let path = Path::new("test_driver_fail_cargo_config");
        let driver_type = DriverType::Kmdf;
        let verbosity_level = Verbosity::default();

        // Set up mocks with failure at copy cargo config to driver project step
        let test_new_action = TestSetup::new(path)
            .expect_cargo_new(None, None)
            .expect_copy_lib_rs_template(true)
            .expect_update_cargo_toml(true, true, true)
            .expect_create_inx_file(true)
            .expect_copy_build_rs_template(true)
            .expect_copy_cargo_config(false); // Force failure here

        let result = NewAction::new(
            path,
            driver_type,
            verbosity_level,
            &test_new_action.mock_exec,
            &test_new_action.mock_fs,
        )
        .run();
        assert!(
            matches!(
                result,
                Err(NewActionError::FileSystem(FileError::WriteError(_, _)))
            ),
            "Expected FileSystem WriteError from copy_cargo_config step"
        );
    }

    struct TestSetup<'a> {
        path: &'a Path,
        mock_exec: MockCommandExec,
        mock_fs: MockFs,
    }

    impl<'a> TestSetup<'a> {
        fn new(path: &'a Path) -> Self {
            Self {
                path,
                mock_exec: MockCommandExec::new(),
                mock_fs: MockFs::new(),
            }
        }

        fn expect_cargo_new(
            mut self,
            override_output: Option<Output>,
            expected_flag: Option<String>,
        ) -> Self {
            let expected_path = self.path.to_string_lossy().to_string();
            self.mock_exec
                .expect_run()
                .withf(move |cmd, args, _, _| {
                    let matched = cmd == "cargo"
                        && args.len() >= 5
                        && args[0] == "new"
                        && args[1] == "--lib"
                        && args[2] == expected_path
                        && args[3] == "--vcs"
                        && args[4] == "none";

                    expected_flag
                        .clone()
                        .map_or(matched, |flag| matched && args.len() > 5 && args[5] == flag)
                })
                .returning(move |_, _, _, _| match override_output.clone() {
                    Some(output) => match output.status.code() {
                        Some(0) => Ok(Output {
                            status: ExitStatus::from_raw(0),
                            stdout: vec![],
                            stderr: vec![],
                        }),
                        _ => Err(CommandError::from_output("cargo", &[], &output)),
                    },
                    None => Ok(Output {
                        status: ExitStatus::from_raw(0),
                        stdout: vec![],
                        stderr: vec![],
                    }),
                });
            self
        }

        fn expect_copy_lib_rs_template(mut self, is_copy_success: bool) -> Self {
            let lib_rs_path = self.path.join("src").join("lib.rs");
            self.mock_fs
                .expect_write_to_file()
                .withf(move |path, _| path == lib_rs_path)
                .returning(move |_, _| {
                    if !is_copy_success {
                        return Err(FileError::WriteError(
                            PathBuf::from("/some_random_path/src/lib.rs"),
                            Error::other("Write error"),
                        ));
                    }
                    Ok(())
                });
            self
        }

        fn expect_update_cargo_toml(
            mut self,
            is_cargo_toml_read_success: bool,
            is_dep_section_removal_success: bool,
            is_template_append_to_cargo_toml_success: bool,
        ) -> Self {
            let cargo_toml_path = self.path.join("Cargo.toml");
            let expected_file_to_write = cargo_toml_path.clone();
            let expected_file_to_append = cargo_toml_path.clone();
            self.mock_fs
                .expect_read_file_to_string()
                .withf(move |path| path == cargo_toml_path)
                .returning(move |_| {
                    if is_cargo_toml_read_success {
                        Ok("[package]\nname = \"test_driver\"\nversion = \
                            \"0.1.0\"\n\n[dependencies]\n"
                            .to_string())
                    } else {
                        Err(FileError::NotFound(PathBuf::from("Cargo.toml")))
                    }
                });
            self.mock_fs
                .expect_write_to_file()
                .withf(move |path, content| path == expected_file_to_write && !content.is_empty())
                .returning(move |_, _| {
                    if is_dep_section_removal_success {
                        Ok(())
                    } else {
                        Err(FileError::WriteError(
                            PathBuf::from("/some_random_path/Cargo.toml"),
                            Error::other("Write error"),
                        ))
                    }
                });
            self.mock_fs
                .expect_append_to_file()
                .withf(move |path, content| path == expected_file_to_append && !content.is_empty())
                .returning(move |_, _| {
                    if is_template_append_to_cargo_toml_success {
                        Ok(())
                    } else {
                        Err(FileError::AppendError(
                            PathBuf::from("/some_random_path/Cargo.toml"),
                            Error::other("Append error"),
                        ))
                    }
                });
            self
        }

        fn expect_create_inx_file(mut self, is_create_success: bool) -> Self {
            let driver_crate_name = self.path.file_name().unwrap().to_string_lossy().to_string();
            let underscored_driver_crate_name = driver_crate_name.replace('-', "_");
            let inx_output_path = self
                .path
                .join(format!("{underscored_driver_crate_name}.inx"));
            self.mock_fs
                .expect_write_to_file()
                .withf(move |path, content| path == inx_output_path && !content.is_empty())
                .returning(move |_, _| {
                    if is_create_success {
                        Ok(())
                    } else {
                        Err(FileError::WriteError(
                            PathBuf::from("/some_random_path/some_driver.inx"),
                            Error::other("Write error"),
                        ))
                    }
                });
            self
        }

        fn expect_copy_build_rs_template(mut self, is_copy_success: bool) -> Self {
            let build_rs_path = self.path.join("build.rs");
            self.mock_fs
                .expect_write_to_file()
                .withf(move |path, _| path == build_rs_path)
                .returning(move |_, _| {
                    if is_copy_success {
                        Ok(())
                    } else {
                        Err(FileError::WriteError(
                            PathBuf::from("/some_random_path/build.rs"),
                            Error::other("Write error"),
                        ))
                    }
                });
            self
        }

        fn expect_copy_cargo_config(mut self, is_copy_success: bool) -> Self {
            let cargo_config_path = self.path.join(".cargo").join("config.toml");
            self.mock_fs
                .expect_write_to_file()
                .withf(move |path, _| path == cargo_config_path)
                .returning(move |_, _| {
                    if is_copy_success {
                        Ok(())
                    } else {
                        Err(FileError::WriteError(
                            PathBuf::from("/some_random_path/.cargo/config.toml"),
                            Error::other("Write error"),
                        ))
                    }
                });
            self
        }
    }
}
