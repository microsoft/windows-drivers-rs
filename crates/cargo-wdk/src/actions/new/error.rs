// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! This module defines error types for new action module.
use thiserror::Error;

use crate::providers::error::{CommandError, FileError};

/// Errors for the new action layer
#[derive(Debug, Error)]
pub enum NewActionError {
    #[error("Error executing cargo new")]
    CargoNewCommand(#[from] CommandError),
    #[error(transparent)]
    FileSystem(#[from] FileError),
    #[error("Template file not found: {0}")]
    TemplateNotFound(String),
    #[error("Unable to derive driver crate name from the provided path: {0}")]
    InvalidDriverCrateName(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
