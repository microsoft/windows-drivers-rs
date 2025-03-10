// Warns the run method is not used, however it is used.
// The intellisense confusion seems to come from automock
#![allow(dead_code)]
#![allow(clippy::unused_self)]
use std::path::Path;

use cargo_metadata::Metadata;
use mockall::automock;

/// Provides limited access to wdk-build crate methods
#[derive(Default)]
pub struct WdkBuild {}

#[automock]
impl WdkBuild {
    pub fn get_cargo_metadata_at_path(
        &self,
        manifest_path: &Path,
    ) -> cargo_metadata::Result<Metadata> {
        wdk_build::metadata::get_cargo_metadata_at_path(manifest_path)
    }

    pub fn detect_wdk_build_number(&self) -> Result<u32, wdk_build::ConfigError> {
        wdk_build::detect_wdk_build_number()
    }
}
