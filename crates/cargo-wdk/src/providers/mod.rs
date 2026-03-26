// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! The `providers` module serves as a centralized abstraction layer for various
//! subsystems used throughout the application. It encapsulates functionality
//! such as file system operations, command execution,
//! metadata handling, and interactions with the `wdk-build` crate. By
//! consolidating these external dependencies, the module promotes cleaner
//! separation of concerns and enhances testability. This design allows external
//! calls to be easily mocked, simplifying unit testing and enabling more robust
//! and maintainable code in the action layer.

pub mod exec;
pub mod fs;
pub mod metadata;
pub mod wdk_build;

pub mod error {
    use std::{io, path::PathBuf, process::Output};

    use super::exec::CaptureStream;

    /// Error type for `std::process::command` execution failures
    #[derive(Debug, thiserror::Error)]
    pub enum CommandError {
        #[error("Command '{command}' with args {args:?} failed \n {stream_label}: {output}")]
        CommandFailed {
            command: String,
            args: Vec<String>,
            stream_label: CaptureStream,
            output: String,
        },
        #[error("Command '{0}' with args {1:?} IO error")]
        IoError(String, Vec<String>, #[source] io::Error),
    }

    impl CommandError {
        pub fn from_output(
            command: &str,
            args: &[&str],
            output: &Output,
            capture_stream: CaptureStream,
        ) -> Self {
            let stream_content = match capture_stream {
                CaptureStream::StdOut => &output.stdout,
                CaptureStream::StdErr => &output.stderr,
            };
            Self::CommandFailed {
                command: command.to_string(),
                args: args.iter().map(|&s| s.to_string()).collect(),
                stream_label: capture_stream,
                output: String::from_utf8_lossy(stream_content).to_string(),
            }
        }

        pub fn from_io_error(command: &str, args: &[&str], e: io::Error) -> Self {
            Self::IoError(
                command.to_string(),
                args.iter().map(|&s| s.to_string()).collect(),
                e,
            )
        }
    }

    /// Error type for `std::file` operations
    #[derive(Debug, thiserror::Error)]
    pub enum FileError {
        #[error("File {0} not found")]
        NotFound(PathBuf),
        #[error("Failed to write to file {0}")]
        WriteError(PathBuf, #[source] io::Error),
        #[error("Failed to read file {0}")]
        ReadError(PathBuf, #[source] io::Error),
        #[error("Failed to open file {0}")]
        OpenError(PathBuf, #[source] io::Error),
        #[error("Failed to append to file {0}")]
        AppendError(PathBuf, #[source] io::Error),
        #[error("Failed to copy file from {0} to {1}")]
        CopyError(PathBuf, PathBuf, #[source] io::Error),
        #[error("Failed to create directory at path {0}")]
        CreateDirError(PathBuf, #[source] io::Error),
        #[error("Failed to rename file from {0} to {1}")]
        RenameError(PathBuf, PathBuf, #[source] io::Error),
        #[error("Failed to get file type for directory entry {0:#?}")]
        DirFileTypeError(PathBuf, #[source] io::Error),
        #[error("Failed to read directory {0}")]
        ReadDirError(PathBuf, #[source] io::Error),
        #[error("Failed to read directory entries for {0}")]
        ReadDirEntriesError(PathBuf, #[source] io::Error),
    }
}
