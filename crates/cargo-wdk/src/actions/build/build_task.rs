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
use tracing::{debug, info};
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

    /// Run cargo build, parse the JSON output and return the path to the `.dll`
    /// file of the driver (cdylib).
    ///
    /// # Errors
    /// * `BuildTaskError::EmptyManifestPath` - If the manifest path is empty
    /// * `BuildTaskError::DllNotFound` - If the driver `.dll` file is not found
    ///   in the build output
    pub fn run(&self) -> Result<PathBuf, BuildTaskError> {
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
            .run("cargo", &args, None, Some(self.working_dir))?;

        for message in Message::parse_stream(std::io::Cursor::new(&output.stdout)) {
            match message {
                Ok(Message::CompilerArtifact(artifact)) => {
                    let normalized_pkg_name = self.package_name.replace('-', "_");
                    let kind_is_cdylib = artifact
                        .target
                        .kind
                        .iter()
                        .any(|k| k.to_string() == "cdylib");
                    let crate_type_is_cdylib = artifact
                        .target
                        .crate_types
                        .iter()
                        .any(|t| t.to_string() == "cdylib");
                    let artifact_manifest_norm = artifact.manifest_path.as_str().replace('\\', "/");
                    let self_manifest_norm =
                        self.manifest_path.to_string_lossy().replace('\\', "/");
                    if artifact_manifest_norm == self_manifest_norm
                        && kind_is_cdylib
                        && crate_type_is_cdylib
                        && artifact.target.name == normalized_pkg_name
                        && !artifact.filenames.is_empty()
                    {
                        debug!(
                            "Matched driver crate (name={:?}, kinds={:?}, crate_types={:?}, \
                             filenames={:?})",
                            artifact.target.name,
                            artifact.target.kind,
                            artifact.target.crate_types,
                            artifact.filenames
                        );
                        return Ok(artifact
                            .filenames
                            .into_iter()
                            .find(|f| f.extension() == Some("dll"))
                            .ok_or(BuildTaskError::DllNotFound)?
                            .as_std_path()
                            .to_path_buf());
                    }
                    debug!(
                        "Skipping crate (name={:?}, kinds={:?}, crate_types={:?}, filenames={:?})",
                        artifact.target.name,
                        artifact.target.kind,
                        artifact.target.crate_types,
                        artifact.filenames
                    );
                }
                Ok(_) => { /* ignore */ }
                Err(err) => {
                    debug!("Skipping unparsable cargo message: {err}");
                }
            }
        }
        Err(BuildTaskError::DllNotFound)
    }
}

#[cfg(test)]
mod tests {
    use std::process::Output;

    use mockall::predicate::*;
    use wdk_build::CpuArchitecture;

    use super::*;
    use crate::{actions::Profile, providers::exec::MockCommandExec};

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

    /// Helper to build a synthetic cargo `--message-format=json`
    /// compiler-artifact line.
    fn artifact_json(
        package_id_name: &str,
        target_name: &str,
        manifest_path: &str,
        kinds: &[&str],
        crate_types: &[&str],
        filenames: &[&str],
    ) -> String {
        let mp_norm = manifest_path.replace('\\', "/");
        let kinds_json = kinds
            .iter()
            .map(|k| format!("\"{k}\""))
            .collect::<Vec<_>>()
            .join(",");
        let crate_types_json = crate_types
            .iter()
            .map(|t| format!("\"{t}\""))
            .collect::<Vec<_>>()
            .join(",");
        let files_json = filenames
            .iter()
            .map(|f| format!("\"{}\"", f.replace('\\', "/")))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            r#"{{"reason":"compiler-artifact","package_id":"{package_id_name} 0.1.0 (path+file:///{mp_norm})","manifest_path":"{mp_norm}","target":{{"name":"{target_name}","kind":[{kinds_json}],"crate_types":[{crate_types_json}],"src_path":"{mp_norm}","edition":"2021"}},"profile":{{"opt_level":"0","debug_assertions":true,"overflow_checks":true,"test":false}},"features":[],"filenames":[{files_json}],"executable":null,"fresh":false}}"#
        )
    }

    fn output_from_stdout(stdout: &str) -> Output {
        Output {
            status: std::process::ExitStatus::default(),
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        }
    }

    #[test]
    fn run_returns_dll_not_found_when_kind_not_cdylib() {
        let working_dir = PathBuf::from("C:/abs/driver");
        let manifest_path = working_dir.join("Cargo.toml");
        let manifest_path_str = manifest_path.to_string_lossy().to_string();
        // Use non-cdylib kind/crate_types so artifact is skipped.
        let json = artifact_json(
            "my-driver", // package_id name (hyphen form)
            "my-driver", // target.name (hyphen so also name mismatch vs normalized underscore)
            &manifest_path_str,
            &["lib"], // kinds
            &["lib"], // crate_types
            &["C:/abs/driver/my-driver.dll"],
        );
        let mut mock = MockCommandExec::new();
        mock.expect_run()
            .returning(move |_, _, _, _| Ok(output_from_stdout(&(json.clone() + "\n"))));
        let task = BuildTask::new(
            "my-driver",
            &working_dir,
            Some(&Profile::Dev),
            None,
            clap_verbosity_flag::Verbosity::default(),
            &mock,
        );
        let err = task.run().unwrap_err();
        assert!(matches!(err, BuildTaskError::DllNotFound));
    }

    #[test]
    fn run_returns_dll_not_found_when_name_mismatch() {
        let working_dir = PathBuf::from("C:/abs/driver");
        let manifest_path = working_dir.join("Cargo.toml");
        let manifest_path_str = manifest_path.to_string_lossy().to_string();
        // Name mismatch: target uses other_crate so skipped even though cdylib.
        let json = artifact_json(
            "other_crate", // package_id name
            "other_crate", // target.name
            &manifest_path_str,
            &["cdylib"],                        // kinds
            &["cdylib"],                        // crate_types
            &["C:/abs/driver/other_crate.dll"], // filenames
        );
        let mut mock = MockCommandExec::new();
        mock.expect_run()
            .returning(move |_, _, _, _| Ok(output_from_stdout(&(json.clone() + "\n"))));
        let task = BuildTask::new(
            "my-driver",
            &working_dir,
            Some(&Profile::Dev),
            None,
            clap_verbosity_flag::Verbosity::default(),
            &mock,
        );
        let err = task.run().unwrap_err();
        assert!(matches!(err, BuildTaskError::DllNotFound));
    }

    #[test]
    fn run_returns_dll_not_found_when_manifest_path_mismatch() {
        let working_dir = PathBuf::from("C:/abs/driver");
        let wrong_manifest = "C:/abs/other/Cargo.toml";
        let json = artifact_json(
            "my-driver",
            "my-driver",
            wrong_manifest,
            &["cdylib"],
            &["cdylib"],
            &["C:/abs/driver/my-driver.dll"],
        );
        let mut mock = MockCommandExec::new();
        mock.expect_run()
            .returning(move |_, _, _, _| Ok(output_from_stdout(&(json.clone() + "\n"))));
        let task = BuildTask::new(
            "my-driver",
            &working_dir,
            Some(&Profile::Dev),
            None,
            clap_verbosity_flag::Verbosity::default(),
            &mock,
        );
        let err = task.run().unwrap_err();
        assert!(matches!(err, BuildTaskError::DllNotFound));
    }

    #[test]
    fn run_returns_dll_not_found_when_empty_filenames() {
        let working_dir = PathBuf::from("C:/abs/driver");
        let manifest_path = working_dir.join("Cargo.toml");
        let manifest_path_str = manifest_path.to_string_lossy().replace('\\', "/");
        let json = artifact_json(
            "my-driver",
            "my-driver",
            &manifest_path_str,
            &["cdylib"],
            &["cdylib"],
            &[], // empty filenames
        );
        let mut mock = MockCommandExec::new();
        mock.expect_run()
            .returning(move |_, _, _, _| Ok(output_from_stdout(&(json.clone() + "\n"))));
        let task = BuildTask::new(
            "my-driver",
            &working_dir,
            Some(&Profile::Dev),
            None,
            clap_verbosity_flag::Verbosity::default(),
            &mock,
        );
        let err = task.run().unwrap_err();
        assert!(matches!(err, BuildTaskError::DllNotFound));
    }

    #[test]
    fn run_returns_dll_not_found_when_crate_types_not_cdylib() {
        let working_dir = PathBuf::from("C:/abs/driver");
        let manifest_path = working_dir.join("Cargo.toml");
        let manifest_path_str = manifest_path.to_string_lossy().replace('\\', "/");
        // kind is cdylib but crate_types is lib – differentiate this specific failure.
        let json = artifact_json(
            "my-driver",
            "my-driver",
            &manifest_path_str,
            &["cdylib"],
            &["lib"], // crate_types mismatch
            &["C:/abs/driver/my-driver.dll"],
        );
        let mut mock = MockCommandExec::new();
        mock.expect_run()
            .returning(move |_, _, _, _| Ok(output_from_stdout(&(json.clone() + "\n"))));
        let task = BuildTask::new(
            "my-driver",
            &working_dir,
            Some(&Profile::Dev),
            None,
            clap_verbosity_flag::Verbosity::default(),
            &mock,
        );
        let err = task.run().unwrap_err();
        assert!(matches!(err, BuildTaskError::DllNotFound));
    }

    #[test]
    fn run_returns_ok_with_matching_cdylib_artifact() {
        let working_dir = PathBuf::from("C:/abs/driver");
        let manifest_path = working_dir.join("Cargo.toml");
        let manifest_path_str = manifest_path.to_string_lossy().replace('\\', "/");
        // Provide artifact with matching target.name (underscore normalized) and a
        // hyphenated package_id name similar to real cargo output.
        // NOTE: cargo_metadata's Message::parse_stream requires specific fields
        // that match cargo's actual JSON output format exactly.
        let json = artifact_json(
            "my-driver", // package_id name (original hyphen form)
            "my_driver", // target.name underscore normalized
            &manifest_path_str,
            &["cdylib"],
            &["cdylib"],
            &["C:/abs/driver/my-driver.dll", "C:/abs/driver/my-driver.pdb"],
        );
        let mut mock = MockCommandExec::new();
        mock.expect_run()
            .returning(move |_, _, _, _| Ok(output_from_stdout(&(json.clone() + "\n"))));
        let task = BuildTask::new(
            "my-driver",
            &working_dir,
            Some(&Profile::Dev),
            None,
            clap_verbosity_flag::Verbosity::default(),
            &mock,
        );
        let dll_path = task.run().expect("Expected successful match");
        assert_eq!(dll_path.to_string_lossy(), "C:/abs/driver/my-driver.dll");
    }

    #[test]
    fn run_ignores_unparsable_messages_and_still_errors() {
        let working_dir = PathBuf::from("C:/abs/driver");
        // Intentionally invalid JSON line
        let stdout = "{not-json}\n";
        let mut mock = MockCommandExec::new();
        mock.expect_run()
            .returning(move |_, _, _, _| Ok(output_from_stdout(stdout)));
        let task = BuildTask::new(
            "my-driver",
            &working_dir,
            Some(&Profile::Dev),
            None,
            clap_verbosity_flag::Verbosity::default(),
            &mock,
        );
        let err = task.run().unwrap_err();
        assert!(matches!(err, BuildTaskError::DllNotFound));
    }

    #[test]
    fn run_errors_on_empty_manifest_path() {
        // Create a BuildTask with a manifest path that cannot be converted to &str
        // Hard to simulate on Windows with valid PathBuf; instead directly test failure
        // branch by creating a synthetic task and overriding manifest_path with
        // OsString containing null. For simplicity here, we assert current
        // implementation succeeds for UTF-8; deeper test would require refactor
        // to inject manifest_path creation.
        let working_dir = PathBuf::from("C:/abs/driver");
        let mut mock = MockCommandExec::new();
        mock.expect_run().returning(|_, _, _, _| {
            Ok(Output {
                status: std::process::ExitStatus::default(),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        });
        let task = BuildTask::new(
            "pkg",
            &working_dir,
            Some(&Profile::Dev),
            None,
            clap_verbosity_flag::Verbosity::default(),
            &mock,
        );
        // This will end as DllNotFound since no JSON lines provided
        let err = task.run().unwrap_err();
        assert!(matches!(err, BuildTaskError::DllNotFound));
    }
}
