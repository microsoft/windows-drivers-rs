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
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Command, Output, Stdio},
    thread,
};

use anyhow::Result;
use mockall::automock;
use tracing::debug;

use super::error::CommandError;

/// Specifies which output stream to capture in error reporting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureStream {
    /// Capture the standard output stream
    StdOut,
    /// Capture the standard error stream
    StdErr,
}

impl std::fmt::Display for CaptureStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StdOut => write!(f, "STDOUT"),
            Self::StdErr => write!(f, "STDERR"),
        }
    }
}

/// Reads lines from `reader` and writes each line to `writer` in real-time.
/// Returns the collected lines.
fn tee_stream(reader: impl Read, mut writer: impl Write) -> std::io::Result<Vec<String>> {
    let buf_reader = BufReader::new(reader);
    let mut lines = Vec::new();
    for line in buf_reader.lines() {
        let line = line?;
        writeln!(writer, "{line}")?;
        lines.push(line);
    }
    Ok(lines)
}

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
        capture_stream: CaptureStream,
    ) -> Result<Output, CommandError> {
        debug!("Running: {} {:?}", command, args);

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

        // Force child processes to emit ANSI color codes even though the
        // captured stream is a pipe (not a TTY). Without this, most tools
        // detect the pipe and suppress colors.
        cmd.env("CARGO_TERM_COLOR", "always");
        cmd.env("CLICOLOR_FORCE", "1");

        // Pipe only the captured stream; the other inherits by default
        // so it renders directly in the console with full fidelity.
        match capture_stream {
            CaptureStream::StdOut => cmd.stdout(Stdio::piped()),
            CaptureStream::StdErr => cmd.stderr(Stdio::piped()),
        };

        let mut child = cmd
            .spawn()
            .map_err(|e| CommandError::from_io_error(command, args, e))?;

        // Take the appropriate pipe before spawning the thread, to avoid moving child.
        let handle = match capture_stream {
            CaptureStream::StdOut => {
                let pipe = child.stdout.take();
                thread::spawn(move || -> std::io::Result<Vec<String>> {
                    pipe.map_or_else(
                        || Ok(Vec::new()),
                        |reader| tee_stream(reader, std::io::stdout()),
                    )
                })
            }
            CaptureStream::StdErr => {
                let pipe = child.stderr.take();
                thread::spawn(move || -> std::io::Result<Vec<String>> {
                    pipe.map_or_else(
                        || Ok(Vec::new()),
                        |reader| tee_stream(reader, std::io::stderr()),
                    )
                })
            }
        };

        let status = child
            .wait()
            .map_err(|e| CommandError::from_io_error(command, args, e))?;

        let captured_lines = handle
            .join()
            .expect("tee thread panicked")
            .map_err(|e| CommandError::from_io_error(command, args, e))?;

        let captured_buf = captured_lines.join("\n").into_bytes();

        let output = match capture_stream {
            CaptureStream::StdOut => Output {
                status,
                stdout: captured_buf,
                stderr: Vec::new(),
            },
            CaptureStream::StdErr => Output {
                status,
                stdout: Vec::new(),
                stderr: captured_buf,
            },
        };

        if !output.status.success() {
            return Err(CommandError::from_output(
                command,
                args,
                &output,
                capture_stream,
            ));
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
