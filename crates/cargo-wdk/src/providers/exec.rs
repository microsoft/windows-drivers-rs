// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! This module provides a standardized and testable interface for command
//! execution and error handling. It wraps the `std::process::Command` to
//! simplify usage and ensure consistent error reporting. The use of `mockall`
//! enables mocking the `CommandExec` struct for unit testing.

// Suppression added for mockall as it generates mocks with env_vars: &Option
#![allow(clippy::ref_option_ref)]
// Warns the run method is not used, however it is used.
// The intellisense confusion seems to come from automock
#![allow(dead_code)]
#![allow(clippy::unused_self)]

use std::{
    collections::HashMap,
    path::Path,
    process::{Command, Output, Stdio},
};

use anyhow::Result;
use mockall::automock;
use tracing::debug;

use super::error::CommandError;

/// Provides limited access to `std::process::Command` methods
#[derive(Debug, Default)]
pub struct CommandExec {}

#[automock]
impl CommandExec {
    /// Runs an executable specified by a path or a bare name.
    /// Bare names use the OS executable search rules, including the `PATH`
    /// environment variable.
    #[mockall::concretize]
    pub fn run<P: AsRef<Path>>(
        &self,
        path: P,
        args: &[&str],
        env_vars: Option<&HashMap<&str, &str>>,
        working_dir: Option<&Path>,
    ) -> Result<Output, CommandError> {
        self.run_with_redaction(path, args, &[], env_vars, working_dir)
    }

    /// Runs an executable specified by a path or a bare name with the specified
    /// arguments, environment variables, and working directory, while redacting
    /// sensitive arguments from logs and error messages. The
    /// `redaction_indices` parameter specifies the indices of arguments to be
    /// redacted.
    ///
    /// # Panics
    /// If any index in `redaction_indices` is out of bounds for `args`.
    #[mockall::concretize]
    pub fn run_with_redaction<P: AsRef<Path>>(
        &self,
        path: P,
        args: &[&str],
        redaction_indices: &[usize],
        env_vars: Option<&HashMap<&str, &str>>,
        working_dir: Option<&Path>,
    ) -> Result<Output, CommandError> {
        let command = path.as_ref();
        assert!(
            redaction_indices.iter().all(|&i| i < args.len()),
            "redaction index out of bounds for {} argument(s): {redaction_indices:?}",
            args.len()
        );
        let log_args: Vec<&str> = args
            .iter()
            .enumerate()
            .map(|(i, arg)| {
                if redaction_indices.contains(&i) {
                    "<hidden>"
                } else {
                    *arg
                }
            })
            .collect();
        let mut cmd = Command::new(command);
        let command = command.to_string_lossy();
        debug!("Running: {} {:?}", command, log_args);

        cmd.args(args);

        if let Some(env) = env_vars {
            for (key, value) in env {
                cmd.env(key, value);
            }
        }

        if let Some(working_dir) = working_dir {
            cmd.current_dir(working_dir);
        }

        let output = cmd
            .stdout(Stdio::piped())
            .spawn()
            .and_then(std::process::Child::wait_with_output)
            .map_err(|e| CommandError::from_io_error(&command, &log_args, e))?;

        if !output.status.success() {
            return Err(CommandError::from_output(&command, &log_args, &output));
        }

        debug!(
            "COMMAND: {}\n ARGS:{:?}\n OUTPUT: {}\n",
            command,
            log_args,
            String::from_utf8_lossy(&output.stdout)
        );

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use std::{env, ffi::OsString, fs, os::windows::ffi::OsStringExt, path::PathBuf};

    use super::CommandExec;

    #[test]
    fn run_executes_non_unicode_executable_path() {
        let dir = assert_fs::TempDir::new().unwrap();
        let mut name = OsString::from("native ");
        name.push(OsString::from_wide(&[0xD800]));
        name.push(".exe");
        let executable = dir.join(name);
        let source = PathBuf::from(env::var_os("SystemRoot").expect("SystemRoot must be set"))
            .join("System32")
            .join("cmd.exe");
        fs::copy(source, &executable).unwrap();
        assert!(executable.to_str().is_none());

        let output = CommandExec::default()
            .run(&executable, &["/D", "/C", "exit 0"], None, None)
            .unwrap();

        assert!(output.status.success());
    }

    #[test]
    fn run_with_redaction_redacts_secret_arg_in_error() {
        let exec = CommandExec::default();
        let err = exec
            .run_with_redaction(
                "cargo_wdk_nonexistent_command_xyz",
                &["--password", "supersecret"],
                &[1],
                None,
                None,
            )
            .expect_err("a nonexistent command should fail to spawn");
        let msg = err.to_string();
        assert!(
            msg.contains("<hidden>"),
            "expected redaction placeholder in error, got: {msg}"
        );
        assert!(
            !msg.contains("supersecret"),
            "secret value leaked into error output: {msg}"
        );
        assert!(
            msg.contains("--password"),
            "non-redacted args should remain visible: {msg}"
        );
    }

    #[test]
    #[should_panic(expected = "redaction index out of bounds")]
    fn run_with_redaction_panics_on_out_of_bounds_index() {
        let exec = CommandExec::default();
        // Only one argument (index 0); index 1 is out of bounds.
        let _ = exec.run_with_redaction("cmd", &["/C"], &[1], None, None);
    }
}
