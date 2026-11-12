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
    fmt::{self, Display},
    io,
    path::{Path, PathBuf, absolute},
    result::Result::Ok,
    str::FromStr,
};

use anyhow::Result;
use build_task::{BuildTask, BuildTaskParams};
use cargo_metadata::{Message, Metadata as CargoMetadata, Package, TargetKind};
use clap_cargo::Features;
use error::BuildActionError;
use mockall_double::double;
use package_task::{PackageTask, PackageTaskParams};
pub use package_task::{SignMode, TargetPlatform};
use tracing::{debug, error, info};
use wdk_build::{
    CpuArchitecture,
    metadata::{TryFromCargoMetadataError, Wdk},
};

use crate::actions::cargo::cargo_path;
#[double]
use crate::providers::{exec::CommandExec, fs::Fs, metadata::Metadata, wdk_build::WdkBuild};

const X86_64_TARGET_TRIPLE_NAME: &str = "x86_64-pc-windows-msvc";
const AARCH64_TARGET_TRIPLE_NAME: &str = "aarch64-pc-windows-msvc";

/// `Machine` values in the PE COFF file header. See
/// <https://learn.microsoft.com/windows/win32/debug/pe-format#machine-types>.
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
    /// * `BuildActionError::CargoBuildOutputParse` - If the cargo build output
    ///   cannot be read.
    /// * `BuildActionError::CannotDetermineDriverBinary` - If a driver's binary
    ///   (DLL) cannot be found in the build output.
    /// * `BuildActionError::CannotDetermineTargetDir` - If the directory of a
    ///   driver's binary, where its package is created, cannot be determined.
    /// * `BuildActionError::CannotDetectTargetArch` - If a driver binary is not
    ///   a valid PE image.
    /// * `BuildActionError::UnsupportedArchitecture` - If a driver binary is
    ///   built for an unsupported architecture.
    /// * `BuildActionError::FileIo` - If a driver binary cannot be read.
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

        let cargo_metadata = self.get_cargo_metadata(working_dir)?;
        let wdk_metadata = match Wdk::try_from(&cargo_metadata) {
            Ok(wdk_metadata) => wdk_metadata,
            Err(TryFromCargoMetadataError::NoWdkConfigurationsDetected) => {
                debug!("No WDK metadata found. Skipping package task.");
                return Ok(());
            }
            Err(e) => return Err(BuildActionError::WdkMetadataParse(e)),
        };

        for candidate in Self::candidates_for_packaging(build_output, &cargo_metadata) {
            let (package, target_kind, driver_binary) = candidate?;

            let package_name = package.name.as_str();

            if !target_kind.contains(&TargetKind::CDyLib) {
                debug!(
                    "Skipping packaging of {package_name}: target kind {target_kind:?} is not \
                     cdylib"
                );
                continue;
            }

            let driver_binary = driver_binary.ok_or_else(|| {
                BuildActionError::CannotDetermineDriverBinary(anyhow::anyhow!(
                    "no DLL found for {package_name} in cargo build output"
                ))
            })?;

            let target_dir = driver_binary
                .parent()
                .ok_or_else(|| BuildActionError::CannotDetermineTargetDir(driver_binary.clone()))?;

            let target_arch = self.get_arch_from_pe(&driver_binary)?;

            let package_dir = package
                .manifest_path
                .parent()
                .expect("Unable to find package path from Cargo manifest path");
            let package_root_path = absolute(package_dir)
                .map_err(|e| BuildActionError::NotAbsolute(package_dir.into(), e))?;

            info!("Packaging driver {package_name} for {target_arch}");
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
            info!("Finished packaging driver {package_name} for {target_arch}");
        }

        debug!(
            "Build completed successfully for path: {}",
            working_dir.display()
        );

        Ok(())
    }

    /// Returns an iterator over potential driver candidates for packaging from
    /// the cargo build output and the Cargo metadata.
    fn candidates_for_packaging<'m>(
        build_output: impl Iterator<Item = io::Result<Message>> + 'm,
        cargo_metadata: &'m CargoMetadata,
    ) -> impl Iterator<
        Item = Result<(&'m Package, Vec<TargetKind>, Option<PathBuf>), BuildActionError>,
    > + 'm {
        build_output.filter_map(|message| match message {
            Ok(Message::CompilerArtifact(artifact))
                if !artifact.target.kind.contains(&TargetKind::CustomBuild) // Skip build scripts
                    && cargo_metadata
                        .workspace_members
                        .contains(&artifact.package_id)
                    && cargo_metadata[&artifact.package_id]
                        .metadata
                        .get("wdk")
                        .is_some() =>
            {
                Some(Ok((
                    &cargo_metadata[&artifact.package_id],
                    artifact.target.kind,
                    artifact
                        .filenames
                        .into_iter()
                        .find(|path| {
                            path.extension()
                                .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
                        })
                        .map(PathBuf::from),
                )))
            }
            Ok(_) => None,
            Err(e) => Some(Err(BuildActionError::CargoBuildOutputParse(e))),
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
    fn get_arch_from_pe(&self, driver_binary: &Path) -> Result<CpuArchitecture, BuildActionError> {
        match self.get_pe_machine_value(driver_binary)? {
            IMAGE_FILE_MACHINE_AMD64 => Ok(CpuArchitecture::Amd64),
            IMAGE_FILE_MACHINE_ARM64 => Ok(CpuArchitecture::Arm64),
            machine => Err(BuildActionError::UnsupportedArchitecture(format!(
                "{machine:#06x}"
            ))),
        }
    }

    /// Returns the raw `Machine` field of the PE image at `driver_binary`.
    ///
    /// Reads the 64-byte DOS header, which starts with the `MZ` signature and
    /// ends with `e_lfanew` (the offset of the PE header), then the `PE\0\0`
    /// signature followed by the COFF file header's `Machine` field.
    /// See <https://learn.microsoft.com/windows/win32/debug/pe-format>.
    ///
    /// # Errors
    /// * [`BuildActionError::FileIo`] - If the driver binary cannot be opened
    ///   or read, including when it ends before the header fields.
    /// * [`BuildActionError::CannotDetectTargetArch`] - If the driver binary
    ///   has an invalid DOS or PE signature.
    fn get_pe_machine_value(&self, driver_binary: &Path) -> Result<u16, BuildActionError> {
        let mut dos_header = [0; 0x40];
        self.fs.seek_read_exact(driver_binary, &mut dos_header, 0)?;
        let [b'M', b'Z', .., l0, l1, l2, l3] = dos_header else {
            return Err(BuildActionError::CannotDetectTargetArch(
                driver_binary.to_owned(),
                anyhow::anyhow!("missing MZ signature"),
            ));
        };
        let e_lfanew = u64::from(u32::from_le_bytes([l0, l1, l2, l3]));

        let mut pe_header = [0; 6];
        self.fs
            .seek_read_exact(driver_binary, &mut pe_header, e_lfanew)?;
        let [b'P', b'E', 0, 0, m0, m1] = pe_header else {
            return Err(BuildActionError::CannotDetectTargetArch(
                driver_binary.to_owned(),
                anyhow::anyhow!("missing PE signature"),
            ));
        };
        Ok(u16::from_le_bytes([m0, m1]))
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
