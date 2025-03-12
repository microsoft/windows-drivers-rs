use std::{path::PathBuf, string::FromUtf8Error};

use thiserror::Error;

use crate::{actions::build::BuildActionError, providers::error::CommandError};

/// Errors for the package project action layer
#[derive(Error, Debug)]
pub enum PackageProjectError {
    #[error("Wdk Build Config Error")]
    WdkBuildConfig(#[from] wdk_build::ConfigError),
    #[error("Error Parsing Cargo.toml")]
    CargoMetadataParse(#[from] cargo_metadata::Error),
    #[error("Error Parsing WDK metadata from Cargo.toml")]
    WdkMetadataParse(#[from] wdk_build::metadata::TryFromCargoMetadataError),
    #[error("Error running build action: {0}")]
    BuildAction(#[from] BuildActionError),
    #[error("IO Error")]
    Io(#[from] std::io::Error),
    #[error("Command Execution Error")]
    CommandExecution(#[from] CommandError),
    #[error("Not a workspace member, working directory: {0}")]
    NotAWorkspaceMember(PathBuf),
    #[error("Error initiating package tasks, package_name: {0}, error: {1}")]
    PackageTaskInit(String, PackageTaskError),
    #[error("Error performing packaging tasks, package_name: {0}, error: {1}")]
    PackageTask(String, PackageTaskError),
    #[error("No valid rust projects in the current working directory: {0}")]
    NoValidRustProjectsInTheDirectory(PathBuf),
    #[error(
        "One or more rust (possibly driver) projects failed to package in the working directory: \
         {0}"
    )]
    OneOrMoreRustProjectsFailedToBuild(PathBuf),
}

/// Errors for the low level package task layer
#[derive(Error, Debug)]
pub enum PackageTaskError {
    #[error(
        "Missing .inx file in source path: {0}, Please ensure you are in a Rust driver project \
         directory."
    )]
    MissingInxSrcFile(PathBuf),
    #[error("Failed to copy file error, src: {0:?}, dest: {1:?}, error: {2:?}")]
    CopyFile(PathBuf, PathBuf, std::io::Error),
    #[error("Error running stampinf command, error: {0}")]
    StampinfCommand(CommandError),
    #[error("Error running inf2cat command, error: {0}")]
    Inf2CatCommand(CommandError),
    #[error("Creating cert file from store using certmgr, error: {0}")]
    CreateCertFileFromStoreCommand(CommandError),
    #[error("Checking for existence of cert in store using certmgr, error: {0}")]
    VerifyCertExistsInStoreCommand(CommandError),
    #[error(
        "Error reading stdout while checking for existence of cert in store using certmgr, error: \
         {0}"
    )]
    VerifyCertExistsInStoreInvalidCommandOutput(FromUtf8Error),
    #[error("Error generating certificate to cert store using makecert, error: {0}")]
    CertGenerationInStoreCommand(CommandError),
    #[error("Error signing driver binary using signtool, error: {0}")]
    DriverBinarySignCommand(CommandError),
    #[error("Error verifying signed driver binary using signtool, error: {0}")]
    DriverBinarySignVerificationCommand(CommandError),
    #[error("Error verifying inf file using infverif, error: {0}")]
    InfVerificationCommand(CommandError),

    // TODO: We can make this specific error instead of generic one
    #[error("Error from wdk build, error: {0}")]
    WdkBuildConfig(#[from] wdk_build::ConfigError),
    #[error("Io error, error: {0}")]
    Io(#[from] std::io::Error),
}
