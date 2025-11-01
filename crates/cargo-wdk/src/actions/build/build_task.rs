// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! Module that handles low level build operations for driver packages
//! This module defines the `BuildTask` struct and its associated methods for
//! building a driver package with the provided options using the `cargo build`
//! command.

use std::path::{Path, PathBuf};

use anyhow::Result;
use cargo_metadata::Message;
use mockall_double::double;
use tracing::info;
use wdk_build::CpuArchitecture;

#[double]
use crate::providers::exec::CommandExec;
use crate::{
    actions::{Profile, build::error::BuildTaskError, to_target_triple},
    trace,
};

/// Builds specified package by running `cargo build`  
pub struct BuildTask<'a> {
    package_name: &'a str,
    profile: Option<&'a Profile>,
    target_arch: Option<&'a CpuArchitecture>,
    verbosity_level: clap_verbosity_flag::Verbosity,
    manifest_path: PathBuf,
    command_exec: &'a CommandExec,
    working_dir: &'a Path,
}

impl<'a> BuildTask<'a> {
    /// Creates a new instance of `BuildTask`.
    ///
    /// # Arguments
    /// * `package_name` - The name of the package to build
    /// * `working_dir` - The working directory for the build
    /// * `profile` - An optional profile for the build
    /// * `target_arch` - The target architecture for the build
    /// * `verbosity_level` - The verbosity level for logging
    /// * `command_exec` - The command execution provider
    ///
    /// # Returns
    /// * `Self` - A new instance of `BuildTask`.
    ///
    /// # Panics
    /// * If `working_dir` is not absolute
    pub fn new(
        package_name: &'a str,
        working_dir: &'a Path,
        profile: Option<&'a Profile>,
        target_arch: Option<&'a CpuArchitecture>,
        verbosity_level: clap_verbosity_flag::Verbosity,
        command_exec: &'a CommandExec,
    ) -> Self {
        assert!(
            working_dir.is_absolute(),
            "Working directory path must be absolute. Input path: {}",
            working_dir.display()
        );
        Self {
            package_name,
            profile,
            target_arch,
            verbosity_level,
            manifest_path: working_dir.join("Cargo.toml"),
            command_exec,
            working_dir,
        }
    }

    /// Run `cargo build` with the configured options and return the raw
    /// process output for downstream processing.
    ///
    /// # Errors
    /// * `BuildTaskError::EmptyManifestPath` - If the manifest path cannot be
    ///   represented as UTF-8.
    /// * `BuildTaskError::CargoBuild` - If invoking `cargo` fails.
    pub fn run(
        self,
    ) -> Result<impl Iterator<Item = Result<Message, std::io::Error>>, BuildTaskError> {
        info!("Running cargo build for package: {}", self.package_name);
        let mut args = vec!["build".to_string()];
        args.push("--message-format=json".to_string());
        args.push("-p".to_string());
        args.push(self.package_name.to_string());
        if let Some(path) = self.manifest_path.to_str() {
            args.push("--manifest-path".to_string());
            args.push(path.to_string());
        } else {
            return Err(BuildTaskError::EmptyManifestPath);
        }
        if let Some(profile) = self.profile {
            args.push("--profile".to_string());
            args.push(profile.to_string());
        }
        if let Some(target_arch) = self.target_arch {
            args.push("--target".to_string());
            args.push(to_target_triple(*target_arch));
        }
        if let Some(flag) = trace::get_cargo_verbose_flags(self.verbosity_level) {
            args.push(flag.to_string());
        }
        let args = args
            .iter()
            .map(std::string::String::as_str)
            .collect::<Vec<&str>>();

        // Run cargo build from the provided working directory so that config.toml
        // is respected
        let output = self
            .command_exec
            .run("cargo", &args, None, Some(self.working_dir))
            .map_err(BuildTaskError::CargoBuild)?;

        Ok(Message::parse_stream(std::io::Cursor::new(output.stdout)))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        os::windows::process::ExitStatusExt,
        process::{ExitStatus, Output},
    };

    use wdk_build::CpuArchitecture;

    use super::*;
    use crate::{
        actions::Profile,
        providers::{error::CommandError, exec::MockCommandExec},
    };

    #[test]
    fn new_succeeds_for_valid_args() {
        let working_dir = PathBuf::from("C:/absolute/path/to/working/dir");
        let package_name = "test_package";
        let profile = Profile::Dev;
        let target_arch = Some(&CpuArchitecture::Amd64);
        let verbosity_level = clap_verbosity_flag::Verbosity::default();
        let command_exec = CommandExec::new();

        let build_task = BuildTask::new(
            package_name,
            &working_dir,
            Some(&profile),
            target_arch,
            verbosity_level,
            &command_exec,
        );

        assert_eq!(build_task.package_name, package_name);
        assert_eq!(build_task.profile, Some(&profile));
        assert_eq!(build_task.target_arch, target_arch);
        assert_eq!(build_task.manifest_path, working_dir.join("Cargo.toml"));
        assert_eq!(
            std::ptr::from_ref(build_task.command_exec),
            &raw const command_exec,
            "CommandExec instances are not the same"
        );
        // TODO: Add assert for verbosity_level once `clap-verbosity-flag` crate
        // is updated to 3.0.4
    }

    #[test]
    #[should_panic(expected = "Working directory path must be absolute. Input path: \
                               relative/path/to/working/dir")]
    fn new_panics_when_working_dir_is_not_absolute() {
        let working_dir = PathBuf::from("relative/path/to/working/dir");
        let package_name = "test_package";
        let profile = Some(Profile::Dev);
        let target_arch = Some(&CpuArchitecture::Arm64);
        let verbosity_level = clap_verbosity_flag::Verbosity::default();
        let command_exec = CommandExec::new();

        BuildTask::new(
            package_name,
            &working_dir,
            profile.as_ref(),
            target_arch,
            verbosity_level,
            &command_exec,
        );
    }

    #[test]
    fn run_invokes_cargo_build_with_expected_args_and_returns_output() {
        let working_dir = PathBuf::from("C:/abs/driver");
        let manifest_path = working_dir.join("Cargo.toml");
        let manifest_path_string = manifest_path.to_string_lossy().to_string();
        let profile = Profile::Release;
        let target_arch = CpuArchitecture::Amd64;
        let expected_target = super::to_target_triple(target_arch);
        let verbosity = clap_verbosity_flag::Verbosity::default();
        let mut expected_args = vec![
            "build".to_string(),
            "--message-format=json".to_string(),
            "-p".to_string(),
            "my-driver".to_string(),
            "--manifest-path".to_string(),
            manifest_path_string,
            "--profile".to_string(),
            profile.to_string(),
            "--target".to_string(),
            expected_target,
        ];
        if let Some(flag) = trace::get_cargo_verbose_flags(verbosity) {
            expected_args.push(flag.to_string());
        }
        let expected_working_dir = working_dir.clone();
        let mut expected_stdout = br#"{"reason":"build-finished","success":true}"#.to_vec();
        expected_stdout.push(b'\n');
        let expected_stdout_for_mock = expected_stdout.clone();

        let mut mock = MockCommandExec::new();
        mock.expect_run()
            .withf(move |command, args, _env, working_dir_opt| {
                let matches_command = command == "cargo";
                let matches_args = args.len() == expected_args.len()
                    && args
                        .iter()
                        .zip(expected_args.iter())
                        .all(|(actual, expected)| actual == expected);
                let matches_working_dir = working_dir_opt
                    .map(|dir| dir == expected_working_dir.as_path())
                    .unwrap_or(false);
                matches_command && matches_args && matches_working_dir
            })
            .return_once(move |_, _, _, _| {
                Ok(Output {
                    status: ExitStatus::default(),
                    stdout: expected_stdout_for_mock,
                    stderr: Vec::new(),
                })
            });
        let task = BuildTask::new(
            "my-driver",
            &working_dir,
            Some(&profile),
            Some(&target_arch),
            verbosity,
            &mock,
        );

        let messages = task
            .run()
            .expect("expected cargo output")
            .collect::<std::result::Result<Vec<_>, _>>()
            .expect("expected valid cargo messages");

        assert_eq!(messages.len(), 1);
        match &messages[0] {
            Message::BuildFinished(message) => {
                assert!(message.success, "expected build to succeed");
            }
            other => panic!("unexpected message: {other:?}"),
        }
    }

    #[test]
    fn run_returns_error_when_cargo_command_fails() {
        let working_dir = PathBuf::from("C:/abs/driver");
        let mut mock = MockCommandExec::new();
        mock.expect_run().return_once(|_, _, _, _| {
            let failure_output = Output {
                status: ExitStatus::from_raw(1),
                stdout: b"error".to_vec(),
                stderr: b"failure".to_vec(),
            };
            Err(CommandError::from_output(
                "cargo",
                &["build"],
                &failure_output,
            ))
        });

        let task = BuildTask::new(
            "my-driver",
            &working_dir,
            None,
            None,
            clap_verbosity_flag::Verbosity::default(),
            &mock,
        );

        let err = task.run().err().expect("expected cargo failure");
        assert!(matches!(err, BuildTaskError::CargoBuild(_)));
    }
}
