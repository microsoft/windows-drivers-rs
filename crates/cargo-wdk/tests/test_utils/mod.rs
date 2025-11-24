//! Utility methods for tests.
//! Note: The current layout (`tests/test_utils/mod.rs`) is intentional; using a
//! subdirectory prevents Cargo from treating this as an independent integration
//! test crate and instead lets other tests import it as a regular module.

use std::{collections::HashMap, ffi::OsStr};

use fs4::fs_std::FileExt;

/// Sets the `RUSTFLAGS` environment variable to include `+crt-static`.
///
/// # Panics
/// * Panics if `RUSTFLAGS` is not set and setting it fails.
///
/// FIXME: This is needed for tests as "cargo make wdk-pre-commit-hook-flow"
/// somehow seems to mess with `RUSTFLAGS`.
pub fn set_crt_static_flag() {
    if let Ok(rustflags) = std::env::var("RUSTFLAGS") {
        let updated_rust_flags = format!("{rustflags} -C target-feature=+crt-static");
        set_var("RUSTFLAGS", updated_rust_flags);
        println!("RUSTFLAGS set, adding the +crt-static: {rustflags:?}");
    } else {
        set_var("RUSTFLAGS", "-C target-feature=+crt-static");
        println!(
            "No RUSTFLAGS set, setting it to: {:?}",
            std::env::var("RUSTFLAGS").expect("RUSTFLAGS not set")
        );
    }
}

/// Acquires an exclusive lock on a file and executes the provided closure.
/// This is useful for ensuring that only one instance of a test can run at a
/// time.
///
/// # Panics
/// * Panics if the lock file cannot be created.
/// * Panics if the lock cannot be acquired.
/// * Panics if the lock cannot be released.
pub fn with_file_lock<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    let lock_file = std::fs::File::create("cargo-wdk-test.lock")
        .expect("Unable to create lock file for cargo-wdk tests");
    FileExt::lock_exclusive(&lock_file).expect("Unable to cargo-wdk-test.lock file");
    let result = f();
    FileExt::unlock(&lock_file).expect("Unable to unlock cargo-wdk-test.lock file");
    result
}

#[allow(
    dead_code,
    reason = "This method is used only in build_command_test.rs; appears unused in other \
              integration test crates when running with --all-targets."
)]
/// Runs function after modifying environment variables, and returns the
/// function's return value.
///
/// The environment is guaranteed to be not modified during the execution
/// of the function, and the environment is reset to its original state
/// after execution of the function. No testing asserts should be called in
/// the function, since a failing test will poison the mutex, and cause all
/// remaining tests to fail.
///
/// # Panics
///
/// * Panics if called with duplicate environment variable keys.
/// * If the lock file cannot be created/locked/released.
pub fn with_env<K, V, F, R>(env_vars_key_value_pairs: &[(K, Option<V>)], f: F) -> R
where
    K: AsRef<OsStr> + std::cmp::Eq + std::hash::Hash,
    V: AsRef<OsStr>,
    F: FnOnce() -> R,
{
    with_file_lock(|| {
        let mut original_env_vars = HashMap::new();

        // set requested environment variables
        for (key, value) in env_vars_key_value_pairs {
            if let Ok(original_value) = std::env::var(key) {
                let insert_result = original_env_vars.insert(key, original_value);
                assert!(
                    insert_result.is_none(),
                    "Duplicate environment variable keys were provided"
                );
            }

            // Remove the env var if value is None
            if let Some(value) = value {
                set_var(key, value);
            } else {
                remove_var(key);
            }
        }

        let result = f();

        // reset all set environment variables
        for (key, _) in env_vars_key_value_pairs {
            original_env_vars.get(key).map_or_else(
                || {
                    remove_var(key);
                },
                |value| {
                    set_var(key, value);
                },
            );
        }

        result
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
        std::env::set_var(key, value);
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
        std::env::remove_var(key);
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
