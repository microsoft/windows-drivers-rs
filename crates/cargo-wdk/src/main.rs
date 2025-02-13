mod cli;
mod errors;
mod log;
mod actions;
mod utils;
mod providers;

use anyhow::Result;
use clap::Parser;
use cli::Cli;


fn main() -> Result<()> {
    let cli: Cli = Cli::parse();
    log::init_logging(cli.verbose)?;
    cli.run()
}
