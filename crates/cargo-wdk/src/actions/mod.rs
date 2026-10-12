// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! This module contains the core functionality of the cargo-wdk utility. It
//! include modules which implement the business logic and common types that can
//! be shared across different actions. The `action` modules that implement the
//! business logic of the cargo-wdk utility are:
//! * `new` - New action module
//! * `build` - Build action module
//! * `clean` - Clean action module
pub mod build;
pub mod clean;
pub mod new;

use std::{path::Path, process::Output};

use mockall_double::double;

#[double]
use crate::providers::exec::CommandExec;
use crate::providers::{
    cargo_path,
    error::{CargoPathError, CommandError},
};

/// Runs the Cargo executable selected by the invoking environment.
///
/// # Errors
/// Returns path-resolution or execution errors converted to the caller's error
/// type, preserving its command-specific context.
fn run_cargo<E>(
    command_exec: &CommandExec,
    args: &[&str],
    working_dir: Option<&Path>,
) -> Result<Output, E>
where
    E: From<CargoPathError> + From<CommandError>,
{
    let cargo = cargo_path()?;
    Ok(command_exec.run(&cargo, args, None, working_dir)?)
}
