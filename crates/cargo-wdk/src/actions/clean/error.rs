// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! This module defines error types for the clean action module.

use std::path::PathBuf;

use thiserror::Error;

use crate::providers::error::{CommandError, FileError};

/// Errors for the clean action layer
#[derive(Error, Debug)]
pub enum CleanActionError {
    #[error(transparent)]
    FileIo(#[from] FileError),
    #[error("No valid rust projects in the current working directory: {0}")]
    NoValidRustProjectsInTheDirectory(PathBuf),
    #[error("One or more projects failed to clean in the emulated workspace: {0}")]
    OneOrMoreRustProjectsFailedToClean(PathBuf),
    #[error(transparent)]
    CargoClean(#[from] CommandError),
}
