use thiserror::Error;

/// Validation errors for the arguments passed to new project sub command
#[derive(Debug, Error)]
pub enum NewProjectArgsError {
    #[error("Invalid driver project name: {0}, error: {1}")]
    InvalidDriverProjectNameError(String, InvalidDriverProjectNameError),
    #[error("Invalid driver type: {0}")]
    InvalidDriverTypeError(String),
}

/// Validation errors for the driver project name arg passed to new project sub
/// command
#[derive(Debug, Error)]
pub enum InvalidDriverProjectNameError {
    #[error("Project name cannot be empty")]
    EmptyProjectNameError,
    #[error("Project name can only contain alphanumeric characters, hyphens, and underscores")]
    NonAlphanumericProjectNameError,
    #[error("Project name must start with an alphabetic character")]
    InvalidStartCharacter,
    #[error("'{0}' is a reserved keyword or invalid name and cannot be used as a project name")]
    ReservedName(String),
}

#[derive(Debug, Error)]
#[error("Unsupported host architecture: {arch}")]
pub struct UnsupportedHostArchError {
    pub arch: String,
}
