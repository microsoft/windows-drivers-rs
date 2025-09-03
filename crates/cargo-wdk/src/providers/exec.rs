// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! This module provides a standardized and testable interface for command
//! execution and error handling. It wraps the `std::process::Command` to
//! simplify usage and ensure consistent error reporting. The use of `mockall`
//! enables mocking the `CommandExec` struct for unit testing.
use std::{
    collections::HashMap,
    process::{Command, Output, Stdio},
};

use anyhow::Result;
use tracing::debug;

use super::error::CommandError;

/// Provides limited access to `std::process::Command` methods
#[derive(Debug, Default)]
pub struct CommandExec {}

#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "Tests use mocked implementation, so this implementation becomes dead code in \
                  test configuration."
    )
)]
#[allow(
    clippy::unused_self,
    reason = "Mocking associated functions complicates testing; using instance methods instead"
)]
#[cfg_attr(test, mockall::automock)]
impl CommandExec {
    #[allow(
        clippy::ref_option_ref,
        reason = "Mock adds an extra reference to Option<HashMap<&str, &str>>."
    )]
    pub fn run<'a>(
        &self,
        command: &'a str,
        args: &'a [&'a str],
        env_vars: Option<&'a HashMap<&'a str, &'a str>>,
    ) -> Result<Output, CommandError> {
        debug!("Running: {} {:?}", command, args);

        let mut cmd = Command::new(command);
        cmd.args(args);

        if let Some(env) = env_vars {
            for (key, value) in env {
                cmd.env(key, value);
            }
        }

        let output = cmd
            .stdout(Stdio::piped())
            .spawn()
            .and_then(std::process::Child::wait_with_output)
            .map_err(|e| CommandError::from_io_error(command, args, e))?;

        if !output.status.success() {
            return Err(CommandError::from_output(command, args, &output));
        }

        debug!(
            "COMMAND: {}\n ARGS:{:?}\n OUTPUT: {}\n",
            command,
            args,
            String::from_utf8_lossy(&output.stdout)
        );

        Ok(output)
    }
}
