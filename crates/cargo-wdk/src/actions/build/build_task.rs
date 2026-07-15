// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! Module that handles low level build operations for driver packages
//! This module defines the `BuildTask` struct and its associated methods for
//! building a driver package with the provided options using the `cargo build`
//! command.

use std::path::{Path, PathBuf};

use anyhow::Result;
use cargo_metadata::Message;
use clap_cargo::Features;
use mockall::automock;
use mockall_double::double;
use tracing::debug;
use wdk_build::CpuArchitecture;

use super::features_to_cargo_args;
#[double]
use crate::providers::exec::CommandExec;
use crate::{
    actions::{Profile, build::error::BuildTaskError, to_target_triple},
    providers::error::CommandError,
    trace,
};

/// Parameters for constructing a [`BuildTask`].
#[derive(Clone, Copy)]
pub struct BuildTaskParams<'a> {
    /// The name of the package to build
    pub package_name: &'a str,
    /// The working directory for the build
    pub working_dir: &'a Path,
    /// An optional profile for the build
    pub profile: Option<&'a Profile>,
    /// The target architecture for the build
    pub target_arch: Option<CpuArchitecture>,
    /// Whether to forward `--locked` to the `cargo` invocations
    pub locked: bool,
    /// The feature selection to forward to the `cargo` invocations
    pub features: &'a Features,
    /// The verbosity level for logging
    pub verbosity_level: clap_verbosity_flag::Verbosity,
}

#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct BuildTaskRunner {}

#[automock]
#[allow(dead_code, clippy::unused_self, clippy::elidable_lifetime_names)]
impl BuildTaskRunner {
    // Returns `Box<dyn Iterator<...>>` rather than `impl Iterator<...>` because
    // this method is `#[automock]`ed. mockall must be able to *name* the return
    // type to generate the mock's expectation storage, and it cannot mock an
    // opaque `impl Trait` return. Boxing into a trait object gives mockall a
    // concrete, nameable type while still forwarding `BuildTask::run`'s lazy
    // message stream, so consumers like `get_target_dir_from_output` can
    // short-circuit instead of parsing/allocating every cargo message up front.
    pub fn run<'a>(
        &self,
        params: &BuildTaskParams<'a>,
        command_exec: &CommandExec,
    ) -> Result<Box<dyn Iterator<Item = Result<Message, std::io::Error>>>, BuildTaskError> {
        BuildTask::new(*params, command_exec).run().map(|messages| {
            Box::new(messages) as Box<dyn Iterator<Item = Result<Message, std::io::Error>>>
        })
    }
}

/// Builds specified package by running `cargo build`  
pub struct BuildTask<'a> {
    params: BuildTaskParams<'a>,
    manifest_path: PathBuf,
    command_exec: &'a CommandExec,
}

impl<'a> BuildTask<'a> {
    /// Creates a new instance of `BuildTask`.
    ///
    /// # Arguments
    /// * `params` - The [`BuildTaskParams`] describing the package to build and
    ///   the flags to forward to `cargo build`
    /// * `command_exec` - The command execution provider
    ///
    /// # Returns
    /// * `Self` - A new instance of `BuildTask`.
    ///
    /// # Panics
    /// * If `params.working_dir` is not absolute
    pub fn new(params: BuildTaskParams<'a>, command_exec: &'a CommandExec) -> Self {
        assert!(
            params.working_dir.is_absolute(),
            "Working directory path must be absolute. Input path: {}",
            params.working_dir.display()
        );
        let manifest_path = params.working_dir.join("Cargo.toml");
        Self {
            params,
            manifest_path,
            command_exec,
        }
    }

    /// Run `cargo build` with the configured options
    ///
    /// # Returns
    /// `Result<impl Iterator<Item = Result<Message, std::io::Error>>,
    /// BuildTaskError>`
    ///
    /// The returned iterator yields `Result<Message, std::io::Error>` values.
    /// Consumers must handle these results while iterating, because parsing
    /// errors surface lazily when the message stream is consumed.
    ///
    /// # Errors
    /// * `BuildTaskError::EmptyManifestPath` - If the manifest path is empty or
    ///   not a valid unicode
    /// * `BuildTaskError::CargoBuild` - If there is an error running the `cargo
    ///   build` command
    // `+ use<>` opts this RPIT out of capturing the `&self` lifetime (edition
    // 2024 captures in-scope lifetimes by default). The returned iterator owns
    // its buffer (`Cursor<Vec<u8>>`), so it is effectively `'static`; opting out
    // of the capture lets `BuildTaskRunner::run` move it out of the temporary
    // `BuildTask` and box it as a `'static` trait object.
    pub fn run(
        &self,
    ) -> Result<impl Iterator<Item = Result<Message, std::io::Error>> + use<>, BuildTaskError> {
        debug!("Running cargo build");
        let mut args = vec!["build".to_string()];
        args.push("--message-format=json-render-diagnostics".to_string());
        args.push("-p".to_string());
        args.push(self.params.package_name.to_string());
        if let Some(path) = self.manifest_path.to_str() {
            args.push("--manifest-path".to_string());
            args.push(path.to_string());
        } else {
            return Err(BuildTaskError::EmptyManifestPath);
        }
        if let Some(profile) = self.params.profile {
            args.push("--profile".to_string());
            args.push(profile.to_string());
        }
        if let Some(target_arch) = self.params.target_arch {
            args.push("--target".to_string());
            args.push(to_target_triple(target_arch));
        }
        if self.params.locked {
            args.push("--locked".to_string());
        }
        args.extend(features_to_cargo_args(self.params.features));
        if let Some(flag) = trace::get_cargo_verbose_flags(self.params.verbosity_level) {
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
            .run("cargo", &args, None, Some(self.params.working_dir))
            .map_err(|mut err| {
                // Drop stdout from CommandFailed so the noisy
                // --message-format=json-render-diagnostics output isn't bubbled up
                // in the wrapped error.
                if let CommandError::CommandFailed { stdout, .. } = &mut err {
                    stdout.clear();
                }
                BuildTaskError::CargoBuild(err)
            })?;

        debug!("cargo build done");
        Ok(Message::parse_stream(std::io::Cursor::new(output.stdout)))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        os::windows::process::ExitStatusExt,
        process::{ExitStatus, Output},
    };

    use cargo_metadata::{BuildFinished, Message};
    use wdk_build::CpuArchitecture;

    use super::*;
    use crate::{
        actions::Profile,
        providers::{error::CommandError, exec::MockCommandExec},
    };

    fn default_build_task_params<'a>(
        working_dir: &'a Path,
        features: &'a Features,
    ) -> BuildTaskParams<'a> {
        BuildTaskParams {
            package_name: "my-driver",
            working_dir,
            profile: None,
            target_arch: None,
            locked: false,
            features,
            verbosity_level: clap_verbosity_flag::Verbosity::default(),
        }
    }

    #[test]
    fn new_succeeds_for_valid_args() {
        let working_dir = PathBuf::from("C:/absolute/path/to/working/dir");
        let package_name = "test_package";
        let profile = Profile::Dev;
        let target_arch = Some(CpuArchitecture::Amd64);
        let features = Features::default();
        let command_exec = CommandExec::new();

        let build_task = BuildTask::new(
            BuildTaskParams {
                package_name,
                profile: Some(&profile),
                target_arch,
                ..default_build_task_params(&working_dir, &features)
            },
            &command_exec,
        );

        assert_eq!(build_task.params.package_name, package_name);
        assert_eq!(build_task.params.profile, Some(&profile));
        assert_eq!(build_task.params.target_arch, target_arch);
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
        let features = Features::default();
        let command_exec = CommandExec::new();

        BuildTask::new(
            default_build_task_params(&working_dir, &features),
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
        let features = Features::default();
        let expected_args = vec![
            "build".to_string(),
            "--message-format=json-render-diagnostics".to_string(),
            "-p".to_string(),
            "my-driver".to_string(),
            "--manifest-path".to_string(),
            manifest_path_string,
            "--profile".to_string(),
            "release".to_string(),
            "--target".to_string(),
            "x86_64-pc-windows-msvc".to_string(),
        ];
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
                let working_dir = working_dir_opt
                    .expect("working directory must be provided when running cargo build");
                let matches_working_dir = working_dir == expected_working_dir.as_path();
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
            BuildTaskParams {
                profile: Some(&profile),
                target_arch: Some(target_arch),
                ..default_build_task_params(&working_dir, &features)
            },
            &mock,
        );

        let messages = task
            .run()
            .expect("expected an iterator over parsed cargo message objects")
            .collect::<std::result::Result<Vec<_>, _>>()
            .expect("expected valid cargo messages");

        assert!(
            matches!(
                messages.as_slice(),
                [Message::BuildFinished(BuildFinished { success: true, .. })]
            ),
            "expected one successful BuildFinished message, got: {messages:?}"
        );
    }

    #[test]
    fn run_returns_command_failed_error_with_empty_stdout_when_cargo_build_exits_nonzero() {
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

        let features = Features::default();
        let task = BuildTask::new(default_build_task_params(&working_dir, &features), &mock);

        let err = task.run().err().expect("expected cargo failure");
        let BuildTaskError::CargoBuild(CommandError::CommandFailed {
            command,
            args,
            stdout,
        }) = err
        else {
            panic!("expected CargoBuild(CommandFailed) error, got: {err:?}");
        };
        assert_eq!(command, "cargo");
        assert!(
            args.contains(&"build".to_string()),
            "expected args to contain 'build', got: {args:?}"
        );
        assert!(
            stdout.is_empty(),
            "expected stdout to be omitted, got: {stdout}"
        );
    }

    #[test]
    fn run_returns_io_error_when_cargo_build_command_invocation_fails() {
        let working_dir = PathBuf::from("C:/abs/driver");
        let mut mock = MockCommandExec::new();
        mock.expect_run().return_once(|_, _, _, _| {
            Err(CommandError::from_io_error(
                "cargo",
                &["build"],
                std::io::Error::new(std::io::ErrorKind::NotFound, "program not found"),
            ))
        });

        let features = Features::default();
        let task = BuildTask::new(default_build_task_params(&working_dir, &features), &mock);

        let err = task
            .run()
            .err()
            .expect("expected command invocation failure");
        let BuildTaskError::CargoBuild(CommandError::IoError(command, args, io_err)) = err else {
            panic!("expected CargoBuild(IoError) error, got: {err:?}");
        };
        assert_eq!(command, "cargo");
        assert!(
            args.contains(&"build".to_string()),
            "expected args to contain 'build', got: {args:?}"
        );
        assert_eq!(io_err.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn run_forwards_locked_to_cargo_invocation_when_locked_is_set() {
        let working_dir = PathBuf::from("C:/abs/driver");
        let features = Features::default();
        let mut expected_stdout = br#"{"reason":"build-finished","success":true}"#.to_vec();
        expected_stdout.push(b'\n');
        let expected_stdout_for_mock = expected_stdout.clone();

        let mut mock = MockCommandExec::new();
        mock.expect_run()
            .withf(|command, args, _env, _wd| command == "cargo" && args.contains(&"--locked"))
            .return_once(move |_, _, _, _| {
                Ok(Output {
                    status: ExitStatus::default(),
                    stdout: expected_stdout_for_mock,
                    stderr: Vec::new(),
                })
            });

        let task = BuildTask::new(
            BuildTaskParams {
                locked: true,
                ..default_build_task_params(&working_dir, &features)
            },
            &mock,
        );

        task.run()
            .expect("expected an iterator over parsed cargo message objects")
            .collect::<std::result::Result<Vec<_>, _>>()
            .expect("expected valid cargo messages");
    }

    #[test]
    fn run_forwards_features_to_cargo_invocation_when_features_are_set() {
        let working_dir = PathBuf::from("C:/abs/driver");
        let mut features = Features::default();
        features.all_features = true;
        features.no_default_features = true;
        features.features = vec!["foo".to_string(), "bar".to_string()];
        let mut expected_stdout = br#"{"reason":"build-finished","success":true}"#.to_vec();
        expected_stdout.push(b'\n');
        let expected_stdout_for_mock = expected_stdout.clone();

        let mut mock = MockCommandExec::new();
        mock.expect_run()
            .withf(move |command, args, _env, _working_dir_opt| {
                command == "cargo"
                    && args.contains(&"--all-features")
                    && args.contains(&"--no-default-features")
                    && args.windows(2).any(|w| w == ["--features", "foo"])
                    && args.windows(2).any(|w| w == ["--features", "bar"])
            })
            .return_once(move |_, _, _, _| {
                Ok(Output {
                    status: ExitStatus::default(),
                    stdout: expected_stdout_for_mock,
                    stderr: Vec::new(),
                })
            });

        let task = BuildTask::new(default_build_task_params(&working_dir, &features), &mock);

        task.run()
            .expect("expected cargo build to succeed")
            .collect::<std::result::Result<Vec<_>, _>>()
            .expect("expected valid cargo messages");
    }
}
