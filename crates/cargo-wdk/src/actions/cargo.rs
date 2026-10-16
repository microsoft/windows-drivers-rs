// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! Provides utilities for running Cargo commands.

use std::{ffi::OsString, path::Path, process::Output};

use anyhow::{Result, anyhow};
use mockall_double::double;

#[double]
use crate::providers::exec::CommandExec;

/// Returns the path specified by the `CARGO` env var.
///
/// # Errors
/// Returns an error if `CARGO` env var is missing or empty.
pub fn cargo_path() -> Result<OsString> {
    match std::env::var_os("CARGO") {
        None => Err(anyhow!("`CARGO` environment variable is not set")),
        Some(path) if path.is_empty() => Err(anyhow!("`CARGO` environment variable is empty")),
        Some(path) => Ok(path),
    }
}

/// Runs the Cargo executable specified by the `CARGO` env var.
///
/// # Errors
/// Returns an error if `CARGO` is missing or empty, or if the command fails.
pub fn run_cargo(
    command_exec: &CommandExec,
    args: &[&str],
    working_dir: Option<&Path>,
) -> Result<Output> {
    let cargo = cargo_path()?;
    Ok(command_exec.run(&cargo, args, None, working_dir)?)
}
