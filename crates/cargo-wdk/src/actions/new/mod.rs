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
        let mut args = vec!["new", "--lib", &path_str];
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

    use crate::{
        actions::{
            DriverType,
            new::{NewAction, NewActionError},
        },
        providers::{
            error::{CommandError, FileError},
            exec::MockCommandExec,
            fs::MockFs,
        },
    };

    #[test]
    fn new_project_created_successfully() {
        let path = Path::new("test_driver");
        let driver_type = DriverType::Kmdf;

        let cases = vec![
            (Verbosity::default(), None),                   // Default
            (Verbosity::new(0, 1), Some("-q".to_string())), // Quiet
            (Verbosity::new(1, 0), Some("-v".to_string())), // Verbose
        ];

        // Set up mocks to assert a successful driver project creation.
        // The for loop below tests various verbosity levels as well
        for (verbosity_level, expected_flag) in cases {
            setup_and_assert(
                path,
                driver_type,
                verbosity_level,
                |test_setup| test_setup.set_expectations(None, expected_flag),
                |result| {
                    assert!(result.is_ok());
                },
            );
        }
    }

    #[test]
    fn when_cargo_new_fails_then_returns_cargo_new_command_error() {
        let path = Path::new("test_driver_fail_cargo_new");
        let driver_type = DriverType::Kmdf;
        let verbosity_level = Verbosity::default();

        setup_and_assert(
            path,
            driver_type,
            verbosity_level,
            |test_setup| {
                // Set up mocks with failure at cargo new step
                test_setup.set_expectations(
                    Some(FailureStep::CargoNew(Output {
                        status: ExitStatus::from_raw(1),
                        stdout: vec![],
                        stderr: "some error".into(),
                    })),
                    None,
                )
            },
            |result| {
                assert!(
                    matches!(result, Err(NewActionError::CargoNewCommand(_))),
                    "Expected CargoNewCommand error"
                );
            },
        );
    }

    #[test]
    fn when_copy_lib_rs_template_fails_then_returns_filesystem_error() {
        let path = Path::new("test_driver_fail_lib_copy");
        let driver_type = DriverType::Kmdf;
        let verbosity_level = Verbosity::default();

        setup_and_assert(
            path,
            driver_type,
            verbosity_level,
            |test_setup| {
                // Set up mocks with failure at copy lib rs template to driver project step
                test_setup.set_expectations(Some(FailureStep::CopyLibRsTemplate), None)
            },
            |result| {
                assert!(
                    matches!(
                        result,
                        Err(NewActionError::FileSystem(FileError::WriteError(_, _)))
                    ),
                    "Expected FileSystem WriteError from copy_lib_rs_template"
                );
            },
        );
    }

    type AssertionFn = fn(Result<(), NewActionError>);

    #[test]
    fn when_update_cargo_toml_fails_then_returns_filesystem_error() {
        let path = Path::new("test_driver_fail_cargo_toml_update");
        let driver_type = DriverType::Kmdf;
        let verbosity_level = Verbosity::default();

        let cases: [(bool, bool, bool, AssertionFn); 3] = [
            (false, true, true, |result: Result<(), NewActionError>| {
                assert!(
                    matches!(
                        result,
                        Err(NewActionError::FileSystem(FileError::NotFound(_)))
                    ),
                    "Expected FileSystem NotFound error from update_cargo_toml read step"
                );
            }), // Fail on reading the generated Cargo.toml
            (true, false, true, |result: Result<(), NewActionError>| {
                assert!(
                    matches!(
                        result,
                        Err(NewActionError::FileSystem(FileError::WriteError(_, _)))
                    ),
                    "Expected FileSystem WriteError from update_cargo_toml dependency section \
                     removal step"
                );
            }), // Fail on updating the cargo toml with default dependencies section removed
            (true, true, false, |result: Result<(), NewActionError>| {
                assert!(
                    matches!(
                        result,
                        Err(NewActionError::FileSystem(FileError::AppendError(_, _)))
                    ),
                    "Expected FileSystem AppendError from update_cargo_toml template append step"
                );
            }), // Fail on appending cargo toml template to the Cargo.toml
        ];

        // Set up mocks with different failure cases for update_cargo_toml
        for (is_read_success, is_dep_removal_success, is_template_append_success, assert_fn) in
            cases
        {
            setup_and_assert(
                path,
                driver_type,
                verbosity_level,
                |test_setup| {
                    test_setup.set_expectations(
                        Some(FailureStep::UpdateCargoToml(
                            is_read_success,
                            is_dep_removal_success,
                            is_template_append_success,
                        )),
                        None,
                    )
                },
                |result| {
                    assert_fn(result);
                },
            );
        }
    }

    #[test]
    fn when_create_inx_file_fails_then_returns_filesystem_error() {
        let path = Path::new("test_driver_fail_create_inx_file");
        let driver_type = DriverType::Kmdf;
        let verbosity_level = Verbosity::default();

        setup_and_assert(
            path,
            driver_type,
            verbosity_level,
            |test_setup| {
                // Set up mocks with failure at creating inx file step
                test_setup.set_expectations(Some(FailureStep::CreateInxFile), None)
            },
            |result| {
                assert!(
                    matches!(
                        result,
                        Err(NewActionError::FileSystem(FileError::WriteError(_, _)))
                    ),
                    "Expected FileSystem WriteError from create_inx_file step"
                );
            },
        );
    }

    #[test]
    fn when_create_inx_file_called_with_invalid_path_then_returns_invalid_driver_crate_name() {
        // Use an empty path component so file_name returns None
        let empty_path = Path::new("");
        let driver_type = DriverType::Kmdf;
        let verbosity_level = Verbosity::default();

        setup_and_assert(
            empty_path,
            driver_type,
            verbosity_level,
            |test_setup| {
                // Set up mocks with failure at parsing driver crate name step
                test_setup
                    .set_expectations(Some(FailureStep::UpdateCargoToml(true, true, true)), None)
            },
            |result| {
                assert!(
                    matches!(result, Err(NewActionError::InvalidDriverCrateName(_))),
                    "Expected InvalidDriverCrateName error from create_inx_file step"
                );
            },
        );
    }

    #[test]
    fn when_copy_build_rs_template_fails_then_returns_filesystem_error() {
        let path = Path::new("test_driver_fail_build_rs");
        let driver_type = DriverType::Kmdf;
        let verbosity_level = Verbosity::default();

        setup_and_assert(
            path,
            driver_type,
            verbosity_level,
            |test_setup| {
                // Set up mocks with failure at copy build rs template to driver project step
                test_setup.set_expectations(Some(FailureStep::CopyBuildRsTemplate), None)
            },
            |result| {
                assert!(
                    matches!(
                        result,
                        Err(NewActionError::FileSystem(FileError::WriteError(_, _)))
                    ),
                    "Expected FileSystem WriteError from copy_build_rs_template step"
                );
            },
        );
    }

    #[test]
    fn when_copy_cargo_config_fails_then_returns_filesystem_error() {
        let path = Path::new("test_driver_fail_cargo_config");
        let driver_type = DriverType::Kmdf;
        let verbosity_level = Verbosity::default();

        setup_and_assert(
            path,
            driver_type,
            verbosity_level,
            |test_setup| {
                // Set up mocks with failure at copy cargo config to driver project step
                test_setup.set_expectations(Some(FailureStep::CopyCargoConfig), None)
            },
            |result| {
                assert!(
                    matches!(
                        result,
                        Err(NewActionError::FileSystem(FileError::WriteError(_, _)))
                    ),
                    "Expected FileSystem WriteError from copy_cargo_config step"
                );
            },
        );
    }

    /// Helper function to set up mock expectations and assert on the result.
    ///
    /// This function takes a closure to configure the test setup (e.g., mock
    /// expectations) and another closure to perform assertions on the
    /// result of running the action. Usage: pass a closure to
    /// `set_expectations_fn` to configure mocks, and a closure to `assert_fn`
    /// to check the outcome.
    fn setup_and_assert(
        path: &Path,
        driver_type: DriverType,
        verbosity_level: Verbosity,
        set_expectations_fn: impl FnOnce(TestSetup) -> TestSetup,
        assert_fn: impl FnOnce(Result<(), NewActionError>),
    ) {
        let test_setup = TestSetup::new(path);
        let test_setup = set_expectations_fn(test_setup);

        let result = NewAction::new(
            path,
            driver_type,
            verbosity_level,
            &test_setup.mock_exec,
            &test_setup.mock_fs,
        )
        .run();

        assert_fn(result);
    }

    enum FailureStep {
        CargoNew(Output),
        CopyLibRsTemplate,
        UpdateCargoToml(bool, bool, bool),
        CreateInxFile,
        CopyBuildRsTemplate,
        CopyCargoConfig,
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

        fn set_expectations(
            mut self,
            failure_step: Option<FailureStep>,
            expected_flag: Option<String>,
        ) -> Self {
            if let Some(FailureStep::CargoNew(override_output)) = failure_step {
                return self.expect_cargo_new(Some(override_output), expected_flag);
            }
            self = self.expect_cargo_new(None, expected_flag);

            if matches!(failure_step, Some(FailureStep::CopyLibRsTemplate)) {
                return self.expect_copy_lib_rs_template(false);
            }
            self = self.expect_copy_lib_rs_template(true);

            if let Some(FailureStep::UpdateCargoToml(
                is_cargo_toml_read_success,
                is_dep_section_removal_success,
                is_template_append_to_cargo_toml_success,
            )) = failure_step
            {
                return self.expect_update_cargo_toml(
                    is_cargo_toml_read_success,
                    is_dep_section_removal_success,
                    is_template_append_to_cargo_toml_success,
                );
            }
            self = self.expect_update_cargo_toml(true, true, true);

            if matches!(failure_step, Some(FailureStep::CreateInxFile)) {
                return self.expect_create_inx_file(false);
            }
            self = self.expect_create_inx_file(true);

            if matches!(failure_step, Some(FailureStep::CopyBuildRsTemplate)) {
                return self.expect_copy_build_rs_template(false);
            }
            self = self.expect_copy_build_rs_template(true);

            if matches!(failure_step, Some(FailureStep::CopyCargoConfig)) {
                return self.expect_copy_cargo_config(false);
            }

            self.expect_copy_cargo_config(true)
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
