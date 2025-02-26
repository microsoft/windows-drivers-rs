use anyhow::Result;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

/// Initializes the logger with tracing subscriber
pub fn init_logging(verbosity_level: clap_verbosity_flag::Verbosity) {
    // clamp to info verbosity level by default
    // no -v -> info log level
    // -v -> debug log level
    // -vv -> trace log level
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
        .with_env_filter(tracing_filter)
        .init();
}

/// Returns the cargo verbose flags based on the verbosity level
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
