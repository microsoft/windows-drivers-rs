// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

use std::{
    env,
    ffi::CStr,
    path::{Path, PathBuf},
};

use thiserror::Error;
use windows::{
    core::{s, PCSTR},
    Win32::System::Registry::{
        RegCloseKey,
        RegGetValueA,
        RegOpenKeyExA,
        HKEY,
        HKEY_LOCAL_MACHINE,
        KEY_READ,
        RRF_RT_REG_SZ,
    },
};

use crate::{CPUArchitecture, ConfigError};

/// Errors that may occur when stripping the extended path prefix from a path
#[derive(Debug, Error, PartialEq, Eq)]
pub enum StripExtendedPathPrefixError {
    #[error("provided path is empty")]
    EmptyPath,
    #[error("provided path has no extended path prefix to strip")]
    NoExtendedPathPrefix,
}
pub trait PathExt {
    type Error;

    fn strip_extended_length_path_prefix(&self) -> Result<PathBuf, Self::Error>;
}

impl<P> PathExt for P
where
    P: AsRef<Path>,
{
    type Error = StripExtendedPathPrefixError;

    fn strip_extended_length_path_prefix(&self) -> Result<PathBuf, Self::Error> {
        const EXTENDED_LENGTH_PATH_PREFIX: &str = r"\\?\";
        let mut path_components = self.as_ref().components();

        let path_prefix = match path_components.next() {
            Some(it) => it.as_os_str().to_string_lossy(),
            None => return Err(Self::Error::EmptyPath),
        };

        if path_prefix.len() < EXTENDED_LENGTH_PATH_PREFIX.len()
            || &path_prefix[..EXTENDED_LENGTH_PATH_PREFIX.len()] != EXTENDED_LENGTH_PATH_PREFIX
        {
            return Err(Self::Error::NoExtendedPathPrefix);
        }

        let mut path_without_prefix =
            PathBuf::from(&path_prefix[EXTENDED_LENGTH_PATH_PREFIX.len()..]);
        path_without_prefix.push(path_components.as_path());

        Ok(path_without_prefix)
    }
}

/// Detect `WDKContentRoot` Directory. Logic is based off of Toolset.props in
/// NI(22H2) WDK
pub fn detect_wdk_content_root() -> Option<PathBuf> {
    // If WDKContentRoot is present in environment(ex. running in an eWDK prompt),
    // use it
    if let Ok(wdk_content_root) = env::var("WDKContentRoot") {
        let path = Path::new(wdk_content_root.as_str());
        if path.is_dir() {
            return Some(path.to_path_buf());
        }
        eprintln!(
            "WDKContentRoot was detected to be {}, but does not exist or is not a valid directory.",
            path.display()
        );
    }

    // If MicrosoftKitRoot environment variable is set, use it to set WDKContentRoot
    if let Ok(microsoft_kit_root) = env::var("MicrosoftKitRoot") {
        let path = Path::new(microsoft_kit_root.as_str());

        if !path.is_absolute() {
            eprintln!(
                "MicrosoftKitRoot({}) was found in environment, but is not an absolute path.",
                path.display()
            );
        } else if !path.is_dir() {
            eprintln!(
                "MicrosoftKitRoot({}) was found in environment, but does not exist or is not a \
                 valid directory.",
                path.display()
            );
        } else {
            let wdk_kit_version =
                env::var("WDKKitVersion").map_or("10.0".to_string(), |version| version);
            let path = path.join("Windows Kits").join(wdk_kit_version);
            if path.is_dir() {
                return Some(path);
            }
            eprintln!(
                "WDKContentRoot was detected to be {}, but does not exist or is not a valid \
                 directory.",
                path.display()
            );
        }
    }

    // Check WDKContentRoot environment variable
    if let Ok(wdk_content_root) = env::var("WDKContentRoot") {
        let path = Path::new(wdk_content_root.as_str());
        if path.is_dir() {
            return Some(path.to_path_buf());
        }
        eprintln!(
            "WDKContentRoot({}) was found in environment, but does not exist or is not a valid \
             directory.",
            path.display()
        );
    }

    // Check HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows Kits\Installed
    // Roots@KitsRoot10 registry key
    if let Some(path) = read_registry_key_string_value(
        HKEY_LOCAL_MACHINE,
        s!(r"SOFTWARE\Microsoft\Windows Kits\Installed Roots"),
        s!(r"KitsRoot10"),
    ) {
        return Some(Path::new(path.as_str()).to_path_buf());
    }

    // Check HKEY_LOCAL_MACHINE\SOFTWARE\Wow6432Node\Microsoft\Windows
    // Kits\Installed Roots@KitsRoot10 registry key
    if let Some(path) = read_registry_key_string_value(
        HKEY_LOCAL_MACHINE,
        s!(r"SOFTWARE\Wow6432Node\Microsoft\Windows Kits\Installed Roots"),
        s!(r"KitsRoot10"),
    ) {
        return Some(Path::new(path.as_str()).to_path_buf());
    }

    None
}

/// Read a string value from a registry key
///
/// # Arguments
///
/// * `key_handle` - a [`windows::Win32::System::Registry::HKEY`] to the base
///   key
/// * `sub_key` - a [`windows::core::PCSTR`] that is the path of a registry key
///   relative to the `key_handle` argument
/// * `value` - a [`windows::core::PCSTR`] that is the name of the string
///   registry value to read
///
/// # Panics
///
/// Panics if read value isn't valid UTF-8 or if the opened regkey could not be
/// closed
fn read_registry_key_string_value(
    key_handle: HKEY,
    sub_key: PCSTR,
    value: PCSTR,
) -> Option<String> {
    let mut opened_key_handle = HKEY::default();
    let mut len = 0;
    if
    // SAFETY: `&mut opened_key_handle` is coerced to a &raw mut, so the address passed as the
    // argument is always valid. `&mut opened_key_handle` is coerced to a pointer of the correct
    // type.
    unsafe { RegOpenKeyExA(key_handle, sub_key, 0, KEY_READ, &mut opened_key_handle) }.is_ok() {
        if
        // SAFETY: `opened_key_handle` is valid key opened with the `KEY_QUERY_VALUE` access right
        // (included in `KEY_READ`). `&mut len` is coerced to a &raw mut, so the address passed as
        // the argument is always valid. `&mut len` is coerced to a pointer of the correct
        // type.
        unsafe {
            RegGetValueA(
                opened_key_handle,
                None,
                value,
                RRF_RT_REG_SZ,
                None,
                None,
                Some(&mut len),
            )
        }
        .is_ok()
        {
            let mut buffer = vec![0u8; len as usize];
            if
            // SAFETY: `opened_key_handle` is valid key opened with the `KEY_QUERY_VALUE` access
            // right (included in `KEY_READ`). `&mut buffer` is coerced to a &raw mut,
            // so the address passed as the argument is always valid. `&mut buffer` is
            // coerced to a pointer of the correct type. `&mut len` is coerced to a &raw
            // mut, so the address passed as the argument is always valid. `&mut len` is
            // coerced to a pointer of the correct type.
            unsafe {
                RegGetValueA(
                    opened_key_handle,
                    None,
                    value,
                    RRF_RT_REG_SZ,
                    None,
                    Some(buffer.as_mut_ptr().cast()),
                    Some(&mut len),
                )
            }
            .is_ok()
            {
                // SAFETY: `opened_key_handle` is valid opened key that was opened by
                // `RegOpenKeyExA`
                unsafe { RegCloseKey(opened_key_handle) }
                    .ok()
                    .expect("opened_key_handle should be successfully closed");
                return Some(
                    CStr::from_bytes_with_nul(&buffer[..len as usize])
                        .expect(
                            "RegGetValueA should always return a null terminated string. The read \
                             string (REG_SZ) from the registry should not contain any interior \
                             nulls.",
                        )
                        .to_str()
                        .expect("Registry value should be parseable as UTF8")
                        .to_string(),
                );
            }
        }

        // SAFETY: `opened_key_handle` is valid opened key that was opened by
        // `RegOpenKeyExA`
        unsafe { RegCloseKey(opened_key_handle) }
            .ok()
            .expect("opened_key_handle should be successfully closed");
    }
    None
}

/// Searches a directory and determines the latest windows SDK version in that
/// directory
pub fn get_latest_windows_sdk_version(path_to_search: &Path) -> Result<String, ConfigError> {
    Ok(path_to_search
        .read_dir()?
        .filter_map(std::result::Result::ok)
        .map(|valid_directory_entry| valid_directory_entry.path())
        .filter(|path| {
            path.is_dir()
                && path.file_name().is_some_and(|directory_name| {
                    directory_name
                        .to_str()
                        .is_some_and(|directory_name| directory_name.starts_with("10."))
                })
        })
        .max() // Get the latest SDK folder in case there are multiple installed
        .ok_or(ConfigError::DirectoryNotFound {
            directory: format!(
                "Windows SDK Directory in {}",
                path_to_search.to_string_lossy()
            ),
        })?
        .file_name()
        .expect("path should never terminate in ..")
        .to_str()
        .expect("directory name should always be valid Unicode")
        .to_string())
}

/// Detect architecture based on cargo TARGET variable.
pub fn detect_cpu_architecture_in_build_script() -> CPUArchitecture {
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").expect(
        "Cargo should have set the CARGO_CFG_TARGET_ARCH environment variable when executing \
         build.rs",
    );

    CPUArchitecture::try_from_cargo_str(&target_arch).unwrap_or_else(|| {
        panic!("The target architecture, {target_arch}, is currently not supported.")
    })
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::Shell::{FOLDERID_ProgramFiles, SHGetKnownFolderPath, KF_FLAG_DEFAULT};

    use super::*;

    #[test]
    fn strip_prefix_successfully() -> Result<(), StripExtendedPathPrefixError> {
        assert_eq!(
            PathBuf::from(r"\\?\C:\Program Files")
                .strip_extended_length_path_prefix()?
                .to_str(),
            Some(r"C:\Program Files")
        );
        Ok(())
    }

    #[test]
    fn empty_path() {
        assert_eq!(
            PathBuf::from("").strip_extended_length_path_prefix(),
            Err(StripExtendedPathPrefixError::EmptyPath)
        );
    }

    #[test]
    fn path_too_short() {
        assert_eq!(
            PathBuf::from(r"C:\").strip_extended_length_path_prefix(),
            Err(StripExtendedPathPrefixError::NoExtendedPathPrefix)
        );
    }

    #[test]
    fn no_prefix_to_strip() {
        assert_eq!(
            PathBuf::from(r"C:\Program Files").strip_extended_length_path_prefix(),
            Err(StripExtendedPathPrefixError::NoExtendedPathPrefix)
        );
    }

    #[test]
    fn read_reg_key_programfilesdir() {
        let program_files_dir =
            // SAFETY: FOLDERID_ProgramFiles is a constant from the windows crate, so the pointer (resulting from its reference being coerced) is always valid to be dereferenced
            unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramFiles, KF_FLAG_DEFAULT, None) }
                .expect("Program Files Folder should always resolve via SHGetKnownFolderPath.");

        assert_eq!(
            read_registry_key_string_value(
                HKEY_LOCAL_MACHINE,
                s!(r"SOFTWARE\Microsoft\Windows\CurrentVersion"),
                s!("ProgramFilesDir")
            ),
            Some(
                // SAFETY: program_files_dir pointer stays valid for reads up until and including
                // its terminating null
                unsafe { program_files_dir.to_string() }
                    .expect("Path resolved from FOLDERID_ProgramFiles should be valid UTF16.")
            )
        );
    }
}
