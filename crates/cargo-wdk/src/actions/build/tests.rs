// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
#![allow(clippy::too_many_lines)] // Package tests are longer and splitting them into sub functions can make the code less readable
#![allow(clippy::ref_option_ref)] // This is suppressed for mockall as it generates mocks with env_vars: &Option
use std::{
    collections::HashMap,
    io::Error,
    os::windows::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::{ExitStatus, Output},
    result::Result::Ok,
};

use cargo_metadata::Metadata as CargoMetadata;
use mockall::predicate::eq;
use mockall_double::double;
use wdk_build::{
    metadata::{TryFromCargoMetadataError, Wdk},
    CpuArchitecture,
    DriverConfig,
};

#[double]
use crate::providers::{
    exec::CommandExec,
    fs::Fs,
    metadata::Metadata as MetadataProvider,
    wdk_build::WdkBuild,
};
use crate::{
    actions::{
        build::{BuildAction, BuildActionError, BuildActionParams},
        to_target_triple,
        Profile,
        TargetArch,
    },
    providers::error::CommandError,
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
    let target_arch = TargetArch::Default(CpuArchitecture::Amd64);
    let verify_signature = false;
    let sample_class = false;
    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(wdk_metadata));
    let expected_certmgr_output = Output {
        status: ExitStatus::default(),
        stdout: r"==============No Certificates ==========
                            ==============No CTLs ==========
                            ==============No CRLs ==========
                            ==============================================
                            CertMgr Succeeded"
            .as_bytes()
            .to_vec(),
        stderr: vec![],
    };

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, target_arch, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_all_package_roots()
        .expect_path_canonicalization_package_manifest_path(&cwd)
        .expect_cargo_build(driver_name, &cwd, None)
        .expect_final_package_dir_exists(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true, &cwd)
        .expect_copy_map_file_to_package_folder(driver_name, &cwd, true)
        .expect_stampinf(driver_name, &cwd, None)
        .expect_inf2cat(driver_name, &cwd, None)
        .expect_self_signed_cert_file_exists(&cwd, false)
        .expect_certmgr_exists_check(Some(expected_certmgr_output))
        .expect_makecert(&cwd, None)
        .expect_copy_self_signed_cert_file_to_package_folder(driver_name, &cwd, true)
        .expect_signtool_sign_driver_binary_sys_file(driver_name, &cwd, None)
        .expect_signtool_sign_cat_file(driver_name, &cwd, None)
        .expect_infverif(driver_name, &cwd, "KMDF", None);

    let build_action = BuildAction::new(
        &BuildActionParams {
            working_dir: &cwd,
            profile: profile.as_ref(),
            target_arch,
            verify_signature,
            is_sample_class: sample_class,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        test_build_action.mock_wdk_build_provider(),
        test_build_action.mock_run_command(),
        test_build_action.mock_fs_provider(),
        test_build_action.mock_metadata_provider(),
    );
    assert!(build_action.is_ok());

    let run_result = build_action.expect("Failed to init build action").run();

    assert!(run_result.is_ok());
}

#[test]
pub fn given_a_driver_project_when_profile_is_release_then_it_builds_successfully() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = Some(Profile::Release);
    let target_arch = TargetArch::Default(CpuArchitecture::Amd64);
    let verify_signature = false;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(wdk_metadata));

    let expected_certmgr_output = Output {
        status: ExitStatus::default(),
        stdout: r"==============No Certificates ==========
                            ==============No CTLs ==========
                            ==============No CRLs ==========
                            ==============================================
                            CertMgr Succeeded"
            .as_bytes()
            .to_vec(),
        stderr: vec![],
    };

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, target_arch, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_all_package_roots()
        .expect_path_canonicalization_package_manifest_path(&cwd)
        .expect_cargo_build(driver_name, &cwd, None)
        .expect_final_package_dir_exists(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true, &cwd)
        .expect_copy_map_file_to_package_folder(driver_name, &cwd, true)
        .expect_stampinf(driver_name, &cwd, None)
        .expect_inf2cat(driver_name, &cwd, None)
        .expect_self_signed_cert_file_exists(&cwd, false)
        .expect_certmgr_exists_check(Some(expected_certmgr_output))
        .expect_makecert(&cwd, None)
        .expect_copy_self_signed_cert_file_to_package_folder(driver_name, &cwd, true)
        .expect_signtool_sign_driver_binary_sys_file(driver_name, &cwd, None)
        .expect_signtool_sign_cat_file(driver_name, &cwd, None)
        .expect_infverif(driver_name, &cwd, "KMDF", None);

    let build_action = BuildAction::new(
        &BuildActionParams {
            working_dir: &cwd,
            profile: profile.as_ref(),
            target_arch,
            verify_signature,
            is_sample_class: sample_class,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        test_build_action.mock_wdk_build_provider(),
        test_build_action.mock_run_command(),
        test_build_action.mock_fs_provider(),
        test_build_action.mock_metadata_provider(),
    );
    assert!(build_action.is_ok());

    let run_result = build_action.expect("Failed to init build action").run();

    assert!(run_result.is_ok());
}

#[test]
pub fn given_a_driver_project_when_target_arch_is_arm64_then_it_builds_successfully() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = TargetArch::Selected(CpuArchitecture::Arm64);
    let verify_signature = false;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(wdk_metadata));

    let expected_certmgr_output = Output {
        status: ExitStatus::default(),
        stdout: r"==============No Certificates ==========
                            ==============No CTLs ==========
                            ==============No CRLs ==========
                            ==============================================
                            CertMgr Succeeded"
            .as_bytes()
            .to_vec(),
        stderr: vec![],
    };

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, target_arch, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_all_package_roots()
        .expect_path_canonicalization_package_manifest_path(&cwd)
        .expect_cargo_build(driver_name, &cwd, None)
        .expect_final_package_dir_exists(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true, &cwd)
        .expect_copy_map_file_to_package_folder(driver_name, &cwd, true)
        .expect_stampinf(driver_name, &cwd, None)
        .expect_inf2cat(driver_name, &cwd, None)
        .expect_self_signed_cert_file_exists(&cwd, false)
        .expect_certmgr_exists_check(Some(expected_certmgr_output))
        .expect_makecert(&cwd, None)
        .expect_copy_self_signed_cert_file_to_package_folder(driver_name, &cwd, true)
        .expect_signtool_sign_driver_binary_sys_file(driver_name, &cwd, None)
        .expect_signtool_sign_cat_file(driver_name, &cwd, None)
        .expect_infverif(driver_name, &cwd, "KMDF", None);

    let build_action = BuildAction::new(
        &BuildActionParams {
            working_dir: &cwd,
            profile: profile.as_ref(),
            target_arch,
            verify_signature,
            is_sample_class: sample_class,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        test_build_action.mock_wdk_build_provider(),
        test_build_action.mock_run_command(),
        test_build_action.mock_fs_provider(),
        test_build_action.mock_metadata_provider(),
    );
    assert!(build_action.is_ok());

    let run_result = build_action.expect("Failed to init build action").run();

    assert!(run_result.is_ok());
}

#[test]
pub fn given_a_driver_project_when_profile_is_release_and_target_arch_is_arm64_then_it_builds_successfully(
) {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = Some(Profile::Release);
    let target_arch = TargetArch::Selected(CpuArchitecture::Arm64);
    let verify_signature = false;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(wdk_metadata));

    let expected_certmgr_output = Output {
        status: ExitStatus::default(),
        stdout: r"==============No Certificates ==========
                            ==============No CTLs ==========
                            ==============No CRLs ==========
                            ==============================================
                            CertMgr Succeeded"
            .as_bytes()
            .to_vec(),
        stderr: vec![],
    };

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, target_arch, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_all_package_roots()
        .expect_path_canonicalization_package_manifest_path(&cwd)
        .expect_cargo_build(driver_name, &cwd, None)
        .expect_final_package_dir_exists(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true, &cwd)
        .expect_copy_map_file_to_package_folder(driver_name, &cwd, true)
        .expect_stampinf(driver_name, &cwd, None)
        .expect_inf2cat(driver_name, &cwd, None)
        .expect_self_signed_cert_file_exists(&cwd, false)
        .expect_certmgr_exists_check(Some(expected_certmgr_output))
        .expect_makecert(&cwd, None)
        .expect_copy_self_signed_cert_file_to_package_folder(driver_name, &cwd, true)
        .expect_signtool_sign_driver_binary_sys_file(driver_name, &cwd, None)
        .expect_signtool_sign_cat_file(driver_name, &cwd, None)
        .expect_infverif(driver_name, &cwd, "KMDF", None);

    let build_action = BuildAction::new(
        &BuildActionParams {
            working_dir: &cwd,
            profile: profile.as_ref(),
            target_arch,
            verify_signature,
            is_sample_class: sample_class,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        test_build_action.mock_wdk_build_provider(),
        test_build_action.mock_run_command(),
        test_build_action.mock_fs_provider(),
        test_build_action.mock_metadata_provider(),
    );
    assert!(build_action.is_ok());

    let run_result = build_action.expect("Failed to init build action").run();

    assert!(run_result.is_ok());
}

#[test]
pub fn given_a_driver_project_when_sample_class_is_true_then_it_builds_successfully() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = TargetArch::Default(CpuArchitecture::Amd64);
    let verify_signature = false;
    let sample_class = true;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(wdk_metadata));

    let expected_certmgr_output = Output {
        status: ExitStatus::default(),
        stdout: r"==============No Certificates ==========
                            ==============No CTLs ==========
                            ==============No CRLs ==========
                            ==============================================
                            CertMgr Succeeded"
            .as_bytes()
            .to_vec(),
        stderr: vec![],
    };

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, target_arch, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_all_package_roots()
        .expect_path_canonicalization_package_manifest_path(&cwd)
        .expect_cargo_build(driver_name, &cwd, None)
        .expect_final_package_dir_exists(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true, &cwd)
        .expect_copy_map_file_to_package_folder(driver_name, &cwd, true)
        .expect_stampinf(driver_name, &cwd, None)
        .expect_inf2cat(driver_name, &cwd, None)
        .expect_self_signed_cert_file_exists(&cwd, false)
        .expect_certmgr_exists_check(Some(expected_certmgr_output))
        .expect_makecert(&cwd, None)
        .expect_copy_self_signed_cert_file_to_package_folder(driver_name, &cwd, true)
        .expect_signtool_sign_driver_binary_sys_file(driver_name, &cwd, None)
        .expect_signtool_sign_cat_file(driver_name, &cwd, None)
        .expect_infverif(driver_name, &cwd, "KMDF", None)
        .expect_detect_wdk_build_number(25100u32);

    let build_action = BuildAction::new(
        &BuildActionParams {
            working_dir: &cwd,
            profile: profile.as_ref(),
            target_arch,
            verify_signature,
            is_sample_class: sample_class,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        test_build_action.mock_wdk_build_provider(),
        test_build_action.mock_run_command(),
        test_build_action.mock_fs_provider(),
        test_build_action.mock_metadata_provider(),
    );
    assert!(build_action.is_ok());

    let run_result = build_action.expect("Failed to init build action").run();

    assert!(run_result.is_ok());
}

#[test]
pub fn given_a_driver_project_when_verify_signature_is_true_then_it_builds_successfully() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = TargetArch::Default(CpuArchitecture::Amd64);
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(wdk_metadata));

    let expected_certmgr_output = Output {
        status: ExitStatus::default(),
        stdout: r"==============No Certificates ==========
                            ==============No CTLs ==========
                            ==============No CRLs ==========
                            ==============================================
                            CertMgr Succeeded"
            .as_bytes()
            .to_vec(),
        stderr: vec![],
    };

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, target_arch, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_all_package_roots()
        .expect_path_canonicalization_package_manifest_path(&cwd)
        .expect_cargo_build(driver_name, &cwd, None)
        .expect_final_package_dir_exists(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true, &cwd)
        .expect_copy_map_file_to_package_folder(driver_name, &cwd, true)
        .expect_stampinf(driver_name, &cwd, None)
        .expect_inf2cat(driver_name, &cwd, None)
        .expect_self_signed_cert_file_exists(&cwd, false)
        .expect_certmgr_exists_check(Some(expected_certmgr_output))
        .expect_makecert(&cwd, None)
        .expect_copy_self_signed_cert_file_to_package_folder(driver_name, &cwd, true)
        .expect_signtool_sign_driver_binary_sys_file(driver_name, &cwd, None)
        .expect_signtool_sign_cat_file(driver_name, &cwd, None)
        .expect_signtool_verify_driver_binary_sys_file(driver_name, &cwd, None)
        .expect_signtool_verify_cat_file(driver_name, &cwd, None)
        .expect_infverif(driver_name, &cwd, "KMDF", None);

    let build_action = BuildAction::new(
        &BuildActionParams {
            working_dir: &cwd,
            profile: profile.as_ref(),
            target_arch,
            verify_signature,
            is_sample_class: sample_class,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        test_build_action.mock_wdk_build_provider(),
        test_build_action.mock_run_command(),
        test_build_action.mock_fs_provider(),
        test_build_action.mock_metadata_provider(),
    );
    assert!(build_action.is_ok());

    let run_result = build_action.expect("Failed to init build action").run();

    assert!(run_result.is_ok());
}

#[test]
pub fn given_a_driver_project_when_self_signed_exists_then_it_should_skip_calling_makecert() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = TargetArch::Default(CpuArchitecture::Amd64);
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(wdk_metadata));

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

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, target_arch, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_all_package_roots()
        .expect_path_canonicalization_package_manifest_path(&cwd)
        .expect_cargo_build(driver_name, &cwd, None)
        .expect_final_package_dir_exists(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true, &cwd)
        .expect_copy_map_file_to_package_folder(driver_name, &cwd, true)
        .expect_stampinf(driver_name, &cwd, None)
        .expect_inf2cat(driver_name, &cwd, None)
        .expect_self_signed_cert_file_exists(&cwd, false)
        .expect_certmgr_exists_check(Some(expected_certmgr_output))
        .expect_certmgr_create_cert_from_store(&cwd, Some(expected_create_cert_output))
        .expect_copy_self_signed_cert_file_to_package_folder(driver_name, &cwd, true)
        .expect_signtool_sign_driver_binary_sys_file(driver_name, &cwd, None)
        .expect_signtool_sign_cat_file(driver_name, &cwd, None)
        .expect_signtool_verify_driver_binary_sys_file(driver_name, &cwd, None)
        .expect_signtool_verify_cat_file(driver_name, &cwd, None)
        .expect_infverif(driver_name, &cwd, "KMDF", None);

    let build_action = BuildAction::new(
        &BuildActionParams {
            working_dir: &cwd,
            profile: profile.as_ref(),
            target_arch,
            verify_signature,
            is_sample_class: sample_class,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        test_build_action.mock_wdk_build_provider(),
        test_build_action.mock_run_command(),
        test_build_action.mock_fs_provider(),
        test_build_action.mock_metadata_provider(),
    );
    assert!(build_action.is_ok());

    let run_result = build_action.expect("Failed to init build action").run();

    assert!(run_result.is_ok());
}

#[test]
pub fn given_a_driver_project_when_final_package_dir_exists_then_it_should_skip_creating_it() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = TargetArch::Default(CpuArchitecture::Amd64);
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(wdk_metadata));
    let expected_certmgr_output = Output {
        status: ExitStatus::default(),
        stdout: r"==============No Certificates ==========
                            ==============No CTLs ==========
                            ==============No CRLs ==========
                            ==============================================
                            CertMgr Succeeded"
            .as_bytes()
            .to_vec(),
        stderr: vec![],
    };

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, target_arch, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_all_package_roots()
        .expect_path_canonicalization_package_manifest_path(&cwd)
        .expect_cargo_build(driver_name, &cwd, None)
        .expect_final_package_dir_exists(driver_name, &cwd, false)
        .expect_dir_created(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true, &cwd)
        .expect_copy_map_file_to_package_folder(driver_name, &cwd, true)
        .expect_stampinf(driver_name, &cwd, None)
        .expect_inf2cat(driver_name, &cwd, None)
        .expect_self_signed_cert_file_exists(&cwd, false)
        .expect_certmgr_exists_check(Some(expected_certmgr_output))
        .expect_makecert(&cwd, None)
        .expect_copy_self_signed_cert_file_to_package_folder(driver_name, &cwd, true)
        .expect_signtool_sign_driver_binary_sys_file(driver_name, &cwd, None)
        .expect_signtool_sign_cat_file(driver_name, &cwd, None)
        .expect_signtool_verify_driver_binary_sys_file(driver_name, &cwd, None)
        .expect_signtool_verify_cat_file(driver_name, &cwd, None)
        .expect_infverif(driver_name, &cwd, "KMDF", None);

    let build_action = BuildAction::new(
        &BuildActionParams {
            working_dir: &cwd,
            profile: profile.as_ref(),
            target_arch,
            verify_signature,
            is_sample_class: sample_class,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        test_build_action.mock_wdk_build_provider(),
        test_build_action.mock_run_command(),
        test_build_action.mock_fs_provider(),
        test_build_action.mock_metadata_provider(),
    );
    assert!(build_action.is_ok());

    let run_result = build_action.expect("Failed to init build action").run();

    assert!(run_result.is_ok());
}

#[test]
pub fn given_a_driver_project_when_inx_file_do_not_exist_then_package_should_fail() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = TargetArch::Default(CpuArchitecture::Amd64);
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(wdk_metadata));

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, target_arch, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_all_package_roots()
        .expect_path_canonicalization_package_manifest_path(&cwd)
        .expect_cargo_build(driver_name, &cwd, None)
        .expect_final_package_dir_exists(driver_name, &cwd, false)
        .expect_dir_created(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, false);

    let build_action = BuildAction::new(
        &BuildActionParams {
            working_dir: &cwd,
            profile: profile.as_ref(),
            target_arch,
            verify_signature,
            is_sample_class: sample_class,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        test_build_action.mock_wdk_build_provider(),
        test_build_action.mock_run_command(),
        test_build_action.mock_fs_provider(),
        test_build_action.mock_metadata_provider(),
    );
    assert!(build_action.is_ok());

    let run_result = build_action.expect("Failed to init build action").run();

    assert!(matches!(
        run_result.as_ref().expect_err("expected error"),
        BuildActionError::OneOrMoreWorkspaceMembersFailedToBuild(_)
    ));
}

#[test]
pub fn given_a_driver_project_when_copy_of_an_artifact_fails_then_the_package_should_fail() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = TargetArch::Default(CpuArchitecture::Amd64);
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(wdk_metadata));

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, target_arch, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_all_package_roots()
        .expect_path_canonicalization_package_manifest_path(&cwd)
        .expect_cargo_build(driver_name, &cwd, None)
        .expect_final_package_dir_exists(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, false);

    let build_action = BuildAction::new(
        &BuildActionParams {
            working_dir: &cwd,
            profile: profile.as_ref(),
            target_arch,
            verify_signature,
            is_sample_class: sample_class,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        test_build_action.mock_wdk_build_provider(),
        test_build_action.mock_run_command(),
        test_build_action.mock_fs_provider(),
        test_build_action.mock_metadata_provider(),
    );
    assert!(build_action.is_ok());

    let run_result = build_action.expect("Failed to init build action").run();

    assert!(matches!(
        run_result.as_ref().expect_err("expected error"),
        BuildActionError::OneOrMoreWorkspaceMembersFailedToBuild(_)
    ));
}

#[test]
pub fn given_a_driver_project_when_stampinf_command_execution_fails_then_package_should_fail() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = TargetArch::Default(CpuArchitecture::Amd64);
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(wdk_metadata));

    let expected_stampinf_output = Output {
        status: ExitStatus::from_raw(1),
        stdout: vec![],
        stderr: vec![],
    };

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, target_arch, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_all_package_roots()
        .expect_path_canonicalization_package_manifest_path(&cwd)
        .expect_cargo_build(driver_name, &cwd, None)
        .expect_final_package_dir_exists(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true, &cwd)
        .expect_copy_map_file_to_package_folder(driver_name, &cwd, true)
        .expect_stampinf(driver_name, &cwd, Some(expected_stampinf_output));

    let build_action = BuildAction::new(
        &BuildActionParams {
            working_dir: &cwd,
            profile: profile.as_ref(),
            target_arch,
            verify_signature,
            is_sample_class: sample_class,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        test_build_action.mock_wdk_build_provider(),
        test_build_action.mock_run_command(),
        test_build_action.mock_fs_provider(),
        test_build_action.mock_metadata_provider(),
    );
    assert!(build_action.is_ok());

    let run_result = build_action.expect("Failed to init build action").run();

    assert!(matches!(
        run_result.as_ref().expect_err("expected error"),
        BuildActionError::OneOrMoreWorkspaceMembersFailedToBuild(_)
    ));
}

#[test]
pub fn given_a_driver_project_when_inf2cat_command_execution_fails_then_package_should_fail() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = TargetArch::Default(CpuArchitecture::Amd64);
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(wdk_metadata));

    let expected_inf2cat_output = Output {
        status: ExitStatus::from_raw(1),
        stdout: vec![],
        stderr: vec![],
    };

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, target_arch, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_all_package_roots()
        .expect_path_canonicalization_package_manifest_path(&cwd)
        .expect_cargo_build(driver_name, &cwd, None)
        .expect_final_package_dir_exists(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true, &cwd)
        .expect_copy_map_file_to_package_folder(driver_name, &cwd, true)
        .expect_stampinf(driver_name, &cwd, None)
        .expect_inf2cat(driver_name, &cwd, Some(expected_inf2cat_output));

    let build_action = BuildAction::new(
        &BuildActionParams {
            working_dir: &cwd,
            profile: profile.as_ref(),
            target_arch,
            verify_signature,
            is_sample_class: sample_class,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        test_build_action.mock_wdk_build_provider(),
        test_build_action.mock_run_command(),
        test_build_action.mock_fs_provider(),
        test_build_action.mock_metadata_provider(),
    );
    assert!(build_action.is_ok());

    let run_result = build_action.expect("Failed to init build action").run();

    assert!(matches!(
        run_result.as_ref().expect_err("expected error"),
        BuildActionError::OneOrMoreWorkspaceMembersFailedToBuild(_)
    ));
}

#[test]
pub fn given_a_driver_project_when_certmgr_command_execution_fails_then_package_should_fail() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = TargetArch::Default(CpuArchitecture::Amd64);
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(wdk_metadata));

    let expected_output = Output {
        status: ExitStatus::from_raw(1),
        stdout: vec![],
        stderr: vec![],
    };

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, target_arch, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_all_package_roots()
        .expect_path_canonicalization_package_manifest_path(&cwd)
        .expect_cargo_build(driver_name, &cwd, None)
        .expect_final_package_dir_exists(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true, &cwd)
        .expect_copy_map_file_to_package_folder(driver_name, &cwd, true)
        .expect_stampinf(driver_name, &cwd, None)
        .expect_inf2cat(driver_name, &cwd, None)
        .expect_self_signed_cert_file_exists(&cwd, false)
        .expect_certmgr_exists_check(Some(expected_output));

    let build_action = BuildAction::new(
        &BuildActionParams {
            working_dir: &cwd,
            profile: profile.as_ref(),
            target_arch,
            verify_signature,
            is_sample_class: sample_class,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        test_build_action.mock_wdk_build_provider(),
        test_build_action.mock_run_command(),
        test_build_action.mock_fs_provider(),
        test_build_action.mock_metadata_provider(),
    );
    assert!(build_action.is_ok());

    let run_result = build_action.expect("Failed to init build action").run();

    assert!(matches!(
        run_result.as_ref().expect_err("expected error"),
        BuildActionError::OneOrMoreWorkspaceMembersFailedToBuild(_)
    ));
}

#[test]
pub fn given_a_driver_project_when_makecert_command_execution_fails_then_package_should_fail() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = TargetArch::Default(CpuArchitecture::Amd64);
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(wdk_metadata));

    let expected_output = Output {
        status: ExitStatus::from_raw(1),
        stdout: vec![],
        stderr: vec![],
    };

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, target_arch, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_all_package_roots()
        .expect_path_canonicalization_package_manifest_path(&cwd)
        .expect_cargo_build(driver_name, &cwd, None)
        .expect_final_package_dir_exists(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true, &cwd)
        .expect_copy_map_file_to_package_folder(driver_name, &cwd, true)
        .expect_stampinf(driver_name, &cwd, None)
        .expect_inf2cat(driver_name, &cwd, None)
        .expect_self_signed_cert_file_exists(&cwd, false)
        .expect_certmgr_exists_check(None)
        .expect_makecert(&cwd, Some(expected_output));

    let build_action = BuildAction::new(
        &BuildActionParams {
            working_dir: &cwd,
            profile: profile.as_ref(),
            target_arch,
            verify_signature,
            is_sample_class: sample_class,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        test_build_action.mock_wdk_build_provider(),
        test_build_action.mock_run_command(),
        test_build_action.mock_fs_provider(),
        test_build_action.mock_metadata_provider(),
    );
    assert!(build_action.is_ok());

    let run_result = build_action.expect("Failed to init build action").run();

    assert!(matches!(
        run_result.as_ref().expect_err("expected error"),
        BuildActionError::OneOrMoreWorkspaceMembersFailedToBuild(_)
    ));
}

#[test]
pub fn given_a_driver_project_when_signtool_command_execution_fails_then_package_should_fail() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = TargetArch::Default(CpuArchitecture::Amd64);
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(wdk_metadata));

    let expected_output = Output {
        status: ExitStatus::from_raw(1),
        stdout: vec![],
        stderr: vec![],
    };

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, target_arch, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_all_package_roots()
        .expect_path_canonicalization_package_manifest_path(&cwd)
        .expect_cargo_build(driver_name, &cwd, None)
        .expect_final_package_dir_exists(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true, &cwd)
        .expect_copy_map_file_to_package_folder(driver_name, &cwd, true)
        .expect_stampinf(driver_name, &cwd, None)
        .expect_inf2cat(driver_name, &cwd, None)
        .expect_self_signed_cert_file_exists(&cwd, false)
        .expect_certmgr_exists_check(None)
        .expect_makecert(&cwd, None)
        .expect_copy_self_signed_cert_file_to_package_folder(driver_name, &cwd, true)
        .expect_signtool_sign_driver_binary_sys_file(driver_name, &cwd, Some(expected_output));

    let build_action = BuildAction::new(
        &BuildActionParams {
            working_dir: &cwd,
            profile: profile.as_ref(),
            target_arch,
            verify_signature,
            is_sample_class: sample_class,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        test_build_action.mock_wdk_build_provider(),
        test_build_action.mock_run_command(),
        test_build_action.mock_fs_provider(),
        test_build_action.mock_metadata_provider(),
    );
    assert!(build_action.is_ok());

    let run_result = build_action.expect("Failed to init build action").run();

    assert!(matches!(
        run_result.as_ref().expect_err("expected error"),
        BuildActionError::OneOrMoreWorkspaceMembersFailedToBuild(_)
    ));
}

#[test]
pub fn given_a_driver_project_when_infverif_command_execution_fails_then_package_should_fail() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = TargetArch::Default(CpuArchitecture::Amd64);
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, Some(wdk_metadata));

    let expected_output = Output {
        status: ExitStatus::from_raw(1),
        stdout: vec![],
        stderr: vec![],
    };

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, target_arch, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_all_package_roots()
        .expect_path_canonicalization_package_manifest_path(&cwd)
        .expect_cargo_build(driver_name, &cwd, None)
        .expect_final_package_dir_exists(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true, &cwd)
        .expect_copy_map_file_to_package_folder(driver_name, &cwd, true)
        .expect_stampinf(driver_name, &cwd, None)
        .expect_inf2cat(driver_name, &cwd, None)
        .expect_self_signed_cert_file_exists(&cwd, false)
        .expect_certmgr_exists_check(None)
        .expect_makecert(&cwd, None)
        .expect_copy_self_signed_cert_file_to_package_folder(driver_name, &cwd, true)
        .expect_signtool_sign_driver_binary_sys_file(driver_name, &cwd, None)
        .expect_signtool_sign_cat_file(driver_name, &cwd, None)
        .expect_infverif(driver_name, &cwd, "KMDF", Some(expected_output));

    let build_action = BuildAction::new(
        &BuildActionParams {
            working_dir: &cwd,
            profile: profile.as_ref(),
            target_arch,
            verify_signature,
            is_sample_class: sample_class,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        test_build_action.mock_wdk_build_provider(),
        test_build_action.mock_run_command(),
        test_build_action.mock_fs_provider(),
        test_build_action.mock_metadata_provider(),
    );
    assert!(build_action.is_ok());

    let run_result = build_action.expect("Failed to init build action").run();

    assert!(matches!(
        run_result.as_ref().expect_err("expected error"),
        BuildActionError::OneOrMoreWorkspaceMembersFailedToBuild(_)
    ));
}

#[test]
pub fn given_a_non_driver_project_when_default_values_are_provided_then_wdk_metadata_parse_should_fail(
) {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = TargetArch::Default(CpuArchitecture::Amd64);
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_name = "non-driver";
    let driver_version = "0.0.1";
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, driver_name, driver_version, None);

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, target_arch, sample_class)
        .set_up_standalone_driver_project((workspace_member, package))
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_all_package_roots()
        .expect_path_canonicalization_package_manifest_path(&cwd)
        .expect_cargo_build(driver_name, &cwd, None);

    let build_action = BuildAction::new(
        &BuildActionParams {
            working_dir: &cwd,
            profile: profile.as_ref(),
            target_arch,
            verify_signature,
            is_sample_class: sample_class,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        test_build_action.mock_wdk_build_provider(),
        test_build_action.mock_run_command(),
        test_build_action.mock_fs_provider(),
        test_build_action.mock_metadata_provider(),
    );
    assert!(build_action.is_ok());

    let run_result = build_action.expect("Failed to init build action").run();
    assert!(matches!(
        run_result.as_ref().expect_err("expected error"),
        BuildActionError::WdkMetadataParse(TryFromCargoMetadataError::NoWdkConfigurationsDetected)
    ));
}

#[test]
pub fn given_a_invalid_driver_project_with_partial_wdk_metadata_when_valid_default_values_are_provided_then_wdk_metadata_parse_should_fail(
) {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp\\sample-driver");
    let profile = None;
    let target_arch = TargetArch::Default(CpuArchitecture::Amd64);
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let driver_name = "sample-driver";
    let cargo_toml_metadata = invalid_driver_cargo_toml();

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, target_arch, sample_class)
        .set_up_with_custom_toml(&cargo_toml_metadata)
        .expect_detect_wdk_build_number(25100u32)
        .expect_root_manifest_exists(&cwd, true)
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_all_package_roots()
        .expect_path_canonicalization_package_manifest_path(&cwd)
        .expect_cargo_build(driver_name, &cwd, None);

    let build_action = BuildAction::new(
        &BuildActionParams {
            working_dir: &cwd,
            profile: profile.as_ref(),
            target_arch,
            verify_signature,
            is_sample_class: sample_class,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        test_build_action.mock_wdk_build_provider(),
        test_build_action.mock_run_command(),
        test_build_action.mock_fs_provider(),
        test_build_action.mock_metadata_provider(),
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

////////////////////////////////////////////////////////////////////////////////
/// Workspace tests
////////////////////////////////////////////////////////////////////////////////
#[test]
pub fn given_a_workspace_with_multiple_driver_and_non_driver_projects_when_default_values_are_provided_then_it_packages_successfully(
) {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = TargetArch::Default(CpuArchitecture::Amd64);
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
        &cwd.join(driver_name_1),
        driver_name_1,
        driver_version_1,
        Some(wdk_metadata.clone()),
    );
    let (workspace_member_2, package_2) = get_cargo_metadata_package(
        &cwd.join(driver_name_2),
        driver_name_2,
        driver_version_2,
        Some(wdk_metadata.clone()),
    );
    let (workspace_member_3, package_3) =
        get_cargo_metadata_package(&cwd.join(non_driver), non_driver, non_driver_version, None);

    let expected_certmgr_output = Output {
        status: ExitStatus::default(),
        stdout: r"==============No Certificates ==========
                            ==============No CTLs ==========
                            ==============No CRLs ==========
                            ==============================================
                            CertMgr Succeeded"
            .as_bytes()
            .to_vec(),
        stderr: vec![],
    };

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, target_arch, sample_class)
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
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_all_package_roots()
        .expect_path_canonicalization_package_manifest_path(&cwd.join(driver_name_1))
        .expect_cargo_build(driver_name_1, &cwd.join(driver_name_1), None)
        .expect_final_package_dir_exists(driver_name_1, &cwd, true)
        .expect_inx_file_exists(driver_name_1, &cwd.join(driver_name_1), true)
        .expect_rename_driver_binary_dll_to_sys(driver_name_1, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name_1, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name_1, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name_1, &cwd.join(driver_name_1), true, &cwd)
        .expect_copy_map_file_to_package_folder(driver_name_1, &cwd, true)
        .expect_stampinf(driver_name_1, &cwd, None)
        .expect_inf2cat(driver_name_1, &cwd, None)
        .expect_self_signed_cert_file_exists(&cwd, false)
        .expect_certmgr_exists_check(Some(expected_certmgr_output.clone()))
        .expect_makecert(&cwd, None)
        .expect_copy_self_signed_cert_file_to_package_folder(driver_name_1, &cwd, true)
        .expect_signtool_sign_driver_binary_sys_file(driver_name_1, &cwd, None)
        .expect_signtool_sign_cat_file(driver_name_1, &cwd, None)
        .expect_signtool_verify_driver_binary_sys_file(driver_name_1, &cwd, None)
        .expect_signtool_verify_cat_file(driver_name_1, &cwd, None)
        .expect_infverif(driver_name_1, &cwd, "KMDF", None)
        // Second driver project
        .expect_path_canonicalization_package_manifest_path(&cwd.join(driver_name_2))
        .expect_cargo_build(driver_name_2, &cwd.join(driver_name_2), None)
        .expect_final_package_dir_exists(driver_name_2, &cwd, true)
        .expect_inx_file_exists(driver_name_2, &cwd.join(driver_name_2), true)
        .expect_rename_driver_binary_dll_to_sys(driver_name_2, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name_2, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name_2, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name_2, &cwd.join(driver_name_2), true, &cwd)
        .expect_copy_map_file_to_package_folder(driver_name_2, &cwd, true)
        .expect_stampinf(driver_name_2, &cwd, None)
        .expect_inf2cat(driver_name_2, &cwd, None)
        .expect_self_signed_cert_file_exists(&cwd, false)
        .expect_certmgr_exists_check(Some(expected_certmgr_output))
        .expect_makecert(&cwd, None)
        .expect_copy_self_signed_cert_file_to_package_folder(driver_name_2, &cwd, true)
        .expect_signtool_sign_driver_binary_sys_file(driver_name_2, &cwd, None)
        .expect_signtool_sign_cat_file(driver_name_2, &cwd, None)
        .expect_signtool_verify_driver_binary_sys_file(driver_name_2, &cwd, None)
        .expect_signtool_verify_cat_file(driver_name_2, &cwd, None)
        .expect_infverif(driver_name_2, &cwd, "KMDF", None)
        // Non-driver project
        .expect_path_canonicalization_package_manifest_path(&cwd.join(non_driver))
        .expect_cargo_build(non_driver, &cwd.join(non_driver), None);

    let build_action = BuildAction::new(
        &BuildActionParams {
            working_dir: &cwd,
            profile: profile.as_ref(),
            target_arch,
            verify_signature,
            is_sample_class: sample_class,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        test_build_action.mock_wdk_build_provider(),
        test_build_action.mock_run_command(),
        test_build_action.mock_fs_provider(),
        test_build_action.mock_metadata_provider(),
    );
    assert!(build_action.is_ok());

    let run_result = build_action.expect("Failed to init build action").run();

    assert!(run_result.is_ok());
}

#[test]
pub fn given_a_workspace_with_multiple_driver_and_non_driver_projects_when_cwd_is_driver_project_then_it_packages_driver_project_successfully(
) {
    // Input CLI args
    let workspace_root_dir = PathBuf::from("C:\\tmp");
    let cwd = workspace_root_dir.join("sample-kmdf-1");
    let profile = None;
    let target_arch = TargetArch::Default(CpuArchitecture::Amd64);
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
        Some(wdk_metadata.clone()),
    );
    let (workspace_member_2, package_2) = get_cargo_metadata_package(
        &workspace_root_dir.join(driver_name_2),
        driver_name_2,
        driver_version_2,
        Some(wdk_metadata.clone()),
    );
    let (workspace_member_3, package_3) = get_cargo_metadata_package(
        &workspace_root_dir.join(non_driver),
        non_driver,
        non_driver_version,
        None,
    );

    let expected_certmgr_output = Output {
        status: ExitStatus::default(),
        stdout: r"==============No Certificates ==========
                            ==============No CTLs ==========
                            ==============No CRLs ==========
                            ==============================================
                            CertMgr Succeeded"
            .as_bytes()
            .to_vec(),
        stderr: vec![],
    };

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, target_arch, sample_class) // Even when cwd is changed to driver project inside the workspace, cargo metadata read is
        // going to be for the whole workspace
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
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_package_root(&cwd)
        .expect_path_canonicalization_package_manifest_path(&cwd)
        .expect_cargo_build(driver_name_1, &cwd, None)
        .expect_final_package_dir_exists(driver_name_1, &workspace_root_dir, true)
        .expect_inx_file_exists(driver_name_1, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name_1, &workspace_root_dir)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name_1, &workspace_root_dir, true)
        .expect_copy_pdb_file_to_package_folder(driver_name_1, &workspace_root_dir, true)
        .expect_copy_inx_file_to_package_folder(driver_name_1, &cwd, true, &workspace_root_dir)
        .expect_copy_map_file_to_package_folder(driver_name_1, &workspace_root_dir, true)
        .expect_stampinf(driver_name_1, &workspace_root_dir, None)
        .expect_inf2cat(driver_name_1, &workspace_root_dir, None)
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
        .expect_infverif(driver_name_1, &workspace_root_dir, "KMDF", None);

    let build_action = BuildAction::new(
        &BuildActionParams {
            working_dir: &cwd,
            profile: profile.as_ref(),
            target_arch,
            verify_signature,
            is_sample_class: sample_class,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        test_build_action.mock_wdk_build_provider(),
        test_build_action.mock_run_command(),
        test_build_action.mock_fs_provider(),
        test_build_action.mock_metadata_provider(),
    );
    assert!(build_action.is_ok());

    let run_result = build_action.expect("Failed to init build action").run();

    assert!(run_result.is_ok());
}

#[test]
pub fn given_a_workspace_with_multiple_driver_and_non_driver_projects_when_verify_signature_is_false_then_it_skips_verify_tasks(
) {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = TargetArch::Default(CpuArchitecture::Amd64);
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
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member_1, package_1) = get_cargo_metadata_package(
        &cwd.join(driver_name_1),
        driver_name_1,
        driver_version_1,
        Some(wdk_metadata.clone()),
    );
    let (workspace_member_2, package_2) = get_cargo_metadata_package(
        &cwd.join(driver_name_2),
        driver_name_2,
        driver_version_2,
        Some(wdk_metadata.clone()),
    );
    let (workspace_member_3, package_3) =
        get_cargo_metadata_package(&cwd.join(non_driver), non_driver, non_driver_version, None);

    let expected_certmgr_output = Output {
        status: ExitStatus::default(),
        stdout: r"==============No Certificates ==========
                            ==============No CTLs ==========
                            ==============No CRLs ==========
                            ==============================================
                            CertMgr Succeeded"
            .as_bytes()
            .to_vec(),
        stderr: vec![],
    };

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, target_arch, sample_class)
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
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_all_package_roots()
        .expect_path_canonicalization_package_manifest_path(&cwd.join(driver_name_1))
        .expect_cargo_build(driver_name_1, &cwd.join(driver_name_1), None)
        .expect_final_package_dir_exists(driver_name_1, &cwd, true)
        .expect_inx_file_exists(driver_name_1, &cwd.join(driver_name_1), true)
        .expect_rename_driver_binary_dll_to_sys(driver_name_1, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name_1, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name_1, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name_1, &cwd.join(driver_name_1), true, &cwd)
        .expect_copy_map_file_to_package_folder(driver_name_1, &cwd, true)
        .expect_stampinf(driver_name_1, &cwd, None)
        .expect_inf2cat(driver_name_1, &cwd, None)
        .expect_self_signed_cert_file_exists(&cwd, false)
        .expect_certmgr_exists_check(Some(expected_certmgr_output.clone()))
        .expect_makecert(&cwd, None)
        .expect_copy_self_signed_cert_file_to_package_folder(driver_name_1, &cwd, true)
        .expect_signtool_sign_driver_binary_sys_file(driver_name_1, &cwd, None)
        .expect_signtool_sign_cat_file(driver_name_1, &cwd, None)
        .expect_infverif(driver_name_1, &cwd, "KMDF", None)
        // Second driver project
        .expect_path_canonicalization_package_manifest_path(&cwd.join(driver_name_2))
        .expect_cargo_build(driver_name_2, &cwd.join(driver_name_2), None)
        .expect_final_package_dir_exists(driver_name_2, &cwd, true)
        .expect_inx_file_exists(driver_name_2, &cwd.join(driver_name_2), true)
        .expect_rename_driver_binary_dll_to_sys(driver_name_2, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name_2, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name_2, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name_2, &cwd.join(driver_name_2), true, &cwd)
        .expect_copy_map_file_to_package_folder(driver_name_2, &cwd, true)
        .expect_stampinf(driver_name_2, &cwd, None)
        .expect_inf2cat(driver_name_2, &cwd, None)
        .expect_self_signed_cert_file_exists(&cwd, false)
        .expect_certmgr_exists_check(Some(expected_certmgr_output))
        .expect_makecert(&cwd, None)
        .expect_copy_self_signed_cert_file_to_package_folder(driver_name_2, &cwd, true)
        .expect_signtool_sign_driver_binary_sys_file(driver_name_2, &cwd, None)
        .expect_signtool_sign_cat_file(driver_name_2, &cwd, None)
        .expect_infverif(driver_name_2, &cwd, "KMDF", None)
        // Non-driver project
        .expect_path_canonicalization_package_manifest_path(&cwd.join(non_driver))
        .expect_cargo_build(non_driver, &cwd.join(non_driver), None);

    let build_action = BuildAction::new(
        &BuildActionParams {
            working_dir: &cwd,
            profile: profile.as_ref(),
            target_arch,
            verify_signature,
            is_sample_class: sample_class,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        test_build_action.mock_wdk_build_provider(),
        test_build_action.mock_run_command(),
        test_build_action.mock_fs_provider(),
        test_build_action.mock_metadata_provider(),
    );
    assert!(build_action.is_ok());

    let run_result = build_action.expect("Failed to init build action").run();

    assert!(run_result.is_ok());
}

#[test]
pub fn given_a_workspace_with_multiple_driver_and_non_driver_projects_when_cwd_is_non_driver_project_then_it_builds_but_skips_packaging(
) {
    // Input CLI args
    let workspace_root_dir = PathBuf::from("C:\\tmp");
    let cwd = workspace_root_dir.join("non-driver");
    let profile = None;
    let target_arch = TargetArch::Default(CpuArchitecture::Amd64);
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
        Some(wdk_metadata.clone()),
    );
    let (workspace_member_2, package_2) = get_cargo_metadata_package(
        &workspace_root_dir.join(driver_name_2),
        driver_name_2,
        driver_version_2,
        Some(wdk_metadata.clone()),
    );
    let (workspace_member_3, package_3) = get_cargo_metadata_package(
        &workspace_root_dir.join(non_driver),
        non_driver,
        non_driver_version,
        None,
    );

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, target_arch, sample_class) // Even when cwd is changed to driver project inside the workspace, cargo metadata read is
        // going to be for the whole workspace
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
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_all_package_roots()
        .expect_path_canonicalization_package_manifest_path(&cwd)
        .expect_cargo_build(non_driver, &cwd, None);

    let build_action = BuildAction::new(
        &BuildActionParams {
            working_dir: &cwd,
            profile: profile.as_ref(),
            target_arch,
            verify_signature,
            is_sample_class: sample_class,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        test_build_action.mock_wdk_build_provider(),
        test_build_action.mock_run_command(),
        test_build_action.mock_fs_provider(),
        test_build_action.mock_metadata_provider(),
    );
    assert!(build_action.is_ok());

    let run_result = build_action.expect("Failed to init build action").run();

    assert!(run_result.is_ok());
}

#[test]
pub fn given_a_workspace_with_multiple_distinct_wdk_configurations_at_each_workspace_member_level_when_default_values_are_provided_then_wdk_metadata_parse_should_fail(
) {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = TargetArch::Default(CpuArchitecture::Amd64);
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
        Some(wdk_metadata_1.clone()),
    );
    let (workspace_member_2, package_2) = get_cargo_metadata_package(
        &cwd.join(driver_name_2),
        driver_name_2,
        driver_version_2,
        Some(wdk_metadata_2),
    );

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, target_arch, sample_class)
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
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_all_package_roots()
        .expect_path_canonicalization_package_manifest_path(&cwd.join(driver_name_1))
        .expect_path_canonicalization_package_manifest_path(&cwd.join(driver_name_2))
        .expect_cargo_build(driver_name_1, &cwd.join(driver_name_1), None)
        .expect_cargo_build(driver_name_2, &cwd.join(driver_name_2), None);

    let build_action = BuildAction::new(
        &BuildActionParams {
            working_dir: &cwd,
            profile: profile.as_ref(),
            target_arch,
            verify_signature,
            is_sample_class: sample_class,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        test_build_action.mock_wdk_build_provider(),
        test_build_action.mock_run_command(),
        test_build_action.mock_fs_provider(),
        test_build_action.mock_metadata_provider(),
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
pub fn given_a_workspace_with_multiple_distinct_wdk_configurations_at_root_and_workspace_member_level_when_default_values_are_provided_then_wdk_metadata_parse_should_fail(
) {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = TargetArch::Default(CpuArchitecture::Amd64);
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
        Some(wdk_metadata_1.clone()),
    );
    let (workspace_member_2, package_2) = get_cargo_metadata_package(
        &cwd.join(driver_name_2),
        driver_name_2,
        driver_version_2,
        Some(wdk_metadata_1),
    );

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, target_arch, sample_class)
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
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_all_package_roots()
        .expect_path_canonicalization_package_manifest_path(&cwd.join(driver_name_1))
        .expect_path_canonicalization_package_manifest_path(&cwd.join(driver_name_2))
        .expect_cargo_build(driver_name_1, &cwd.join(driver_name_1), None)
        .expect_cargo_build(driver_name_2, &cwd.join(driver_name_2), None);

    let build_action = BuildAction::new(
        &BuildActionParams {
            working_dir: &cwd,
            profile: profile.as_ref(),
            target_arch,
            verify_signature,
            is_sample_class: sample_class,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        test_build_action.mock_wdk_build_provider(),
        test_build_action.mock_run_command(),
        test_build_action.mock_fs_provider(),
        test_build_action.mock_metadata_provider(),
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
pub fn given_a_workspace_only_with_non_driver_projects_when_cwd_is_workspace_root_then_wdk_metadata_parse_should_fail(
) {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = None;
    let target_arch = TargetArch::Default(CpuArchitecture::Amd64);
    let verify_signature = true;
    let sample_class = false;

    // Driver project data
    let non_driver = "non-driver";
    let non_driver_version = "0.0.3";
    let (workspace_member_3, package_3) =
        get_cargo_metadata_package(&cwd.join(non_driver), non_driver, non_driver_version, None);

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, target_arch, sample_class) // Even when cwd is changed to driver project inside the workspace, cargo metadata read is
        // going to be for the whole workspace
        .set_up_workspace_with_multiple_driver_projects(
            &cwd,
            None,
            vec![(workspace_member_3, package_3)],
        )
        .expect_root_manifest_exists(&cwd, true)
        .expect_detect_wdk_build_number(25100u32)
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_all_package_roots()
        .expect_path_canonicalization_package_manifest_path(&cwd.join(non_driver))
        .expect_cargo_build(non_driver, &cwd.join(non_driver), None);

    let build_action = BuildAction::new(
        &BuildActionParams {
            working_dir: &cwd,
            profile: profile.as_ref(),
            target_arch,
            verify_signature,
            is_sample_class: sample_class,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        test_build_action.mock_wdk_build_provider(),
        test_build_action.mock_run_command(),
        test_build_action.mock_fs_provider(),
        test_build_action.mock_metadata_provider(),
    );
    assert!(build_action.is_ok());

    let run_result = build_action.expect("Failed to init build action").run();

    assert!(matches!(
        run_result.expect_err("run_result error in test: given_a_workspace_only_with_non_driver_projects_when_cwd_is_workspace_root_then_wdk_metadata_parse_should_fail"),
        BuildActionError::WdkMetadataParse(
            TryFromCargoMetadataError::NoWdkConfigurationsDetected
        )
    ));
}

#[test]
pub fn given_a_workspace_only_with_non_driver_projects_when_cwd_is_workspace_member_then_wdk_metadata_parse_should_fail(
) {
    // Input CLI args
    let workspace_root_dir = PathBuf::from("C:\\tmp");
    let cwd = workspace_root_dir.join("non-driver");
    let profile = None;
    let target_arch = TargetArch::Default(CpuArchitecture::Amd64);
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

    let test_build_action = &TestBuildAction::new(cwd.clone(), profile, target_arch, sample_class) // Even when cwd is changed to driver project inside the workspace, cargo metadata read is
        // going to be for the whole workspace
        .set_up_workspace_with_multiple_driver_projects(
            &workspace_root_dir,
            None,
            vec![(workspace_member_3, package_3)],
        )
        .expect_root_manifest_exists(&cwd, true)
        .expect_detect_wdk_build_number(25100u32)
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_all_package_roots()
        .expect_path_canonicalization_package_manifest_path(&cwd)
        .expect_cargo_build(non_driver, &cwd, None);

    let build_action = BuildAction::new(
        &BuildActionParams {
            working_dir: &cwd,
            profile: profile.as_ref(),
            target_arch,
            verify_signature,
            is_sample_class: sample_class,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        test_build_action.mock_wdk_build_provider(),
        test_build_action.mock_run_command(),
        test_build_action.mock_fs_provider(),
        test_build_action.mock_metadata_provider(),
    );
    assert!(build_action.is_ok());

    let run_result = build_action.expect("Failed to init build action").run();

    assert!(matches!(
        run_result.expect_err("run_result error in test: given_a_workspace_only_with_non_driver_projects_when_cwd_is_workspace_member_then_wdk_metadata_parse_should_fail"),
        BuildActionError::WdkMetadataParse(
            TryFromCargoMetadataError::NoWdkConfigurationsDetected
        )
    ));
}

/// Helper functions
////////////////////////////////////////////////////////////////////////////////
struct TestBuildAction {
    cwd: PathBuf,
    profile: Option<Profile>,
    target_arch: TargetArch,
    sample_class: bool,

    cargo_metadata: Option<CargoMetadata>,
    // mocks
    mock_run_command: CommandExec,
    mock_wdk_build_provider: WdkBuild,
    mock_fs_provider: Fs,
    mock_metadata_provider: MetadataProvider,
}

// Presence of method ensures specific mock expectation is set
// Dir argument in any method means to operate on the relevant dir
// Output argument in any method means to override return output from default
// success with no stdout/stderr
trait TestSetupPackageExpectations {
    fn expect_root_manifest_exists(self, root_dir: &Path, does_exist: bool) -> Self;
    fn expect_path_canonicalization_cwd(self) -> Self;
    fn expect_path_canonicalization_workspace_root(self) -> Self;
    fn expect_path_canonicalization_all_package_roots(self) -> Self;
    fn expect_path_canonicalization_package_root(self, driver_dir: &Path) -> Self;
    fn expect_self_signed_cert_file_exists(self, driver_dir: &Path, does_exist: bool) -> Self;
    fn expect_final_package_dir_exists(
        self,
        driver_name: &str,
        driver_dir: &Path,
        does_exist: bool,
    ) -> Self;
    fn expect_dir_created(self, driver_name: &str, driver_dir: &Path, created: bool) -> Self;
    fn expect_path_canonicalization_package_manifest_path(self, driver_dir: &Path) -> Self;
    fn expect_cargo_build(
        self,
        driver_name: &str,
        driver_dir: &Path,
        override_output: Option<Output>,
    ) -> Self;
    fn expect_inx_file_exists(self, driver_name: &str, driver_dir: &Path, does_exist: bool)
        -> Self;
    fn expect_rename_driver_binary_dll_to_sys(self, driver_name: &str, driver_dir: &Path) -> Self;
    fn expect_copy_driver_binary_sys_to_package_folder(
        self,
        driver_name: &str,
        driver_dir: &Path,
        is_success: bool,
    ) -> Self;
    fn expect_copy_pdb_file_to_package_folder(
        self,
        driver_name: &str,
        driver_dir: &Path,
        is_success: bool,
    ) -> Self;
    fn expect_copy_inx_file_to_package_folder(
        self,
        driver_name: &str,
        driver_dir: &Path,
        is_success: bool,
        workspace_root_dir: &Path,
    ) -> Self;
    fn expect_copy_map_file_to_package_folder(
        self,
        driver_name: &str,
        driver_dir: &Path,
        is_success: bool,
    ) -> Self;
    fn expect_copy_self_signed_cert_file_to_package_folder(
        self,
        driver_name: &str,
        driver_dir: &Path,
        is_success: bool,
    ) -> Self;

    fn expect_stampinf(
        self,
        driver_name: &str,
        driver_dir: &Path,
        override_output: Option<Output>,
    ) -> Self;
    fn expect_inf2cat(
        self,
        driver_name: &str,
        driver_dir: &Path,
        override_output: Option<Output>,
    ) -> Self;
    fn expect_certmgr_exists_check(self, override_output: Option<Output>) -> Self;
    fn expect_certmgr_create_cert_from_store(
        self,
        driver_dir: &Path,
        override_output: Option<Output>,
    ) -> Self;
    fn expect_makecert(self, driver_dir: &Path, override_output: Option<Output>) -> Self;

    fn expect_signtool_sign_driver_binary_sys_file(
        self,
        driver_name: &str,
        driver_dir: &Path,
        override_output: Option<Output>,
    ) -> Self;
    fn expect_signtool_sign_cat_file(
        self,
        driver_name: &str,
        driver_dir: &Path,
        override_output: Option<Output>,
    ) -> Self;
    fn expect_signtool_verify_driver_binary_sys_file(
        self,
        driver_name: &str,
        driver_dir: &Path,
        override_output: Option<Output>,
    ) -> Self;
    fn expect_signtool_verify_cat_file(
        self,
        driver_name: &str,
        driver_dir: &Path,
        override_output: Option<Output>,
    ) -> Self;

    fn expect_detect_wdk_build_number(self, expected_wdk_build_number: u32) -> Self;
    fn expect_infverif(
        self,
        driver_name: &str,
        driver_dir: &Path,
        driver_type: &str,
        override_output: Option<Output>,
    ) -> Self;

    fn mock_wdk_build_provider(&self) -> &WdkBuild;
    fn mock_run_command(&self) -> &CommandExec;
    fn mock_fs_provider(&self) -> &Fs;
    fn mock_metadata_provider(&self) -> &MetadataProvider;
}

impl TestBuildAction {
    fn new(
        cwd: PathBuf,
        profile: Option<Profile>,
        target_arch: TargetArch,
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
            mock_run_command,
            mock_wdk_build_provider,
            mock_fs_provider,
            mock_metadata_provider,
            cargo_metadata: None,
        }
    }

    fn set_up_standalone_driver_project(
        mut self,
        package_metadata: (TestMetadataWorkspaceMemberId, TestMetadataPackage),
    ) -> impl TestSetupPackageExpectations {
        let cargo_toml_metadata = get_cargo_metadata(
            &self.cwd,
            vec![package_metadata.1],
            &[package_metadata.0],
            None,
        );
        let cargo_toml_metadata =
            serde_json::from_str::<cargo_metadata::Metadata>(&cargo_toml_metadata)
                .expect("Failed to parse cargo metadata in set_up_standalone_driver_project");
        let cargo_toml_metadata_clone = cargo_toml_metadata.clone();
        self.mock_metadata_provider
            .expect_get_cargo_metadata_at_path()
            .once()
            .returning(move |_| Ok(cargo_toml_metadata_clone.clone()));
        self.cargo_metadata = Some(cargo_toml_metadata);
        self
    }

    fn set_up_workspace_with_multiple_driver_projects(
        mut self,
        workspace_root_dir: &Path,
        workspace_additional_metadata: Option<TestWdkMetadata>,
        package_metadata_list: Vec<(TestMetadataWorkspaceMemberId, TestMetadataPackage)>,
    ) -> impl TestSetupPackageExpectations {
        let cargo_toml_metadata = get_cargo_metadata(
            workspace_root_dir,
            package_metadata_list.iter().map(|p| p.1.clone()).collect(),
            package_metadata_list
                .into_iter()
                .map(|p| p.0)
                .collect::<Vec<_>>()
                .as_slice(),
            workspace_additional_metadata,
        );
        let cargo_toml_metadata = serde_json::from_str::<cargo_metadata::Metadata>(
            &cargo_toml_metadata,
        )
        .expect("Failed to parse cargo metadata in set_up_workspace_with_multiple_driver_projects");
        let cargo_toml_metadata_clone = cargo_toml_metadata.clone();
        self.mock_metadata_provider
            .expect_get_cargo_metadata_at_path()
            .once()
            .returning(move |_| Ok(cargo_toml_metadata_clone.clone()));
        self.cargo_metadata = Some(cargo_toml_metadata);
        self
    }

    fn set_up_with_custom_toml(
        mut self,
        cargo_toml_metadata: &str,
    ) -> impl TestSetupPackageExpectations {
        let cargo_toml_metadata =
            serde_json::from_str::<cargo_metadata::Metadata>(cargo_toml_metadata)
                .expect("Failed to parse cargo metadata in set_up_with_custom_toml");
        let cargo_toml_metadata_clone = cargo_toml_metadata.clone();
        self.mock_metadata_provider
            .expect_get_cargo_metadata_at_path()
            .once()
            .returning(move |_| Ok(cargo_toml_metadata_clone.clone()));
        self.cargo_metadata = Some(cargo_toml_metadata);
        self
    }

    fn setup_target_dir(&self, dir_path: &Path) -> PathBuf {
        let mut expected_target_dir = dir_path.join("target");

        if let TargetArch::Selected(target_arch) = self.target_arch {
            expected_target_dir = expected_target_dir.join(to_target_triple(target_arch));
        }

        expected_target_dir = match self.profile {
            Some(Profile::Release) => expected_target_dir.join("release"),
            _ => expected_target_dir.join("debug"),
        };
        expected_target_dir
    }
}

impl TestSetupPackageExpectations for TestBuildAction {
    fn expect_root_manifest_exists(mut self, root_dir: &Path, does_exist: bool) -> Self {
        self.mock_fs_provider
            .expect_exists()
            .with(eq(root_dir.join("Cargo.toml")))
            .once()
            .returning(move |_| does_exist);
        self
    }

    fn expect_path_canonicalization_cwd(mut self) -> Self {
        let cwd: PathBuf = self.cwd.clone();
        let expected_cwd = cwd.clone();
        self.mock_fs_provider
            .expect_canonicalize_path()
            .withf(move |d: &Path| d.eq(&expected_cwd))
            .once()
            .returning(move |_| Ok(cwd.clone()));
        self
    }

    fn expect_path_canonicalization_workspace_root(mut self) -> Self {
        let workspace_root_dir: PathBuf = self
            .cargo_metadata
            .as_ref()
            .expect("Cargo metadata must be available")
            .workspace_root
            .clone()
            .into();
        let expected_workspace_root_dir = workspace_root_dir.clone();
        self.mock_fs_provider
            .expect_canonicalize_path()
            .withf(move |d: &Path| d.eq(&expected_workspace_root_dir))
            .once()
            .returning(move |_| Ok(workspace_root_dir.clone()));
        self
    }

    fn expect_path_canonicalization_all_package_roots(mut self) -> Self {
        self.cargo_metadata
            .as_ref()
            .expect("Cargo metadata must be available")
            .workspace_packages()
            .iter()
            .for_each(|package| {
                let package_root_path: PathBuf = package
                    .manifest_path
                    .parent()
                    .expect("Manifest's parent directory must be available")
                    .into();
                let expected_package_root_path = package_root_path.clone();
                self.mock_fs_provider
                    .expect_canonicalize_path()
                    .withf(move |d: &Path| d.eq(&expected_package_root_path))
                    .once()
                    .returning(move |_| Ok(package_root_path.clone()));
            });
        self
    }

    fn expect_path_canonicalization_package_root(mut self, driver_dir: &Path) -> Self {
        let expected_package_root_path = driver_dir.to_owned();
        let package_root_path_to_be_returned = driver_dir.to_owned();
        self.mock_fs_provider
            .expect_canonicalize_path()
            .withf(move |d: &Path| d.eq(&expected_package_root_path))
            .once()
            .returning(move |_| Ok(package_root_path_to_be_returned.clone()));
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

    fn expect_final_package_dir_exists(
        mut self,
        driver_name: &str,
        cwd: &Path,
        does_exist: bool,
    ) -> Self {
        let expected_driver_name_underscored = driver_name.replace('-', "_");
        let expected_target_dir = self.setup_target_dir(cwd);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{expected_driver_name_underscored}_package"));
        self.mock_fs_provider
            .expect_exists()
            .with(eq(expected_final_package_dir_path))
            .once()
            .returning(move |_| does_exist);
        self
    }

    fn expect_dir_created(mut self, driver_name: &str, cwd: &Path, created: bool) -> Self {
        let expected_driver_name_underscored = driver_name.replace('-', "_");
        let expected_target_dir = self.setup_target_dir(cwd);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{expected_driver_name_underscored}_package"));
        self.mock_fs_provider
            .expect_create_dir()
            .with(eq(expected_final_package_dir_path))
            .once()
            .returning(move |_| {
                if created {
                    Ok(())
                } else {
                    Err(Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "create error",
                    ))
                }
            });
        self
    }

    fn expect_path_canonicalization_package_manifest_path(mut self, driver_dir: &Path) -> Self {
        let expected_package_manifest_path = driver_dir.join("Cargo.toml");
        let package_manifest_path_to_be_returned = expected_package_manifest_path.clone();
        self.mock_fs_provider
            .expect_canonicalize_path()
            .withf(move |d: &Path| d.eq(&expected_package_manifest_path))
            .once()
            .returning(move |_| Ok(package_manifest_path_to_be_returned.clone()));
        self
    }

    fn expect_cargo_build(
        mut self,
        driver_name: &str,
        cwd: &Path,
        override_output: Option<Output>,
    ) -> Self {
        // cargo build on the package
        let expected_cargo_command: &'static str = "cargo";
        let manifest_path = cwd
            .join("Cargo.toml")
            .to_string_lossy()
            .trim_start_matches("\\\\?\\")
            .to_string();
        let mut expected_cargo_build_args: Vec<String> = vec![
            "build",
            "-p",
            &driver_name,
            "--manifest-path",
            &manifest_path,
        ]
        .into_iter()
        .map(std::string::ToString::to_string)
        .collect();
        if let Some(profile) = self.profile {
            expected_cargo_build_args.push("--profile".to_string());
            expected_cargo_build_args.push(profile.to_string());
        }

        if let TargetArch::Selected(target_arch) = self.target_arch {
            expected_cargo_build_args.push("--target".to_string());
            expected_cargo_build_args.push(to_target_triple(target_arch));
        }

        expected_cargo_build_args.push("-v".to_string());
        let expected_output = override_output.map_or_else(
            || Output {
                status: ExitStatus::default(),
                stdout: vec![],
                stderr: vec![],
            },
            |output| output,
        );
        self.mock_run_command
            .expect_run()
            .withf(
                move |command: &str,
                      args: &[&str],
                      _env_vars: &Option<&HashMap<&str, &str>>|
                      -> bool {
                    command == expected_cargo_command && args == expected_cargo_build_args
                },
            )
            .once()
            .returning(move |_, _, _| Ok(expected_output.clone()));
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
                eq(expected_src_driver_binary_path),
                eq(expected_dest_driver_binary_path),
            )
            .once()
            .returning(move |_, _| {
                if is_success {
                    Ok(mock_non_zero_bytes_copied_size)
                } else {
                    Err(Error::new(std::io::ErrorKind::UnexpectedEof, "copy error"))
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
                eq(expected_src_driver_pdb_path),
                eq(expected_dest_driver_pdb_path),
            )
            .once()
            .returning(move |_, _| {
                if is_success {
                    Ok(mock_non_zero_bytes_copied_size)
                } else {
                    Err(Error::new(std::io::ErrorKind::UnexpectedEof, "copy error"))
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
                eq(expected_src_driver_inx_path),
                eq(expected_dest_driver_inf_path),
            )
            .once()
            .returning(move |_, _| {
                if is_success {
                    Ok(mock_non_zero_bytes_copied_size)
                } else {
                    Err(Error::new(std::io::ErrorKind::UnexpectedEof, "copy error"))
                }
            });
        self
    }

    fn expect_copy_map_file_to_package_folder(
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

        // copy map file to package directory
        let expected_src_driver_map_path = expected_target_dir
            .join("deps")
            .join(format!("{expected_driver_name_underscored}.map"));
        let expected_dest_driver_map_path =
            expected_final_package_dir_path.join(format!("{expected_driver_name_underscored}.map"));
        self.mock_fs_provider
            .expect_copy()
            .with(
                eq(expected_src_driver_map_path),
                eq(expected_dest_driver_map_path),
            )
            .once()
            .returning(move |_, _| {
                if is_success {
                    Ok(mock_non_zero_bytes_copied_size)
                } else {
                    Err(Error::new(std::io::ErrorKind::UnexpectedEof, "copy error"))
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
                eq(expected_src_cert_file_path),
                eq(expected_dest_driver_cert_path),
            )
            .once()
            .returning(move |_, _| {
                if is_success {
                    Ok(mock_non_zero_bytes_copied_size)
                } else {
                    Err(Error::new(std::io::ErrorKind::UnexpectedEof, "copy error"))
                }
            });
        self
    }

    fn expect_stampinf(
        mut self,
        driver_name: &str,
        driver_dir: &Path,
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

        let target_arch = match self.target_arch {
            TargetArch::Default(target_arch) | TargetArch::Selected(target_arch) => target_arch,
        };

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
                    move |command: &str,
                          args: &[&str],
                          _env_vars: &Option<&HashMap<&str, &str>>|
                          -> bool {
                        println!("command: {command}, args: {args:?}");
                        println!(
                            "expected_command: {expected_stampinf_command}, expected_args: \
                             {expected_stampinf_args:?}"
                        );
                        command == expected_stampinf_command && args == expected_stampinf_args
                    },
                )
                .once()
                .returning(move |_, _, _| match override_output.clone() {
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
        override_output: Option<Output>,
    ) -> Self {
        // Run inf2cat command
        let expected_driver_name_underscored = driver_name.replace('-', "_");
        let expected_target_dir = self.setup_target_dir(driver_dir);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{expected_driver_name_underscored}_package"));

        let expected_inf2cat_command: &'static str = "inf2cat";

        let target_arch = match self.target_arch {
            TargetArch::Default(target_arch) | TargetArch::Selected(target_arch) => target_arch,
        };

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
                move |command: &str,
                      args: &[&str],
                      _env_vars: &Option<&HashMap<&str, &str>>|
                      -> bool {
                    println!("command: {command}, args: {args:?}");
                    println!(
                        "expected_command: {expected_inf2cat_command}, expected_args: \
                         {expected_inf2cat_args:?}"
                    );
                    command == expected_inf2cat_command && args == expected_inf2cat_args
                },
            )
            .once()
            .returning(move |_, _, _| match override_output.clone() {
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
                move |command: &str,
                      args: &[&str],
                      _env_vars: &Option<&HashMap<&str, &str>>|
                      -> bool {
                    command == expected_certmgr_command && args == expected_certmgr_args
                },
            )
            .once()
            .returning(move |_, _, _| match override_output.clone() {
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
                move |command: &str,
                      args: &[&str],
                      _env_vars: &Option<&HashMap<&str, &str>>|
                      -> bool {
                    command == expected_certmgr_command && args == expected_certmgr_args
                },
            )
            .once()
            .returning(move |_, _, _| match override_output.clone() {
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
                move |command: &str,
                      args: &[&str],
                      _env_vars: &Option<&HashMap<&str, &str>>|
                      -> bool {
                    command == expected_makecert_command && args == expected_makecert_args
                },
            )
            .once()
            .returning(move |_, _, _| match override_output.clone() {
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
            .expect_run()
            .withf(
                move |command: &str,
                      args: &[&str],
                      _env_vars: &Option<&HashMap<&str, &str>>|
                      -> bool {
                    command == expected_signtool_command && args == expected_signtool_args
                },
            )
            .once()
            .returning(move |_, _, _| match override_output.clone() {
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
            .expect_run()
            .withf(
                move |command: &str,
                      args: &[&str],
                      _env_vars: &Option<&HashMap<&str, &str>>|
                      -> bool {
                    command == expected_signtool_command && args == expected_signtool_args
                },
            )
            .once()
            .returning(move |_, _, _| match override_output.clone() {
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
                move |command: &str,
                      args: &[&str],
                      _env_vars: &Option<&HashMap<&str, &str>>|
                      -> bool {
                    command == expected_signtool_command && args == expected_signtool_verify_args
                },
            )
            .once()
            .returning(move |_, _, _| match override_output.clone() {
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
                move |command: &str,
                      args: &[&str],
                      _env_vars: &Option<&HashMap<&str, &str>>|
                      -> bool {
                    command == expected_signtool_command && args == expected_signtool_verify_args
                },
            )
            .once()
            .returning(move |_, _, _| match override_output.clone() {
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
        driver_type: &str,
        override_output: Option<Output>,
    ) -> Self {
        let mut expected_infverif_args = vec!["/v".to_string()];
        if driver_type.eq_ignore_ascii_case("KMDF") || driver_type.eq_ignore_ascii_case("WDM") {
            expected_infverif_args.push("/w".to_string());
        } else {
            expected_infverif_args.push("/u".to_string());
        }
        if self.sample_class {
            expected_infverif_args.push("/msft".to_string());
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
                move |command: &str,
                      args: &[&str],
                      _env_vars: &Option<&HashMap<&str, &str>>|
                      -> bool {
                    command == expected_infverif_command && args == expected_infverif_args
                },
            )
            .once()
            .returning(move |_, _, _| match override_output.clone() {
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

    fn mock_wdk_build_provider(&self) -> &WdkBuild {
        &self.mock_wdk_build_provider
    }

    fn mock_run_command(&self) -> &CommandExec {
        &self.mock_run_command
    }

    fn mock_fs_provider(&self) -> &Fs {
        &self.mock_fs_provider
    }

    fn mock_metadata_provider(&self) -> &MetadataProvider {
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
        metadata_section
    )
}

fn get_cargo_metadata_package(
    root_dir: &Path,
    default_package_name: &str,
    default_package_version: &str,
    metadata: Option<TestWdkMetadata>,
) -> (TestMetadataWorkspaceMemberId, TestMetadataPackage) {
    let package_id = format!(
        "path+file:///{}#{}@{}",
        root_dir.to_string_lossy().escape_default(),
        default_package_name,
        default_package_version
    );
    let metadata_section = match metadata {
        Some(metadata) => metadata.0,
        None => String::from("null"),
    };
    (
        TestMetadataWorkspaceMemberId(package_id),
        #[allow(clippy::format_in_format_args)]
        TestMetadataPackage(format!(
            r#"
            {{
            "name": "{}",
            "version": "{}",
            "id": "{}",
            "dependencies": [],
            "targets": [
                {{
                    "kind": [
                        "cdylib"
                    ],
                    "crate_types": [
                        "cdylib"
                    ],
                    "name": "{}",
                    "src_path": "{}",
                    "edition": "2021",
                    "doc": true,
                    "doctest": false,
                    "test": true
                }}
            ],
            "features": {{}},
            "manifest_path": "{}",
            "authors": [],
            "categories": [],
            "keywords": [],
            "edition": "2021",
            "metadata": {}
        }}
        "#,
            default_package_name,
            default_package_version,
            format!(
                "path+file:///{}#{}@{}",
                root_dir.to_string_lossy().escape_default(),
                default_package_name,
                default_package_version
            ),
            default_package_name,
            root_dir
                .join("src")
                .join("main.rs")
                .to_string_lossy()
                .escape_default(),
            root_dir
                .join("Cargo.toml")
                .to_string_lossy()
                .escape_default(),
            metadata_section
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
