//! Main entry point for the cargo-wdk CLI application.
//!
//! This module initializes the CLI, sets up logging, and runs the specified
//! commands. It uses the `clap` crate for command-line argument parsing and the
//! `log` crate for logging.

mod actions;
mod cli;
mod log;
mod providers;

use std::process::exit;

use anyhow::{Ok, Result};
use clap::Parser;
use cli::Cli;
use tracing::error;

/// Main function for the cargo-wdk CLI application.
///
/// This function initializes the CLI, sets up logging, and runs the specified
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
/// This function will return an error if logging initialization fails or if the
/// CLI command execution fails.
fn main() -> Result<()> {
    let cli: Cli = Cli::parse();
    log::init_logging(cli.verbose)?;
    if let Err(e) = cli.run() {
        error!("{}", e);
        exit(1);
    }
    Ok(())
}
