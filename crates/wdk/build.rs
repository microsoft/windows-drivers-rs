// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

fn main() -> Result<(), wdk_build::ConfigError> {
    // Re-export config from wdk-sys
    Ok(wdk_build::Config::from_env_auto()?.export_config()?)
}
