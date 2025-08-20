// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! This module provides a wrapper around the `wdk-build` crate methods,
//! focusing on the functionality required for detecting the WDK build number.
//! It leverages the `mockall` crate to enable mocking of the `WdkBuild` struct
//! for improved testability in unit tests.

#![allow(clippy::unused_self)]

/// Provides limited access to wdk-build crate methods
#[derive(Default)]
pub struct WdkBuild {}

#[cfg_attr(test, mockall::automock)]
impl WdkBuild {
    pub fn detect_wdk_build_number(&self) -> Result<u32, wdk_build::ConfigError> {
        wdk_build::detect_wdk_build_number()
    }
}
