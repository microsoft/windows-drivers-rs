//! This module defines error types for new action module.
use thiserror::Error;

use crate::providers::error::{CommandError, FileError};

/// Errors for the action layer
#[derive(Debug, Error)]
pub enum NewActionError {
    #[error("Error executing cargo new, error: {0}")]
    CargoNewCommand(CommandError),
    #[error("File System Error, error: {0}")]
    FileSystem(#[from] FileError),
    #[error("Template file not found: {0}")]
    TemplateNotFound(String),
    #[error("IO Error")]
    Io(#[from] std::io::Error),
}
