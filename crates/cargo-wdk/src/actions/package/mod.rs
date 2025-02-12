#[cfg(test)]
mod tests;

// Module imports
mod error;
use error::PackageProjectError;
mod package_driver;

// Non local imports
use anyhow::Result;
use log::{debug, info};
use package_driver::PackageDriver;
use std::{fmt, path::PathBuf, result::Result::Ok};
use wdk_build::metadata::Wdk;

use super::build::BuildAction;
use crate::{cli::{Profile, TargetArch}, providers::{exec::RunCommand, fs::FSProvider, wdk_build::WdkBuildProvider}};

struct TargetTriplet(String);

impl From<&TargetArch> for TargetTriplet {
    fn from(target_arch: &TargetArch) -> Self {
        Self(format!("{}-pc-windows-msvc", target_arch))
    }
}

impl fmt::Display for TargetTriplet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// #[derive(Debug)]
pub struct PackageAction<'a> {
    working_dir: PathBuf,
    profile: Profile,
    target_triplet: TargetTriplet,
    is_sample_class: bool,
    verbosity_level: clap_verbosity_flag::Verbosity,

    // Injected deps
    wdk_build_provider: &'a dyn WdkBuildProvider,
    command_exec: &'a dyn RunCommand,
    fs_provider: &'a dyn FSProvider,
}

impl<'a> PackageAction<'a> {
    pub fn new(
        working_dir: PathBuf,
        profile: Profile,
        target_arch: TargetArch,
        is_sample_class: bool,
        verbosity_level: clap_verbosity_flag::Verbosity,
        wdk_build_provider: &'a dyn WdkBuildProvider,
        command_exec: &'a dyn RunCommand,
        fs_provider: &'a dyn FSProvider,
    ) -> Result<Self> {
        // TODO: validate and init attrs here
        let path_env_var_values = wdk_build::cargo_make::setup_path()?;
        debug!(
            "Values set into PATH env variable: {:?}",
            path_env_var_values.into_iter().collect::<Vec<String>>()
        );

        debug!(
            "Initializing packaging for project at: {}",
            working_dir.display()
        );
        // FIXME: Canonicalizing here leads to a cargo_metadata error. Probably because it is already canonicalized, * (wild chars) won't be resolved to actual paths
        let working_dir = fs_provider.canonicalize_path(working_dir)?;
        let target_triplet = TargetTriplet::from(&target_arch);
        Ok(Self {
            working_dir,
            profile,
            target_triplet,
            is_sample_class,
            verbosity_level,
            command_exec,
            wdk_build_provider,
            fs_provider,
        })
    }

    // TODO: Add docs
    pub fn run(&self) -> Result<(), PackageProjectError> {
        // Get Cargo metadata at the current path
        let working_dir: PathBuf = self
            .working_dir
            .to_string_lossy()
            .trim_start_matches("\\\\?\\")
            .into();
        let cargo_metadata = self
            .wdk_build_provider
            .get_cargo_metadata_at_path(&working_dir)?;

        // Get target directory for the profile.
        let target_directory = cargo_metadata.target_directory.join(&self.profile.to_string());

        // Get WDK metadata once per workspace
        let wdk_metadata = Wdk::try_from(&cargo_metadata)?;

        let workspace_packages = cargo_metadata.workspace_packages();

        // TODO: Add tests
        let workspace_root = self
            .fs_provider
            .canonicalize_path(cargo_metadata.workspace_root.clone().into())?;

        if workspace_root.eq(&self.working_dir) {
            debug!("Running from workspace root");
            let target_directory: PathBuf = target_directory.into();
            for package in workspace_packages {
                let package_root_path: PathBuf = package
                    .manifest_path
                    .parent()
                    .expect("Unable to find package path from Cargo manifest path")
                    .into();

                let package_root_path = self.fs_provider.canonicalize_path(package_root_path)?;
                debug!(
                    "Processing workspace driver package: {}",
                    package_root_path.display()
                );
                self.build_and_package(
                    &package_root_path,
                    &wdk_metadata,
                    &package.metadata,
                    package.name.clone(),
                    &target_directory,
                )?;
            }
            return Ok(());
        }
        info!("Running from workspace member directory");
        let package = workspace_packages.iter().find(|p| {
            let package_root_path: PathBuf = p
                .manifest_path
                .parent()
                .expect("Unable to find package path from Cargo manifest path")
                .into();
            if let Ok(package_root_path) = self.fs_provider.canonicalize_path(package_root_path) {
                debug!(
                    "Processing workspace driver package: {}",
                    package_root_path.display()
                );
                package_root_path.eq(&self.working_dir)
            } else {
                false
            }
        });

        if package.is_none() {
            return Err(PackageProjectError::NotAWorkspaceMemberError(
                self.working_dir.clone(),
            ));
        }
        let package = package.unwrap();
        self.build_and_package(
            &self.working_dir,
            &wdk_metadata,
            &package.metadata,
            package.name.clone(),
            &target_directory.into(),
        )?;

        info!("Building and packaging completed successfully");

        Ok(())
    }

    fn build_and_package(
        &self,
        working_dir: &PathBuf,
        wdk_metadata: &Wdk,
        metadata: &serde_json::Value,
        package_name: String,
        target_dir: &PathBuf,
    ) -> Result<(), PackageProjectError> {
        info!("Processing package: {}", package_name);
        BuildAction::new(&package_name, self.verbosity_level, self.command_exec).run()?;
        if metadata.get("wdk").is_none() {
            debug!(
                "No wdk metadata found. Skipping driver package workflow for package: {}",
                package_name
            );
            return Ok(());
        }

        debug!("Found wdk metadata in package: {}", package_name);
        let package_driver = PackageDriver::new(
            &package_name,
            &working_dir,
            target_dir,
            &self.target_triplet,
            self.is_sample_class,
            wdk_metadata.driver_model.clone(),
            self.wdk_build_provider,
            self.command_exec,
            self.fs_provider,
        );
        if let Err(e) = package_driver {
            return Err(PackageProjectError::PackageDriverInitError(package_name, e));
        }

        if let Err(e) = package_driver.unwrap().run() {
            return Err(PackageProjectError::PackageDriverError(package_name, e));
        }
        info!("Processing completed for package: {}", package_name);
        Ok(())
    }
}
