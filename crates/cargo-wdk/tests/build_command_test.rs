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

// Helper to hash a file
fn digest_file<P: AsRef<Path>>(path: P) -> String {
    let file_contents = fs::read(path).expect("Failed to read file");
    let result = Sha256::digest(&file_contents);
    format!("{result:x}")
}

#[test]
fn given_a_mixed_package_kmdf_workspace_when_cargo_wdk_is_executed_then_driver_package_folder_is_created_with_expected_files(
) {
    with_file_lock(|| {
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
            "No package.metadata.wdk section found. Skipping driver build workflow for package: \
             non_driver_crate"
        ));

        // assert driver package
        assert!(
            PathBuf::from("tests/mixed-package-kmdf-workspace/target/debug/driver_package")
                .exists()
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

        // assert if the files are copied properly
        assert_eq!(
            digest_file(Path::new(
                "tests/mixed-package-kmdf-workspace/target/debug/driver_package/driver.map"
            )),
            digest_file(Path::new(
                "tests/mixed-package-kmdf-workspace/target/debug/deps/driver.map"
            ))
        );
        assert_eq!(
            digest_file(Path::new(
                "tests/mixed-package-kmdf-workspace/target/debug/driver_package/driver.pdb"
            )),
            digest_file(Path::new(
                "tests/mixed-package-kmdf-workspace/target/debug/driver.pdb"
            ))
        );
        assert_eq!(
            digest_file(Path::new(
                "tests/mixed-package-kmdf-workspace/target/debug/driver_package/WDRLocalTestCert.\
                 cer"
            )),
            digest_file(Path::new(
                "tests/mixed-package-kmdf-workspace/target/debug/WDRLocalTestCert.cer"
            ))
        );
    });
}

#[test]
fn given_a_kmdf_driver_with_cert_available_in_store_when_cargo_wdk_is_executed_then_driver_package_folder_is_created_with_expected_files(
) {
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

    with_file_lock(|| build_driver_project("kmdf"));
}

#[test]
fn given_a_umdf_driver_when_cargo_wdk_is_executed_then_driver_package_folder_is_created_with_expected_files(
) {
    with_file_lock(|| build_driver_project("umdf"));
}

#[test]
fn given_a_wdm_driver_when_cargo_wdk_is_executed_then_driver_package_folder_is_created_with_expected_files(
) {
    with_file_lock(|| build_driver_project("wdm"));
}

fn build_driver_project(driver_type: &str) {
    set_crt_static_flag();

    let driver_folder = format!("{driver_type}-driver");
    let driver_folder_underscored = format!("{driver_type}_driver");

    let mut cmd = Command::cargo_bin("cargo-wdk").expect("unable to find cargo-wdk binary");
    cmd.args([
        "build",
        "--cwd",
        &format!("tests/{driver_folder}"), // Root dir for tests is cargo-wdk
    ]);

    // assert command output
    let cmd_assertion = cmd.assert().success();
    let output = cmd_assertion.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&format!(
        "Processing completed for package: {driver_folder}"
    )));

    // assert driver package
    assert!(PathBuf::from(format!(
        "tests/{driver_folder}/target/debug/{driver_folder_underscored}_package"
    ))
    .exists());
    assert!(PathBuf::from(format!(
        "tests/{driver_folder}/target/debug/{driver_folder_underscored}_package/\
         {driver_folder_underscored}.cat"
    ))
    .exists());
    assert!(PathBuf::from(format!(
        "tests/{driver_folder}/target/debug/{driver_folder_underscored}_package/\
         {driver_folder_underscored}.inf"
    ))
    .exists());
    assert!(PathBuf::from(format!(
        "tests/{driver_folder}/target/debug/{driver_folder_underscored}_package/\
         {driver_folder_underscored}.map"
    ))
    .exists());
    assert!(PathBuf::from(format!(
        "tests/{driver_folder}/target/debug/{driver_folder_underscored}_package/\
         {driver_folder_underscored}.pdb"
    ))
    .exists());

    if matches!(driver_type, "kmdf" | "wdm") {
        assert!(PathBuf::from(format!(
            "tests/{driver_folder}/target/debug/{driver_folder_underscored}_package/\
             {driver_folder_underscored}.sys"
        ))
        .exists());
    } else {
        assert!(PathBuf::from(format!(
            "tests/{driver_folder}/target/debug/{driver_folder_underscored}_package/\
             {driver_folder_underscored}.dll"
        ))
        .exists());
    }

    assert!(PathBuf::from(format!(
        "tests/{driver_folder}/target/debug/{driver_folder_underscored}_package/WDRLocalTestCert.\
         cer"
    ))
    .exists());

    // assert if the files are copied properly
    assert_eq!(
        digest_file(PathBuf::from(format!(
            "tests/{driver_folder}/target/debug/{driver_folder_underscored}_package/\
             {driver_folder_underscored}.map"
        ))),
        digest_file(PathBuf::from(format!(
            "tests/{driver_folder}/target/debug/deps/{driver_folder_underscored}.map"
        ))),
    );

    assert_eq!(
        digest_file(PathBuf::from(format!(
            "tests/{driver_folder}/target/debug/{driver_folder_underscored}_package/\
             {driver_folder_underscored}.pdb"
        ))),
        digest_file(PathBuf::from(format!(
            "tests/{driver_folder}/target/debug/{driver_folder_underscored}.pdb"
        )))
    );

    assert_eq!(
        digest_file(PathBuf::from(format!(
            "tests/{driver_folder}/target/debug/{driver_folder_underscored}_package/\
             WDRLocalTestCert.cer"
        ))),
        digest_file(PathBuf::from(format!(
            "tests/{driver_folder}/target/debug/WDRLocalTestCert.cer"
        )))
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn given_an_emulated_workspace_when_cargo_wdk_is_executed_then_all_driver_and_non_driver_projects_are_built_and_packaged_successfully(
) {
    with_file_lock(|| {
        set_crt_static_flag();

        let mut cmd = Command::cargo_bin("cargo-wdk").expect("unable to find cargo-wdk binary");
        cmd.args([
            "build",
            "--cwd",
            "tests/emulated-workspace", // Root dir for tests is cargo-wdk
        ]);

        // assert command output
        let cmd_assertion = cmd.assert().success();
        let output = cmd_assertion.get_output();
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Matches warning about WDK metadata not being available for non driver project
        // but a valid rust project
        assert!(stdout.contains(
            "WDK metadata is not available. Skipping driver build workflow for package: \
             rust-project"
        ));

        assert!(stdout.contains("Processing completed for package: driver_1"));
        assert!(stdout.contains("Processing completed for package: driver_2"));
        assert!(stdout.contains(r"Build completed successfully"));

        // assert umdf-driver-workspace driver package
        assert!(PathBuf::from(
            "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_1_package"
        )
        .exists());
        assert!(PathBuf::from(
            "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_1_package/\
             driver_1.cat"
        )
        .exists());
        assert!(PathBuf::from(
            "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_1_package/\
             driver_1.inf"
        )
        .exists());
        assert!(PathBuf::from(
            "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_1_package/\
             driver_1.map"
        )
        .exists());
        assert!(PathBuf::from(
            "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_1_package/\
             driver_1.pdb"
        )
        .exists());
        assert!(PathBuf::from(
            "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_1_package/\
             driver_1.dll"
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
            "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_2_package/\
             driver_2.cat"
        )
        .exists());
        assert!(PathBuf::from(
            "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_2_package/\
             driver_2.inf"
        )
        .exists());
        assert!(PathBuf::from(
            "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_2_package/\
             driver_2.map"
        )
        .exists());
        assert!(PathBuf::from(
            "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_2_package/\
             driver_2.pdb"
        )
        .exists());
        assert!(PathBuf::from(
            "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_2_package/\
             driver_2.dll"
        )
        .exists());
        assert!(PathBuf::from(
            "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_2_package/\
             WDRLocalTestCert.cer"
        )
        .exists());

        // assert if the files are copied properly
        assert_eq!(
            digest_file(Path::new(
                "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_1_package/\
                 driver_1.map"
            )),
            digest_file(Path::new(
                "tests/emulated-workspace/umdf-driver-workspace/target/debug/deps/driver_1.map"
            ))
        );

        assert_eq!(
            digest_file(Path::new(
                "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_1_package/\
                 driver_1.pdb"
            )),
            digest_file(Path::new(
                "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_1.pdb"
            ))
        );

        assert_eq!(
            digest_file(Path::new(
                "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_1_package/\
                 WDRLocalTestCert.cer"
            )),
            digest_file(Path::new(
                "tests/emulated-workspace/umdf-driver-workspace/target/debug/WDRLocalTestCert.cer"
            ))
        );

        assert_eq!(
            digest_file(Path::new(
                "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_2_package/\
                 driver_2.map"
            )),
            digest_file(Path::new(
                "tests/emulated-workspace/umdf-driver-workspace/target/debug/deps/driver_2.map"
            ))
        );

        assert_eq!(
            digest_file(Path::new(
                "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_2_package/\
                 driver_2.pdb"
            )),
            digest_file(Path::new(
                "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_2.pdb"
            ))
        );

        assert_eq!(
            digest_file(Path::new(
                "tests/emulated-workspace/umdf-driver-workspace/target/debug/driver_2_package/\
                 WDRLocalTestCert.cer"
            )),
            digest_file(Path::new(
                "tests/emulated-workspace/umdf-driver-workspace/target/debug/WDRLocalTestCert.cer"
            ))
        );
    });
}
