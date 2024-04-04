// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Build script for the `wdk` crate.

fn main() -> Result<(), wdk_build::ConfigError> {
    tracing_subscriber::fmt().pretty().init();

    match wdk_build::Config::from_env_auto() {
        Ok(config) => {
            config.configure_library_build()?;
            // Re-export config from wdk-sys
            Ok(config.export_config()?)
        }
        Err(wdk_build::ConfigFromEnvError::ConfigNotFound) => {
            // No WDK configurations will be detected if the crate is not being used in a
            // driver. This includes when building this crate standalone or in the
            // windows-drivers-rs workspace
            tracing::warn!("No WDK configurations detected.");
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}
