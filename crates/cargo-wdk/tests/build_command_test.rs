//! System level tests for cargo wdk build flow
mod test_utils;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Condvar, Mutex, OnceLock},
    thread,
    time::Duration,
};

use assert_cmd::prelude::*;
use sha2::{Digest, Sha256};
use test_utils::{set_crt_static_flag, with_env, with_mutex};

const STAMPINF_VERSION_ENV_VAR: &str = "STAMPINF_VERSION";
const X86_64_TARGET_TRIPLE_NAME: &str = "x86_64-pc-windows-msvc";
const AARCH64_TARGET_TRIPLE_NAME: &str = "aarch64-pc-windows-msvc";

#[derive(Clone, Copy)]
enum TestIsolation {
    Shared,
    Exclusive,
}

fn run_test<F, R>(isolation: TestIsolation, f: F) -> R
where
    F: FnOnce() -> R,
{
    match isolation {
        TestIsolation::Shared => {
            let _guard = test_scheduler().acquire_shared();
            f()
        }
        TestIsolation::Exclusive => {
            let _guard = test_scheduler().acquire_exclusive();
            f()
        }
    }
}

fn test_scheduler() -> &'static TestScheduler {
    static SCHEDULER: OnceLock<TestScheduler> = OnceLock::new();
    SCHEDULER.get_or_init(TestScheduler::new)
}

struct TestScheduler {
    state: Mutex<SchedulerState>,
    cv: Condvar,
}

impl TestScheduler {
    const fn new() -> Self {
        Self {
            state: Mutex::new(SchedulerState {
                shared_count: 0,
                exclusive_active: false,
            }),
            cv: Condvar::new(),
        }
    }

    fn acquire_shared(&'static self) -> SharedGuard {
        let mut state = self.state.lock().unwrap();
        while state.exclusive_active {
            state = self.cv.wait(state).unwrap();
        }
        state.shared_count += 1;
        drop(state);
        SharedGuard { scheduler: self }
    }

    fn acquire_exclusive(&'static self) -> ExclusiveGuard {
        let mut state = self.state.lock().unwrap();
        while state.exclusive_active || state.shared_count > 0 {
            state = self.cv.wait(state).unwrap();
        }
        state.exclusive_active = true;
        drop(state);
        ExclusiveGuard { scheduler: self }
    }

    fn release_shared(&self) {
        let mut state = self.state.lock().unwrap();
        state.shared_count -= 1;
        let should_notify = state.shared_count == 0;
        drop(state);
        if should_notify {
            self.cv.notify_all();
        }
    }

    fn release_exclusive(&self) {
        let mut state = self.state.lock().unwrap();
        state.exclusive_active = false;
        drop(state);
        self.cv.notify_all();
    }
}

struct SharedGuard {
    scheduler: &'static TestScheduler,
}

impl Drop for SharedGuard {
    fn drop(&mut self) {
        self.scheduler.release_shared();
    }
}

struct ExclusiveGuard {
    scheduler: &'static TestScheduler,
}

impl Drop for ExclusiveGuard {
    fn drop(&mut self) {
        self.scheduler.release_exclusive();
    }
}

struct SchedulerState {
    shared_count: usize,
    exclusive_active: bool,
}

#[test]
fn mixed_package_kmdf_workspace_builds_successfully() {
    run_test(TestIsolation::Exclusive, || {
        let path = "tests/mixed-package-kmdf-workspace";
        with_mutex(path, || {
            run_cargo_clean(path);
            let stdout = run_build_cmd(path, None, None);
            assert!(stdout.contains("Building package driver"));
            assert!(stdout.contains("Building package non_driver_crate"));
            let params = VerificationParams {
                path,
                driver_type: "kmdf",
                driver_name: "driver",
                driver_version: None,
                target_triple: None,
                profile: "debug",
            };
            verify_driver_package_files(&params);
        });
    });
}

#[test]
fn kmdf_driver_builds_successfully() {
    run_test(TestIsolation::Shared, || {
        // Setup for executables
        wdk_build::cargo_make::setup_path().expect("failed to set up paths for executables");
        let driver = "kmdf-driver";
        let driver_path = format!("tests/{driver}");
        // Create a self signed certificate in store if not already present
        let output = Command::new("certmgr.exe")
            .args(["-s", "WDRTestCertStore"])
            .output()
            .expect("failed to check for certificates in store");

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);

        if !stdout.contains("WDRLocalTestCert") {
            let args = [
                "-r",
                "-pe",
                "-a",
                "SHA256",
                "-eku",
                "1.3.6.1.5.5.7.3.3",
                "-ss",
                "WDRTestCertStore",
                "-n",
                "CN=WDRLocalTestCert",
            ];

            let output = Command::new("makecert").args(args).output().unwrap();
            assert!(output.status.success());
        }

        clean_build_and_verify_driver_project("kmdf", &driver_path, driver, None, None, None, None);
    });
}

#[test]
fn umdf_driver_builds_successfully() {
    run_test(TestIsolation::Shared, || {
        let driver = "umdf-driver";
        let driver_path = format!("tests/{driver}");
        clean_build_and_verify_driver_project("umdf", &driver_path, driver, None, None, None, None);
    });
}

#[test]
fn wdm_driver_builds_successfully() {
    run_test(TestIsolation::Shared, || {
        let driver = "wdm-driver";
        let driver_path = format!("tests/{driver}");
        clean_build_and_verify_driver_project("wdm", &driver_path, driver, None, None, None, None);
    });
}

#[test]
fn wdm_driver_builds_successfully_with_given_version() {
    run_test(TestIsolation::Shared, || {
        let driver = "wdm-driver";
        let driver_path = format!("tests/{driver}");
        let env = &[(STAMPINF_VERSION_ENV_VAR, Some("5.1.0"))];
        with_env(env, || {
            clean_build_and_verify_driver_project(
                "wdm",
                &driver_path,
                driver,
                Some("5.1.0.0"),
                None,
                Some(env),
                None,
            );
        });
    });
}

#[test]
fn emulated_workspace_builds_successfully() {
    run_test(TestIsolation::Exclusive, || {
        let emulated_workspace_path = "tests/emulated-workspace";
        let umdf_driver_workspace_path = format!("{emulated_workspace_path}/umdf-driver-workspace");
        with_mutex(emulated_workspace_path, || {
            run_cargo_clean(&umdf_driver_workspace_path);
            run_cargo_clean(&format!("{emulated_workspace_path}/rust-project"));
            let stdout = run_build_cmd(emulated_workspace_path, None, None);
            assert!(stdout.contains("Building package driver_1"));
            assert!(stdout.contains("Building package driver_2"));
            assert!(stdout.contains("Build completed successfully"));
            let params = VerificationParams {
                path: &umdf_driver_workspace_path,
                driver_type: "umdf",
                driver_name: "driver_1",
                driver_version: None,
                profile: "debug",
                target_triple: None,
            };
            verify_driver_package_files(&params);
            let params = VerificationParams {
                path: &umdf_driver_workspace_path,
                driver_type: "umdf",
                driver_name: "driver_2",
                driver_version: None,
                profile: "debug",
                target_triple: None,
            };
            verify_driver_package_files(&params);
        });
    });
}

#[test]
fn kmdf_driver_with_target_arch_cli_option_builds_successfully() {
    run_test(TestIsolation::Exclusive, || {
        let driver = "kmdf-driver";
        let driver_path = format!("tests/{driver}");
        let target_arch = "ARM64";
        let args = BuildArgs {
            target_arch: Some(target_arch),
            profile: None,
        };
        if let Ok(nuget_package_root) = std::env::var("NugetPackagesRoot") {
            let wdk_content_root = get_nuget_wdk_content_root(target_arch, &nuget_package_root);
            let env = &[("WDKContentRoot", Some(wdk_content_root.as_str()))];
            with_env(env, || {
                clean_build_and_verify_driver_project(
                    "kmdf",
                    &driver_path,
                    driver,
                    None,
                    Some(&args),
                    Some(env),
                    Some(target_arch),
                );
            });
        } else {
            clean_build_and_verify_driver_project(
                "kmdf",
                &driver_path,
                driver,
                None,
                Some(&args),
                None,
                Some(target_arch),
            );
        }
    });
}

// `config.toml` with `build.target` = "x86_64-pc-windows-msvc"
#[test]
fn kmdf_driver_with_target_override_via_config_toml() {
    run_test(TestIsolation::Exclusive, || {
        let driver = "kmdf-driver-with-target-override";
        let driver_path = format!("tests/{driver}");
        let target_arch = "x64";
        if let Ok(nuget_package_root) = std::env::var("NugetPackagesRoot") {
            let wdk_content_root = get_nuget_wdk_content_root(target_arch, &nuget_package_root);
            with_env(
                &[("WDKContentRoot", Some(wdk_content_root.as_str()))],
                || {
                    clean_build_and_verify_driver_project(
                        "kmdf",
                        &driver_path,
                        driver,
                        None,
                        None,
                        None,
                        Some(target_arch),
                    );
                },
            );
        } else {
            clean_build_and_verify_driver_project(
                "kmdf",
                &driver_path,
                driver,
                None,
                None,
                None,
                Some(target_arch),
            );
        }
    });
}

#[test]
fn kmdf_driver_with_target_override_env_wins() {
    run_test(TestIsolation::Exclusive, || {
        let driver = "kmdf-driver-with-target-override";
        let driver_path = format!("tests/{driver}");
        let target_arch = "ARM64";
        if let Ok(nuget_package_root) = std::env::var("NugetPackagesRoot") {
            let wdk_content_root = get_nuget_wdk_content_root(target_arch, &nuget_package_root);
            let env = &[
                ("CARGO_BUILD_TARGET", Some(AARCH64_TARGET_TRIPLE_NAME)),
                ("WDKContentRoot", Some(wdk_content_root.as_str())),
            ];
            with_env(env, || {
                clean_build_and_verify_driver_project(
                    "kmdf",
                    &driver_path,
                    driver,
                    None,
                    None,
                    Some(env),
                    Some(target_arch),
                );
            });
        } else {
            let env = &[("CARGO_BUILD_TARGET", Some(AARCH64_TARGET_TRIPLE_NAME))];
            with_env(env, || {
                clean_build_and_verify_driver_project(
                    "kmdf",
                    &driver_path,
                    driver,
                    None,
                    None,
                    Some(env),
                    Some(target_arch),
                );
            });
        }
    });
}

#[test]
fn kmdf_driver_with_target_override_cli_wins() {
    run_test(TestIsolation::Exclusive, || {
        let driver = "kmdf-driver-with-target-override";
        let driver_path = format!("tests/{driver}");
        let target_arch = "ARM64";
        let build_args = BuildArgs {
            target_arch: Some(target_arch),
            profile: None,
        };
        if let Ok(nuget_package_root) = std::env::var("NugetPackagesRoot") {
            let wdk_content_root = get_nuget_wdk_content_root(target_arch, &nuget_package_root);
            let env = &[
                ("CARGO_BUILD_TARGET", Some(X86_64_TARGET_TRIPLE_NAME)),
                ("WDKContentRoot", Some(wdk_content_root.as_str())),
            ];
            with_env(env, || {
                clean_build_and_verify_driver_project(
                    "kmdf",
                    &driver_path,
                    driver,
                    None,
                    Some(&build_args),
                    Some(env),
                    Some(target_arch),
                );
            });
        } else {
            let env = &[("CARGO_BUILD_TARGET", Some(AARCH64_TARGET_TRIPLE_NAME))];
            with_env(env, || {
                clean_build_and_verify_driver_project(
                    "kmdf",
                    &driver_path,
                    driver,
                    None,
                    Some(&build_args),
                    Some(env),
                    Some(target_arch),
                );
            });
        }
    });
}

#[test]
fn umdf_driver_with_target_arch_and_release_profile() {
    run_test(TestIsolation::Exclusive, || {
        let driver_path = "tests/umdf-driver";
        let target_arch = "ARM64";
        let profile = "release";
        let build_args = BuildArgs {
            target_arch: Some(target_arch),
            profile: Some(profile),
        };
        if let Ok(nuget_package_root) = std::env::var("NugetPackagesRoot") {
            let wdk_content_root = get_nuget_wdk_content_root(target_arch, &nuget_package_root);
            let env = &[("WDKContentRoot", Some(wdk_content_root.as_str()))];
            with_env(env, || {
                clean_build_and_verify_driver_project(
                    "umdf",
                    driver_path,
                    "umdf-driver",
                    None,
                    Some(&build_args),
                    Some(env),
                    Some(target_arch),
                );
            });
        } else {
            clean_build_and_verify_driver_project(
                "umdf",
                driver_path,
                "umdf-driver",
                None,
                Some(&build_args),
                None,
                Some(target_arch),
            );
        }
    });
}

struct BuildArgs<'a> {
    target_arch: Option<&'a str>,
    profile: Option<&'a str>,
}

fn clean_build_and_verify_driver_project(
    driver_type: &str,
    driver_path: &str,
    driver_name: &str,
    driver_version: Option<&str>,
    build_args: Option<&BuildArgs>,
    env: Option<&[(&str, Option<&str>)]>,
    target_arch_for_verification: Option<&str>,
) {
    with_mutex(&format!("{driver_path}-{driver_name}"), || {
        let run = |env_overrides: Option<&[(&str, Option<&str>)]>| {
            run_cargo_clean(driver_path);
            let mut args = vec![];
            if let Some(target_arch) = build_args.and_then(|b| b.target_arch) {
                args.push("--target-arch");
                args.push(target_arch);
            }
            let profile = build_args
                .and_then(|b| b.profile)
                .map_or("debug", |profile| {
                    args.push("--profile");
                    args.push(profile);
                    profile
                });
            let stdout = run_build_cmd(
                driver_path,
                if args.is_empty() { None } else { Some(args) },
                env_overrides.map(|e| {
                    let mut map = HashMap::new();
                    for (key, value) in e {
                        if let Some(value) = value {
                            map.insert(*key, *value);
                        }
                    }
                    map
                }),
            );
            let target_triple = target_arch_for_verification.and_then(to_target_triple);
            let params = VerificationParams {
                path: driver_path,
                driver_type,
                driver_name,
                driver_version,
                target_triple,
                profile,
            };

            assert!(stdout.contains(&format!("Building package {driver_name}")));
            assert!(stdout.contains(&format!("Finished building {driver_name}")));
            verify_driver_package_files(&params);
        };

        if let Some(env_overrides) = env {
            with_env(env_overrides, || run(Some(env_overrides)));
        } else {
            run(None);
        }
    });
}

fn to_target_triple(target_arch: &str) -> Option<&'static str> {
    match target_arch.to_ascii_lowercase().as_str() {
        "x64" | "amd64" => Some(X86_64_TARGET_TRIPLE_NAME),
        "arm64" | "aarch64" => Some(AARCH64_TARGET_TRIPLE_NAME),
        _ => None,
    }
}

fn run_cargo_clean(driver_path: &str) {
    const MAX_RETRIES: u32 = 5;
    for attempt in 1..=MAX_RETRIES {
        let mut cmd = Command::new("cargo");
        cmd.args(["clean"]) // ensure the driver crate starts from a clean slate
            .current_dir(driver_path);

        let output = cmd.output().expect("failed to run cargo clean");
        if output.status.success() {
            return;
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        let locked = stderr.contains("os error 5") || stderr.contains("os error 32");
        assert!(
            locked && attempt < MAX_RETRIES,
            "cargo clean failed for {driver_path}: {stderr}"
        );

        let backoff = Duration::from_millis(250 * u64::from(attempt));
        eprintln!(
            "cargo clean hit a file-lock (attempt {attempt}/{MAX_RETRIES}) for {driver_path}; \
             retrying in {backoff:?}"
        );
        thread::sleep(backoff);
    }
}

fn run_build_cmd(
    driver_path: &str,
    additional_args: Option<Vec<&str>>,
    env_vars: Option<HashMap<&str, &str>>,
) -> String {
    set_crt_static_flag();
    let mut cmd = Command::cargo_bin("cargo-wdk").expect("unable to find cargo-wdk binary");
    cmd.current_dir(driver_path);
    let mut args = vec!["build"];
    if let Some(additional_args) = additional_args {
        args.extend(additional_args);
    }
    cmd.args(&args);
    if let Some(vars) = env_vars {
        cmd.envs(vars);
    }
    let cmd_assertion = cmd.assert().success();
    let output = cmd_assertion.get_output();
    String::from_utf8_lossy(&output.stdout).to_string()
}

struct VerificationParams<'a> {
    path: &'a str, // Standalone driver or workspace root path
    driver_type: &'a str,
    driver_name: &'a str,
    driver_version: Option<&'a str>,
    profile: &'a str,
    target_triple: Option<&'a str>,
}

fn verify_driver_package_files(params: &VerificationParams) {
    let driver_name = params.driver_name.replace('-', "_");
    let target_root = PathBuf::from(params.path).join("target");
    let target_folder_path = params.target_triple.map_or_else(
        || target_root.join(params.profile),
        |triple| target_root.join(triple).join(params.profile),
    );
    let target_folder_path_str = target_folder_path.to_string_lossy().to_string();
    let package_path = target_folder_path
        .join(format!("{driver_name}_package"))
        .to_string_lossy()
        .to_string();

    // Verify files exist in package folder
    assert_dir_exists(&package_path);
    let driver_binary_extension = match params.driver_type {
        "kmdf" | "wdm" => "sys",
        "umdf" => "dll",
        _ => panic!("Unsupported driver type: {}", params.driver_type),
    };

    for ext in ["cat", "inf", "map", "pdb", driver_binary_extension] {
        assert_file_exists(&format!("{package_path}/{driver_name}.{ext}"));
    }

    assert_file_exists(&format!("{package_path}/WDRLocalTestCert.cer"));

    // Verify hashes of files copied from debug to package folder
    assert_file_hash(
        &format!("{package_path}/{driver_name}.map"),
        &format!("{target_folder_path_str}/deps/{driver_name}.map"),
    );

    assert_file_hash(
        &format!("{package_path}/{driver_name}.pdb"),
        &format!("{target_folder_path_str}/{driver_name}.pdb"),
    );

    assert_file_hash(
        &format!("{package_path}/WDRLocalTestCert.cer"),
        &format!("{target_folder_path_str}/WDRLocalTestCert.cer"),
    );

    assert_driver_ver(&package_path, &driver_name, params.driver_version);
}

fn assert_dir_exists(path: &str) {
    let path = PathBuf::from(path);
    assert!(path.exists(), "Expected {} to exist", path.display());
    assert!(
        path.is_dir(),
        "Expected {} to be a directory",
        path.display()
    );
}

fn assert_file_exists(path: &str) {
    let path = PathBuf::from(path);
    assert!(path.exists(), "Expected {} to exist", path.display());
    assert!(path.is_file(), "Expected {} to be a file", path.display());
}

fn assert_file_hash(path1: &str, path2: &str) {
    assert_file_exists(path1);
    assert_file_exists(path2);

    assert_eq!(
        digest_file(PathBuf::from(path1)),
        digest_file(PathBuf::from(path2)),
        "Hash mismatch between {path1} and {path2}"
    );
}

fn assert_driver_ver(package_path: &str, driver_name: &str, driver_version: Option<&str>) {
    // Read the INF file as raw bytes and produce a best-effort UTF-8 string.
    let file_content =
        fs::read(format!("{package_path}/{driver_name}.inf")).expect("Unable to read inf file");
    let file_content = if file_content.starts_with(&[0xFF, 0xFE]) {
        // Handle UTF-16 LE (BOM 0xFF 0xFE).
        let file_content = file_content
            .chunks(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<u16>>();
        String::from_utf16_lossy(&file_content)
    } else {
        // Otherwise, treat the content as UTF-8; our test setups do not include
        // UTF16-BE encoded .inx files.
        String::from_utf8_lossy(&file_content).to_string()
    };

    // Example: DriverVer = 09/13/2023,1.0.0.0
    let driver_version_regex =
        driver_version.map_or_else(|| r"\d+\.\d+\.\d+\.\d+".to_string(), regex::escape);
    let re = regex::Regex::new(&format!(
        r"^DriverVer\s+=\s+\d+/\d+/\d+,{driver_version_regex}$"
    ))
    .unwrap();

    let line = file_content
        .lines()
        .find(|line| line.starts_with("DriverVer"))
        .expect("DriverVer line not found in inf file");
    assert!(re.captures(line).is_some());
}

// Helper to hash a file
fn digest_file<P: AsRef<Path>>(path: P) -> String {
    let file_contents = fs::read(&path)
        .unwrap_or_else(|_| panic!("Failed to read file. Path: {}", path.as_ref().display()));
    let result = Sha256::digest(&file_contents);
    format!("{result:x}")
}

// Constructs the path to the WDK content root for a given architecture from
// NuGet packages. Use this function when cross-compiling with a NuGet-based
// WDK, to locate the correct content root for the specified architecture and
// WDK version.
fn get_nuget_wdk_content_root(arch: &str, nuget_packages_root: &str) -> String {
    let full_version_number = std::env::var("FullVersionNumber")
        .expect("FullVersionNumber must be set when using NuGet source");
    let target_wdk_package = format!("Microsoft.Windows.WDK.{arch}.{full_version_number}");

    let wdk_package_dir = fs::read_dir(Path::new(nuget_packages_root))
        .unwrap_or_else(|err| {
            panic!("Failed to read NuGet package root '{nuget_packages_root}': {err}")
        })
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().to_string().eq(&target_wdk_package))
        })
        .unwrap_or_else(|| {
            panic!(
                "Unable to locate WDK package for target architecture {arch} under \
                 '{nuget_packages_root}'"
            )
        });

    let wdk_content_root_path = wdk_package_dir.join("c");
    assert!(
        wdk_content_root_path.is_dir(),
        "Expected WDK content root '{}' to exist",
        wdk_content_root_path.display()
    );
    wdk_content_root_path.to_string_lossy().into_owned()
}
