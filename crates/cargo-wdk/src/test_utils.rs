use std::{collections::HashMap, ffi::OsStr, sync::Mutex};

/// This is a helper function used in child module unit tests.
///
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
/// Panics if called with duplicate environment variable keys.
pub fn with_env<K, V, F, R>(env_vars_key_value_pairs: &[(K, Option<V>)], f: F) -> R
where
    K: AsRef<OsStr> + std::cmp::Eq + std::hash::Hash,
    V: AsRef<OsStr>,
    F: FnOnce() -> R,
{
    // Tests can execute in multiple threads in the same process, so mutex must be
    // used to guard access to the environment variables
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    let _mutex_guard = ENV_MUTEX.lock().unwrap();
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

    let f_return_value = f();

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

    f_return_value
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
