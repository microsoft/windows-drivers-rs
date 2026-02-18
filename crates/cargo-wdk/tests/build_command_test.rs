//! System level tests for cargo wdk build flow
mod test_utils;
use std::{
    env,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use assert_cmd::prelude::*;
use sha2::{Digest, Sha256};
use test_utils::{create_cargo_wdk_cmd, with_mutex};

const STAMPINF_VERSION_ENV_VAR: &str = "STAMPINF_VERSION";
const WDK_CONTENT_ROOT_ENV_VAR: &str = "WDKContentRoot";
const NUGET_PACKAGES_ROOT_ENV_VAR: &str = "NugetPackagesRoot";
const FULL_VERSION_NUMBER_ENV_VAR: &str = "FullVersionNumber";
const X86_64_TARGET_TRIPLE_NAME: &str = "x86_64-pc-windows-msvc";
const AARCH64_TARGET_TRIPLE_NAME: &str = "aarch64-pc-windows-msvc";

#[test]
fn mixed_package_kmdf_workspace_builds_successfully() {
    clean_build_and_verify_project(
        "kmdf",
        "driver",
        Some("tests/mixed-package-kmdf-workspace"),
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

    clean_build_and_verify_project("kmdf", driver, None, None, None, None, None, None);
}

#[test]
fn kmdf_driver_cross_compiles_with_cli_option_successfully() {
    let driver = "kmdf-driver";
    let target_arch = cross_compile_target_arch();
    let env_overrides = nuget_wdk_content_root_path(target_arch)
        .map(|path| vec![(WDK_CONTENT_ROOT_ENV_VAR, Some(path))]);
    clean_build_and_verify_project(
        "kmdf",
        driver,
        None,
        None,
        Some(target_arch),
        None,
        env_overrides.as_deref(),
        Some(target_arch),
    );
}

#[test]
fn umdf_driver_builds_successfully() {
    let driver = "umdf-driver";
    clean_build_and_verify_project("umdf", driver, None, None, None, None, None, None);
}

#[test]
fn umdf_driver_cross_compiles_with_cli_option_successfully() {
    let driver = "umdf-driver";
    let target_arch = cross_compile_target_arch();
    let env_overrides = nuget_wdk_content_root_path(target_arch)
        .map(|path| vec![(WDK_CONTENT_ROOT_ENV_VAR, Some(path))]);
    clean_build_and_verify_project(
        "umdf",
        driver,
        None,
        None,
        Some(target_arch),
        None,
        env_overrides.as_deref(),
        Some(target_arch),
    );
}

#[test]
fn umdf_driver_with_target_arch_cli_option_and_release_profile_builds_successfully() {
    let driver = "umdf-driver";
    let target_arch = "ARM64";
    let profile = "release";
    let env_overrides = nuget_wdk_content_root_path(target_arch)
        .map(|path| vec![(WDK_CONTENT_ROOT_ENV_VAR, Some(path))]);
    clean_build_and_verify_project(
        "umdf",
        driver,
        None,
        None,
        Some(target_arch),
        Some(profile),
        env_overrides.as_deref(),
        Some(target_arch),
    );
}

#[test]
fn wdm_driver_builds_successfully() {
    let driver = "wdm-driver";
    clean_build_and_verify_project("wdm", driver, None, None, None, None, None, None);
}

#[test]
fn wdm_driver_builds_successfully_with_given_version() {
    let driver = "wdm-driver";
    let env = [(STAMPINF_VERSION_ENV_VAR, Some("5.1.0".to_string()))];
    clean_build_and_verify_project(
        "wdm",
        driver,
        None,
        Some("5.1.0.0"),
        None,
        None,
        Some(&env),
        None,
    );
}

#[test]
fn wdm_driver_cross_compiles_with_cli_option_successfully() {
    let driver = "wdm-driver";
    let target_arch = cross_compile_target_arch();
    let env_overrides = nuget_wdk_content_root_path(target_arch)
        .map(|path| vec![(WDK_CONTENT_ROOT_ENV_VAR, Some(path))]);
    clean_build_and_verify_project(
        "wdm",
        driver,
        None,
        None,
        Some(target_arch),
        None,
        env_overrides.as_deref(),
        Some(target_arch),
    );
}

#[test]
fn emulated_workspace_builds_successfully() {
    let emulated_workspace_path = "tests/emulated-workspace";
    let umdf_driver_workspace_path = format!("{emulated_workspace_path}/umdf-driver-workspace");
    with_mutex(emulated_workspace_path, || {
        run_cargo_clean(&umdf_driver_workspace_path);
        run_cargo_clean(&format!("{emulated_workspace_path}/rust-project"));
        let stderr = run_build_cmd(emulated_workspace_path, None, None);
        assert!(stderr.contains("Building package driver_1"));
        assert!(stderr.contains("Building package driver_2"));
        assert!(stderr.contains("Build completed successfully"));
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

mod kmdf_driver_with_target_override {
    use super::*;

    const DRIVER_PROJECT: &str = "kmdf-driver-with-target-override";
    const CARGO_BUILD_TARGET_ENV_VAR: &str = "CARGO_BUILD_TARGET";

    fn configure_env(
        wdk_target_arch: &str,
        cargo_build_target: Option<&str>,
    ) -> Vec<(&'static str, Option<String>)> {
        let mut env: Vec<(&'static str, Option<String>)> = Vec::new();

        if let Some(target_triple) = cargo_build_target {
            env.push((CARGO_BUILD_TARGET_ENV_VAR, Some(target_triple.to_string())));
        }

        if let Some(path) = nuget_wdk_content_root_path(wdk_target_arch) {
            env.push((WDK_CONTENT_ROOT_ENV_VAR, Some(path)));
        }
        env
    }

    // `config.toml` with `build.target` = "x86_64-pc-windows-msvc"
    #[test]
    fn via_config_toml_builds_successfully() {
        let target_arch = "x64";
        let env = configure_env(target_arch, None);
        clean_build_and_verify_project(
            "kmdf",
            DRIVER_PROJECT,
            None,
            None,
            None,
            None,
            Some(env.as_slice()),
            Some(target_arch),
        );
    }

    #[test]
    fn via_env_wins_over_config_toml() {
        let target_arch = "ARM64";
        let env = configure_env(target_arch, Some(AARCH64_TARGET_TRIPLE_NAME));
        clean_build_and_verify_project(
            "kmdf",
            DRIVER_PROJECT,
            None,
            None,
            None,
            None,
            Some(env.as_slice()),
            Some(target_arch),
        );
    }

    #[test]
    fn via_cli_wins_over_env() {
        let target_arch = "x64";
        let cli_target_arch = "amd64";
        let env = configure_env(target_arch, Some(AARCH64_TARGET_TRIPLE_NAME));
        clean_build_and_verify_project(
            "kmdf",
            DRIVER_PROJECT,
            None,
            None,
            Some(cli_target_arch),
            None,
            Some(env.as_slice()),
            Some(cli_target_arch),
        );
    }

    #[test]
    fn via_cli_wins_over_env_and_config() {
        let target_arch = "ARM64";
        let env = configure_env(target_arch, Some(X86_64_TARGET_TRIPLE_NAME));
        clean_build_and_verify_project(
            "kmdf",
            DRIVER_PROJECT,
            None,
            None,
            Some(target_arch),
            None,
            Some(env.as_slice()),
            Some(target_arch),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn clean_build_and_verify_project(
    driver_type: &str,
    driver_name: &str,
    project_path: Option<&str>,
    driver_version: Option<&str>,
    input_target_arch: Option<&str>,
    profile: Option<&str>,
    env_overrides: Option<&[(&str, Option<String>)]>,
    target_arch_for_verification: Option<&str>,
) {
    let project_path =
        project_path.map_or_else(|| format!("tests/{driver_name}"), ToString::to_string);
    let mutex_name = project_path.clone();
    with_mutex(&mutex_name, || {
        run_cargo_clean(&project_path);

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
        let stderr = run_build_cmd(&project_path, cmd_args, env_overrides);

        assert!(stderr.contains(&format!("Building package {driver_name}")));
        assert!(stderr.contains(&format!("Finished building {driver_name}")));

        let driver_binary_extension = match driver_type {
            "kmdf" | "wdm" => "sys",
            "umdf" => "dll",
            _ => panic!("Unsupported driver type: {driver_type}"),
        };

        let target_triple = target_arch_for_verification.and_then(to_target_triple);

        verify_driver_package_files(
            &project_path,
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
    cmd.stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .assert()
        .success();
}

fn run_build_cmd(
    path: &str,
    args: Option<&[&str]>,
    env_vars: Option<&[(&str, Option<String>)]>,
) -> String {
    // assert command output
    let mut cmd = create_cargo_wdk_cmd("build", args, env_vars, Some(path));
    let output = cmd
        .output()
        .expect("Failed to execute cargo wdk build command");

    assert!(
        output.status.success(),
        "Cargo wdk build command failed to execute successfully. \nSTDOUT: {}\nSTDERR: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8_lossy(&output.stderr).to_string()
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

/// Returns the `WDKContentRoot` path derived from the Nuget WDK package, if the
/// Nuget WDK and Full Version Number env vars are present.
///
/// Behavior:
/// - If `NugetPackagesRoot` and `FullVersionNumber` are set, returns
///   `Some(path)`.
/// - Otherwise, returns `None` so tests do not override (or remove)
///   `WDKContentRoot` in non-Nuget environments.
///
/// # Panics:
/// - If the Nuget env vars are set but the expected WDK package folder cannot
///   be found.
fn nuget_wdk_content_root_path(target_arch: &str) -> Option<String> {
    let Ok(nuget_packages_root) = std::env::var(NUGET_PACKAGES_ROOT_ENV_VAR) else {
        return None;
    };
    let Ok(full_version_number) = std::env::var(FULL_VERSION_NUMBER_ENV_VAR) else {
        return None;
    };

    // NuGet WDK package folder names use `x64` (lowercase and not `amd64`) and
    // `ARM64` (uppercase) whereas `cargo-wdk` CLI uses `amd64` / `arm64`
    // (case-insensitive), so we normalize here in order to locate the right package
    // folder during tests.
    let target_arch_lower = target_arch.to_ascii_lowercase();
    let nuget_arch = match target_arch_lower.as_str() {
        "amd64" | "x64" | "x86_64" => "x64",
        "arm64" | "aarch64" => "ARM64",
        other => other,
    };

    let expected_wdk_package_dir_name =
        format!("Microsoft.Windows.WDK.{nuget_arch}.{full_version_number}");

    let wdk_package_dir = fs::read_dir(Path::new(&nuget_packages_root))
        .unwrap_or_else(|err| {
            panic!("Failed to read Nuget package root '{nuget_packages_root}': {err}")
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
                "Unable to locate WDK package for target architecture {target_arch} (NuGet arch: \
                 {nuget_arch}) under '{nuget_packages_root}'"
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

/// Returns the cross-compilation target architecture for the current host.
///
/// - If host is `x86_64`, we cross-compile to `ARM64`.
/// - If host is `aarch64`, we cross-compile to `AMD64`.
fn cross_compile_target_arch() -> &'static str {
    match env::consts::ARCH {
        "x86_64" => "ARM64",
        "aarch64" => "AMD64",
        other => panic!(
            "Unsupported host architecture '{other}' for cross-compilation tests. Expected \
             'x86_64' or 'aarch64'."
        ),
    }
}
