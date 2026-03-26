//! System level tests for cargo wdk new flow
mod test_utils;
use std::path::PathBuf;

use assert_cmd::{Command, assert::OutputAssertExt};
use assert_fs::{TempDir, assert::PathAssert, prelude::PathChild};
use mockall::PredicateBooleanExt;
use test_utils::create_cargo_wdk_cmd;

#[test]
fn kmdf_driver_is_created_successfully() {
    project_is_created("kmdf");
}

#[test]
fn umdf_driver_is_created_successfully() {
    project_is_created("umdf");
}

#[test]
fn wdm_driver_is_created_successfully() {
    project_is_created("wdm");
}

#[test]
fn if_no_driver_type_given_command_fails() {
    test_command_invocation(&[], true, false, |stdout, stderr| {
        assert!(stdout.is_empty());
        assert!(stderr.contains("error: the following required arguments were not provided:"));
        assert!(stderr.contains("<--kmdf|--umdf|--wdm>"));
    });
}

#[test]
fn if_multiple_driver_types_given_command_fails() {
    test_command_invocation(&["--kmdf", "--umdf"], true, false, |stdout, stderr| {
        assert!(stdout.is_empty());
        assert!(stderr.contains("error: the argument '--kmdf' cannot be used with '--umdf'"));
    });
}

#[test]
fn if_missing_required_arguments_command_fails() {
    test_command_invocation(&[], false, false, |stdout, stderr| {
        assert!(stdout.is_empty());
        assert!(stderr.contains("error: the following required arguments were not provided:"));
        assert!(stderr.contains("<--kmdf|--umdf|--wdm>"));
        assert!(stderr.contains("<PATH>"));
    });
}

#[test]
fn help_works() {
    test_command_invocation(&["--help"], false, true, |stdout, stderr| {
        assert!(stdout.contains("Create a new Windows Driver Kit project"));
        assert!(stdout.contains("Usage: cargo wdk new [OPTIONS] <--kmdf|--umdf|--wdm> <PATH>"));
        assert!(stderr.is_empty());
    });
}

fn project_is_created(driver_type: &str) {
    let tmp_dir = TempDir::new().expect("Unable to create new temp dir for test");
    let project_path = verify_project_creation(driver_type, &tmp_dir);

    // Build the project only if SKIP_BUILD_IN_CARGO_WDK_NEW_TESTS is not set.
    // This env var is used in release-plz PRs, wherein it is set to skip the
    // project build because it would fail due to not yet released
    // dependencies
    if std::env::var("SKIP_BUILD_IN_CARGO_WDK_NEW_TESTS").unwrap_or_default() == "1" {
        println!(
            "Skipping driver build due to SKIP_BUILD_IN_CARGO_WDK_NEW_TESTS environment variable"
        );
    } else {
        verify_project_build(&project_path);
    }
}

fn verify_project_creation(driver_type: &str, tmp_dir: &TempDir) -> PathBuf {
    let driver_name = format!("test-{driver_type}-driver");
    let driver_name_underscored = driver_name.replace('-', "_");

    println!("Temp dir: {}", tmp_dir.path().display());

    let driver_path = tmp_dir.join(driver_name.clone());
    let driver_path_str = driver_path.to_string_lossy();
    let args = [&format!("--{driver_type}"), driver_path_str.as_ref()];
    let mut cmd = create_cargo_wdk_cmd::<&str>("new", Some(&args), None, None);

    // assert command output
    let cmd_assertion = cmd.assert().success();
    let output = cmd_assertion.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("stdout: {stdout}");
    println!("stderr: {stderr}");
    println!("driver path: {}", driver_path.display());
    assert!(stderr.contains(&format!(
        "New {} driver crate created successfully at: {}",
        driver_type,
        tmp_dir.path().join(&driver_name).display()
    )));

    // assert paths
    assert!(tmp_dir.join(&driver_name).is_dir());
    assert!(tmp_dir.join(&driver_name).join(".git").is_dir());
    assert!(tmp_dir.join(&driver_name).join("build.rs").is_file());
    assert!(tmp_dir.join(&driver_name).join("Cargo.toml").is_file());
    assert!(
        tmp_dir
            .join(&driver_name)
            .join(format!("{driver_name_underscored}.inx"))
            .is_file()
    );
    assert!(
        tmp_dir
            .join(&driver_name)
            .join("src")
            .join("lib.rs")
            .is_file()
    );
    assert!(
        tmp_dir
            .join(&driver_name)
            .join(".cargo")
            .join("config.toml")
            .is_file()
    );

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

    driver_path
}

fn verify_project_build(path: &std::path::Path) {
    // assert if cargo wdk build works on the created driver project
    let mut cmd = create_cargo_wdk_cmd("build", None, None, Some(path));

    let output = cmd
        .output()
        .expect("Failed to execute cargo wdk build command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "Cargo wdk build command succeeded unexpectedly. \nSTDOUT: {stdout}\nSTDERR: {stderr}"
    );

    let stderr: String = stderr.into();
    // Assert build output contains expected errors (the INF file is intentionally
    // incomplete)
    assert!(
        stderr.contains(
            "Required directive Provider missing, empty, or invalid in [Version] section."
        )
    );
    assert!(
        stderr
            .contains("Required directive Class missing, empty, or invalid in [Version] section.")
    );
    assert!(
        stderr
            .contains("Invalid ClassGuid \"\", expecting {XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}.")
    );
    assert!(stderr.contains("INF is NOT VALID"));
}

fn test_command_invocation<F: FnOnce(&str, &str)>(
    args: &[&str],
    add_path_arg: bool,
    command_succeeds: bool,
    assert: F,
) {
    let mut args = args
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<String>>();
    args.insert(0, String::from("new"));

    if add_path_arg {
        let driver_name = "test-driver";
        let tmp_dir = TempDir::new().expect("Unable to create new temp dir for test");
        println!("Temp dir: {}", tmp_dir.path().display());
        let driver_path = tmp_dir.join(driver_name);
        args.push(driver_path.to_string_lossy().to_string());
    }

    let mut cmd = Command::cargo_bin("cargo-wdk").expect("unable to find cargo-wdk binary");
    cmd.args(args);

    let output = cmd
        .output()
        .expect("Failed to execute cargo wdk build command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success() == command_succeeds,
        "Cargo wdk new command did not execute as expected. \nSTDOUT: {stdout}\nSTDERR: {stderr}",
    );

    println!("stdout: {stdout}");
    println!("stderr: {stderr}");

    assert(&stdout, &stderr);
}
