// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Build script for the `wdk-alloc` crate.
//!
//! Based on the [`wdk_build::Config`] parsed from the build tree, this build
//! script will provide the `wdk_alloc` crate with `cfg` settings to
//! conditionally compile code.

fn main() -> Result<(), wdk_build::ConfigError> {
    tracing_subscriber::fmt().pretty().init();

    wdk_build::configure_wdk_library_build()
}
