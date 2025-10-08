// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! The [`cargo-wdk`][crate] crate is a Cargo extension that can be used to
//! create build and package Windows driver projects.

#![allow(clippy::multiple_crate_versions)]
/// The `regex-syntax` and `regex-automata` crates have multiple version
/// dependencies because of the `matchers` crate. This will be resolved by <https://github.com/tokio-rs/tracing/pull/3219>
mod actions;
mod cli;
mod providers;
mod trace;

use anyhow::{Ok, Result};
use clap::Parser;
use cli::Cli;
use tracing::error;

#[cfg(test)]
mod test_utils;

/// Main function for the [`cargo-wdk`][crate] CLI application.
///
/// The main function parses the CLI input, sets up tracing and executes the
/// command. If an error occurs during execution, it logs the error and exits
/// with a non-zero status code.
///
/// # Returns
///
/// `Result<()>`, which is `Ok` on success or an `anyhow::Error` on failure.
///
/// # Errors
///
/// This function will return an error if tracing initialization fails or if the
/// CLI command execution fails.
fn main() -> Result<()> {
    let cli: Cli = Cli::parse();
    trace::init_tracing(cli.verbose);
    cli.run().inspect_err(|e| error!("{}", e))?;
    Ok(())
}
