//! This module provides a standardized method for command execution
//! and error handling. It serves as a wrapper around the
//! `std::process::Command`.

// This is suppressed for mockall as it generates mocks with env_vars: &Option
#![allow(clippy::ref_option_ref)]
// Warns the run method is not used, however it is used.
// The intellisense confusion seems to come from automock
#![allow(dead_code)]
#![allow(clippy::unused_self)]

use std::{
    collections::HashMap,
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
            .map_err(CommandError::IoError)?;

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
