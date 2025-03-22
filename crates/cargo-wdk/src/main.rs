//! Main entry point for the cargo-wdk CLI application.
//!
//! This module initializes the CLI, sets up tracing, and runs the specified
//! commands. It uses the `clap` crate for command-line argument parsing and the
//! `tracing` crate for tracing.

mod actions;
mod cli;
mod providers;
mod trace;

use anyhow::{Ok, Result};
use clap::Parser;
use cli::Cli;
use tracing::error;

/// Main function for the cargo-wdk CLI application.
///
/// This function initializes the CLI, sets up tracing, and runs the specified
/// commands. If an error occurs during execution, it logs the error and exits
/// with a non-zero status code.
///
/// # Returns
///
/// This function returns a `Result<()>`, which is `Ok` on success or an
/// `anyhow::Error` on failure.
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
