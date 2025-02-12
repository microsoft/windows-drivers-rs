use std::path::PathBuf;

use mockall::automock;

pub(crate) struct FS {}

#[automock]
pub(crate) trait FSProvider {
    fn rename(&self, src: &PathBuf, dest: &PathBuf) -> Result<(), std::io::Error>;
    fn canonicalize_path(&self, path: PathBuf) -> Result<PathBuf, std::io::Error>;
    fn copy(&self, src: &PathBuf, dest: &PathBuf) -> Result<u64, std::io::Error>;
    fn exists(&self, path: &PathBuf) -> bool;
    fn create_dir(&self, path: &PathBuf) -> Result<(), std::io::Error>;
}

impl FSProvider for FS {
    fn canonicalize_path(&self, path: PathBuf) -> Result<PathBuf, std::io::Error> {
        path.canonicalize()
    }

    fn copy(&self, src: &PathBuf, dest: &PathBuf) -> Result<u64, std::io::Error> {
        std::fs::copy(src, dest)
    }

    fn exists(&self, path: &PathBuf) -> bool {
        path.exists()
    }
    
    fn create_dir(&self, path: &PathBuf) -> Result<(), std::io::Error> {
        std::fs::create_dir(path)
    }

    fn rename(&self, src: &PathBuf, dest: &PathBuf) -> Result<(), std::io::Error> {
        std::fs::rename(src, dest)
    }
}