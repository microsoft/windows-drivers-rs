// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Build script for the `sample-umdf-driver` crate.
//!
//! Based on the [`wdk_build::Config`] parsed from the build tree, this build
//! script will provide `Cargo` with the necessary information to build the
//! driver binary (ex. linker flags)

fn main() -> Result<(), wdk_build::ConfigError> {
    wdk_build::Config::from_env_auto()?.configure_binary_build()
}
