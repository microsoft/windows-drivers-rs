pub mod exec;
pub mod fs;
pub mod metadata;
pub mod wdk_build;

pub mod error {
    use std::{io::Error, path::PathBuf, process::Output};

    /// Error type for `std::process::command` execution failures
    #[derive(Debug, thiserror::Error)]
    pub enum CommandError {
        #[error("Command '{command}' with args {args:?} failed \n STDOUT: {stdout}")]
        CommandFailed {
            command: String,
            args: Vec<String>,
            stdout: String,
        },
        #[error("IO error: {0}")]
        IoError(#[from] Error),
    }

    impl CommandError {
        pub fn from_output(command: &str, args: &[&str], output: &Output) -> Self {
            Self::CommandFailed {
                command: command.to_string(),
                args: args.iter().map(|&s| s.to_string()).collect(),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            }
        }
    }

    /// Error type for `std::file` operations
    #[derive(Debug, thiserror::Error)]
    pub enum FileError {
        #[error("File not found error: {0}")]
        NotFound(PathBuf),
        #[error("Failed to write to file: {0}")]
        WriteError(PathBuf, Error),
        #[error("Failed to read file: {0}")]
        ReadError(PathBuf, Error),
        #[error("Failed to open file: {0}")]
        OpenError(PathBuf, Error),
        #[error("Failed to append to file: {0}")]
        AppendError(PathBuf, Error),
    }
}
