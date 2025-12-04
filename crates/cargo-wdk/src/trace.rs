// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! This module provides utilities for configuring logging and determining
//! cargo verbosity flags based on the verbosity level specified via clap.
//!
//! It includes:
//! - A function to initialize the tracing subscriber with appropriate log
//!   levels.
//! - A function to map clap verbosity levels to corresponding cargo verbose
//!   flags.

use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

/// Initializes the tracing subscriber with a filter based on clap's verbosity
/// level.
pub fn init_tracing(verbosity_level: clap_verbosity_flag::Verbosity) {
    // Change default log level to
    // * INFO if no verbosity level is set
    // * Debug level when -v is set
    // * Trace level when -vv is set
    let level = match verbosity_level.filter() {
        clap_verbosity_flag::VerbosityFilter::Off => LevelFilter::OFF,
        clap_verbosity_flag::VerbosityFilter::Error => LevelFilter::INFO,
        clap_verbosity_flag::VerbosityFilter::Warn => LevelFilter::DEBUG,
        _ => LevelFilter::TRACE,
    };

    let tracing_filter = EnvFilter::default().add_directive(level.into());

    tracing_subscriber::fmt()
        .compact()
        .without_time()
        .with_target(false)
        .with_file(false)
        .with_env_filter(tracing_filter)
        .init();
}

/// Gets the verbose flags for cargo command based on clap's verbosity level.
/// `clap_verbosity_flag::Verbosity` has a different set of verbosity levels
/// compared to cargo.
/// The method maps the right cargo verbose flag as follows:
/// Returns
///     * `None` when clap's verbosity level is `Error`
///     * `Some("-q")` when clap's verbosity level is `Off`
///     * `Some("-v")` when clap's verbosity level is `Warn`
///     * `Some("-vv")` when clap's verbosity level is `Info` or `Debug` or
///       `Trace`
pub fn get_cargo_verbose_flags<'a>(
    verbosity_level: clap_verbosity_flag::Verbosity,
) -> Option<&'a str> {
    match verbosity_level.filter() {
        clap_verbosity_flag::VerbosityFilter::Off => Some("-q"),
        clap_verbosity_flag::VerbosityFilter::Error => None,
        clap_verbosity_flag::VerbosityFilter::Warn => Some("-v"),
        _ => Some("-vv"),
    }
}
