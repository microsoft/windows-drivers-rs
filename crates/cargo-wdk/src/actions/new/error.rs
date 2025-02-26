use thiserror::Error;

use crate::providers::error::{CommandError, FileError};

#[derive(Debug, Error)]
pub enum NewProjectError {
    #[error("Error initializing new driver object, error: {0}")]
    NewDriverInitError(NewDriverError),
    #[error("Error creating new driver project, error: {0}")]
    NewDriverError(NewDriverError),
}

#[derive(Debug, Error)]
pub enum NewDriverError {
    #[error("Error executing cargo new, error: {0}")]
    CargoNewError(CommandError),
    #[error("File System Error, error: {0}")]
    FileSystemError(#[from] FileError),
    #[error("Template file not found: {0}")]
    TemplateNotFoundError(String),
    #[error("IO Error")]
    IoError(#[from] std::io::Error),
}
