use std::path::PathBuf;

use anyhow::Result;
use log::{debug, info};

use crate::{errors::CommandError, log as logger, providers::exec::RunCommand};

pub struct BuildAction<'a> {
    package_name: &'a str,
    working_dir: &'a PathBuf,
    verbosity_level: clap_verbosity_flag::Verbosity,
    command_exec: &'a dyn RunCommand,
}

impl<'a> BuildAction<'a> {
    pub fn new(
        package_name: &'a str,
        working_dir: &'a PathBuf,
        verbosity_level: clap_verbosity_flag::Verbosity,
        command_exec: &'a dyn RunCommand,
    ) -> Self {
        Self {
            package_name,
            working_dir,
            verbosity_level,
            command_exec,
        }
    }

    pub fn run(&self) -> Result<(), CommandError> {
        info!("Running cargo build for package: {}", self.package_name);
        let manifest_path = self
            .working_dir
            .join("Cargo.toml")
            .to_string_lossy()
            .trim_start_matches("\\\\?\\")
            .to_string();
        let args = match logger::get_cargo_verbose_flags(self.verbosity_level) {
            Some(flag) => vec![
                "build",
                flag,
                "--manifest-path",
                &manifest_path,
                "-p",
                self.package_name,
            ],
            None => vec![
                "build",
                "--manifest-path",
                &manifest_path,
                "-p",
                self.package_name,
            ],
        };
        self.command_exec.run("cargo", &args, None)?;
        debug!("Done");
        Ok(())
    }
}
