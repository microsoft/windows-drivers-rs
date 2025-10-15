// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! The [`wdk-build`][crate] crate is a library that is used within Cargo build
//! scripts to configure any build that depends on the WDK (Windows Driver Kit).
//! This is especially useful for crates that generate FFI bindings to the WDK,
//! WDK-dependent libraries, and programs built on top of the WDK (ex. Drivers).
//! This library is built to be able to accommodate different WDK releases, as
//! well strives to allow for all the configuration the WDK allows. This
//! includes being ables to select different WDF versions and different driver
//! models (WDM, KMDF, UMDF).

#![cfg_attr(nightly_toolchain, feature(assert_matches))]
use std::{
    env,
    fmt,
    path::{Path, PathBuf, absolute},
    str::FromStr,
    sync::LazyLock,
};

pub use bindgen::BuilderExt;
use metadata::TryFromCargoMetadataError;
use tracing::debug;

pub mod cargo_make;
pub mod metadata;

mod utils;

mod bindgen;

use cargo_metadata::MetadataCommand;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::utils::detect_windows_sdk_version;

/// Configuration parameters for a build dependent on the WDK
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    /// Path to root of WDK. Corresponds with `WDKContentRoot` environment
    /// variable in eWDK
    wdk_content_root: PathBuf,
    /// CPU architecture to target
    cpu_architecture: CpuArchitecture,
    /// Build configuration of driver
    pub driver_config: DriverConfig,
}

/// The driver type with its associated configuration parameters
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(
    tag = "DRIVER_TYPE",
    deny_unknown_fields,
    rename_all = "UPPERCASE",
    from = "DeserializableDriverConfig"
)]
pub enum DriverConfig {
    /// Windows Driver Model
    Wdm,
    /// Kernel Mode Driver Framework
    Kmdf(KmdfConfig),
    /// User Mode Driver Framework
    Umdf(UmdfConfig),
}

/// Private enum identical to [`DriverConfig`] but with different tag name to
/// deserialize from.
///
/// [`serde_derive`] doesn't support different tag names for serialization vs.
/// deserialization, and also doesn't support aliases for tag names, so the
/// `from` attribute is used in conjunction with this type to facilitate a
/// different tag name for deserialization.
///
/// Relevant Github Issues:
/// * <https://github.com/serde-rs/serde/issues/2776>
/// * <https://github.com/serde-rs/serde/issues/2324>
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "driver-type", deny_unknown_fields, rename_all = "UPPERCASE")]
enum DeserializableDriverConfig {
    Wdm,
    Kmdf(KmdfConfig),
    Umdf(UmdfConfig),
}

/// The CPU architecture that's configured to be compiled for
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CpuArchitecture {
    /// AMD64 CPU architecture. Also known as x64 or x86-64.
    Amd64,
    /// ARM64 CPU architecture. Also known as aarch64.
    Arm64,
}

impl FromStr for CpuArchitecture {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "amd64" => Ok(Self::Amd64),
            "arm64" => Ok(Self::Arm64),
            _ => Err(format!("'{s}' is not a valid target architecture")),
        }
    }
}

impl fmt::Display for CpuArchitecture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Amd64 => "amd64",
            Self::Arm64 => "arm64",
        };
        write!(f, "{s}")
    }
}

/// The configuration parameters for KMDF drivers
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(
    deny_unknown_fields,
    rename_all(serialize = "SCREAMING_SNAKE_CASE", deserialize = "kebab-case")
)]
pub struct KmdfConfig {
    /// Major KMDF Version
    pub kmdf_version_major: u8,
    /// Minor KMDF Version (Target Version)
    pub target_kmdf_version_minor: u8,
    /// Minor KMDF Version (Minimum Required)
    pub minimum_kmdf_version_minor: Option<u8>,
}

/// The configuration parameters for UMDF drivers
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(
    deny_unknown_fields,
    rename_all(serialize = "SCREAMING_SNAKE_CASE", deserialize = "kebab-case")
)]
pub struct UmdfConfig {
    /// Major UMDF Version
    pub umdf_version_major: u8,
    /// Minor UMDF Version (Target Version)
    pub target_umdf_version_minor: u8,
    /// Minor UMDF Version (Minimum Required)
    pub minimum_umdf_version_minor: Option<u8>,
}

/// Metadata providing additional context for [`std::io::Error`] failures
///
/// This enum provides structured information about the file system paths
/// or operations that led to an I/O error. It can represent either single
/// path operations or operations involving both source and destination paths.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum IoErrorMetadata {
    /// Path related to [`std::io::Error`] failure
    #[error(r#"failed to perform an IO operation on "{path}""#)]
    Path {
        /// The file system path where the I/O error occurred
        path: PathBuf,
    },
    /// Source and destination paths related to [`std::io::Error`] failure.
    ///
    /// This can be provided for APIs like [`std::fs::copy`] which have both a
    /// `from` and `to` path.
    #[error(r#"failed to perform an IO operation from "{from_path}" to "{to_path}""#)]
    SrcDestPaths {
        /// The source path in a copy or move operation that failed
        from_path: PathBuf,
        /// The destination path in a copy or move operation that failed
        to_path: PathBuf,
    },
}

/// Dedicated error type for I/O operations with extra metadata context
///
/// This error type wraps [`std::io::Error`] with additional [`IoErrorMetadata`]
/// to provide better context about which file system paths or operations
/// failed. It can be used directly by functions that only perform I/O
/// operations, and automatically converts to [`ConfigError`] when needed.
#[derive(Debug, Error)]
#[error("{metadata}")]
pub struct IoError {
    /// Extra metadata related to the error
    metadata: IoErrorMetadata,
    /// [`std::io::Error`] that caused the operation to fail
    #[source]
    source: std::io::Error,
}

impl IoError {
    /// Creates a new `IoError` with a single path and source error.
    pub fn with_path(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self {
            metadata: IoErrorMetadata::Path { path: path.into() },
            source,
        }
    }

    /// Creates a new `IoError` for operations involving a source and
    /// destination path.
    pub fn with_src_dest_paths(
        from_path: impl Into<PathBuf>,
        to_path: impl Into<PathBuf>,
        source: std::io::Error,
    ) -> Self {
        Self {
            metadata: IoErrorMetadata::SrcDestPaths {
                from_path: from_path.into(),
                to_path: to_path.into(),
            },
            source,
        }
    }
}

/// Errors that could result from configuring a build via [`wdk_build`][crate]
#[derive(Debug, Error)]
pub enum ConfigError {
    /// Error returned when an [`std::io`] operation fails
    #[error(transparent)]
    IoError(#[from] IoError),

    /// Error returned when an expected directory does not exist
    #[error("cannot find directory: {directory}")]
    DirectoryNotFound {
        /// Path of directory that was not found
        directory: String,
    },

    /// Error returned when a package is not found in Cargo metadata
    #[error("cannot find wdk-build package in Cargo metadata")]
    WdkBuildPackageNotFoundInCargoMetadata,

    /// Error returned Cargo manifest contains an unsupported edition
    #[error("Cargo manifest contains unsupported Rust edition: {edition}")]
    UnsupportedRustEdition {
        /// Edition of the Cargo manifest that was not supported
        edition: String,
    },

    /// Error returned when `bindgen` does not support `rust-version` in Cargo
    /// manifest
    #[error("Rust version {msrv} not supported by Bindgen: {reason}")]
    MsrvNotSupportedByBindgen {
        /// MSRV that was not supported by Bindgen
        msrv: String,
        /// Reason why the MSRV was not supported
        reason: String,
    },

    /// Error returned when `semver` parsing of the Rust version fails
    #[error("failed to parse rust-version in manifest")]
    RustVersionParseError {
        /// [`semver::Error`] that caused parsing the Rust version to fail
        #[source]
        error_source: semver::Error,
    },

    /// Error returned when a [`metadata::Wdk`] fails to be parsed from a Cargo
    /// Manifest
    #[error(transparent)]
    TryFromCargoMetadataError(#[from] metadata::TryFromCargoMetadataError),

    /// Error returned when a [`Config`] fails to be serialized
    #[error(
        "WDKContentRoot should be able to be detected. Ensure that the WDK is installed, or that \
         the environment setup scripts in the eWDK have been run."
    )]
    WdkContentRootDetectionError,

    /// Error returned when the WDK version string does not match the expected
    /// format
    #[error("the WDK version string provided ({version}) was not in a valid format")]
    WdkVersionStringFormatError {
        /// The incorrect WDK version string.
        version: String,
    },

    /// Error returned when `cargo_metadata` execution or parsing fails
    #[error(transparent)]
    CargoMetadataError(#[from] cargo_metadata::Error),

    /// Error returned when no wdk-build package is detected
    #[error("no wdk-build package is detected")]
    NoWdkBuildCrateDetected,

    /// Error returned when multiple versions of the wdk-build package are
    /// detected
    #[error(
        "multiple versions of the wdk-build package are detected, but only one version is \
         allowed: {package_ids:#?}"
    )]
    MultipleWdkBuildCratesDetected {
        /// package ids of the wdk-build crates detected
        package_ids: Vec<cargo_metadata::PackageId>,
    },

    /// Error returned when the c runtime is not configured to be statically
    /// linked
    #[error(
        "the C runtime is not properly configured to be statically linked. This is required for building WDK drivers. The recommended solution is to add the following snippet to a \
        `.cargo/config.toml` file:
[build]
rustflags = [\"-C\", \"target-feature=+crt-static\"]

\
        See https://doc.rust-lang.org/reference/linkage.html#static-and-dynamic-c-runtimes for more ways \
        to enable static crt linkage"
    )]
    StaticCrtNotEnabled,

    /// Error returned when [`metadata::ser::Serializer`] fails to serialize the
    /// [`metadata::Wdk`]
    #[error(transparent)]
    SerdeError(#[from] metadata::Error),
}

/// Subset of APIs in the Windows Driver Kit
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ApiSubset {
    /// API subset typically required for all Windows drivers
    Base,
    /// API subset required for WDF (Windows Driver Framework) drivers: <https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/_wdf/>
    Wdf,
    /// API subset for GPIO (General Purpose Input/Output) drivers: <https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/_gpio/>
    Gpio,
    /// API subset for HID (Human Interface Device) drivers: <https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/_hid/>
    Hid,
    /// API subset for Parallel Ports drivers: <https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/_parports/>
    ParallelPorts,
    /// API subset for SPB (Serial Peripheral Bus) drivers: <https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/_spb/>
    Spb,
    /// API subset for Storage drivers: <https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/_storage/>
    Storage,
    /// API subset for USB (Universal Serial Bus) drivers: <https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/_usbref/>
    Usb,
}

#[derive(Debug, Error, PartialEq, Eq)]
/// Error when parsing a [`TwoPartVersion`].
pub enum TwoPartVersionError {
    /// Supplied string didn't match MAJOR.MINOR format.
    #[error("Invalid version: {0}. Expected format is 'major.minor'")]
    InvalidFormat(String),
    /// A numeric component failed to parse (component name, original string).
    #[error("Error parsing {0} version to 'u32'. Version string: {1}")]
    ParseError(String, String),
}

/// Version of the form MAJOR.MINOR (both u32). Accepts leading zeros.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct TwoPartVersion(pub u32, pub u32);

/// Parses a string of the form `MAJOR.MINOR` into a [`TwoPartVersion`].
///
/// # Expected format
/// - The input string must contain exactly one dot (`.`) separating two
///   non-empty components.
/// - Both components must be valid unsigned 32-bit integers (`u32`). Leading
///   zeros are accepted.
///
/// # Errors
/// - Returns [`TwoPartVersionError::InvalidFormat`] if the string does not
///   contain exactly one dot or has empty components.
/// - Returns [`TwoPartVersionError::ParseError`] if either component cannot be
///   parsed as a `u32`.
impl FromStr for TwoPartVersion {
    type Err = TwoPartVersionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let dot_count = s.matches('.').count();
        if dot_count != 1 {
            return Err(TwoPartVersionError::InvalidFormat(s.to_string()));
        }
        let (major_str, minor_str) = s
            .split_once('.')
            .ok_or_else(|| TwoPartVersionError::InvalidFormat(s.to_string()))?;
        if major_str.is_empty() || minor_str.is_empty() {
            return Err(TwoPartVersionError::InvalidFormat(s.to_string()));
        }
        let major = major_str
            .parse::<u32>()
            .map_err(|_| TwoPartVersionError::ParseError("major".to_string(), s.to_string()))?;
        let minor = minor_str
            .parse::<u32>()
            .map_err(|_| TwoPartVersionError::ParseError("minor".to_string(), s.to_string()))?;
        Ok(Self(major, minor))
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            wdk_content_root: utils::detect_wdk_content_root().expect(
                "WDKContentRoot should be able to be detected. Ensure that the WDK is installed, \
                 or that the environment setup scripts in the eWDK have been run.",
            ),
            driver_config: DriverConfig::Wdm,
            cpu_architecture: utils::detect_cpu_architecture_in_build_script(),
        }
    }
}

impl Config {
    /// Create a new [`Config`] with default values
    #[must_use]
    #[tracing::instrument(level = "debug")]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a [`Config`] from parsing the top-level Cargo manifest into a
    /// [`metadata::Wdk`], and using it to populate the [`Config`]. It also
    /// emits `cargo::rerun-if-changed` directives for any files that are
    /// used to create the [`Config`].
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// * the execution of `cargo metadata` fails
    /// * the parsing of [`metadata::Wdk`] from any of the Cargo manifests fail
    /// * multiple conflicting [`metadata::Wdk`] configurations are detected
    /// * no [`metadata::Wdk`] configurations are detected
    ///
    /// # Panics
    ///
    /// Panics if the resolved top-level Cargo manifest path is not valid UTF-8
    #[tracing::instrument(level = "debug")]
    pub fn from_env_auto() -> Result<Self, ConfigError> {
        let top_level_cargo_manifest_path = find_top_level_cargo_manifest();
        debug!(
            "Top level Cargo manifest path: {:?}",
            top_level_cargo_manifest_path
        );

        let cwd = top_level_cargo_manifest_path
            .parent()
            .expect("Cargo manifest should have a valid parent directory");

        let cargo_metadata = MetadataCommand::new()
            // Run `cargo_metadata` in the same working directory as the top level manifest in order
            // to respect `config.toml` overrides
            .current_dir(cwd)
            // top-level manifest path must be used in order for metadata from the top-level crates
            // to be discovered
            .manifest_path(&top_level_cargo_manifest_path)
            .exec()?;
        let wdk_metadata = metadata::Wdk::try_from(&cargo_metadata)?;

        // Force rebuilds if any of the manifest files change (ex. if wdk metadata
        // section is modified)
        for manifest_path in metadata::iter_manifest_paths(cargo_metadata)
            .into_iter()
            .chain(std::iter::once(
                top_level_cargo_manifest_path
                    .try_into()
                    .expect("Path to Cargo manifests should always be valid UTF8"),
            ))
        {
            println!("cargo:rerun-if-changed={manifest_path}");
        }

        Ok(Self {
            driver_config: wdk_metadata.driver_model,
            ..Default::default()
        })
    }

    /// Emit `cargo::rustc-check-cfg` directives corresponding to all the
    /// possible `rustc-cfg` settings `wdk_build` could emit
    ///
    /// This is useful in situations where a library may not have a valid WDK
    /// config available during build. This function is not needed if the build
    /// was already configured via [`configure_wdk_binary_build`],
    /// [`configure_wdk_library_build`], or
    /// [`configure_wdk_library_build_and_then`]
    #[tracing::instrument(level = "debug")]
    pub fn emit_check_cfg_settings() {
        for (cfg_key, allowed_values) in EXPORTED_CFG_SETTINGS.iter() {
            let allowed_cfg_value_string =
                allowed_values.iter().fold(String::new(), |mut acc, value| {
                    const OPENING_QUOTE: char = '"';
                    const CLOSING_QUOTE_AND_COMMA: &str = r#"","#;

                    acc.reserve(
                        value.len() + OPENING_QUOTE.len_utf8() + CLOSING_QUOTE_AND_COMMA.len(),
                    );
                    acc.push(OPENING_QUOTE);
                    acc.push_str(value);
                    acc.push_str(CLOSING_QUOTE_AND_COMMA);
                    acc
                });

            let cfg_key = {
                // Replace `metadata::ser::KEY_NAME_SEPARATOR` with `__` so that `cfg_key` is a
                // valid rust identifier name
                let mut k = cfg_key.replace(metadata::ser::KEY_NAME_SEPARATOR, "__");
                // convention is that cfg keys are lowercase
                k.make_ascii_lowercase();
                k
            };

            // Emit allowed cfg values
            println!("cargo::rustc-check-cfg=cfg({cfg_key}, values({allowed_cfg_value_string}))");
        }
    }

    /// Expose `cfg` settings based on this [`Config`] to enable conditional
    /// compilation. This emits specially formatted prints to Cargo based on
    /// this [`Config`].
    #[tracing::instrument(level = "trace")]
    fn emit_cfg_settings(&self) -> Result<(), ConfigError> {
        Self::emit_check_cfg_settings();

        let serialized_wdk_metadata_map =
            metadata::to_map::<std::collections::BTreeMap<_, _>>(&metadata::Wdk {
                driver_model: self.driver_config.clone(),
            })?;

        for cfg_key in EXPORTED_CFG_SETTINGS.iter().map(|(key, _)| *key) {
            let cfg_value = &serialized_wdk_metadata_map[cfg_key];

            let cfg_key = {
                // Replace `metadata::ser::KEY_NAME_SEPARATOR` with `__` so that `cfg_key` is a
                // valid rust identifier name
                let mut k = cfg_key.replace(metadata::ser::KEY_NAME_SEPARATOR, "__");
                // convention is that cfg keys are lowercase
                k.make_ascii_lowercase();
                k
            };

            // Emit cfg
            println!(r#"cargo::rustc-cfg={cfg_key}="{cfg_value}""#);
        }

        Ok(())
    }

    /// Return header include paths required to build and link based off of the
    /// configuration of `Config`
    ///
    /// # Errors
    ///
    /// This function will return an error if any of the required paths do not
    /// exist.
    #[tracing::instrument(level = "debug")]
    pub fn include_paths(&self) -> Result<impl Iterator<Item = PathBuf>, ConfigError> {
        let mut include_paths = vec![];
        let sdk_version = detect_windows_sdk_version(&self.wdk_content_root)?;
        let include_directory = self.wdk_content_root.join("Include");

        // Add windows sdk include paths
        // Based off of logic from WindowsDriver.KernelMode.props &
        // WindowsDriver.UserMode.props in NI(22H2) WDK
        let windows_sdk_include_path = include_directory.join(sdk_version);

        let crt_include_path = windows_sdk_include_path.join("km/crt");
        Self::validate_and_add_folder_path(&mut include_paths, &crt_include_path)?;

        let km_or_um_include_path = windows_sdk_include_path.join(match self.driver_config {
            DriverConfig::Wdm | DriverConfig::Kmdf(_) => "km",
            DriverConfig::Umdf(_) => "um",
        });
        Self::validate_and_add_folder_path(&mut include_paths, &km_or_um_include_path)?;

        let kit_shared_include_path = windows_sdk_include_path.join("shared");
        Self::validate_and_add_folder_path(&mut include_paths, &kit_shared_include_path)?;

        // Add other driver type-specific include paths
        match &self.driver_config {
            DriverConfig::Wdm => {}
            DriverConfig::Kmdf(kmdf_config) => {
                let kmdf_include_path = include_directory.join(format!(
                    "wdf/kmdf/{}.{}",
                    kmdf_config.kmdf_version_major, kmdf_config.target_kmdf_version_minor
                ));
                Self::validate_and_add_folder_path(&mut include_paths, &kmdf_include_path)?;

                // `ufxclient.h` relies on `ufxbase.h` being on the headers search path. The WDK
                // normally does not automatically include this search path, but it is required
                // here so that the headers can be processed successfully.
                let ufx_include_path = km_or_um_include_path.join("ufx/1.1");
                Self::validate_and_add_folder_path(&mut include_paths, &ufx_include_path)?;
            }
            DriverConfig::Umdf(umdf_config) => {
                let umdf_include_path = include_directory.join(format!(
                    "wdf/umdf/{}.{}",
                    umdf_config.umdf_version_major, umdf_config.target_umdf_version_minor
                ));
                Self::validate_and_add_folder_path(&mut include_paths, &umdf_include_path)?;
            }
        }

        Ok(include_paths.into_iter())
    }

    /// Validate that a path refers to an existing directory and push its
    /// canonical absolute form into the provided collection.
    ///
    /// This helper is used for both header include directories and library
    /// directories. It normalizes paths before insertion.
    fn validate_and_add_folder_path(
        include_paths: &mut Vec<PathBuf>,
        path: &Path,
    ) -> Result<(), ConfigError> {
        // Include paths should be directories
        if !path.is_dir() {
            return Err(ConfigError::DirectoryNotFound {
                directory: path.to_string_lossy().into(),
            });
        }

        let absolute_path = absolute(path).map_err(|source| IoError::with_path(path, source))?;

        include_paths.push(absolute_path);
        Ok(())
    }

    /// Return library include paths required to build and link based off of
    /// the configuration of [`Config`].
    ///
    /// For UMDF drivers, this assumes a "Windows-Driver" Target Platform.
    ///
    /// # Errors
    ///
    /// This function will return an error if any of the required paths do not
    /// exist.
    #[tracing::instrument(level = "debug")]
    pub fn library_paths(&self) -> Result<impl Iterator<Item = PathBuf>, ConfigError> {
        let mut library_paths = vec![];
        let sdk_version = detect_windows_sdk_version(&self.wdk_content_root)?;

        // Add windows sdk library paths
        // Based off of logic from WindowsDriver.KernelMode.props &
        // WindowsDriver.UserMode.props in NI(22H2) WDK
        let windows_sdk_library_path = self.sdk_library_path(sdk_version)?;
        Self::validate_and_add_folder_path(&mut library_paths, &windows_sdk_library_path)?;

        // Add other driver type-specific library paths
        let library_directory = self.wdk_content_root.join("Lib");
        match &self.driver_config {
            DriverConfig::Wdm => (),
            DriverConfig::Kmdf(kmdf_config) => {
                let kmdf_library_path = library_directory.join(format!(
                    "wdf/kmdf/{}/{}.{}",
                    self.cpu_architecture.as_windows_str(),
                    kmdf_config.kmdf_version_major,
                    kmdf_config.target_kmdf_version_minor
                ));
                Self::validate_and_add_folder_path(&mut library_paths, &kmdf_library_path)?;
            }
            DriverConfig::Umdf(umdf_config) => {
                let umdf_library_path = library_directory.join(format!(
                    "wdf/umdf/{}/{}.{}",
                    self.cpu_architecture.as_windows_str(),
                    umdf_config.umdf_version_major,
                    umdf_config.target_umdf_version_minor,
                ));
                Self::validate_and_add_folder_path(&mut library_paths, &umdf_library_path)?;
            }
        }

        // Reverse order of library paths so that paths pushed later into the vec take
        // precedence
        library_paths.reverse();
        Ok(library_paths.into_iter())
    }

    /// Return an iterator of strings that represent compiler definitions
    /// derived from the `Config`
    #[tracing::instrument(level = "debug")]
    pub fn preprocessor_definitions(&self) -> impl Iterator<Item = (String, Option<String>)> {
        // _WIN32_WINNT=$(WIN32_WINNT_VERSION);
        // WINVER=$(WINVER_VERSION);
        // WINNT=1;
        // NTDDI_VERSION=$(NTDDI_VERSION);

        // Definition sourced from: Program Files\Windows
        // Kits\10\build\10.0.26040.0\WindowsDriver.Shared.Props
        // vec![ //from driver.os.props //D:\EWDK\rsprerelease\content\Program
        // Files\Windows Kits\10\build\10.0.26040.0\WindowsDriver.OS.Props
        // ("_WIN32_WINNT", Some()),CURRENT_WIN32_WINNT_VERSION
        // ("WINVER", Some()), = CURRENT_WIN32_WINNT_VERSION
        // ("WINNT", Some(1)),1
        // ("NTDDI_VERSION", Some()),CURRENT_NTDDI_VERSION
        // ]
        // .into_iter()
        // .map(|(key, value)| (key.to_string(), value.map(|v| v.to_string())))
        match self.cpu_architecture {
            // Definitions sourced from `Program Files\Windows
            // Kits\10\build\10.0.22621.0\WindowsDriver.x64.props`
            CpuArchitecture::Amd64 => {
                vec![("_WIN64", None), ("_AMD64_", None), ("AMD64", None)]
            }
            // Definitions sourced from `Program Files\Windows
            // Kits\10\build\10.0.22621.0\WindowsDriver.arm64.props`
            CpuArchitecture::Arm64 => {
                vec![
                    ("_ARM64_", None),
                    ("ARM64", None),
                    ("_USE_DECLSPECS_FOR_SAL", Some(1)),
                    ("STD_CALL", None),
                ]
            }
        }
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.map(|v| v.to_string())))
        .chain(
            match self.driver_config {
                DriverConfig::Wdm => {
                    vec![
                        ("_KERNEL_MODE", None), // Normally defined by msvc via /kernel flag
                    ]
                }
                DriverConfig::Kmdf(kmdf_config) => {
                    let mut kmdf_definitions = vec![
                        ("_KERNEL_MODE", None), // Normally defined by msvc via /kernel flag
                        ("KMDF_VERSION_MAJOR", Some(kmdf_config.kmdf_version_major)),
                        (
                            "KMDF_VERSION_MINOR",
                            Some(kmdf_config.target_kmdf_version_minor),
                        ),
                    ];

                    if let Some(minimum_minor_version) = kmdf_config.minimum_kmdf_version_minor {
                        kmdf_definitions
                            .push(("KMDF_MINIMUM_VERSION_REQUIRED", Some(minimum_minor_version)));
                    }
                    kmdf_definitions
                }
                DriverConfig::Umdf(umdf_config) => {
                    let mut umdf_definitions = vec![
                        ("UMDF_VERSION_MAJOR", Some(umdf_config.umdf_version_major)),
                        (
                            "UMDF_VERSION_MINOR",
                            Some(umdf_config.target_umdf_version_minor),
                        ),
                        // Definition sourced from: Program Files\Windows
                        // Kits\10\build\10.0.26040.0\Windows.UserMode.props
                        ("_ATL_NO_WIN_SUPPORT", None),
                        // Definition sourced from: Program Files\Windows
                        // Kits\10\build\10.0.26040.0\WindowsDriver.Shared.Props
                        ("WIN32_LEAN_AND_MEAN", Some(1)),
                    ];

                    if let Some(minimum_minor_version) = umdf_config.minimum_umdf_version_minor {
                        umdf_definitions
                            .push(("UMDF_MINIMUM_VERSION_REQUIRED", Some(minimum_minor_version)));
                    }

                    if umdf_config.umdf_version_major >= 2 {
                        umdf_definitions.push(("UMDF_USING_NTSTATUS", None));
                        umdf_definitions.push(("_UNICODE", None));
                        umdf_definitions.push(("UNICODE", None));
                    }

                    umdf_definitions
                }
            }
            .into_iter()
            .map(|(key, value)| (key.to_string(), value.map(|v| v.to_string()))),
        )
    }

    /// Return an iterator of strings that represent compiler flags (i.e.
    /// warnings, settings, etc.) used by bindgen to parse WDK headers
    #[tracing::instrument(level = "debug")]
    pub fn wdk_bindgen_compiler_flags() -> impl Iterator<Item = String> {
        vec![
            // Enable Microsoft C/C++ extensions and compatibility options (https://clang.llvm.org/docs/UsersManual.html#microsoft-extensions)
            "-fms-compatibility",
            "-fms-extensions",
            "-fdelayed-template-parsing",
            // Windows SDK & DDK have non-portable paths (ex. #include "DriverSpecs.h" but the
            // file is actually driverspecs.h)
            "--warn-=no-nonportable-include-path",
            // Windows SDK & DDK use pshpack and poppack headers to change packing
            "--warn-=no-pragma-pack",
            "--warn-=no-ignored-attributes",
            "--warn-=no-ignored-pragma-intrinsic",
            "--warn-=no-visibility",
            "--warn-=no-switch",
            "--warn-=no-comment",
            // Don't warn for deprecated declarations. Deprecated items should be explicitly
            // blocklisted (i.e. by the bindgen invocation). Any non-blocklisted function
            // definitions will trigger a -WDeprecated warning
            "--warn-=no-deprecated-declarations",
            // Windows SDK & DDK contain unnecessary token pasting (ex. &##_variable: `&` and
            // `_variable` are separate tokens already, and don't need `##` to concatenate
            // them)
            "--warn-=no-invalid-token-paste",
            // Windows SDK & DDK headers rely on Microsoft extensions to C/C++
            "--warn-=no-microsoft",
        ]
        .into_iter()
        .map(ToString::to_string)
    }

    /// Returns a [`String`] iterator over all the headers for a given
    /// [`ApiSubset`]
    ///
    /// The iterator considers both the [`ApiSubset`] and the [`Config`] to
    /// determine which headers to yield
    ///
    /// # Errors
    /// [`ConfigError`] - if the headers for the given [`ApiSubset`] could not
    /// be determined
    #[tracing::instrument(level = "debug")]
    pub fn headers(
        &self,
        api_subset: ApiSubset,
    ) -> Result<impl Iterator<Item = String>, ConfigError> {
        let headers = match api_subset {
            ApiSubset::Base => self.base_headers(),
            ApiSubset::Wdf => self.wdf_headers(),
            ApiSubset::Gpio => self.gpio_headers(),
            ApiSubset::Hid => self.hid_headers(),
            ApiSubset::ParallelPorts => self.parallel_ports_headers(),
            ApiSubset::Spb => self.spb_headers(),
            ApiSubset::Storage => self.storage_headers(),
            ApiSubset::Usb => return self.usb_headers().map(std::iter::IntoIterator::into_iter),
        };
        Ok(headers
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
            .into_iter())
    }

    #[tracing::instrument(level = "trace")]
    fn base_headers(&self) -> Vec<&'static str> {
        match &self.driver_config {
            DriverConfig::Wdm | DriverConfig::Kmdf(_) => {
                vec!["ntifs.h", "ntddk.h", "ntstrsafe.h"]
            }
            DriverConfig::Umdf(_) => {
                vec!["windows.h"]
            }
        }
    }

    #[tracing::instrument(level = "trace")]
    fn wdf_headers(&self) -> Vec<&'static str> {
        if matches!(
            self.driver_config,
            DriverConfig::Kmdf(_) | DriverConfig::Umdf(_)
        ) {
            vec!["wdf.h"]
        } else {
            vec![]
        }
    }

    #[tracing::instrument(level = "trace")]
    fn gpio_headers(&self) -> Vec<&'static str> {
        let mut headers = vec!["gpio.h"];
        if matches!(self.driver_config, DriverConfig::Kmdf(_)) {
            headers.extend(["gpioclx.h"]);
        }
        headers
    }

    #[tracing::instrument(level = "trace")]
    fn hid_headers(&self) -> Vec<&'static str> {
        let mut headers = vec!["hidclass.h", "hidsdi.h", "hidpi.h", "vhf.h"];
        if matches!(
            self.driver_config,
            DriverConfig::Wdm | DriverConfig::Kmdf(_)
        ) {
            headers.extend(["hidpddi.h", "hidport.h", "kbdmou.h", "ntdd8042.h"]);
        }

        if matches!(self.driver_config, DriverConfig::Kmdf(_)) {
            headers.extend(["HidSpiCx/1.0/hidspicx.h"]);
        }
        headers
    }

    #[tracing::instrument(level = "trace")]
    fn parallel_ports_headers(&self) -> Vec<&'static str> {
        let mut headers = vec!["ntddpar.h", "ntddser.h"];
        if matches!(
            self.driver_config,
            DriverConfig::Wdm | DriverConfig::Kmdf(_)
        ) {
            headers.extend(["parallel.h"]);
        }
        headers
    }

    #[tracing::instrument(level = "trace")]
    fn spb_headers(&self) -> Vec<&'static str> {
        let mut headers = vec!["spb.h", "reshub.h"];
        if matches!(
            self.driver_config,
            DriverConfig::Wdm | DriverConfig::Kmdf(_)
        ) {
            headers.extend(["pwmutil.h"]);
        }
        if matches!(self.driver_config, DriverConfig::Kmdf(_)) {
            headers.extend(["spb/1.1/spbcx.h"]);
        }
        headers
    }

    #[tracing::instrument(level = "trace")]
    fn storage_headers(&self) -> Vec<&'static str> {
        let mut headers = vec![
            "ehstorioctl.h",
            "ntddcdrm.h",
            "ntddcdvd.h",
            "ntdddisk.h",
            "ntddmmc.h",
            "ntddscsi.h",
            "ntddstor.h",
            "ntddtape.h",
            "ntddvol.h",
            "ufs.h",
        ];
        if matches!(
            self.driver_config,
            DriverConfig::Wdm | DriverConfig::Kmdf(_)
        ) {
            headers.extend([
                "mountdev.h",
                "mountmgr.h",
                "ntddchgr.h",
                "ntdddump.h",
                "storduid.h",
                "storport.h",
            ]);
        }
        if matches!(self.driver_config, DriverConfig::Kmdf(_)) {
            headers.extend(["ehstorbandmgmt.h"]);
        }
        headers
    }

    #[tracing::instrument(level = "trace")]
    fn usb_headers(&self) -> Result<Vec<String>, ConfigError> {
        let mut headers = Vec::new();
        headers.extend(
            [
                "usb.h",
                "usbfnbase.h",
                "usbioctl.h",
                "usbspec.h",
                "Usbpmapi.h",
            ]
            .iter()
            .map(ToString::to_string),
        );
        if matches!(
            self.driver_config,
            DriverConfig::Wdm | DriverConfig::Kmdf(_)
        ) {
            headers.extend(
                ["usbbusif.h", "usbdlib.h", "usbfnattach.h", "usbfnioctl.h"]
                    .iter()
                    .map(ToString::to_string),
            );
        }

        if matches!(
            self.driver_config,
            DriverConfig::Kmdf(_) | DriverConfig::Umdf(_)
        ) {
            headers.extend(["wdfusb.h".to_string()]);
        }

        if matches!(self.driver_config, DriverConfig::Kmdf(_)) {
            headers.extend(
                [
                    "ucm/1.0/UcmCx.h",
                    "UcmTcpci/1.0/UcmTcpciCx.h",
                    "UcmUcsi/1.0/UcmucsiCx.h",
                    "ude/1.1/UdeCx.h",
                    "ufx/1.1/ufxbase.h",
                    "ufxproprietarycharger.h",
                    "urs/1.0/UrsCx.h",
                ]
                .iter()
                .map(ToString::to_string),
            );

            let latest_ucx_header_path = self.ucx_header()?;
            headers.push(latest_ucx_header_path);

            if Self::should_include_ufxclient() {
                headers.push("ufx/1.1/ufxclient.h".to_string());
            }
        }
        Ok(headers)
    }

    /// Determines whether to include the ufxclient.h header based on the Clang
    /// version used by bindgen.
    ///
    /// The ufxclient.h header contains FORCEINLINE annotations that are invalid
    /// according to the C standard. While MSVC silently ignores these in C
    /// mode, older versions of Clang (pre-20.0) will error, even with MSVC
    /// compatibility enabled.
    ///
    /// This function checks if the current Clang version is 20.0 or newer,
    /// where the issue was fixed. See
    /// <https://github.com/llvm/llvm-project/issues/124869> for details.
    #[tracing::instrument(level = "trace")]
    fn should_include_ufxclient() -> bool {
        const MINIMUM_CLANG_MAJOR_VERSION_WITH_INVALID_INLINE_FIX: u32 = 20;

        let clang_version = ::bindgen::clang_version();
        match clang_version.parsed {
            Some((major, _minor))
                if major >= MINIMUM_CLANG_MAJOR_VERSION_WITH_INVALID_INLINE_FIX =>
            {
                true
            }
            Some(_) => {
                tracing::info!(
                    "Skipping ufxclient.h due to FORCEINLINE bug in {}",
                    clang_version.full
                );
                false
            }
            None => {
                tracing::warn!(
                    "Failed to parse semver Major and Minor components from full Clang version \
                     string: {}",
                    clang_version.full
                );
                false
            }
        }
    }

    /// Returns a [`String`] containing the contents of a header file designed
    /// for [`bindgen`](https://docs.rs/bindgen) to process
    ///
    /// The contents contain `#include`'ed headers based off the [`ApiSubset`]
    /// and [`Config`], as well as any additional definitions required for the
    /// headers to be processed successfully.
    ///
    /// # Errors
    /// [`ConfigError`] - if the headers for a [`ApiSubset`] could not be
    /// determined
    #[tracing::instrument(level = "debug")]
    pub fn bindgen_header_contents(
        &self,
        api_subsets: impl IntoIterator<Item = ApiSubset> + fmt::Debug,
    ) -> Result<String, ConfigError> {
        Ok(api_subsets
            .into_iter()
            .map(|api_subset| self.headers(api_subset))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flat_map(|iter| iter.map(|header| format!("#include \"{header}\"\n")))
            .collect())
    }

    /// Configure a Cargo build of a library that depends on the WDK. This
    /// emits specially formatted prints to Cargo based on this [`Config`].
    ///
    /// # Errors
    ///
    /// This function will return an error if the [`Config`] fails to be
    /// serialized
    #[tracing::instrument(level = "debug")]
    pub fn configure_library_build(&self) -> Result<(), ConfigError> {
        self.emit_cfg_settings()
    }

    /// Compute the name of the `WdfFunctions` symbol used for WDF function
    /// dispatching based off of the [`Config`]. Returns `None` if the driver
    /// model is [`DriverConfig::Wdm`]
    #[must_use]
    #[tracing::instrument(level = "debug")]
    pub fn compute_wdffunctions_symbol_name(&self) -> Option<String> {
        let (wdf_major_version, wdf_minor_version) = match self.driver_config {
            DriverConfig::Kmdf(config) => {
                (config.kmdf_version_major, config.target_kmdf_version_minor)
            }
            DriverConfig::Umdf(config) => {
                (config.umdf_version_major, config.target_umdf_version_minor)
            }
            DriverConfig::Wdm => return None,
        };

        Some(format!(
            "WdfFunctions_{wdf_major_version:02}0{wdf_minor_version:02}"
        ))
    }

    /// Configure a Cargo build of a binary that depends on the WDK. This
    /// emits specially formatted prints to Cargo based on this [`Config`].
    ///
    /// This consists mainly of linker setting configuration. This must be
    /// called from a Cargo build script of the binary being built
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// * any of the required WDK paths do not exist
    /// * the C runtime is not configured to be statically linked for a
    ///   kernel-mode driver
    ///
    /// # Panics
    ///
    /// Panics if the invoked from outside a Cargo build environment
    #[tracing::instrument(level = "debug")]
    pub fn configure_binary_build(&self) -> Result<(), ConfigError> {
        if !Self::is_crt_static_linked() {
            cfg_if::cfg_if! {
                if #[cfg(all(wdk_build_unstable, skip_umdf_static_crt_check))] {
                    if !matches!(self.driver_config, DriverConfig::Umdf(_)) {
                        return Err(ConfigError::StaticCrtNotEnabled);
                    }
                } else {
                    return Err(ConfigError::StaticCrtNotEnabled);
                }
            };
        }

        // Emit linker search paths
        for path in self.library_paths()? {
            println!("cargo::rustc-link-search={}", path.display());
        }

        match &self.driver_config {
            DriverConfig::Wdm => {
                // Emit WDM-specific libraries to link to
                println!("cargo::rustc-link-lib=static=BufferOverflowFastFailK");
                println!("cargo::rustc-link-lib=static=ntoskrnl");
                println!("cargo::rustc-link-lib=static=hal");
                println!("cargo::rustc-link-lib=static=wmilib");

                // Emit ARM64-specific libraries to link to derived from
                // WindowsDriver.arm64.props
                if self.cpu_architecture == CpuArchitecture::Arm64 {
                    println!("cargo::rustc-link-lib=static=arm64rt");
                }

                // Linker arguments derived from WindowsDriver.KernelMode.props in Ni(22H2) WDK
                println!("cargo::rustc-cdylib-link-arg=/DRIVER");
                println!("cargo::rustc-cdylib-link-arg=/NODEFAULTLIB");
                println!("cargo::rustc-cdylib-link-arg=/SUBSYSTEM:NATIVE");
                println!("cargo::rustc-cdylib-link-arg=/KERNEL");

                // Linker arguments derived from WindowsDriver.KernelMode.WDM.props in Ni(22H2)
                // WDK
                println!("cargo::rustc-cdylib-link-arg=/ENTRY:DriverEntry");

                // Ignore `LNK4257: object file was not compiled for kernel mode; the image
                // might not run` since `rustc` has no support for `/KERNEL`
                println!("cargo::rustc-cdylib-link-arg=/IGNORE:4257");

                // Ignore `LNK4216: Exported entry point DriverEntry` since Rust currently
                // provides no way to set a symbol's name without also exporting the symbol:
                // https://github.com/rust-lang/rust/issues/67399
                println!("cargo::rustc-cdylib-link-arg=/IGNORE:4216");
            }
            DriverConfig::Kmdf(_) => {
                // Emit KMDF-specific libraries to link to
                println!("cargo::rustc-link-lib=static=BufferOverflowFastFailK");
                println!("cargo::rustc-link-lib=static=ntoskrnl");
                println!("cargo::rustc-link-lib=static=hal");
                println!("cargo::rustc-link-lib=static=wmilib");
                println!("cargo::rustc-link-lib=static=WdfLdr");
                println!("cargo::rustc-link-lib=static=WdfDriverEntry");

                // Emit ARM64-specific libraries to link to derived from
                // WindowsDriver.arm64.props
                if self.cpu_architecture == CpuArchitecture::Arm64 {
                    println!("cargo::rustc-link-lib=static=arm64rt");
                }

                // Linker arguments derived from WindowsDriver.KernelMode.props in Ni(22H2) WDK
                println!("cargo::rustc-cdylib-link-arg=/DRIVER");
                println!("cargo::rustc-cdylib-link-arg=/NODEFAULTLIB");
                println!("cargo::rustc-cdylib-link-arg=/SUBSYSTEM:NATIVE");
                println!("cargo::rustc-cdylib-link-arg=/KERNEL");

                // Linker arguments derived from WindowsDriver.KernelMode.KMDF.props in
                // Ni(22H2) WDK
                println!("cargo::rustc-cdylib-link-arg=/ENTRY:FxDriverEntry");

                // Ignore `LNK4257: object file was not compiled for kernel mode; the image
                // might not run` since `rustc` has no support for `/KERNEL`
                println!("cargo::rustc-cdylib-link-arg=/IGNORE:4257");
            }
            DriverConfig::Umdf(umdf_config) => {
                // Emit UMDF-specific libraries to link to
                if umdf_config.umdf_version_major >= 2 {
                    println!("cargo::rustc-link-lib=static=WdfDriverStubUm");
                    println!("cargo::rustc-link-lib=static=ntdll");
                }

                println!("cargo::rustc-cdylib-link-arg=/NODEFAULTLIB:kernel32.lib");
                println!("cargo::rustc-cdylib-link-arg=/NODEFAULTLIB:user32.lib");
                println!("cargo::rustc-link-lib=static=OneCoreUAP");

                // Linker arguments derived from WindowsDriver.UserMode.props in Ni(22H2) WDK
                println!("cargo::rustc-cdylib-link-arg=/SUBSYSTEM:WINDOWS");
            }
        }

        // Emit linker arguments common to all configs
        {
            // Linker arguments derived from Microsoft.Link.Common.props in Ni(22H2) WDK
            println!("cargo::rustc-cdylib-link-arg=/NXCOMPAT");
            println!("cargo::rustc-cdylib-link-arg=/DYNAMICBASE");

            // Always generate Map file with Exports
            println!("cargo::rustc-cdylib-link-arg=/MAP");
            println!("cargo::rustc-cdylib-link-arg=/MAPINFO:EXPORTS");

            // Force Linker Optimizations
            println!("cargo::rustc-cdylib-link-arg=/OPT:REF,ICF");

            // Enable "Forced Integrity Checking" to prevent non-signed binaries from
            // loading
            println!("cargo::rustc-cdylib-link-arg=/INTEGRITYCHECK");

            // Disable Manifest File Generation
            println!("cargo::rustc-cdylib-link-arg=/MANIFEST:NO");
        }

        self.emit_cfg_settings()
    }

    #[tracing::instrument(level = "trace")]
    fn is_crt_static_linked() -> bool {
        const STATICALLY_LINKED_C_RUNTIME_FEATURE_NAME: &str = "crt-static";

        let enabled_cpu_target_features = env::var("CARGO_CFG_TARGET_FEATURE")
            .expect("CARGO_CFG_TARGET_FEATURE should be set by Cargo");

        enabled_cpu_target_features.contains(STATICALLY_LINKED_C_RUNTIME_FEATURE_NAME)
    }

    /// Constructs the architecture-specific Windows SDK library path using the
    /// provided SDK Version and the driver configuration.
    ///
    /// Builds the library path following the Windows SDK convention:
    /// `{library_directory}/{sdk_version}/{km|um}/{architecture}/`
    ///
    /// # Arguments
    ///
    /// * `sdk_version` - Windows SDK version string (e.g., "10.0.22621.0")
    ///
    /// # Returns
    ///
    /// The constructed library path if it exists, otherwise
    /// `ConfigError::DirectoryNotFound`.
    ///
    /// # Examples
    ///
    /// KMDF/AMD64: `C:\...\Lib\10.0.22621.0\km\x64`
    /// UMDF/ARM64: `C:\...\Lib\10.0.22621.0\um\arm64`
    #[tracing::instrument(level = "trace")]
    fn sdk_library_path(&self, sdk_version: String) -> Result<PathBuf, ConfigError> {
        let windows_sdk_library_path =
            self.wdk_content_root
                .join("Lib")
                .join(sdk_version)
                .join(match self.driver_config {
                    DriverConfig::Wdm | DriverConfig::Kmdf(_) => {
                        format!("km/{}", self.cpu_architecture.as_windows_str(),)
                    }
                    DriverConfig::Umdf(_) => {
                        format!("um/{}", self.cpu_architecture.as_windows_str(),)
                    }
                });
        if !windows_sdk_library_path.is_dir() {
            return Err(ConfigError::DirectoryNotFound {
                directory: windows_sdk_library_path.to_string_lossy().into(),
            });
        }
        Ok(windows_sdk_library_path)
    }

    /// Returns the path to the latest available UCX header file present in the
    /// Lib folder of the WDK content root
    #[tracing::instrument(level = "trace")]
    fn ucx_header(&self) -> Result<String, ConfigError> {
        let sdk_version = utils::detect_windows_sdk_version(&self.wdk_content_root)?;
        let ucx_header_root_dir = self.sdk_library_path(sdk_version)?.join("ucx");
        let max_version = utils::find_max_version_in_directory(&ucx_header_root_dir)?;
        let path = format!("ucx/{}.{}/ucxclass.h", max_version.0, max_version.1);
        Ok(path)
    }
}

impl From<DeserializableDriverConfig> for DriverConfig {
    fn from(config: DeserializableDriverConfig) -> Self {
        match config {
            DeserializableDriverConfig::Wdm => Self::Wdm,
            DeserializableDriverConfig::Kmdf(kmdf_config) => Self::Kmdf(kmdf_config),
            DeserializableDriverConfig::Umdf(umdf_config) => Self::Umdf(umdf_config),
        }
    }
}

impl Default for KmdfConfig {
    fn default() -> Self {
        // FIXME: determine default values from TargetVersion and _NT_TARGET_VERSION
        Self {
            kmdf_version_major: 1,
            target_kmdf_version_minor: 33,
            minimum_kmdf_version_minor: None,
        }
    }
}

impl KmdfConfig {
    /// Creates a new [`KmdfConfig`] with default values
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for UmdfConfig {
    fn default() -> Self {
        // FIXME: determine default values from TargetVersion and _NT_TARGET_VERSION
        Self {
            umdf_version_major: 2,
            target_umdf_version_minor: 33,
            minimum_umdf_version_minor: None,
        }
    }
}

impl UmdfConfig {
    /// Creates a new [`UmdfConfig`] with default values
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl CpuArchitecture {
    /// Converts [`CpuArchitecture`] to the string corresponding to what the
    /// architecture is typically referred to in Windows
    #[must_use]
    pub const fn as_windows_str(&self) -> &str {
        match self {
            Self::Amd64 => "x64",
            Self::Arm64 => "ARM64",
        }
    }

    /// Converts from a cargo-provided [`std::str`] to a [`CpuArchitecture`].
    #[must_use]
    pub fn try_from_cargo_str<S: AsRef<str>>(cargo_str: S) -> Option<Self> {
        // Specifically not using the [`std::convert::TryFrom`] trait to be more
        // explicit in function name, since only arch strings from cargo are handled.
        match cargo_str.as_ref() {
            "x86_64" => Some(Self::Amd64),
            "aarch64" => Some(Self::Arm64),
            _ => None,
        }
    }
}

/// Find the path of the toplevel Cargo manifest of the currently executing
/// Cargo subcommand. This should resolve to either:
/// 1. the `Cargo.toml` of the package where the Cargo subcommand (build, check,
///    etc.) was run
/// 2. the `Cargo.toml` provided to the `--manifest-path` argument to the Cargo
///    subcommand
/// 3. the `Cargo.toml` of the workspace that contains the package pointed to by
///    1 or 2
///
/// The returned path should be a manifest in the same directory of the
/// lockfile. This does not support invocations that use non-default target
/// directories (ex. via `--target-dir`). This function only works when called
/// from a `build.rs` file
///
/// # Panics
///
/// Panics if a `Cargo.lock` file cannot be found in any of the ancestors of
/// `OUT_DIR` or if this function was called outside of a `build.rs` file
#[must_use]
#[tracing::instrument(level = "debug")]
pub fn find_top_level_cargo_manifest() -> PathBuf {
    let out_dir =
        PathBuf::from(env::var("OUT_DIR").expect(
            "Cargo should have set the OUT_DIR environment variable when executing build.rs",
        ));

    out_dir
        .ancestors()
        .find(|path| path.join("Cargo.lock").exists())
        .expect("a Cargo.lock file should exist in the same directory as the top-level Cargo.toml")
        .join("Cargo.toml")
}

/// Configure a Cargo build of a library that depends on the WDK.
///
/// This emits specially formatted prints to Cargo based on the [`Config`]
/// derived from `metadata.wdk` sections of `Cargo.toml`s.
///
/// Cargo build graphs that have no valid WDK configurations will emit a
/// warning, but will still return [`Ok`]. This allows libraries
/// designed for multiple configurations to successfully compile when built in
/// isolation.
///
/// # Errors
///
/// This function will return an error if the [`Config`] fails to be
/// serialized
#[tracing::instrument(level = "debug")]
pub fn configure_wdk_library_build() -> Result<(), ConfigError> {
    match Config::from_env_auto() {
        Ok(config) => {
            config.configure_library_build()?;
            Ok(())
        }
        Err(ConfigError::TryFromCargoMetadataError(
            TryFromCargoMetadataError::NoWdkConfigurationsDetected,
        )) => {
            // No WDK configurations will be detected if the crate is not being used in a
            // driver. Since this is usually the case when libraries are being built
            // standalone, this scenario is treated as a warning.
            tracing::warn!("No WDK configurations detected.");
            // check_cfg must be emitted even if no WDK configurations are detected, so that
            // cfg options are still checked
            Config::emit_check_cfg_settings();
            Ok(())
        }

        Err(error) => Err(error),
    }
}

/// Configure a Cargo build of a library that depends on the WDK, then execute a
/// function or closure with the [`Config`] derived from `metadata.wdk` sections
/// of `Cargo.toml`s.
///
/// This emits specially formatted prints to Cargo based on the [`Config`]
/// derived from `metadata.wdk` sections of `Cargo.toml`s.
///
/// Cargo build graphs that have no valid WDK configurations will emit a
/// warning, but will still return [`Ok`]. This allows libraries
/// designed for multiple configurations to successfully compile when built in
/// isolation.
///
/// # Errors
///
/// This function will return an error if the [`Config`] fails to be
/// serialized
#[tracing::instrument(level = "debug", skip(f))]
pub fn configure_wdk_library_build_and_then<F, E>(mut f: F) -> Result<(), E>
where
    F: FnMut(Config) -> Result<(), E>,
    E: std::convert::From<ConfigError>,
{
    match Config::from_env_auto() {
        Ok(config) => {
            config.configure_library_build()?;
            debug!("Calling closure with {config:#?}");
            Ok(f(config)?)
        }
        Err(ConfigError::TryFromCargoMetadataError(
            TryFromCargoMetadataError::NoWdkConfigurationsDetected,
        )) => {
            // No WDK configurations will be detected if the crate is not being used in a
            // driver. Since this is usually the case when libraries are being built
            // standalone, this scenario is treated as a warning.
            tracing::warn!("No WDK configurations detected.");
            // check_cfg must be emitted even if no WDK configurations are detected, so that
            // cfg options are still checked
            Config::emit_check_cfg_settings();
            Ok(())
        }

        Err(error) => Err(error.into()),
    }
}

/// Configure a Cargo build of a binary that depends on the WDK using a
/// [`Config`] derived from `metadata.wdk` sections of `Cargo.toml`s.
///
/// # Errors
///
/// This function will return an error if:
/// * any of the required WDK paths do not exist
/// * the C runtime is not configured to be statically linked
///
/// # Panics
///
/// Panics if the invoked from outside a Cargo build environment
#[tracing::instrument(level = "debug")]
pub fn configure_wdk_binary_build() -> Result<(), ConfigError> {
    Config::from_env_auto()?.configure_binary_build()
}

/// This currently only exports the driver type, but may export more metadata in
/// the future. `EXPORTED_CFG_SETTINGS` is a mapping of cfg key to allowed cfg
/// values
static EXPORTED_CFG_SETTINGS: LazyLock<Vec<(&'static str, Vec<&'static str>)>> =
    LazyLock::new(|| vec![("DRIVER_MODEL-DRIVER_TYPE", vec!["WDM", "KMDF", "UMDF"])]);

/// Detects the WDK build number.
///
/// Detects the Windows Driver Kit (WDK) build number by locating
/// the WDK content root, retrieving the latest Windows SDK version, validating
/// the version format, and extracting the build number.
///
/// # Returns
///
/// Returns a `Result<u32, ConfigError>`, which contains the WDK
/// build number on success or a `ConfigError` on failure.
///
/// # Errors
///
/// Returns an error if:
/// * The WDK content root cannot be detected.
/// * The latest Windows SDK version cannot be retrieved.
/// * The WDK version string format is invalid.
/// * The WDK version number cannot be parsed.
///
/// # Panics
///
/// Panics if the WDK version number cannot be extracted from
/// the version string.
#[tracing::instrument(level = "debug")]
pub fn detect_wdk_build_number() -> Result<u32, ConfigError> {
    let wdk_content_root =
        utils::detect_wdk_content_root().ok_or(ConfigError::WdkContentRootDetectionError)?;
    let detected_sdk_version = detect_windows_sdk_version(&wdk_content_root)?;

    if !utils::validate_wdk_version_format(&detected_sdk_version) {
        return Err(ConfigError::WdkVersionStringFormatError {
            version: detected_sdk_version,
        });
    }

    let wdk_build_number =
        str::parse::<u32>(&utils::get_wdk_version_number(&detected_sdk_version)?).unwrap_or_else(
            |_| panic!("Couldn't parse WDK version number! Version number: {detected_sdk_version}"),
        );

    Ok(wdk_build_number)
}

#[cfg(test)]
mod tests {
    #[cfg(nightly_toolchain)]
    use std::assert_matches::assert_matches;
    use std::{collections::HashMap, ffi::OsStr, sync::Mutex};

    use super::*;
    use crate::utils::{remove_var, set_var};

    mod two_part_version {
        use super::*;

        #[test]
        fn valid_versions() {
            assert_eq!("1.2".parse(), Ok(TwoPartVersion(1, 2)));
            assert_eq!("0.0".parse(), Ok(TwoPartVersion(0, 0)));
            assert_eq!("10.15".parse(), Ok(TwoPartVersion(10, 15)));
            assert_eq!("999.1".parse(), Ok(TwoPartVersion(999, 1)));
            assert_eq!("1.999".parse(), Ok(TwoPartVersion(1, 999)));
            assert_eq!("01.02".parse(), Ok(TwoPartVersion(1, 2)));
            assert_eq!("1.02".parse(), Ok(TwoPartVersion(1, 2)));
            assert_eq!("01.2".parse(), Ok(TwoPartVersion(1, 2)));
        }

        #[test]
        fn invalid_format_versions() {
            assert_eq!(
                String::new().parse::<TwoPartVersion>(),
                Err(TwoPartVersionError::InvalidFormat(String::new()))
            );
            assert_eq!(
                "1".parse::<TwoPartVersion>(),
                Err(TwoPartVersionError::InvalidFormat("1".to_string()))
            );
            assert_eq!(
                "123".parse::<TwoPartVersion>(),
                Err(TwoPartVersionError::InvalidFormat("123".to_string()))
            );
            assert_eq!(
                "1.2.3.4".parse::<TwoPartVersion>(),
                Err(TwoPartVersionError::InvalidFormat("1.2.3.4".to_string()))
            );
            assert_eq!(
                ".".parse::<TwoPartVersion>(),
                Err(TwoPartVersionError::InvalidFormat(".".to_string()))
            );
            assert_eq!(
                ".2".parse::<TwoPartVersion>(),
                Err(TwoPartVersionError::InvalidFormat(".2".to_string()))
            );
            assert_eq!(
                "1.".parse::<TwoPartVersion>(),
                Err(TwoPartVersionError::InvalidFormat("1.".to_string()))
            );
            assert_eq!(
                "myfolder".parse::<TwoPartVersion>(),
                Err(TwoPartVersionError::InvalidFormat("myfolder".to_string()))
            );
        }

        #[test]
        fn parse_error_versions() {
            assert_eq!(
                "a.b".parse::<TwoPartVersion>(),
                Err(TwoPartVersionError::ParseError(
                    "major".to_string(),
                    "a.b".to_string()
                ))
            );
            assert_eq!(
                "1.b".parse::<TwoPartVersion>(),
                Err(TwoPartVersionError::ParseError(
                    "minor".to_string(),
                    "1.b".to_string()
                ))
            );
            assert_eq!(
                "a.2".parse::<TwoPartVersion>(),
                Err(TwoPartVersionError::ParseError(
                    "major".to_string(),
                    "a.2".to_string()
                ))
            );
            assert_eq!(
                "1.2a".parse::<TwoPartVersion>(),
                Err(TwoPartVersionError::ParseError(
                    "minor".to_string(),
                    "1.2a".to_string()
                ))
            );
            assert_eq!(
                "1a.2".parse::<TwoPartVersion>(),
                Err(TwoPartVersionError::ParseError(
                    "major".to_string(),
                    "1a.2".to_string()
                ))
            );
            assert_eq!(
                " 1.2".parse::<TwoPartVersion>(),
                Err(TwoPartVersionError::ParseError(
                    "major".to_string(),
                    " 1.2".to_string()
                ))
            );
            assert_eq!(
                "1.2 ".parse::<TwoPartVersion>(),
                Err(TwoPartVersionError::ParseError(
                    "minor".to_string(),
                    "1.2 ".to_string()
                ))
            );
            assert_eq!(
                "1 .2".parse::<TwoPartVersion>(),
                Err(TwoPartVersionError::ParseError(
                    "major".to_string(),
                    "1 .2".to_string()
                ))
            );
            assert_eq!(
                "1. 2".parse::<TwoPartVersion>(),
                Err(TwoPartVersionError::ParseError(
                    "minor".to_string(),
                    "1. 2".to_string()
                ))
            );
        }

        #[test]
        fn version_ordering() {
            let v1_0 = TwoPartVersion(1, 0);
            let v1_1 = TwoPartVersion(1, 1);
            let v1_999 = TwoPartVersion(1, 999);
            let v2_0 = TwoPartVersion(2, 0);
            let v2_1 = TwoPartVersion(2, 1);

            assert!(v1_0 < v1_1);
            assert!(v1_1 < v1_999);
            assert!(v1_999 < v2_0);
            assert!(v2_0 < v2_1);
        }

        #[test]
        fn equality() {
            let v1 = TwoPartVersion(1, 2);
            let v2 = TwoPartVersion(1, 2);
            let v3 = TwoPartVersion(1, 3);
            assert_eq!(v1, v2);
            assert_ne!(v1, v3);
        }

        #[test]
        fn debug_formatting() {
            let version = TwoPartVersion(1, 2);
            let debug_str = format!("{version:?}");
            assert_eq!(debug_str, "TwoPartVersion(1, 2)");
        }

        #[test]
        fn max_selection() {
            let versions = [
                TwoPartVersion(1, 2),
                TwoPartVersion(1, 10),
                TwoPartVersion(2, 0),
                TwoPartVersion(1, 5),
                TwoPartVersion(2, 1),
                TwoPartVersion(1, 999),
            ];

            let max_version = versions.iter().max().unwrap();
            assert_eq!(*max_version, TwoPartVersion(2, 1));
        }

        #[test]
        fn u32_max_and_overflow() {
            assert_eq!(
                "4294967295.4294967295".parse::<TwoPartVersion>(),
                Ok(TwoPartVersion(4_294_967_295, 4_294_967_295))
            );
            assert_eq!(
                "4294967296.0".parse::<TwoPartVersion>(),
                Err(TwoPartVersionError::ParseError(
                    "major".to_string(),
                    "4294967296.0".to_string()
                ))
            );
            assert_eq!(
                "99999999999999999999.0".parse::<TwoPartVersion>(),
                Err(TwoPartVersionError::ParseError(
                    "major".to_string(),
                    "99999999999999999999.0".to_string()
                ))
            );
            assert_eq!(
                "0.4294967296".parse::<TwoPartVersion>(),
                Err(TwoPartVersionError::ParseError(
                    "minor".to_string(),
                    "0.4294967296".to_string()
                ))
            );
            assert_eq!(
                "1.99999999999999999999".parse::<TwoPartVersion>(),
                Err(TwoPartVersionError::ParseError(
                    "minor".to_string(),
                    "1.99999999999999999999".to_string()
                ))
            );
            assert_eq!(
                "4294967296.4294967296".parse::<TwoPartVersion>(),
                Err(TwoPartVersionError::ParseError(
                    "major".to_string(),
                    "4294967296.4294967296".to_string()
                ))
            );
        }
    }

    /// Runs function after modifying environment variables, and returns the
    /// function's return value.
    ///
    /// The environment is guaranteed to be not modified during the execution
    /// of the function, and the environment is reset to its original state
    /// after execution of the function. No testing asserts should be called in
    /// the function, since a failing test will poison the mutex, and cause all
    /// remaining tests to fail.
    ///
    /// # Panics
    ///
    /// Panics if called with duplicate environment variable keys.
    pub fn with_env<K, V, F, R>(env_vars_key_value_pairs: &[(K, V)], f: F) -> R
    where
        K: AsRef<OsStr> + std::cmp::Eq + std::hash::Hash,
        V: AsRef<OsStr>,
        F: FnOnce() -> R,
    {
        // Tests can execute in multiple threads in the same process, so mutex must be
        // used to guard access to the environment variables
        static ENV_MUTEX: Mutex<()> = Mutex::new(());

        let _mutex_guard = ENV_MUTEX.lock().unwrap();
        let mut original_env_vars = HashMap::new();

        // set requested environment variables
        for (key, value) in env_vars_key_value_pairs {
            if let Ok(original_value) = env::var(key) {
                let insert_result = original_env_vars.insert(key, original_value);
                assert!(
                    insert_result.is_none(),
                    "Duplicate environment variable keys were provided"
                );
            }
            set_var(key, value);
        }

        let f_return_value = f();

        // reset all set environment variables
        for (key, _) in env_vars_key_value_pairs {
            original_env_vars.get(key).map_or_else(
                || {
                    remove_var(key);
                },
                |value| {
                    set_var(key, value);
                },
            );
        }

        f_return_value
    }

    #[test]
    fn default_config() {
        let config = with_env(&[("CARGO_CFG_TARGET_ARCH", "x86_64")], Config::new);

        #[cfg(nightly_toolchain)]
        assert_matches!(config.driver_config, DriverConfig::Wdm);
        assert_eq!(config.cpu_architecture, CpuArchitecture::Amd64);
    }

    #[test]
    fn wdm_config() {
        let config = with_env(&[("CARGO_CFG_TARGET_ARCH", "x86_64")], || Config {
            driver_config: DriverConfig::Wdm,
            ..Config::default()
        });

        #[cfg(nightly_toolchain)]
        assert_matches!(config.driver_config, DriverConfig::Wdm);
        assert_eq!(config.cpu_architecture, CpuArchitecture::Amd64);
    }

    #[test]
    fn default_kmdf_config() {
        let config = with_env(&[("CARGO_CFG_TARGET_ARCH", "x86_64")], || Config {
            driver_config: DriverConfig::Kmdf(KmdfConfig::new()),
            ..Config::default()
        });

        #[cfg(nightly_toolchain)]
        assert_matches!(
            config.driver_config,
            DriverConfig::Kmdf(KmdfConfig {
                kmdf_version_major: 1,
                target_kmdf_version_minor: 33,
                minimum_kmdf_version_minor: None
            })
        );
        assert_eq!(config.cpu_architecture, CpuArchitecture::Amd64);
    }

    #[test]
    fn kmdf_config() {
        let config = with_env(&[("CARGO_CFG_TARGET_ARCH", "x86_64")], || Config {
            driver_config: DriverConfig::Kmdf(KmdfConfig {
                kmdf_version_major: 1,
                target_kmdf_version_minor: 15,
                minimum_kmdf_version_minor: None,
            }),
            ..Config::default()
        });

        #[cfg(nightly_toolchain)]
        assert_matches!(
            config.driver_config,
            DriverConfig::Kmdf(KmdfConfig {
                kmdf_version_major: 1,
                target_kmdf_version_minor: 15,
                minimum_kmdf_version_minor: None
            })
        );
        assert_eq!(config.cpu_architecture, CpuArchitecture::Amd64);
    }

    #[test]
    fn default_umdf_config() {
        let config = with_env(&[("CARGO_CFG_TARGET_ARCH", "x86_64")], || Config {
            driver_config: DriverConfig::Umdf(UmdfConfig::new()),
            ..Config::default()
        });

        #[cfg(nightly_toolchain)]
        assert_matches!(
            config.driver_config,
            DriverConfig::Umdf(UmdfConfig {
                umdf_version_major: 2,
                target_umdf_version_minor: 33,
                minimum_umdf_version_minor: None
            })
        );
        assert_eq!(config.cpu_architecture, CpuArchitecture::Amd64);
    }

    #[test]
    fn umdf_config() {
        let config = with_env(&[("CARGO_CFG_TARGET_ARCH", "aarch64")], || Config {
            driver_config: DriverConfig::Umdf(UmdfConfig {
                umdf_version_major: 2,
                target_umdf_version_minor: 15,
                minimum_umdf_version_minor: None,
            }),
            ..Config::default()
        });

        #[cfg(nightly_toolchain)]
        assert_matches!(
            config.driver_config,
            DriverConfig::Umdf(UmdfConfig {
                umdf_version_major: 2,
                target_umdf_version_minor: 15,
                minimum_umdf_version_minor: None
            })
        );
        assert_eq!(config.cpu_architecture, CpuArchitecture::Arm64);
    }

    #[test]
    fn test_try_from_cargo_str() {
        assert_eq!(
            CpuArchitecture::try_from_cargo_str("x86_64"),
            Some(CpuArchitecture::Amd64)
        );
        assert_eq!(
            CpuArchitecture::try_from_cargo_str("aarch64"),
            Some(CpuArchitecture::Arm64)
        );
        assert_eq!(CpuArchitecture::try_from_cargo_str("arm"), None);
    }

    mod bindgen_header_contents {
        use super::*;
        use crate::{KmdfConfig, UmdfConfig};

        #[test]
        fn wdm() {
            let config = with_env(&[("CARGO_CFG_TARGET_ARCH", "x86_64")], || Config {
                driver_config: DriverConfig::Wdm,
                ..Default::default()
            });

            assert_eq!(
                config.bindgen_header_contents([ApiSubset::Base]).unwrap(),
                r#"#include "ntifs.h"
#include "ntddk.h"
#include "ntstrsafe.h"
"#,
            );
        }

        #[test]
        fn kmdf() {
            let config = with_env(&[("CARGO_CFG_TARGET_ARCH", "x86_64")], || Config {
                driver_config: DriverConfig::Kmdf(KmdfConfig {
                    kmdf_version_major: 1,
                    target_kmdf_version_minor: 33,
                    minimum_kmdf_version_minor: None,
                }),
                ..Default::default()
            });

            assert_eq!(
                config
                    .bindgen_header_contents([ApiSubset::Base, ApiSubset::Wdf])
                    .unwrap(),
                r#"#include "ntifs.h"
#include "ntddk.h"
#include "ntstrsafe.h"
#include "wdf.h"
"#,
            );
        }

        #[test]
        fn umdf() {
            let config = with_env(&[("CARGO_CFG_TARGET_ARCH", "aarch64")], || Config {
                driver_config: DriverConfig::Umdf(UmdfConfig {
                    umdf_version_major: 2,
                    target_umdf_version_minor: 15,
                    minimum_umdf_version_minor: None,
                }),
                ..Default::default()
            });

            assert_eq!(
                config
                    .bindgen_header_contents([ApiSubset::Base, ApiSubset::Wdf])
                    .unwrap(),
                r#"#include "windows.h"
#include "wdf.h"
"#,
            );
        }
    }
    mod compute_wdffunctions_symbol_name {
        use super::*;
        use crate::{KmdfConfig, UmdfConfig};

        #[test]
        fn kmdf() {
            let config = with_env(&[("CARGO_CFG_TARGET_ARCH", "x86_64")], || Config {
                driver_config: DriverConfig::Kmdf(KmdfConfig {
                    kmdf_version_major: 1,
                    target_kmdf_version_minor: 15,
                    minimum_kmdf_version_minor: None,
                }),
                ..Default::default()
            });

            let result = config.compute_wdffunctions_symbol_name();

            assert_eq!(result, Some("WdfFunctions_01015".to_string()));
        }

        #[test]
        fn umdf() {
            let config = with_env(&[("CARGO_CFG_TARGET_ARCH", "aarch64")], || Config {
                driver_config: DriverConfig::Umdf(UmdfConfig {
                    umdf_version_major: 2,
                    target_umdf_version_minor: 33,
                    minimum_umdf_version_minor: None,
                }),
                ..Default::default()
            });

            let result = config.compute_wdffunctions_symbol_name();

            assert_eq!(result, Some("WdfFunctions_02033".to_string()));
        }

        #[test]
        fn wdm() {
            let config = with_env(&[("CARGO_CFG_TARGET_ARCH", "x86_64")], || Config {
                driver_config: DriverConfig::Wdm,
                ..Default::default()
            });

            let result = config.compute_wdffunctions_symbol_name();

            assert_eq!(result, None);
        }
    }

    mod validate_and_add_folder_path {
        use assert_fs::prelude::*;

        use super::*;

        #[test]
        fn valid_directory_is_added_successfully() {
            let temp_dir = assert_fs::TempDir::new().unwrap();
            let mut include_paths = Vec::new();

            let result = Config::validate_and_add_folder_path(&mut include_paths, temp_dir.path());

            assert!(result.is_ok());
            assert_eq!(include_paths.len(), 1);
            assert!(include_paths[0].exists());
            assert!(include_paths[0].is_dir());

            // Verify the exact canonicalized path was added
            let expected_path = absolute(temp_dir.path()).unwrap();
            assert_eq!(include_paths[0], expected_path);
        }

        #[test]
        fn non_existent_path_returns_directory_not_found_error() {
            let non_existent_path = std::path::Path::new("/this/path/does/not/exist");
            let mut include_paths = Vec::new();

            let result =
                Config::validate_and_add_folder_path(&mut include_paths, non_existent_path);

            assert!(result.is_err());
            #[cfg(nightly_toolchain)]
            assert_matches!(
                result.unwrap_err(),
                ConfigError::DirectoryNotFound { directory } if directory == non_existent_path.to_string_lossy()
            );
            assert_eq!(include_paths.len(), 0);
        }

        #[test]
        fn file_path_returns_directory_not_found_error() {
            let temp_dir = assert_fs::TempDir::new().unwrap();
            let file = temp_dir.child("test_file.txt");
            file.write_str("test content").unwrap();
            let mut include_paths = Vec::new();

            let result = Config::validate_and_add_folder_path(&mut include_paths, file.path());

            assert!(result.is_err());
            #[cfg(nightly_toolchain)]
            assert_matches!(
                result.unwrap_err(),
                ConfigError::DirectoryNotFound { directory } if directory == file.path().to_string_lossy()
            );
            assert_eq!(include_paths.len(), 0);
        }

        #[test]
        fn path_is_canonicalized_before_adding() {
            let temp_dir = assert_fs::TempDir::new().unwrap();
            let sub_dir = temp_dir.child("subdir");
            sub_dir.create_dir_all().unwrap();

            // Create a path with ".." to test canonicalization
            let complex_path = sub_dir.path().join("..").join("subdir");
            let mut include_paths = Vec::new();

            let result = Config::validate_and_add_folder_path(&mut include_paths, &complex_path);

            assert!(result.is_ok());
            assert_eq!(include_paths.len(), 1);

            // The canonicalized path should not contain ".."
            assert!(!include_paths[0].to_string_lossy().contains(".."));
            assert!(include_paths[0].is_absolute());

            // Verify the path resolves to the actual subdir path
            let expected_path = absolute(sub_dir.path()).unwrap();
            assert_eq!(include_paths[0], expected_path);
        }

        #[test]
        fn multiple_paths_are_added_correctly() {
            let temp_dir = assert_fs::TempDir::new().unwrap();
            let dir1 = temp_dir.child("dir1");
            let dir2 = temp_dir.child("dir2");
            dir1.create_dir_all().unwrap();
            dir2.create_dir_all().unwrap();

            let mut include_paths = Vec::new();

            let result1 = Config::validate_and_add_folder_path(&mut include_paths, dir1.path());
            let result2 = Config::validate_and_add_folder_path(&mut include_paths, dir2.path());

            assert!(result1.is_ok());
            assert!(result2.is_ok());
            assert_eq!(include_paths.len(), 2);

            // Both paths should be present and different
            assert_ne!(include_paths[0], include_paths[1]);
            assert!(include_paths[0].exists());
            assert!(include_paths[1].exists());

            // Verify both paths match their expected canonicalized values
            let expected_path1 = absolute(dir1.path()).unwrap();
            let expected_path2 = absolute(dir2.path()).unwrap();
            assert_eq!(include_paths[0], expected_path1);
            assert_eq!(include_paths[1], expected_path2);
        }

        #[test]
        fn nested_directory_is_handled_correctly() {
            let temp_dir = assert_fs::TempDir::new().unwrap();
            let nested_dir = temp_dir.child("level1").child("level2").child("level3");
            nested_dir.create_dir_all().unwrap();
            let mut include_paths = Vec::new();

            let result =
                Config::validate_and_add_folder_path(&mut include_paths, nested_dir.path());

            assert!(result.is_ok());
            assert_eq!(include_paths.len(), 1);
            assert!(include_paths[0].exists());
            assert!(include_paths[0].is_dir());

            // Verify the nested path matches the expected canonicalized value
            let expected_path = absolute(nested_dir.path()).unwrap();
            assert_eq!(include_paths[0], expected_path);
        }

        #[test]
        fn same_directory_can_be_added_multiple_times() {
            let temp_dir = assert_fs::TempDir::new().unwrap();
            let mut include_paths = Vec::new();

            let result1 = Config::validate_and_add_folder_path(&mut include_paths, temp_dir.path());
            let result2 = Config::validate_and_add_folder_path(&mut include_paths, temp_dir.path());

            assert!(result1.is_ok());
            assert!(result2.is_ok());
            assert_eq!(include_paths.len(), 2);
            assert_eq!(include_paths[0], include_paths[1]);

            // Verify both entries match the expected canonicalized path
            let expected_path = absolute(temp_dir.path()).unwrap();
            assert_eq!(include_paths[0], expected_path);
            assert_eq!(include_paths[1], expected_path);
        }

        #[cfg(windows)]
        #[test]
        fn windows_extended_length_paths_are_stripped() {
            let temp_dir = assert_fs::TempDir::new().unwrap();
            let mut include_paths = Vec::new();

            let result = Config::validate_and_add_folder_path(&mut include_paths, temp_dir.path());

            assert!(result.is_ok());
            assert_eq!(include_paths.len(), 1);

            // `validate_and_add_folder_path` should always ensure that the path should not
            // start with \\?\ on Windows
            let path_str = include_paths[0].to_string_lossy();
            assert!(!path_str.starts_with(r"\\?\"));

            // Verify the path matches expected value
            let expected_path = absolute(temp_dir.path()).unwrap();
            assert_eq!(include_paths[0], expected_path);
        }
    }
}
