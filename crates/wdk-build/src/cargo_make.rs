// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! This module provides argument parsing functionality used by
//! `rust-driver-makefile.toml` to validate and forward arguments common to
//! cargo commands. It uses a combination of `clap` and `clap_cargo` to provide
//! a CLI very close to cargo's own, but only exposes the arguments supported by
//! `rust-driver-makefile.toml`. Help text and other `clap::Arg`

use clap::{Args, Parser};

/// The name of the environment variable that cargo-make uses during `cargo
/// build` and `cargo test` commands
const CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR: &str = "CARGO_MAKE_CARGO_BUILD_TEST_FLAGS";

const CARGO_MAKE_PROFILE_ENV_VAR: &str = "CARGO_MAKE_PROFILE";
const CARGO_MAKE_CARGO_PROFILE_ENV_VAR: &str = "CARGO_MAKE_CARGO_PROFILE";
const CARGO_MAKE_RUST_DEFAULT_TOOLCHAIN_ENV_VAR: &str = "CARGO_MAKE_RUST_DEFAULT_TOOLCHAIN";

/// `clap` uses an exit code of 2 for usage errors: <https://github.com/clap-rs/clap/blob/14fd853fb9c5b94e371170bbd0ca2bf28ef3abff/clap_builder/src/util/mod.rs#L30C18-L30C28>
const CLAP_USAGE_EXIT_CODE: i32 = 2;

trait ParseCargoArg {
    fn parse_cargo_arg(&self);
}

#[derive(Parser, Debug)]
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

    // TODO: support building multiple targets at once
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

impl ParseCargoArg for BaseOptions {
    fn parse_cargo_arg(&self) {
        if self.quiet && self.verbose > 0 {
            eprintln!("Cannot specify both --quiet and --verbose");
            std::process::exit(CLAP_USAGE_EXIT_CODE);
        }

        if self.quiet {
            append_to_space_delimited_env_var(CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR, "--quiet");
        }

        if self.verbose > 0 {
            append_to_space_delimited_env_var(
                CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                format!("-{}", "v".repeat(self.verbose.into())).as_str(),
            );
        }
    }
}

impl ParseCargoArg for clap_cargo::Workspace {
    fn parse_cargo_arg(&self) {
        if !self.package.is_empty() {
            append_to_space_delimited_env_var(
                CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                self.package
                    .iter()
                    .fold(
                        String::with_capacity({
                            const MINIMUM_PACKAGE_SPEC_LENGTH: usize = 1;
                            const MINIMUM_PACKAGE_ARG_LENGTH: usize =
                                "--package ".len() + MINIMUM_PACKAGE_SPEC_LENGTH + " ".len();
                            self.package.len() * MINIMUM_PACKAGE_ARG_LENGTH
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

        if self.workspace {
            append_to_space_delimited_env_var(
                CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                "--workspace",
            );
        }

        if !self.exclude.is_empty() {
            if !self.workspace {
                eprintln!("--exclude can only be used together with --workspace");
                std::process::exit(CLAP_USAGE_EXIT_CODE);
            }

            append_to_space_delimited_env_var(
                CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                self.exclude
                    .iter()
                    .fold(
                        String::with_capacity({
                            const MINIMUM_PACKAGE_SPEC_LENGTH: usize = 1;
                            const MINIMUM_EXCLUDE_ARG_LENGTH: usize =
                                "--exclude ".len() + MINIMUM_PACKAGE_SPEC_LENGTH + " ".len();
                            self.package.len() * MINIMUM_EXCLUDE_ARG_LENGTH
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

        if self.all {
            append_to_space_delimited_env_var(CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR, "--all");
        }
    }
}

impl ParseCargoArg for clap_cargo::Features {
    fn parse_cargo_arg(&self) {
        if self.all_features {
            append_to_space_delimited_env_var(
                CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                "--all-features",
            );
        }

        if self.no_default_features {
            append_to_space_delimited_env_var(
                CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                "--no-default-features",
            );
        }

        if !self.features.is_empty() {
            append_to_space_delimited_env_var(
                CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                self.features
                    .iter()
                    .fold(
                        String::with_capacity({
                            const MINIMUM_FEATURE_NAME_LENGTH: usize = 1;
                            const MINIMUM_FEATURE_ARG_LENGTH: usize =
                                "--features ".len() + MINIMUM_FEATURE_NAME_LENGTH + " ".len();
                            self.features.len() * MINIMUM_FEATURE_ARG_LENGTH
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

impl ParseCargoArg for CompilationOptions {
    fn parse_cargo_arg(&self) {
        if self.release && self.profile.is_some() {
            eprintln!("the `--release` flag should not be specified with the `--profile` flag");
            std::process::exit(CLAP_USAGE_EXIT_CODE);
        }
        match std::env::var(CARGO_MAKE_PROFILE_ENV_VAR)
            .unwrap_or_else(|_| panic!("{CARGO_MAKE_PROFILE_ENV_VAR} should be set by cargo-make."))
            .as_ref()
        {
            "release" => {
                // cargo-make release profile sets the `--profile release` flag
                if let Some(profile) = &self.profile {
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
            }
            _ => {
                // All other cargo-make profiles do not set a specific cargo profile. Cargo
                // profiles set by --release, --profile <PROFILE>, or -p <PROFILE> (after the
                // cargo-make task name) are forwarded to cargo commands
                if self.release {
                    println!("{CARGO_MAKE_CARGO_PROFILE_ENV_VAR}=release");
                    append_to_space_delimited_env_var(
                        CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                        "--release",
                    );
                } else if let Some(profile) = &self.profile {
                    println!("{CARGO_MAKE_CARGO_PROFILE_ENV_VAR}={profile}");
                    append_to_space_delimited_env_var(
                        CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                        format!("--profile {profile}").as_str(),
                    );
                }
            }
        }

        if let Some(jobs) = &self.jobs {
            append_to_space_delimited_env_var(
                CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                format!("--jobs {jobs}").as_str(),
            );
        }

        if let Some(target) = &self.target {
            println!("CARGO_MAKE_CRATE_TARGET_TRIPLE={target}");
            append_to_space_delimited_env_var(
                CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                format!("--target {target}").as_str(),
            );
        }

        if let Some(timings_option) = &self.timings {
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

impl ParseCargoArg for ManifestOptions {
    fn parse_cargo_arg(&self) {
        if self.frozen {
            append_to_space_delimited_env_var(
                CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                "--frozen",
            );
        }

        if self.locked {
            append_to_space_delimited_env_var(
                CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                "--locked",
            );
        }

        if self.offline {
            append_to_space_delimited_env_var(
                CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR,
                "--offline",
            );
        }
    }
}

/// Parses the command line arguments, validates that they are supported by
/// `rust-driver-makefile.toml`, and forwards them to `cargo-make` by printing
/// them to stdout.
///
/// # Panics
///
/// This function will panic if there's an internal error (i.e. bug) in its
/// argument processing.
pub fn validate_and_forward_args() {
    const TOOLCHAIN_ARG_POSITION: usize = 1;

    let mut env_args = std::env::args_os().collect::<Vec<_>>();

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
                .to_owned(),
        )
    } else {
        None
    };

    let command_line_interface: CommandLineInterface =
        CommandLineInterface::parse_from(env_args.iter());
    // This print signifies the start of the forwarding and signals to the
    // `rust-env-update` plugin that it should forward args. This is also used to
    // signal that the auto-generated help from `clap` was not executed.
    println!("FORWARDING ARGS TO CARGO-MAKE:");

    if let Some(toolchain) = toolchain_arg {
        println!("{CARGO_MAKE_RUST_DEFAULT_TOOLCHAIN_ENV_VAR}={toolchain}");
    }

    command_line_interface.base.parse_cargo_arg();
    command_line_interface.workspace.parse_cargo_arg();
    command_line_interface.features.parse_cargo_arg();
    command_line_interface.compilation_options.parse_cargo_arg();
    command_line_interface.manifest_options.parse_cargo_arg();

    forward_env_var_to_cargo_make(CARGO_MAKE_CARGO_BUILD_TEST_FLAGS_ENV_VAR);
}

fn append_to_space_delimited_env_var<S: AsRef<str>>(env_var_name: S, string_to_append: S) {
    let env_var_name = env_var_name.as_ref();
    let string_to_append = string_to_append.as_ref();

    let mut env_var_value = std::env::var(env_var_name).unwrap_or_default();
    env_var_value.push(' ');
    env_var_value.push_str(string_to_append);
    std::env::set_var(env_var_name, env_var_value.trim());
}

fn forward_env_var_to_cargo_make<S: AsRef<str>>(env_var_name: S) {
    let env_var_name = env_var_name.as_ref();

    // Since this executes in a child proccess to cargo-make, we need to forward the
    // values we want to change to duckscript, in order to get it to modify the
    // parent process (ie. cargo-make)
    if let Some(env_var_value) = std::env::var_os(env_var_name) {
        println!(
            "{env_var_name}={}",
            env_var_value
                .to_str()
                .expect("env var value should be valid UTF-8")
        );
    }
}
