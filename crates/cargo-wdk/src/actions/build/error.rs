// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! This module defines error types used in the build action module.

use std::{error::Error, fmt, path::PathBuf, string::FromUtf8Error};

use thiserror::Error;

use crate::providers::error::CommandError;

/// Errors for the build action layer
#[derive(Error)]
pub enum BuildActionError {
    #[error(transparent)]
    WdkBuildConfig(#[from] wdk_build::ConfigError),
    #[error("Error Parsing Cargo.toml, not a valid rust project/workspace")]
    CargoMetadataParse(#[from] cargo_metadata::Error),
    #[error("Error Parsing WDK metadata from Cargo.toml, not a valid driver project/workspace")]
    WdkMetadataParse(#[from] wdk_build::metadata::TryFromCargoMetadataError),
    #[error(transparent)]
    BuildTask(#[from] BuildTaskError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    CommandExecution(#[from] CommandError),
    #[error("Not a workspace member, working directory: {0}")]
    NotAWorkspaceMember(PathBuf),
    #[error(transparent)]
    PackageTask(#[from] PackageTaskError),
    #[error("No valid rust projects in the current working directory: {0}")]
    NoValidRustProjectsInTheDirectory(PathBuf),
    #[error(
        "One or more rust (possibly driver) projects failed to build in the emulated workspace: \
         {0}"
    )]
    OneOrMoreRustProjectsFailedToBuild(PathBuf),
    #[error("One or more workspace members failed to build in the workspace: {0}")]
    OneOrMoreWorkspaceMembersFailedToBuild(PathBuf),
}

// Require explicit implementation since we log at multiple levels
impl fmt::Debug for BuildActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{self}")?;
        let mut source = self.source();
        while let Some(err) = source {
            writeln!(f, "Caused by: {err}")?;
            source = err.source();
        }
        Ok(())
    }
}

/// Errors for the low level build task layer
#[derive(Error, Debug)]
pub enum BuildTaskError {
    #[error("Error getting canonicalized path for manifest file")]
    CanonicalizeManifestPath(#[from] std::io::Error),
    #[error("Empty manifest path found error")]
    EmptyManifestPath,
    #[error("Error running cargo build command")]
    CargoBuild(#[from] CommandError),
}

/// Errors for the low level package task layer
#[derive(Error, Debug)]
pub enum PackageTaskError {
    #[error(
        "Missing .inx file in source path: {0}, Please ensure you are in a Rust driver project \
         directory."
    )]
    MissingInxSrcFile(PathBuf),
    #[error("Failed to copy file error, src: {0}, dest: {1}, error: {2:#?}")]
    CopyFile(PathBuf, PathBuf, std::io::Error),
    #[error("Error running stampinf command")]
    StampinfCommand(#[source] CommandError),
    #[error("Error running inf2cat command")]
    Inf2CatCommand(#[source] CommandError),
    #[error("Creating cert file from store using certmgr")]
    CreateCertFileFromStoreCommand(#[source] CommandError),
    #[error("Checking for existence of cert in store using certmgr")]
    VerifyCertExistsInStoreCommand(#[source] CommandError),
    #[error("Error reading stdout while checking for existence of cert in store using certmgr")]
    VerifyCertExistsInStoreInvalidCommandOutput(#[source] FromUtf8Error),
    #[error("Error generating certificate to cert store using makecert")]
    CertGenerationInStoreCommand(#[source] CommandError),
    #[error("Error signing driver binary using signtool")]
    DriverBinarySignCommand(#[source] CommandError),
    #[error("Error verifying signed driver binary using signtool")]
    DriverBinarySignVerificationCommand(#[source] CommandError),
    #[error("Error verifying inf file using infverif")]
    InfVerificationCommand(#[source] CommandError),

    // TODO: We can make this specific error instead of generic one
    #[error(transparent)]
    WdkBuildConfig(#[from] wdk_build::ConfigError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
