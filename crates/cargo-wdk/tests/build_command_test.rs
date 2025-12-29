//! System level tests for cargo wdk build flow
mod test_utils;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use assert_cmd::prelude::*;
use sha2::{Digest, Sha256};
use test_utils::{create_cargo_wdk_cmd, with_mutex};

const STAMPINF_VERSION_ENV_VAR: &str = "STAMPINF_VERSION";
const X86_64_TARGET_TRIPLE_NAME: &str = "x86_64-pc-windows-msvc";
const AARCH64_TARGET_TRIPLE_NAME: &str = "aarch64-pc-windows-msvc";

#[test]
fn mixed_package_kmdf_workspace_builds_successfully() {
    clean_build_and_verify_project(
        "tests/mixed-package-kmdf-workspace",
        "kmdf",
        "driver",
        None,
        None,
        None,
        None,
        None,
    );
}

#[test]
fn kmdf_driver_builds_successfully() {
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

    clean_build_and_verify_project(&driver_path, "kmdf", driver, None, None, None, None, None);
}

#[test]
fn umdf_driver_builds_successfully() {
    let driver = "umdf-driver";
    let driver_path = format!("tests/{driver}");
    clean_build_and_verify_project(&driver_path, "umdf", driver, None, None, None, None, None);
}

#[test]
fn wdm_driver_builds_successfully() {
    let driver = "wdm-driver";
    let driver_path = format!("tests/{driver}");
    clean_build_and_verify_project(&driver_path, "wdm", driver, None, None, None, None, None);
}

#[test]
fn wdm_driver_builds_successfully_with_given_version() {
    let driver = "wdm-driver";
    let driver_path = format!("tests/{driver}");
    let env = [(STAMPINF_VERSION_ENV_VAR, Some("5.1.0"))];
    clean_build_and_verify_project(
        &driver_path,
        "wdm",
        driver,
        Some("5.1.0.0"),
        None,
        None,
        Some(&env),
        None,
    );
}

#[test]
fn emulated_workspace_builds_successfully() {
    let emulated_workspace_path = "tests/emulated-workspace";
    let umdf_driver_workspace_path = format!("{emulated_workspace_path}/umdf-driver-workspace");
    with_mutex(emulated_workspace_path, || {
        run_cargo_clean(&umdf_driver_workspace_path);
        run_cargo_clean(&format!("{emulated_workspace_path}/rust-project"));
        let stdout = run_build_cmd(emulated_workspace_path, None, None);
        assert!(stdout.contains("Building package driver_1"));
        assert!(stdout.contains("Building package driver_2"));
        assert!(stdout.contains("Build completed successfully"));
        verify_driver_package_files(
            &umdf_driver_workspace_path,
            "driver_1",
            "dll",
            None,
            None,
            None,
        );
        verify_driver_package_files(
            &umdf_driver_workspace_path,
            "driver_2",
            "dll",
            None,
            None,
            None,
        );
    });
}

#[test]
fn kmdf_driver_with_target_arch_cli_option_builds_successfully() {
    let driver = "kmdf-driver";
    let driver_path = format!("tests/{driver}");
    let target_arch = "ARM64";
    let wdk_content_root = get_wdk_content_root_from_nuget_packages_root(target_arch);
    let env = wdk_content_root
        .as_deref()
        .map(|wdk_content_root| [("WDKContentRoot", Some(wdk_content_root))]);
    clean_build_and_verify_project(
        &driver_path,
        "kmdf",
        driver,
        None,
        Some(target_arch),
        None,
        env.as_ref().map(|env| &env[..]),
        Some(target_arch),
    );
}

// `config.toml` with `build.target` = "x86_64-pc-windows-msvc"
#[test]
fn kmdf_driver_with_target_override_via_config_toml() {
    let driver = "kmdf-driver-with-target-override";
    let driver_path = format!("tests/{driver}");
    let target_arch = "x64";
    let wdk_content_root = get_wdk_content_root_from_nuget_packages_root(target_arch);
    let env = wdk_content_root
        .as_deref()
        .map(|wdk_content_root| [("WDKContentRoot", Some(wdk_content_root))]);
    clean_build_and_verify_project(
        &driver_path,
        "kmdf",
        driver,
        None,
        None,
        None,
        env.as_ref().map(|env| &env[..]),
        Some(target_arch),
    );
}

#[test]
fn kmdf_driver_with_target_override_env_wins() {
    let driver = "kmdf-driver-with-target-override";
    let driver_path = format!("tests/{driver}");
    let target_arch = "ARM64";
    let mut env: Vec<(&str, Option<&str>)> =
        vec![("CARGO_BUILD_TARGET", Some(AARCH64_TARGET_TRIPLE_NAME))];
    let wdk_content_root = get_wdk_content_root_from_nuget_packages_root(target_arch);
    if let Some(wdk_content_root) = wdk_content_root.as_deref() {
        env.push(("WDKContentRoot", Some(wdk_content_root)));
    }
    clean_build_and_verify_project(
        &driver_path,
        "kmdf",
        driver,
        None,
        None,
        None,
        Some(env.as_slice()),
        Some(target_arch),
    );
}

/// Verifies that the CLI `--target-arch` argument takes precedence over:
/// - `.cargo/config.toml` `build.target` setting
/// - `CARGO_BUILD_TARGET` environment variable
#[test]
fn kmdf_driver_with_target_override_cli_wins_over_env_and_config() {
    let driver = "kmdf-driver-with-target-override";
    let driver_path = format!("tests/{driver}");
    let target_arch = "ARM64";
    let mut env: Vec<(&str, Option<&str>)> =
        vec![("CARGO_BUILD_TARGET", Some(X86_64_TARGET_TRIPLE_NAME))];
    let wdk_content_root = get_wdk_content_root_from_nuget_packages_root(target_arch);
    if let Some(wdk_content_root) = wdk_content_root.as_deref() {
        env.push(("WDKContentRoot", Some(wdk_content_root)));
    }
    clean_build_and_verify_project(
        &driver_path,
        "kmdf",
        driver,
        None,
        Some(target_arch),
        None,
        Some(env.as_slice()),
        Some(target_arch),
    );
}

#[test]
fn umdf_driver_with_target_arch_and_release_profile() {
    let driver_path = "tests/umdf-driver";
    let target_arch = "ARM64";
    let profile = "release";
    let wdk_content_root = get_wdk_content_root_from_nuget_packages_root(target_arch);
    let env = wdk_content_root
        .as_deref()
        .map(|wdk_content_root| [("WDKContentRoot", Some(wdk_content_root))]);
    clean_build_and_verify_project(
        driver_path,
        "umdf",
        "umdf-driver",
        None,
        Some(target_arch),
        Some(profile),
        env.as_ref().map(|env| &env[..]),
        Some(target_arch),
    );
}

#[allow(clippy::too_many_arguments)]
fn clean_build_and_verify_project(
    project_path: &str,
    driver_type: &str,
    driver_name: &str,
    driver_version: Option<&str>,
    input_target_arch: Option<&str>,
    profile: Option<&str>,
    env_overrides: Option<&[(&str, Option<&str>)]>,
    target_arch_for_verification: Option<&str>,
) {
    let mutex_name = format!("{project_path}-{driver_name}");
    with_mutex(&mutex_name, || {
        run_cargo_clean(project_path);

        let mut args: Vec<&str> = Vec::new();
        if let Some(target_arch) = input_target_arch {
            args.push("--target-arch");
            args.push(target_arch);
        }
        if let Some(profile) = profile {
            args.push("--profile");
            args.push(profile);
        }
        let cmd_args = if args.is_empty() {
            None
        } else {
            Some(args.as_slice())
        };
        let stdout = run_build_cmd(project_path, cmd_args, env_overrides);

        assert!(stdout.contains(&format!("Building package {driver_name}")));
        assert!(stdout.contains(&format!("Finished building {driver_name}")));

        let driver_binary_extension = match driver_type {
            "kmdf" | "wdm" => "sys",
            "umdf" => "dll",
            _ => panic!("Unsupported driver type: {driver_type}"),
        };

        let target_triple = target_arch_for_verification.and_then(to_target_triple);

        verify_driver_package_files(
            project_path,
            driver_name,
            driver_binary_extension,
            driver_version,
            target_triple,
            profile,
        );
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
    let mut cmd = Command::new("cargo");
    cmd.args(["clean"]).current_dir(driver_path);
    cmd.assert().success();
}

fn run_build_cmd(
    path: &str,
    args: Option<&[&str]>,
    env_vars: Option<&[(&str, Option<&str>)]>,
) -> String {
    // assert command output
    let mut cmd = create_cargo_wdk_cmd("build", args, env_vars, Some(path));
    let cmd_assertion = cmd.assert().success();
    let output = cmd_assertion.get_output();
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn verify_driver_package_files(
    driver_or_workspace_path: &str,
    driver_name: &str,
    driver_binary_extension: &str,
    driver_version: Option<&str>,
    target_triple: Option<&str>,
    profile: Option<&str>,
) {
    let driver_name = driver_name.replace('-', "_");
    let profile = profile.unwrap_or("debug");
    let target_folder_path = target_triple.map_or_else(
        || format!("{driver_or_workspace_path}/target/{profile}"),
        |target_triple| format!("{driver_or_workspace_path}/target/{target_triple}/{profile}"),
    );
    let package_path = PathBuf::from(&target_folder_path)
        .join(format!("{driver_name}_package"))
        .to_string_lossy()
        .to_string();

    // Verify files exist in package folder
    assert_dir_exists(&package_path);

    for ext in ["cat", "inf", "map", "pdb", driver_binary_extension] {
        assert_file_exists(&format!("{package_path}/{driver_name}.{ext}"));
    }

    assert_file_exists(&format!("{package_path}/WDRLocalTestCert.cer"));

    // Verify hashes of files copied from debug to package folder
    assert_file_hash(
        &format!("{package_path}/{driver_name}.map"),
        &format!("{target_folder_path}/deps/{driver_name}.map"),
    );

    assert_file_hash(
        &format!("{package_path}/{driver_name}.pdb"),
        &format!("{target_folder_path}/{driver_name}.pdb"),
    );

    assert_file_hash(
        &format!("{package_path}/WDRLocalTestCert.cer"),
        &format!("{target_folder_path}/WDRLocalTestCert.cer"),
    );

    assert_driver_ver(&package_path, &driver_name, driver_version);
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
        .unwrap_or_else(|e| panic!("Failed to read file at {}: {}", path.as_ref().display(), e));
    let result = Sha256::digest(&file_contents);
    format!("{result:x}")
}

/// This helper function tries to get the WDK content root path from Nuget
/// packages root.
///
/// # Returns:
/// - WDK content's (for the given architecture) root path as a string if
///   `NugetPackagesRoot` and `FullVersionNumber` environment variables are set
/// - `None` if these environment variables are not set.
///
/// # Panics:
/// - If the expected WDK package folder cannot be found.
fn get_wdk_content_root_from_nuget_packages_root(target_arch: &str) -> Option<String> {
    let nuget_packages_root = std::env::var("NugetPackagesRoot").ok()?;
    let full_version_number = std::env::var("FullVersionNumber").ok()?;

    let expected_wdk_package_dir_name =
        format!("Microsoft.Windows.WDK.{target_arch}.{full_version_number}");

    let wdk_package_dir = fs::read_dir(Path::new(&nuget_packages_root))
        .unwrap_or_else(|err| {
            panic!("Failed to read NuGet package root '{nuget_packages_root}': {err}")
        })
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name == expected_wdk_package_dir_name)
        })
        .unwrap_or_else(|| {
            panic!(
                "Unable to locate WDK package for target architecture {target_arch} under \
                 '{nuget_packages_root}'"
            )
        });

    let wdk_content_root_path = wdk_package_dir.join("c");
    assert!(
        wdk_content_root_path.is_dir(),
        "Expected WDK content root '{}' to exist",
        wdk_content_root_path.display()
    );
    Some(wdk_content_root_path.to_string_lossy().into_owned())
}
