use std::{path::PathBuf, process::Command};

use assert_cmd::prelude::*;

#[test]
fn given_a_mixed_package_kmdf_workspace_when_cargo_wdk_is_executed_then_driver_package_folder_is_created_with_expected_files(
) {
    let mut cmd = Command::cargo_bin("cargo-wdk").unwrap();
    cmd.args(&[
        "build",
        "--cwd",
        "tests/mixed-package-kmdf-workspace", // Root dir for tests is cargo-wdk
    ]);

    // assert command output
    let cmd_assertion = cmd.assert().success();
    let output = cmd_assertion.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("INFO: Processing completed for package: driver"));
    assert!(stdout.contains(
        "WARN: No package.metadata.wdk section found. Skipping driver package workflow for \
         package: non_driver_crate"
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
