// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! Abstracts running Cargo commands across the action layer.

use std::{ffi::OsString, path::Path, process::Output};

use anyhow::{Result, anyhow};
use mockall_double::double;

#[double]
use crate::providers::exec::CommandExec;

/// Returns the Cargo executable specified by the `CARGO` env var.
///
/// # Errors
/// Returns an error if `CARGO` env var is missing or empty.
pub fn cargo_path() -> Result<OsString> {
    cargo_path_from_env(std::env::var_os("CARGO"))
}

fn cargo_path_from_env(value: Option<OsString>) -> Result<OsString> {
    match value {
        None => Err(anyhow!("`CARGO` environment variable is not set")),
        Some(path) if path.is_empty() => Err(anyhow!("`CARGO` environment variable is empty")),
        Some(path) => Ok(path),
    }
}

/// Runs the Cargo executable specified by the `CARGO` env var.
///
/// # Errors
/// Returns an error if `CARGO` is missing or empty, or if the command fails.
pub fn run_cargo(
    command_exec: &CommandExec,
    args: &[&str],
    working_dir: Option<&Path>,
) -> Result<Output> {
    let cargo = cargo_path()?;
    Ok(command_exec.run(&cargo, args, None, working_dir)?)
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};

    use super::cargo_path_from_env;

    #[test]
    fn missing_cargo_is_an_error() {
        let error = cargo_path_from_env(None).unwrap_err();
        assert_eq!(error.to_string(), "`CARGO` environment variable is not set");
    }

    #[test]
    fn empty_cargo_is_an_error() {
        let error = cargo_path_from_env(Some(OsString::new())).unwrap_err();
        assert_eq!(error.to_string(), "`CARGO` environment variable is empty");
    }

    #[test]
    fn non_unicode_cargo_path_is_preserved() {
        let path = OsString::from_wide(&[0x0043, 0x003A, 0x005C, 0xD800]);
        assert_eq!(cargo_path_from_env(Some(path.clone())).unwrap(), path);
    }

    #[test]
    fn cargo_path_is_preserved() {
        let path = r"C:\nonexistent cargo-wdk test toolchain\bin\cargo.exe";
        assert!(!std::path::Path::new(path).exists());
        assert_eq!(cargo_path_from_env(Some(path.into())).unwrap(), path);
    }
}
