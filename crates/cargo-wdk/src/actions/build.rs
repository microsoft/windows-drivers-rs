use crate::errors::CommandError;
use crate::log as logger;
use crate::providers::exec::RunCommand;
use anyhow::Result;
use log::{debug, info};

pub struct BuildAction<'a> {
    package_name: &'a str,
    verbosity_level: clap_verbosity_flag::Verbosity,
    command_exec: &'a dyn RunCommand,
}

impl<'a> BuildAction<'a> {
    pub fn new(
        package_name: &'a str,
        verbosity_level: clap_verbosity_flag::Verbosity,
        command_exec: &'a dyn RunCommand,
    ) -> Self {
        Self {
            package_name,
            verbosity_level,
            command_exec,
        }
    }

    pub fn run(&self) -> Result<(), CommandError> {
        info!("Running cargo build for package: {}", self.package_name);
        let args = match logger::get_cargo_verbose_flags(self.verbosity_level) {
            Some(flag) => vec!["build", flag, "-p", self.package_name],
            None => vec!["build", "-p", self.package_name],
        };

        self.command_exec.run("cargo", &args, None)?;
        debug!("Done");
        Ok(())
    }
}
