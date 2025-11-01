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
    pub fn run<'a>(
        &self,
        command: &'a str,
        args: &'a [&'a str],
        env_vars: Option<&'a HashMap<&'a str, &'a str>>,
        working_dir: Option<&'a Path>,
    ) -> Result<Output, CommandError> {
        debug!("Running: {} {:?}", command, args);

        let mut cmd = Command::new(command);
        cmd.args(args);

        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

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
            .map_err(|e| CommandError::from_io_error(command, args, e))?;

        if !output.status.success() {
            debug!(
                "Command: {}\n Args: {:?} returned status code: {}\n",
                command, args, output.status
            );
            return Err(CommandError::from_output(command, args, &output));
        }

        debug!(
            "Command: {}\n Args: {:?}\n executed successfully.",
            command, args
        );
        Ok(output)
    }
}
