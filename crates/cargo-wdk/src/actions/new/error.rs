use thiserror::Error;

use crate::providers::error::FileError;

#[derive(Debug, Error)]
pub enum NewProjectError {
    #[error("File error: {0}")]
    FileError(#[from] FileError),

    #[error("Failed to execute cargo new: {0}")]
    CargoNewError(#[from] std::io::Error),

    #[error("Template file not found: {0}")]
    TemplateNotFoundError(String),
}