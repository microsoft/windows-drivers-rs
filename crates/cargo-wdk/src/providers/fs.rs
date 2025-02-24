use std::{
    fs::{copy, create_dir, rename, File, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
};

use mockall::automock;

use super::error::FileError;

/// Provides limited access to std::fs methods
pub(crate) struct FS {}

/// A Provider trait with methods for file system access
#[automock]
pub(crate) trait FSProvider {
    fn rename(&self, src: &PathBuf, dest: &PathBuf) -> Result<(), std::io::Error>;
    fn canonicalize_path(&self, path: PathBuf) -> Result<PathBuf, std::io::Error>;
    fn copy(&self, src: &PathBuf, dest: &PathBuf) -> Result<u64, std::io::Error>;
    fn exists(&self, path: &PathBuf) -> bool;
    fn create_dir(&self, path: &PathBuf) -> Result<(), std::io::Error>;
    fn read_file_to_string(&self, path: &PathBuf) -> Result<String, FileError>;
    fn write_to_file(&self, path: &PathBuf, data: &[u8]) -> Result<(), FileError>;
    fn append_to_file(&self, path: &PathBuf, data: &[u8]) -> Result<(), FileError>;
}

impl FSProvider for FS {
    fn canonicalize_path(&self, path: PathBuf) -> Result<PathBuf, std::io::Error> {
        path.canonicalize()
    }

    fn copy(&self, src: &PathBuf, dest: &PathBuf) -> Result<u64, std::io::Error> {
        copy(src, dest)
    }

    fn exists(&self, path: &PathBuf) -> bool {
        path.exists()
    }

    fn create_dir(&self, path: &PathBuf) -> Result<(), std::io::Error> {
        create_dir(path)
    }

    fn rename(&self, src: &PathBuf, dest: &PathBuf) -> Result<(), std::io::Error> {
        rename(src, dest)
    }

    fn read_file_to_string(&self, path: &PathBuf) -> Result<String, FileError> {
        if !path.exists() {
            return Err(FileError::OpenError(format!(
                "File does not exist: {}",
                path.to_string_lossy()
            )));
        }
        let mut content = String::new();
        let mut file = File::open(path).map_err(|e| {
            FileError::OpenError(format!(
                "Failed to open file: {}: {}",
                path.to_string_lossy(),
                e
            ))
        })?;
        file.read_to_string(&mut content).map_err(|e| {
            FileError::ReadError(format!(
                "Failed to read file: {}: {}",
                path.to_string_lossy(),
                e
            ))
        })?;
        Ok(content)
    }

    fn write_to_file(&self, path: &PathBuf, data: &[u8]) -> Result<(), FileError> {
        let mut file = File::create(path)
            .map_err(|_| FileError::WriteError(path.to_string_lossy().to_string()))?;
        file.write_all(data)
            .map_err(|_| FileError::WriteError(path.to_string_lossy().to_string()))?;
        Ok(())
    }

    fn append_to_file(&self, path: &PathBuf, data: &[u8]) -> Result<(), FileError> {
        let mut file = OpenOptions::new()
            .write(true)
            .append(true)
            .open(path)
            .map_err(|_| FileError::AppendError(path.to_string_lossy().to_string()))?;
        file.write_all(data)
            .map_err(|_| FileError::WriteError(path.to_string_lossy().to_string()))?;
        Ok(())
    }
}
