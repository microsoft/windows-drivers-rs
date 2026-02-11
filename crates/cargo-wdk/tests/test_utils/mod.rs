//! Utility methods for tests.
//! Note: The current layout (`tests/test_utils/mod.rs`) is intentional; using a
//! subdirectory prevents Cargo from treating this as an independent integration
//! test crate and instead lets other tests import it as a regular module.

use std::{
    collections::HashMap,
    env,
    ffi::{CStr, CString, OsStr},
    marker::PhantomData,
    path::Path,
    process::Command,
};

use assert_cmd::cargo::CommandCargoExt;
use windows::{
    Win32::{
        Foundation::{CloseHandle, GetLastError, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0},
        System::Threading::{CreateMutexA, INFINITE, ReleaseMutex, WaitForSingleObject},
    },
    core::{Error as WinError, PCSTR},
};
/// Acquires a system-wide mutex with the given name and executes
/// the provided closure
///
/// # Panics
/// * Panics if the provided name is not a valid C string.
pub fn with_mutex<F, R>(mutex_name: &str, f: F) -> R
where
    F: FnOnce() -> R,
{
    // Append an arbitrary suffix to minimize the chance of
    // collisions with something else on the machine
    let mutex_name = format!("{mutex_name}_104da4527a7");
    let mutex_name = CString::new(mutex_name).expect("mutex_name is not a valid C string");
    let _mutex = NamedMutex::acquire(&mutex_name).expect("failed to acquire mutex");

    f()
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
    with_mutex("env", || {
        let mut original_env_vars = HashMap::new();

        // set requested environment variables
        for (key, value) in env_vars_key_value_pairs {
            if let Ok(original_value) = env::var(key) {
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

/// An RAII wrapper over a Win API named mutex
pub struct NamedMutex {
    handle: HANDLE,
    // `ReleaseMutex` requires that it is called
    // only by threads that own the mutex handle.
    // Being `!Send` ensures that's always the case.
    _not_send: PhantomData<*const ()>,
}

impl NamedMutex {
    /// Acquires named mutex
    pub fn acquire(name: &CStr) -> Result<Self, WinError> {
        fn get_last_error() -> WinError {
            // SAFETY: We have to just assume this function is safe to call
            // because the windows crate has no documentation for it and
            // the MSDN documentation does not specify any preconditions
            // for calling it
            unsafe { GetLastError().into() }
        }

        // SAFETY: The name ptr is valid because it comes from a CStr
        let handle = unsafe { CreateMutexA(None, false, PCSTR(name.as_ptr().cast()))? };
        if handle.is_invalid() {
            return Err(get_last_error());
        }

        // SAFETY: The handle is valid since it was created right above
        match unsafe { WaitForSingleObject(handle, INFINITE) } {
            res if res == WAIT_OBJECT_0 || res == WAIT_ABANDONED => Ok(Self {
                handle,
                _not_send: PhantomData,
            }),
            _ => {
                // SAFETY: The handle is valid since it was created right above
                unsafe { CloseHandle(handle)? };
                Err(get_last_error())
            }
        }
    }
}

impl Drop for NamedMutex {
    fn drop(&mut self) {
        // SAFETY: the handle is guaranteed to be valid
        // because this type itself created it and it
        // was never exposed outside. Also the requirement
        // that the calling thread must own the handle
        // is upheld because this type is `!Send`
        let _ = unsafe { ReleaseMutex(self.handle) };

        // SAFETY: the handle is valid as explained above.
        let _ = unsafe { CloseHandle(self.handle) };
    }
}

/// Creates an [`std::process::Command`] object representing
/// a cargo-wdk command invocation for tests.
///
/// It automatically locates the cargo-wdk binary and makes
/// sure its environment variables are set correctly e.g.
/// by removing those that might interfere with its operation.
///
/// # Arguments
///
/// * `cmd_name` - Name of the cargo-wdk command. Can be only "new" or "build"
/// * `cmd_args` - Optional args for the command
/// * `env_vars` - Optional environment variables to overlay for the command
/// * `curr_working_dir` - Optional current working directory for the command
pub fn create_cargo_wdk_cmd<P: AsRef<Path>>(
    cmd_name: &str,
    cmd_args: Option<&[&str]>,
    env_vars: Option<&[(&str, Option<String>)]>,
    curr_working_dir: Option<P>,
) -> Command {
    assert!(
        cmd_name == "build" || cmd_name == "new",
        "Only 'build' and 'new' commands are supported"
    );

    let mut cmd = Command::cargo_bin("cargo-wdk").expect("unable to find cargo-wdk binary");

    let mut args = vec![cmd_name];
    if let Some(cmd_args) = cmd_args {
        args.extend(cmd_args);
    }
    cmd.args(args);

    sanitize_env_vars(&mut cmd);

    if let Some(env_vars) = env_vars {
        for (key, value) in env_vars {
            match value {
                Some(value) => {
                    cmd.env(key, value);
                }
                None => {
                    cmd.env_remove(key);
                }
            }
        }
    }

    if let Some(curr_working_dir) = curr_working_dir {
        cmd.current_dir(curr_working_dir);
    }

    if cmd_name == "build" {
        // RUSTFLAGS is relevant only for cargo wdk build
        cmd.env("RUSTFLAGS", "-C target-feature=+crt-static");
    }

    cmd
}

/// Makes sure the given command is free of environment
/// variables typically set by cargo.
///
/// This is useful when both the command and its parent
/// process is a cargo invocation. In such situations
/// the parent might set cargo-related environment
/// variables that might affect the child.
///
/// This function wipes the slate clean and ensures
/// the child runs in a clean environment.
///
/// In particular, this function removes:
/// - All env vars starting with `CARGO` or `RUST` except `CARGO_HOME` and
///   `RUSTUP_HOME`
/// - Entries added to the "PATH" variable by cargo
fn sanitize_env_vars(cmd: &mut Command) {
    const PATH_VAR: &str = "PATH";

    // Remove all vars added by cargo
    let vars_to_remove = env::vars().filter_map(|(var, _)| {
        let var_upper = var.to_uppercase();
        if (var_upper.starts_with("CARGO") || var_upper.starts_with("RUST"))
            // Leaving these two in as removing them can cause
            // issues with finding toolchains installed at 
            // non-default locations
            && var_upper != "CARGO_HOME"
            && var_upper != "RUSTUP_HOME"
        {
            Some(var)
        } else {
            None
        }
    });

    for var in vars_to_remove {
        cmd.env_remove(var);
    }

    // Remove paths in the PATH variable that were
    // added by cargo
    let path_value = env::var(PATH_VAR).expect("PATH env var not found");
    let paths = env::split_paths(&path_value);

    let paths_to_keep = paths.filter(|path| {
        // Paths we are looking to remove are those added by
        // cargo-llvm-cov, which may be used to run tests, and
        // Rust toolchain paths
        !(path.ends_with("target/llvm-cov-target/debug")
            || path.ends_with("target/llvm-cov-target/debug/deps")
            || path.ends_with("target/llvm-cov-target/release")
            || path.ends_with("target/llvm-cov-target/release/deps")
            || path
                .to_string_lossy()
                .replace('\\', "/")
                .contains(".rustup/toolchain"))
    });

    let new_value = env::join_paths(paths_to_keep).expect("unable to join PATH entries");

    cmd.env(PATH_VAR, new_value);
}
