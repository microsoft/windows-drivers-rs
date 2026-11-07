// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
#![allow(clippy::too_many_lines)] // Package tests are longer and splitting them into sub functions can make the code less readable
#![allow(clippy::ref_option_ref)] // This is suppressed for mockall as it generates mocks with env_vars: &Option
use std::{
    collections::HashMap,
    os::windows::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::{ExitStatus, Output},
    result::Result::Ok,
};

use cargo_metadata::Metadata as CargoMetadata;
use clap_cargo::Features;
use mockall::{Sequence, predicate::eq};
use mockall_double::double;
use wdk_build::{
    CpuArchitecture,
    DriverConfig,
    metadata::{TryFromCargoMetadataError, Wdk},
};

#[double]
use crate::providers::{
    exec::CommandExec,
    fs::Fs,
    metadata::Metadata as MetadataProvider,
    wdk_build::WdkBuild,
};
use crate::{
    actions::build::{
        BuildAction,
        BuildActionParams,
        IMAGE_FILE_MACHINE_AMD64,
        IMAGE_FILE_MACHINE_ARM64,
        Profile,
        SignMode,
        TargetPlatform,
        error::BuildActionError,
        to_target_triple,
    },
    providers::error::{CommandError, FileError},
};

////////////////////////////////////////////////////////////////////////////////
/// Standalone driver project tests
////////////////////////////////////////////////////////////////////////////////
// Test name is of form Given When Then
// Given: A driver project
// When: Default values are provided
// Then: It builds successfully
#[test]
pub fn given_a_driver_project_when_default_values_are_provided_then_it_builds_successfully() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = CpuArchitecture::Amd64;
    let verify_signature = false;
    let sample_class = false;
    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(&wdk_metadata));

    let cargo_build_output =
        create_cargo_build_output_json(driver_name, driver_version, &cwd, None, profile);

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_default_build_task_steps(Some(cargo_build_output), 25100u32)
        .expect_default_package_task_steps(driver_name, target_arch, verify_signature);

    assert_build_action_run_with_env_is_success(
        &cwd,
        profile,
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
}

#[test]
pub fn given_a_driver_project_when_profile_is_release_then_it_builds_successfully() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = Some(Profile::Release);
    let target_arch = CpuArchitecture::Amd64;
    let verify_signature = false;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(&wdk_metadata));

    let cargo_build_output =
        create_cargo_build_output_json(driver_name, driver_version, &cwd, None, profile);
    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_default_build_task_steps(Some(cargo_build_output), 25100u32)
        .expect_default_package_task_steps(driver_name, target_arch, verify_signature);

    assert_build_action_run_with_env_is_success(
        &cwd,
        profile,
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
}

#[test]
pub fn given_a_driver_project_when_target_arch_is_arm64_then_it_builds_successfully() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = CpuArchitecture::Arm64;
    let verify_signature = false;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(&wdk_metadata));

    let cargo_build_output = create_cargo_build_output_json(
        driver_name,
        driver_version,
        &cwd,
        Some(&to_target_triple(target_arch)),
        profile,
    );
    let test_build_action =
        &TestBuildAction::new(cwd.clone(), profile, Some(target_arch), sample_class)
            .set_up_standalone_driver_project((workspace_member, package))
            .expect_default_build_task_steps(Some(cargo_build_output), 25100u32)
            .expect_default_package_task_steps(driver_name, target_arch, verify_signature);

    assert_build_action_run_with_env_is_success(
        &cwd,
        profile,
        Some(target_arch),
        verify_signature,
        sample_class,
        test_build_action,
    );
}

#[test]
pub fn given_a_driver_project_when_profile_is_release_and_target_arch_is_arm64_then_it_builds_successfully()
 {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = Some(Profile::Release);
    let target_arch = CpuArchitecture::Arm64;
    let verify_signature = false;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(&wdk_metadata));

    let cargo_build_output = create_cargo_build_output_json(
        driver_name,
        driver_version,
        &cwd,
        Some(&to_target_triple(target_arch)),
        profile,
    );

    let test_build_action =
        &TestBuildAction::new(cwd.clone(), profile, Some(target_arch), sample_class)
            .set_up_standalone_driver_project((workspace_member, package))
            .expect_default_build_task_steps(Some(cargo_build_output), 25100u32)
            .expect_default_package_task_steps(driver_name, target_arch, verify_signature);

    assert_build_action_run_with_env_is_success(
        &cwd,
        profile,
        Some(target_arch),
        verify_signature,
        sample_class,
        test_build_action,
    );
}

#[test]
pub fn given_sample_class_is_true_and_wdk_build_is_above_range_then_infverif_runs_with_samples_flag()
 {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = CpuArchitecture::Amd64;
    let verify_signature = false;
    let sample_class = true;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(&wdk_metadata));

    let cargo_build_output =
        create_cargo_build_output_json(driver_name, driver_version, &cwd, None, profile);
    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_default_build_task_steps(Some(cargo_build_output), 26101u32)
        .expect_package_task_steps_without_infverif(driver_name, target_arch, verify_signature)
        .expect_infverif(driver_name, &cwd, Some("/samples"), None)
        .expect_detect_wdk_build_number(26101u32);

    assert_build_action_run_with_env_is_success(
        &cwd,
        profile,
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
}

#[test]
pub fn given_sample_class_is_true_and_wdk_build_below_range_then_infverif_runs_with_msft_flag() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = CpuArchitecture::Amd64;
    let verify_signature = false;
    let sample_class = true;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(&wdk_metadata));

    let cargo_build_output =
        create_cargo_build_output_json(driver_name, driver_version, &cwd, None, profile);
    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_default_build_task_steps(Some(cargo_build_output), 25797u32)
        .expect_package_task_steps_without_infverif(driver_name, target_arch, verify_signature)
        .expect_infverif(driver_name, &cwd, Some("/msft"), None)
        .expect_detect_wdk_build_number(25797u32);

    assert_build_action_run_with_env_is_success(
        &cwd,
        profile,
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
}

#[test]
pub fn given_sample_class_is_true_and_wdk_build_within_range_then_infverif_is_skipped() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = CpuArchitecture::Amd64;
    let verify_signature = false;
    let sample_class = true;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(&wdk_metadata));

    let cargo_build_output =
        create_cargo_build_output_json(driver_name, driver_version, &cwd, None, profile);
    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_default_build_task_steps(Some(cargo_build_output), 26100u32)
        .expect_package_task_steps_without_infverif(driver_name, target_arch, verify_signature)
        .expect_detect_wdk_build_number(26100u32);

    assert_build_action_run_with_env_is_success(
        &cwd,
        profile,
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
}

#[test]
pub fn given_a_driver_project_when_verify_signature_is_true_then_it_builds_successfully() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = CpuArchitecture::Amd64;
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(&wdk_metadata));

    let cargo_build_output =
        create_cargo_build_output_json(driver_name, driver_version, &cwd, None, profile);
    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_default_build_task_steps(Some(cargo_build_output), 25100u32)
        .expect_default_package_task_steps(driver_name, target_arch, verify_signature);

    assert_build_action_run_with_env_is_success(
        &cwd,
        profile,
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
}

#[test]
pub fn given_a_driver_project_when_sign_mode_is_off_then_signing_and_verification_steps_are_skipped()
 {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = CpuArchitecture::Amd64;
    let verify_signature = false;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(&wdk_metadata));

    let cargo_build_output =
        create_cargo_build_output_json(driver_name, driver_version, &cwd, None, profile);
    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .with_sign_mode(SignMode::Off)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_default_build_task_steps(Some(cargo_build_output), 25100u32)
        .expect_package_task_steps_with_sign_mode_off(driver_name, target_arch);

    assert_build_action_run_with_env_is_success(
        &cwd,
        profile,
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
}

#[test]
pub fn given_a_driver_project_when_locked_is_set_then_it_is_forwarded_to_cargo_invocations() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = CpuArchitecture::Amd64;
    let verify_signature = false;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(&wdk_metadata));

    let cargo_build_output =
        create_cargo_build_output_json(driver_name, driver_version, &cwd, None, profile);

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .with_locked(true)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_default_build_task_steps(Some(cargo_build_output), 25100u32)
        .expect_default_package_task_steps(driver_name, target_arch, verify_signature);

    assert_build_action_run_with_env_is_success(
        &cwd,
        profile,
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
}

#[test]
pub fn given_a_driver_project_when_self_signed_exists_then_it_should_skip_calling_makecert() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = CpuArchitecture::Amd64;
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(&wdk_metadata));
    let expected_certmgr_output = Output {
        status: ExitStatus::default(),
        stdout: r"==============Certificate # 1 ==========
                    Subject::
                    [0,0] 2.5.4.3 (CN) WDRLocalTestCert
                    Issuer::
                    [0,0] 2.5.4.3 (CN) WDRLocalTestCert
                    SerialNumber::
                    5E 04 0D 63 35 20 76 A5 4A E1 96 BF CF 01 0F 96
                    SHA1 Thumbprint::
                        FB972842 C63CD369 E07D0C71 88E17921 B5813C71
                    MD5 Thumbprint::
                        832B3F18 707EA3F6 54465207 345A93F1
                    Provider Type:: 1 Provider Name:: Microsoft Strong Cryptographic Provider Container: 68f79a6e-6afa-4ec7-be5b-16d6656edd3f KeySpec: 2
                    NotBefore::
                    Tue Jan 28 13:51:04 2025
                    NotAfter::
                    Sun Jan 01 05:29:59 2040
                    ==============No CTLs ==========
                    ==============No CRLs ==========
                    ==============================================
                    CertMgr Succeeded".as_bytes().to_vec(),
        stderr: vec![],
    };

    let expected_create_cert_output = Output {
        status: ExitStatus::default(),
        stdout: vec![],
        stderr: vec![],
    };

    let cargo_build_output =
        create_cargo_build_output_json(driver_name, driver_version, &cwd, None, profile);

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_default_build_task_steps(Some(cargo_build_output), 25100u32)
        .expect_read_driver_binary(driver_name, &cwd, target_arch)
        .expect_final_package_dir_created(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true, &cwd)
        .expect_stampinf(driver_name, &cwd, target_arch, None)
        .expect_inf2cat(driver_name, &cwd, target_arch, None)
        .expect_self_signed_cert_file_exists(&cwd, false)
        .expect_certmgr_exists_check(Some(expected_certmgr_output))
        .expect_certmgr_create_cert_from_store(&cwd, Some(expected_create_cert_output))
        .expect_copy_self_signed_cert_file_to_package_folder(driver_name, &cwd, true)
        .expect_signtool_sign_driver_binary_sys_file(driver_name, &cwd, None)
        .expect_signtool_sign_cat_file(driver_name, &cwd, None)
        .expect_infverif(driver_name, &cwd, None, None)
        .expect_signtool_verify_driver_binary_sys_file(driver_name, &cwd, None)
        .expect_signtool_verify_cat_file(driver_name, &cwd, None);

    assert_build_action_run_with_env_is_success(
        &cwd,
        profile,
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
}

#[test]
pub fn given_a_driver_project_when_package_dir_exists_then_it_is_removed_and_recreated() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = CpuArchitecture::Amd64;
    let verify_signature = false;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(&wdk_metadata));
    let expected_certmgr_output = get_certmgr_success_output();

    let cargo_build_output =
        create_cargo_build_output_json(driver_name, driver_version, &cwd, None, profile);

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_default_build_task_steps(Some(cargo_build_output), 25100u32)
        .expect_read_driver_binary(driver_name, &cwd, target_arch)
        .expect_final_package_dir_created(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true, &cwd)
        .expect_stampinf(driver_name, &cwd, target_arch, None)
        .expect_inf2cat(driver_name, &cwd, target_arch, None)
        .expect_self_signed_cert_file_exists(&cwd, false)
        .expect_certmgr_exists_check(Some(expected_certmgr_output))
        .expect_makecert(&cwd, None)
        .expect_copy_self_signed_cert_file_to_package_folder(driver_name, &cwd, true)
        .expect_signtool_sign_driver_binary_sys_file(driver_name, &cwd, None)
        .expect_signtool_sign_cat_file(driver_name, &cwd, None)
        .expect_infverif(driver_name, &cwd, None, None);

    assert_build_action_run_with_env_is_success(
        &cwd,
        profile,
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
}

#[test]
pub fn given_a_driver_project_when_inx_file_do_not_exist_then_package_should_fail() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = CpuArchitecture::Amd64;
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(&wdk_metadata));

    let cargo_build_output =
        create_cargo_build_output_json(driver_name, driver_version, &cwd, None, profile);

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_default_build_task_steps(Some(cargo_build_output), 25100u32)
        .expect_read_driver_binary(driver_name, &cwd, target_arch)
        .expect_inx_file_exists(driver_name, &cwd, false);

    let build_action = initialize_build_action(
        &cwd,
        profile.as_ref(),
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
    assert!(build_action.is_ok());
    let run_result = build_action.expect("Failed to init build action").run();

    assert!(matches!(
        run_result.as_ref().expect_err("expected error"),
        BuildActionError::OneOrMoreDriversFailedToPackage(_)
    ));
}

#[test]
pub fn given_a_driver_project_when_copy_of_an_artifact_fails_then_the_package_should_fail() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = CpuArchitecture::Amd64;
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(&wdk_metadata));

    let cargo_build_output = create_cargo_build_output_json(
        driver_name,
        driver_version,
        &cwd,
        Some(&to_target_triple(target_arch)),
        profile,
    );

    let test_build_action =
        &TestBuildAction::new(cwd.clone(), profile, Some(target_arch), sample_class)
            .set_up_standalone_driver_project((workspace_member, package))
            .expect_default_build_task_steps(Some(cargo_build_output), 25100u32)
            .expect_read_driver_binary(driver_name, &cwd, target_arch)
            .expect_final_package_dir_created(driver_name, &cwd, true)
            .expect_inx_file_exists(driver_name, &cwd, true)
            .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
            .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, false);

    let build_action = initialize_build_action(
        &cwd,
        profile.as_ref(),
        Some(target_arch),
        verify_signature,
        sample_class,
        test_build_action,
    );
    assert!(build_action.is_ok());
    let run_result = build_action.expect("Failed to init build action").run();

    assert!(matches!(
        run_result.as_ref().expect_err("expected error"),
        BuildActionError::OneOrMoreDriversFailedToPackage(_)
    ));
}

#[test]
pub fn given_a_driver_project_when_stampinf_command_execution_fails_then_package_should_fail() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = CpuArchitecture::Amd64;
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(&wdk_metadata));

    let expected_stampinf_output = Output {
        status: ExitStatus::from_raw(1),
        stdout: vec![],
        stderr: vec![],
    };

    let cargo_build_output =
        create_cargo_build_output_json(driver_name, driver_version, &cwd, None, profile);

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_default_build_task_steps(Some(cargo_build_output), 25100u32)
        .expect_read_driver_binary(driver_name, &cwd, target_arch)
        .expect_final_package_dir_created(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true, &cwd)
        .expect_stampinf(
            driver_name,
            &cwd,
            target_arch,
            Some(expected_stampinf_output),
        );

    let build_action = initialize_build_action(
        &cwd,
        profile.as_ref(),
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
    assert!(build_action.is_ok());
    let run_result = run_build_action(build_action);

    assert!(matches!(
        run_result.as_ref().expect_err("expected error"),
        BuildActionError::OneOrMoreDriversFailedToPackage(_)
    ));
}

#[test]
pub fn given_a_driver_project_when_inf2cat_command_execution_fails_then_package_should_fail() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = CpuArchitecture::Amd64;
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(&wdk_metadata));

    let expected_inf2cat_output = Output {
        status: ExitStatus::from_raw(1),
        stdout: vec![],
        stderr: vec![],
    };

    let cargo_build_output =
        create_cargo_build_output_json(driver_name, driver_version, &cwd, None, profile);

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_default_build_task_steps(Some(cargo_build_output), 25100u32)
        .expect_read_driver_binary(driver_name, &cwd, target_arch)
        .expect_final_package_dir_created(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true, &cwd)
        .expect_stampinf(driver_name, &cwd, target_arch, None)
        .expect_inf2cat(
            driver_name,
            &cwd,
            target_arch,
            Some(expected_inf2cat_output),
        );

    let build_action = initialize_build_action(
        &cwd,
        profile.as_ref(),
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
    assert!(build_action.is_ok());
    let run_result = run_build_action(build_action);

    assert!(matches!(
        run_result.as_ref().expect_err("expected error"),
        BuildActionError::OneOrMoreDriversFailedToPackage(_)
    ));
}

#[test]
pub fn given_a_driver_project_when_certmgr_command_execution_fails_then_package_should_fail() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = CpuArchitecture::Amd64;
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(&wdk_metadata));

    let expected_output = Output {
        status: ExitStatus::from_raw(1),
        stdout: vec![],
        stderr: vec![],
    };

    let cargo_build_output =
        create_cargo_build_output_json(driver_name, driver_version, &cwd, None, profile);

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_default_build_task_steps(Some(cargo_build_output), 25100u32)
        .expect_read_driver_binary(driver_name, &cwd, target_arch)
        .expect_final_package_dir_created(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true, &cwd)
        .expect_stampinf(driver_name, &cwd, target_arch, None)
        .expect_inf2cat(driver_name, &cwd, target_arch, None)
        .expect_infverif(driver_name, &cwd, None, None)
        .expect_self_signed_cert_file_exists(&cwd, false)
        .expect_certmgr_exists_check(Some(expected_output));

    let build_action = initialize_build_action(
        &cwd,
        profile.as_ref(),
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
    assert!(build_action.is_ok());
    let run_result = run_build_action(build_action);

    assert!(matches!(
        run_result.as_ref().expect_err("expected error"),
        BuildActionError::OneOrMoreDriversFailedToPackage(_)
    ));
}

#[test]
pub fn given_a_driver_project_when_makecert_command_execution_fails_then_package_should_fail() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = CpuArchitecture::Amd64;
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(&wdk_metadata));

    let expected_output = Output {
        status: ExitStatus::from_raw(1),
        stdout: vec![],
        stderr: vec![],
    };

    let cargo_build_output =
        create_cargo_build_output_json(driver_name, driver_version, &cwd, None, profile);

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_default_build_task_steps(Some(cargo_build_output), 25100u32)
        .expect_read_driver_binary(driver_name, &cwd, target_arch)
        .expect_final_package_dir_created(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true, &cwd)
        .expect_stampinf(driver_name, &cwd, target_arch, None)
        .expect_inf2cat(driver_name, &cwd, target_arch, None)
        .expect_infverif(driver_name, &cwd, None, None)
        .expect_self_signed_cert_file_exists(&cwd, false)
        .expect_certmgr_exists_check(None)
        .expect_makecert(&cwd, Some(expected_output));

    let build_action = initialize_build_action(
        &cwd,
        profile.as_ref(),
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
    assert!(build_action.is_ok());
    let run_result = run_build_action(build_action);

    assert!(matches!(
        run_result.as_ref().expect_err("expected error"),
        BuildActionError::OneOrMoreDriversFailedToPackage(_)
    ));
}

#[test]
pub fn given_a_driver_project_when_signtool_command_execution_fails_then_package_should_fail() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = CpuArchitecture::Amd64;
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(&wdk_metadata));

    let cargo_build_output =
        create_cargo_build_output_json(driver_name, driver_version, &cwd, None, profile);

    let expected_output = Output {
        status: ExitStatus::from_raw(1),
        stdout: vec![],
        stderr: vec![],
    };

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_default_build_task_steps(Some(cargo_build_output), 25100u32)
        .expect_read_driver_binary(driver_name, &cwd, target_arch)
        .expect_final_package_dir_created(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true, &cwd)
        .expect_stampinf(driver_name, &cwd, target_arch, None)
        .expect_inf2cat(driver_name, &cwd, target_arch, None)
        .expect_infverif(driver_name, &cwd, None, None)
        .expect_self_signed_cert_file_exists(&cwd, false)
        .expect_certmgr_exists_check(None)
        .expect_makecert(&cwd, None)
        .expect_copy_self_signed_cert_file_to_package_folder(driver_name, &cwd, true)
        .expect_signtool_sign_driver_binary_sys_file(driver_name, &cwd, Some(expected_output));

    let build_action = initialize_build_action(
        &cwd,
        profile.as_ref(),
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
    assert!(build_action.is_ok());
    let run_result = run_build_action(build_action);

    assert!(matches!(
        run_result.as_ref().expect_err("expected error"),
        BuildActionError::OneOrMoreDriversFailedToPackage(_)
    ));
}

#[test]
pub fn given_a_driver_project_when_infverif_command_execution_fails_then_package_should_fail() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = CpuArchitecture::Amd64;
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(&wdk_metadata));

    let cargo_build_output =
        create_cargo_build_output_json(driver_name, driver_version, &cwd, None, profile);

    let expected_output = Output {
        status: ExitStatus::from_raw(1),
        stdout: vec![],
        stderr: vec![],
    };

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_default_build_task_steps(Some(cargo_build_output), 25100u32)
        .expect_read_driver_binary(driver_name, &cwd, target_arch)
        .expect_final_package_dir_created(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true, &cwd)
        .expect_stampinf(driver_name, &cwd, target_arch, None)
        .expect_inf2cat(driver_name, &cwd, target_arch, None)
        .expect_infverif(driver_name, &cwd, None, Some(expected_output));

    let build_action = initialize_build_action(
        &cwd,
        profile.as_ref(),
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
    assert!(build_action.is_ok());
    let run_result = run_build_action(build_action);

    assert!(matches!(
        run_result.as_ref().expect_err("expected error"),
        BuildActionError::OneOrMoreDriversFailedToPackage(_)
    ));
}

#[test]
pub fn given_a_non_driver_project_when_default_values_are_provided_with_no_wdk_metadata_are_provided_then_build_should_be_successful()
 {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_name = "non-driver";
    let driver_version = "0.0.1";
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, None);

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_default_build_task_steps(None, 25100u32);

    assert_build_action_run_is_success(
        &cwd,
        profile,
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
}

#[test]
pub fn given_a_invalid_driver_project_with_partial_wdk_metadata_when_valid_default_values_are_provided_then_wdk_metadata_parse_should_fail()
 {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp\\sample-driver");
    let profile = None;
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let cargo_toml_metadata = invalid_driver_cargo_toml();

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_with_custom_toml(&cargo_toml_metadata)
        .expect_default_build_task_steps(None, 25100u32);

    let build_action = initialize_build_action(
        &cwd,
        profile.as_ref(),
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
    assert!(build_action.is_ok());
    let run_result = build_action.expect("Failed to init build action").run();
    assert!(matches!(
        run_result.as_ref().expect_err("expected error"),
        BuildActionError::WdkMetadataParse(TryFromCargoMetadataError::WdkMetadataDeserialization {
            metadata_source: _,
            error_source: _
        })
    ));
}

#[test]
pub fn given_a_driver_project_when_driver_binary_cannot_be_read_then_packaging_should_fail() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let verify_signature = false;
    let sample_class = false;
    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(&wdk_metadata));

    let cargo_build_output =
        create_cargo_build_output_json(driver_name, driver_version, &cwd, None, profile);

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_default_build_task_steps(Some(cargo_build_output), 25100u32)
        .expect_read_driver_binary_returning(driver_name, &cwd, 1, |path, _, _| {
            Err(FileError::OpenError(
                path.to_owned(),
                std::io::Error::from_raw_os_error(32),
            ))
        });

    let build_action = initialize_build_action(
        &cwd,
        profile.as_ref(),
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
    assert!(build_action.is_ok());
    let run_result = build_action.expect("Failed to init build action").run();
    assert!(matches!(
        run_result.as_ref().expect_err("expected error"),
        BuildActionError::OneOrMoreDriversFailedToPackage(_)
    ));
}
////////////////////////////////////////////////////////////////////////////////
/// Workspace tests
////////////////////////////////////////////////////////////////////////////////
#[test]
pub fn given_a_workspace_with_multiple_driver_and_non_driver_projects_when_default_values_are_provided_then_it_packages_successfully()
 {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = CpuArchitecture::Amd64;
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name_1 = "sample-kmdf-1";
    let driver_version_1 = "0.0.1";
    let driver_name_2 = "sample-kmdf-2";
    let driver_version_2 = "0.0.2";
    let non_driver = "non-driver";
    let non_driver_version = "0.0.3";

    // Create artifact outputs for workspace packages
    let artifact_1 = create_cargo_build_output_json_with_manifest(
        driver_name_1,
        driver_version_1,
        &cwd,
        &cwd.join(driver_name_1).join("Cargo.toml"),
        None,
        profile,
        true,
    );
    let artifact_2 = create_cargo_build_output_json_with_manifest(
        driver_name_2,
        driver_version_2,
        &cwd,
        &cwd.join(driver_name_2).join("Cargo.toml"),
        None,
        profile,
        true,
    );
    let artifact_non_driver = create_cargo_build_output_json_with_manifest(
        non_driver,
        non_driver_version,
        &cwd,
        &cwd.join(non_driver).join("Cargo.toml"),
        None,
        profile,
        false,
    );
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member_1, package_1) = get_cargo_metadata_package(
        &cwd.join(driver_name_1),
        driver_name_1,
        driver_version_1,
        Some(&wdk_metadata),
    );
    let (workspace_member_2, package_2) = get_cargo_metadata_package(
        &cwd.join(driver_name_2),
        driver_name_2,
        driver_version_2,
        Some(&wdk_metadata),
    );
    let (workspace_member_3, package_3) =
        get_cargo_metadata_package(&cwd.join(non_driver), non_driver, non_driver_version, None);

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_workspace_with_multiple_driver_projects(
            &cwd,
            Some(wdk_metadata),
            vec![
                (workspace_member_1, package_1),
                (workspace_member_2, package_2),
                (workspace_member_3, package_3),
            ],
        )
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        // A single cargo build for the whole workspace
        .expect_cargo_build(
            &cwd,
            Some(combine_cargo_build_outputs(&[
                artifact_1,
                artifact_2,
                artifact_non_driver,
            ])),
        )
        .expect_default_package_task_steps_for_workspace(
            driver_name_1,
            target_arch,
            verify_signature,
        )
        // Second driver project
        .expect_default_package_task_steps_for_workspace(
            driver_name_2,
            target_arch,
            verify_signature,
        );

    assert_build_action_run_with_env_is_success(
        &cwd,
        profile,
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
}

#[test]
pub fn given_a_workspace_with_multiple_driver_and_non_driver_projects_when_cwd_is_driver_project_then_it_packages_driver_project_successfully()
 {
    // Input CLI args
    let workspace_root_dir = PathBuf::from("C:\\tmp");
    let cwd = workspace_root_dir.join("sample-kmdf-1");
    let profile = None;
    let target_arch = CpuArchitecture::Amd64;
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name_1 = "sample-kmdf-1";
    let driver_version_1 = "0.0.1";
    let driver_name_2 = "sample-kmdf-2";
    let driver_version_2 = "0.0.2";
    let non_driver = "non-driver";
    let non_driver_version = "0.0.3";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member_1, package_1) = get_cargo_metadata_package(
        &workspace_root_dir.join(driver_name_1),
        driver_name_1,
        driver_version_1,
        Some(&wdk_metadata),
    );
    let (workspace_member_2, package_2) = get_cargo_metadata_package(
        &workspace_root_dir.join(driver_name_2),
        driver_name_2,
        driver_version_2,
        Some(&wdk_metadata),
    );
    let (workspace_member_3, package_3) = get_cargo_metadata_package(
        &workspace_root_dir.join(non_driver),
        non_driver,
        non_driver_version,
        None,
    );

    let expected_certmgr_output = get_certmgr_success_output();

    let cargo_build_output = create_cargo_build_output_json_with_manifest(
        driver_name_1,
        driver_version_1,
        &workspace_root_dir,
        &workspace_root_dir.join(driver_name_1).join("Cargo.toml"),
        None,
        profile,
        true,
    );

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class) // Even when cwd is changed to driver project inside the workspace, cargo metadata read
        // is going to be for the whole workspace
        .set_up_workspace_with_multiple_driver_projects(
            &workspace_root_dir,
            Some(wdk_metadata),
            vec![
                (workspace_member_1, package_1),
                (workspace_member_2, package_2),
                (workspace_member_3, package_3),
            ],
        )
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        .expect_cargo_build(&cwd, Some(cargo_build_output))
        .expect_read_driver_binary(driver_name_1, &workspace_root_dir, target_arch)
        .expect_final_package_dir_created(driver_name_1, &workspace_root_dir, true)
        .expect_inx_file_exists(driver_name_1, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name_1, &workspace_root_dir)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name_1, &workspace_root_dir, true)
        .expect_copy_pdb_file_to_package_folder(driver_name_1, &workspace_root_dir, true)
        .expect_copy_inx_file_to_package_folder(driver_name_1, &cwd, true, &workspace_root_dir)
        .expect_stampinf(driver_name_1, &workspace_root_dir, target_arch, None)
        .expect_inf2cat(driver_name_1, &workspace_root_dir, target_arch, None)
        .expect_self_signed_cert_file_exists(&workspace_root_dir, false)
        .expect_certmgr_exists_check(Some(expected_certmgr_output))
        .expect_makecert(&workspace_root_dir, None)
        .expect_copy_self_signed_cert_file_to_package_folder(
            driver_name_1,
            &workspace_root_dir,
            true,
        )
        .expect_signtool_sign_driver_binary_sys_file(driver_name_1, &workspace_root_dir, None)
        .expect_signtool_sign_cat_file(driver_name_1, &workspace_root_dir, None)
        .expect_signtool_verify_driver_binary_sys_file(driver_name_1, &workspace_root_dir, None)
        .expect_signtool_verify_cat_file(driver_name_1, &workspace_root_dir, None)
        .expect_infverif(driver_name_1, &workspace_root_dir, None, None);

    assert_build_action_run_with_env_is_success(
        &cwd,
        profile,
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
}

#[test]
pub fn given_a_workspace_with_multiple_driver_and_non_driver_projects_when_verify_signature_is_false_then_it_skips_verify_tasks()
 {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = CpuArchitecture::Amd64;
    let verify_signature = false;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name_1 = "sample-kmdf-1";
    let driver_version_1 = "0.0.1";
    let driver_name_2 = "sample-kmdf-2";
    let driver_version_2 = "0.0.2";
    let non_driver = "non-driver";
    let non_driver_version = "0.0.3";

    // Create artifact outputs for workspace packages
    let artifact_1 = create_cargo_build_output_json_with_manifest(
        driver_name_1,
        driver_version_1,
        &cwd,
        &cwd.join(driver_name_1).join("Cargo.toml"),
        None,
        profile,
        true,
    );
    let artifact_2 = create_cargo_build_output_json_with_manifest(
        driver_name_2,
        driver_version_2,
        &cwd,
        &cwd.join(driver_name_2).join("Cargo.toml"),
        None,
        profile,
        true,
    );
    let artifact_non_driver = create_cargo_build_output_json_with_manifest(
        non_driver,
        non_driver_version,
        &cwd,
        &cwd.join(non_driver).join("Cargo.toml"),
        None,
        profile,
        false, // NOT a driver - will use "lib" instead of "cdylib"
    );
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member_1, package_1) = get_cargo_metadata_package(
        &cwd.join(driver_name_1),
        driver_name_1,
        driver_version_1,
        Some(&wdk_metadata),
    );
    let (workspace_member_2, package_2) = get_cargo_metadata_package(
        &cwd.join(driver_name_2),
        driver_name_2,
        driver_version_2,
        Some(&wdk_metadata),
    );
    let (workspace_member_3, package_3) =
        get_cargo_metadata_package(&cwd.join(non_driver), non_driver, non_driver_version, None);

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_workspace_with_multiple_driver_projects(
            &cwd,
            Some(wdk_metadata),
            vec![
                (workspace_member_1, package_1),
                (workspace_member_2, package_2),
                (workspace_member_3, package_3),
            ],
        )
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        // A single cargo build for the whole workspace
        .expect_cargo_build(
            &cwd,
            Some(combine_cargo_build_outputs(&[
                artifact_1,
                artifact_2,
                artifact_non_driver,
            ])),
        )
        .expect_default_package_task_steps_for_workspace(
            driver_name_1,
            target_arch,
            verify_signature,
        )
        // Second driver project
        .expect_default_package_task_steps_for_workspace(
            driver_name_2,
            target_arch,
            verify_signature,
        );

    assert_build_action_run_with_env_is_success(
        &cwd,
        profile,
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
}

#[test]
pub fn given_a_workspace_with_multiple_driver_and_non_driver_projects_when_cwd_is_non_driver_project_then_it_builds_but_skips_packaging()
 {
    // Input CLI args
    let workspace_root_dir = PathBuf::from("C:\\tmp");
    let cwd = workspace_root_dir.join("non-driver");
    let profile = None;
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name_1 = "sample-kmdf-1";
    let driver_version_1 = "0.0.1";
    let driver_name_2 = "sample-kmdf-2";
    let driver_version_2 = "0.0.2";
    let non_driver = "non-driver";
    let non_driver_version = "0.0.3";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member_1, package_1) = get_cargo_metadata_package(
        &workspace_root_dir.join(driver_name_1),
        driver_name_1,
        driver_version_1,
        Some(&wdk_metadata),
    );
    let (workspace_member_2, package_2) = get_cargo_metadata_package(
        &workspace_root_dir.join(driver_name_2),
        driver_name_2,
        driver_version_2,
        Some(&wdk_metadata),
    );
    let (workspace_member_3, package_3) = get_cargo_metadata_package(
        &workspace_root_dir.join(non_driver),
        non_driver,
        non_driver_version,
        None,
    );

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class) // Even when cwd is changed to driver project inside the workspace, cargo metadata read
        // is going to be for the whole workspace
        .set_up_workspace_with_multiple_driver_projects(
            &workspace_root_dir,
            Some(wdk_metadata),
            vec![
                (workspace_member_1, package_1),
                (workspace_member_2, package_2),
                (workspace_member_3, package_3),
            ],
        )
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        .expect_cargo_build(&cwd, None);

    assert_build_action_run_is_success(
        &cwd,
        profile,
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
}

#[test]
pub fn given_a_workspace_with_multiple_distinct_wdk_configurations_at_each_workspace_member_level_when_default_values_are_provided_then_wdk_metadata_parse_should_fail()
 {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_type_1 = "KMDF";
    let driver_name_1 = "sample-kmdf-1";
    let driver_type_2 = "UMDF";
    let driver_version_1 = "0.0.1";
    let driver_name_2 = "sample-kmdf-2";
    let driver_version_2 = "0.0.2";
    let wdk_metadata_1 = get_cargo_metadata_wdk_metadata(driver_type_1, 1, 33);
    let wdk_metadata_2 = get_cargo_metadata_wdk_metadata(driver_type_2, 1, 33);
    let (workspace_member_1, package_1) = get_cargo_metadata_package(
        &cwd.join(driver_name_1),
        driver_name_1,
        driver_version_1,
        Some(&wdk_metadata_1),
    );
    let (workspace_member_2, package_2) = get_cargo_metadata_package(
        &cwd.join(driver_name_2),
        driver_name_2,
        driver_version_2,
        Some(&wdk_metadata_2),
    );

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_workspace_with_multiple_driver_projects(
            &cwd,
            Some(wdk_metadata_1),
            vec![
                (workspace_member_1, package_1),
                (workspace_member_2, package_2),
            ],
        )
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        .expect_cargo_build(&cwd, None);

    let build_action = initialize_build_action(
        &cwd,
        profile.as_ref(),
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
    assert!(build_action.is_ok());
    let run_result = build_action.expect("Failed to init build action").run();

    assert!(matches!(
        run_result.expect_err("run_result error in test: given_a_workspace_with_multiple_distinct_wdk_configurations_at_each_workspace_member_level_when_default_values_are_provided_then_wdk_metadata_parse_should_fail"),
        BuildActionError::WdkMetadataParse(
            TryFromCargoMetadataError::MultipleWdkConfigurationsDetected {
                wdk_metadata_configurations: _
            }
        )
    ));
}

#[test]
pub fn given_a_workspace_with_multiple_distinct_wdk_configurations_at_root_and_workspace_member_level_when_default_values_are_provided_then_wdk_metadata_parse_should_fail()
 {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_type_1 = "KMDF";
    let driver_name_1 = "sample-kmdf-1";
    let driver_type_2 = "UMDF";
    let driver_version_1 = "0.0.1";
    let driver_name_2 = "sample-kmdf-2";
    let driver_version_2 = "0.0.2";
    let wdk_metadata_1 = get_cargo_metadata_wdk_metadata(driver_type_1, 1, 33);
    let wdk_metadata_2 = get_cargo_metadata_wdk_metadata(driver_type_2, 1, 33);
    let (workspace_member_1, package_1) = get_cargo_metadata_package(
        &cwd.join(driver_name_1),
        driver_name_1,
        driver_version_1,
        Some(&wdk_metadata_1),
    );
    let (workspace_member_2, package_2) = get_cargo_metadata_package(
        &cwd.join(driver_name_2),
        driver_name_2,
        driver_version_2,
        Some(&wdk_metadata_1),
    );

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_workspace_with_multiple_driver_projects(
            &cwd,
            Some(wdk_metadata_2),
            vec![
                (workspace_member_1, package_1),
                (workspace_member_2, package_2),
            ],
        )
        .expect_root_manifest_exists(&cwd, true)
        .expect_detect_wdk_build_number(25100u32)
        .expect_cargo_build(&cwd, None);

    let build_action = initialize_build_action(
        &cwd,
        profile.as_ref(),
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
    assert!(build_action.is_ok());
    let run_result = build_action.expect("Failed to init build action").run();

    assert!(matches!(
        run_result.expect_err("run_result error in test: given_a_workspace_with_multiple_distinct_wdk_configurations_at_root_and_workspace_member_level_when_default_values_are_provided_then_wdk_metadata_parse_should_fail"),
        BuildActionError::WdkMetadataParse(
            TryFromCargoMetadataError::MultipleWdkConfigurationsDetected {
                wdk_metadata_configurations: _
            }
        )
    ));
}

#[test]
pub fn given_a_workspace_only_with_non_driver_projects_when_cwd_is_workspace_root_then_build_should_be_successful()
 {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let non_driver = "non-driver";
    let non_driver_version = "0.0.3";
    let (workspace_member_3, package_3) =
        get_cargo_metadata_package(&cwd.join(non_driver), non_driver, non_driver_version, None);

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class) // Even when cwd is changed to driver project inside the workspace, cargo metadata read
        // is going to be for the whole workspace
        .set_up_workspace_with_multiple_driver_projects(
            &cwd,
            None,
            vec![(workspace_member_3, package_3)],
        )
        .expect_root_manifest_exists(&cwd, true)
        .expect_detect_wdk_build_number(25100u32)
        .expect_cargo_build(&cwd, None);

    assert_build_action_run_is_success(
        &cwd,
        profile,
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
}

#[test]
pub fn given_a_workspace_only_with_non_driver_projects_when_cwd_is_workspace_member_then_build_should_be_successful()
 {
    // Input CLI args
    let workspace_root_dir = PathBuf::from("C:\\tmp");
    let cwd = workspace_root_dir.join("non-driver");
    let profile = None;
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let non_driver = "non-driver";
    let non_driver_version = "0.0.3";
    let (workspace_member_3, package_3) = get_cargo_metadata_package(
        &workspace_root_dir.join(non_driver),
        non_driver,
        non_driver_version,
        None,
    );

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class) // Even when cwd is changed to driver project inside the workspace, cargo metadata read
        // is going to be for the whole workspace
        .set_up_workspace_with_multiple_driver_projects(
            &workspace_root_dir,
            None,
            vec![(workspace_member_3, package_3)],
        )
        .expect_root_manifest_exists(&cwd, true)
        .expect_detect_wdk_build_number(25100u32)
        .expect_cargo_build(&cwd, None);

    assert_build_action_run_is_success(
        &cwd,
        profile,
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
}

#[test]
pub fn given_a_workspace_when_default_members_are_a_subset_then_only_those_drivers_are_packaged() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = CpuArchitecture::Amd64;
    let verify_signature = false;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name_1 = "sample-kmdf-1";
    let driver_version_1 = "0.0.1";
    let driver_name_2 = "sample-kmdf-2";
    let driver_version_2 = "0.0.2";
    let artifact_1 = create_cargo_build_output_json_with_manifest(
        driver_name_1,
        driver_version_1,
        &cwd,
        &cwd.join(driver_name_1).join("Cargo.toml"),
        None,
        profile,
        true,
    );
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member_1, package_1) = get_cargo_metadata_package(
        &cwd.join(driver_name_1),
        driver_name_1,
        driver_version_1,
        Some(&wdk_metadata),
    );
    let (workspace_member_2, package_2) = get_cargo_metadata_package(
        &cwd.join(driver_name_2),
        driver_name_2,
        driver_version_2,
        Some(&wdk_metadata),
    );

    // `default-members` selects only the first driver, so cargo builds and
    // cargo-wdk packages only that driver
    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .with_default_members(vec![workspace_member_1.clone()])
        .set_up_workspace_with_multiple_driver_projects(
            &cwd,
            Some(wdk_metadata),
            vec![
                (workspace_member_1, package_1),
                (workspace_member_2, package_2),
            ],
        )
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        .expect_cargo_build(&cwd, Some(artifact_1))
        .expect_default_package_task_steps_for_workspace(
            driver_name_1,
            target_arch,
            verify_signature,
        );

    assert_build_action_run_with_env_is_success(
        &cwd,
        profile,
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
}

#[test]
pub fn given_a_workspace_when_workspace_flag_is_set_then_all_driver_members_are_packaged() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = CpuArchitecture::Amd64;
    let verify_signature = false;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name_1 = "sample-kmdf-1";
    let driver_version_1 = "0.0.1";
    let driver_name_2 = "sample-kmdf-2";
    let driver_version_2 = "0.0.2";
    let artifact_1 = create_cargo_build_output_json_with_manifest(
        driver_name_1,
        driver_version_1,
        &cwd,
        &cwd.join(driver_name_1).join("Cargo.toml"),
        None,
        profile,
        true,
    );
    let artifact_2 = create_cargo_build_output_json_with_manifest(
        driver_name_2,
        driver_version_2,
        &cwd,
        &cwd.join(driver_name_2).join("Cargo.toml"),
        None,
        profile,
        true,
    );
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member_1, package_1) = get_cargo_metadata_package(
        &cwd.join(driver_name_1),
        driver_name_1,
        driver_version_1,
        Some(&wdk_metadata),
    );
    let (workspace_member_2, package_2) = get_cargo_metadata_package(
        &cwd.join(driver_name_2),
        driver_name_2,
        driver_version_2,
        Some(&wdk_metadata),
    );

    // `--workspace` selects all members even though only the first driver is a
    // default member, so both drivers are packaged
    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .with_workspace(true)
        .with_default_members(vec![workspace_member_1.clone()])
        .set_up_workspace_with_multiple_driver_projects(
            &cwd,
            Some(wdk_metadata),
            vec![
                (workspace_member_1, package_1),
                (workspace_member_2, package_2),
            ],
        )
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        .expect_cargo_build(
            &cwd,
            Some(combine_cargo_build_outputs(&[artifact_1, artifact_2])),
        )
        .expect_default_package_task_steps_for_workspace(
            driver_name_1,
            target_arch,
            verify_signature,
        )
        .expect_default_package_task_steps_for_workspace(
            driver_name_2,
            target_arch,
            verify_signature,
        );

    assert_build_action_run_with_env_is_success(
        &cwd,
        profile,
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
}

#[test]
pub fn given_a_workspace_when_a_driver_is_built_only_as_a_dependency_then_it_is_not_packaged() {
    // Input CLI args
    let workspace_root_dir = PathBuf::from("C:\\tmp");
    let cwd = workspace_root_dir.join("non-driver");
    let profile = None;
    let verify_signature = false;
    let sample_class = false;

    // Driver project data: the non-driver member depends on the driver, so
    // building the non-driver member also builds the driver
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let non_driver = "non-driver";
    let non_driver_version = "0.0.3";
    let driver_artifact = create_cargo_build_output_json_with_manifest(
        driver_name,
        driver_version,
        &workspace_root_dir,
        &workspace_root_dir.join(driver_name).join("Cargo.toml"),
        None,
        profile,
        true,
    );
    let non_driver_artifact = create_cargo_build_output_json_with_manifest(
        non_driver,
        non_driver_version,
        &workspace_root_dir,
        &cwd.join("Cargo.toml"),
        None,
        profile,
        false,
    );
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (driver_member, driver_package) = get_cargo_metadata_package(
        &workspace_root_dir.join(driver_name),
        driver_name,
        driver_version,
        Some(&wdk_metadata),
    );
    let (non_driver_member, non_driver_package) =
        get_cargo_metadata_package(&cwd, non_driver, non_driver_version, None);

    // The non-driver member in cwd is the only selected package. No probe or
    // packaging expectations are set, so packaging the driver fails the test
    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_workspace_with_multiple_driver_projects(
            &workspace_root_dir,
            Some(wdk_metadata),
            vec![
                (driver_member, driver_package),
                (non_driver_member, non_driver_package),
            ],
        )
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        .expect_cargo_build(
            &cwd,
            Some(combine_cargo_build_outputs(&[
                driver_artifact,
                non_driver_artifact,
            ])),
        );

    assert_build_action_run_is_success(
        &cwd,
        profile,
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
}

#[test]
pub fn given_a_workspace_when_a_cdylib_member_has_no_wdk_metadata_then_it_is_not_packaged() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = CpuArchitecture::Amd64;
    let verify_signature = false;
    let sample_class = false;

    // Driver project data: `helper-dll` is a selected cdylib member without WDK
    // metadata
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let helper = "helper-dll";
    let helper_version = "0.0.3";
    let driver_artifact = create_cargo_build_output_json_with_manifest(
        driver_name,
        driver_version,
        &cwd,
        &cwd.join(driver_name).join("Cargo.toml"),
        None,
        profile,
        true,
    );
    let helper_artifact = create_cargo_build_output_json_with_manifest(
        helper,
        helper_version,
        &cwd,
        &cwd.join(helper).join("Cargo.toml"),
        None,
        profile,
        true,
    );
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (driver_member, driver_package) = get_cargo_metadata_package(
        &cwd.join(driver_name),
        driver_name,
        driver_version,
        Some(&wdk_metadata),
    );
    let (helper_member, helper_package) =
        get_cargo_metadata_package(&cwd.join(helper), helper, helper_version, None);

    // Only the driver has packaging expectations, so packaging `helper-dll`
    // fails the test
    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_workspace_with_multiple_driver_projects(
            &cwd,
            Some(wdk_metadata),
            vec![
                (driver_member, driver_package),
                (helper_member, helper_package),
            ],
        )
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        .expect_cargo_build(
            &cwd,
            Some(combine_cargo_build_outputs(&[
                driver_artifact,
                helper_artifact,
            ])),
        )
        .expect_default_package_task_steps_for_workspace(
            driver_name,
            target_arch,
            verify_signature,
        );

    assert_build_action_run_with_env_is_success(
        &cwd,
        profile,
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
}

#[test]
pub fn given_a_workspace_when_cargo_build_fails_then_no_driver_is_packaged() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let verify_signature = false;
    let sample_class = false;

    // No metadata or packaging expectations are set, so reading metadata or
    // running any packaging step after the failed build fails the test
    let mut test_build_action = TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true);
    test_build_action
        .mock_run_command
        .expect_run()
        .withf(|_, args, _, _| args.first() == Some(&"build"))
        .once()
        .returning(|_, _, _, _| {
            Err(CommandError::from_output(
                "cargo",
                &["build"],
                &Output {
                    status: ExitStatus::from_raw(101),
                    stdout: vec![],
                    stderr: vec![],
                },
            ))
        });

    let build_action = initialize_build_action(
        &cwd,
        profile.as_ref(),
        None,
        verify_signature,
        sample_class,
        &test_build_action,
    );
    assert!(matches!(
        run_build_action(build_action),
        Err(BuildActionError::BuildTask(_))
    ));
}

#[test]
pub fn given_a_workspace_when_build_output_order_differs_from_metadata_then_all_drivers_are_packaged()
 {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = CpuArchitecture::Amd64;
    let verify_signature = false;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name_1 = "sample-kmdf-1";
    let driver_version_1 = "0.0.1";
    let driver_name_2 = "sample-kmdf-2";
    let driver_version_2 = "0.0.2";
    let artifact_1 = create_cargo_build_output_json_with_manifest(
        driver_name_1,
        driver_version_1,
        &cwd,
        &cwd.join(driver_name_1).join("Cargo.toml"),
        None,
        profile,
        true,
    );
    let artifact_2 = create_cargo_build_output_json_with_manifest(
        driver_name_2,
        driver_version_2,
        &cwd,
        &cwd.join(driver_name_2).join("Cargo.toml"),
        None,
        profile,
        true,
    );
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member_1, package_1) = get_cargo_metadata_package(
        &cwd.join(driver_name_1),
        driver_name_1,
        driver_version_1,
        Some(&wdk_metadata),
    );
    let (workspace_member_2, package_2) = get_cargo_metadata_package(
        &cwd.join(driver_name_2),
        driver_name_2,
        driver_version_2,
        Some(&wdk_metadata),
    );

    // Cargo reports the second driver first
    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_workspace_with_multiple_driver_projects(
            &cwd,
            Some(wdk_metadata),
            vec![
                (workspace_member_1, package_1),
                (workspace_member_2, package_2),
            ],
        )
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        .expect_cargo_build(
            &cwd,
            Some(combine_cargo_build_outputs(&[artifact_2, artifact_1])),
        )
        .expect_default_package_task_steps_for_workspace(
            driver_name_2,
            target_arch,
            verify_signature,
        )
        .expect_default_package_task_steps_for_workspace(
            driver_name_1,
            target_arch,
            verify_signature,
        );

    assert_build_action_run_with_env_is_success(
        &cwd,
        profile,
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
}

#[test]
pub fn given_a_driver_project_when_build_output_has_no_artifact_for_it_then_packaging_is_skipped() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let verify_signature = false;
    let sample_class = false;
    // Driver project data
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata("KMDF", 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(&wdk_metadata));

    // The build output has no artifact for the driver. No probe or packaging
    // expectations are set, so any packaging step fails the test
    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_default_build_task_steps(None, 25100u32);

    assert_build_action_run_is_success(
        &cwd,
        profile,
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
}

#[test]
pub fn given_a_driver_project_when_build_output_has_duplicate_artifacts_then_it_fails() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = CpuArchitecture::Amd64;
    let verify_signature = false;
    let sample_class = false;
    // Driver project data
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata("KMDF", 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(&wdk_metadata));
    let artifact = create_cargo_build_output_json(driver_name, driver_version, &cwd, None, profile);

    // The first artifact is packaged; the second one for the same driver fails
    // the run
    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_default_build_task_steps(
            Some(combine_cargo_build_outputs(&[artifact.clone(), artifact])),
            25100u32,
        )
        .expect_default_package_task_steps(driver_name, target_arch, verify_signature);

    let build_action = initialize_build_action(
        &cwd,
        profile.as_ref(),
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
    let result = run_build_action(build_action);
    assert!(
        matches!(
            result,
            Err(BuildActionError::CannotDetermineDriverBinary(ref message))
            if message.contains("more than one cdylib artifact")
        ),
        "Expected duplicate-artifact error, got: {result:?}"
    );
}

#[test]
pub fn given_a_driver_project_when_build_output_is_unreadable_then_it_fails() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let verify_signature = false;
    let sample_class = false;
    // Driver project data
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata("KMDF", 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(&wdk_metadata));

    // Invalid UTF-8 makes reading the build output fail
    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_default_build_task_steps(
            Some(Output {
                status: ExitStatus::default(),
                stdout: vec![0xFF, b'\n'],
                stderr: vec![],
            }),
            25100u32,
        );

    let build_action = initialize_build_action(
        &cwd,
        profile.as_ref(),
        None,
        verify_signature,
        sample_class,
        test_build_action,
    );
    let result = run_build_action(build_action);
    assert!(
        matches!(result, Err(BuildActionError::CargoBuildOutputParse(_))),
        "Expected CargoBuildOutputParse error, got: {result:?}"
    );
}

#[test]
pub fn given_a_driver_project_when_cargo_metadata_fails_after_the_build_then_it_fails() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let verify_signature = false;
    let sample_class = false;

    let mut test_build_action = TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .expect_default_build_task_steps(None, 25100u32);
    test_build_action
        .mock_metadata_provider
        .expect_get_cargo_metadata_at_path()
        .once()
        .return_once(|_, _, _, _| {
            Err(cargo_metadata::Error::Io(std::io::Error::other(
                "cargo metadata failed",
            )))
        });

    let build_action = initialize_build_action(
        &cwd,
        profile.as_ref(),
        None,
        verify_signature,
        sample_class,
        &test_build_action,
    );
    let result = run_build_action(build_action);
    assert!(
        matches!(result, Err(BuildActionError::CargoMetadata(_))),
        "Expected CargoMetadata error, got: {result:?}"
    );
}

#[test]
pub fn given_a_driver_project_when_cargo_metadata_does_not_report_default_members_then_it_fails() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let verify_signature = false;
    let sample_class = false;
    // Driver project data
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata("KMDF", 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(&wdk_metadata));

    // Cargo before 1.71 does not report `workspace_default_members`
    let workspace_members = [workspace_member];
    let mut metadata: serde_json::Value = serde_json::from_str(&get_cargo_metadata(
        &cwd,
        vec![package],
        &workspace_members,
        &workspace_members,
        None,
    ))
    .expect("Failed to parse cargo metadata");
    metadata
        .as_object_mut()
        .expect("Expected cargo metadata to be an object")
        .remove("workspace_default_members");
    let metadata: CargoMetadata =
        serde_json::from_value(metadata).expect("Failed to parse cargo metadata");

    // No probe or packaging expectations are set, so any packaging step fails
    // the test
    let mut test_build_action = TestBuildAction::new(cwd.clone(), profile, None, sample_class)
        .expect_default_build_task_steps(None, 25100u32);
    test_build_action
        .mock_metadata_provider
        .expect_get_cargo_metadata_at_path()
        .once()
        .return_once(|_, _, _, _| Ok(metadata));

    let build_action = initialize_build_action(
        &cwd,
        profile.as_ref(),
        None,
        verify_signature,
        sample_class,
        &test_build_action,
    );
    let result = run_build_action(build_action);
    assert!(
        matches!(
            result,
            Err(BuildActionError::CargoMetadata(ref source))
            if source.to_string().contains("did not report the workspace default members")
        ),
        "Expected missing default members error, got: {result:?}"
    );
}

////////////////////////////////////////////////////////////////////////////////
/// Metadata error tests
////////////////////////////////////////////////////////////////////////////////
#[test]
fn given_a_metadata_io_error_when_getting_metadata_then_cargo_metadata_error_is_preserved() {
    let cwd = PathBuf::from(r"C:\tmp");
    let mut test_build_action = TestBuildAction::new(cwd.clone(), None, None, false);
    test_build_action
        .mock_metadata_provider
        .expect_get_cargo_metadata_at_path()
        .once()
        .return_once(|_, _, _, _| {
            Err(cargo_metadata::Error::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "selected Cargo executable is missing",
            )))
        });
    let action =
        initialize_build_action(&cwd, None, None, false, false, &test_build_action).unwrap();

    let error = action.get_cargo_metadata(&cwd).unwrap_err();

    let BuildActionError::CargoMetadata(ref source) = error else {
        panic!("expected CargoMetadata error, got: {error:?}");
    };
    assert!(matches!(
        source.downcast_ref::<cargo_metadata::Error>(),
        Some(cargo_metadata::Error::Io(io_error)) if io_error.kind() == std::io::ErrorKind::NotFound
    ));
    assert_eq!(
        error.to_string(),
        "Failed to read Cargo metadata for the project/workspace"
    );
    assert!(
        format!("{:#}", anyhow::Error::new(error)).contains("selected Cargo executable is missing")
    );
}

fn assert_build_action_run_is_success(
    cwd: &PathBuf,
    profile: Option<Profile>,
    target_arch: Option<CpuArchitecture>,
    verify_signature: bool,
    sample_class: bool,
    test_build_action: &TestBuildAction,
) {
    let build_action = initialize_build_action(
        cwd,
        profile.as_ref(),
        target_arch,
        verify_signature,
        sample_class,
        test_build_action,
    );
    assert!(build_action.is_ok());
    let run_result = build_action.expect("Failed to init build action").run();
    assert!(
        run_result.is_ok(),
        "build action failed unexpectedly: {run_result:?}"
    );
}

/// Builds the minimal bytes of a PE image for `machine`: the `MZ` signature,
/// `e_lfanew` = 0x80, the `PE\0\0` signature and `Machine`.
fn pe_image(machine: u16) -> Vec<u8> {
    let mut data = vec![0; 0x86];
    data[..2].copy_from_slice(b"MZ");
    data[0x3C..0x40].copy_from_slice(&0x80_u32.to_le_bytes());
    data[0x80..0x84].copy_from_slice(b"PE\0\0");
    data[0x84..0x86].copy_from_slice(&machine.to_le_bytes());
    data
}

/// Returns a mock implementation of `Fs::seek_read_exact` that serves reads
/// from `image`, failing with `UnexpectedEof` like a real file when a read
/// goes past its end.
fn read_from_image(
    image: Vec<u8>,
) -> impl Fn(&Path, &mut [u8], u64) -> Result<(), FileError> + Send + 'static {
    move |path, buf, offset| {
        let bytes = usize::try_from(offset)
            .ok()
            .and_then(|start| image.get(start..start.checked_add(buf.len())?));
        bytes.map_or_else(
            || {
                Err(FileError::ReadError(
                    path.to_owned(),
                    std::io::ErrorKind::UnexpectedEof.into(),
                ))
            },
            |bytes| {
                buf.copy_from_slice(bytes);
                Ok(())
            },
        )
    }
}

fn initialize_build_action<'a>(
    cwd: &'a PathBuf,
    profile: Option<&'a Profile>,
    target_arch: Option<CpuArchitecture>,
    verify_signature: bool,
    sample_class: bool,
    test_build_action: &'a TestBuildAction,
) -> Result<BuildAction<'a>, anyhow::Error> {
    let sign_mode = match &test_build_action.sign_mode {
        SignMode::Off => SignMode::Off,
        SignMode::Test { signtool_args, .. } => SignMode::Test {
            verify_signature,
            signtool_args: signtool_args.clone(),
        },
    };
    BuildAction::new(
        &BuildActionParams {
            working_dir: cwd,
            profile,
            target_arch,
            sign_mode,
            inf2cat_args: None,
            stampinf_args: None,
            infverif_args: None,
            is_sample_class: sample_class,
            locked: test_build_action.locked,
            workspace: test_build_action.workspace,
            target_platform: TargetPlatform::Universal,
            features: &test_build_action.features,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        test_build_action.mock_wdk_build_provider(),
        test_build_action.mock_run_command(),
        test_build_action.mock_fs_provider(),
        test_build_action.mock_metadata_provider(),
    )
}

fn get_certmgr_success_output() -> Output {
    Output {
        status: ExitStatus::default(),
        stdout: r"==============No Certificates ==========
                            ==============No CTLs ==========
                            ==============No CRLs ==========
                            ==============================================
                            CertMgr Succeeded"
            .as_bytes()
            .to_vec(),
        stderr: vec![],
    }
}

fn assert_build_action_run_with_env_is_success(
    cwd: &PathBuf,
    profile: Option<Profile>,
    target_arch: Option<CpuArchitecture>,
    verify_signature: bool,
    sample_class: bool,
    test_build_action: &TestBuildAction,
) {
    let build_action = initialize_build_action(
        cwd,
        profile.as_ref(),
        target_arch,
        verify_signature,
        sample_class,
        test_build_action,
    );
    assert!(build_action.is_ok());
    let run_result = run_build_action(build_action);
    assert!(
        run_result.is_ok(),
        "build action with env failed unexpectedly: {run_result:?}"
    );
}

fn run_build_action(
    build_action: Result<BuildAction<'_>, anyhow::Error>,
) -> Result<(), BuildActionError> {
    let build_action = build_action.expect("Failed to init build action");
    crate::test_utils::with_env::<&str, &str, _, _>(&[], || build_action.run())
}

/// Helper functions
////////////////////////////////////////////////////////////////////////////////
struct TestBuildAction {
    cwd: PathBuf,
    profile: Option<Profile>,
    target_arch: Option<CpuArchitecture>,
    sample_class: bool,
    sign_mode: SignMode,
    locked: bool,
    workspace: bool,
    default_members: Option<Vec<TestMetadataWorkspaceMemberId>>,
    features: Features,

    cargo_metadata: Option<CargoMetadata>,
    // mocks
    mock_run_command: CommandExec,
    mock_wdk_build_provider: WdkBuild,
    mock_fs_provider: Fs,
    mock_metadata_provider: MetadataProvider,
}

impl TestBuildAction {
    fn new(
        cwd: PathBuf,
        profile: Option<Profile>,
        target_arch: Option<CpuArchitecture>,
        sample_class: bool,
    ) -> Self {
        let mock_run_command = CommandExec::default();
        let mock_wdk_build_provider = WdkBuild::default();
        let mock_fs_provider = Fs::default();
        let mock_metadata_provider = MetadataProvider::default();

        Self {
            cwd,
            profile,
            target_arch,
            sample_class,
            sign_mode: SignMode::Test {
                verify_signature: false,
                signtool_args: None,
            },
            locked: false,
            workspace: false,
            default_members: None,
            features: Features::default(),
            mock_run_command,
            mock_wdk_build_provider,
            mock_fs_provider,
            mock_metadata_provider,
            cargo_metadata: None,
        }
    }

    fn with_sign_mode(mut self, sign_mode: SignMode) -> Self {
        self.sign_mode = sign_mode;
        self
    }

    fn with_locked(mut self, locked: bool) -> Self {
        self.locked = locked;
        self
    }

    fn with_workspace(mut self, workspace: bool) -> Self {
        self.workspace = workspace;
        self
    }

    fn with_default_members(mut self, default_members: Vec<TestMetadataWorkspaceMemberId>) -> Self {
        self.default_members = Some(default_members);
        self
    }

    fn set_up_standalone_driver_project(
        mut self,
        package_metadata: (TestMetadataWorkspaceMemberId, TestMetadataPackage),
    ) -> Self {
        let workspace_members = [package_metadata.0];
        let cargo_toml_metadata = get_cargo_metadata(
            &self.cwd,
            vec![package_metadata.1],
            &workspace_members,
            &workspace_members,
            None,
        );
        let cargo_toml_metadata =
            serde_json::from_str::<cargo_metadata::Metadata>(&cargo_toml_metadata)
                .expect("Failed to parse cargo metadata in set_up_standalone_driver_project");
        let cargo_toml_metadata_clone = cargo_toml_metadata.clone();
        let expected_options: Vec<String> = if self.locked {
            vec!["--locked".to_string()]
        } else {
            vec![]
        };
        let expected_features = self.features.clone();
        let expected_cargo = std::env::var_os("CARGO").expect("run tests through cargo test");
        self.mock_metadata_provider
            .expect_get_cargo_metadata_at_path()
            .withf(move |cargo, _working_dir, other_options, features| {
                cargo == Path::new(&expected_cargo)
                    && *other_options == expected_options
                    && *features == expected_features
            })
            .once()
            .returning(move |_, _, _, _| Ok(cargo_toml_metadata_clone.clone()));
        self.cargo_metadata = Some(cargo_toml_metadata);
        self
    }

    fn set_up_workspace_with_multiple_driver_projects(
        mut self,
        workspace_root_dir: &Path,
        workspace_additional_metadata: Option<TestWdkMetadata>,
        package_metadata_list: Vec<(TestMetadataWorkspaceMemberId, TestMetadataPackage)>,
    ) -> Self {
        let (workspace_members, packages): (Vec<_>, Vec<_>) =
            package_metadata_list.into_iter().unzip();
        // Like Cargo: the member in cwd, otherwise all members (virtual root)
        let default_members = self.default_members.clone().unwrap_or_else(|| {
            workspace_members
                .iter()
                .zip(&packages)
                .find(|(_, package)| {
                    serde_json::from_str::<cargo_metadata::Package>(&package.0).is_ok_and(|p| {
                        p.manifest_path
                            .parent()
                            .is_some_and(|dir| dir.as_std_path() == self.cwd)
                    })
                })
                .map_or_else(
                    || workspace_members.clone(),
                    |(member, _)| vec![member.clone()],
                )
        });
        let cargo_toml_metadata = get_cargo_metadata(
            workspace_root_dir,
            packages,
            &workspace_members,
            &default_members,
            workspace_additional_metadata,
        );
        let cargo_toml_metadata = serde_json::from_str::<cargo_metadata::Metadata>(
            &cargo_toml_metadata,
        )
        .expect("Failed to parse cargo metadata in set_up_workspace_with_multiple_driver_projects");
        let cargo_toml_metadata_clone = cargo_toml_metadata.clone();
        let expected_options: Vec<String> = if self.locked {
            vec!["--locked".to_string()]
        } else {
            vec![]
        };
        let expected_features = self.features.clone();
        let expected_cargo = std::env::var_os("CARGO").expect("run tests through cargo test");
        self.mock_metadata_provider
            .expect_get_cargo_metadata_at_path()
            .withf(move |cargo, _working_dir, other_options, features| {
                cargo == Path::new(&expected_cargo)
                    && *other_options == expected_options
                    && *features == expected_features
            })
            .once()
            .returning(move |_, _, _, _| Ok(cargo_toml_metadata_clone.clone()));
        self.cargo_metadata = Some(cargo_toml_metadata);
        self
    }

    fn set_up_with_custom_toml(mut self, cargo_toml_metadata: &str) -> Self {
        let cargo_toml_metadata =
            serde_json::from_str::<cargo_metadata::Metadata>(cargo_toml_metadata)
                .expect("Failed to parse cargo metadata in set_up_with_custom_toml");
        let cargo_toml_metadata_clone = cargo_toml_metadata.clone();
        let expected_options: Vec<String> = if self.locked {
            vec!["--locked".to_string()]
        } else {
            vec![]
        };
        let expected_features = self.features.clone();
        let expected_cargo = std::env::var_os("CARGO").expect("run tests through cargo test");
        self.mock_metadata_provider
            .expect_get_cargo_metadata_at_path()
            .withf(move |cargo, _working_dir, other_options, features| {
                cargo == Path::new(&expected_cargo)
                    && *other_options == expected_options
                    && *features == expected_features
            })
            .once()
            .returning(move |_, _, _, _| Ok(cargo_toml_metadata_clone.clone()));
        self.cargo_metadata = Some(cargo_toml_metadata);
        self
    }

    fn setup_target_dir(&self, dir_path: &Path) -> PathBuf {
        let mut base = dir_path.join("target");
        let profile_dir_name = match self.profile {
            Some(Profile::Release) => "release",
            _ => "debug",
        };
        if let Some(target_arch) = self.target_arch {
            let triple = to_target_triple(target_arch);
            base = base.join(triple);
        }
        base.join(profile_dir_name)
    }
}

// Presence of method ensures specific mock expectation is set
// Dir argument in any method means to operate on the relevant dir
// Output argument in any method means to override return output from default
// is_success boolean means success result of copy operation
// does_exist boolean means existence of the file or dir
// is_created boolean means whether the dir was created or not
impl TestBuildAction {
    fn expect_default_build_task_steps(
        self,
        cargo_build_output: Option<Output>,
        wdk_build_number: u32,
    ) -> Self {
        let cwd = self.cwd.clone();
        self.expect_detect_wdk_build_number(wdk_build_number)
            .expect_root_manifest_exists(&cwd, true)
            .expect_cargo_build(&cwd, cargo_build_output)
    }

    fn expect_default_package_task_steps(
        self,
        driver_name: &str,
        target_arch: CpuArchitecture,
        verify_signature: bool,
    ) -> Self {
        let cwd = self.cwd.clone();
        self.expect_package_task_steps_without_infverif(driver_name, target_arch, verify_signature)
            .expect_infverif(driver_name, &cwd, None, None)
    }

    fn expect_package_task_steps_without_infverif(
        self,
        driver_name: &str,
        target_arch: CpuArchitecture,
        verify_signature: bool,
    ) -> Self {
        let cwd = self.cwd.clone();
        let expected_certmgr_output = get_certmgr_success_output();
        let expectations = self
            .expect_read_driver_binary(driver_name, &cwd, target_arch)
            .expect_final_package_dir_created(driver_name, &cwd, false)
            .expect_inx_file_exists(driver_name, &cwd, true)
            .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
            .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
            .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
            .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true, &cwd)
            .expect_stampinf(driver_name, &cwd, target_arch, None)
            .expect_inf2cat(driver_name, &cwd, target_arch, None)
            .expect_self_signed_cert_file_exists(&cwd, false)
            .expect_certmgr_exists_check(Some(expected_certmgr_output))
            .expect_makecert(&cwd, None)
            .expect_copy_self_signed_cert_file_to_package_folder(driver_name, &cwd, true)
            .expect_signtool_sign_driver_binary_sys_file(driver_name, &cwd, None)
            .expect_signtool_sign_cat_file(driver_name, &cwd, None);
        if !verify_signature {
            return expectations;
        }
        expectations
            .expect_signtool_verify_driver_binary_sys_file(driver_name, &cwd, None)
            .expect_signtool_verify_cat_file(driver_name, &cwd, None)
    }

    /// Sets up package-task expectations for `SignMode::Off`: stampinf,
    /// inf2cat, and infverif are still expected, but all certificate
    /// generation, signing, and signature-verification steps are skipped.
    fn expect_package_task_steps_with_sign_mode_off(
        self,
        driver_name: &str,
        target_arch: CpuArchitecture,
    ) -> Self {
        let cwd = self.cwd.clone();
        self.expect_read_driver_binary(driver_name, &cwd, target_arch)
            .expect_final_package_dir_created(driver_name, &cwd, false)
            .expect_inx_file_exists(driver_name, &cwd, true)
            .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
            .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
            .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
            .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true, &cwd)
            .expect_stampinf(driver_name, &cwd, target_arch, None)
            .expect_inf2cat(driver_name, &cwd, target_arch, None)
            .expect_infverif(driver_name, &cwd, None, None)
    }

    fn expect_default_package_task_steps_for_workspace(
        self,
        driver_name: &str,
        target_arch: CpuArchitecture,
        verify_signature: bool,
    ) -> Self {
        let cwd = self.cwd.clone();
        let expected_certmgr_output = get_certmgr_success_output();
        let expectations = self
            .expect_read_driver_binary(driver_name, &cwd, target_arch)
            .expect_final_package_dir_created(driver_name, &cwd, false)
            .expect_inx_file_exists(driver_name, &cwd.join(driver_name), true)
            .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
            .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
            .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
            .expect_copy_inx_file_to_package_folder(driver_name, &cwd.join(driver_name), true, &cwd)
            .expect_stampinf(driver_name, &cwd, target_arch, None)
            .expect_inf2cat(driver_name, &cwd, target_arch, None)
            .expect_self_signed_cert_file_exists(&cwd, false)
            .expect_certmgr_exists_check(Some(expected_certmgr_output))
            .expect_makecert(&cwd, None)
            .expect_copy_self_signed_cert_file_to_package_folder(driver_name, &cwd, true)
            .expect_signtool_sign_driver_binary_sys_file(driver_name, &cwd, None)
            .expect_signtool_sign_cat_file(driver_name, &cwd, None)
            .expect_infverif(driver_name, &cwd, None, None);
        if !verify_signature {
            return expectations;
        }
        expectations
            .expect_signtool_verify_driver_binary_sys_file(driver_name, &cwd, None)
            .expect_signtool_verify_cat_file(driver_name, &cwd, None)
    }

    fn expect_root_manifest_exists(mut self, root_dir: &Path, does_exist: bool) -> Self {
        self.mock_fs_provider
            .expect_exists()
            .with(eq(root_dir.join("Cargo.toml")))
            .once()
            .returning(move |_| does_exist);
        self
    }

    fn expect_self_signed_cert_file_exists(mut self, driver_dir: &Path, does_exist: bool) -> Self {
        let expected_target_dir = self.setup_target_dir(driver_dir);
        let expected_src_driver_cert_path = expected_target_dir.join("WDRLocalTestCert.cer");
        self.mock_fs_provider
            .expect_exists()
            .with(eq(expected_src_driver_cert_path))
            .once()
            .returning(move |_| does_exist);
        self
    }

    fn expect_final_package_dir_created(
        mut self,
        driver_name: &str,
        cwd: &Path,
        removes_existing: bool,
    ) -> Self {
        let expected_driver_name_underscored = driver_name.replace('-', "_");
        let expected_target_dir = self.setup_target_dir(cwd);
        let expected_package_dir =
            expected_target_dir.join(format!("{expected_driver_name_underscored}_package"));

        // `run()` checks existence, removes any stale dir, then creates a fresh
        // one. The Sequence asserts this exists -> remove -> create ordering,
        // not just the per-method call counts.
        let mut seq = Sequence::new();
        self.mock_fs_provider
            .expect_exists()
            .with(eq(expected_package_dir.clone()))
            .once()
            .in_sequence(&mut seq)
            .returning(move |_| removes_existing);
        if removes_existing {
            self.mock_fs_provider
                .expect_remove_dir_all()
                .with(eq(expected_package_dir.clone()))
                .once()
                .in_sequence(&mut seq)
                .returning(move |_| Ok(()));
        }
        self.mock_fs_provider
            .expect_create_dir()
            .with(eq(expected_package_dir))
            .once()
            .in_sequence(&mut seq)
            .returning(move |_| Ok(()));
        self
    }

    fn expect_cargo_build(mut self, cwd: &Path, override_output: Option<Output>) -> Self {
        // cargo build of the package(s) selected by cargo
        let manifest_path = cwd
            .join("Cargo.toml")
            .to_string_lossy()
            .trim_start_matches("\\\\?\\")
            .to_string();
        let mut expected_cargo_build_args: Vec<String> = vec![
            "build",
            "--message-format=json-render-diagnostics",
            "--manifest-path",
            &manifest_path,
        ]
        .into_iter()
        .map(ToString::to_string)
        .collect();
        if self.workspace {
            expected_cargo_build_args.push("--workspace".to_string());
        }
        if let Some(profile) = self.profile {
            expected_cargo_build_args.push("--profile".to_string());
            expected_cargo_build_args.push(profile.to_string());
        }

        if let Some(target_arch) = self.target_arch {
            expected_cargo_build_args.push("--target".to_string());
            expected_cargo_build_args.push(to_target_triple(target_arch));
        }

        if self.locked {
            expected_cargo_build_args.push("--locked".to_string());
        }

        expected_cargo_build_args.push("-v".to_string());
        let expected_output = override_output.unwrap_or_else(|| Output {
            status: ExitStatus::default(),
            stdout: vec![],
            stderr: vec![],
        });
        let expected_cargo = std::env::var_os("CARGO").expect("run tests through cargo test");
        let expected_working_dir = cwd.to_path_buf();
        self.mock_run_command
            .expect_run()
            .withf(move |command, args, _env, working_dir| {
                command.as_ref() == Path::new(&expected_cargo)
                    && *working_dir == Some(expected_working_dir.as_path())
                    && args == expected_cargo_build_args
            })
            .once()
            .returning(move |_, _, _, _| Ok(expected_output.clone()));
        self
    }

    fn expect_read_driver_binary(
        self,
        driver_name: &str,
        cwd: &Path,
        built_arch: CpuArchitecture,
    ) -> Self {
        let machine = match built_arch {
            CpuArchitecture::Amd64 => IMAGE_FILE_MACHINE_AMD64,
            CpuArchitecture::Arm64 => IMAGE_FILE_MACHINE_ARM64,
        };
        // MZ signature, e_lfanew, PE signature and Machine
        self.expect_read_driver_binary_returning(
            driver_name,
            cwd,
            4,
            read_from_image(pe_image(machine)),
        )
    }

    fn expect_read_driver_binary_returning(
        mut self,
        driver_name: &str,
        cwd: &Path,
        times: usize,
        result: impl Fn(&Path, &mut [u8], u64) -> Result<(), FileError> + Send + 'static,
    ) -> Self {
        let expected_driver_binary = self
            .setup_target_dir(cwd)
            .join(format!("{}.dll", driver_name.replace('-', "_")));
        self.mock_fs_provider
            .expect_seek_read_exact()
            .withf(move |path, _, _| path == expected_driver_binary)
            .times(times)
            .returning(result);
        self
    }

    fn expect_inx_file_exists(
        mut self,
        driver_name: &str,
        driver_dir: &Path,
        does_exist: bool,
    ) -> Self {
        let expected_driver_name_underscored = driver_name.replace('-', "_");
        let expected_inx_file_path =
            driver_dir.join(format!("{expected_driver_name_underscored}.inx"));
        self.mock_fs_provider
            .expect_exists()
            .with(eq(expected_inx_file_path))
            .once()
            .returning(move |_| does_exist);
        self
    }

    fn expect_rename_driver_binary_dll_to_sys(
        mut self,
        driver_name: &str,
        driver_dir: &Path,
    ) -> Self {
        let expected_driver_name_underscored = driver_name.replace('-', "_");
        let expected_target_dir = self.setup_target_dir(driver_dir);
        let expected_src_driver_dll_path =
            expected_target_dir.join(format!("{expected_driver_name_underscored}.dll"));
        let expected_src_driver_sys_path =
            expected_target_dir.join(format!("{expected_driver_name_underscored}.sys"));
        self.mock_fs_provider
            .expect_rename()
            .with(
                eq(expected_src_driver_dll_path),
                eq(expected_src_driver_sys_path),
            )
            .once()
            .returning(|_, _| Ok(()));
        self
    }

    fn expect_copy_driver_binary_sys_to_package_folder(
        mut self,
        driver_name: &str,
        driver_dir: &Path,
        is_success: bool,
    ) -> Self {
        let expected_driver_name_underscored = driver_name.replace('-', "_");
        let expected_target_dir = self.setup_target_dir(driver_dir);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{expected_driver_name_underscored}_package"));
        let mock_non_zero_bytes_copied_size = 1000u64;

        let expected_src_driver_sys_path =
            expected_target_dir.join(format!("{expected_driver_name_underscored}.sys"));
        let expected_dest_driver_binary_path =
            expected_final_package_dir_path.join(format!("{expected_driver_name_underscored}.sys"));
        let expected_src_driver_binary_path = expected_src_driver_sys_path;
        self.mock_fs_provider
            .expect_copy()
            .with(
                eq(expected_src_driver_binary_path.clone()),
                eq(expected_dest_driver_binary_path.clone()),
            )
            .once()
            .returning(move |_, _| {
                if is_success {
                    Ok(mock_non_zero_bytes_copied_size)
                } else {
                    Err(FileError::CopyError(
                        expected_src_driver_binary_path.clone(),
                        expected_dest_driver_binary_path.clone(),
                        std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "copy error"),
                    ))
                }
            });
        self
    }

    fn expect_copy_pdb_file_to_package_folder(
        mut self,
        driver_name: &str,
        driver_dir: &Path,
        is_success: bool,
    ) -> Self {
        let expected_driver_name_underscored = driver_name.replace('-', "_");
        let expected_target_dir = self.setup_target_dir(driver_dir);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{expected_driver_name_underscored}_package"));
        let mock_non_zero_bytes_copied_size = 1000u64;

        // copy pdb file to package directory
        let expected_src_driver_pdb_path =
            expected_target_dir.join(format!("{expected_driver_name_underscored}.pdb"));
        let expected_dest_driver_pdb_path =
            expected_final_package_dir_path.join(format!("{expected_driver_name_underscored}.pdb"));
        self.mock_fs_provider
            .expect_copy()
            .with(
                eq(expected_src_driver_pdb_path.clone()),
                eq(expected_dest_driver_pdb_path.clone()),
            )
            .once()
            .returning(move |_, _| {
                if is_success {
                    Ok(mock_non_zero_bytes_copied_size)
                } else {
                    Err(FileError::CopyError(
                        expected_src_driver_pdb_path.clone(),
                        expected_dest_driver_pdb_path.clone(),
                        std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "copy error"),
                    ))
                }
            });
        self
    }

    fn expect_copy_inx_file_to_package_folder(
        mut self,
        driver_name: &str,
        driver_dir: &Path,
        is_success: bool,
        workspace_root_dir: &Path,
    ) -> Self {
        let expected_driver_name_underscored = driver_name.replace('-', "_");
        let expected_target_dir = self.setup_target_dir(workspace_root_dir);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{expected_driver_name_underscored}_package"));
        let mock_non_zero_bytes_copied_size = 1000u64;

        // copy inx file to package directory
        let expected_src_driver_inx_path =
            driver_dir.join(format!("{expected_driver_name_underscored}.inx"));
        let expected_dest_driver_inf_path =
            expected_final_package_dir_path.join(format!("{expected_driver_name_underscored}.inf"));
        self.mock_fs_provider
            .expect_copy()
            .with(
                eq(expected_src_driver_inx_path.clone()),
                eq(expected_dest_driver_inf_path.clone()),
            )
            .once()
            .returning(move |_, _| {
                if is_success {
                    Ok(mock_non_zero_bytes_copied_size)
                } else {
                    Err(FileError::CopyError(
                        expected_src_driver_inx_path.clone(),
                        expected_dest_driver_inf_path.clone(),
                        std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "copy error"),
                    ))
                }
            });
        self
    }

    fn expect_copy_self_signed_cert_file_to_package_folder(
        mut self,
        driver_name: &str,
        driver_dir: &Path,
        is_success: bool,
    ) -> Self {
        let expected_driver_name_underscored = driver_name.replace('-', "_");
        let expected_target_dir = self.setup_target_dir(driver_dir);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{expected_driver_name_underscored}_package"));
        let mock_non_zero_bytes_copied_size = 1000u64;

        // copy self signed certificate to package directory
        let expected_src_cert_file_path = expected_target_dir.join("WDRLocalTestCert.cer");
        let expected_dest_driver_cert_path =
            expected_final_package_dir_path.join("WDRLocalTestCert.cer");
        self.mock_fs_provider
            .expect_copy()
            .with(
                eq(expected_src_cert_file_path.clone()),
                eq(expected_dest_driver_cert_path.clone()),
            )
            .once()
            .returning(move |_, _| {
                if is_success {
                    Ok(mock_non_zero_bytes_copied_size)
                } else {
                    Err(FileError::CopyError(
                        expected_src_cert_file_path.clone(),
                        expected_dest_driver_cert_path.clone(),
                        std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "copy error"),
                    ))
                }
            });
        self
    }

    fn expect_stampinf(
        mut self,
        driver_name: &str,
        driver_dir: &Path,
        target_arch: CpuArchitecture,
        override_output: Option<Output>,
    ) -> Self {
        // Run stampinf command
        let expected_driver_name_underscored = driver_name.replace('-', "_");
        let expected_target_dir = self.setup_target_dir(driver_dir);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{expected_driver_name_underscored}_package"));
        let expected_dest_driver_inf_path =
            expected_final_package_dir_path.join(format!("{expected_driver_name_underscored}.inf"));

        let expected_stampinf_command: &'static str = "stampinf";
        let wdk_metadata = Wdk::try_from(
            self.cargo_metadata
                .as_ref()
                .expect("cargo metadata must be available"),
        )
        .expect("Wdk metadata must be available");

        if let DriverConfig::Kmdf(kmdf_config) = wdk_metadata.driver_model {
            let expected_cat_file_name = format!("{expected_driver_name_underscored}.cat");
            let expected_stampinf_args: Vec<String> = vec![
                "-f".to_string(),
                expected_dest_driver_inf_path.to_string_lossy().to_string(),
                "-d".to_string(),
                "*".to_string(),
                "-a".to_string(),
                target_arch.to_string(),
                "-c".to_string(),
                expected_cat_file_name,
                "-v".to_string(),
                "*".to_string(),
                "-k".to_string(),
                format!(
                    "{}.{}",
                    kmdf_config.kmdf_version_major, kmdf_config.target_kmdf_version_minor
                ),
            ];

            self.mock_run_command
                .expect_run()
                .withf(
                    move |command: &dyn AsRef<Path>,
                          args: &[&str],
                          _env_vars: &Option<&HashMap<&str, &str>>,
                          _working_dir: &Option<&Path>|
                          -> bool {
                        let command = command.as_ref();
                        println!("command: {command:?}, args: {args:?}");
                        println!(
                            "expected_command: {expected_stampinf_command}, expected_args: \
                             {expected_stampinf_args:?}"
                        );
                        command == Path::new(expected_stampinf_command)
                            && args == expected_stampinf_args
                    },
                )
                .once()
                .returning(move |_, _, _, _| match override_output.clone() {
                    Some(output) => match output.status.code() {
                        Some(0) => Ok(Output {
                            status: ExitStatus::from_raw(0),
                            stdout: vec![],
                            stderr: vec![],
                        }),
                        _ => Err(CommandError::from_output("stampinf", &[], &output)),
                    },
                    None => Ok(Output {
                        status: ExitStatus::default(),
                        stdout: vec![],
                        stderr: vec![],
                    }),
                });
        }
        self
    }

    fn expect_inf2cat(
        mut self,
        driver_name: &str,
        driver_dir: &Path,
        target_arch: CpuArchitecture,
        override_output: Option<Output>,
    ) -> Self {
        // Run inf2cat command
        let expected_driver_name_underscored = driver_name.replace('-', "_");
        let expected_target_dir = self.setup_target_dir(driver_dir);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{expected_driver_name_underscored}_package"));

        let expected_inf2cat_command: &'static str = "inf2cat";

        let expected_inf2cat_arg = match target_arch {
            CpuArchitecture::Amd64 => "10_x64",
            CpuArchitecture::Arm64 => "Server10_arm64",
        };
        let expected_inf2cat_args: Vec<String> = vec![
            format!(
                "/driver:{}",
                expected_final_package_dir_path.to_string_lossy()
            ),
            format!("/os:{}", expected_inf2cat_arg),
            "/uselocaltime".to_string(),
        ];

        self.mock_run_command
            .expect_run()
            .withf(
                move |command: &dyn AsRef<Path>,
                      args: &[&str],
                      _env_vars: &Option<&HashMap<&str, &str>>,
                      _working_dir: &Option<&Path>|
                      -> bool {
                    let command = command.as_ref();
                    println!("command: {command:?}, args: {args:?}");
                    println!(
                        "expected_command: {expected_inf2cat_command}, expected_args: \
                         {expected_inf2cat_args:?}"
                    );
                    command == Path::new(expected_inf2cat_command) && args == expected_inf2cat_args
                },
            )
            .once()
            .returning(move |_, _, _, _| match override_output.clone() {
                Some(output) => match output.status.code() {
                    Some(0) => Ok(Output {
                        status: ExitStatus::from_raw(0),
                        stdout: vec![],
                        stderr: vec![],
                    }),
                    _ => Err(CommandError::from_output("inf2cat", &[], &output)),
                },
                None => Ok(Output {
                    status: ExitStatus::default(),
                    stdout: vec![],
                    stderr: vec![],
                }),
            });
        self
    }

    fn expect_certmgr_exists_check(mut self, override_output: Option<Output>) -> Self {
        // check for cert in cert store using certmgr
        let expected_certmgr_command: &'static str = "certmgr.exe";
        let expected_certmgr_args: Vec<String> =
            vec!["-s".to_string(), "WDRTestCertStore".to_string()];
        self.mock_run_command
            .expect_run()
            .withf(
                move |command: &dyn AsRef<Path>,
                      args: &[&str],
                      _env_vars: &Option<&HashMap<&str, &str>>,
                      _working_dir: &Option<&Path>|
                      -> bool {
                    command.as_ref() == Path::new(expected_certmgr_command)
                        && args == expected_certmgr_args
                },
            )
            .returning(move |_, _, _, _| match override_output.clone() {
                Some(output) => match output.status.code() {
                    Some(0) => Ok(Output {
                        status: ExitStatus::from_raw(0),
                        stdout: output.stdout,
                        stderr: output.stderr,
                    }),
                    _ => Err(CommandError::from_output("certmgr", &[], &output)),
                },
                None => Ok(Output {
                    status: ExitStatus::default(),
                    stdout: vec![],
                    stderr: vec![],
                }),
            });
        self
    }

    fn expect_certmgr_create_cert_from_store(
        mut self,
        driver_dir: &Path,
        override_output: Option<Output>,
    ) -> Self {
        // create cert from store using certmgr
        let expected_target_dir = self.setup_target_dir(driver_dir);
        let expected_self_signed_cert_file_path = expected_target_dir.join("WDRLocalTestCert.cer");

        let expected_certmgr_command: &'static str = "certmgr.exe";
        let expected_certmgr_args: Vec<String> = vec![
            "-put".to_string(),
            "-s".to_string(),
            "WDRTestCertStore".to_string(),
            "-c".to_string(),
            "-n".to_string(),
            "WDRLocalTestCert".to_string(),
            expected_self_signed_cert_file_path
                .to_string_lossy()
                .to_string(),
        ];
        self.mock_run_command
            .expect_run()
            .withf(
                move |command: &dyn AsRef<Path>,
                      args: &[&str],
                      _env_vars: &Option<&HashMap<&str, &str>>,
                      _working_dir: &Option<&Path>|
                      -> bool {
                    command.as_ref() == Path::new(expected_certmgr_command)
                        && args == expected_certmgr_args
                },
            )
            .once()
            .returning(move |_, _, _, _| match override_output.clone() {
                Some(output) => match output.status.code() {
                    Some(0) => Ok(Output {
                        status: ExitStatus::from_raw(0),
                        stdout: vec![],
                        stderr: vec![],
                    }),
                    _ => Err(CommandError::from_output("certmgr", &[], &output)),
                },
                None => Ok(Output {
                    status: ExitStatus::default(),
                    stdout: vec![],
                    stderr: vec![],
                }),
            });
        self
    }

    fn expect_makecert(mut self, driver_dir: &Path, override_output: Option<Output>) -> Self {
        // create self signed certificate using makecert
        let expected_target_dir = self.setup_target_dir(driver_dir);
        let expected_makecert_command: &'static str = "makecert";
        let expected_src_driver_cert_path = expected_target_dir.join("WDRLocalTestCert.cer");
        let expected_makecert_args: Vec<String> = vec![
            "-r".to_string(),
            "-pe".to_string(),
            "-a".to_string(),
            "SHA256".to_string(),
            "-eku".to_string(),
            "1.3.6.1.5.5.7.3.3".to_string(),
            "-ss".to_string(),
            "WDRTestCertStore".to_string(),
            "-n".to_string(),
            "CN=WDRLocalTestCert".to_string(),
            expected_src_driver_cert_path.to_string_lossy().to_string(),
        ];

        self.mock_run_command
            .expect_run()
            .withf(
                move |command: &dyn AsRef<Path>,
                      args: &[&str],
                      _env_vars: &Option<&HashMap<&str, &str>>,
                      _working_dir: &Option<&Path>|
                      -> bool {
                    command.as_ref() == Path::new(expected_makecert_command)
                        && args == expected_makecert_args
                },
            )
            .once()
            .returning(move |_, _, _, _| match override_output.clone() {
                Some(output) => match output.status.code() {
                    Some(0) => Ok(Output {
                        status: ExitStatus::from_raw(0),
                        stdout: vec![],
                        stderr: vec![],
                    }),
                    _ => Err(CommandError::from_output("makecert", &[], &output)),
                },
                None => Ok(Output {
                    status: ExitStatus::default(),
                    stdout: vec![],
                    stderr: vec![],
                }),
            });
        self
    }

    fn expect_signtool_sign_driver_binary_sys_file(
        mut self,
        driver_name: &str,
        driver_dir: &Path,
        override_output: Option<Output>,
    ) -> Self {
        let expected_driver_name_underscored = driver_name.replace('-', "_");
        let expected_target_dir = self.setup_target_dir(driver_dir);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{expected_driver_name_underscored}_package"));
        let expected_signtool_command: &'static str = "signtool";

        // sign driver binary using signtool
        let expected_dest_driver_binary_path =
            expected_final_package_dir_path.join(format!("{expected_driver_name_underscored}.sys"));
        let expected_signtool_args: Vec<String> = vec![
            "sign".to_string(),
            "/v".to_string(),
            "/s".to_string(),
            "WDRTestCertStore".to_string(),
            "/n".to_string(),
            "WDRLocalTestCert".to_string(),
            "/t".to_string(),
            "http://timestamp.digicert.com".to_string(),
            "/fd".to_string(),
            "SHA256".to_string(),
            expected_dest_driver_binary_path
                .to_string_lossy()
                .to_string(),
        ];

        self.mock_run_command
            .expect_run_with_redaction()
            .withf(
                move |command: &dyn AsRef<Path>,
                      args: &[&str],
                      _redaction_indices: &[usize],
                      _env_vars: &Option<&HashMap<&str, &str>>,
                      _working_dir: &Option<&Path>|
                      -> bool {
                    command.as_ref() == Path::new(expected_signtool_command)
                        && args == expected_signtool_args
                },
            )
            .once()
            .returning(move |_, _, _, _, _| match override_output.clone() {
                Some(output) => match output.status.code() {
                    Some(0) => Ok(Output {
                        status: ExitStatus::from_raw(0),
                        stdout: vec![],
                        stderr: vec![],
                    }),
                    _ => Err(CommandError::from_output("signtool", &[], &output)),
                },
                None => Ok(Output {
                    status: ExitStatus::default(),
                    stdout: vec![],
                    stderr: vec![],
                }),
            });
        self
    }

    fn expect_signtool_sign_cat_file(
        mut self,
        driver_name: &str,
        driver_dir: &Path,
        override_output: Option<Output>,
    ) -> Self {
        let expected_driver_name_underscored = driver_name.replace('-', "_");
        let expected_target_dir = self.setup_target_dir(driver_dir);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{expected_driver_name_underscored}_package"));
        let expected_signtool_command: &'static str = "signtool";

        // sign driver cat file using signtool
        let expected_dest_driver_cat_file_path =
            expected_final_package_dir_path.join(format!("{expected_driver_name_underscored}.cat"));
        let expected_signtool_args: Vec<String> = vec![
            "sign".to_string(),
            "/v".to_string(),
            "/s".to_string(),
            "WDRTestCertStore".to_string(),
            "/n".to_string(),
            "WDRLocalTestCert".to_string(),
            "/t".to_string(),
            "http://timestamp.digicert.com".to_string(),
            "/fd".to_string(),
            "SHA256".to_string(),
            expected_dest_driver_cat_file_path
                .to_string_lossy()
                .to_string(),
        ];

        self.mock_run_command
            .expect_run_with_redaction()
            .withf(
                move |command: &dyn AsRef<Path>,
                      args: &[&str],
                      _redaction_indices: &[usize],
                      _env_vars: &Option<&HashMap<&str, &str>>,
                      _working_dir: &Option<&Path>|
                      -> bool {
                    command.as_ref() == Path::new(expected_signtool_command)
                        && args == expected_signtool_args
                },
            )
            .once()
            .returning(move |_, _, _, _, _| match override_output.clone() {
                Some(output) => match output.status.code() {
                    Some(0) => Ok(Output {
                        status: ExitStatus::from_raw(0),
                        stdout: vec![],
                        stderr: vec![],
                    }),
                    _ => Err(CommandError::from_output("signtool", &[], &output)),
                },
                None => Ok(Output {
                    status: ExitStatus::default(),
                    stdout: vec![],
                    stderr: vec![],
                }),
            });
        self
    }

    fn expect_signtool_verify_driver_binary_sys_file(
        mut self,
        driver_name: &str,
        driver_dir: &Path,
        override_output: Option<Output>,
    ) -> Self {
        let expected_driver_name_underscored = driver_name.replace('-', "_");
        let expected_target_dir = self.setup_target_dir(driver_dir);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{expected_driver_name_underscored}_package"));
        let expected_signtool_command: &'static str = "signtool";

        // verify signed driver binary using signtool
        let expected_dest_driver_binary_path =
            expected_final_package_dir_path.join(format!("{expected_driver_name_underscored}.sys"));
        let expected_signtool_verify_args: Vec<String> = vec![
            "verify".to_string(),
            "/v".to_string(),
            "/pa".to_string(),
            expected_dest_driver_binary_path
                .to_string_lossy()
                .to_string(),
        ];
        self.mock_run_command
            .expect_run()
            .withf(
                move |command: &dyn AsRef<Path>,
                      args: &[&str],
                      _env_vars: &Option<&HashMap<&str, &str>>,
                      _working_dir: &Option<&Path>|
                      -> bool {
                    command.as_ref() == Path::new(expected_signtool_command)
                        && args == expected_signtool_verify_args
                },
            )
            .once()
            .returning(move |_, _, _, _| match override_output.clone() {
                Some(output) => match output.status.code() {
                    Some(0) => Ok(Output {
                        status: ExitStatus::from_raw(0),
                        stdout: vec![],
                        stderr: vec![],
                    }),
                    _ => Err(CommandError::from_output("signtool", &[], &output)),
                },
                None => Ok(Output {
                    status: ExitStatus::default(),
                    stdout: vec![],
                    stderr: vec![],
                }),
            });
        self
    }

    fn expect_signtool_verify_cat_file(
        mut self,
        driver_name: &str,
        driver_dir: &Path,
        override_output: Option<Output>,
    ) -> Self {
        let expected_driver_name_underscored = driver_name.replace('-', "_");
        let expected_target_dir = self.setup_target_dir(driver_dir);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{expected_driver_name_underscored}_package"));
        let expected_signtool_command: &'static str = "signtool";

        // verify signed driver cat file using signtool
        let expected_dest_driver_cat_file_path =
            expected_final_package_dir_path.join(format!("{expected_driver_name_underscored}.cat"));
        let expected_signtool_verify_args: Vec<String> = vec![
            "verify".to_string(),
            "/v".to_string(),
            "/pa".to_string(),
            expected_dest_driver_cat_file_path
                .to_string_lossy()
                .to_string(),
        ];
        self.mock_run_command
            .expect_run()
            .withf(
                move |command: &dyn AsRef<Path>,
                      args: &[&str],
                      _env_vars: &Option<&HashMap<&str, &str>>,
                      _working_dir: &Option<&Path>|
                      -> bool {
                    command.as_ref() == Path::new(expected_signtool_command)
                        && args == expected_signtool_verify_args
                },
            )
            .once()
            .returning(move |_, _, _, _| match override_output.clone() {
                Some(output) => match output.status.code() {
                    Some(0) => Ok(Output {
                        status: ExitStatus::from_raw(0),
                        stdout: vec![],
                        stderr: vec![],
                    }),
                    _ => Err(CommandError::from_output("stampinf", &[], &output)),
                },
                None => Ok(Output {
                    status: ExitStatus::default(),
                    stdout: vec![],
                    stderr: vec![],
                }),
            });
        self
    }

    fn expect_detect_wdk_build_number(mut self, expected_wdk_build_number: u32) -> Self {
        self.mock_wdk_build_provider
            .expect_detect_wdk_build_number()
            .once()
            .returning(move || Ok(expected_wdk_build_number));
        self
    }

    fn expect_infverif(
        mut self,
        driver_name: &str,
        driver_dir: &Path,
        expected_sample_flag: Option<&str>,
        override_output: Option<Output>,
    ) -> Self {
        let mut expected_infverif_args = vec!["/v".to_string(), "/u".to_string()];
        if self.sample_class
            && let Some(sample_flag) = expected_sample_flag
        {
            expected_infverif_args.push(sample_flag.to_string());
        }
        let expected_infverif_command: &'static str = "infverif";
        let expected_driver_name_underscored = driver_name.replace('-', "_");
        let expected_target_dir = self.setup_target_dir(driver_dir);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{expected_driver_name_underscored}_package"));
        let expected_dest_inf_file_path =
            expected_final_package_dir_path.join(format!("{expected_driver_name_underscored}.inf"));
        expected_infverif_args.push(expected_dest_inf_file_path.to_string_lossy().to_string());

        self.mock_run_command
            .expect_run()
            .withf(
                move |command: &dyn AsRef<Path>,
                      args: &[&str],
                      _env_vars: &Option<&HashMap<&str, &str>>,
                      _working_dir: &Option<&Path>|
                      -> bool {
                    command.as_ref() == Path::new(expected_infverif_command)
                        && args == expected_infverif_args
                },
            )
            .once()
            .returning(move |_, _, _, _| match override_output.clone() {
                Some(output) => match output.status.code() {
                    Some(0) => Ok(Output {
                        status: ExitStatus::from_raw(0),
                        stdout: vec![],
                        stderr: vec![],
                    }),
                    _ => Err(CommandError::from_output("infverif", &[], &output)),
                },
                None => Ok(Output {
                    status: ExitStatus::default(),
                    stdout: vec![],
                    stderr: vec![],
                }),
            });
        self
    }

    const fn mock_wdk_build_provider(&self) -> &WdkBuild {
        &self.mock_wdk_build_provider
    }

    const fn mock_run_command(&self) -> &CommandExec {
        &self.mock_run_command
    }

    const fn mock_fs_provider(&self) -> &Fs {
        &self.mock_fs_provider
    }

    const fn mock_metadata_provider(&self) -> &MetadataProvider {
        &self.mock_metadata_provider
    }
}

fn invalid_driver_cargo_toml() -> String {
    r#"
        {
            "packages": [
                {
                    "name": "sample-driver",
                    "version": "0.0.1",
                    "id": "path+file:///C:/tmp/sample-driver#0.0.1",
                    "license": "MIT OR Apache-2.0",
                    "license_file": null,
                    "description": null,
                    "source": null,
                    "dependencies": [],
                    "targets": [
                        {
                            "kind": [
                                "cdylib"
                            ],
                            "crate_types": [
                                "cdylib"
                            ],
                            "name": "sample_driver",
                            "src_path": "C:\\tmp\\sample-driver\\src\\lib.rs",
                            "edition": "2021",
                            "doc": true,
                            "doctest": false,
                            "test": false
                        },
                        {
                            "kind": [
                                "custom-build"
                            ],
                            "crate_types": [
                                "bin"
                            ],
                            "name": "build-script-build",
                            "src_path": "C:\\tmp\\sample-driver\\build.rs",
                            "edition": "2021",
                            "doc": false,
                            "doctest": false,
                            "test": false
                        }
                    ],
                    "features": {
                        "default": [],
                        "nightly": [
                            "wdk/nightly",
                            "wdk-sys/nightly"
                        ]
                    },
                    "manifest_path": "C:\\tmp\\sample-driver\\Cargo.toml",
                    "metadata": {
                        "wdk": {}
                    },
                    "publish": [],
                    "authors": [],
                    "categories": [],
                    "keywords": [],
                    "readme": null,
                    "repository": null,
                    "homepage": null,
                    "documentation": null,
                    "edition": "2021",
                    "links": null,
                    "default_run": null,
                    "rust_version": null
                }
            ],
            "workspace_members": [
                "path+file:///C:/tmp/sample-driver#0.0.1"
            ],
            "workspace_default_members": [
                "path+file:///C:/tmp/sample-driver#0.0.1"
            ],
            "target_directory": "C:\\tmp\\sample-driver\\target",
            "version": 1,
            "workspace_root": "C:\\tmp\\sample-driver",
            "metadata": {
                "wdk": {
                    "driver-model": {
                        "driver-type": "KMDF"
                    }
                }
            }
        }
    "#
    .to_string()
}

#[derive(Clone)]
struct TestMetadataPackage(String);
#[derive(Clone)]
struct TestMetadataWorkspaceMemberId(String);
#[derive(Clone)]
struct TestWdkMetadata(String);

fn get_cargo_metadata(
    root_dir: &Path,
    package_list: Vec<TestMetadataPackage>,
    workspace_member_list: &[TestMetadataWorkspaceMemberId],
    workspace_default_member_list: &[TestMetadataWorkspaceMemberId],
    metadata: Option<TestWdkMetadata>,
) -> String {
    let metadata_section = match metadata {
        Some(metadata) => metadata.0,
        None => String::from("null"),
    };
    format!(
        r#"
    {{
        "target_directory": "{}",
        "workspace_root": "{}",
        "packages": [
            {}
            ],
        "workspace_members": [{}],
        "workspace_default_members": [{}],
        "metadata": {},
        "version": 1
    }}"#,
        root_dir.join("target").to_string_lossy().escape_default(),
        root_dir.to_string_lossy().escape_default(),
        package_list
            .into_iter()
            .map(|p| p.0)
            .collect::<Vec<String>>()
            .join(", "),
        // Require quotes around each member
        workspace_member_list
            .iter()
            .map(|s| format!("\"{}\"", s.0))
            .collect::<Vec<String>>()
            .join(", "),
        workspace_default_member_list
            .iter()
            .map(|s| format!("\"{}\"", s.0))
            .collect::<Vec<String>>()
            .join(", "),
        metadata_section
    )
}

fn get_cargo_metadata_package(
    root_dir: &Path,
    default_package_name: &str,
    default_package_version: &str,
    metadata: Option<&TestWdkMetadata>,
) -> (TestMetadataWorkspaceMemberId, TestMetadataPackage) {
    let normalized_root = root_dir.to_string_lossy().replace('\\', "/");
    let normalized_root = normalized_root.trim_start_matches("//?/");
    let package_id =
        format!("path+file:///{normalized_root}#{default_package_name}@{default_package_version}");
    let (metadata_section, has_metadata) = metadata.map_or_else(
        || (String::from("null"), false),
        |metadata| ((metadata.0).clone(), true),
    );
    let (target_kind, crate_type, source_file) = if has_metadata {
        ("cdylib", "cdylib", "main.rs")
    } else {
        ("lib", "lib", "lib.rs")
    };
    let manifest_path = root_dir
        .join("Cargo.toml")
        .to_string_lossy()
        .escape_default()
        .to_string();
    let source_path = root_dir
        .join("src")
        .join(source_file)
        .to_string_lossy()
        .escape_default()
        .to_string();
    (
        TestMetadataWorkspaceMemberId(package_id.clone()),
        TestMetadataPackage(format!(
            r#"
            {{
            "name": "{default_package_name}",
            "version": "{default_package_version}",
            "id": "{package_id}",
            "dependencies": [],
            "targets": [
                {{
                    "kind": [
                        "{target_kind}"
                    ],
                    "crate_types": [
                        "{crate_type}"
                    ],
                    "name": "{default_package_name}",
                    "src_path": "{source_path}",
                    "edition": "2021",
                    "doc": true,
                    "doctest": false,
                    "test": true
                }}
            ],
            "features": {{}},
            "manifest_path": "{manifest_path}",
            "authors": [],
            "categories": [],
            "keywords": [],
            "edition": "2021",
            "metadata": {metadata_section}
        }}
        "#
        )),
    )
}

fn get_cargo_metadata_wdk_metadata(
    driver_type: &str,
    kmdf_version_major: u8,
    target_kmdf_version_minor: u8,
) -> TestWdkMetadata {
    TestWdkMetadata(format!(
        r#"
        {{
            "wdk": {{
                "driver-model": {{
                    "driver-type": "{}",
                    "{}-version-major": {},
                    "target-{}-version-minor": {}
                }}
            }}
        }}
    "#,
        driver_type,
        driver_type.to_ascii_lowercase(),
        kmdf_version_major,
        driver_type.to_ascii_lowercase(),
        target_kmdf_version_minor
    ))
}

/// Creates a valid cargo compiler-artifact JSON message for testing.
/// This simulates the JSON output that `cargo build --message-format=json`
/// produces.
fn create_cargo_build_output_json(
    package_name: &str,
    package_version: &str,
    cwd: &Path,
    target_triple: Option<&str>,
    profile: Option<Profile>,
) -> Output {
    create_cargo_build_output_json_with_manifest(
        package_name,
        package_version,
        cwd,
        &cwd.join("Cargo.toml"),
        target_triple,
        profile,
        true,
    )
}

/// Concatenates the stdout of multiple cargo build outputs, as produced by a
/// single cargo build of multiple packages.
fn combine_cargo_build_outputs(outputs: &[Output]) -> Output {
    Output {
        status: ExitStatus::default(),
        stdout: outputs.iter().flat_map(|o| o.stdout.clone()).collect(),
        stderr: vec![],
    }
}

fn strip_windows_extended_prefix(path: &Path) -> String {
    let path_str = path.to_string_lossy();
    let Some(without_prefix) = path_str.strip_prefix(r"\\?\") else {
        return path_str.into_owned();
    };

    // Handle UNC paths
    without_prefix.strip_prefix("UNC\\").map_or_else(
        || without_prefix.to_string(),
        |unc_rest| format!(r"\\{unc_rest}"),
    )
}

fn create_cargo_build_output_json_with_manifest(
    package_name: &str,
    package_version: &str,
    workspace_root: &Path,
    manifest_path: &Path,
    target_triple: Option<&str>,
    profile: Option<Profile>,
    is_driver: bool,
) -> Output {
    let normalized_name = package_name.replace('-', "_");

    // Determine profile directory name
    let profile_dir = match profile {
        Some(Profile::Release) => "release",
        _ => "debug",
    };

    // For non-driver projects, use "lib" instead of "cdylib" to ensure
    // BuildTask returns DllNotFound
    let (kind, crate_types, file_ext) = if is_driver {
        ("cdylib", "cdylib", "dll")
    } else {
        ("lib", "lib", "rlib")
    };

    // Build the artifact path based on target_triple and profile
    let mut artifact_path = workspace_root.join("target");
    if let Some(target) = target_triple {
        artifact_path = artifact_path.join(target);
    }
    artifact_path = artifact_path
        .join(profile_dir)
        .join(format!("{normalized_name}.{file_ext}"));

    let package_dir = manifest_path.parent().unwrap_or(workspace_root);
    let package_dir = strip_windows_extended_prefix(package_dir).replace('\\', "/");
    let package_id = format!("path+file:///{package_dir}#{package_name}@{package_version}");
    let manifest_path = strip_windows_extended_prefix(manifest_path);
    let artifact_path = strip_windows_extended_prefix(&artifact_path);

    let pdb_path = Path::new(&artifact_path)
        .with_extension("pdb")
        .to_string_lossy()
        .to_string();
    let filenames = vec![artifact_path, pdb_path];

    let artifact_json = serde_json::json!({
        "reason": "compiler-artifact",
        "package_id": package_id,
        "manifest_path": manifest_path,
        "target": {
            "kind": [kind],
            "crate_types": [crate_types],
            "name": normalized_name,
            "src_path": "src/lib.rs",
            "edition": "2021",
            "doc": false,
            "doctest": false,
            "test": false
        },
        "profile": {
            "opt_level": "0",
            "debuginfo": 2,
            "debug_assertions": true,
            "overflow_checks": true,
            "test": false
        },
        "features": [],
        "filenames": filenames,
        "executable": null,
        "fresh": false
    });

    Output {
        status: ExitStatus::default(),
        stdout: format!("{artifact_json}\n").into_bytes(),
        stderr: vec![],
    }
}

mod get_driver_binary_from_artifact {
    use std::{io, path::PathBuf};

    use cargo_metadata::{Artifact, Message};

    use super::{BuildAction, BuildActionError};

    fn artifact(package_name: &str, package_version: &str) -> Artifact {
        let workspace_root_dir = PathBuf::from(r"C:\tmp\sample-kmdf");
        let output = super::create_cargo_build_output_json_with_manifest(
            package_name,
            package_version,
            &workspace_root_dir,
            &workspace_root_dir.join("Cargo.toml"),
            None,
            None,
            true,
        );
        Message::parse_stream(io::Cursor::new(output.stdout))
            .find_map(|message| match message {
                Ok(Message::CompilerArtifact(artifact)) => Some(artifact),
                _ => None,
            })
            .expect("Expected a compiler-artifact message")
    }

    #[test]
    fn artifact_without_dll_fails() {
        let mut artifact = artifact("sample-kmdf", "0.0.1");
        artifact.filenames.retain(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("pdb"))
        });

        let result = BuildAction::get_driver_binary_from_artifact(&artifact);
        assert!(
            matches!(
                result,
                Err(BuildActionError::CannotDetermineDriverBinary(ref message))
                if message.contains("Could not find driver binary (DLL)")
            ),
            "Expected CannotDetermineDriverBinary no-dll error, got: {result:?}"
        );
    }
}

mod get_arch_from_pe {
    use std::{
        io,
        path::{Path, PathBuf},
    };

    use wdk_build::CpuArchitecture;

    use super::{
        BuildActionError,
        FileError,
        IMAGE_FILE_MACHINE_AMD64,
        TestBuildAction,
        pe_image,
        read_from_image,
    };

    const DRIVER_BINARY: &str = r"C:\tmp\target\debug\sample_kmdf_driver.dll";

    fn run_with(
        target_arch: Option<CpuArchitecture>,
        read: impl Fn(&Path, &mut [u8], u64) -> Result<(), FileError> + Send + 'static,
    ) -> Result<CpuArchitecture, BuildActionError> {
        let cwd = PathBuf::from(r"C:\tmp");
        let mut test_build_action = TestBuildAction::new(cwd.clone(), None, target_arch, false);
        test_build_action
            .mock_fs_provider
            .expect_seek_read_exact()
            .withf(|path, _, _| path == Path::new(DRIVER_BINARY))
            .returning(read);

        let build_action = super::initialize_build_action(
            &cwd,
            None,
            target_arch,
            true,
            false,
            &test_build_action,
        )
        .expect("Failed to init build action");
        build_action.get_arch_from_pe(Path::new(DRIVER_BINARY))
    }

    fn run(
        target_arch: Option<CpuArchitecture>,
        image: Vec<u8>,
    ) -> Result<CpuArchitecture, BuildActionError> {
        run_with(target_arch, read_from_image(image))
    }

    fn assert_not_pe(result: &Result<CpuArchitecture, BuildActionError>, expected_reason: &str) {
        assert!(
            matches!(
                result,
                Err(BuildActionError::CannotDetectTargetArch(path, reason))
                    if path == Path::new(DRIVER_BINARY) && *reason == expected_reason
            ),
            "expected CannotDetectTargetArch({expected_reason:?}), got {result:?}"
        );
    }

    #[test]
    fn mismatched_target_arch_returns_error() {
        let err = run(
            Some(CpuArchitecture::Arm64),
            pe_image(IMAGE_FILE_MACHINE_AMD64),
        )
        .expect_err("Expected TargetArchMismatch error");
        assert!(matches!(
            err,
            BuildActionError::TargetArchMismatch {
                pe_arch: CpuArchitecture::Amd64,
                target_arch: CpuArchitecture::Arm64,
                ..
            }
        ));
    }

    #[test]
    fn unsupported_machines_return_error() {
        // I386, ARMNT, ARM64EC, ARM64X, RISCV64 and UNKNOWN
        for machine in [0x014C, 0x01C4, 0xA641, 0xA64E, 0x5064, 0x0000] {
            let err = run(None, pe_image(machine)).expect_err("Expected UnsupportedArchitecture");
            assert!(
                matches!(err, BuildActionError::UnsupportedArchitecture(ref a) if *a == format!("{machine:#06x}")),
                "{err:?}"
            );
        }
    }

    #[test]
    fn malformed_headers_return_error() {
        assert_not_pe(&run(None, vec![]), "file too short for a DOS header");
        let mut short = pe_image(IMAGE_FILE_MACHINE_AMD64);
        short.truncate(0x3F);
        assert_not_pe(&run(None, short), "file too short for a DOS header");

        let mut bad_mz = pe_image(IMAGE_FILE_MACHINE_AMD64);
        bad_mz[0] = b'X';
        assert_not_pe(&run(None, bad_mz), "missing MZ signature");

        let mut bad_pe = pe_image(IMAGE_FILE_MACHINE_AMD64);
        bad_pe[0x80] = b'X';
        assert_not_pe(&run(None, bad_pe), "missing PE signature");

        for e_lfanew in [0x1000, u32::MAX] {
            let mut data = pe_image(IMAGE_FILE_MACHINE_AMD64);
            data[0x3C..0x40].copy_from_slice(&e_lfanew.to_le_bytes());
            assert_not_pe(&run(None, data), "PE header beyond end of file");
        }

        let mut no_machine = pe_image(IMAGE_FILE_MACHINE_AMD64);
        no_machine.truncate(0x85);
        assert_not_pe(&run(None, no_machine), "PE header beyond end of file");
    }

    #[test]
    fn read_error_other_than_eof_is_propagated() {
        let err = run_with(None, |path, _, _| {
            Err(FileError::ReadError(
                path.to_owned(),
                io::Error::from_raw_os_error(5),
            ))
        })
        .expect_err("Expected FileIo error");
        assert!(matches!(
            err,
            BuildActionError::FileIo(FileError::ReadError(..))
        ));
    }
}
