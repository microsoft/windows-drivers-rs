// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Utilities for `cargo-make` tasks used to package binaries dependent on the
//! `WDK`.
//!
//! This module provides functions used in the rust scripts in
//! `rust-driver-makefile.toml`. This includes argument parsing functionality
//! used by `rust-driver-makefile.toml` to validate and forward arguments common
//! to cargo commands. It uses a combination of `clap` and `clap_cargo` to
//! provide a CLI very close to cargo's own, but only exposes the arguments
//! supported by `rust-driver-makefile.toml`.

use core::{fmt, ops::RangeFrom};
use std::{
    env,
    panic::UnwindSafe,
    path::{Path, PathBuf, absolute},
    process::Command,
};

use anyhow::Context;
use cargo_metadata::{Metadata, MetadataCommand, camino::Utf8Path};
use clap::{Args, ColorChoice, CommandFactory, FromArgMatches, Parser};
use tracing::{instrument, trace, warn};

use crate::{
    ConfigError,
    CpuArchitecture,
    IoError,
    metadata,
    utils::{detect_wdk_content_root, detect_windows_sdk_version, get_wdk_version_number, set_var},
};

/// The filename of the main makefile for Rust Windows drivers.
pub const RUST_DRIVER_MAKEFILE_NAME: &str = "rust-driver-makefile.toml";
/// The filename of the samples makefile for Rust Windows drivers.
pub const RUST_DRIVER_SAMPLE_MAKEFILE_NAME: &str = "rust-driver-sample-makefile.toml";

const PATH_ENV_VAR: &str = "Path";
/// The environment variable that [`setup_wdk_version`] stores the WDK version
/// in.
pub const WDK_VERSION_ENV_VAR: &str = "WDK_BUILD_DETECTED_VERSION";
/// The first WDK version with the new `InfVerif` behavior.
const MINIMUM_SAMPLES_FLAG_WDK_VERSION: i32 = 25798;
const WDK_INF_ADDITIONAL_FLAGS_ENV_VAR: &str = "WDK_BUILD_ADDITIONAL_INFVERIF_FLAGS";
const WDK_BUILD_OUTPUT_DIRECTORY_ENV_VAR: &str = "WDK_BUILD_OUTPUT_DIRECTORY";

/// The name of the environment variable that cargo-make uses during `cargo
/// build` and `cargo test` commands
const CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR: &str = "CARGO_MAKE_CARGO_BUILD_TEST_FLAGS";

const CARGO_MAKE_DISABLE_COLOR_ENV_VAR: &str = "CARGO_MAKE_DISABLE_COLOR";
const CARGO_MAKE_PROFILE_ENV_VAR: &str = "CARGO_MAKE_PROFILE";
const CARGO_MAKE_CARGO_PROFILE_ENV_VAR: &str = "CARGO_MAKE_CARGO_PROFILE";
const CARGO_MAKE_CRATE_TARGET_TRIPLE_ENV_VAR: &str = "CARGO_MAKE_CRATE_TARGET_TRIPLE";
const CARGO_MAKE_CRATE_CUSTOM_TRIPLE_TARGET_DIRECTORY_ENV_VAR: &str =
    "CARGO_MAKE_CRATE_CUSTOM_TRIPLE_TARGET_DIRECTORY";
const CARGO_MAKE_RUST_DEFAULT_TOOLCHAIN_ENV_VAR: &str = "CARGO_MAKE_RUST_DEFAULT_TOOLCHAIN";
const CARGO_MAKE_CRATE_NAME_ENV_VAR: &str = "CARGO_MAKE_CRATE_NAME";
const CARGO_MAKE_CRATE_FS_NAME_ENV_VAR: &str = "CARGO_MAKE_CRATE_FS_NAME";
const CARGO_MAKE_WORKSPACE_WORKING_DIRECTORY_ENV_VAR: &str =
    "CARGO_MAKE_WORKSPACE_WORKING_DIRECTORY";
const CARGO_MAKE_CURRENT_TASK_NAME_ENV_VAR: &str = "CARGO_MAKE_CURRENT_TASK_NAME";

/// `clap` uses an exit code of 2 for usage errors: <https://github.com/clap-rs/clap/blob/14fd853fb9c5b94e371170bbd0ca2bf28ef3abff/clap_builder/src/util/mod.rs#L30C18-L30C28>
const CLAP_USAGE_EXIT_CODE: i32 = 2;

// This range is inclusive of 25798. FIXME: update with range end after /sample
// flag is added to InfVerif CLI
const MISSING_SAMPLE_FLAG_WDK_BUILD_NUMBER_RANGE: RangeFrom<u32> = 25798..;

trait ParseCargoArgs {
    fn parse_cargo_args(&self);
}

#[derive(Parser, Debug)]
#[command(styles = clap_cargo::style::CLAP_STYLING)]
struct CommandLineInterface {
    #[command(flatten)]
    base: BaseOptions,

    #[command(flatten)]
    #[command(next_help_heading = "Package Selection")]
    workspace: clap_cargo::Workspace,

    #[command(flatten)]
    #[command(next_help_heading = "Feature Selection")]
    features: clap_cargo::Features,

    #[command(flatten)]
    compilation_options: CompilationOptions,

    #[command(flatten)]
    manifest_options: ManifestOptions,
}

#[derive(Args, Debug)]
struct BaseOptions {
    #[arg(long, help = "Do not print cargo log messages")]
    quiet: bool,

    #[arg(short, long, action = clap::ArgAction::Count, help = "Use verbose output (-vv very verbose/build.rs output)")]
    verbose: u8,
}

#[derive(Args, Debug)]
#[command(next_help_heading = "Compilation Options")]
struct CompilationOptions {
    #[arg(
        short,
        long,
        help = "Build artifacts in release mode, with optimizations"
    )]
    release: bool,

    #[arg(
        long,
        value_name = "PROFILE-NAME",
        help = "Build artifacts with the specified profile"
    )]
    profile: Option<String>,

    #[arg(
        short,
        long,
        value_name = "N",
        allow_negative_numbers = true,
        help = "Number of parallel jobs, defaults to # of CPUs."
    )]
    jobs: Option<String>,

    // FIXME: support building multiple targets at once
    #[arg(long, value_name = "TRIPLE", help = "Build for a target triple")]
    target: Option<String>,

    #[allow(clippy::option_option)] // This is how clap_derive expects "optional value for optional argument" args
    #[arg(
        long,
        value_name = "FMTS",
        require_equals = true,
        help = "Timing output formats (unstable) (comma separated): html, json"
    )]
    timings: Option<Option<String>>,
}

#[derive(Args, Debug)]
#[command(next_help_heading = "Manifest Options")]
struct ManifestOptions {
    #[arg(long, help = "Require Cargo.lock and cache are up to date")]
    frozen: bool,

    #[arg(long, help = "Require Cargo.lock is up to date")]
    locked: bool,

    #[arg(long, help = "Run without accessing the network")]
    offline: bool,
}

impl ParseCargoArgs for CommandLineInterface {
    fn parse_cargo_args(&self) {
        let Self {
            base,
            workspace,
            features,
            compilation_options,
            manifest_options,
        } = self;

        base.parse_cargo_args();
        workspace.parse_cargo_args();
        features.parse_cargo_args();
        compilation_options.parse_cargo_args();
        manifest_options.parse_cargo_args();
    }
}

impl ParseCargoArgs for BaseOptions {
    fn parse_cargo_args(&self) {
        let Self { quiet, verbose } = self;

        if *quiet && *verbose > 0 {
            eprintln!("Cannot specify both --quiet and --verbose");
            std::process::exit(CLAP_USAGE_EXIT_CODE);
        }

        if *quiet {
            append_to_space_delimited_env_var(CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR, "--quiet");
        }

        if *verbose > 0 {
            append_to_space_delimited_env_var(
                CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                format!("-{}", "v".repeat((*verbose).into())).as_str(),
            );
        }
    }
}

impl ParseCargoArgs for clap_cargo::Workspace {
    fn parse_cargo_args(&self) {
        let Self {
            package,
            workspace,
            all,
            exclude,
            ..
        } = self;

        if !package.is_empty() {
            append_to_space_delimited_env_var(
                CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                package
                    .iter()
                    .fold(
                        String::with_capacity({
                            const MINIMUM_PACKAGE_SPEC_LENGTH: usize = 1;
                            const MINIMUM_PACKAGE_ARG_LENGTH: usize =
                                "--package ".len() + MINIMUM_PACKAGE_SPEC_LENGTH + " ".len();
                            package.len() * MINIMUM_PACKAGE_ARG_LENGTH
                        }),
                        |mut package_args, package_spec| {
                            package_args.push_str("--package ");
                            package_args.push_str(package_spec);
                            package_args.push(' ');
                            package_args
                        },
                    )
                    .trim_end(),
            );
        }

        if *workspace {
            append_to_space_delimited_env_var(
                CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                "--workspace",
            );
        }

        if !exclude.is_empty() {
            if !*workspace {
                eprintln!("--exclude can only be used together with --workspace");
                std::process::exit(CLAP_USAGE_EXIT_CODE);
            }

            append_to_space_delimited_env_var(
                CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                exclude
                    .iter()
                    .fold(
                        String::with_capacity({
                            const MINIMUM_PACKAGE_SPEC_LENGTH: usize = 1;
                            const MINIMUM_EXCLUDE_ARG_LENGTH: usize =
                                "--exclude ".len() + MINIMUM_PACKAGE_SPEC_LENGTH + " ".len();
                            package.len() * MINIMUM_EXCLUDE_ARG_LENGTH
                        }),
                        |mut exclude_args, package_spec| {
                            exclude_args.push_str("--exclude ");
                            exclude_args.push_str(package_spec);
                            exclude_args.push(' ');
                            exclude_args
                        },
                    )
                    .trim_end(),
            );
        }

        if *all {
            append_to_space_delimited_env_var(CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR, "--all");
        }
    }
}

impl ParseCargoArgs for clap_cargo::Features {
    fn parse_cargo_args(&self) {
        let Self {
            all_features,
            no_default_features,
            features,
            ..
        } = self;
        if *all_features {
            append_to_space_delimited_env_var(
                CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                "--all-features",
            );
        }

        if *no_default_features {
            append_to_space_delimited_env_var(
                CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                "--no-default-features",
            );
        }

        if !features.is_empty() {
            append_to_space_delimited_env_var(
                CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                features
                    .iter()
                    .fold(
                        String::with_capacity({
                            const MINIMUM_FEATURE_NAME_LENGTH: usize = 1;
                            const MINIMUM_FEATURE_ARG_LENGTH: usize =
                                "--features ".len() + MINIMUM_FEATURE_NAME_LENGTH + " ".len();
                            features.len() * MINIMUM_FEATURE_ARG_LENGTH
                        }),
                        |mut feature_args: String, feature| {
                            feature_args.push_str("--features ");
                            feature_args.push_str(feature);
                            feature_args.push(' ');
                            feature_args
                        },
                    )
                    .trim_end(),
            );
        }
    }
}

impl ParseCargoArgs for CompilationOptions {
    fn parse_cargo_args(&self) {
        let Self {
            release,
            profile,
            jobs,
            target,
            timings,
        } = self;
        if *release && profile.is_some() {
            eprintln!("the `--release` flag should not be specified with the `--profile` flag");
            std::process::exit(CLAP_USAGE_EXIT_CODE);
        }
        let cargo_make_cargo_profile = match env::var(CARGO_MAKE_PROFILE_ENV_VAR)
            .unwrap_or_else(|_| panic!("{CARGO_MAKE_PROFILE_ENV_VAR} should be set by cargo-make"))
            .as_str()
        {
            "release" => {
                // cargo-make release profile sets the `--profile release` flag
                if let Some(profile) = &profile {
                    if profile != "release" {
                        eprintln!(
                            "Specifying `--profile release` for cargo-make conflicts with the \
                             setting `--profile {profile}` to forward to tasks"
                        );
                        std::process::exit(CLAP_USAGE_EXIT_CODE);
                    }
                }

                append_to_space_delimited_env_var(
                    CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                    "--profile release",
                );
                "release".to_string()
            }
            _ => {
                // All other cargo-make profiles do not set a specific cargo profile. Cargo
                // profiles set by --release, --profile <PROFILE>, or -p <PROFILE> (after
                // the cargo-make task name) are forwarded to cargo
                // commands
                if *release {
                    append_to_space_delimited_env_var(
                        CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                        "--release",
                    );
                    "release".to_string()
                } else if let Some(profile) = &profile {
                    append_to_space_delimited_env_var(
                        CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                        format!("--profile {profile}").as_str(),
                    );
                    profile.into()
                } else {
                    env::var(CARGO_MAKE_CARGO_PROFILE_ENV_VAR).unwrap_or_else(|_| {
                        panic!("{CARGO_MAKE_CARGO_PROFILE_ENV_VAR} should be set by cargo-make")
                    })
                }
            }
        };

        set_var(CARGO_MAKE_CARGO_PROFILE_ENV_VAR, &cargo_make_cargo_profile);

        if let Some(jobs) = &jobs {
            append_to_space_delimited_env_var(
                CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                format!("--jobs {jobs}").as_str(),
            );
        }

        if let Some(target) = &target {
            set_var(CARGO_MAKE_CRATE_TARGET_TRIPLE_ENV_VAR, target);
            append_to_space_delimited_env_var(
                CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                format!("--target {target}").as_str(),
            );
        }

        configure_wdf_build_output_dir(target.as_ref(), &cargo_make_cargo_profile);

        if let Some(timings_option) = &timings {
            timings_option.as_ref().map_or_else(
                || {
                    append_to_space_delimited_env_var(
                        CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                        "--timings",
                    );
                },
                |timings_value| {
                    append_to_space_delimited_env_var(
                        CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                        format!("--timings {timings_value}").as_str(),
                    );
                },
            );
        }
    }
}

impl ParseCargoArgs for ManifestOptions {
    fn parse_cargo_args(&self) {
        let Self {
            frozen,
            locked,
            offline,
        } = self;

        if *frozen {
            append_to_space_delimited_env_var(
                CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                "--frozen",
            );
        }

        if *locked {
            append_to_space_delimited_env_var(
                CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                "--locked",
            );
        }

        if *offline {
            append_to_space_delimited_env_var(
                CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                "--offline",
            );
        }
    }
}

/// Parses the command line arguments, validates that they are supported by
/// `rust-driver-makefile.toml`, and then returns a list of environment variable
/// names that were updated.
///
/// These environment variable names should be passed to
/// [`forward_printed_env_vars`] to forward values to cargo-make.
///
/// # Panics
///
/// This function will panic if there's an internal error (i.e. bug) in its
/// argument processing.
#[must_use]
pub fn validate_command_line_args() -> impl IntoIterator<Item = String> {
    const TOOLCHAIN_ARG_POSITION: usize = 1;

    let mut env_args = env::args_os().collect::<Vec<_>>();

    // +<toolchain> is a special argument that can't currently be handled by clap parsing: https://github.com/clap-rs/clap/issues/2468
    let toolchain_arg = if env_args
        .get(TOOLCHAIN_ARG_POSITION)
        .is_some_and(|arg| arg.to_string_lossy().starts_with('+'))
    {
        Some(
            env_args
                .remove(TOOLCHAIN_ARG_POSITION)
                .to_string_lossy()
                .strip_prefix('+')
                .expect("Toolchain arg should have a + prefix")
                .to_string(),
        )
    } else {
        None
    };

    if let Some(toolchain) = toolchain_arg {
        set_var(CARGO_MAKE_RUST_DEFAULT_TOOLCHAIN_ENV_VAR, toolchain);
    }

    CommandLineInterface::from_arg_matches_mut(
        &mut CommandLineInterface::command()
            .color(if is_cargo_make_color_disabled() {
                ColorChoice::Never
            } else {
                // `ColorChoice::Always` is used instead of `ColorChoice::Auto` to force color.
                // This function is always executed from rust-script invoked by cargo-make,
                // whose piping of stdout/stderr disables color by default.
                ColorChoice::Always
            })
            .get_matches_from(env_args),
    )
    .unwrap_or_else(|err| err.exit())
    .parse_cargo_args();

    [
        CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
        CARGO_MAKE_CARGO_PROFILE_ENV_VAR,
        CARGO_MAKE_CRATE_TARGET_TRIPLE_ENV_VAR,
        CARGO_MAKE_RUST_DEFAULT_TOOLCHAIN_ENV_VAR,
        WDK_BUILD_OUTPUT_DIRECTORY_ENV_VAR,
    ]
    .into_iter()
    .filter(|env_var_name| env::var_os(env_var_name).is_some())
    .map(ToString::to_string)
}

fn is_cargo_make_color_disabled() -> bool {
    env::var(CARGO_MAKE_DISABLE_COLOR_ENV_VAR).is_ok_and(|value| {
        !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            // when color is enabled in cargo-make, the env var is guaranteed to be set to one
            // of the below values, or not be set at all
            "0" | "false" | "no" | ""
        )
    })
}

/// Prepends the path variable with the necessary paths to access WDK(+SDK)
/// tools.
///
/// # Errors
///
/// This function returns a [`ConfigError::WdkContentRootDetectionError`] if the
/// WDK content root directory could not be found.
///
/// # Panics
///
/// This function will panic if the CPU architecture cannot be determined from
/// [`env::consts::ARCH`] or if the PATH variable contains non-UTF8
/// characters.
pub fn setup_path() -> Result<impl IntoIterator<Item = String>, ConfigError> {
    let wdk_content_root =
        detect_wdk_content_root().ok_or(ConfigError::WdkContentRootDetectionError)?;

    let sdk_version = detect_windows_sdk_version(&wdk_content_root)?;

    let host_arch = CpuArchitecture::try_from_cargo_str(env::consts::ARCH)
        .expect("The rust standard library should always set env::consts::ARCH");

    let wdk_bin_root = get_wdk_bin_root(&wdk_content_root, &sdk_version);

    let host_windows_sdk_ver_bin_path = {
        let path = wdk_bin_root.join(host_arch.as_windows_str());
        absolute(&path).map_err(|source| IoError::with_path(path, source))?
    }
    .to_str()
    .expect("WDK bin path should be valid UTF-8")
    .to_string();

    let x86_windows_sdk_ver_bin_path = {
        let path = wdk_bin_root.join("x86");
        absolute(&path).map_err(|source| IoError::with_path(path, source))?
    }
    .to_str()
    .expect("WDK x86 bin path should be valid UTF-8")
    .to_string();

    if let Ok(sdk_bin_path) = env::var("WindowsSdkBinPath") {
        let sdk_bin_path = {
            let path = PathBuf::from(sdk_bin_path)
                .join(&sdk_version)
                .join(host_arch.as_windows_str());
            absolute(&path).map_err(|source| IoError::with_path(path, source))?
        }
        .to_str()
        .expect("WindowsSdkBinPath should be valid UTF-8")
        .to_string();
        prepend_to_semicolon_delimited_env_var(PATH_ENV_VAR, sdk_bin_path);
    }

    prepend_to_semicolon_delimited_env_var(
        PATH_ENV_VAR,
        format!("{host_windows_sdk_ver_bin_path};{x86_windows_sdk_ver_bin_path}"),
    );

    let wdk_tool_root = get_wdk_tools_root(&wdk_content_root, &sdk_version);
    let host_windows_sdk_version_tool_path = {
        let path = wdk_tool_root.join(host_arch.as_windows_str());
        absolute(&path).map_err(|source| IoError::with_path(path, source))?
    }
    .to_str()
    .expect("WDK tool path should be valid UTF-8")
    .to_string();
    prepend_to_semicolon_delimited_env_var(PATH_ENV_VAR, host_windows_sdk_version_tool_path);

    Ok([PATH_ENV_VAR].map(ToString::to_string))
}

fn get_wdk_tools_root(wdk_content_root: &Path, sdk_version: &str) -> PathBuf {
    get_path_from_env("WDKToolRoot", wdk_content_root, "tools", sdk_version)
}

fn get_wdk_bin_root(wdk_content_root: &Path, sdk_version: &str) -> PathBuf {
    get_path_from_env("WDKBinRoot", wdk_content_root, "bin", sdk_version)
}

/// Reads path from the given env variable or falls back to
/// constructing it from WDK content root.
///
/// The path in `env_var` is already the full path because eWDK
/// and Nuget CLI set it that way. In the fallback however we
/// have to append `sub_folder` and `sdk_version` manually.
fn get_path_from_env(
    env_var: &str,
    wdk_content_root: &Path,
    sub_folder: &str,
    sdk_version: &str,
) -> PathBuf {
    env::var(env_var).map_or_else(
        |e| {
            const FALLBACK_MSG: &str = "Constructing path from WDK content root instead";
            match e {
                env::VarError::NotPresent => {
                    trace!("Env var '{env_var}' not found. {FALLBACK_MSG}");
                }
                env::VarError::NotUnicode(val) => {
                    warn!(
                        "Env var '{env_var}' contains non-UTF8 characters: {val:?}. {FALLBACK_MSG}"
                    );
                }
            }
            wdk_content_root.join(sub_folder).join(sdk_version)
        },
        PathBuf::from,
    )
}

/// Forwards the specified environment variables in this process to the parent
/// cargo-make. This is facilitated by printing to `stdout`, and having the
/// `rust-env-update` plugin parse the printed output.
///
/// # Panics
///
/// Panics if any of the `env_vars` do not exist or contain a non-UTF8 value.
pub fn forward_printed_env_vars(env_vars: impl IntoIterator<Item = impl AsRef<str>>) {
    // This print signifies the start of the forwarding and signals to the
    // `rust-env-update` plugin that it should forward args
    println!("FORWARDING ARGS TO CARGO-MAKE:");

    for env_var_name in env_vars {
        let env_var_name = env_var_name.as_ref();

        // Since this executes in a child process to cargo-make, we need to forward the
        // values we want to change to duckscript, in order to get it to modify the
        // parent process (ie. cargo-make)
        println!(
            "{env_var_name}={}",
            env::var(env_var_name).unwrap_or_else(|_| panic!(
                "{env_var_name} should be the name of an environment variable that is set and \
                 contains a valid UTF-8 value"
            ))
        );
    }

    // This print signifies the end of the forwarding and signals to the
    // `rust-env-update` plugin that it should stop forwarding args
    println!("END OF FORWARDING ARGS TO CARGO-MAKE");
}

/// Adds the WDK version to the environment in the full string form of
/// 10.xxx.yyy.zzz, where x, y, and z are numerical values.
///
/// # Errors
///
/// This function returns a [`ConfigError::WdkContentRootDetectionError`] if the
/// WDK content root directory could not be found, or if the WDK version is
/// ill-formed.
pub fn setup_wdk_version() -> Result<impl IntoIterator<Item = String>, ConfigError> {
    let Some(wdk_content_root) = detect_wdk_content_root() else {
        return Err(ConfigError::WdkContentRootDetectionError);
    };

    let detected_sdk_version = detect_windows_sdk_version(&wdk_content_root)?;

    if let Ok(existing_version) = std::env::var(WDK_VERSION_ENV_VAR) {
        if detected_sdk_version == existing_version {
            // Skip updating.  This can happen in certain recursive
            // cargo-make cases.
            return Ok([WDK_VERSION_ENV_VAR].map(ToString::to_string));
        }
        // We have a bad version string set somehow.  Return an error.
        return Err(ConfigError::WdkContentRootDetectionError);
    }

    if !crate::utils::validate_wdk_version_format(&detected_sdk_version) {
        return Err(ConfigError::WdkVersionStringFormatError {
            version: detected_sdk_version,
        });
    }

    set_var(WDK_VERSION_ENV_VAR, detected_sdk_version);
    Ok([WDK_VERSION_ENV_VAR].map(ToString::to_string))
}

/// Sets the `WDK_INFVERIF_SAMPLE_FLAG` environment variable to contain the
/// appropriate flag for building samples.
///
/// # Errors
///
/// This function returns a [`ConfigError::WdkContentRootDetectionError`] if
/// an invalid WDK version is provided.
///
/// # Panics
///
/// This function will panic if the function for validating a WDK version string
/// is ever changed to no longer validate that each part of the version string
/// is an i32.
pub fn setup_infverif_for_samples<S: AsRef<str> + ToString + ?Sized>(
    version: &S,
) -> Result<impl IntoIterator<Item = String>, ConfigError> {
    let validated_version_string = crate::utils::get_wdk_version_number(version)?;

    // Safe to unwrap as we called .parse::<i32>().is_ok() in our call to
    // validate_wdk_version_format above.
    let version = validated_version_string
        .parse::<i32>()
        .expect("Unable to parse the build number of the WDK version string as an int!");
    let sample_flag = if version > MINIMUM_SAMPLES_FLAG_WDK_VERSION {
        // Note: Not currently implemented, so in samples TOML we currently skip
        // infverif
        "/samples"
    } else {
        "/msft"
    };
    append_to_space_delimited_env_var(WDK_INF_ADDITIONAL_FLAGS_ENV_VAR, sample_flag);

    Ok([WDK_INF_ADDITIONAL_FLAGS_ENV_VAR].map(ToString::to_string))
}

/// Returns the path to the WDK build output directory for the current
/// cargo-make flow
///
/// # Panics
///
/// This function will panic if the `WDK_BUILD_OUTPUT_DIRECTORY` environment
/// variable is not set
#[must_use]
pub fn get_wdk_build_output_directory() -> PathBuf {
    PathBuf::from(
        env::var("WDK_BUILD_OUTPUT_DIRECTORY")
            .expect("WDK_BUILD_OUTPUT_DIRECTORY should have been set by the wdk-build-init task"),
    )
}

/// Returns the name of the current cargo package cargo-make is processing
///
/// # Panics
///
/// This function will panic if the `CARGO_MAKE_CRATE_FS_NAME` environment
/// variable is not set
#[must_use]
pub fn get_current_package_name() -> String {
    env::var(CARGO_MAKE_CRATE_FS_NAME_ENV_VAR).unwrap_or_else(|_| {
        panic!(
            "{} should be set by cargo-make",
            &CARGO_MAKE_CRATE_FS_NAME_ENV_VAR
        )
    })
}

/// Copies the file or directory at `path_to_copy` to the Driver Package folder
///
/// # Errors
///
/// This function returns a [`ConfigError::IoError`] if the it encounters IO
/// errors while copying the file or creating the directory
///
/// # Panics
///
/// This function will panic if `path_to_copy` does end with a valid file or
/// directory name
pub fn copy_to_driver_package_folder<P: AsRef<Path>>(path_to_copy: P) -> Result<(), ConfigError> {
    let path_to_copy = path_to_copy.as_ref();

    let package_folder_path: PathBuf =
        get_wdk_build_output_directory().join(format!("{}_package", get_current_package_name()));
    if !package_folder_path.exists() {
        std::fs::create_dir(&package_folder_path)
            .map_err(|source| IoError::with_path(&package_folder_path, source))?;
    }

    let destination_path = package_folder_path.join(
        path_to_copy
            .file_name()
            .expect("path_to_copy should always end with a valid file or directory name"),
    );
    std::fs::copy(path_to_copy, &destination_path)
        .map_err(|source| IoError::with_src_dest_paths(path_to_copy, destination_path, source))?;

    Ok(())
}

/// Makes `rust-driver-makefile.toml` available in the `target` folder where it
/// can be extended from a `Makefile.toml`.
///
/// This is necessary so that paths in the `rust-driver-makefile.toml` can be
/// relative to `CARGO_MAKE_CURRENT_TASK_INITIAL_MAKEFILE_DIRECTORY`.
///
/// When `wdk-build` is a registry dependency, the makefile is copied with its
/// `wdk-build = { path = "." }` dependency rewritten to a versioned registry
/// dependency. When it is a path or git dependency, it is symlinked instead.
///
/// # Errors
///
/// This function returns:
/// - [`ConfigError::CargoMetadataError`] if there is an error executing or
///   parsing `cargo_metadata`
/// - [`ConfigError::NoWdkBuildCrateDetected`] if `wdk-build` is not found in
///   the dependency graph
/// - [`ConfigError::MultipleWdkBuildCratesDetected`] if there are multiple
///   versions of the WDK build crate detected
/// - [`ConfigError::IoError`] if there is an error reading, writing, or
///   symlinking the makefile
///
/// # Panics
///
/// This function will panic if the `CARGO_MAKE_WORKSPACE_WORKING_DIRECTORY`
/// environment variable is not set
pub fn load_rust_driver_makefile() -> Result<(), ConfigError> {
    load_wdk_build_makefile(RUST_DRIVER_MAKEFILE_NAME)
}

/// Makes `rust-driver-sample-makefile.toml` available in the `target` folder
/// where it can be extended from a `Makefile.toml`.
///
/// This is necessary so that paths in the `rust-driver-sample-makefile.toml`
/// can be relative to `CARGO_MAKE_CURRENT_TASK_INITIAL_MAKEFILE_DIRECTORY`.
///
/// When `wdk-build` is a registry dependency, the makefile is copied with its
/// `wdk-build = { path = "." }` dependency rewritten to a versioned registry
/// dependency. When it is a path or git dependency, it is symlinked instead.
///
/// # Errors
///
/// This function returns:
/// - [`ConfigError::CargoMetadataError`] if there is an error executing or
///   parsing `cargo_metadata`
/// - [`ConfigError::NoWdkBuildCrateDetected`] if `wdk-build` is not found in
///   the dependency graph
/// - [`ConfigError::MultipleWdkBuildCratesDetected`] if there are multiple
///   versions of the WDK build crate detected
/// - [`ConfigError::IoError`] if there is an error reading, writing, or
///   symlinking the makefile
///
/// # Panics
///
/// This function will panic if the `CARGO_MAKE_WORKSPACE_WORKING_DIRECTORY`
/// environment variable is not set
pub fn load_rust_driver_sample_makefile() -> Result<(), ConfigError> {
    load_wdk_build_makefile(RUST_DRIVER_SAMPLE_MAKEFILE_NAME)
}

/// Makes a [`wdk_build`] `cargo-make` makefile available in the `target`
/// folder where it can be extended from a downstream `Makefile.toml`.
///
/// When `wdk-build` is a **registry dependency**, the makefile is copied and
/// its embedded `rust-script` `wdk-build = { path = "." }` dependency is
/// rewritten to a versioned registry dependency (`wdk-build = "X.Y.Z"`).
/// This ensures cargo applies `--cap-lints allow` to the published `wdk-build`
/// crate, preventing the caller's `RUSTFLAGS` from leaking into its
/// compilation.
///
/// When `wdk-build` is a **path or git dependency**, the makefile is symlinked
/// so that `path = "."` resolves correctly via `--base-path`, preserving the
/// user's intent to build against their local `wdk-build` source.
///
/// # Custom registries
///
/// The rewritten dependency uses a bare version requirement (e.g.
/// `wdk-build = "0.5.1"`) which resolves from the default registry
/// (crates.io). If `wdk-build` is consumed from a non-crates.io registry
/// declared via `[registries]` in `.cargo/config.toml`, a warning is emitted.
/// Users in this situation should prefer a `[source.crates-io] replace-with`
/// configuration, which transparently redirects crates.io lookups to their
/// custom registry without affecting dependency resolution.
///
/// The version of `wdk-build` from which the file comes is determined by the
/// working directory of the process that invokes this function. For example,
/// if this function is ultimately executing in a `cargo_make` `load_script`,
/// the files will come from the `wdk-build` version that is in the
/// `Cargo.lock` file, and not the `wdk-build` version specified in the
/// `load_script`.
///
/// # Errors
///
/// This function returns:
/// - [`ConfigError::CargoMetadataError`] if there is an error executing or
///   parsing `cargo_metadata`
/// - [`ConfigError::MultipleWdkBuildCratesDetected`] if there are multiple
///   versions of the WDK build crate detected
/// - [`ConfigError::IoError`] if there is an error creating or updating the
///   symlink to the makefile.
///
/// # Panics
///
/// This function will panic if the `CARGO_MAKE_WORKSPACE_WORKING_DIRECTORY`
/// environment variable is not set
#[instrument(level = "trace")]
fn load_wdk_build_makefile<S: AsRef<str> + AsRef<Utf8Path> + AsRef<Path> + fmt::Debug>(
    makefile_name: S,
) -> Result<(), ConfigError> {
    let cargo_metadata = MetadataCommand::new().exec()?;
    trace!(cargo_metadata_output = ?cargo_metadata);

    let wdk_build_package_matches = cargo_metadata
        .packages
        .into_iter()
        .filter(|package| package.name == "wdk-build")
        .collect::<Vec<_>>();

    match wdk_build_package_matches.len() {
        0 => {
            return Err(ConfigError::NoWdkBuildCrateDetected);
        }
        1 => {}
        _ => {
            return Err(ConfigError::MultipleWdkBuildCratesDetected {
                package_ids: wdk_build_package_matches
                    .iter()
                    .map(|package_info| package_info.id.clone())
                    .collect(),
            });
        }
    }

    let wdk_build_package = &wdk_build_package_matches[0];

    let rust_driver_makefile_toml_path = wdk_build_package
        .manifest_path
        .parent()
        .expect("The parsed manifest_path should have a valid parent directory")
        .join(&makefile_name)
        .into_std_path_buf();

    let cargo_make_workspace_working_directory =
        env::var(CARGO_MAKE_WORKSPACE_WORKING_DIRECTORY_ENV_VAR).unwrap_or_else(|_| {
            panic!("{CARGO_MAKE_WORKSPACE_WORKING_DIRECTORY_ENV_VAR} should be set by cargo-make")
        });

    let destination_path = Path::new(&cargo_make_workspace_working_directory)
        .join("target")
        .join(&makefile_name);

    let is_registry_source = wdk_build_package
        .source
        .as_ref()
        .is_some_and(|s| s.repr.starts_with("registry+") || s.repr.starts_with("sparse+"));

    if is_registry_source {
        // Warn if the source is a non-crates.io registry (e.g. a private ADO
        // Artifacts feed declared via [registries] without source replacement).
        // The rewrite produces a bare version dep that resolves from crates.io,
        // which may fail if the user can only reach their custom registry.
        let src = wdk_build_package
            .source
            .as_ref()
            .expect("source should be Some when is_registry_source is true");
        if !src.is_crates_io() {
            warn!(
                "wdk-build was resolved from a non-crates.io registry ({repr}). The rust-script \
                 dependency will be rewritten to a bare version requirement that resolves from \
                 crates.io. If crates.io is not reachable or does not have this version, the \
                 build will fail. Consider using a [source.crates-io] replace-with in \
                 .cargo/config.toml instead of a custom registry.",
                repr = src.repr,
            );
        }

        // wdk-build is a registry dependency. Rewrite path dependencies in the
        // makefile's rust-script embedded manifests to version-only dependencies
        // so that cargo treats wdk-build as a registry dep and applies
        // --cap-lints. This prevents the caller's RUSTFLAGS (e.g. -D warnings)
        // from leaking into the published wdk-build crate's compilation.
        let makefile_content = std::fs::read_to_string(&rust_driver_makefile_toml_path)
            .map_err(|source| IoError::with_path(&rust_driver_makefile_toml_path, source))?;

        let version = &wdk_build_package.version;
        let patched_content = rewrite_wdk_build_path_deps_to_version(&makefile_content, version);

        if patched_content == makefile_content {
            warn!(
                "No wdk-build path dependency found to rewrite in {makefile_name:?}. The makefile \
                 format may have changed."
            );
        }

        // Only write if content changed or destination doesn't exist, to
        // avoid unnecessary rebuilds from rust-script cache invalidation.
        // NOTE: symlink_metadata is used instead of exists() because
        // exists() returns false for dangling symlinks left over from a
        // previous path-dep run, which we need to detect and replace.
        let path_occupied = destination_path.symlink_metadata().is_ok();

        let should_write = if path_occupied && !destination_path.is_symlink() {
            let existing_content = std::fs::read_to_string(&destination_path)
                .map_err(|source| IoError::with_path(&destination_path, source))?;
            existing_content != patched_content
        } else {
            true
        };

        if should_write {
            if path_occupied {
                std::fs::remove_file(&destination_path)
                    .map_err(|source| IoError::with_path(&destination_path, source))?;
            }
            std::fs::write(&destination_path, patched_content)
                .map_err(|source| IoError::with_path(&destination_path, source))?;
        }
    } else {
        // wdk-build is a path dependency, workspace member, or git dependency.
        // Keep the symlink so that path = "." resolves correctly via
        // --base-path, preserving the user's intent to build against their
        // local wdk-build source.
        // NOTE: symlink_metadata is used instead of exists() because
        // exists() returns false for dangling symlinks left over from a
        // previous registry-dep run, which we need to detect and replace.
        let path_occupied = destination_path.symlink_metadata().is_ok();

        if !path_occupied {
            std::os::windows::fs::symlink_file(&rust_driver_makefile_toml_path, &destination_path)
                .map_err(|source| {
                    IoError::with_src_dest_paths(
                        rust_driver_makefile_toml_path,
                        destination_path,
                        source,
                    )
                })?;
        } else if !destination_path.is_symlink()
            || std::fs::read_link(&destination_path)
                .map_err(|source| IoError::with_path(&destination_path, source))?
                != rust_driver_makefile_toml_path
        {
            std::fs::remove_file(&destination_path)
                .map_err(|source| IoError::with_path(&destination_path, source))?;
            std::os::windows::fs::symlink_file(&rust_driver_makefile_toml_path, &destination_path)
                .map_err(|source| {
                    IoError::with_src_dest_paths(
                        rust_driver_makefile_toml_path,
                        destination_path,
                        source,
                    )
                })?;
        }
        // Symlink is already up to date
    }

    Ok(())
}

/// Rewrites `wdk-build = { path = "." }` dependency specs in a makefile's
/// content to version-only registry dependencies (`wdk-build = "X.Y.Z"`).
///
/// Returns the patched content. If no replacements were made, the content is
/// returned unchanged.
fn rewrite_wdk_build_path_deps_to_version(
    makefile_content: &str,
    version: &semver::Version,
) -> String {
    makefile_content.replace(
        r#"wdk-build = { path = "." }"#,
        &format!("wdk-build = \"{version}\""),
    )
}

/// Get [`cargo_metadata::Metadata`] based off of manifest in
/// `CARGO_MAKE_WORKING_DIRECTORY`
///
/// # Errors
///
/// This function will return a [`cargo_metadata::Error`] if `cargo_metadata`
/// fails
///
/// # Panics
///
/// This function will panic if executed outside of a `cargo-make` task
pub fn get_cargo_metadata() -> cargo_metadata::Result<Metadata> {
    let manifest_path = {
        let mut p: PathBuf = std::path::PathBuf::from(
            std::env::var("CARGO_MAKE_WORKING_DIRECTORY")
                .expect("CARGO_MAKE_WORKING_DIRECTORY should be set by cargo-make"),
        );
        p.push("Cargo.toml");
        p
    };

    cargo_metadata::MetadataCommand::new()
        .manifest_path(manifest_path)
        .exec()
}

/// Execute a `FnOnce` closure, and handle its contents in a way compatible with
/// `cargo-make`'s `condition_script`:
/// 1. If the closure panics, the panic is caught and it returns an `Ok(())`.
///    This ensures that panics encountered in `condition_script_closure` will
///    not default to skipping the task.
/// 2. If the closure executes without panicking, forward the result to
///    `cargo-make`. `Ok` types will result in the task being run, and `Err`
///    types will print the `Err` contents and then skip the task.
///
/// If you want your task to be skipped, return an `Err` from
/// `condition_script_closure`. If you want the task to execute, return an
/// `Ok(())` from `condition_script_closure`
///
/// # Errors
///
/// This function returns an error whenever `condition_script_closure` returns
/// an error
///
/// # Panics
///
/// Panics if `CARGO_MAKE_CURRENT_TASK_NAME` is not set in the environment
pub fn condition_script<F, E>(condition_script_closure: F) -> anyhow::Result<(), E>
where
    F: FnOnce() -> anyhow::Result<(), E> + UnwindSafe,
{
    std::panic::catch_unwind(condition_script_closure).unwrap_or_else(|_| {
        // Note: Any panic messages has already been printed by this point

        let cargo_make_task_name = env::var(CARGO_MAKE_CURRENT_TASK_NAME_ENV_VAR)
            .expect("CARGO_MAKE_CURRENT_TASK_NAME should be set by cargo-make");

        eprintln!(
            r#"`condition_script` for "{cargo_make_task_name}" task panicked while executing. \
             Defaulting to running "{cargo_make_task_name}" task."#
        );
        Ok(())
    })
}

/// `cargo-make` condition script for `package-driver-flow` task in
/// [`rust-driver-makefile.toml`](../rust-driver-makefile.toml)
///
/// # Errors
///
/// This function returns an error whenever it determines that the
/// `package-driver-flow` `cargo-make` task should be skipped (i.e. when the
/// current package isn't a cdylib depending on the WDK, or when no valid WDK
/// configurations are detected)
///
/// # Panics
///
/// Panics if `CARGO_MAKE_CURRENT_TASK_NAME` is not set in the environment
pub fn package_driver_flow_condition_script() -> anyhow::Result<()> {
    condition_script(|| {
        // Get the current package name via `CARGO_MAKE_CRATE_NAME_ENV_VAR` instead of
        // `CARGO_MAKE_CRATE_FS_NAME_ENV_VAR`, since `cargo_metadata` output uses the
        // non-preprocessed name (ie. - instead of _)
        let current_package_name = env::var(CARGO_MAKE_CRATE_NAME_ENV_VAR).unwrap_or_else(|_| {
            panic!(
                "{} should be set by cargo-make",
                &CARGO_MAKE_CRATE_NAME_ENV_VAR
            )
        });
        let cargo_metadata = get_cargo_metadata()?;

        // Skip task if the current crate is not a driver (i.e. a cdylib with a
        // `package.metadata.wdk` section)
        let current_package = cargo_metadata
            .packages
            .iter()
            .find(|package| package.name == current_package_name)
            .expect("The current package should be present in the cargo metadata output");
        if current_package.metadata["wdk"].is_null() {
            return Err::<(), anyhow::Error>(
                metadata::TryFromCargoMetadataError::NoWdkConfigurationsDetected.into(),
            )
            .with_context(|| {
                "Skipping package-driver-flow cargo-make task because the current crate does not \
                 have a package.metadata.wdk section"
            });
        }
        if !current_package
            .targets
            .iter()
            .any(|target| target.kind.contains(&cargo_metadata::TargetKind::CDyLib))
        {
            return Err::<(), anyhow::Error>(
                metadata::TryFromCargoMetadataError::NoWdkConfigurationsDetected.into(),
            )
            .with_context(|| {
                "Skipping package-driver-flow cargo-make task because the current crate does not \
                 contain a cdylib target"
            });
        }

        match metadata::Wdk::try_from(&cargo_metadata) {
            Err(e @ metadata::TryFromCargoMetadataError::NoWdkConfigurationsDetected) => {
                // Skip task only if no WDK configurations are detected
                Err::<(), anyhow::Error>(e.into()).with_context(|| {
                    "Skipping package-driver-flow cargo-make task because the current crate is not \
                     a driver"
                })
            }

            Ok(_) => Ok(()),

            Err(unexpected_error) => {
                eprintln!("Unexpected error: {unexpected_error:#?}");
                // Do not silently skip task if unexpected error in parsing WDK Metadata occurs
                Ok(())
            }
        }
    })
}

/// `cargo-make` condition script for `generate-certificate` task in
/// [`rust-driver-makefile.toml`](../rust-driver-makefile.toml)
///
/// # Errors
///
/// This functions returns an error whenever it determines that the
/// `generate-certificate` `cargo-make` task should be skipped. This only
/// occurs when `WdrLocalTestCert` already exists in `WDRTestCertStore`.
///
/// # Panics
///
/// Panics if `CARGO_MAKE_CURRENT_TASK_NAME` is not set in the environment.
pub fn generate_certificate_condition_script() -> anyhow::Result<()> {
    condition_script(|| {
        let mut command = Command::new("certmgr");

        command.args([
            "-put".as_ref(),
            "-s".as_ref(),
            "WDRTestCertStore".as_ref(),
            "-c".as_ref(),
            "-n".as_ref(),
            "WdrLocalTestCert".as_ref(),
            get_wdk_build_output_directory()
                .join("WDRLocalTestCert.cer")
                .as_os_str(),
        ]);

        let output = command.output().unwrap_or_else(|err| {
            panic!(
                "Failed to run certmgr.exe {} due to error: {}",
                command
                    .get_args()
                    .map(|arg| arg.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(" "),
                err
            )
        });

        match output.status.code() {
            Some(0) => Err(anyhow::anyhow!(
                "WDRLocalTestCert found in WDRTestCertStore. Skipping certificate generation."
            )),
            Some(1) => {
                eprintln!(
                    "WDRLocalTestCert not found in WDRTestCertStore. Generating new certificate."
                );
                Ok(())
            }
            Some(_) => {
                eprintln!("Unknown status code found from certmgr. Generating new certificate.");
                Ok(())
            }
            None => {
                unreachable!("Unreachable, no status code found from certmgr.");
            }
        }
    })
}

fn configure_wdf_build_output_dir(target_arg: Option<&String>, cargo_make_cargo_profile: &str) {
    let cargo_make_crate_custom_triple_target_directory =
        env::var(CARGO_MAKE_CRATE_CUSTOM_TRIPLE_TARGET_DIRECTORY_ENV_VAR).unwrap_or_else(|_| {
            panic!(
                "{CARGO_MAKE_CRATE_CUSTOM_TRIPLE_TARGET_DIRECTORY_ENV_VAR} should be set by \
                 cargo-make."
            )
        });

    let wdk_build_output_directory = {
        let mut output_dir = cargo_make_crate_custom_triple_target_directory;

        // Providing the "--target" flag causes the build output to go into a subdirectory: https://doc.rust-lang.org/cargo/guide/build-cache.html#build-cache
        if let Some(target) = target_arg {
            output_dir += "/";
            output_dir += target;
        }

        if cargo_make_cargo_profile == "dev" {
            // Cargo puts "dev" profile builds in the "debug" target folder: https://doc.rust-lang.org/cargo/guide/build-cache.html#build-cache.
            // This also supports cargo-make profile of "development" since cargo-make maps
            // CARGO_MAKE_PROFILE value of "development" to CARGO_MAKE_CARGO_PROFILE of
            // "dev".
            output_dir += "/debug";
        } else {
            output_dir += "/";
            output_dir += cargo_make_cargo_profile;
        }

        output_dir
    };
    set_var(
        WDK_BUILD_OUTPUT_DIRECTORY_ENV_VAR,
        wdk_build_output_directory,
    );
}

fn append_to_space_delimited_env_var<S, T>(env_var_name: S, string_to_append: T)
where
    S: AsRef<str>,
    T: AsRef<str>,
{
    let env_var_name: &str = env_var_name.as_ref();
    let string_to_append: &str = string_to_append.as_ref();

    let mut env_var_value: String = env::var(env_var_name).unwrap_or_default();
    env_var_value.push(' ');
    env_var_value.push_str(string_to_append);
    set_var(env_var_name, env_var_value.trim());
}

fn prepend_to_semicolon_delimited_env_var<S, T>(env_var_name: S, string_to_prepend: T)
where
    S: AsRef<str>,
    T: AsRef<str>,
{
    let env_var_name = env_var_name.as_ref();
    let string_to_prepend = string_to_prepend.as_ref();

    let mut env_var_value = string_to_prepend.to_string();
    env_var_value.push(';');
    env_var_value.push_str(env::var(env_var_name).unwrap_or_default().as_str());
    set_var(env_var_name, env_var_value);
}

/// `cargo-make` condition script for `infverif` task in
/// [`rust-driver-sample-makefile.toml`](../rust-driver-sample-makefile.toml)
///
/// # Errors
///
/// This function returns an error whenever it determines that the
/// `infverif` `cargo-make` task should be skipped (i.e. when the WDK Version is
/// bugged and does not contain /samples flag)
///
/// # Panics
/// Panics if `CARGO_MAKE_CURRENT_TASK_NAME` is not set in the environment
pub fn driver_sample_infverif_condition_script() -> anyhow::Result<()> {
    condition_script(|| {
        let wdk_version = env::var(WDK_VERSION_ENV_VAR).expect(
            "WDK_BUILD_DETECTED_VERSION should always be set by wdk-build-init cargo make task",
        );
        let wdk_build_number = str::parse::<u32>(
            &get_wdk_version_number(&wdk_version).expect("Failed to get WDK version number"),
        )
        .unwrap_or_else(|_| {
            panic!("Couldn't parse WDK version number! Version number: {wdk_version}")
        });
        if MISSING_SAMPLE_FLAG_WDK_BUILD_NUMBER_RANGE.contains(&wdk_build_number) {
            // cargo_make will interpret returning an error from the rust-script
            // condition_script as skipping the task
            return Err::<(), anyhow::Error>(anyhow::Error::msg(format!(
                "Skipping InfVerif. InfVerif in WDK Build {wdk_build_number} is bugged and does \
                 not contain the /samples flag.",
            )));
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    mod setup_infverif_for_samples {
        use crate::ConfigError;

        const WDK_TEST_OLD_INF_VERSION: &str = "10.0.22061.0";
        const WDK_TEST_NEW_INF_VERSION: &str = "10.0.26100.0";

        #[test]
        fn sample_drivers_flag_selection() -> Result<(), ConfigError> {
            // Old version should map to /msft
            crate::cargo_make::setup_infverif_for_samples(WDK_TEST_OLD_INF_VERSION)?;
            let env_string = std::env::var_os(crate::cargo_make::WDK_INF_ADDITIONAL_FLAGS_ENV_VAR)
                .map_or_else(
                    || panic!("Couldn't get OS string"),
                    |os_env_string| os_env_string.to_string_lossy().into_owned(),
                );
            assert_eq!(env_string.split(' ').next_back(), Some("/msft"));

            // Newer version should map to /samples
            crate::cargo_make::setup_infverif_for_samples(WDK_TEST_NEW_INF_VERSION)?;
            let env_string = std::env::var_os(crate::cargo_make::WDK_INF_ADDITIONAL_FLAGS_ENV_VAR)
                .map_or_else(
                    || panic!("Couldn't get OS string"),
                    |os_env_string| os_env_string.to_string_lossy().into_owned(),
                );
            assert_eq!(env_string.split(' ').next_back(), Some("/samples"));
            Ok(())
        }
    }

    mod setup_path {
        use std::env;

        use assert_fs::TempDir;

        use super::super::{PathBuf, absolute};
        use crate::CpuArchitecture;

        /// Create a minimal fake WDK directory layout needed for path
        /// canonicalization.
        fn setup_test_wdk_layout(temp: &TempDir, sdk_version: &str, host_arch: &str) -> PathBuf {
            let wdk_content_root = temp.path().to_path_buf();
            let lib_version_path = wdk_content_root.join("Lib").join(sdk_version);
            std::fs::create_dir_all(&lib_version_path).unwrap();

            let bin_root_versioned = wdk_content_root.join("bin").join(sdk_version);
            std::fs::create_dir_all(bin_root_versioned.join(host_arch)).unwrap();
            std::fs::create_dir_all(bin_root_versioned.join("x86")).unwrap();

            let tools_root_versioned = wdk_content_root.join("tools").join(sdk_version);
            std::fs::create_dir_all(tools_root_versioned.join(host_arch)).unwrap();

            wdk_content_root
        }

        /// Convert a list of `PathBufs` to their absolute string
        /// representations
        fn expected_path_strings<I>(paths: I) -> Vec<String>
        where
            I: IntoIterator<Item = PathBuf>,
        {
            paths
                .into_iter()
                .map(|path| absolute(path).unwrap().to_string_lossy().into_owned())
                .collect()
        }

        /// Run a single test case for `setup_path`, setting the given env vars
        /// and verifying that the expected PATH components are present in
        /// order.
        fn run_setup_path_testcase(
            env_vars: &[(&str, Option<PathBuf>)],
            expected_paths: &[String],
        ) {
            crate::tests::with_env(env_vars, || {
                let result = super::super::setup_path()
                    .expect("setup_path should succeed for the test layout");
                let returned: Vec<String> = result.into_iter().collect();
                assert_eq!(
                    returned,
                    vec!["Path"],
                    "setup_path should return that only PATH was modified"
                );

                let path_value = std::env::var("Path").expect("Path should be set");
                let mut parts = path_value.split(';');
                for expected in expected_paths {
                    assert_eq!(parts.next(), Some(expected.as_str()));
                }
            });
        }

        #[test]
        fn without_wdk_root_env_vars() {
            // Create test WDK directory layout
            let temp = TempDir::new().unwrap();
            let sdk_version = "10.0.1.0";
            let host_cpu_arch = CpuArchitecture::try_from_cargo_str(env::consts::ARCH).unwrap();
            let host_arch = host_cpu_arch.as_windows_str();
            let wdk_content_root = setup_test_wdk_layout(&temp, sdk_version, host_arch);

            // Calculate expected PATH components based on default WDK structure.
            // When WDKBinRoot/WDKToolRoot are not set, setup_path constructs paths from
            // WDKContentRoot.
            let expected_paths = expected_path_strings(vec![
                wdk_content_root
                    .join("tools")
                    .join(sdk_version)
                    .join(host_arch),
                wdk_content_root
                    .join("bin")
                    .join(sdk_version)
                    .join(host_arch),
                wdk_content_root.join("bin").join(sdk_version).join("x86"),
            ]);

            run_setup_path_testcase(
                &[
                    ("WDKContentRoot", Some(wdk_content_root)),
                    ("WDKBinRoot", None),
                    ("WDKToolRoot", None),
                    ("Version_Number", None),
                    ("WindowsSdkBinPath", None),
                ],
                &expected_paths,
            );
        }

        #[test]
        fn with_wdk_root_env_vars() {
            // Create test WDK directory layout
            let temp = TempDir::new().unwrap();
            let sdk_version = "10.0.1.0";
            let host_cpu_arch = CpuArchitecture::try_from_cargo_str(env::consts::ARCH).unwrap();
            let host_arch = host_cpu_arch.as_windows_str();
            let wdk_content_root = setup_test_wdk_layout(&temp, sdk_version, host_arch);

            // When WDKBinRoot/WDKToolRoot are set (eWDK/NuGet scenario), they should point
            // to their respective versioned folders
            let bin_root_versioned = wdk_content_root.join("bin").join(sdk_version);
            let tools_root_versioned = wdk_content_root.join("tools").join(sdk_version);
            let expected_paths = expected_path_strings(vec![
                tools_root_versioned.join(host_arch),
                bin_root_versioned.join(host_arch),
                bin_root_versioned.join("x86"),
            ]);

            run_setup_path_testcase(
                &[
                    ("WDKContentRoot", Some(wdk_content_root)),
                    ("WDKBinRoot", Some(bin_root_versioned)),
                    ("WDKToolRoot", Some(tools_root_versioned)),
                    ("Version_Number", None),
                    ("WindowsSdkBinPath", None),
                ],
                &expected_paths,
            );
        }
    }

    mod rewrite_wdk_build_path_deps_to_version {
        use semver::Version;

        use super::super::rewrite_wdk_build_path_deps_to_version;

        #[test]
        fn rewrites_path_dep_to_version() {
            let input = r#"
//! ```cargo
//! [dependencies]
//! wdk-build = { path = "." }
//! ```
"#;
            let version = Version::new(0, 5, 1);
            let result = rewrite_wdk_build_path_deps_to_version(input, &version);
            let expected = r#"
//! ```cargo
//! [dependencies]
//! wdk-build = "0.5.1"
//! ```
"#;
            assert_eq!(result, expected);
        }

        #[test]
        fn rewrites_across_multiple_rust_script_blocks() {
            // The shipped makefiles contain many independent rust-script
            // blocks, each with their own `[dependencies]` section. All
            // occurrences must be rewritten.
            let input = r#"
[tasks.first-task]
script_runner = "@rust"
script = '''
//! ```cargo
//! [dependencies]
//! wdk-build = { path = "." }
//! ```
fn main() {}
'''

[tasks.second-task]
script_runner = "@rust"
script = '''
//! ```cargo
//! [dependencies]
//! wdk-build = { path = "." }
//! ```
fn main() {}
'''
"#;
            let version = Version::new(1, 2, 3);
            let result = rewrite_wdk_build_path_deps_to_version(input, &version);
            let expected = r#"
[tasks.first-task]
script_runner = "@rust"
script = '''
//! ```cargo
//! [dependencies]
//! wdk-build = "1.2.3"
//! ```
fn main() {}
'''

[tasks.second-task]
script_runner = "@rust"
script = '''
//! ```cargo
//! [dependencies]
//! wdk-build = "1.2.3"
//! ```
fn main() {}
'''
"#;
            assert_eq!(result, expected);
        }

        #[test]
        fn no_match_returns_unchanged() {
            let input = r#"
//! wdk-build = "0.5.1"
some other content
"#;
            let version = Version::new(0, 5, 1);
            let result = rewrite_wdk_build_path_deps_to_version(input, &version);
            assert_eq!(result, input);
        }

        #[test]
        fn does_not_rewrite_when_extra_keys_present() {
            // Intentionally conservative: the literal string match only
            // rewrites the exact `wdk-build = { path = "." }` pattern.
            // A dep spec with extra keys (e.g. version) is left untouched
            // to avoid silently dropping constraints. If this happens with
            // shipped makefiles, load_wdk_build_makefile emits a warning.
            let input = r#"//! wdk-build = { path = ".", version = "0.5.1" }"#;
            let version = Version::new(0, 5, 1);
            let result = rewrite_wdk_build_path_deps_to_version(input, &version);
            assert_eq!(result, input);
        }
    }

    /// Characterization tests for [`cargo_metadata::Source`] behavior that
    /// the production registry-detection logic in
    /// [`load_wdk_build_makefile`](super::super::load_wdk_build_makefile)
    /// depends on. These validate our assumptions about `Source::repr`
    /// prefixes and `Source::is_crates_io()`.
    mod registry_source_detection {
        use cargo_metadata::Source;

        fn source(repr: &str) -> Source {
            serde_json::from_value(serde_json::json!(repr)).unwrap()
        }

        fn is_non_crates_io_registry(repr: &str) -> bool {
            let src = source(repr);
            (repr.starts_with("registry+") || repr.starts_with("sparse+")) && !src.is_crates_io()
        }

        #[test]
        fn crates_io_git_index_is_not_flagged() {
            // NOTE: Cargo normalises crates.io to this canonical URL in cargo
            // metadata output regardless of the fetch protocol (sparse or git).
            // See `SourceId::crates_io()` and `RegistrySourceIds` in Cargo.
            assert!(
                !is_non_crates_io_registry("registry+https://github.com/rust-lang/crates.io-index"),
                "crates.io canonical URL should not be flagged as custom registry"
            );
        }

        #[test]
        fn custom_sparse_registry_is_flagged() {
            assert!(
                is_non_crates_io_registry(
                    "sparse+https://pkgs.dev.azure.com/MSFTDEVICES/_packaging/PublicRustPackages/Cargo/index/"
                ),
                "ADO Artifacts sparse registry should be flagged"
            );
        }

        #[test]
        fn custom_registry_is_flagged() {
            assert!(
                is_non_crates_io_registry("registry+https://my-corp-registry.example.com/index"),
                "custom registry should be flagged"
            );
        }

        #[test]
        fn path_source_is_not_flagged() {
            assert!(
                !is_non_crates_io_registry("path+file:///home/user/wdk-build"),
                "path source should not be flagged as registry"
            );
        }

        #[test]
        fn git_source_is_not_flagged() {
            assert!(
                !is_non_crates_io_registry("git+https://github.com/microsoft/windows-drivers-rs"),
                "git source should not be flagged as registry"
            );
        }
    }

    mod shipped_makefile_integrity {
        /// The exact pattern that [`rewrite_wdk_build_path_deps_to_version`]
        /// replaces. If this string is absent from a shipped makefile, the
        /// rewrite silently becomes a no-op and RUSTFLAGS isolation breaks.
        const REWRITABLE_PATTERN: &str = r#"wdk-build = { path = "." }"#;

        #[test]
        fn rust_driver_makefile_contains_rewritable_pattern() {
            let content = include_str!("../rust-driver-makefile.toml");
            assert!(
                content.contains(REWRITABLE_PATTERN),
                "rust-driver-makefile.toml must contain the rewritable pattern \
                 {REWRITABLE_PATTERN:?} for RUSTFLAGS isolation to work"
            );
        }

        #[test]
        fn rust_driver_sample_makefile_contains_rewritable_pattern() {
            let content = include_str!("../rust-driver-sample-makefile.toml");
            assert!(
                content.contains(REWRITABLE_PATTERN),
                "rust-driver-sample-makefile.toml must contain the rewritable pattern \
                 {REWRITABLE_PATTERN:?} for RUSTFLAGS isolation to work"
            );
        }
    }

    /// Shared helpers for [`load_rust_driver_makefile`] and
    /// [`load_rust_driver_sample_makefile`] tests. These tests run against
    /// the real workspace (so `cargo metadata` resolves `wdk-build` as a
    /// path dependency) but use a temporary directory for
    /// `CARGO_MAKE_WORKSPACE_WORKING_DIRECTORY` to isolate filesystem
    /// side-effects.
    fn create_temp_target_dir(temp: &assert_fs::TempDir) -> std::path::PathBuf {
        let target_dir = temp.path().join("target");
        std::fs::create_dir_all(&target_dir).unwrap();
        target_dir
    }

    fn load_makefile_in_temp_dir(temp: &assert_fs::TempDir) -> std::path::PathBuf {
        let target_dir = create_temp_target_dir(temp);

        let ws_dir = temp.path().to_string_lossy().into_owned();
        crate::tests::with_env(
            &[(
                super::CARGO_MAKE_WORKSPACE_WORKING_DIRECTORY_ENV_VAR,
                Some(&ws_dir),
            )],
            || {
                super::load_rust_driver_makefile()
                    .expect("load_rust_driver_makefile should succeed in the workspace");
            },
        );

        target_dir.join(super::RUST_DRIVER_MAKEFILE_NAME)
    }

    mod load_rust_driver_makefile {
        use assert_fs::TempDir;

        use super::super::RUST_DRIVER_MAKEFILE_NAME;

        #[test]
        fn creates_symlink_for_path_dep() {
            let temp = TempDir::new().unwrap();
            let dest = super::load_makefile_in_temp_dir(&temp);

            assert!(dest.exists(), "makefile should exist at {dest:?}");
            assert!(
                dest.is_symlink(),
                "makefile should be a symlink for path deps"
            );

            let content = std::fs::read_to_string(&dest).unwrap();
            assert!(
                content.contains(r#"wdk-build = { path = "." }"#),
                "symlinked makefile should still contain path dep"
            );
        }

        #[test]
        fn idempotent_on_second_call() {
            let temp = TempDir::new().unwrap();
            let dest = super::load_makefile_in_temp_dir(&temp);

            let metadata_before = std::fs::symlink_metadata(&dest).unwrap();

            let ws_dir = temp.path().to_string_lossy().into_owned();
            crate::tests::with_env(
                &[(
                    super::super::CARGO_MAKE_WORKSPACE_WORKING_DIRECTORY_ENV_VAR,
                    Some(&ws_dir),
                )],
                || {
                    super::super::load_rust_driver_makefile().expect("second call should succeed");
                },
            );

            let metadata_after = std::fs::symlink_metadata(&dest).unwrap();
            assert_eq!(
                metadata_before.file_type(),
                metadata_after.file_type(),
                "file type should not change on idempotent call"
            );
        }

        #[test]
        fn replaces_dangling_symlink() {
            let temp = TempDir::new().unwrap();
            let target_dir = super::create_temp_target_dir(&temp);
            let dest = target_dir.join(RUST_DRIVER_MAKEFILE_NAME);

            let nonexistent = temp.path().join("does-not-exist.toml");
            std::os::windows::fs::symlink_file(&nonexistent, &dest)
                .expect("should be able to create symlink");
            assert!(
                dest.symlink_metadata().is_ok(),
                "dangling symlink should be detectable via symlink_metadata"
            );
            assert!(!dest.exists(), "dangling symlink target should not exist");

            let ws_dir = temp.path().to_string_lossy().into_owned();
            crate::tests::with_env(
                &[(
                    super::super::CARGO_MAKE_WORKSPACE_WORKING_DIRECTORY_ENV_VAR,
                    Some(&ws_dir),
                )],
                || {
                    super::super::load_rust_driver_makefile()
                        .expect("should succeed even with dangling symlink");
                },
            );

            assert!(dest.exists(), "makefile should now exist");
            assert!(
                dest.is_symlink(),
                "should be a symlink (path dep in this workspace)"
            );
            let content = std::fs::read_to_string(&dest).unwrap();
            assert!(
                content.contains(r#"wdk-build = { path = "." }"#),
                "symlinked makefile should contain path dep"
            );
        }

        #[test]
        fn replaces_stale_regular_file_with_symlink() {
            let temp = TempDir::new().unwrap();
            let target_dir = super::create_temp_target_dir(&temp);
            let dest = target_dir.join(RUST_DRIVER_MAKEFILE_NAME);

            std::fs::write(&dest, "stale content from a previous registry build").unwrap();
            assert!(!dest.is_symlink(), "should start as a regular file");

            let ws_dir = temp.path().to_string_lossy().into_owned();
            crate::tests::with_env(
                &[(
                    super::super::CARGO_MAKE_WORKSPACE_WORKING_DIRECTORY_ENV_VAR,
                    Some(&ws_dir),
                )],
                || {
                    super::super::load_rust_driver_makefile()
                        .expect("should replace regular file with symlink");
                },
            );

            assert!(dest.exists(), "makefile should exist");
            assert!(
                dest.is_symlink(),
                "should now be a symlink (path dep in workspace)"
            );
        }

        #[test]
        fn replaces_symlink_to_wrong_target() {
            let temp = TempDir::new().unwrap();
            let target_dir = super::create_temp_target_dir(&temp);
            let dest = target_dir.join(RUST_DRIVER_MAKEFILE_NAME);

            let wrong_target = temp.path().join("wrong-makefile.toml");
            std::fs::write(&wrong_target, "wrong content").unwrap();
            std::os::windows::fs::symlink_file(&wrong_target, &dest)
                .expect("should be able to create symlink");
            assert!(dest.is_symlink());
            assert_eq!(
                std::fs::read_to_string(&dest).unwrap(),
                "wrong content",
                "symlink should initially point to wrong file"
            );

            let ws_dir = temp.path().to_string_lossy().into_owned();
            crate::tests::with_env(
                &[(
                    super::super::CARGO_MAKE_WORKSPACE_WORKING_DIRECTORY_ENV_VAR,
                    Some(&ws_dir),
                )],
                || {
                    super::super::load_rust_driver_makefile()
                        .expect("should replace symlink to wrong target");
                },
            );

            assert!(dest.is_symlink(), "should still be a symlink");
            let content = std::fs::read_to_string(&dest).unwrap();
            assert!(
                content.contains(r#"wdk-build = { path = "." }"#),
                "symlink should now point to the correct makefile"
            );
        }
    }

    mod load_rust_driver_sample_makefile {
        use assert_fs::TempDir;

        use super::super::RUST_DRIVER_SAMPLE_MAKEFILE_NAME;

        #[test]
        fn creates_symlink_for_path_dep() {
            let temp = TempDir::new().unwrap();
            let target_dir = super::create_temp_target_dir(&temp);

            let ws_dir = temp.path().to_string_lossy().into_owned();
            crate::tests::with_env(
                &[(
                    super::super::CARGO_MAKE_WORKSPACE_WORKING_DIRECTORY_ENV_VAR,
                    Some(&ws_dir),
                )],
                || {
                    super::super::load_rust_driver_sample_makefile()
                        .expect("load_rust_driver_sample_makefile should succeed");
                },
            );

            let dest = target_dir.join(RUST_DRIVER_SAMPLE_MAKEFILE_NAME);
            assert!(dest.exists(), "sample makefile should exist");
            assert!(dest.is_symlink(), "should be a symlink for path deps");
        }
    }
}
