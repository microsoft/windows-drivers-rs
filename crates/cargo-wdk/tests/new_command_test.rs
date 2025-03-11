//! System level tests for cargo wdk new flow
#![allow(clippy::literal_string_with_formatting_args)]
mod common;
use assert_cmd::Command;
use assert_fs::TempDir;
use common::set_crt_static_flag;
use serial_test::serial;

#[test]
#[serial]
fn given_a_cargo_wdk_new_command_when_driver_type_is_kmdf_then_it_creates_valid_driver_project() {
    let (stdout, _stderr) = create_and_build_new_driver_project("kmdf");
    assert!(stdout
        .contains("Required directive Provider missing, empty, or invalid in [Version] section."));
    assert!(stdout
        .contains("Required directive Class missing, empty, or invalid in [Version] section."));
    assert!(stdout
        .contains("Invalid ClassGuid \"\", expecting {XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}."));
    assert!(stdout.contains("INF is NOT VALID"));
}

#[test]
#[serial]
fn given_a_cargo_wdk_new_command_when_driver_type_is_umdf_then_it_creates_valid_driver_project() {
    let (stdout, _stderr) = create_and_build_new_driver_project("umdf");
    assert!(stdout
        .contains("Required directive Provider missing, empty, or invalid in [Version] section."));
    assert!(stdout
        .contains("Required directive Class missing, empty, or invalid in [Version] section."));
    assert!(stdout
        .contains("Invalid ClassGuid \"\", expecting {XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}."));
    assert!(stdout.contains("INF is NOT VALID"));
}

#[test]
#[serial]
fn given_a_cargo_wdk_new_command_when_driver_type_is_wdm_then_it_creates_valid_driver_project() {
    let (stdout, _stderr) = create_and_build_new_driver_project("wdm");
    assert!(stdout
        .contains("Required directive Provider missing, empty, or invalid in [Version] section."));
    assert!(stdout
        .contains("Required directive Class missing, empty, or invalid in [Version] section."));
    assert!(stdout
        .contains("Invalid ClassGuid \"\", expecting {XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}."));
    assert!(stdout.contains("INF is NOT VALID"));
}

fn create_and_build_new_driver_project(driver_type: &str) -> (String, String) {
    let driver_name = format!("test-{driver_type}-driver");
    let driver_name_underscored = driver_name.replace('-', "_");
    let tmp_dir = TempDir::new().expect("Unable to create new temp dir for test");
    println!("Temp dir: {}", tmp_dir.path().display());
    let mut cmd = Command::cargo_bin("cargo-wdk").expect("unable to find cargo-wdk binary");
    cmd.args([
        "new",
        &driver_name,
        driver_type,
        "--cwd",
        &tmp_dir.to_string_lossy(), // Root dir for tests is cargo-wdk
    ]);

    // assert command output
    let cmd_assertion = cmd.assert().success();
    let output = cmd_assertion.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("stdout: {stdout}");
    println!("stderr: {}", String::from_utf8_lossy(&output.stderr));
    println!(
        "driver path: {}",
        tmp_dir.path().join(&driver_name).display()
    );
    assert!(stdout.contains(&format!(
        "New Driver Project {} created at {}",
        &driver_name_underscored,
        tmp_dir.join(&driver_name).display()
    )));

    assert!(tmp_dir.join(&driver_name).exists());
    assert!(tmp_dir.join(&driver_name).join("build.rs").exists());
    assert!(tmp_dir.join(&driver_name).join("Cargo.toml").exists());
    assert!(tmp_dir
        .join(&driver_name)
        .join(format!("{driver_name_underscored}.inx"))
        .exists());
    assert!(tmp_dir
        .join(&driver_name)
        .join("src")
        .join("lib.rs")
        .exists());

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
