// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! This module contains the `CleanAction` struct and its associated methods
//! for cleaning build artifacts produced by the `build` command.
mod error;

use std::path::{Path, PathBuf, absolute};

use anyhow::Result;
use clap_cargo::Features;
use error::CleanActionError;
use mockall_double::double;
use tracing::{debug, error as err, info};

#[double]
use crate::providers::{exec::CommandExec, fs::Fs, metadata::Metadata};
use crate::trace;

/// Action that removes build artifacts produced by the `build` command for a
/// driver project or emulated workspace.
pub struct CleanAction<'a> {
    working_dir: PathBuf,
    verbosity_level: clap_verbosity_flag::Verbosity,

    // Injected deps
    command_exec: &'a CommandExec,
    fs: &'a Fs,
    metadata: &'a Metadata,
}

impl<'a> CleanAction<'a> {
    /// Creates a new instance of `CleanAction`.
    ///
    /// # Arguments
    /// * `working_dir` - The working directory for the clean action
    /// * `verbosity_level` - The verbosity level for logging
    /// * `command_exec` - The command execution provider instance
    /// * `fs` - The file system provider instance
    /// * `metadata` - The metadata provider instance
    ///
    /// # Returns
    /// * `Result<Self>` - A result containing either a new instance of
    ///   `CleanAction` on success, or an `anyhow::Error`.
    ///
    /// # Errors
    /// * [`anyhow::Error`] - If `working_dir` is not a syntactically valid
    ///   path, e.g. it is empty
    pub fn new(
        working_dir: &Path,
        verbosity_level: clap_verbosity_flag::Verbosity,
        command_exec: &'a CommandExec,
        fs: &'a Fs,
        metadata: &'a Metadata,
    ) -> Result<Self> {
        anyhow::ensure!(
            !working_dir.as_os_str().is_empty(),
            "working_dir must not be empty"
        );
        Ok(Self {
            working_dir: absolute(working_dir)?,
            verbosity_level,
            command_exec,
            fs,
            metadata,
        })
    }

    /// Entry point method to execute the clean action flow.
    ///
    /// The detection strategy is:
    /// 1. If the working directory has a `Cargo.toml`, run `cargo clean`
    ///    directly (standalone project or workspace root).
    /// 2. Otherwise, treat the directory as an emulated workspace: scan
    ///    immediate subdirectories for Rust projects and clean each. NOTE: This
    ///    follows the same logic as the build action.
    ///
    /// # Returns
    /// `Result<(), CleanActionError>`
    ///
    /// # Errors
    /// * `CleanActionError::FileIo` - If there is an IO error.
    /// * `CleanActionError::CargoClean` - If there is an error running the
    ///   `cargo clean` command.
    /// * `CleanActionError::NoValidRustProjectsInTheDirectory` - If no valid
    ///   Rust projects are found in the working directory.
    /// * `CleanActionError::OneOrMoreRustProjectsFailedToClean` - If one or
    ///   more Rust projects fail to clean in an emulated workspace.
    pub fn run(&self) -> Result<(), CleanActionError> {
        debug!(
            "Attempting to clean project at: {}",
            self.working_dir.display()
        );

        // Standalone driver/driver workspace support
        if self.fs.exists(&self.working_dir.join("Cargo.toml")) {
            debug!(
                "Found Cargo.toml in {}. Running cargo clean.",
                self.working_dir.display()
            );
            return self.run_cargo_clean(&self.working_dir);
        }

        if let Ok(cargo_metadata) = self.metadata.get_cargo_metadata_at_path(
            &self.working_dir,
            Vec::new(),
            &Features::default(),
        ) {
            let owns_working_dir = cargo_metadata.workspace_packages().iter().any(|p| {
                p.manifest_path
                    .parent()
                    .and_then(|path| absolute(path.as_std_path()).ok())
                    .is_some_and(|package_dir| package_dir.starts_with(&self.working_dir))
            });

            if owns_working_dir {
                let workspace_root = absolute(cargo_metadata.workspace_root.as_std_path())
                    .map_err(|e| {
                        CleanActionError::NotAbsolute(
                            cargo_metadata.workspace_root.clone().into(),
                            e,
                        )
                    })?;
                debug!(
                    "Working directory {} lies inside the workspace rooted at {}; running cargo \
                     clean from workspace root",
                    self.working_dir.display(),
                    workspace_root.display()
                );
                return self.run_cargo_clean(&workspace_root);
            }
        }

        // Emulated workspaces support
        let dirs = self.fs.read_dir_entries(&self.working_dir)?;
        debug!(
            "Checking for valid Rust projects in the working directory: {}",
            self.working_dir.display()
        );

        let mut found_at_least_one_project = false;
        let mut failed_at_least_one_project = false;
        for entry in dirs {
            debug!("Checking dir entry: {}", entry.path.display());
            if !entry.is_dir || !self.fs.exists(&entry.path.join("Cargo.toml")) {
                debug!("Dir entry is not a valid Rust package");
                continue;
            }

            let cargo_package_path = entry.path;
            let package_dir_name = cargo_package_path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();

            // Emit the log only once for the entire emulated workspace, the first
            // time a valid Rust project is discovered during the scan.
            if !found_at_least_one_project {
                info!("Cleaning package(s) in {}", self.working_dir.display());
            }
            found_at_least_one_project = true;
            debug!("Cleaning package(s) in dir {package_dir_name}");
            if let Err(e) = self.run_cargo_clean(&cargo_package_path) {
                failed_at_least_one_project = true;
                err!(
                    "Error cleaning project: {package_dir_name}, error: {:?}",
                    anyhow::Error::new(e)
                );
            }
        }

        if !found_at_least_one_project {
            return Err(CleanActionError::NoValidRustProjectsInTheDirectory(
                self.working_dir.clone(),
            ));
        }

        debug!("Done cleaning package(s) in {}", self.working_dir.display());
        if failed_at_least_one_project {
            return Err(CleanActionError::OneOrMoreRustProjectsFailedToClean(
                self.working_dir.clone(),
            ));
        }

        info!(
            "Clean completed successfully for package(s) in {}",
            self.working_dir.display()
        );
        Ok(())
    }

    /// Runs `cargo clean` in the specified directory.
    fn run_cargo_clean(&self, working_dir: &Path) -> Result<(), CleanActionError> {
        info!("Running cargo clean in {}", working_dir.display());
        let mut args = vec!["clean"];
        if let Some(flag) = trace::get_cargo_verbose_flags(self.verbosity_level) {
            args.push(flag);
        }
        self.command_exec
            .run("cargo", &args, None, Some(working_dir))
            .map_err(CleanActionError::CargoClean)?;
        info!("Cleaned project at {}", working_dir.display());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io,
        os::windows::process::ExitStatusExt,
        path::{Path, PathBuf},
        process::{ExitStatus, Output},
    };

    use mockall::predicate::eq;
    use mockall_double::double;

    use super::{CleanAction, error::CleanActionError};
    use crate::providers::{
        error::{CommandError, FileError},
        fs::DirEntryInfo,
    };
    #[double]
    use crate::providers::{exec::CommandExec, fs::Fs, metadata::Metadata};

    fn ok_output() -> Output {
        Output {
            status: ExitStatus::from_raw(0),
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }

    fn cargo_clean_err() -> CommandError {
        CommandError::CommandFailed {
            command: "cargo".to_string(),
            args: vec!["clean".to_string()],
            stdout: "boom".to_string(),
        }
    }

    /// Sets up `Fs::exists(<dir>/Cargo.toml) -> exists`.
    fn mock_cargo_toml(fs: &mut Fs, dir: &Path, exists: bool) {
        fs.expect_exists()
            .with(eq(dir.join("Cargo.toml")))
            .returning(move |_| exists);
    }

    /// Sets up `Fs::read_dir_entries(...) -> Ok(entries)` where each entry is
    /// `(name relative to working dir, is_dir)`.
    fn mock_read_dir(fs: &mut Fs, working_dir: &Path, entries: &[(&str, bool)]) {
        let entries: Vec<DirEntryInfo> = entries
            .iter()
            .map(|(name, is_dir)| DirEntryInfo {
                path: working_dir.join(name),
                is_dir: *is_dir,
            })
            .collect();
        fs.expect_read_dir_entries()
            .returning(move |_| Ok(entries.clone()));
    }

    /// Sets up an expectation for `cargo clean` invoked at `dir`.
    fn mock_cargo_clean(exec: &mut CommandExec, dir: &Path, ok: bool) {
        let dir = dir.to_owned();
        exec.expect_run()
            .withf(move |cmd, args, _env, working_dir| {
                cmd == "cargo" && args == ["clean"] && *working_dir == Some(dir.as_path())
            })
            .returning(move |_, _, _, _| {
                if ok {
                    Ok(ok_output())
                } else {
                    Err(cargo_clean_err())
                }
            });
    }

    fn metadata_not_in_workspace() -> Metadata {
        let mut metadata = Metadata::default();
        metadata
            .expect_get_cargo_metadata_at_path()
            .returning(|_, _, _| {
                Err(cargo_metadata::Error::CargoMetadata {
                    stderr: "not a workspace".to_string(),
                })
            });
        metadata
    }

    fn metadata_owning_dir(workspace_root: &Path, member_dir: &Path) -> Metadata {
        let member_fwd = member_dir.to_string_lossy().replace('\\', "/");
        let member_fwd = member_fwd.trim_start_matches("//?/").to_string();
        let id = format!("path+file:///{member_fwd}#pkg@0.1.0");
        let json = serde_json::json!({
            "target_directory": workspace_root.join("target").to_string_lossy(),
            "workspace_root": workspace_root.to_string_lossy(),
            "packages": [{
                "name": "pkg",
                "version": "0.1.0",
                "id": id,
                "dependencies": [],
                "targets": [{
                    "kind": ["lib"],
                    "crate_types": ["lib"],
                    "name": "pkg",
                    "src_path": member_dir.join("src").join("lib.rs").to_string_lossy(),
                    "edition": "2021",
                    "doc": true,
                    "doctest": false,
                    "test": true
                }],
                "features": {},
                "manifest_path": member_dir.join("Cargo.toml").to_string_lossy(),
                "authors": [],
                "categories": [],
                "keywords": [],
                "edition": "2021",
                "metadata": null
            }],
            "workspace_members": [id],
            "metadata": null,
            "version": 1
        });
        let parsed: cargo_metadata::Metadata =
            serde_json::from_value(json).expect("valid cargo metadata");
        let mut metadata = Metadata::default();
        metadata
            .expect_get_cargo_metadata_at_path()
            .returning(move |_, _, _| Ok(parsed.clone()));
        metadata
    }

    fn run_action(
        cwd: &Path,
        fs: &Fs,
        exec: &CommandExec,
        metadata: &Metadata,
    ) -> Result<(), CleanActionError> {
        CleanAction::new(
            cwd,
            clap_verbosity_flag::Verbosity::default(),
            exec,
            fs,
            metadata,
        )
        .expect("CleanAction::new should succeed")
        .run()
    }

    #[test]
    fn new_succeeds_for_valid_args() {
        let cwd = PathBuf::from("C:\\tmp");
        let fs = Fs::default();
        let exec = CommandExec::default();
        let metadata = Metadata::default();
        assert!(
            CleanAction::new(
                &cwd,
                clap_verbosity_flag::Verbosity::default(),
                &exec,
                &fs,
                &metadata,
            )
            .is_ok()
        );
    }

    #[test]
    fn new_fails_if_working_dir_is_empty() {
        let cwd = PathBuf::from("");
        let fs = Fs::default();
        let exec = CommandExec::default();
        let metadata = Metadata::default();
        let err = CleanAction::new(
            &cwd,
            clap_verbosity_flag::Verbosity::default(),
            &exec,
            &fs,
            &metadata,
        )
        .err()
        .expect("CleanAction::new should fail for empty working_dir");
        assert_eq!(err.to_string(), "working_dir must not be empty");
    }

    // ---- standalone driver / workspace root ---------------------------------

    #[test]
    fn run_invokes_cargo_clean_and_succeeds() {
        let cwd = PathBuf::from("C:\\tmp");
        let mut fs = Fs::default();
        let mut exec = CommandExec::default();
        let metadata = Metadata::default();
        mock_cargo_toml(&mut fs, &cwd, true);
        mock_cargo_clean(&mut exec, &cwd, true);
        assert!(run_action(&cwd, &fs, &exec, &metadata).is_ok());
    }

    #[test]
    fn run_returns_error_when_cargo_clean_fails() {
        let cwd = PathBuf::from("C:\\tmp");
        let mut fs = Fs::default();
        let mut exec = CommandExec::default();
        let metadata = Metadata::default();
        mock_cargo_toml(&mut fs, &cwd, true);
        mock_cargo_clean(&mut exec, &cwd, false);
        assert!(matches!(
            run_action(&cwd, &fs, &exec, &metadata),
            Err(CleanActionError::CargoClean(_))
        ));
    }

    // ---- emulated workspace -------------------------------------------------

    #[test]
    fn run_returns_error_when_no_cargo_toml_and_no_rust_projects_are_found() {
        let cwd = PathBuf::from("C:\\tmp");
        let mut fs = Fs::default();
        let exec = CommandExec::default();
        let metadata = metadata_not_in_workspace();
        mock_cargo_toml(&mut fs, &cwd, false);
        mock_read_dir(&mut fs, &cwd, &[]);
        assert!(matches!(
            run_action(&cwd, &fs, &exec, &metadata),
            Err(CleanActionError::NoValidRustProjectsInTheDirectory(_))
        ));
    }

    #[test]
    fn run_cleans_workspace_root_when_invoked_from_subdirectory() {
        let workspace_root = PathBuf::from("C:\\tmp\\ws");
        let group_dir = workspace_root.join("group");
        let member_dir = group_dir.join("pkg");
        let mut fs = Fs::default();
        let mut exec = CommandExec::default();
        let metadata = metadata_owning_dir(&workspace_root, &member_dir);
        mock_cargo_toml(&mut fs, &group_dir, false);
        mock_cargo_clean(&mut exec, &workspace_root, true);
        assert!(run_action(&group_dir, &fs, &exec, &metadata).is_ok());
    }

    #[test]
    fn run_cleans_single_rust_project_in_emulated_workspace() {
        let cwd = PathBuf::from("C:\\tmp");
        let pkg_a = cwd.join("pkg-a");
        let mut fs = Fs::default();
        let mut exec = CommandExec::default();
        let metadata = metadata_not_in_workspace();
        mock_cargo_toml(&mut fs, &cwd, false);
        mock_read_dir(&mut fs, &cwd, &[("pkg-a", true)]);
        mock_cargo_toml(&mut fs, &pkg_a, true);
        mock_cargo_clean(&mut exec, &pkg_a, true);
        assert!(run_action(&cwd, &fs, &exec, &metadata).is_ok());
    }

    #[test]
    fn run_cleans_multiple_rust_projects_in_emulated_workspace() {
        let cwd = PathBuf::from("C:\\tmp");
        let pkg_a = cwd.join("pkg-a");
        let pkg_b = cwd.join("pkg-b");
        let mut fs = Fs::default();
        let mut exec = CommandExec::default();
        let metadata = metadata_not_in_workspace();
        mock_cargo_toml(&mut fs, &cwd, false);
        mock_read_dir(&mut fs, &cwd, &[("pkg-a", true), ("pkg-b", true)]);
        mock_cargo_toml(&mut fs, &pkg_a, true);
        mock_cargo_toml(&mut fs, &pkg_b, true);
        mock_cargo_clean(&mut exec, &pkg_a, true);
        mock_cargo_clean(&mut exec, &pkg_b, true);
        assert!(run_action(&cwd, &fs, &exec, &metadata).is_ok());
    }

    #[test]
    fn run_skips_non_directory_entries_in_emulated_workspace() {
        let cwd = PathBuf::from("C:\\tmp");
        let pkg_a = cwd.join("pkg-a");
        let mut fs = Fs::default();
        let mut exec = CommandExec::default();
        let metadata = metadata_not_in_workspace();
        mock_cargo_toml(&mut fs, &cwd, false);
        // README.md (is_dir=false) is filtered out before any Cargo.toml probe.
        mock_read_dir(&mut fs, &cwd, &[("README.md", false), ("pkg-a", true)]);
        mock_cargo_toml(&mut fs, &pkg_a, true);
        mock_cargo_clean(&mut exec, &pkg_a, true);
        assert!(run_action(&cwd, &fs, &exec, &metadata).is_ok());
    }

    #[test]
    fn run_skips_directories_without_cargo_toml_in_emulated_workspace() {
        let cwd = PathBuf::from("C:\\tmp");
        let docs = cwd.join("docs");
        let pkg_a = cwd.join("pkg-a");
        let mut fs = Fs::default();
        let mut exec = CommandExec::default();
        let metadata = metadata_not_in_workspace();
        mock_cargo_toml(&mut fs, &cwd, false);
        mock_read_dir(&mut fs, &cwd, &[("docs", true), ("pkg-a", true)]);
        mock_cargo_toml(&mut fs, &docs, false);
        mock_cargo_toml(&mut fs, &pkg_a, true);
        mock_cargo_clean(&mut exec, &pkg_a, true);
        assert!(run_action(&cwd, &fs, &exec, &metadata).is_ok());
    }

    #[test]
    fn run_returns_error_when_no_subdir_has_cargo_toml() {
        let cwd = PathBuf::from("C:\\tmp");
        let docs = cwd.join("docs");
        let scripts = cwd.join("scripts");
        let mut fs = Fs::default();
        let exec = CommandExec::default();
        let metadata = metadata_not_in_workspace();
        mock_cargo_toml(&mut fs, &cwd, false);
        mock_read_dir(&mut fs, &cwd, &[("docs", true), ("scripts", true)]);
        mock_cargo_toml(&mut fs, &docs, false);
        mock_cargo_toml(&mut fs, &scripts, false);
        assert!(matches!(
            run_action(&cwd, &fs, &exec, &metadata),
            Err(CleanActionError::NoValidRustProjectsInTheDirectory(_))
        ));
    }

    #[test]
    fn run_returns_error_when_one_subproject_fails_to_clean_in_emulated_workspace() {
        let cwd = PathBuf::from("C:\\tmp");
        let pkg_ok = cwd.join("pkg-ok");
        let pkg_bad = cwd.join("pkg-bad");
        let mut fs = Fs::default();
        let mut exec = CommandExec::default();
        let metadata = metadata_not_in_workspace();
        mock_cargo_toml(&mut fs, &cwd, false);
        mock_read_dir(&mut fs, &cwd, &[("pkg-ok", true), ("pkg-bad", true)]);
        mock_cargo_toml(&mut fs, &pkg_ok, true);
        mock_cargo_toml(&mut fs, &pkg_bad, true);
        mock_cargo_clean(&mut exec, &pkg_ok, true);
        mock_cargo_clean(&mut exec, &pkg_bad, false);
        assert!(matches!(
            run_action(&cwd, &fs, &exec, &metadata),
            Err(CleanActionError::OneOrMoreRustProjectsFailedToClean(_))
        ));
    }

    #[test]
    fn run_returns_error_when_read_dir_entries_fails() {
        let cwd = PathBuf::from("C:\\tmp");
        let mut fs = Fs::default();
        let exec = CommandExec::default();
        let metadata = metadata_not_in_workspace();
        mock_cargo_toml(&mut fs, &cwd, false);
        let cwd_clone = cwd.clone();
        fs.expect_read_dir_entries().returning(move |_| {
            Err(FileError::ReadDirError(
                cwd_clone.clone(),
                io::Error::new(io::ErrorKind::PermissionDenied, "denied"),
            ))
        });
        assert!(matches!(
            run_action(&cwd, &fs, &exec, &metadata),
            Err(CleanActionError::FileIo(_))
        ));
    }
}
