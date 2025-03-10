// This module provides methods to initialize log level and to
// get cargo verbose flags based on the verbosity level.
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

/// Initializes the tracing subscriber with a filter based on the verbosity
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

/// Gets the cargo verbose flags based on the verbosity level
/// Returns
///     * `None` indicating no flags should be passed to cargo
///     * `Some("-q")` indicating -q flag should be passed to cargo
///     * `Some("-v")` indicating -v flag should be passed to cargo
///     * `Some("-vv")` indicating -vv flag should be passed to cargo
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
