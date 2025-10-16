// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! This module provides a wrapper around a subset of `std::fs` methods,
//! offering a simplified and testable interface for common file system
//! operations such as reading, writing, copying, and checking file existence.
//! It also integrates with `mockall` to enable mocking for unit tests.

// Warns the methods are not used, however they are used.
// The intellisense confusion seems to come from automock
#![allow(dead_code)]
#![allow(clippy::unused_self)]

use std::{
    fs::{DirEntry, File, FileType, OpenOptions, copy, create_dir, read_dir, rename},
    io::{Read, Write},
    path::Path,
};

use mockall::automock;

use super::error::FileError;

/// Provides limited access to `std::fs` methods
#[derive(Default)]
pub struct Fs {}

#[automock]
impl Fs {
    pub fn copy(&self, src: &Path, dest: &Path) -> Result<u64, FileError> {
        copy(src, dest).map_err(|e| FileError::CopyError(src.to_owned(), dest.to_owned(), e))
    }

    pub fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    pub fn create_dir(&self, path: &Path) -> Result<(), FileError> {
        create_dir(path).map_err(|e| FileError::CreateDirError(path.to_owned(), e))
    }

    pub fn dir_file_type(&self, dir: &DirEntry) -> Result<FileType, FileError> {
        dir.file_type()
            .map_err(|e| FileError::DirFileTypeError(dir.path(), e))
    }

    pub fn read_dir_entries(&self, path: &Path) -> Result<Vec<DirEntry>, FileError> {
        read_dir(path)
            .map_err(|e| FileError::ReadDirError(path.to_owned(), e))?
            .collect::<Result<Vec<DirEntry>, std::io::Error>>()
            .map_err(|e| FileError::ReadDirEntriesError(path.to_owned(), e))
    }

    pub fn rename(&self, src: &Path, dest: &Path) -> Result<(), FileError> {
        rename(src, dest).map_err(|e| FileError::RenameError(src.to_owned(), dest.to_owned(), e))
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
