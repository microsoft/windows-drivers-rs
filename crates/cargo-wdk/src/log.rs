use anyhow::Result;
use log4rs::{
    append::console::ConsoleAppender,
    config::{Appender, Root},
    encode::pattern::PatternEncoder,
    Config,
};

/// Initializes the logger with a console appender and a pattern encoder
pub fn init_logging(verbosity_level: clap_verbosity_flag::Verbosity) -> Result<()> {
    let stdout = ConsoleAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{h({l:>5})}: {m}{n}")))
        .build();

    // clamp to info verbosity level by default
    // no -v -> info log level
    // -v -> debug log level
    // -vv -> trace log level
    let level = match verbosity_level.filter() {
        clap_verbosity_flag::VerbosityFilter::Off => log::LevelFilter::Off,
        clap_verbosity_flag::VerbosityFilter::Error => log::LevelFilter::Info,
        clap_verbosity_flag::VerbosityFilter::Warn => log::LevelFilter::Debug,
        _ => log::LevelFilter::Trace,
    };

    let config = Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(stdout)))
        .build(Root::builder().appender("stdout").build(level))?;

    log4rs::init_config(config)?;

    Ok(())
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
