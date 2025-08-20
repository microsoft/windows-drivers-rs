// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! This module provides a wrapper around the `cargo-metadata` crate, offering
//! methods to retrieve metadata about Cargo projects. The module leverages the
//! `mockall` crate to enable mocking of its methods, facilitating easier unit
//! testing.

#![allow(clippy::unused_self)]

use std::path::Path;

#[derive(Default)]
pub struct Metadata {}

#[cfg_attr(test, mockall::automock)]
impl Metadata {
    /// Get the Cargo metadata at a given path.
    ///
    /// This function executes the `cargo metadata` command to retrieve the
    /// metadata for the Cargo project located at the specified path. The
    /// metadata includes information about the project's dependencies,
    /// targets, and other relevant details.
    ///
    /// # Arguments
    ///
    /// * `working_dir` - A reference to a `Path` that specifies the path to the
    ///   working directory.
    ///
    /// # Returns
    ///
    /// This function returns a
    /// `cargo_metadata::Result<cargo_metadata::Metadata>`, which is a
    /// result type that contains the `Metadata` on success or a
    /// `cargo_metadata::Error` on failure.
    ///
    /// # Errors
    ///
    /// This function will return an error if the `cargo metadata` command fails
    /// to execute or if the specified path is not a valid Cargo project.
    pub fn get_cargo_metadata_at_path(
        &self,
        working_dir: &Path,
    ) -> cargo_metadata::Result<cargo_metadata::Metadata> {
        cargo_metadata::MetadataCommand::new()
            .current_dir(working_dir)
            .exec()
    }
}
