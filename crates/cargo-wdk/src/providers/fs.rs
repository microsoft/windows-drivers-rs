// Warns the methods are not used, however they are used.
// The intellisense confusion seems to come from automock
#![allow(dead_code)]
#![allow(clippy::unused_self)]
use std::{
    fs::{copy, create_dir, rename, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use mockall::automock;

use super::error::FileError;

/// Provides limited access to `std::fs` methods
#[derive(Default)]
pub struct Fs {}

#[automock]
impl Fs {
    pub fn canonicalize_path(&self, path: &Path) -> Result<PathBuf, std::io::Error> {
        path.canonicalize()
    }

    pub fn copy(&self, src: &Path, dest: &Path) -> Result<u64, std::io::Error> {
        copy(src, dest)
    }

    pub fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    pub fn create_dir(&self, path: &Path) -> Result<(), std::io::Error> {
        create_dir(path)
    }

    pub fn rename(&self, src: &Path, dest: &Path) -> Result<(), std::io::Error> {
        rename(src, dest)
    }

    pub fn read_file_to_string(&self, path: &Path) -> Result<String, FileError> {
        if !path.exists() {
            return Err(FileError::NotFound(path.to_owned()));
        }
        let mut content = String::new();
        let mut file = File::open(path).map_err(|e| FileError::OpenError(path.to_owned(), e))?;
        file.read_to_string(&mut content)
            .map_err(|e| FileError::ReadError(path.to_owned(), e))?;
        Ok(content)
    }

    pub fn write_to_file(&self, path: &Path, data: &[u8]) -> Result<(), FileError> {
        let mut file = File::create(path).map_err(|e| FileError::WriteError(path.to_owned(), e))?;
        file.write_all(data)
            .map_err(|e| FileError::WriteError(path.to_owned(), e))?;
        Ok(())
    }

    pub fn append_to_file(&self, path: &Path, data: &[u8]) -> Result<(), FileError> {
        let mut file = OpenOptions::new()
            .append(true)
            .open(path)
            .map_err(|e| FileError::AppendError(path.to_owned(), e))?;
        file.write_all(data)
            .map_err(|e| FileError::WriteError(path.to_owned(), e))?;
        Ok(())
    }
}
