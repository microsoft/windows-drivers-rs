mod actions;
mod cli;
mod log;
mod providers;

use std::process::exit;

use anyhow::{Ok, Result};
use clap::Parser;
use cli::Cli;
use ::log::error;

fn main() -> Result<()> {
    let cli: Cli = Cli::parse();
    log::init_logging(cli.verbose)?;
    if let Err(e) = cli.run() {
        error!("{}", e);
        exit(1);
    }
    Ok(())
}
