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
use tracing::{instrument, trace};

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
    env::var(CARGO_MAKE_DISABLE_COLOR_ENV_VAR)
        .map(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                // when color is enabled in cargo-make, the env var is guaranteed to be set to one
                // of the below values, or not be set at all
                "0" | "false" | "no" | ""
            )
        })
        .unwrap_or(false)
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
        format!("{host_windows_sdk_ver_bin_path};{x86_windows_sdk_ver_bin_path}",),
    );

    let wdk_tool_root = get_wdk_tools_root(&wdk_content_root, sdk_version);
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

fn get_wdk_tools_root(wdk_content_root: &Path, sdk_version: String) -> PathBuf {
    env::var("WDKToolRoot")
        .map_or_else(|_| wdk_content_root.join("tools"), PathBuf::from)
        .join(sdk_version)
}

fn get_wdk_bin_root(wdk_content_root: &Path, sdk_version: &String) -> PathBuf {
    env::var("WDKBinRoot")
        .map_or_else(|_| wdk_content_root.join("bin"), PathBuf::from)
        .join(sdk_version)
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

/// Symlinks `rust-driver-makefile.toml` to the `target` folder where it can be
/// extended from a `Makefile.toml`.
///
/// This is necessary so that paths in the `rust-driver-makefile.toml` can to be
/// relative to `CARGO_MAKE_CURRENT_TASK_INITIAL_MAKEFILE_DIRECTORY`
///
/// # Errors
///
/// This function returns:
/// - [`ConfigError::CargoMetadataError`] if there is an error executing or
///   parsing `cargo_metadata`
/// - [`ConfigError::MultipleWdkBuildCratesDetected`] if there are multiple
///   versions of the WDK build crate detected
/// - [`ConfigError::IoError`] if there is an error creating or updating the
///   symlink to `rust-driver-makefile.toml`
///
/// # Panics
///
/// This function will panic if the `CARGO_MAKE_WORKSPACE_WORKING_DIRECTORY`
/// environment variable is not set
pub fn load_rust_driver_makefile() -> Result<(), ConfigError> {
    load_wdk_build_makefile(RUST_DRIVER_MAKEFILE_NAME)
}

/// Symlinks `rust-driver-sample-makefile.toml` to the `target` folder where it
/// can be extended from a `Makefile.toml`.
///
/// This is necessary so that paths in the `rust-driver-sample-makefile.toml`
/// can to be relative to `CARGO_MAKE_CURRENT_TASK_INITIAL_MAKEFILE_DIRECTORY`
///
/// # Errors
///
/// This function returns:
/// - [`ConfigError::CargoMetadataError`] if there is an error executing or
///   parsing `cargo_metadata`
/// - [`ConfigError::MultipleWdkBuildCratesDetected`] if there are multiple
///   versions of the WDK build crate detected
/// - [`ConfigError::IoError`] if there is an error creating or updating the
///   symlink to `rust-driver-sample-makefile.toml`
///
/// # Panics
///
/// This function will panic if the `CARGO_MAKE_WORKSPACE_WORKING_DIRECTORY`
/// environment variable is not set
pub fn load_rust_driver_sample_makefile() -> Result<(), ConfigError> {
    load_wdk_build_makefile(RUST_DRIVER_SAMPLE_MAKEFILE_NAME)
}

/// Symlinks a [`wdk_build`] `cargo-make` makefile to the `target` folder where
/// it can be extended from a downstream `Makefile.toml`.
///
/// This is necessary so that paths in the [`wdk_build`] makefile can be
/// relative to `CARGO_MAKE_CURRENT_TASK_INITIAL_MAKEFILE_DIRECTORY`. The
/// version of `wdk-build` from which the file being symlinked to comes from is
/// determined by the workding directory of the process that invokes this
/// function. For example, if this function is ultimately executing in a
/// `cargo_make` `load_script`, the files will be symlinked from the `wdk-build`
/// version that is in the `.Cargo.lock` file, and not the `wdk-build` version
/// specified in the `load_script`.
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

    let rust_driver_makefile_toml_path = wdk_build_package_matches[0]
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

    // Only create a new symlink if the existing one is not already pointing to the
    // correct file
    if !destination_path.exists() {
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
    Ok(())
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
    use crate::ConfigError;

    const WDK_TEST_OLD_INF_VERSION: &str = "10.0.22061.0";
    const WDK_TEST_NEW_INF_VERSION: &str = "10.0.26100.0";

    #[test]
    fn check_env_passing() -> Result<(), ConfigError> {
        crate::cargo_make::setup_infverif_for_samples(WDK_TEST_OLD_INF_VERSION)?;
        let env_string = std::env::var_os(crate::cargo_make::WDK_INF_ADDITIONAL_FLAGS_ENV_VAR)
            .map_or_else(
                || panic!("Couldn't get OS string"),
                |os_env_string| os_env_string.to_string_lossy().into_owned(),
            );
        assert_eq!(env_string.split(' ').next_back(), Some("/msft"));

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
