//! System level tests for cargo wdk build flow
#![allow(clippy::literal_string_with_formatting_args)]
mod common;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use assert_cmd::prelude::*;
use common::{set_crt_static_flag, with_file_lock};
use sha2::{Digest, Sha256};

const STAMPINF_VERSION_ENV_VAR: &str = "STAMPINF_VERSION";

#[test]
fn mixed_package_kmdf_workspace_builds_successfully() {
    with_file_lock::<&str, &str, _>(&[], || {
        run_cargo_clean("tests/mixed-package-kmdf-workspace");
        let stdout = run_build_cmd("tests/mixed-package-kmdf-workspace");

        assert!(stdout.contains("Building package driver"));
        assert!(stdout.contains("Building package non_driver_crate"));
        verify_driver_package_files("tests/mixed-package-kmdf-workspace", "driver", "sys", None);
    });
}

#[test]
fn kmdf_driver_builds_successfully() {
    // Setup for executables
    wdk_build::cargo_make::setup_path().expect("failed to set up paths for executables");

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

    with_file_lock::<&str, &str, _>(&[], || clean_and_build_driver_project("kmdf", None));
}

#[test]
fn umdf_driver_builds_successfully() {
    with_file_lock::<&str, &str, _>(&[], || clean_and_build_driver_project("umdf", None));
}

#[test]
fn wdm_driver_builds_successfully() {
    with_file_lock::<&str, &str, _>(&[], || clean_and_build_driver_project("wdm", None));
}

#[test]
fn wdm_driver_builds_successfully_with_given_version() {
    with_file_lock::<&str, &str, _>(&[(STAMPINF_VERSION_ENV_VAR, "5.1.0")], || {
        clean_and_build_driver_project("wdm", Some("5.1.0.0"));
    });
}

#[test]
fn emulated_workspace_builds_successfully() {
    with_file_lock::<&str, &str, _>(&[], || {
        let emulated_workspace_path = "tests/emulated-workspace";
        let umdf_driver_workspace_path = format!("{emulated_workspace_path}/umdf-driver-workspace");

        run_cargo_clean(&umdf_driver_workspace_path);
        let stdout = run_build_cmd(emulated_workspace_path);

        assert!(stdout.contains("Building package driver_1"));
        assert!(stdout.contains("Building package driver_2"));
        assert!(stdout.contains("Build completed successfully"));

        verify_driver_package_files(&umdf_driver_workspace_path, "driver_1", "dll", None);
        verify_driver_package_files(&umdf_driver_workspace_path, "driver_2", "dll", None);
    });
}

fn clean_and_build_driver_project(driver_type: &str, driver_version: Option<&str>) {
    let driver_name = format!("{driver_type}-driver");
    let driver_path = format!("tests/{driver_name}");

    run_cargo_clean(&driver_path);
    let stdout = run_build_cmd(&driver_path);

    assert!(stdout.contains(&format!("Building package {driver_name}")));

    let driver_binary_extension = match driver_type {
        "kmdf" | "wdm" => "sys",
        "umdf" => "dll",
        _ => panic!("Unsupported driver type: {driver_type}"),
    };

    verify_driver_package_files(
        &driver_path,
        &driver_name,
        driver_binary_extension,
        driver_version,
    );
}

fn run_cargo_clean(driver_path: &str) {
    let mut cmd = Command::new("cargo");
    cmd.args(["clean"]).current_dir(driver_path);
    cmd.assert().success();
}

fn run_build_cmd(driver_path: &str) -> String {
    set_crt_static_flag();

    let mut cmd = Command::cargo_bin("cargo-wdk").expect("unable to find cargo-wdk binary");
    cmd.args(["build"]).current_dir(driver_path);

    // assert command output
    let cmd_assertion = cmd.assert().success();
    let output = cmd_assertion.get_output();

    String::from_utf8_lossy(&output.stdout).to_string()
}

fn verify_driver_package_files(
    driver_or_workspace_path: &str,
    driver_name: &str,
    driver_binary_extension: &str,
    driver_version: Option<&str>,
) {
    let driver_name = driver_name.replace('-', "_");
    let debug_folder_path = format!("{driver_or_workspace_path}/target/debug");
    let package_path = format!("{debug_folder_path}/{driver_name}_package");

    // Verify files exist in package folder
    assert_dir_exists(&package_path);

    for ext in ["cat", "inf", "map", "pdb", driver_binary_extension] {
        assert_file_exists(&format!("{package_path}/{driver_name}.{ext}"));
    }

    assert_file_exists(&format!("{package_path}/WDRLocalTestCert.cer"));

    // Verify hashes of files copied from debug to package folder
    assert_file_hash(
        &format!("{package_path}/{driver_name}.map"),
        &format!("{debug_folder_path}/deps/{driver_name}.map"),
    );

    assert_file_hash(
        &format!("{package_path}/{driver_name}.pdb"),
        &format!("{debug_folder_path}/{driver_name}.pdb"),
    );

    assert_file_hash(
        &format!("{package_path}/WDRLocalTestCert.cer"),
        &format!("{debug_folder_path}/WDRLocalTestCert.cer"),
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
    let file_content =
        fs::read(format!("{package_path}/{driver_name}.inf")).expect("Unable to read inf file");
    let file_content = String::from_utf8_lossy(&file_content);
    file_content.lines().for_each(|line| {
        // Example: DriverVer=09/13/2023,1.0.0.0
        let driver_version_regex = driver_version.unwrap_or(r"\d+\.\d+\.\d+\.\d+");
        let re = regex::Regex::new(&format!(
            r"^DriverVer\s+=\s+\d+/\d+/\d+,{driver_version_regex}$"
        ))
        .unwrap();
        if line.starts_with("DriverVer") {
            assert!(re.captures(line).is_some());
        }
    });
}

// Helper to hash a file
fn digest_file<P: AsRef<Path>>(path: P) -> String {
    let file_contents = fs::read(path).expect("Failed to read file");
    let result = Sha256::digest(&file_contents);
    format!("{result:x}")
}
