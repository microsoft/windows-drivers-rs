// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Private module for utility code related to the cargo-make experience for
//! building drivers.

use std::{
    env,
    ffi::{CStr, OsStr},
    io,
    path::{Path, PathBuf},
};

use windows::{
    Win32::System::Registry::{
        HKEY,
        HKEY_LOCAL_MACHINE,
        KEY_READ,
        RRF_RT_REG_SZ,
        RegCloseKey,
        RegGetValueA,
        RegOpenKeyExA,
    },
    core::{PCSTR, s},
};

use crate::{ConfigError, CpuArchitecture, IoError, TwoPartVersion};

/// Detect `WDKContentRoot` Directory. Logic is based off of Toolset.props in
/// NI(22H2) WDK
#[must_use]
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
            let wdk_kit_version = env::var("WDKKitVersion").unwrap_or_else(|_| "10.0".to_string());
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

/// Searches a directory and determines the latest windows SDK version in that
/// directory
///
/// # Errors
///
/// Returns a `ConfigError::DirectoryNotFound` error if the directory provided
/// does not exist.
///
/// # Panics
///
/// Panics if the path provided is not valid Unicode.
pub fn get_latest_windows_sdk_version(path_to_search: &Path) -> Result<String, ConfigError> {
    Ok(path_to_search
        .read_dir()
        .map_err(|source| IoError::with_path(path_to_search, source))?
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
///
/// # Panics
///
/// Panics if the `CARGO_CFG_TARGET_ARCH` environment variable is not set,
/// or if the cargo architecture is unsupported.
#[must_use]
pub fn detect_cpu_architecture_in_build_script() -> CpuArchitecture {
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").expect(
        "Cargo should have set the CARGO_CFG_TARGET_ARCH environment variable when executing \
         build.rs",
    );

    CpuArchitecture::try_from_cargo_str(&target_arch).unwrap_or_else(|| {
        panic!("The target architecture, {target_arch}, is currently not supported.")
    })
}

/// Validates that a given string matches the WDK version format (10.xxx.yyy.zzz
/// where xxx, yyy, and zzz are numeric and not necessarily 3 digits long).
#[rustversion::attr(
    nightly,
    allow(
        clippy::nonminimal_bool,
        reason = "is_some_or is not stable until 1.82.0 is released on 10/17/24"
    )
)]
pub fn validate_wdk_version_format<S: AsRef<str>>(version_string: S) -> bool {
    let version = version_string.as_ref();
    let version_parts: Vec<&str> = version.split('.').collect();

    // First, check if we have "10" as our first value
    if version_parts.first().is_none_or(|first| *first != "10") {
        return false;
    }

    // Now check that we have four entries.
    if version_parts.len() != 4 {
        return false;
    }

    // Finally, confirm each part is numeric.
    if !version_parts
        .iter()
        .all(|version_part| version_part.parse::<i32>().is_ok())
    {
        return false;
    }

    true
}

/// Returns the version number from a full WDK version string.
///
/// # Errors
///
/// This function returns a [`ConfigError::WdkVersionStringFormatError`] if the
/// version string provided is ill-formed.
///
/// # Panics
///
/// If the WDK version format validation function is ever changed not to
/// validate that there are 4 substrings in the WDK version string, this
/// function will panic.
pub fn get_wdk_version_number<S: AsRef<str> + ToString + ?Sized>(
    version_string: &S,
) -> Result<String, ConfigError> {
    if !validate_wdk_version_format(version_string) {
        return Err(ConfigError::WdkVersionStringFormatError {
            version: version_string.to_string(),
        });
    }

    let version_substrings = version_string.as_ref().split('.').collect::<Vec<&str>>();
    let version_substring = version_substrings.get(2).expect(
        "WDK version string was validated to be well-formatted, but we couldn't get the \
         appropriate substring!",
    );
    Ok((*version_substring).to_string())
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
    unsafe { RegOpenKeyExA(key_handle, sub_key, 0, KEY_READ, &raw mut opened_key_handle) }
        .is_ok()
    {
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
                Some(&raw mut len),
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
                    Some(&raw mut len),
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
                            "RegGetValueA should always return a null-terminated string. The read \
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

/// Detects the Windows SDK version from the `Version_Number` env var or from
/// the WDK content's `Lib` directory.
///
/// # Arguments
/// * `wdk_content_root` - A reference to the path where the WDK content root is
///   located.
///
/// # Errors
///
/// Returns a `ConfigError::DirectoryNotFound` error if the directory provided
/// does not exist.
pub fn detect_windows_sdk_version(wdk_content_root: &Path) -> Result<String, ConfigError> {
    env::var("Version_Number")
        .or_else(|_| get_latest_windows_sdk_version(&wdk_content_root.join("Lib")))
}

/// Finds the maximum version in a directory where subdirectories are named with
/// version format "x.y"
pub fn find_max_version_in_directory<P: AsRef<Path>>(
    directory_path: P,
) -> Result<TwoPartVersion, IoError> {
    let directory_path = directory_path.as_ref();
    std::fs::read_dir(directory_path)
        .map_err(|source| IoError::with_path(directory_path, source))?
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|ft| ft.is_dir()))
        .filter_map(|entry| entry.file_name().to_str()?.parse().ok())
        .max()
        .ok_or_else(|| {
            IoError::with_path(
                directory_path,
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("Maximum version in {} not found", directory_path.display()),
                ),
            )
        })
}

/// Safely sets an environment variable. Will not compile if crate is not
/// targeted for Windows.
///
/// This function provides a safe wrapper around [`std::env::set_var`] that
/// became unsafe in Rust 2024 edition.
///
/// # Panics
///
/// This function may panic if key is empty, contains an ASCII equals sign '='
/// or the NUL character '\0', or when value contains the NUL character.
#[cfg(target_os = "windows")]
pub fn set_var<K, V>(key: K, value: V)
where
    K: AsRef<OsStr>,
    V: AsRef<OsStr>,
{
    // SAFETY: this function is only conditionally compiled for windows targets, and
    // env::set_var is always safe for windows targets
    unsafe {
        env::set_var(key, value);
    }
}

#[cfg(not(target_os = "windows"))]
pub fn set_var<K, V>(_key: K, _value: V)
where
    K: AsRef<OsStr>,
    V: AsRef<OsStr>,
{
    compile_error!(
        "windows-drivers-rs is designed to be run on a Windows host machine in a WDK environment. \
         Please build using a Windows target."
    );
}

/// Safely removes an environment variable. Will not compile if crate is not
/// targeted for Windows.
///
/// This function provides a safe wrapper around [`std::env::remove_var`] that
/// became unsafe in Rust 2024 edition.
///
/// # Panics
///
/// This function may panic if key is empty, contains an ASCII equals sign '='
/// or the NUL character '\0', or when value contains the NUL character.
#[allow(dead_code)]
#[cfg(target_os = "windows")]
pub fn remove_var<K>(key: K)
where
    K: AsRef<OsStr>,
{
    // SAFETY: this function is only conditionally compiled for windows targets, and
    // env::remove_var is always safe for windows targets
    unsafe {
        env::remove_var(key);
    }
}

#[allow(dead_code)]
#[cfg(not(target_os = "windows"))]
pub fn remove_var<K>(_key: K)
where
    K: AsRef<OsStr>,
{
    compile_error!(
        "windows-drivers-rs is designed to be run on a Windows host machine in a WDK environment. \
         Please build using a Windows target."
    );
}

#[cfg(test)]
mod tests {
    use assert_fs::prelude::*;

    use super::*;

    // Function with_clean_env clears the inputted environment variable and runs the
    // closure
    fn with_clean_env<F>(key: &str, f: F)
    where
        F: FnOnce(),
    {
        let original = env::var(key).ok();

        // SAFETY: We have verified that this is built for a Windows host due to no
        // compile errors from building `set_var`.
        unsafe {
            env::remove_var(key);
        }

        f();

        if let Some(val) = &original {
            // SAFETY: We have verified that this is built for a Windows host due to no
            // compile errors from building `set_var`.
            unsafe {
                env::set_var(key, val);
            }
        } else {
            // SAFETY: We have verified that this is built for a Windows host due to no
            // compile errors from building `set_var`.
            unsafe {
                env::remove_var(key);
            }
        }

        assert!(env::var(key).ok() == original);
    }

    mod read_registry_key_string_value {
        use windows::Win32::UI::Shell::{
            FOLDERID_ProgramFiles,
            KF_FLAG_DEFAULT,
            SHGetKnownFolderPath,
        };

        use super::*;

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
                    // SAFETY: program_files_dir pointer stays valid for reads up until and
                    // including its terminating null
                    unsafe { program_files_dir.to_string() }
                        .expect("Path resolved from FOLDERID_ProgramFiles should be valid UTF16.")
                )
            );
        }
    }

    #[test]
    fn validate_wdk_strings() {
        let test_string = "10.0.12345.0";
        assert_eq!(
            get_wdk_version_number(test_string).ok(),
            Some("12345".to_string())
        );
        let test_string = "10.0.5.0";
        assert_eq!(
            get_wdk_version_number(test_string).ok(),
            Some("5".to_string())
        );
        let test_string = "10.0.0.0";
        assert_eq!(
            get_wdk_version_number(test_string).ok(),
            Some("0".to_string())
        );
        let test_string = "11.0.0.0";
        assert_eq!(
            format!("{}", get_wdk_version_number(test_string).err().unwrap()),
            format!(
                "the WDK version string provided ({}) was not in a valid format",
                test_string
            )
        );
        let test_string = "10.0.12345.0.0";
        assert_eq!(
            format!("{}", get_wdk_version_number(test_string).err().unwrap()),
            format!(
                "the WDK version string provided ({}) was not in a valid format",
                test_string
            )
        );
        let test_string = "10.0.12345.a";
        assert_eq!(
            format!("{}", get_wdk_version_number(test_string).err().unwrap()),
            format!(
                "the WDK version string provided ({}) was not in a valid format",
                test_string
            )
        );
        let test_string = "10.0.12345";
        assert_eq!(
            format!("{}", get_wdk_version_number(test_string).err().unwrap()),
            format!(
                "the WDK version string provided ({}) was not in a valid format",
                test_string
            )
        );
        let test_string = "10.0.1234!5.0";
        assert_eq!(
            format!("{}", get_wdk_version_number(test_string).err().unwrap()),
            format!(
                "the WDK version string provided ({}) was not in a valid format",
                test_string
            )
        );
        let test_string = "Not a real version!";
        assert_eq!(
            format!("{}", get_wdk_version_number(test_string).err().unwrap()),
            format!(
                "the WDK version string provided ({}) was not in a valid format",
                test_string
            )
        );
        let test_string = "";
        assert_eq!(
            format!("{}", get_wdk_version_number(test_string).err().unwrap()),
            format!(
                "the WDK version string provided ({}) was not in a valid format",
                test_string
            )
        );
    }

    mod find_max_version_in_directory {
        use super::*;

        #[test]
        fn empty_directory() {
            let temp_dir = assert_fs::TempDir::new().unwrap();
            let result = find_max_version_in_directory(temp_dir.path());
            assert!(result.is_err());
            assert_eq!(
                result.unwrap_err().source.kind(),
                std::io::ErrorKind::NotFound
            );
        }

        #[test]
        fn nonexistent_directory() {
            let nonexistent_path = std::path::Path::new("/this/path/does/not/exist");
            let result = find_max_version_in_directory(nonexistent_path);
            assert!(result.is_err());
        }

        #[test]
        fn valid_version_directories() {
            // Single valid version directory
            let temp_dir = assert_fs::TempDir::new().unwrap();
            temp_dir.child("3.14").create_dir_all().unwrap();
            temp_dir.child("folder1").create_dir_all().unwrap();
            assert_eq!(
                find_max_version_in_directory(temp_dir.path()).unwrap(),
                TwoPartVersion(3, 14)
            );
            // Multiple valid version directories
            let temp_dir = assert_fs::TempDir::new().unwrap();
            temp_dir.child("1.2").create_dir_all().unwrap();
            temp_dir.child("1.10").create_dir_all().unwrap();
            temp_dir.child("2.0").create_dir_all().unwrap();
            temp_dir.child("not_a_version").create_dir_all().unwrap();
            assert_eq!(
                find_max_version_in_directory(temp_dir.path()).unwrap(),
                TwoPartVersion(2, 0)
            );
        }

        #[test]
        fn invalid_version_directories() {
            // Single invalid directory
            let temp_dir = assert_fs::TempDir::new().unwrap();
            temp_dir.child("folder1").create_dir_all().unwrap();
            let result = find_max_version_in_directory(temp_dir.path());
            assert!(result.is_err());
            assert_eq!(
                result.unwrap_err().source.kind(),
                std::io::ErrorKind::NotFound
            );

            // Multiple invalid directories
            let temp_dir = assert_fs::TempDir::new().unwrap();
            temp_dir.child("folder1").create_dir_all().unwrap();
            temp_dir.child("1.2.3").create_dir_all().unwrap(); // Too many dots
            temp_dir.child("a.b").create_dir_all().unwrap(); // Non-numeric
            temp_dir.child("1").create_dir_all().unwrap(); // No dot
            temp_dir.child("1.").create_dir_all().unwrap(); // Missing minor
            temp_dir.child(".5").create_dir_all().unwrap(); // Missing major
            let result = find_max_version_in_directory(temp_dir.path());
            assert!(result.is_err());
            assert_eq!(
                result.unwrap_err().source.kind(),
                std::io::ErrorKind::NotFound
            );
        }

        #[test]
        fn major_version_priority() {
            let temp_dir = assert_fs::TempDir::new().unwrap();
            temp_dir.child("1.999").create_dir_all().unwrap();
            temp_dir.child("2.0").create_dir_all().unwrap();
            temp_dir.child("1.1000").create_dir_all().unwrap();
            assert_eq!(
                find_max_version_in_directory(temp_dir.path()).unwrap(),
                TwoPartVersion(2, 0)
            );
        }

        #[test]
        fn minor_version_comparison() {
            let temp_dir = assert_fs::TempDir::new().unwrap();
            temp_dir.child("1.5").create_dir_all().unwrap();
            temp_dir.child("1.10").create_dir_all().unwrap();
            temp_dir.child("1.2").create_dir_all().unwrap();
            assert_eq!(
                find_max_version_in_directory(temp_dir.path()).unwrap(),
                TwoPartVersion(1, 10)
            );
        }

        #[test]
        fn zero_versions() {
            let temp_dir = assert_fs::TempDir::new().unwrap();
            temp_dir.child("0.0").create_dir_all().unwrap();
            temp_dir.child("0.1").create_dir_all().unwrap();
            assert_eq!(
                find_max_version_in_directory(temp_dir.path()).unwrap(),
                TwoPartVersion(0, 1)
            );
            temp_dir.child("1.0").create_dir_all().unwrap();
            assert_eq!(
                find_max_version_in_directory(temp_dir.path()).unwrap(),
                TwoPartVersion(1, 0)
            );
        }

        #[test]
        fn mixed_valid_and_invalid_entries() {
            let temp_dir = assert_fs::TempDir::new().unwrap();
            temp_dir.child("1.5").create_dir_all().unwrap();
            temp_dir.child("2.0").create_dir_all().unwrap();
            temp_dir.child("invalid").create_dir_all().unwrap();
            temp_dir.child("1.2.3").create_dir_all().unwrap(); // Invalid: too many dots
            temp_dir.child("a.b").create_dir_all().unwrap(); // Invalid: non-numeric
            temp_dir.child("not_version").touch().unwrap(); // File: ignored
            temp_dir.child("3.0").touch().unwrap(); // File: ignored
            // Should find the maximum among valid version directories only
            assert_eq!(
                find_max_version_in_directory(temp_dir.path()).unwrap(),
                TwoPartVersion(2, 0)
            );
        }
    }

    #[cfg(target_os = "windows")]
    mod safe_env_vars {
        use super::*;

        #[test]
        fn set_var_and_remove_var() {
            let key = "WDK_BUILD_TEST_VAR";
            with_clean_env(key, || {
                set_var(key, "test_value");
                assert_eq!(env::var(key).unwrap(), "test_value");
                remove_var(key);
                assert!(env::var(key).is_err());
            });
        }
    }
}
