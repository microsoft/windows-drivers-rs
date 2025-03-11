//! System level tests for cargo wdk build flow
mod common;
use std::{path::PathBuf, process::Command};

use assert_cmd::prelude::*;
use common::set_crt_static_flag;
use serial_test::serial;

#[test]
#[serial]
fn given_a_mixed_package_kmdf_workspace_when_cargo_wdk_is_executed_then_driver_package_folder_is_created_with_expected_files(
) {
    set_crt_static_flag();

    let mut cmd = Command::cargo_bin("cargo-wdk").expect("unable to find cargo-wdk binary");
    cmd.args([
        "build",
        "--cwd",
        "tests/mixed-package-kmdf-workspace", // Root dir for tests is cargo-wdk
    ]);

    // assert command output
    let cmd_assertion = cmd.assert().success();
    let output = cmd_assertion.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Processing completed for package: driver"));
    assert!(stdout.contains(
        "No package.metadata.wdk section found. Skipping driver package workflow for package: \
         non_driver_crate"
    ));

    // assert driver package
    assert!(
        PathBuf::from("tests/mixed-package-kmdf-workspace/target/debug/driver_package").exists()
    );
    assert!(PathBuf::from(
        "tests/mixed-package-kmdf-workspace/target/debug/driver_package/driver.cat"
    )
    .exists());
    assert!(PathBuf::from(
        "tests/mixed-package-kmdf-workspace/target/debug/driver_package/driver.inf"
    )
    .exists());
    assert!(PathBuf::from(
        "tests/mixed-package-kmdf-workspace/target/debug/driver_package/driver.map"
    )
    .exists());
    assert!(PathBuf::from(
        "tests/mixed-package-kmdf-workspace/target/debug/driver_package/driver.pdb"
    )
    .exists());
    assert!(PathBuf::from(
        "tests/mixed-package-kmdf-workspace/target/debug/driver_package/driver.sys"
    )
    .exists());
    assert!(PathBuf::from(
        "tests/mixed-package-kmdf-workspace/target/debug/driver_package/WDRLocalTestCert.cer"
    )
    .exists());
}

#[test]
#[serial]
fn given_a_umdf_driver_when_cargo_wdk_is_executed_then_driver_package_folder_is_created_with_expected_files(
) {
    set_crt_static_flag();

    let mut cmd = Command::cargo_bin("cargo-wdk").expect("unable to find cargo-wdk binary");
    cmd.args([
        "build",
        "--cwd",
        "tests/umdf-driver", // Root dir for tests is cargo-wdk
    ]);

    // assert command output
    let cmd_assertion = cmd.assert().success();
    let output = cmd_assertion.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Processing completed for package: umdf-driver"));

    // assert driver package
    assert!(PathBuf::from("tests/umdf-driver/target/debug/umdf_driver_package").exists());
    assert!(
        PathBuf::from("tests/umdf-driver/target/debug/umdf_driver_package/umdf_driver.cat")
            .exists()
    );
    assert!(
        PathBuf::from("tests/umdf-driver/target/debug/umdf_driver_package/umdf_driver.inf")
            .exists()
    );
    assert!(
        PathBuf::from("tests/umdf-driver/target/debug/umdf_driver_package/umdf_driver.map")
            .exists()
    );
    assert!(
        PathBuf::from("tests/umdf-driver/target/debug/umdf_driver_package/umdf_driver.pdb")
            .exists()
    );
    assert!(
        PathBuf::from("tests/umdf-driver/target/debug/umdf_driver_package/umdf_driver.dll")
            .exists()
    );
    assert!(PathBuf::from(
        "tests/umdf-driver/target/debug/umdf_driver_package/WDRLocalTestCert.cer"
    )
    .exists());
}

#[test]
#[serial]
fn given_an_emulated_workspace_when_cargo_wdk_is_executed_then_all_driver_projects_are_built_and_packaged_and_non_driver_rust_projects_failed_and_rest_ignored(
) {
    set_crt_static_flag();

    let mut cmd = Command::cargo_bin("cargo-wdk").expect("unable to find cargo-wdk binary");
    cmd.args([
        "build",
        "--cwd",
        "tests/emulated-workspace", // Root dir for tests is cargo-wdk
    ]);

    // assert command output
    let cmd_assertion = cmd.assert().failure(); // Since setup includes non driver rust project
    let output = cmd_assertion.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains(
        "Error packaging the child project: rust-project, error: Error Parsing WDK metadata from \
         Cargo.toml"
    )); // rust-project is not a driver and it is expected to fail
    assert!(stdout.contains("Processing completed for package: driver_1"));
    assert!(stdout.contains("Processing completed for package: driver_2"));
    assert!(stdout.contains(
        r"One or more rust (possibly driver) projects failed to package in the working directory: "
    ));

    // assert umdf-driver-workspace driver package
    assert!(PathBuf::from(
        "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_1_package"
    )
    .exists());
    assert!(PathBuf::from(
        "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_1_package/driver_1.cat"
    )
    .exists());
    assert!(PathBuf::from(
        "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_1_package/driver_1.inf"
    )
    .exists());
    assert!(PathBuf::from(
        "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_1_package/driver_1.map"
    )
    .exists());
    assert!(PathBuf::from(
        "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_1_package/driver_1.pdb"
    )
    .exists());
    assert!(PathBuf::from(
        "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_1_package/driver_1.dll"
    )
    .exists());
    assert!(PathBuf::from(
        "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_1_package/\
         WDRLocalTestCert.cer"
    )
    .exists());

    assert!(PathBuf::from(
        "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_2_package"
    )
    .exists());
    assert!(PathBuf::from(
        "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_2_package/driver_2.cat"
    )
    .exists());
    assert!(PathBuf::from(
        "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_2_package/driver_2.inf"
    )
    .exists());
    assert!(PathBuf::from(
        "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_2_package/driver_2.map"
    )
    .exists());
    assert!(PathBuf::from(
        "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_2_package/driver_2.pdb"
    )
    .exists());
    assert!(PathBuf::from(
        "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_2_package/driver_2.dll"
    )
    .exists());
    assert!(PathBuf::from(
        "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_2_package/\
         WDRLocalTestCert.cer"
    )
    .exists());
}
