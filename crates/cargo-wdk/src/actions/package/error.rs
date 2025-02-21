use std::{path::PathBuf, string::FromUtf8Error};

use crate::errors::CommandError;

#[derive(thiserror::Error, Debug)]
pub enum PackageProjectError {
    #[error("Wdk Build Config Error")]
    WdkBuildConfigError(#[from] wdk_build::ConfigError),
    #[error("Error Parsing Cargo.toml")]
    CargoMetadataParseError(#[from] cargo_metadata::Error),
    #[error("Error Parsing WDK metadata from Cargo.toml")]
    WdkMetadataParseError(#[from] wdk_build::metadata::TryFromCargoMetadataError),
    #[error("IO Error")]
    IoError(#[from] std::io::Error),
    #[error("Command Execution Error")]
    CommandExecutionError(#[from] CommandError),
    #[error("Not a workspace member, working directory: {0}")]
    NotAWorkspaceMemberError(PathBuf),
    #[error("Error initiating driver packaging, package_name: {0}, error: {1}")]
    PackageDriverInitError(String, PackageDriverError),
    #[error("Error packaging driver, package_name: {0}, error: {1}")]
    PackageDriverError(String, PackageDriverError),
    #[error("No valid rust projects in the current working directory: {0}")]
    NoValidRustProjectsInTheDirectory(PathBuf),
    #[error(
        "One or more rust (possibly driver) projects failed to package in the working directory: \
         {0}"
    )]
    OneOrMoreRustProjectsFailedToBuild(PathBuf),
}

#[derive(thiserror::Error, Debug)]
pub enum PackageDriverError {
    #[error(
        "Missing .inx file in source path: {0}, Please ensure you are in a Rust driver project \
         directory."
    )]
    MissingInxSrcFileError(PathBuf),
    #[error("Failed to copy file error, src: {0:?}, dest: {1:?}, error: {2:?}")]
    CopyFileError(PathBuf, PathBuf, std::io::Error),
    #[error("Error running stampinf command, error: {0}")]
    StampinfError(CommandError),
    #[error("Error running inf2cat command, error: {0}")]
    Inf2CatError(CommandError),
    #[error("Creating cert file from store using certmgr, error: {0}")]
    CreateCertFileFromStoreError(CommandError),
    #[error("Checking for existence of cert in store using certmgr, error: {0}")]
    VerifyCertExistsInStoreError(CommandError),
    #[error(
        "Error reading stdout while checking for existence of cert in store using certmgr, error: \
         {0}"
    )]
    VerifyCertExistsInStoreInvalidCommandOutputError(FromUtf8Error),
    #[error("Error generating certificate to cert store using makecert, error: {0}")]
    CertGenerationInStoreError(CommandError),
    #[error("Error signing driver binary using signtool, error: {0}")]
    DriverBinarySignError(CommandError),
    #[error("Error verifying signed driver binary using signtool, error: {0}")]
    DriverBinarySignVerificationError(CommandError),
    #[error("Error verifying inf file using infverif, error: {0}")]
    InfVerificationError(CommandError),

    // TODO: We can make this specific error instead of generic one
    #[error("Error from wdk build, error: {0}")]
    WdkBuildConfigError(#[from] wdk_build::ConfigError),
    #[error("Io error, error: {0}")]
    IoError(#[from] std::io::Error),
}
