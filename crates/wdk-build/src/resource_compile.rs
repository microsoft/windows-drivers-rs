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
//! [`Config::configure_binary_build`][crate::Config::configure_binary_build]
//! automatically compiles and links a version resource. Add optional metadata
//! to your driver's `Cargo.toml` to override the Cargo-derived defaults:
//!
//! ```toml
//! [package.metadata.wdk.version-resource]
//! company-name = "Microsoft Corporation"
//! copyright = "Copyright (C) Microsoft Corporation. All rights reserved"
//! product-name = "Surface"
//! file-description = "My Driver"
//! ```
//!
//! # Version Sourcing
//!
//! The version is determined by CI pipeline env var with
//! cargo package version fallback:
//! 1. `STAMPINF_VERSION` environment variable (for CI pipelines)
//! 2. `CARGO_PKG_VERSION` (from `Cargo.toml` `[package]` version)
//!
//! Semver versions are mapped to 4-part Windows versions by appending `.0`
//! for the revision component. Prerelease suffixes (e.g. `-preview`) are
//! stripped. Each component must fit in a 16-bit word (0–65535).

use std::{
    env,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf, absolute},
    process::Command,
};

use crate::{Config, ConfigError, DriverConfig};

/// Environment variable for overriding the driver version in CI pipelines.
///
/// When set, this takes priority over `CARGO_PKG_VERSION`. The value should
/// be in the format `MAJOR.MINOR.PATCH` or `MAJOR.MINOR.PATCH.REVISION`.
/// A prerelease suffix (e.g. `-preview`) is stripped automatically.
const VERSION_ENV_VAR: &str = "STAMPINF_VERSION";

/// A parsed 4-part Windows version number.
///
/// Windows `VERSIONINFO` resources use four 16-bit components:
/// `MAJOR.MINOR.PATCH.REVISION`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DriverVersion {
    /// Major version number
    major: u16,
    /// Minor version number
    minor: u16,
    /// Patch/build version number
    patch: u16,
    /// Revision number
    revision: u16,
}

impl DriverVersion {
    /// Format as a comma-separated string for the `VER_FILEVERSION` RC macro.
    ///
    /// Example: `1,2,3,0`
    fn as_rc_numeric(self) -> String {
        format!(
            "{},{},{},{}",
            self.major, self.minor, self.patch, self.revision
        )
    }

    /// Format as a dot-separated string for the `VER_FILEVERSION_STR` RC macro.
    ///
    /// Example: `"1.2.3.0"`
    fn as_rc_string(self) -> String {
        format!(
            "{}.{}.{}.{}",
            self.major, self.minor, self.patch, self.revision
        )
    }
}

/// Metadata for the version resource, sourced from Cargo defaults and optional
/// `[package.metadata.wdk.version-resource]` overrides in `Cargo.toml`.
#[derive(Debug, Clone)]
struct VersionResourceMetadata {
    /// Company name (e.g. "Microsoft Corporation")
    company_name: String,
    /// Copyright string
    copyright: String,
    /// Product name (e.g. "Surface")
    product_name: String,
    /// File description shown in Explorer properties
    file_description: String,
    /// Internal name of the binary (e.g. "MyDriver.sys")
    internal_name: Option<String>,
    /// Original filename of the binary
    original_filename: Option<String>,
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
    #[error("rc.exe failed with {status}:\n{output}")]
    CompilerFailed {
        /// The exit status of `rc.exe`
        status: std::process::ExitStatus,
        /// The stdout and stderr output from `rc.exe`
        output: String,
    },

    /// Metadata is missing or invalid.
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
fn parse_version(version_str: &str) -> Result<DriverVersion, ResourceCompileError> {
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
                    "expected 3 or 4 dot-separated components, found {} in cleaned version `{}`",
                    parts.len(),
                    version_clean
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
/// `CARGO_PKG_VERSION` env var (emitted by cargo)
/// if env var is not present or empty.
fn resolve_version() -> Result<DriverVersion, ResourceCompileError> {
    let version_str = env_var_non_empty(VERSION_ENV_VAR).map_or_else(
        || {
            env::var("CARGO_PKG_VERSION").map_err(|_| ResourceCompileError::MetadataError {
                detail: "CARGO_PKG_VERSION environment variable not set. This function must be \
                         called from a Cargo build script."
                    .to_string(),
            })
        },
        Ok,
    )?;

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

/// Read version resource metadata from Cargo defaults and optional
/// `[package.metadata.wdk.version-resource]` overrides.
///
/// This reads the current package's `CARGO_MANIFEST_DIR/Cargo.toml`, extracts
/// version-resource metadata using `cargo_metadata`, and fills absent fields
/// from Cargo package environment variables.
///
/// # Errors
///
/// Returns [`ResourceCompileError::MetadataError`] if metadata exists but is
/// invalid.
fn read_version_resource_metadata() -> Result<VersionResourceMetadata, ResourceCompileError> {
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
        .and_then(|w| w.get("version-resource"));
    if let Some(version_resource) = version_resource
        && !version_resource.is_object()
    {
        return Err(ResourceCompileError::MetadataError {
            detail: "[package.metadata.wdk.version-resource] must be a table".to_string(),
        });
    }

    let company_name = version_resource_string(version_resource, "company-name")?
        .or_else(|| env_var_non_empty("CARGO_PKG_AUTHORS"))
        .unwrap_or_default();

    let copyright = version_resource_string(version_resource, "copyright")?.unwrap_or_default();

    let product_name = version_resource_string(version_resource, "product-name")?
        .unwrap_or_else(|| pkg_name.clone());

    let file_description = version_resource_string(version_resource, "file-description")?
        .or_else(|| env::var("CARGO_PKG_DESCRIPTION").ok())
        .filter(|description| !description.is_empty())
        .unwrap_or_else(|| product_name.clone());

    let internal_name = version_resource_string(version_resource, "internal-name")?;

    let original_filename = version_resource_string(version_resource, "original-filename")?;

    // Check both provided and fallback metadata values for invalid characters
    validate_rc_metadata_string("company-name", &company_name)?;
    validate_rc_metadata_string("copyright", &copyright)?;
    validate_rc_metadata_string("product-name", &product_name)?;
    validate_rc_metadata_string("file-description", &file_description)?;
    if let Some(name) = internal_name.as_deref() {
        validate_rc_metadata_string("internal-name", name)?;
    }
    if let Some(name) = original_filename.as_deref() {
        validate_rc_metadata_string("original-filename", name)?;
    }

    Ok(VersionResourceMetadata {
        company_name,
        copyright,
        product_name,
        file_description,
        internal_name,
        original_filename,
    })
}

fn version_resource_string(
    version_resource: Option<&serde_json::Value>,
    key: &str,
) -> Result<Option<String>, ResourceCompileError> {
    let Some(value) = version_resource.and_then(|metadata| metadata.get(key)) else {
        return Ok(None);
    };

    value
        .as_str()
        .map(ToString::to_string)
        .map(Some)
        .ok_or_else(|| ResourceCompileError::MetadataError {
            detail: format!("[package.metadata.wdk.version-resource].{key} must be a string"),
        })
}

/// Validates that a metadata string is safe to interpolate into an RC string
/// literal.
///
/// Rejects characters that could break the generated `.rc` syntax or allow
/// injection of additional RC directives:
/// - `"` — would terminate the RC string literal early.
/// - `\` — could escape the closing quote or alter RC escape sequences.
/// - Any Unicode control character (newlines, tabs, NUL, etc.) — would break
///   the single-line RC string or be silently re-encoded.
///
/// # Errors
///
/// Returns [`ResourceCompileError::MetadataError`] identifying the field and
/// the first offending character (by Unicode code point and byte offset).
fn validate_rc_metadata_string(field: &str, value: &str) -> Result<(), ResourceCompileError> {
    for (offset, ch) in value.char_indices() {
        let reason = if ch == '"' {
            Some("contains a double-quote ('\"') which would terminate the RC string literal")
        } else if ch == '\\' {
            Some(
                "contains a backslash ('\\') which could alter RC escape sequences; use ASCII \
                 forward slashes or omit",
            )
        } else if ch.is_control() {
            Some(
                "contains a control character which cannot be safely embedded in an RC string \
                 literal",
            )
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(ResourceCompileError::MetadataError {
                detail: format!(
                    "version-resource field '{field}' {reason} (offending character U+{:04X} at \
                     byte offset {offset})",
                    ch as u32,
                ),
            });
        }
    }
    Ok(())
}

/// Reads an environment variable, returning `None` for both unset and empty
/// values. eWDK/vcvars occasionally export variables with empty defaults that
/// should be treated as unset.
fn env_var_non_empty(key: &str) -> Option<String> {
    env::var(key).ok().filter(|value| !value.is_empty())
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
fn generate_rc_content(
    version: DriverVersion,
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

/// Builds the `/I` include-path list passed to `rc.exe` when compiling the
/// driver's `VERSIONINFO` resource.
///
/// Paths are collected from two roots so both combined WDK installs (SDK
/// headers nested under the WDK content root) and split eWDK layouts (SDK
/// headers in a separate Windows Kit referenced by `WindowsSdkDir` /
/// `WindowsSdkBinPath`) are supported:
///
/// 1. SDK include root, if locatable from env vars: `um`, `shared`, `ucrt`
/// 2. WDK include root: `um`, `shared`, `ucrt`
/// 3. WDK kernel-mode headers (`km\crt`, `km`) for [`DriverConfig::Wdm`] and
///    [`DriverConfig::Kmdf`]; omitted for [`DriverConfig::Umdf`].
///
/// Missing subdirectories are silently skipped, and duplicates (e.g. when
/// the SDK and WDK roots coincide) are removed.
///
/// # Errors
///
/// Returns [`ResourceCompileError::MetadataError`] if no `um` directory is
/// found in either root (rc.exe would otherwise fail with a cryptic
/// `windows.h: No such file or directory`).
fn resource_include_paths(config: &Config) -> Result<Vec<PathBuf>, ResourceCompileError> {
    let sdk_version = crate::utils::detect_windows_sdk_version(&config.wdk_content_root)
        .map_err(|e| ResourceCompileError::ConfigError(Box::new(e)))?;
    let wdk_include_directory = config.wdk_content_root.join("Include").join(&sdk_version);

    let mut paths = vec![];

    if let Some(sdk_include_directory) = windows_sdk_include_directory(&sdk_version) {
        add_resource_include_paths(&mut paths, &sdk_include_directory)?;
    }

    add_resource_include_paths(&mut paths, &wdk_include_directory)?;

    match config.driver_config {
        DriverConfig::Wdm | DriverConfig::Kmdf(_) => {
            push_existing_absolute_path(&mut paths, &wdk_include_directory.join("km").join("crt"))?;
            push_existing_absolute_path(&mut paths, &wdk_include_directory.join("km"))?;
        }
        DriverConfig::Umdf(_) => {}
    }

    if !paths.iter().any(|p| p.ends_with("um")) {
        return Err(ResourceCompileError::MetadataError {
            detail: format!(
                "Could not locate the Windows SDK 'um' include directory for SDK version \
                 '{sdk_version}'. Looked under '{}' and the directories referenced by the \
                 WindowsSdkDir / WindowsSdkBinPath environment variables. Build from an eWDK \
                 developer prompt, or set WindowsSdkDir to a Windows Kit root that contains \
                 'Include\\{sdk_version}\\um'.",
                wdk_include_directory.display(),
            ),
        });
    }

    Ok(paths)
}

/// Appends the standard user-mode include subdirectories (`um`, `shared`,
/// `ucrt`) of `include_directory` to `paths`. Subdirectories that do not
/// exist on disk are silently skipped.
fn add_resource_include_paths(
    paths: &mut Vec<PathBuf>,
    include_directory: &Path,
) -> Result<(), ResourceCompileError> {
    for include_subdirectory in ["um", "shared", "ucrt"] {
        push_existing_absolute_path(paths, &include_directory.join(include_subdirectory))?;
    }

    Ok(())
}

/// Pushes `path` onto `paths` as an absolute path, but only when it points to
/// an existing directory and is not already present. Uses
/// [`std::path::absolute`] (no filesystem walk) to avoid the `\\?\` UNC
/// prefixes that [`std::fs::canonicalize`] produces, which `rc.exe` mishandles.
fn push_existing_absolute_path(
    paths: &mut Vec<PathBuf>,
    path: &Path,
) -> Result<(), ResourceCompileError> {
    if path.is_dir() {
        let absolute_path = absolute(path)?;
        if !paths.contains(&absolute_path) {
            paths.push(absolute_path);
        }
    }

    Ok(())
}

/// Resolves the Windows SDK include directory for `sdk_version` from
/// environment variables exported by the eWDK developer prompt / vcvars:
///
/// 1. `WindowsSdkDir` — the Windows Kit root (preferred).
/// 2. `WindowsSdkBinPath` — a Kit `bin` subdirectory; the SDK root is found by
///    walking its ancestors.
///
/// Returns `None` if neither variable points to a directory containing
/// `Include\<sdk_version>`.
fn windows_sdk_include_directory(sdk_version: &str) -> Option<PathBuf> {
    env_var_non_empty("WindowsSdkDir")
        .and_then(|windows_sdk_dir| {
            include_directory_from_windows_sdk_root(Path::new(&windows_sdk_dir), sdk_version)
        })
        .or_else(|| {
            env_var_non_empty("WindowsSdkBinPath")
                .and_then(|windows_sdk_bin_path| {
                    find_windows_sdk_root_from_bin_path(
                        Path::new(&windows_sdk_bin_path),
                        sdk_version,
                    )
                })
                .and_then(|windows_sdk_root| {
                    include_directory_from_windows_sdk_root(&windows_sdk_root, sdk_version)
                })
        })
}

/// Returns `<windows_sdk_root>\Include\<sdk_version>` if it exists, else
/// `None`.
fn include_directory_from_windows_sdk_root(
    windows_sdk_root: &Path,
    sdk_version: &str,
) -> Option<PathBuf> {
    let include_directory = windows_sdk_root.join("Include").join(sdk_version);
    include_directory.is_dir().then_some(include_directory)
}

/// Walks the ancestors of `windows_sdk_bin_path` (typically
/// `<root>\bin\<sdk_version>\<arch>`) and returns the first ancestor that
/// contains an `Include\<sdk_version>` directory, i.e. the Windows Kit root.
fn find_windows_sdk_root_from_bin_path(
    windows_sdk_bin_path: &Path,
    sdk_version: &str,
) -> Option<PathBuf> {
    windows_sdk_bin_path
        .ancestors()
        .find(|path| include_directory_from_windows_sdk_root(path, sdk_version).is_some())
        .map(Path::to_path_buf)
}

/// Generate and compile a Windows `VERSIONINFO` resource, then emit a
/// linker directive to embed it in the driver binary.
///
/// This is the main entry point for version resource compilation. It:
/// 1. Reads version from `STAMPINF_VERSION` or `CARGO_PKG_VERSION`
/// 2. Reads metadata from Cargo defaults and optional
///    `[package.metadata.wdk.version-resource]` overrides
/// 3. Generates a WDK-style `.rc` file in `OUT_DIR`
/// 4. Compiles it with `rc.exe`
/// 5. Emits `cargo::rustc-cdylib-link-arg` to link the `.res` into the binary
///
/// # Errors
///
/// Returns [`ConfigError`] (wrapping [`ResourceCompileError`]) if:
/// - Version metadata is invalid
/// - `rc.exe` cannot be found
/// - Resource compilation fails
///
/// # Panics
///
/// Panics if `OUT_DIR` is not set (i.e., not called from a build script).
// `pub(super)` is the minimum visibility needed for `lib.rs` to invoke this
// from `Config::configure_binary_build`. Clippy's `redundant_pub_crate` flags
// it as redundant because the module is private, but removing the visibility
// breaks the call site (E0603), so the lint is a false positive here.
#[allow(
    clippy::redundant_pub_crate,
    reason = "Needed for call from Config::configure_binary_build in lib.rs, flagged because \
              module is private."
)]
pub(super) fn compile_version_resource(config: &Config) -> Result<(), ConfigError> {
    Ok(compile_version_resource_inner(config)?)
}

/// Inner implementation that uses [`ResourceCompileError`] directly.
fn compile_version_resource_inner(config: &Config) -> Result<(), ResourceCompileError> {
    let version = resolve_version()?;
    let metadata = read_version_resource_metadata()?;

    let out_dir =
        PathBuf::from(env::var("OUT_DIR").expect(
            "Cargo should have set the OUT_DIR environment variable when executing build.rs",
        ));

    let rc_path = out_dir.join("version.rc");
    let res_path = out_dir.join("version.res");

    // Generate RC file content
    let rc_content = generate_rc_content(version, &metadata, config);
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
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ResourceCompileError::CompilerFailed {
            status: output.status,
            output: format!("stdout:\n{stdout}\nstderr:\n{stderr}"),
        });
    }

    // Emit linker directive to include the compiled resource
    println!("cargo::rustc-cdylib-link-arg={}", res_path.display());

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::Mutex};

    use assert_fs::prelude::*;

    use super::*;
    use crate::{
        CpuArchitecture,
        utils::{remove_var, set_var},
    };

    struct EnvVarGuard {
        original_values: Vec<(&'static str, Option<String>)>,
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            for (key, value) in &self.original_values {
                match value {
                    Some(value) => set_var(key, value),
                    None => remove_var(key),
                }
            }
        }
    }

    fn with_env<F, R>(env_vars: &[(&'static str, Option<&str>)], f: F) -> R
    where
        F: FnOnce() -> R,
    {
        static ENV_MUTEX: Mutex<()> = Mutex::new(());

        let _mutex_guard = ENV_MUTEX.lock().unwrap();
        let original_values = env_vars
            .iter()
            .map(|(key, _)| (*key, env::var(key).ok()))
            .collect();
        let _env_var_guard = EnvVarGuard { original_values };

        for (key, value) in env_vars {
            match value {
                Some(value) => set_var(key, value),
                None => remove_var(key),
            }
        }

        f()
    }

    fn create_test_crate(cargo_toml: &str) -> assert_fs::TempDir {
        let temp_dir = assert_fs::TempDir::new().unwrap();
        temp_dir.child("Cargo.toml").write_str(cargo_toml).unwrap();
        temp_dir.child("src").create_dir_all().unwrap();
        temp_dir.child("src").child("lib.rs").write_str("").unwrap();
        temp_dir
    }

    mod version_parsing {
        use super::*;

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
    }

    mod version_resolution {
        use super::*;

        #[test]
        fn resolve_version_prefers_stampinf_version() {
            let version = with_env(
                &[
                    (VERSION_ENV_VAR, Some("5.1.0")),
                    ("CARGO_PKG_VERSION", Some("1.2.3")),
                ],
                resolve_version,
            )
            .unwrap();

            assert_eq!(
                version,
                DriverVersion {
                    major: 5,
                    minor: 1,
                    patch: 0,
                    revision: 0
                }
            );
        }

        #[test]
        fn resolve_version_falls_back_to_cargo_pkg_version() {
            let version = with_env(
                &[
                    (VERSION_ENV_VAR, None),
                    ("CARGO_PKG_VERSION", Some("1.2.3")),
                ],
                resolve_version,
            )
            .unwrap();

            assert_eq!(
                version,
                DriverVersion {
                    major: 1,
                    minor: 2,
                    patch: 3,
                    revision: 0
                }
            );
        }

        #[test]
        fn resolve_version_falls_back_to_cargo_pkg_version_when_stampinf_version_is_empty() {
            let version = with_env(
                &[
                    (VERSION_ENV_VAR, Some("")),
                    ("CARGO_PKG_VERSION", Some("1.2.3")),
                ],
                resolve_version,
            )
            .unwrap();

            assert_eq!(
                version,
                DriverVersion {
                    major: 1,
                    minor: 2,
                    patch: 3,
                    revision: 0
                }
            );
        }

        #[test]
        fn resolve_version_errors_without_version_sources() {
            let result = with_env(
                &[(VERSION_ENV_VAR, None), ("CARGO_PKG_VERSION", None)],
                resolve_version,
            );

            assert!(matches!(
                result,
                Err(ResourceCompileError::MetadataError { .. })
            ));
        }
    }

    mod driver_version_formatting {
        use super::*;

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
    }

    mod metadata_resolution {
        use super::*;

        #[test]
        fn read_version_resource_metadata_uses_cargo_defaults_without_overrides() {
            let temp_dir = create_test_crate(
                r#"
                [package]
                name = "test-driver"
                version = "1.2.3"
                edition = "2021"
            "#,
            );
            let manifest_dir = temp_dir.path().to_string_lossy().to_string();

            let metadata = with_env(
                &[
                    ("CARGO_MANIFEST_DIR", Some(&manifest_dir)),
                    ("CARGO_PKG_NAME", Some("test-driver")),
                    ("CARGO_PKG_AUTHORS", Some("Test Authors")),
                    ("CARGO_PKG_DESCRIPTION", Some("Default driver description")),
                ],
                read_version_resource_metadata,
            )
            .unwrap();

            assert_eq!(metadata.company_name, "Test Authors");
            assert_eq!(metadata.copyright, "");
            assert_eq!(metadata.product_name, "test-driver");
            assert_eq!(metadata.file_description, "Default driver description");
            assert_eq!(metadata.internal_name, None);
            assert_eq!(metadata.original_filename, None);
        }

        #[test]
        fn read_version_resource_metadata_defaults_file_description_to_product_name() {
            let temp_dir = create_test_crate(
                r#"
                [package]
                name = "test-driver"
                version = "1.2.3"
                edition = "2021"
            "#,
            );
            let manifest_dir = temp_dir.path().to_string_lossy().to_string();

            let metadata = with_env(
                &[
                    ("CARGO_MANIFEST_DIR", Some(&manifest_dir)),
                    ("CARGO_PKG_NAME", Some("test-driver")),
                    ("CARGO_PKG_AUTHORS", Some("")),
                    ("CARGO_PKG_DESCRIPTION", Some("")),
                ],
                read_version_resource_metadata,
            )
            .unwrap();

            assert_eq!(metadata.company_name, "");
            assert_eq!(metadata.product_name, "test-driver");
            assert_eq!(metadata.file_description, "test-driver");
        }

        #[test]
        fn read_version_resource_metadata_applies_cargo_toml_overrides() {
            let temp_dir = create_test_crate(
                r#"
                [package]
                name = "test-driver"
                version = "1.2.3"
                edition = "2021"

                [package.metadata.wdk.version-resource]
                company-name = "Override Company"
                copyright = "Override Copyright"
                product-name = "Override Product"
                file-description = "Override Driver"
                internal-name = "override.sys"
                original-filename = "original.sys"
            "#,
            );
            let manifest_dir = temp_dir.path().to_string_lossy().to_string();

            let metadata = with_env(
                &[
                    ("CARGO_MANIFEST_DIR", Some(&manifest_dir)),
                    ("CARGO_PKG_NAME", Some("test-driver")),
                    ("CARGO_PKG_AUTHORS", Some("Default Authors")),
                    ("CARGO_PKG_DESCRIPTION", Some("Default Description")),
                ],
                read_version_resource_metadata,
            )
            .unwrap();

            assert_eq!(metadata.company_name, "Override Company");
            assert_eq!(metadata.copyright, "Override Copyright");
            assert_eq!(metadata.product_name, "Override Product");
            assert_eq!(metadata.file_description, "Override Driver");
            assert_eq!(metadata.internal_name, Some("override.sys".to_string()));
            assert_eq!(metadata.original_filename, Some("original.sys".to_string()));
        }

        #[test]
        fn read_version_resource_metadata_rejects_non_table_metadata() {
            let temp_dir = create_test_crate(
                r#"
                [package]
                name = "test-driver"
                version = "1.2.3"
                edition = "2021"

                [package.metadata.wdk]
                version-resource = "invalid"
            "#,
            );
            let manifest_dir = temp_dir.path().to_string_lossy().to_string();

            let result = with_env(
                &[
                    ("CARGO_MANIFEST_DIR", Some(&manifest_dir)),
                    ("CARGO_PKG_NAME", Some("test-driver")),
                ],
                read_version_resource_metadata,
            );

            assert!(matches!(
                result,
                Err(ResourceCompileError::MetadataError { detail })
                    if detail.contains("[package.metadata.wdk.version-resource] must be a table")
            ));
        }

        #[test]
        fn read_version_resource_metadata_rejects_non_string_fields() {
            let temp_dir = create_test_crate(
                r#"
                [package]
                name = "test-driver"
                version = "1.2.3"
                edition = "2021"

                [package.metadata.wdk.version-resource]
                company-name = 42
            "#,
            );
            let manifest_dir = temp_dir.path().to_string_lossy().to_string();

            let result = with_env(
                &[
                    ("CARGO_MANIFEST_DIR", Some(&manifest_dir)),
                    ("CARGO_PKG_NAME", Some("test-driver")),
                ],
                read_version_resource_metadata,
            );

            assert!(matches!(
                result,
                Err(ResourceCompileError::MetadataError { detail })
                    if detail.contains("[package.metadata.wdk.version-resource].company-name must be a string")
            ));
        }

        #[test]
        fn read_version_resource_metadata_rejects_double_quote_injection() {
            let temp_dir = create_test_crate(
                r#"
                [package]
                name = "test-driver"
                version = "1.2.3"
                edition = "2021"

                [package.metadata.wdk.version-resource]
                company-name = '"; #include "evil.rc"; "'
            "#,
            );
            let manifest_dir = temp_dir.path().to_string_lossy().to_string();

            let result = with_env(
                &[
                    ("CARGO_MANIFEST_DIR", Some(&manifest_dir)),
                    ("CARGO_PKG_NAME", Some("test-driver")),
                ],
                read_version_resource_metadata,
            );

            assert!(matches!(
                result,
                Err(ResourceCompileError::MetadataError { detail })
                    if detail.contains("company-name") && detail.contains("double-quote")
            ));
        }

        #[test]
        fn read_version_resource_metadata_rejects_control_character() {
            let temp_dir = create_test_crate(
                r#"
                [package]
                name = "test-driver"
                version = "1.2.3"
                edition = "2021"

                [package.metadata.wdk.version-resource]
                copyright = "line1\nline2"
            "#,
            );
            let manifest_dir = temp_dir.path().to_string_lossy().to_string();

            let result = with_env(
                &[
                    ("CARGO_MANIFEST_DIR", Some(&manifest_dir)),
                    ("CARGO_PKG_NAME", Some("test-driver")),
                ],
                read_version_resource_metadata,
            );

            assert!(matches!(
                result,
                Err(ResourceCompileError::MetadataError { detail })
                    if detail.contains("copyright") && detail.contains("control character")
            ));
        }

        #[test]
        fn read_version_resource_metadata_rejects_backslash() {
            let temp_dir = create_test_crate(
                r#"
                [package]
                name = "test-driver"
                version = "1.2.3"
                edition = "2021"

                [package.metadata.wdk.version-resource]
                file-description = "Driver for C:\\Windows"
            "#,
            );
            let manifest_dir = temp_dir.path().to_string_lossy().to_string();

            let result = with_env(
                &[
                    ("CARGO_MANIFEST_DIR", Some(&manifest_dir)),
                    ("CARGO_PKG_NAME", Some("test-driver")),
                ],
                read_version_resource_metadata,
            );

            assert!(matches!(
                result,
                Err(ResourceCompileError::MetadataError { detail })
                    if detail.contains("file-description") && detail.contains("backslash")
            ));
        }

        #[test]
        fn read_version_resource_metadata_validates_env_var_fallback() {
            let temp_dir = create_test_crate(
                r#"
                [package]
                name = "test-driver"
                version = "1.2.3"
                edition = "2021"
            "#,
            );
            let manifest_dir = temp_dir.path().to_string_lossy().to_string();

            let result = with_env(
                &[
                    ("CARGO_MANIFEST_DIR", Some(&manifest_dir)),
                    ("CARGO_PKG_NAME", Some("test-driver")),
                    ("CARGO_PKG_DESCRIPTION", Some("Driver with \" injection")),
                ],
                read_version_resource_metadata,
            );

            assert!(matches!(
                result,
                Err(ResourceCompileError::MetadataError { detail })
                    if detail.contains("file-description") && detail.contains("double-quote")
            ));
        }
    }

    mod rc_generation {
        use super::*;

        const SDK_VERSION: &str = "10.0.26100.0";

        /// Create a test `Config` without relying on `Config::default()` which
        /// requires build-script env vars like `CARGO_CFG_TARGET_ARCH`.
        fn test_config(driver_config: DriverConfig) -> Config {
            Config {
                wdk_content_root: PathBuf::from("C:\\fake\\wdk"),
                cpu_architecture: CpuArchitecture::Amd64,
                driver_config,
            }
        }

        fn test_config_with_wdk_root(
            driver_config: DriverConfig,
            wdk_content_root: PathBuf,
        ) -> Config {
            Config {
                wdk_content_root,
                cpu_architecture: CpuArchitecture::Amd64,
                driver_config,
            }
        }

        fn create_include_subdirectories(include_directory: &Path, subdirectories: &[&str]) {
            for subdirectory in subdirectories {
                fs::create_dir_all(include_directory.join(subdirectory)).unwrap();
            }
        }

        fn path_string(path: &Path) -> String {
            path.to_string_lossy().to_string()
        }

        fn absolute_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
            paths.iter().map(|path| absolute(path).unwrap()).collect()
        }

        #[test]
        fn resolve_driver_filename_uses_crate_artifact_name_and_driver_extension() {
            let (kmdf_filename, umdf_filename) =
                with_env(&[("CARGO_PKG_NAME", Some("surface-button"))], || {
                    (
                        resolve_driver_filename(&test_config(DriverConfig::Kmdf(
                            crate::KmdfConfig::default(),
                        ))),
                        resolve_driver_filename(&test_config(DriverConfig::Umdf(
                            crate::UmdfConfig::default(),
                        ))),
                    )
                });

            assert_eq!(kmdf_filename, "surface_button.sys");
            assert_eq!(umdf_filename, "surface_button.dll");
        }

        #[test]
        fn resource_include_paths_support_split_sdk_and_wdk_layout_for_kmdf() {
            let temp_dir = assert_fs::TempDir::new().unwrap();
            let sdk_root = temp_dir.path().join("sdk").join("c");
            let sdk_bin_path = sdk_root.join("bin");
            let sdk_include_directory = sdk_root.join("Include").join(SDK_VERSION);
            let wdk_root = temp_dir.path().join("wdk").join("c");
            let wdk_include_directory = wdk_root.join("Include").join(SDK_VERSION);

            fs::create_dir_all(&sdk_bin_path).unwrap();
            create_include_subdirectories(&sdk_include_directory, &["um", "shared", "ucrt"]);
            create_include_subdirectories(&wdk_include_directory, &["shared", "km\\crt", "km"]);

            let config = test_config_with_wdk_root(
                DriverConfig::Kmdf(crate::KmdfConfig::default()),
                wdk_root,
            );
            let sdk_bin_path = path_string(&sdk_bin_path);
            let include_paths = with_env(
                &[
                    ("Version_Number", Some(SDK_VERSION)),
                    ("WindowsSdkDir", None),
                    ("WindowsSdkBinPath", Some(&sdk_bin_path)),
                ],
                || resource_include_paths(&config),
            )
            .unwrap();

            assert_eq!(
                include_paths,
                absolute_paths(&[
                    sdk_include_directory.join("um"),
                    sdk_include_directory.join("shared"),
                    sdk_include_directory.join("ucrt"),
                    wdk_include_directory.join("shared"),
                    wdk_include_directory.join("km").join("crt"),
                    wdk_include_directory.join("km"),
                ])
            );
        }

        #[test]
        fn resource_include_paths_omits_kernel_paths_for_umdf() {
            let temp_dir = assert_fs::TempDir::new().unwrap();
            let sdk_root = temp_dir.path().join("sdk").join("c");
            let sdk_bin_path = sdk_root.join("bin");
            let sdk_include_directory = sdk_root.join("Include").join(SDK_VERSION);
            let wdk_root = temp_dir.path().join("wdk").join("c");
            let wdk_include_directory = wdk_root.join("Include").join(SDK_VERSION);

            fs::create_dir_all(&sdk_bin_path).unwrap();
            create_include_subdirectories(&sdk_include_directory, &["um", "shared", "ucrt"]);
            create_include_subdirectories(&wdk_include_directory, &["shared", "km\\crt", "km"]);

            let config = test_config_with_wdk_root(
                DriverConfig::Umdf(crate::UmdfConfig::default()),
                wdk_root,
            );
            let sdk_bin_path = path_string(&sdk_bin_path);
            let include_paths = with_env(
                &[
                    ("Version_Number", Some(SDK_VERSION)),
                    ("WindowsSdkDir", None),
                    ("WindowsSdkBinPath", Some(&sdk_bin_path)),
                ],
                || resource_include_paths(&config),
            )
            .unwrap();

            assert_eq!(
                include_paths,
                absolute_paths(&[
                    sdk_include_directory.join("um"),
                    sdk_include_directory.join("shared"),
                    sdk_include_directory.join("ucrt"),
                    wdk_include_directory.join("shared"),
                ])
            );
        }

        #[test]
        fn resource_include_paths_errors_when_um_directory_missing() {
            let temp_dir = assert_fs::TempDir::new().unwrap();
            let wdk_root = temp_dir.path().join("wdk").join("c");
            let wdk_include_directory = wdk_root.join("Include").join(SDK_VERSION);
            create_include_subdirectories(&wdk_include_directory, &["shared", "km\\crt", "km"]);

            let config = test_config_with_wdk_root(
                DriverConfig::Kmdf(crate::KmdfConfig::default()),
                wdk_root,
            );

            let result = with_env(
                &[
                    ("Version_Number", Some(SDK_VERSION)),
                    ("WindowsSdkDir", None),
                    ("WindowsSdkBinPath", None),
                ],
                || resource_include_paths(&config),
            );

            assert!(matches!(
                result,
                Err(ResourceCompileError::MetadataError { detail })
                    if detail.contains("'um'") && detail.contains("WindowsSdkDir")
            ));
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

            let rc = generate_rc_content(version, &metadata, &config);

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

            let rc = generate_rc_content(version, &metadata, &config);

            assert!(rc.contains("VFT_DLL"));
            assert!(rc.contains("VFT2_UNKNOWN"));
        }
    }
}
