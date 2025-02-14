// FIXME: Error types and rename file as file_utils.rs
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
};

use crate::errors::FileError;

// Helper function to read the contents of a file into a string
pub fn read_file_to_string(path: &PathBuf) -> Result<String, FileError> {
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

// // Helper function to read the contents of a file into a byte vector
// pub fn read_file_to_bytes(path: &PathBuf) -> Result<Vec<u8>, FileError> {
//     if !path.exists() {
//         return Err(FileError::OpenError(format!("File does not exist: {}",
// path.to_string_lossy())));     }
//     let mut content = Vec::new();
//     let mut file = File::open(path).map_err(|e|
// FileError::OpenError(format!("Failed to open file: {}: {}",
// path.to_string_lossy(), e)))?;     file.read_to_end(&mut content).map_err(|e|
// FileError::ReadError(format!("Failed to read file: {}: {}",
// path.to_string_lossy(), e)))?;     Ok(content)
// }

// Helper function to write data to a file
pub fn write_to_file(path: &PathBuf, data: &[u8]) -> Result<(), FileError> {
    let mut file = File::create(path)
        .map_err(|_| FileError::WriteError(path.to_string_lossy().to_string()))?;
    file.write_all(data)
        .map_err(|_| FileError::WriteError(path.to_string_lossy().to_string()))?;
    Ok(())
}

// Helper function to append data to a file
pub fn append_to_file(path: &PathBuf, data: &[u8]) -> Result<(), FileError> {
    let mut file = OpenOptions::new()
        .write(true)
        .append(true)
        .open(path)
        .map_err(|_| FileError::AppendError(path.to_string_lossy().to_string()))?;
    file.write_all(data)
        .map_err(|_| FileError::WriteError(path.to_string_lossy().to_string()))?;
    Ok(())
}
