// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

use std::path::{Path, PathBuf};

use mockall_double::double;

#[double]
use crate::providers::fs::Fs;
use crate::providers::{error::FileError, fs::DirEntryInfo};

/// Iterates over immediate subdirectories that contain a `Cargo.toml`.
pub(super) struct CargoProjectIterator<'a> {
    fs: &'a Fs,
    entries: std::vec::IntoIter<DirEntryInfo>,
}

impl<'a> CargoProjectIterator<'a> {
    /// Reads the immediate entries under `working_dir` and prepares to yield
    /// only Rust project directories.
    pub(super) fn new(fs: &'a Fs, working_dir: &Path) -> Result<Self, FileError> {
        Ok(Self {
            fs,
            entries: fs.read_dir_entries(working_dir)?.into_iter(),
        })
    }
}

impl Iterator for CargoProjectIterator<'_> {
    type Item = PathBuf;

    fn next(&mut self) -> Option<Self::Item> {
        self.entries.find_map(|entry| {
            (entry.is_dir && self.fs.exists(&entry.path.join("Cargo.toml"))).then_some(entry.path)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use mockall::predicate::eq;
    use mockall_double::double;

    use super::CargoProjectIterator;
    use crate::providers::fs::DirEntryInfo;
    #[double]
    use crate::providers::fs::Fs;

    fn mock_entries(fs: &mut Fs, root: &Path, entries: &[(&str, bool)]) {
        let root = root.to_owned();
        let entries = entries
            .iter()
            .map(|(name, is_dir)| DirEntryInfo {
                path: root.join(name),
                is_dir: *is_dir,
            })
            .collect::<Vec<_>>();
        fs.expect_read_dir_entries()
            .with(eq(root))
            .times(1)
            .return_once(move |_| Ok(entries));
    }

    #[test]
    fn yields_only_directories_with_cargo_toml() {
        let root = PathBuf::from("C:\\tmp");
        let docs = root.join("docs");
        let package = root.join("package");
        let mut fs = Fs::default();

        mock_entries(
            &mut fs,
            &root,
            &[("README.md", false), ("docs", true), ("package", true)],
        );
        fs.expect_exists()
            .with(eq(docs.join("Cargo.toml")))
            .times(1)
            .return_const(false);
        fs.expect_exists()
            .with(eq(package.join("Cargo.toml")))
            .times(1)
            .return_const(true);

        let projects = CargoProjectIterator::new(&fs, &root)
            .expect("directory enumeration should succeed")
            .collect::<Vec<_>>();

        assert_eq!(projects, vec![package]);
    }

    #[test]
    fn empty_iterator_when_no_rust_projects_exist() {
        let root = PathBuf::from("C:\\tmp");
        let docs = root.join("docs");
        let mut fs = Fs::default();

        mock_entries(&mut fs, &root, &[("docs", true)]);
        fs.expect_exists()
            .with(eq(docs.join("Cargo.toml")))
            .times(1)
            .return_const(false);

        assert!(
            CargoProjectIterator::new(&fs, &root)
                .expect("directory enumeration should succeed")
                .next()
                .is_none()
        );
    }
}
