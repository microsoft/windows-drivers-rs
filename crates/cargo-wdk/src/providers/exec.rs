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
    // The `'a` lifetime is required by mockall's `#[automock]` to generate the
    // mock impl
    #[allow(clippy::extra_unused_lifetimes)]
    pub fn run<'a>(
        &self,
        command: &'a str,
        args: &'a [&'a str],
        env_vars: Option<&'a HashMap<&'a str, &'a str>>,
        working_dir: Option<&'a Path>,
    ) -> Result<Output, CommandError> {
        self.run_with_redaction(command, args, &[], env_vars, working_dir)
    }

    /// Runs a command with the specified arguments, environment variables, and
    /// working directory, while redacting sensitive arguments from logs and
    /// error messages. The `redaction_indices` parameter specifies the indices
    /// of arguments to be redacted.
    ///
    /// # Panics
    /// If any index in `redaction_indices` is out of bounds for `args`.
    #[allow(clippy::extra_unused_lifetimes)]
    pub fn run_with_redaction<'a>(
        &self,
        command: &'a str,
        args: &'a [&'a str],
        redaction_indices: &'a [usize],
        env_vars: Option<&'a HashMap<&'a str, &'a str>>,
        working_dir: Option<&'a Path>,
    ) -> Result<Output, CommandError> {
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
        debug!("Running: {} {:?}", command, log_args);

        let mut cmd = Command::new(command);
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
            .map_err(|e| CommandError::from_io_error(command, &log_args, e))?;

        if !output.status.success() {
            return Err(CommandError::from_output(command, &log_args, &output));
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
    use super::CommandExec;

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
