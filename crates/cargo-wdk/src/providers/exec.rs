#![allow(clippy::ref_option_ref)] // This is suppressed for mockall as it generates mocks with env_vars: &Option
use std::{
    collections::HashMap,
    process::{Command, Output, Stdio},
};

use anyhow::Result;
use log::debug;
use mockall::automock;

use super::error::CommandError;

/// Provides limited access to `std::process::Command` methods
#[derive(Debug)]
pub struct CommandExec {}

/// A Provider trait with methods for command execution
#[automock]
pub trait RunCommand {
    fn run<'a>(
        &self,
        command: &'a str,
        args: &'a [&'a str],
        env_vars: Option<&'a HashMap<&'a str, &'a str>>,
    ) -> Result<Output, CommandError>;
}

impl RunCommand for CommandExec {
    fn run<'a>(
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
