use std::io;
use std::process::Output;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NewProjectError {
    #[error("File error: {0}")]
    FileError(#[from] FileError),

    #[error("Failed to execute cargo new: {0}")]
    CargoNewError(#[from] std::io::Error),

    #[error("Template file not found: {0}")]
    TemplateNotFoundError(String),
}

#[derive(Error, Debug)]
pub enum CommandError {
    #[error("Command '{command}' with args {args:?} failed \n STDOUT: {stdout}")]
    CommandFailed {
        command: String,
        args: Vec<String>,
        stdout: String,
    },
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),
}

impl CommandError {
    pub fn from_output(command: &str, args: &[&str], output: Output) -> Self {
        CommandError::CommandFailed {
            command: command.to_string(),
            args: args.iter().map(|&s| s.to_string()).collect(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        }
    }
}

#[derive(Debug, Error)]
pub enum FileError {
    #[error("Failed to write to file: {0}")]
    WriteError(String),

    #[error("Failed to read file: {0}")]
    ReadError(String),

    #[error("Failed to open file: {0}")]
    OpenError(String),

    #[error("Failed to append to file: {0}")]
    AppendError(String),
}
