//! System level tests for cargo wdk new flow
#![allow(clippy::literal_string_with_formatting_args)]
mod common;
use std::path::PathBuf;

use assert_cmd::Command;
use assert_fs::{assert::PathAssert, prelude::PathChild, TempDir};
use common::{set_crt_static_flag, with_file_lock};
use mockall::PredicateBooleanExt;

#[test]
fn given_a_cargo_wdk_new_command_when_driver_type_is_kmdf_then_it_creates_valid_driver_project() {
    with_file_lock(|| {
        let (stdout, _stderr) = create_and_build_new_driver_project("kmdf");
        assert!(stdout.contains(
            "Required directive Provider missing, empty, or invalid in [Version] section."
        ));
        assert!(stdout
            .contains("Required directive Class missing, empty, or invalid in [Version] section."));
        assert!(stdout
            .contains("Invalid ClassGuid \"\", expecting {XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}."));
        assert!(stdout.contains("INF is NOT VALID"));
    });
}

#[test]
fn given_a_cargo_wdk_new_command_when_driver_type_is_umdf_then_it_creates_valid_driver_project() {
    with_file_lock(|| {
        let (stdout, _stderr) = create_and_build_new_driver_project("umdf");
        assert!(stdout.contains(
            "Required directive Provider missing, empty, or invalid in [Version] section."
        ));
        assert!(stdout
            .contains("Required directive Class missing, empty, or invalid in [Version] section."));
        assert!(stdout
            .contains("Invalid ClassGuid \"\", expecting {XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}."));
        assert!(stdout.contains("INF is NOT VALID"));
    });
}

#[test]
fn given_a_cargo_wdk_new_command_when_driver_type_is_wdm_then_it_creates_valid_driver_project() {
    with_file_lock(|| {
        let (stdout, _stderr) = create_and_build_new_driver_project("wdm");
        assert!(stdout.contains(
            "Required directive Provider missing, empty, or invalid in [Version] section."
        ));
        assert!(stdout
            .contains("Required directive Class missing, empty, or invalid in [Version] section."));
        assert!(stdout
            .contains("Invalid ClassGuid \"\", expecting {XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}."));
        assert!(stdout.contains("INF is NOT VALID"));
    });
}

#[test]
fn given_a_cargo_wdk_new_command_when_no_driver_type_is_provided_then_it_fails() {
    with_file_lock(|| {
        let driver_name = "test-invalid-driver";
        let tmp_dir = TempDir::new().expect("Unable to create new temp dir for test");
        println!("Temp dir: {}", tmp_dir.path().display());
        let driver_path = tmp_dir.join(driver_name);
        let mut cmd = Command::cargo_bin("cargo-wdk").expect("unable to find cargo-wdk binary");
        cmd.args(["new", driver_path.to_string_lossy().as_ref()]);

        // assert command output
        let cmd_assertion = cmd.assert().failure();
        let output = cmd_assertion.get_output();
        let stdout = String::from_utf8_lossy(&output.stdout);
        println!("stdout: {stdout}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        println!("stderr: {stderr}");
        assert!(stderr.contains("error: the following required arguments were not provided:"));
        assert!(stderr.contains("<--kmdf|--umdf|--wdm>"));
    });
}

fn create_and_build_new_driver_project(driver_type: &str) -> (String, String) {
    let driver_name = format!("test-{driver_type}-driver");
    let driver_name_underscored = driver_name.replace('-', "_");
    let tmp_dir = TempDir::new().expect("Unable to create new temp dir for test");
    println!("Temp dir: {}", tmp_dir.path().display());
    let driver_path = tmp_dir.join(driver_name.clone());
    let mut cmd = Command::cargo_bin("cargo-wdk").expect("unable to find cargo-wdk binary");
    cmd.args([
        "new",
        &format!("--{driver_type}"),
        driver_path.to_string_lossy().as_ref(),
    ]);

    // assert command output
    let cmd_assertion = cmd.assert().success();
    let output = cmd_assertion.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("stdout: {stdout}");
    println!("stderr: {}", String::from_utf8_lossy(&output.stderr));
    println!("driver path: {}", driver_path.display());
    assert!(stdout.contains(&format!(
        "New {} driver crate created successfully at: {}",
        driver_type,
        tmp_dir.path().join(&driver_name).display()
    )));

    // asert paths
    assert!(tmp_dir.join(&driver_name).is_dir());
    assert!(tmp_dir.join(&driver_name).join("build.rs").is_file());
    assert!(tmp_dir.join(&driver_name).join("Cargo.toml").is_file());
    assert!(tmp_dir
        .join(&driver_name)
        .join(format!("{driver_name_underscored}.inx"))
        .is_file());
    assert!(tmp_dir
        .join(&driver_name)
        .join("src")
        .join("lib.rs")
        .is_file());
    assert!(tmp_dir
        .join(&driver_name)
        .join(".cargo")
        .join("config.toml")
        .is_file());

    // assert content
    let driver_name_path = PathBuf::from(&driver_name);
    tmp_dir
        .child(driver_name_path.join("build.rs"))
        .assert(predicates::str::contains(
            "wdk_build::configure_wdk_binary_build()",
        ));
    tmp_dir.child(driver_name_path.join("Cargo.toml")).assert(
        predicates::str::contains("[package.metadata.wdk.driver-model]").and(
            predicates::str::contains(format!("driver-type = \"{}\"", driver_type.to_uppercase()))
                .and(predicates::str::contains("crate-type = [\"cdylib\"]")),
        ),
    );
    tmp_dir
        .child(driver_name_path.join(format!("{driver_name_underscored}.inx")))
        .assert(
            predicates::str::contains("[Version]").and(
                predicates::str::contains(format!("CatalogFile = {driver_name_underscored}.cat"))
                    .and(
                        predicates::str::contains("[Manufacturer]")
                            .and(predicates::str::contains("[Strings]")),
                    ),
            ),
        );
    tmp_dir
        .child(driver_name_path.join("src").join("lib.rs"))
        .assert(predicates::str::is_empty().not());
    tmp_dir
        .child(driver_name_path.join(".cargo").join("config.toml"))
        .assert(predicates::str::contains("target-feature=+crt-static"));

    // assert if cargo wdk build works on the created driver project
    set_crt_static_flag();

    let mut cmd = Command::cargo_bin("cargo-wdk").expect("unable to find cargo-wdk binary");
    cmd.args([
        "build",
        "--cwd",
        &tmp_dir.join(&driver_name).to_string_lossy(), // Root dir for tests is cargo-wdk
    ]);

    let cmd_assertion = cmd.assert().failure();
    tmp_dir
        .close()
        .expect("Unable to close temp dir after test");
    let output = cmd_assertion.get_output();
    let stdout: String = String::from_utf8_lossy(&output.stdout).into();
    let stderr: String = String::from_utf8_lossy(&output.stderr).into();
    (stdout, stderr)
}
