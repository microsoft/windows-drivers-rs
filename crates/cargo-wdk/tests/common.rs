//! Common methods for tests.

#![allow(clippy::literal_string_with_formatting_args)]

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
        std::env::set_var("RUSTFLAGS", updated_rust_flags);
        println!("RUSTFLAGS set, adding the +crt-static: {rustflags:?}");
    } else {
        std::env::set_var("RUSTFLAGS", "-C target-feature=+crt-static");
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
pub fn with_file_lock<F>(f: F)
where
    F: FnOnce(),
{
    let lock_file = std::fs::File::create("cargo-wdk-test.lock")
        .expect("Unable to create lock file for cargo-wdk tests");
    FileExt::lock_exclusive(&lock_file).expect("Unable to cargo-wdk-test.lock file");
    f();
    FileExt::unlock(&lock_file).expect("Unable to unlock cargo-wdk-test.lock file");
}
