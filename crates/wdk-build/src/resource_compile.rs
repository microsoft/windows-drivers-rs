// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Version resource compilation for Windows driver binaries.
//!
//! This module generates and compiles a Windows `VERSIONINFO` resource (`.rc`
//! file) that gets linked into the driver binary. This embeds version metadata
//! (file version, product name, company, copyright, etc.) into the `.sys` or
//! `.dll` PE file, making it visible in Windows Explorer's file properties.
//!
//! # Usage
//!
//! Add version resource metadata to your driver's `Cargo.toml`:
//!
//! ```toml
//! [package.metadata.wdk.version-resource]
//! company-name = "Microsoft Corporation"
//! copyright = "Copyright (C) Microsoft Corporation. All rights reserved"
//! product-name = "Surface"
//! file-description = "My Driver"
//! ```
//!
//! Then call [`compile_version_resource`] from your `build.rs`:
//!
//! ```rust,ignore
//! let config = wdk_build::Config::from_env_auto()?;
//! config.configure_binary_build()?;
//! wdk_build::resource_compile::compile_version_resource(&config)?;
//! ```
//!
//! # Version Sourcing
//!
//! The version is determined by (in priority order):
//! 1. `WDK_BUILD_VERSION` environment variable (for CI pipelines)
//! 2. `CARGO_PKG_VERSION` (from `Cargo.toml` `[package]` version)
//!
//! Semver versions are mapped to 4-part Windows versions by appending `.0`
//! for the revision component. Prerelease suffixes (e.g. `-preview`) are
//! stripped. Each component must fit in a 16-bit word (0–65535).

use std::{
    env,
    fmt::Write as _,
    fs,
    path::{absolute, Path, PathBuf},
    process::Command,
};

use crate::{Config, ConfigError, DriverConfig};

/// Environment variable for overriding the driver version in CI pipelines.
///
/// When set, this takes priority over `CARGO_PKG_VERSION`. The value should
/// be in the format `MAJOR.MINOR.PATCH` or `MAJOR.MINOR.PATCH.REVISION`.
/// A prerelease suffix (e.g. `-preview`) is stripped automatically.
const VERSION_ENV_VAR: &str = "WDK_BUILD_VERSION";

/// A parsed 4-part Windows version number.
///
/// Windows `VERSIONINFO` resources use four 16-bit components:
/// `MAJOR.MINOR.PATCH.REVISION`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriverVersion {
    /// Major version number
    pub major: u16,
    /// Minor version number
    pub minor: u16,
    /// Patch/build version number
    pub patch: u16,
    /// Revision number
    pub revision: u16,
}

impl DriverVersion {
    /// Format as a comma-separated string for the `VER_FILEVERSION` RC macro.
    ///
    /// Example: `1,2,3,0`
    #[must_use]
    pub fn as_rc_numeric(&self) -> String {
        format!(
            "{},{},{},{}",
            self.major, self.minor, self.patch, self.revision
        )
    }

    /// Format as a dot-separated string for the `VER_FILEVERSION_STR` RC macro.
    ///
    /// Example: `"1.2.3.0"`
    #[must_use]
    pub fn as_rc_string(&self) -> String {
        format!(
            "{}.{}.{}.{}",
            self.major, self.minor, self.patch, self.revision
        )
    }
}

/// Metadata for the version resource, sourced from
/// `[package.metadata.wdk.version-resource]` in `Cargo.toml`.
#[derive(Debug, Clone)]
pub struct VersionResourceMetadata {
    /// Company name (e.g. "Microsoft Corporation")
    pub company_name: String,
    /// Copyright string
    pub copyright: String,
    /// Product name (e.g. "Surface")
    pub product_name: String,
    /// File description shown in Explorer properties
    pub file_description: String,
    /// Internal name of the binary (e.g. "MyDriver.sys")
    pub internal_name: Option<String>,
    /// Original filename of the binary
    pub original_filename: Option<String>,
}

/// Errors specific to version resource compilation.
#[derive(Debug, thiserror::Error)]
pub enum ResourceCompileError {
    /// A version string could not be parsed into a valid driver version.
    #[error("invalid version string '{value}': {reason}")]
    VersionParseError {
        /// The version string that could not be parsed
        value: String,
        /// Description of why the parsing failed
        reason: String,
    },

    /// The resource compiler (`rc.exe`) exited with a non-zero status.
    #[error("rc.exe failed with {status}:\n{stderr}")]
    CompilerFailed {
        /// The exit status of `rc.exe`
        status: std::process::ExitStatus,
        /// The stderr output from `rc.exe`
        stderr: String,
    },

    /// Required metadata is missing from `Cargo.toml`.
    #[error("version resource metadata error: {detail}")]
    MetadataError {
        /// Description of the metadata problem
        detail: String,
    },

    /// An I/O error occurred during resource compilation.
    #[error("I/O error during resource compilation")]
    IoError(#[from] std::io::Error),

    /// An error from the WDK build configuration.
    #[error("WDK build configuration error during resource compilation")]
    ConfigError(#[source] Box<ConfigError>),
}

/// Parse a version string into a [`DriverVersion`].
///
/// Accepts the following formats:
/// - `MAJOR.MINOR.PATCH` (revision defaults to 0)
/// - `MAJOR.MINOR.PATCH.REVISION`
/// - Semver with prerelease tag: `1.2.3-alpha` (prerelease suffix is stripped)
///
/// Each component must be in the range `0..=65535`.
///
/// # Errors
///
/// Returns [`ResourceCompileError::VersionParseError`] if the string cannot
/// be parsed or any component exceeds the 16-bit limit.
pub fn parse_version(version_str: &str) -> Result<DriverVersion, ResourceCompileError> {
    // Strip semver prerelease suffix (everything after first `-`) if present.
    // e.g. "3.0.433-preview" → "3.0.433"
    let version_clean = version_str.split('-').next().unwrap_or(version_str);

    let parts: Vec<&str> = version_clean.split('.').collect();

    let (major, minor, patch, revision) = match parts.len() {
        3 => {
            let major = parse_version_component(parts[0], "major", version_str)?;
            let minor = parse_version_component(parts[1], "minor", version_str)?;
            let patch = parse_version_component(parts[2], "patch", version_str)?;
            (major, minor, patch, 0)
        }
        4 => {
            let major = parse_version_component(parts[0], "major", version_str)?;
            let minor = parse_version_component(parts[1], "minor", version_str)?;
            let patch = parse_version_component(parts[2], "patch", version_str)?;
            let revision = parse_version_component(parts[3], "revision", version_str)?;
            (major, minor, patch, revision)
        }
        _ => {
            return Err(ResourceCompileError::VersionParseError {
                value: version_str.to_string(),
                reason: format!(
                    "expected 3 or 4 dot-separated components, found {}",
                    version_str.to_string()
                ),
            });
        }
    };

    Ok(DriverVersion {
        major,
        minor,
        patch,
        revision,
    })
}

/// Parse a single version component string into a `u16`.
fn parse_version_component(
    s: &str,
    component_name: &str,
    full_version: &str,
) -> Result<u16, ResourceCompileError> {
    s.parse()
        .map_err(|_| ResourceCompileError::VersionParseError {
            value: full_version.to_string(),
            reason: format!("{component_name} component '{s}' is not a valid u16 (0-65535)"),
        })
}

/// Determine the driver version to embed in the binary.
///
/// Checks pipeline env var first, then falls back to
/// `CARGO_PKG_VERSION` env var (emitted by cargo).
fn resolve_version() -> Result<DriverVersion, ResourceCompileError> {
    let version_str = if let Ok(ci_version) = env::var(VERSION_ENV_VAR) {
        ci_version
    } else {
        env::var("CARGO_PKG_VERSION").map_err(|_| ResourceCompileError::MetadataError {
            detail: "CARGO_PKG_VERSION environment variable not set. This function must be \
                     called from a Cargo build script."
                .to_string(),
        })?
    };

    parse_version(&version_str)
}

/// Resolve the driver binary filename based on driver type and crate name.
fn resolve_driver_filename(config: &Config) -> String {
    let crate_name = env::var("CARGO_PKG_NAME").unwrap_or_else(|_| "driver".to_string());
    // Cargo converts hyphens to underscores in artifact names
    let artifact_name = crate_name.replace('-', "_");

    let extension = match config.driver_config {
        DriverConfig::Wdm | DriverConfig::Kmdf(_) => "sys",
        DriverConfig::Umdf(_) => "dll",
    };

    format!("{artifact_name}.{extension}")
}

/// Read version resource metadata from `Cargo.toml`
/// `[package.metadata.wdk.version-resource]` section.
///
/// This reads the current package's `CARGO_MANIFEST_DIR/Cargo.toml` and
/// extracts the version-resource metadata using `cargo_metadata`.
///
/// # Errors
///
/// Returns [`ResourceCompileError::MetadataError`] if required fields are
/// missing.
pub fn read_version_resource_metadata() -> Result<VersionResourceMetadata, ResourceCompileError> {
    let manifest_dir =
        env::var("CARGO_MANIFEST_DIR").map_err(|_| ResourceCompileError::MetadataError {
            detail: "CARGO_MANIFEST_DIR not set. Must be called from a build script.".to_string(),
        })?;

    let manifest_path = Path::new(&manifest_dir).join("Cargo.toml");

    let metadata = cargo_metadata::MetadataCommand::new()
        .manifest_path(&manifest_path)
        .no_deps()
        .exec()
        .map_err(|e| ResourceCompileError::MetadataError {
            detail: format!("cargo metadata failed: {e}"),
        })?;

    // Find the current package
    let pkg_name = env::var("CARGO_PKG_NAME").map_err(|_| ResourceCompileError::MetadataError {
        detail: "CARGO_PKG_NAME not set".to_string(),
    })?;

    let package = metadata
        .packages
        .iter()
        .find(|p| p.name == pkg_name)
        .ok_or_else(|| ResourceCompileError::MetadataError {
            detail: format!("package '{pkg_name}' not found in cargo metadata"),
        })?;

    let version_resource = package
        .metadata
        .get("wdk")
        .and_then(|w| w.get("version-resource"))
        .ok_or_else(|| ResourceCompileError::MetadataError {
            detail: "[package.metadata.wdk.version-resource] section not found in Cargo.toml"
                .to_string(),
        })?;

    let company_name = version_resource
        .get("company-name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| ResourceCompileError::MetadataError {
            detail: "company-name is required in [package.metadata.wdk.version-resource]"
                .to_string(),
        })?
        .to_string();

    let copyright = version_resource
        .get("copyright")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| ResourceCompileError::MetadataError {
            detail: "copyright is required in [package.metadata.wdk.version-resource]".to_string(),
        })?
        .to_string();

    let product_name = version_resource
        .get("product-name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| ResourceCompileError::MetadataError {
            detail: "product-name is required in [package.metadata.wdk.version-resource]"
                .to_string(),
        })?
        .to_string();

    // file-description defaults to CARGO_PKG_DESCRIPTION
    let file_description = version_resource
        .get("file-description")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
        .or_else(|| env::var("CARGO_PKG_DESCRIPTION").ok())
        .unwrap_or_else(|| company_name.clone());

    let internal_name = version_resource
        .get("internal-name")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string);

    let original_filename = version_resource
        .get("original-filename")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string);

    Ok(VersionResourceMetadata {
        company_name,
        copyright,
        product_name,
        file_description,
        internal_name,
        original_filename,
    })
}

/// Generate the contents of a WDK-style `.rc` file.
///
/// The generated file uses the standard Windows driver pattern:
/// `ntverp.h` + `common.ver` to emit a `VS_VERSION_INFO` resource block.
///
/// # Arguments
///
/// * `version` - The 4-part driver version to embed
/// * `metadata` - Version resource metadata (company, product, etc.)
/// * `config` - WDK build configuration (used to determine file type)
#[must_use]
pub fn generate_rc_content(
    version: &DriverVersion,
    metadata: &VersionResourceMetadata,
    config: &Config,
) -> String {
    let driver_filename = metadata
        .internal_name
        .clone()
        .unwrap_or_else(|| resolve_driver_filename(config));

    let original_filename = metadata
        .original_filename
        .clone()
        .unwrap_or_else(|| driver_filename.clone());

    // Determine file type based on driver model
    let (ver_filetype, ver_filesubtype) = match config.driver_config {
        DriverConfig::Wdm | DriverConfig::Kmdf(_) => ("VFT_DRV", "VFT2_DRV_SYSTEM"),
        DriverConfig::Umdf(_) => ("VFT_DLL", "VFT2_UNKNOWN"),
    };

    let mut rc = String::with_capacity(1024);
    writeln!(rc, "#include <windows.h>").expect("write to String should not fail");
    writeln!(rc, "#include <ntverp.h>").expect("write to String should not fail");
    writeln!(rc).expect("write to String should not fail");
    writeln!(rc, "#define VER_FILETYPE             {ver_filetype}")
        .expect("write to String should not fail");
    writeln!(rc, "#define VER_FILESUBTYPE          {ver_filesubtype}")
        .expect("write to String should not fail");
    writeln!(rc).expect("write to String should not fail");
    writeln!(rc, "#define VER_INTERNALNAME_STR     \"{driver_filename}\"")
        .expect("write to String should not fail");
    writeln!(
        rc,
        "#define VER_ORIGINALFILENAME_STR \"{original_filename}\""
    )
    .expect("write to String should not fail");
    writeln!(rc).expect("write to String should not fail");

    // Use consistent #ifdef/#undef/#define pattern for all overridden macros
    let file_description = &metadata.file_description;
    let product_name = &metadata.product_name;
    let copyright = &metadata.copyright;
    let company_name = &metadata.company_name;
    for (macro_name, macro_value) in [
        ("VER_FILEDESCRIPTION_STR", format!("\"{file_description}\"")),
        ("VER_PRODUCTNAME_STR", format!("\"{product_name}\"")),
        ("VER_FILEVERSION", version.as_rc_numeric()),
        (
            "VER_FILEVERSION_STR",
            format!("\"{}\"", version.as_rc_string()),
        ),
        ("VER_PRODUCTVERSION", "VER_FILEVERSION".to_string()),
        ("VER_PRODUCTVERSION_STR", "VER_FILEVERSION_STR".to_string()),
        ("VER_LEGALCOPYRIGHT_STR", format!("\"{copyright}\"")),
        ("VER_COMPANYNAME_STR", format!("\"{company_name}\"")),
    ] {
        writeln!(rc, "#ifdef  {macro_name}").expect("write to String should not fail");
        writeln!(rc, "#undef  {macro_name}").expect("write to String should not fail");
        writeln!(rc, "#endif").expect("write to String should not fail");
        writeln!(rc, "#define {macro_name}  {macro_value}")
            .expect("write to String should not fail");
        writeln!(rc).expect("write to String should not fail");
    }

    writeln!(rc, "#include \"common.ver\"").expect("write to String should not fail");

    rc
}

/// Compute the include paths needed for resource compilation.
///
/// Unlike [`Config::include_paths`] (which provides paths for C/bindgen
/// compilation), resource compilation needs:
/// - `Include/<sdk>/um` (for `windows.h`)
/// - `Include/<sdk>/shared` (for shared headers)
/// - `Include/<sdk>/km` (for `ntverp.h`, kernel-mode drivers only)
fn resource_include_paths(config: &Config) -> Result<Vec<PathBuf>, ResourceCompileError> {
    let sdk_version = crate::utils::detect_windows_sdk_version(&config.wdk_content_root)
        .map_err(|e| ResourceCompileError::ConfigError(Box::new(e)))?;
    let include_directory = config.wdk_content_root.join("Include").join(&sdk_version);

    let mut paths = vec![];

    // um directory contains windows.h
    let um_path = include_directory.join("um");
    if um_path.is_dir() {
        paths.push(absolute(&um_path)?);
    }

    // shared directory contains shared headers
    let shared_path = include_directory.join("shared");
    if shared_path.is_dir() {
        paths.push(absolute(&shared_path)?);
    }

    // km directory contains ntverp.h (for kernel-mode drivers)
    match config.driver_config {
        DriverConfig::Wdm | DriverConfig::Kmdf(_) => {
            let km_path = include_directory.join("km");
            if km_path.is_dir() {
                paths.push(absolute(&km_path)?);
            }
        }
        DriverConfig::Umdf(_) => {}
    }

    Ok(paths)
}

/// Generate and compile a Windows `VERSIONINFO` resource, then emit a
/// linker directive to embed it in the driver binary.
///
/// This is the main entry point for version resource compilation. It:
/// 1. Reads version from `WDK_BUILD_VERSION` or `CARGO_PKG_VERSION`
/// 2. Reads metadata from `[package.metadata.wdk.version-resource]`
/// 3. Generates a WDK-style `.rc` file in `OUT_DIR`
/// 4. Compiles it with `rc.exe`
/// 5. Emits `cargo::rustc-cdylib-link-arg` to link the `.res` into the binary
///
/// # Errors
///
/// Returns [`ConfigError`] (wrapping [`ResourceCompileError`]) if:
/// - Version metadata is missing or invalid
/// - `rc.exe` cannot be found
/// - Resource compilation fails
///
/// # Panics
///
/// Panics if `OUT_DIR` is not set (i.e., not called from a build script).
pub fn compile_version_resource(config: &Config) -> Result<(), ConfigError> {
    Ok(compile_version_resource_inner(config)?)
}

/// Inner implementation that uses [`ResourceCompileError`] directly.
fn compile_version_resource_inner(config: &Config) -> Result<(), ResourceCompileError> {
    // Emit rerun-if-env-changed for version-affecting variables
    println!("cargo::rerun-if-env-changed={VERSION_ENV_VAR}");

    let version = resolve_version()?;
    let metadata = read_version_resource_metadata()?;

    let out_dir =
        PathBuf::from(env::var("OUT_DIR").expect(
            "Cargo should have set the OUT_DIR environment variable when executing build.rs",
        ));

    let rc_path = out_dir.join("version.rc");
    let res_path = out_dir.join("version.res");

    // Generate RC file content
    let rc_content = generate_rc_content(&version, &metadata, config);
    fs::write(&rc_path, &rc_content)?;

    // Invoke rc.exe (expected to be on PATH via eWDK prompt or cargo-wdk setup)
    let include_paths = resource_include_paths(config)?;

    let mut cmd = Command::new("rc.exe");
    for path in &include_paths {
        cmd.arg("/I");
        cmd.arg(path);
    }
    cmd.arg("/fo");
    cmd.arg(&res_path);
    cmd.arg(&rc_path);

    let output = cmd.output()?;

    if !output.status.success() {
        return Err(ResourceCompileError::CompilerFailed {
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    // Emit linker directive to include the compiled resource
    println!("cargo::rustc-cdylib-link-arg={}", res_path.display());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::CpuArchitecture;

    // ── Version parsing tests ──────────────────────────────────────────

    #[test]
    fn parse_version_three_part() {
        let v = parse_version("1.2.3").unwrap();
        assert_eq!(
            v,
            DriverVersion {
                major: 1,
                minor: 2,
                patch: 3,
                revision: 0
            }
        );
    }

    #[test]
    fn parse_version_four_part() {
        let v = parse_version("1.2.3.4").unwrap();
        assert_eq!(
            v,
            DriverVersion {
                major: 1,
                minor: 2,
                patch: 3,
                revision: 4
            }
        );
    }

    #[test]
    fn parse_version_strips_prerelease() {
        let v = parse_version("3.0.433-preview").unwrap();
        assert_eq!(
            v,
            DriverVersion {
                major: 3,
                minor: 0,
                patch: 433,
                revision: 0
            }
        );
    }

    #[test]
    fn parse_version_max_values() {
        let v = parse_version("65535.65535.65535.65535").unwrap();
        assert_eq!(
            v,
            DriverVersion {
                major: 65535,
                minor: 65535,
                patch: 65535,
                revision: 65535
            }
        );
    }

    #[test]
    fn parse_version_overflow_rejected() {
        let result = parse_version("65536.0.0");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(
            err,
            ResourceCompileError::VersionParseError { .. }
        ));
    }

    #[test]
    fn parse_version_too_few_parts() {
        assert!(parse_version("1.2").is_err());
    }

    #[test]
    fn parse_version_too_many_parts() {
        assert!(parse_version("1.2.3.4.5").is_err());
    }

    #[test]
    fn parse_version_empty_string() {
        assert!(parse_version("").is_err());
    }

    #[test]
    fn parse_version_non_numeric() {
        assert!(parse_version("1.2.abc").is_err());
    }

    #[test]
    fn parse_version_zero() {
        let v = parse_version("0.0.0").unwrap();
        assert_eq!(
            v,
            DriverVersion {
                major: 0,
                minor: 0,
                patch: 0,
                revision: 0
            }
        );
    }

    // ── DriverVersion formatting tests ────────────────────────────────

    #[test]
    fn windows_version_rc_numeric() {
        let v = DriverVersion {
            major: 1,
            minor: 2,
            patch: 3,
            revision: 4,
        };
        assert_eq!(v.as_rc_numeric(), "1,2,3,4");
    }

    #[test]
    fn windows_version_rc_string() {
        let v = DriverVersion {
            major: 1,
            minor: 2,
            patch: 3,
            revision: 4,
        };
        assert_eq!(v.as_rc_string(), "1.2.3.4");
    }

    // ── RC template generation tests ───────────────────────────────────

    /// Create a test `Config` without relying on `Config::default()` which
    /// requires build-script env vars like `CARGO_CFG_TARGET_ARCH`.
    fn test_config(driver_config: DriverConfig) -> Config {
        Config {
            wdk_content_root: PathBuf::from("C:\\fake\\wdk"),
            cpu_architecture: CpuArchitecture::Amd64,
            driver_config,
        }
    }

    #[test]
    fn generate_rc_content_contains_expected_fields() {
        let version = DriverVersion {
            major: 1,
            minor: 0,
            patch: 0,
            revision: 0,
        };
        let metadata = VersionResourceMetadata {
            company_name: "Test Corp".to_string(),
            copyright: "Copyright Test".to_string(),
            product_name: "Test Product".to_string(),
            file_description: "Test Driver".to_string(),
            internal_name: Some("test.sys".to_string()),
            original_filename: Some("test.sys".to_string()),
        };
        let config = test_config(DriverConfig::Kmdf(crate::KmdfConfig::default()));

        let rc = generate_rc_content(&version, &metadata, &config);

        assert!(rc.contains("#include <windows.h>"));
        assert!(rc.contains("#include <ntverp.h>"));
        assert!(rc.contains("#include \"common.ver\""));
        assert!(rc.contains("VFT_DRV"));
        assert!(rc.contains("VFT2_DRV_SYSTEM"));
        assert!(rc.contains("\"test.sys\""));
        assert!(rc.contains("\"Test Driver\""));
        assert!(rc.contains("\"Test Product\""));
        assert!(rc.contains("\"Test Corp\""));
        assert!(rc.contains("\"Copyright Test\""));
        assert!(rc.contains("1,0,0,0"));
        assert!(rc.contains("\"1.0.0.0\""));
    }

    #[test]
    fn generate_rc_content_umdf_uses_dll_type() {
        let version = DriverVersion {
            major: 1,
            minor: 0,
            patch: 0,
            revision: 0,
        };
        let metadata = VersionResourceMetadata {
            company_name: "Test".to_string(),
            copyright: "Copyright".to_string(),
            product_name: "Product".to_string(),
            file_description: "Driver".to_string(),
            internal_name: None,
            original_filename: None,
        };
        let config = test_config(DriverConfig::Umdf(crate::UmdfConfig::default()));

        let rc = generate_rc_content(&version, &metadata, &config);

        assert!(rc.contains("VFT_DLL"));
        assert!(rc.contains("VFT2_UNKNOWN"));
    }
}
