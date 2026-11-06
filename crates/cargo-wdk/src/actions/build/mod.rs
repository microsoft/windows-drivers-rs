// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! This module contains the `BuildAction` struct and its associated methods
//! for orchestrating the build and packaging of a driver project. It consists
//! the logic to build and package standalone projects, workspaces, individual
//! members in a workspace and emulated workspaces. It consists of two tasks -
//! `BuildTask` that handles the build phase and the `PackageTask` that handles
//! the package phase.

mod build_task;
mod error;
mod package_task;
#[cfg(test)]
mod tests;
use std::{
    collections::{BTreeMap, HashSet},
    fmt::{self, Display},
    io,
    path::{Path, PathBuf, absolute},
    result::Result::Ok,
    str::FromStr,
};

use anyhow::Result;
use build_task::{BuildTask, BuildTaskParams};
use cargo_metadata::{
    Artifact,
    CrateType,
    Message,
    Metadata as CargoMetadata,
    Package,
    PackageId,
    TargetKind,
};
use clap_cargo::Features;
use error::BuildActionError;
use mockall_double::double;
use package_task::{PackageTask, PackageTaskParams};
pub use package_task::{SignMode, TargetPlatform};
use tracing::{debug, error, info, trace, warn};
use wdk_build::{
    CpuArchitecture,
    metadata::{TryFromCargoMetadataError, Wdk},
};

#[double]
use crate::providers::{exec::CommandExec, fs::Fs, metadata::Metadata, wdk_build::WdkBuild};
use crate::{actions::cargo::cargo_path, providers::error::FileError};

const X86_64_TARGET_TRIPLE_NAME: &str = "x86_64-pc-windows-msvc";
const AARCH64_TARGET_TRIPLE_NAME: &str = "aarch64-pc-windows-msvc";

/// `Machine` values in the PE COFF file header
const IMAGE_FILE_MACHINE_AMD64: u16 = 0x8664;
const IMAGE_FILE_MACHINE_ARM64: u16 = 0xAA64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Dev,
    Release,
}
impl FromStr for Profile {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "dev" => Ok(Self::Dev),
            "release" => Ok(Self::Release),
            _ => Err(format!("'{s}' is not a valid profile")),
        }
    }
}
impl Display for Profile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Dev => "dev",
            Self::Release => "release",
        };
        write!(f, "{s}")
    }
}

/// Converts `CpuArchitecture` to its corresponding target triple name.
#[must_use]
pub fn to_target_triple(cpu_arch: CpuArchitecture) -> String {
    match cpu_arch {
        CpuArchitecture::Amd64 => X86_64_TARGET_TRIPLE_NAME.to_string(),
        CpuArchitecture::Arm64 => AARCH64_TARGET_TRIPLE_NAME.to_string(),
    }
}

pub struct BuildActionParams<'a> {
    pub working_dir: &'a Path,
    pub profile: Option<&'a Profile>,
    pub target_arch: Option<CpuArchitecture>,
    pub sign_mode: SignMode,
    pub inf2cat_args: Option<Vec<String>>,
    pub stampinf_args: Option<Vec<String>>,
    pub infverif_args: Option<Vec<String>>,
    pub is_sample_class: bool,
    pub locked: bool,
    pub workspace: bool,
    pub target_platform: TargetPlatform,
    pub features: &'a Features,
    pub verbosity_level: clap_verbosity_flag::Verbosity,
}

/// Action that orchestrates the build and package of a driver project. Build is
/// a pre-requisite for packaging.
pub struct BuildAction<'a> {
    working_dir: PathBuf,
    profile: Option<&'a Profile>,
    target_arch: Option<CpuArchitecture>,
    sign_mode: SignMode,
    inf2cat_args: Option<Vec<String>>,
    stampinf_args: Option<Vec<String>>,
    infverif_args: Option<Vec<String>>,
    is_sample_class: bool,
    locked: bool,
    workspace: bool,
    target_platform: TargetPlatform,
    features: &'a Features,
    verbosity_level: clap_verbosity_flag::Verbosity,

    // Injected deps
    wdk_build: &'a WdkBuild,
    command_exec: &'a CommandExec,
    fs: &'a Fs,
    metadata: &'a Metadata,
}

impl<'a> BuildAction<'a> {
    /// Creates a new instance of `BuildAction`.
    ///
    /// # Arguments:
    /// * `params` - The `BuildActionParams` struct containing the parameters
    ///   for the build action
    /// * `wdk_build` - The WDK build provider instance
    /// * `command_exec` - The command execution provider instance
    /// * `fs` - The file system provider instance
    /// * `metadata` - The metadata provider instance
    ///
    /// # Returns
    /// * `Result<Self>` - A result containing either a new instance of
    ///   `BuildAction` on success, or an `anyhow::Error`.
    ///
    /// # Errors
    /// * [`anyhow::Error`] -  If `params.working_dir` is not a syntactically
    ///   valid path, e.g. it is empty
    pub fn new(
        params: &BuildActionParams<'a>,
        wdk_build: &'a WdkBuild,
        command_exec: &'a CommandExec,
        fs: &'a Fs,
        metadata: &'a Metadata,
    ) -> Result<Self> {
        // TODO: validate params
        anyhow::ensure!(
            !params.working_dir.as_os_str().is_empty(),
            "working_dir must not be empty"
        );
        Ok(Self {
            working_dir: absolute(params.working_dir)?,
            profile: params.profile,
            target_arch: params.target_arch,
            sign_mode: params.sign_mode.clone(),
            inf2cat_args: params.inf2cat_args.clone(),
            stampinf_args: params.stampinf_args.clone(),
            infverif_args: params.infverif_args.clone(),
            is_sample_class: params.is_sample_class,
            locked: params.locked,
            workspace: params.workspace,
            target_platform: params.target_platform,
            features: params.features,
            verbosity_level: params.verbosity_level,
            wdk_build,
            command_exec,
            fs,
            metadata,
        })
    }

    /// Entry point method to execute the packaging action flow.
    ///
    /// # Returns
    /// `Result<(), BuildActionError>`
    ///
    /// # Errors
    /// * `BuildActionError::PackageTaskInit` - If there is an error
    ///   initializing the package task.
    /// * `BuildActionError::PackageTask` - If there is an error during the
    ///   package task process.
    /// * `BuildActionError::CargoMetadata` - If `CARGO` is missing or empty, or
    ///   if `cargo metadata` fails to run or its output cannot be parsed.
    /// * `BuildActionError::WdkMetadataParse` - Error Parsing WDK metadata from
    ///   Cargo.toml, not a valid driver project/workspace.
    /// * `BuildActionError::WdkBuildConfig` - If there is an error setting up
    ///   Path for the tools or when failed to detect WDK build number.
    /// * `BuildActionError::Io` - Wraps all possible IO errors.
    /// * `BuildActionError::NoValidRustProjectsInTheDirectory` - If no valid
    ///   Rust projects are found in the working directory.
    /// * `BuildActionError::OneOrMoreRustProjectsFailedToBuild` - If one or
    ///   more Rust projects fail to build in an emulated workspace.
    /// * `BuildActionError::OneOrMoreWorkspaceMembersFailedToBuild` - If one or
    ///   more selected workspace members fail to package after the build.
    /// * `BuildActionError::BuildTask` - If there is an error during the build
    ///   task process.
    pub fn run(&self) -> Result<(), BuildActionError> {
        debug!(
            "Initialized build for project at: {}",
            self.working_dir.display()
        );
        let build_number = self.wdk_build.detect_wdk_build_number()?;
        debug!("WDK build number: {}", build_number);
        wdk_build::cargo_make::setup_path()?;
        debug!("PATH env variable is set with WDK bin and tools paths");

        // Standalone driver/driver workspace support
        if self.fs.exists(&self.working_dir.join("Cargo.toml")) {
            return self.build_and_package(&self.working_dir);
        }

        // Emulated workspaces support
        let dirs = self.fs.read_dir_entries(&self.working_dir)?;
        debug!(
            "Checking for valid Rust projects in the working directory: {}",
            self.working_dir.display()
        );

        let mut is_valid_dir_with_rust_projects = false;
        for entry in &dirs {
            if entry.is_dir && self.fs.exists(&entry.path.join("Cargo.toml")) {
                debug!(
                    "Found at least one valid Rust project directory: {}, continuing with the \
                     build flow",
                    entry
                        .path
                        .file_name()
                        .expect(
                            "package sub directory name ended with \"..\" which is not expected"
                        )
                        .to_string_lossy()
                );
                is_valid_dir_with_rust_projects = true;
                break;
            }
        }

        if !is_valid_dir_with_rust_projects {
            return Err(BuildActionError::NoValidRustProjectsInTheDirectory(
                self.working_dir.clone(),
            ));
        }

        info!("Building packages in {}", self.working_dir.display());

        let mut failed_atleast_one_project = false;
        for entry in dirs {
            debug!("Checking dir entry: {}", entry.path.display());
            if !entry.is_dir || !self.fs.exists(&entry.path.join("Cargo.toml")) {
                debug!("Dir entry is not a valid Rust package");
                continue;
            }

            let cargo_package_path = entry.path;
            let package_dir_name = cargo_package_path
                .file_name()
                .expect("package sub directory name ended with \"..\" which is not expected")
                .to_string_lossy();

            debug!("Building package(s) in dir {package_dir_name}");
            if let Err(e) = self.build_and_package(&cargo_package_path) {
                failed_atleast_one_project = true;
                error!(
                    "Error building project: {package_dir_name}, error: {:?}",
                    anyhow::Error::new(e)
                );
            }
        }

        debug!("Done building packages in {}", self.working_dir.display());
        if failed_atleast_one_project {
            return Err(BuildActionError::OneOrMoreRustProjectsFailedToBuild(
                self.working_dir.clone(),
            ));
        }

        info!(
            "Build completed successfully for packages in {}",
            self.working_dir.display()
        );
        Ok(())
    }

    // Runs the build task from the given working directory, then runs the
    // package task for each driver package.
    fn build_and_package(&self, working_dir: &Path) -> Result<(), BuildActionError> {
        let build_task = BuildTask::new(
            BuildTaskParams {
                working_dir,
                profile: self.profile,
                target_arch: self.target_arch,
                locked: self.locked,
                workspace: self.workspace,
                features: self.features,
                verbosity_level: self.verbosity_level,
            },
            self.command_exec,
        );
        let build_output = build_task.run()?;

        let Some((cargo_metadata, wdk_metadata)) = self.get_cargo_and_wdk_metadata(working_dir)?
        else {
            debug!("No WDK metadata found. Skipping package task.");
            return Ok(());
        };
        let drivers = self.drivers_to_package(&cargo_metadata)?;

        let mut packaged = HashSet::new();
        let mut failed_to_package = false;
        for built_driver in Self::driver_artifacts(build_output, &drivers) {
            let (package, artifact) = built_driver?;
            if !packaged.insert(&package.id) {
                return Err(BuildActionError::CannotDetermineDriverBinary(format!(
                    "Found more than one cdylib artifact for package {} in cargo build output.",
                    package.name
                )));
            }

            let package_name = package.name.as_str();
            info!("Packaging driver {package_name}");
            match self.package_driver(package, &artifact, &wdk_metadata) {
                Ok(()) => info!("Finished packaging driver {package_name}"),
                Err(e) => {
                    failed_to_package = true;
                    error!(
                        "Error packaging driver {package_name}: {:?}",
                        anyhow::Error::new(e)
                    );
                }
            }
        }

        for package in drivers
            .values()
            .filter(|package| !packaged.contains(&package.id))
        {
            warn!(
                "No cdylib artifact found in cargo build output for package: {}. Skipping package \
                 task.",
                package.name
            );
        }

        if failed_to_package {
            return Err(BuildActionError::OneOrMoreWorkspaceMembersFailedToBuild(
                working_dir.to_owned(),
            ));
        }

        debug!(
            "Build completed successfully for path: {}",
            working_dir.display()
        );

        Ok(())
    }

    // Runs the package task for a driver package from its cdylib artifact in
    // the cargo build output.
    fn package_driver(
        &self,
        package: &Package,
        artifact: &Artifact,
        wdk_metadata: &Wdk,
    ) -> Result<(), BuildActionError> {
        let package_name = package.name.as_str();
        let driver_binary = Self::get_driver_binary_from_artifact(artifact)?;
        let target_dir = driver_binary.parent().ok_or_else(|| {
            BuildActionError::CannotDetermineDriverBinary(format!(
                "Driver binary {} has no parent directory",
                driver_binary.display()
            ))
        })?;
        let target_arch = self.get_arch_from_pe(&driver_binary)?;
        debug!(
            "Packaging {package_name} for {target_arch} from {}",
            target_dir.display()
        );
        let package_root_path: PathBuf = package
            .manifest_path
            .parent()
            .expect("Unable to find package path from Cargo manifest path")
            .into();
        let package_root_path = absolute(&package_root_path)
            .map_err(|e| BuildActionError::NotAbsolute(package_root_path.clone(), e))?;
        PackageTask::new(
            PackageTaskParams {
                package_name,
                working_dir: &package_root_path,
                target_dir,
                target_arch: &target_arch,
                sign_mode: self.sign_mode.clone(),
                inf2cat_args: self.inf2cat_args.clone(),
                stampinf_args: self.stampinf_args.clone(),
                infverif_args: self.infverif_args.clone(),
                sample_class: self.is_sample_class,
                driver_model: wdk_metadata.driver_model.clone(),
                target_platform: self.target_platform,
            },
            self.wdk_build,
            self.command_exec,
            self.fs,
        )
        .run()?;
        Ok(())
    }

    // Reads cargo metadata and the WDK configuration from it. Returns `None` if
    // the project has no WDK configuration, i.e. there is nothing to package.
    fn get_cargo_and_wdk_metadata(
        &self,
        working_dir: &Path,
    ) -> Result<Option<(CargoMetadata, Wdk)>, BuildActionError> {
        let cargo_metadata = self.get_cargo_metadata(working_dir)?;
        match Wdk::try_from(&cargo_metadata) {
            Ok(wdk_metadata) => Ok(Some((cargo_metadata, wdk_metadata))),
            Err(TryFromCargoMetadataError::NoWdkConfigurationsDetected) => Ok(None),
            Err(e) => Err(BuildActionError::WdkMetadataParse(e)),
        }
    }

    // Returns the driver packages to package.
    fn drivers_to_package<'m>(
        &self,
        cargo_metadata: &'m CargoMetadata,
    ) -> Result<BTreeMap<&'m PackageId, &'m Package>, BuildActionError> {
        let selected_members: &[PackageId] = if self.workspace {
            &cargo_metadata.workspace_members
        } else if cargo_metadata.workspace_default_members.is_available() {
            &cargo_metadata.workspace_default_members
        } else {
            return Err(BuildActionError::CargoMetadata(anyhow::anyhow!(
                "cargo metadata did not report the workspace default members"
            )));
        };

        let mut drivers = BTreeMap::new();
        for package in cargo_metadata.workspace_packages() {
            if package.metadata.get("wdk").is_none() {
                debug!(
                    "No WDK metadata found for package: {}. Skipping package task.",
                    package.name
                );
                continue;
            }
            if !selected_members.contains(&package.id) {
                debug!(
                    "Package {} was not selected for the build. Skipping package task. To package \
                     it, run the build from its directory or pass --workspace.",
                    package.name
                );
                continue;
            }
            if !package
                .targets
                .iter()
                .any(|t| t.kind.contains(&TargetKind::CDyLib))
            {
                debug!(
                    "No cdylib target found for package: {}. Skipping package task.",
                    package.name
                );
                continue;
            }
            drivers.insert(&package.id, package);
        }
        Ok(drivers)
    }

    // Reads the cargo build output and yields the cdylib artifact of each
    // driver package to package. See
    // https://doc.rust-lang.org/cargo/reference/external-tools.html#json-messages
    // for the format of the messages.
    fn driver_artifacts<'m>(
        build_output: impl Iterator<Item = Result<Message, std::io::Error>>,
        drivers: &BTreeMap<&'m PackageId, &'m Package>,
    ) -> impl Iterator<Item = Result<(&'m Package, Artifact), BuildActionError>> {
        build_output.filter_map(move |message| match message {
            Ok(Message::CompilerArtifact(artifact))
                if artifact.target.kind.contains(&TargetKind::CDyLib)
                    && artifact.target.crate_types.contains(&CrateType::CDyLib) =>
            {
                drivers
                    .get(&artifact.package_id)
                    .map(|package| Ok((*package, artifact)))
            }
            Ok(_) => None,
            Err(e) => Some(Err(BuildActionError::CargoBuildOutputParse(e))),
        })
    }

    fn get_cargo_metadata(&self, working_dir: &Path) -> Result<CargoMetadata, BuildActionError> {
        let working_dir_path_trimmed: PathBuf = working_dir
            .to_string_lossy()
            .trim_start_matches("\\\\?\\")
            .into();
        let mut other_options: Vec<String> = Vec::new();
        if self.locked {
            other_options.push("--locked".to_string());
        }
        let cargo = cargo_path().map_err(BuildActionError::CargoMetadata)?;
        let cargo_metadata = self
            .metadata
            .get_cargo_metadata_at_path(
                Path::new(&cargo),
                &working_dir_path_trimmed,
                other_options,
                self.features,
            )
            .map_err(|error| BuildActionError::CargoMetadata(error.into()))?;
        Ok(cargo_metadata)
    }

    /// Returns the absolute path of the driver binary (DLL) in a cdylib
    /// compiler artifact from the Cargo build output.
    ///
    /// # Errors
    /// - `BuildActionError::CannotDetermineDriverBinary` - If the artifact has
    ///   no DLL, or the DLL path cannot be made absolute.
    fn get_driver_binary_from_artifact(artifact: &Artifact) -> Result<PathBuf, BuildActionError> {
        trace!(
            "Matched driver crate (name={:?}, kinds={:?}, crate_types={:?}, filenames={:?})",
            artifact.target.name,
            &artifact.target.kind,
            &artifact.target.crate_types,
            &artifact.filenames
        );
        let dll_path = artifact
            .filenames
            .iter()
            .find(|path| {
                path.extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
            })
            .map(|path| path.as_std_path())
            .ok_or_else(|| {
                BuildActionError::CannotDetermineDriverBinary(format!(
                    "Could not find driver binary (DLL) in cargo build output for {}",
                    artifact.target.name
                ))
            })?;

        if dll_path.is_absolute() {
            return Ok(dll_path.to_path_buf());
        }

        std::path::absolute(dll_path).map_err(|err| {
            BuildActionError::CannotDetermineDriverBinary(format!(
                "Cannot convert driver binary path {} to absolute path: {err}",
                dll_path.display()
            ))
        })
    }

    /// Returns the architecture of the built driver binary, read from the
    /// `Machine` field of its PE header.
    ///
    /// # Errors
    /// * [`BuildActionError::FileIo`] - If the driver binary cannot be read.
    /// * [`BuildActionError::CannotDetectTargetArch`] - If the driver binary is
    ///   not a valid PE image.
    /// * [`BuildActionError::UnsupportedArchitecture`] - If the driver binary
    ///   is built for an unsupported architecture.
    /// * [`BuildActionError::TargetArchMismatch`] - If the driver binary's
    ///   architecture differs from the requested target architecture.
    fn get_arch_from_pe(&self, driver_binary: &Path) -> Result<CpuArchitecture, BuildActionError> {
        // wdk-build's detect_cpu_architecture_in_build_script panics for any
        // target arch other than x86_64/aarch64, so such a driver never builds.
        let found = match self.get_pe_machine_value(driver_binary)? {
            IMAGE_FILE_MACHINE_AMD64 => CpuArchitecture::Amd64,
            IMAGE_FILE_MACHINE_ARM64 => CpuArchitecture::Arm64,
            machine => {
                return Err(BuildActionError::UnsupportedArchitecture(format!(
                    "{machine:#06x}"
                )));
            }
        };
        if let Some(target_arch) = self.target_arch
            && target_arch != found
        {
            return Err(BuildActionError::TargetArchMismatch {
                driver_bin_path: driver_binary.to_owned(),
                pe_arch: found,
                target_arch,
            });
        }
        Ok(found)
    }

    /// Returns the raw `Machine` field of the PE image at `driver_binary`.
    ///
    /// Reads the following four fields needed to locate and validate it: the
    /// `MZ` signature, `e_lfanew` (the offset of the PE header), the `PE\0\0`
    /// signature and `Machine`.
    /// See <https://learn.microsoft.com/windows/win32/debug/pe-format>.
    ///
    /// # Errors
    /// * [`BuildActionError::FileIo`] - If the driver binary cannot be opened
    ///   or read.
    /// * [`BuildActionError::CannotDetectTargetArch`] - If the driver binary is
    ///   too short or has an invalid DOS or PE signature.
    fn get_pe_machine_value(&self, driver_binary: &Path) -> Result<u16, BuildActionError> {
        let read = |buf: &mut [u8], offset: u64, too_short: &str| {
            self.fs
                .seek_read_exact(driver_binary, buf, offset)
                .map_err(|err| match err {
                    FileError::ReadError(_, ref e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                        BuildActionError::CannotDetectTargetArch(
                            driver_binary.to_owned(),
                            too_short.to_string(),
                        )
                    }
                    err => err.into(),
                })
        };
        let not_pe = |reason: &str| {
            Err(BuildActionError::CannotDetectTargetArch(
                driver_binary.to_owned(),
                reason.to_string(),
            ))
        };

        let mut magic = [0; 2];
        read(&mut magic, 0, "file too short for a DOS header")?;
        if &magic != b"MZ" {
            return not_pe("missing MZ signature");
        }
        let mut e_lfanew = [0; 4];
        read(&mut e_lfanew, 0x3C, "file too short for a DOS header")?;
        let pe_header = u64::from(u32::from_le_bytes(e_lfanew));
        let mut signature = [0; 4];
        read(&mut signature, pe_header, "PE header beyond end of file")?;
        if &signature != b"PE\0\0" {
            return not_pe("missing PE signature");
        }
        let mut machine = [0; 2];
        read(&mut machine, pe_header + 4, "PE header beyond end of file")?;
        Ok(u16::from_le_bytes(machine))
    }
}

/// Returns the `cargo` CLI arguments equivalent to the given
/// [`clap_cargo::Features`] selection.
#[must_use]
fn features_to_cargo_args(features: &Features) -> Vec<String> {
    let mut args = Vec::new();
    if features.all_features {
        args.push("--all-features".to_string());
    }
    if features.no_default_features {
        args.push("--no-default-features".to_string());
    }
    for feature in &features.features {
        args.push("--features".to_string());
        args.push(feature.clone());
    }
    args
}
