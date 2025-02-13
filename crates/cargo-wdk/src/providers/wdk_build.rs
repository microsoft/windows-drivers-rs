use std::path::PathBuf;

use cargo_metadata::Metadata;
use mockall::automock;
pub(crate) struct WdkBuild {}

#[automock]
pub(crate) trait WdkBuildProvider {
    fn get_cargo_metadata_at_path(&self, manifest_path: &PathBuf) -> cargo_metadata::Result<Metadata>;
    fn detect_wdk_build_number(&self) -> Result<u32, wdk_build::ConfigError>;
}

impl WdkBuildProvider for WdkBuild {
    fn get_cargo_metadata_at_path(&self, manifest_path: &PathBuf) -> cargo_metadata::Result<Metadata> {
        wdk_build::metadata::get_cargo_metadata_at_path(manifest_path)
    }

    fn detect_wdk_build_number(&self) -> Result<u32, wdk_build::ConfigError> {
        wdk_build::detect_wdk_build_number()
    }
}