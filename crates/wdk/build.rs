// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Build script for the `wdk` crate.

fn main() -> Result<(), wdk_build::ConfigError> {
    // Re-export config from wdk-sys
    #[cfg(binding_generation)]
    Ok(wdk_build::Config::from_env_auto()?.export_config()?)

    #[cfg(not(binding_generation))]
    Ok(())
}
