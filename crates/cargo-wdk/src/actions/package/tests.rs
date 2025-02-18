use std::{
    any::{Any, TypeId},
    collections::HashMap,
    io::Error,
    os::windows::process::ExitStatusExt,
    path::PathBuf,
    process::{ExitStatus, Output},
    result::Result::Ok,
};

use cargo_metadata::Metadata;
use mockall::predicate::eq;
use wdk_build::{metadata::Wdk, DriverConfig};

use super::PackageAction;
use crate::{
    actions::package::error::PackageProjectError,
    cli::{PackageProjectArgs, TargetArch},
    errors::CommandError,
    providers::{exec::MockRunCommand, fs::MockFSProvider, wdk_build::MockWdkBuildProvider},
};

#[test]
pub fn given_a_simple_driver_project_when_default_values_are_provided_then_it_builds_successfully()
{
    let mut run_command_mock = MockRunCommand::new();
    let mut wdk_build_provider_mock = MockWdkBuildProvider::new();
    let mut fs_provider_mock = MockFSProvider::new();

    // TODO: update output approapriate for each step of packaging
    let output: Output = Output {
        status: ExitStatus::default(),
        stdout: vec![],
        stderr: vec![],
    };

    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = crate::cli::Profile::Debug;
    let target_arch = crate::cli::TargetArch::X86_64;
    let sample_class = true;

    // Emulated driver crate data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, &driver_name, &driver_version, Some(wdk_metadata));
    let cargo_toml_metadata = get_cargo_metadata(&cwd, vec![package], vec![workspace_member], None);
    // println!("cargo_toml_metadata: {}", cargo_toml_metadata);
    let cargo_toml_metadata =
        serde_json::from_str::<cargo_metadata::Metadata>(&cargo_toml_metadata).unwrap();

    // Mock expectation for init
    let cargo_toml_metadata_clone = cargo_toml_metadata.clone();
    wdk_build_provider_mock
        .expect_get_cargo_metadata_at_path()
        .once()
        .returning(move |_| Ok(cargo_toml_metadata_clone.clone()));
    // working dir
    let cwd_clone = cwd.clone();
    let expected_cwd = cwd.clone();
    fs_provider_mock
        .expect_canonicalize_path()
        .withf(move |d: &PathBuf| d.eq(&expected_cwd))
        .once()
        .returning(move |_| Ok(cwd_clone.to_owned()));

    // Mock expectation for run
    // workspace root
    let workspace_root = cargo_toml_metadata.workspace_root.clone();
    let expected_workspace_root = workspace_root.clone();
    fs_provider_mock
        .expect_canonicalize_path()
        .withf(move |d: &PathBuf| d.eq(&expected_workspace_root))
        .once()
        .returning(move |_| Ok(workspace_root.clone().into()));

    // package root path
    let cwd_clone = cwd.clone();
    let expected_cwd = cwd.clone();
    fs_provider_mock
        .expect_canonicalize_path()
        .withf(move |d: &PathBuf| d.eq(&expected_cwd))
        .once()
        .returning(move |_| Ok(cwd_clone.to_owned()));

    // cargo build on the package
    let expected_cargo_command: &'static str = "cargo";
    let expected_cargo_build_args: Vec<&str> = vec!["build", "-v", "-p", &driver_name];
    let expected_output = output.clone();
    run_command_mock
        .expect_run()
        .withf(
            move |command: &str, args: &[&str], _env_vars: &Option<&HashMap<&str, &str>>| -> bool {
                command == expected_cargo_command && args == expected_cargo_build_args
            },
        )
        .once()
        .returning(move |_, _, _| Ok(expected_output.clone()));

    // check if final package directory is created
    let expected_driver_name_underscored = driver_name.replace("-", "_");
    let expected_target_dir = cwd.join("target").join(&profile.to_string());
    let expected_final_package_dir_path =
        expected_target_dir.join(format!("{}_package", expected_driver_name_underscored));
    fs_provider_mock
        .expect_exists()
        .with(eq(expected_final_package_dir_path.clone()))
        .once()
        .returning(|_| true);

    // check if inx file exists
    let expected_inx_file_path = cwd.join(format!("{}.inx", expected_driver_name_underscored));
    fs_provider_mock
        .expect_exists()
        .with(eq(expected_inx_file_path))
        .once()
        .returning(|_| true);

    // rename .dll to .sys
    let expected_src_driver_dll_path =
        expected_target_dir.join(format!("{}.dll", expected_driver_name_underscored));
    let expected_src_driver_sys_path =
        expected_target_dir.join(format!("{}.sys", expected_driver_name_underscored));
    fs_provider_mock
        .expect_rename()
        .with(
            eq(expected_src_driver_dll_path),
            eq(expected_src_driver_sys_path.clone()),
        )
        .once()
        .returning(|_, _| Ok(()));

    // copy .sys to package directory
    let expected_dest_driver_binary_path = expected_final_package_dir_path
        .clone()
        .join(format!("{}.sys", expected_driver_name_underscored));
    let expected_src_driver_binary_path = expected_src_driver_sys_path.clone();
    let mock_non_zero_bytes_copied_size = 1000;
    fs_provider_mock
        .expect_copy()
        .with(
            eq(expected_src_driver_binary_path),
            eq(expected_dest_driver_binary_path.clone()),
        )
        .once()
        .returning(move |_, _| Ok(mock_non_zero_bytes_copied_size));

    // copy pdb file to package directory
    let expected_src_driver_pdb_path =
        expected_target_dir.join(format!("{}.pdb", expected_driver_name_underscored));
    let expected_dest_driver_pdb_path = expected_final_package_dir_path
        .clone()
        .join(format!("{}.pdb", expected_driver_name_underscored));
    fs_provider_mock
        .expect_copy()
        .with(
            eq(expected_src_driver_pdb_path),
            eq(expected_dest_driver_pdb_path.clone()),
        )
        .once()
        .returning(move |_, _| Ok(mock_non_zero_bytes_copied_size));

    // copy inx file to package directory
    let expected_src_driver_inx_path =
        cwd.join(format!("{}.inx", expected_driver_name_underscored));
    let expected_dest_driver_inf_path = expected_final_package_dir_path
        .clone()
        .join(format!("{}.inf", expected_driver_name_underscored));
    fs_provider_mock
        .expect_copy()
        .with(
            eq(expected_src_driver_inx_path),
            eq(expected_dest_driver_inf_path.clone()),
        )
        .once()
        .returning(move |_, _| Ok(mock_non_zero_bytes_copied_size));

    // copy map file to package directory
    let expected_src_driver_map_path = expected_target_dir
        .join("deps")
        .join(format!("{}.map", expected_driver_name_underscored));
    let expected_dest_driver_map_path = expected_final_package_dir_path
        .clone()
        .join(format!("{}.map", expected_driver_name_underscored));
    fs_provider_mock
        .expect_copy()
        .with(
            eq(expected_src_driver_map_path),
            eq(expected_dest_driver_map_path.clone()),
        )
        .once()
        .returning(move |_, _| Ok(mock_non_zero_bytes_copied_size));

    // Run stampinf command
    let expected_stampinf_command: &'static str = "stampinf";
    let wdk_metadata = Wdk::try_from(&cargo_toml_metadata).unwrap();
    let expected_output = output.clone();
    if let DriverConfig::Kmdf(kmdf_config) = wdk_metadata.driver_model {
        let expected_cat_file_name = format!("{}.cat", expected_driver_name_underscored);
        let expected_stampinf_args: Vec<String> = vec![
            "-f".to_string(),
            expected_dest_driver_inf_path
                .clone()
                .to_string_lossy()
                .to_string(),
            "-d".to_string(),
            "*".to_string(),
            "-a".to_string(),
            "amd64".to_string(),
            "-c".to_string(),
            expected_cat_file_name,
            "-v".to_string(),
            "*".to_string(),
            format!(
                "-k {}.{}",
                kmdf_config.kmdf_version_major, kmdf_config.target_kmdf_version_minor
            ),
        ];

        run_command_mock
            .expect_run()
            .withf(
                move |command: &str,
                      args: &[&str],
                      _env_vars: &Option<&HashMap<&str, &str>>|
                      -> bool {
                    command == expected_stampinf_command && args == expected_stampinf_args
                },
            )
            .once()
            .returning(move |_, _, _| Ok(expected_output.to_owned()));
    }

    // Run inf2cat command
    let expected_inf2cat_command: &'static str = "inf2cat";
    let expected_inf2cat_args: Vec<String> = vec![
        format!(
            "/driver:{}",
            expected_final_package_dir_path.to_string_lossy()
        ),
        "/os:10_x64".to_string(),
        "/uselocaltime".to_string(),
    ];
    let expected_output = output.clone();
    run_command_mock
        .expect_run()
        .withf(
            move |command: &str, args: &[&str], _env_vars: &Option<&HashMap<&str, &str>>| -> bool {
                command == expected_inf2cat_command && args == expected_inf2cat_args
            },
        )
        .once()
        .returning(move |_, _, _| Ok(expected_output.to_owned()));

    // check if self signed certificate exists
    let expected_src_driver_cert_path = expected_target_dir.clone().join("WDRLocalTestCert.cer");
    fs_provider_mock
        .expect_exists()
        .with(eq(expected_src_driver_cert_path))
        .once()
        .returning(|_| false);

    // check for cert in cert store using certmgr
    let expected_certmgr_command: &'static str = "certmgr.exe";
    let expected_certmgr_args: Vec<String> = vec!["-s".to_string(), "WDRTestCertStore".to_string()];
    let mut expected_output = output.clone();
    expected_output.stdout = r#"==============No Certificates ==========
                                ==============No CTLs ==========
                                ==============No CRLs ==========
                                ==============================================
                                CertMgr Succeeded"#
        .as_bytes()
        .to_vec();
    run_command_mock
        .expect_run()
        .withf(
            move |command: &str, args: &[&str], _env_vars: &Option<&HashMap<&str, &str>>| -> bool {
                command == expected_certmgr_command && args == expected_certmgr_args
            },
        )
        .once()
        .returning(move |_, _, _| Ok(expected_output.to_owned()));

    // create self signed certificate using makecert
    let expected_makecert_command: &'static str = "makecert";
    let expected_src_driver_cert_path = expected_target_dir.clone().join("WDRLocalTestCert.cer");
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
    let expected_output = output.clone();
    run_command_mock
        .expect_run()
        .withf(
            move |command: &str, args: &[&str], _env_vars: &Option<&HashMap<&str, &str>>| -> bool {
                command == expected_makecert_command && args == expected_makecert_args
            },
        )
        .once()
        .returning(move |_, _, _| Ok(expected_output.to_owned()));

    // copy self signed certificate to package directory
    let expected_src_cert_file_path = expected_target_dir.clone().join("WDRLocalTestCert.cer");
    let expected_dest_driver_cert_path = expected_final_package_dir_path
        .clone()
        .join("WDRLocalTestCert.cer");
    fs_provider_mock
        .expect_copy()
        .with(
            eq(expected_src_cert_file_path),
            eq(expected_dest_driver_cert_path.clone()),
        )
        .once()
        .returning(move |_, _| Ok(mock_non_zero_bytes_copied_size));

    // sign driver binary using signtool
    let expected_signtool_command: &'static str = "signtool";
    let expected_dest_driver_binary_path = expected_final_package_dir_path
        .clone()
        .join(format!("{}.sys", expected_driver_name_underscored));
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
    let expected_output = output.clone();
    run_command_mock
        .expect_run()
        .withf(
            move |command: &str, args: &[&str], _env_vars: &Option<&HashMap<&str, &str>>| -> bool {
                command == expected_signtool_command && args == expected_signtool_args
            },
        )
        .once()
        .returning(move |_, _, _| Ok(expected_output.to_owned()));

    // verify signed driver binary using signtool
    let expected_dest_driver_binary_path = expected_final_package_dir_path
        .clone()
        .join(format!("{}.sys", expected_driver_name_underscored));
    let expected_signtool_verify_args: Vec<String> = vec![
        "verify".to_string(),
        "/v".to_string(),
        "/pa".to_string(),
        expected_dest_driver_binary_path
            .to_string_lossy()
            .to_string(),
    ];
    let expected_output = output.clone();
    run_command_mock
        .expect_run()
        .withf(
            move |command: &str, args: &[&str], _env_vars: &Option<&HashMap<&str, &str>>| -> bool {
                command == expected_signtool_command && args == expected_signtool_verify_args
            },
        )
        .once()
        .returning(move |_, _, _| Ok(expected_output.to_owned()));

    // sign driver cat file using signtool
    let expected_signtool_command: &'static str = "signtool";
    let expected_dest_driver_cat_file_path = expected_final_package_dir_path
        .clone()
        .join(format!("{}.cat", expected_driver_name_underscored));
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
    let expected_output = output.clone();
    run_command_mock
        .expect_run()
        .withf(
            move |command: &str, args: &[&str], _env_vars: &Option<&HashMap<&str, &str>>| -> bool {
                command == expected_signtool_command && args == expected_signtool_args
            },
        )
        .once()
        .returning(move |_, _, _| Ok(expected_output.to_owned()));

    // verify signed driver cat file using signtool
    let expected_dest_driver_cat_file_path = expected_final_package_dir_path
        .clone()
        .join(format!("{}.cat", expected_driver_name_underscored));
    let expected_signtool_verify_args: Vec<String> = vec![
        "verify".to_string(),
        "/v".to_string(),
        "/pa".to_string(),
        expected_dest_driver_cat_file_path
            .to_string_lossy()
            .to_string(),
    ];
    let expected_output = output.clone();
    run_command_mock
        .expect_run()
        .withf(
            move |command: &str, args: &[&str], _env_vars: &Option<&HashMap<&str, &str>>| -> bool {
                command == expected_signtool_command && args == expected_signtool_verify_args
            },
        )
        .once()
        .returning(move |_, _, _| Ok(expected_output.to_owned()));

    // Make sure infverif command is never called for specific build number
    wdk_build_provider_mock
        .expect_detect_wdk_build_number()
        .once()
        .returning(|| Ok(26100));

    // Act
    let def_package_project = PackageProjectArgs {
        cwd: cwd,
        profile,
        target_arch,
        sample_class,
    };
    let package_project = PackageAction::new(
        def_package_project.cwd,
        def_package_project.profile,
        def_package_project.target_arch,
        def_package_project.sample_class,
        clap_verbosity_flag::Verbosity::new(1, 0),
        &wdk_build_provider_mock,
        &run_command_mock,
        &fs_provider_mock,
    );
    // Assert
    assert_eq!(package_project.is_ok(), true);

    // Act
    let run_result = package_project.unwrap().run();

    // Assert
    assert_eq!(run_result.is_ok(), true);
}

fn get_cargo_metadata(
    root_dir: &PathBuf,
    package_list: Vec<String>,
    workspace_member_list: Vec<String>,
    metadata: Option<String>,
) -> String {
    let metadata_section = match metadata {
        Some(metadata) => metadata,
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
        package_list.join(", "),
        // Require quotes around each member
        workspace_member_list
            .iter()
            .map(|s| format!("\"{}\"", s))
            .collect::<Vec<String>>()
            .join(", "),
        metadata_section
    )
}

fn get_cargo_metadata_package(
    root_dir: &PathBuf,
    default_package_name: &str,
    default_package_version: &str,
    metadata: Option<String>,
) -> (String, String) {
    let package_id = format!(
        "path+file:///{}#{}@{}",
        root_dir.to_string_lossy().escape_default(),
        default_package_name,
        default_package_version
    );
    let metadata_section = match metadata {
        Some(metadata) => metadata,
        None => String::from("null"),
    };
    (
        package_id,
        format!(
            r#"
            {{
            "name": "{}",
            "version": "{}",
            "id": "{}",
            "dependencies": [],
            "targets": [
                {{
                    "kind": [
                        "bin"
                    ],
                    "crate_types": [
                        "bin"
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
        ),
    )
}

fn get_cargo_metadata_wdk_metadata(
    driver_type: &str,
    kmdf_version_major: u8,
    target_kmdf_version_minor: u8,
) -> String {
    format!(
        r#"
        {{
            "wdk": {{
                "driver-model": {{
                    "driver-type": "{}",
                    "kmdf-version-major": {},
                    "target-kmdf-version-minor": {}
                }}
            }}
        }}
    "#,
        driver_type, kmdf_version_major, target_kmdf_version_minor
    )
}

// Test name is of form Given When Then
// Given: A driver project
// When: Default values are provided
// Then: It builds successfully
#[test]
pub fn given_a_driver_project_when_default_values_are_provided_then_it_builds_successfully() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = crate::cli::Profile::Debug;
    let target_arch = crate::cli::TargetArch::X86_64;
    let sample_class = true;

    // driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, &driver_name, &driver_version, Some(wdk_metadata));

    let expected_certmgr_output = Output {
        status: ExitStatus::default(),
        stdout: r#"==============No Certificates ==========
                            ==============No CTLs ==========
                            ==============No CRLs ==========
                            ==============================================
                            CertMgr Succeeded"#
            .as_bytes()
            .to_vec(),
        stderr: vec![],
    };

    let package_project = TestPackageAction::new(
        cwd.clone(),
        profile.clone().to_string(),
        &target_arch,
        sample_class,
    );
    let package_project_action = package_project
        .set_up_simple_driver_project((workspace_member, package))
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_package_root()
        .expect_cargo_build(driver_name, None)
        .expect_final_package_dir_exists(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true)
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
        .expect_detect_wdk_build_number(25100u32)
        .expect_infverif(driver_name, &cwd, "KMDF", None);

    let package_project = PackageAction::new(
        cwd,
        profile,
        target_arch,
        sample_class,
        clap_verbosity_flag::Verbosity::new(1, 0),
        package_project_action.mock_wdk_build_provider(),
        package_project_action.mock_run_command(),
        package_project_action.mock_fs_provider(),
    );
    assert_eq!(package_project.is_ok(), true);

    let run_result = package_project.unwrap().run();

    assert_eq!(run_result.is_ok(), true);
}

#[test]
pub fn given_a_driver_project_when_sample_class_is_false_then_it_builds_successfully() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = crate::cli::Profile::Debug;
    let target_arch = crate::cli::TargetArch::X86_64;
    let sample_class = false;

    // driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, &driver_name, &driver_version, Some(wdk_metadata));

    let expected_certmgr_output = Output {
        status: ExitStatus::default(),
        stdout: r#"==============No Certificates ==========
                            ==============No CTLs ==========
                            ==============No CRLs ==========
                            ==============================================
                            CertMgr Succeeded"#
            .as_bytes()
            .to_vec(),
        stderr: vec![],
    };

    let package_project = TestPackageAction::new(
        cwd.clone(),
        profile.clone().to_string(),
        &target_arch,
        sample_class,
    );
    let package_project_action = package_project
        .set_up_simple_driver_project((workspace_member, package))
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_package_root()
        .expect_cargo_build(driver_name, None)
        .expect_final_package_dir_exists(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true)
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

    let package_project = PackageAction::new(
        cwd,
        profile,
        target_arch,
        sample_class,
        clap_verbosity_flag::Verbosity::new(1, 0),
        package_project_action.mock_wdk_build_provider(),
        package_project_action.mock_run_command(),
        package_project_action.mock_fs_provider(),
    );
    assert_eq!(package_project.is_ok(), true);

    let run_result = package_project.unwrap().run();

    assert_eq!(run_result.is_ok(), true);
}

#[test]
pub fn given_a_driver_project_when_profile_is_release_and_target_arch_is_aarch64_then_it_builds_successfully(
) {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = crate::cli::Profile::Release;
    let target_arch = crate::cli::TargetArch::Aarch64;
    let sample_class = false;

    // driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, &driver_name, &driver_version, Some(wdk_metadata));

    let expected_certmgr_output = Output {
        status: ExitStatus::default(),
        stdout: r#"==============No Certificates ==========
                            ==============No CTLs ==========
                            ==============No CRLs ==========
                            ==============================================
                            CertMgr Succeeded"#
            .as_bytes()
            .to_vec(),
        stderr: vec![],
    };

    let package_project = TestPackageAction::new(
        cwd.clone(),
        profile.clone().to_string(),
        &target_arch,
        sample_class,
    );
    let package_project_action = package_project
        .set_up_simple_driver_project((workspace_member, package))
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_package_root()
        .expect_cargo_build(driver_name, None)
        .expect_final_package_dir_exists(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true)
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

    let package_project = PackageAction::new(
        cwd,
        profile,
        target_arch,
        sample_class,
        clap_verbosity_flag::Verbosity::new(1, 0),
        package_project_action.mock_wdk_build_provider(),
        package_project_action.mock_run_command(),
        package_project_action.mock_fs_provider(),
    );
    assert_eq!(package_project.is_ok(), true);

    let run_result = package_project.unwrap().run();

    assert_eq!(run_result.is_ok(), true);
}

#[test]
pub fn given_a_driver_project_when_self_signed_exists_then_it_should_skip_calling_makecert() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = crate::cli::Profile::Debug;
    let target_arch = crate::cli::TargetArch::X86_64;
    let sample_class = true;

    // driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, &driver_name, &driver_version, Some(wdk_metadata));

    let expected_certmgr_output = Output {
        status: ExitStatus::default(),
        stdout: r#"==============Certificate # 1 ==========
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
                    CertMgr Succeeded"#.as_bytes().to_vec(),
        stderr: vec![],
    };

    let expected_create_cert_output = Output {
        status: ExitStatus::default(),
        stdout: vec![],
        stderr: vec![],
    };

    let package_project = TestPackageAction::new(
        cwd.clone(),
        profile.clone().to_string(),
        &target_arch,
        sample_class,
    );
    let package_project_action = package_project
        .set_up_simple_driver_project((workspace_member, package))
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_package_root()
        .expect_cargo_build(driver_name, None)
        .expect_final_package_dir_exists(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true)
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
        .expect_detect_wdk_build_number(25100u32)
        .expect_infverif(driver_name, &cwd, "KMDF", None);

    let package_project = PackageAction::new(
        cwd,
        profile,
        target_arch,
        sample_class,
        clap_verbosity_flag::Verbosity::new(1, 0),
        package_project_action.mock_wdk_build_provider(),
        package_project_action.mock_run_command(),
        package_project_action.mock_fs_provider(),
    );
    assert_eq!(package_project.is_ok(), true);

    let run_result = package_project.unwrap().run();

    assert_eq!(run_result.is_ok(), true);
}

#[test]
pub fn given_a_driver_project_when_final_package_dir_exists_then_it_should_skip_creating_it() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = crate::cli::Profile::Debug;
    let target_arch = crate::cli::TargetArch::X86_64;
    let sample_class = true;

    // driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, &driver_name, &driver_version, Some(wdk_metadata));
    let expected_certmgr_output = Output {
        status: ExitStatus::default(),
        stdout: r#"==============No Certificates ==========
                            ==============No CTLs ==========
                            ==============No CRLs ==========
                            ==============================================
                            CertMgr Succeeded"#
            .as_bytes()
            .to_vec(),
        stderr: vec![],
    };

    let package_project = TestPackageAction::new(
        cwd.clone(),
        profile.clone().to_string(),
        &target_arch,
        sample_class,
    );
    let package_project_action = package_project
        .set_up_simple_driver_project((workspace_member, package))
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_package_root()
        .expect_cargo_build(driver_name, None)
        .expect_final_package_dir_exists(driver_name, &cwd, false)
        .expect_dir_created(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true)
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
        .expect_detect_wdk_build_number(25100u32)
        .expect_infverif(driver_name, &cwd, "KMDF", None);

    let package_project = PackageAction::new(
        cwd,
        profile,
        target_arch,
        sample_class,
        clap_verbosity_flag::Verbosity::new(1, 0),
        package_project_action.mock_wdk_build_provider(),
        package_project_action.mock_run_command(),
        package_project_action.mock_fs_provider(),
    );
    assert_eq!(package_project.is_ok(), true);

    let run_result = package_project.unwrap().run();

    assert_eq!(run_result.is_ok(), true);
}

#[test]
pub fn given_a_driver_project_when_inx_file_do_not_exist_then_package_should_fail() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = crate::cli::Profile::Debug;
    let target_arch = crate::cli::TargetArch::X86_64;
    let sample_class = true;

    // driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, &driver_name, &driver_version, Some(wdk_metadata));

    let package_project = TestPackageAction::new(
        cwd.clone(),
        profile.clone().to_string(),
        &target_arch,
        sample_class,
    );
    let package_project_action = package_project
        .set_up_simple_driver_project((workspace_member, package))
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_package_root()
        .expect_cargo_build(driver_name, None)
        .expect_final_package_dir_exists(driver_name, &cwd, false)
        .expect_dir_created(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, false);

    let package_project = PackageAction::new(
        cwd,
        profile,
        target_arch,
        sample_class,
        clap_verbosity_flag::Verbosity::new(1, 0),
        package_project_action.mock_wdk_build_provider(),
        package_project_action.mock_run_command(),
        package_project_action.mock_fs_provider(),
    );
    assert_eq!(package_project.is_ok(), true);

    let run_result = package_project.unwrap().run();

    assert_eq!(
        run_result.err().unwrap().type_id(),
        TypeId::of::<PackageProjectError>()
    );
}

#[test]
pub fn given_a_driver_project_when_copy_of_an_artifact_fails_then_the_package_should_fail() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = crate::cli::Profile::Debug;
    let target_arch = crate::cli::TargetArch::X86_64;
    let sample_class = true;

    // driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, &driver_name, &driver_version, Some(wdk_metadata));

    let package_project = TestPackageAction::new(
        cwd.clone(),
        profile.clone().to_string(),
        &target_arch,
        sample_class,
    );
    let package_project_action = package_project
        .set_up_simple_driver_project((workspace_member, package))
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_package_root()
        .expect_cargo_build(driver_name, None)
        .expect_final_package_dir_exists(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, false);

    let package_project = PackageAction::new(
        cwd,
        profile,
        target_arch,
        sample_class,
        clap_verbosity_flag::Verbosity::new(1, 0),
        package_project_action.mock_wdk_build_provider(),
        package_project_action.mock_run_command(),
        package_project_action.mock_fs_provider(),
    );
    assert_eq!(package_project.is_ok(), true);

    let run_result = package_project.unwrap().run();

    assert_eq!(
        run_result.err().unwrap().type_id(),
        TypeId::of::<PackageProjectError>()
    );
}

#[test]
pub fn given_a_driver_project_when_stampinf_command_execution_fails_then_package_should_fail() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = crate::cli::Profile::Debug;
    let target_arch = crate::cli::TargetArch::X86_64;
    let sample_class = true;

    // driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, &driver_name, &driver_version, Some(wdk_metadata));

    let expected_stampinf_output = Output {
        status: ExitStatus::from_raw(1),
        stdout: vec![],
        stderr: vec![],
    };

    let package_project = TestPackageAction::new(
        cwd.clone(),
        profile.clone().to_string(),
        &target_arch,
        sample_class,
    );
    let package_project_action = package_project
        .set_up_simple_driver_project((workspace_member, package))
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_package_root()
        .expect_cargo_build(driver_name, None)
        .expect_final_package_dir_exists(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_map_file_to_package_folder(driver_name, &cwd, true)
        .expect_stampinf(driver_name, &cwd, Some(expected_stampinf_output));

    let package_project = PackageAction::new(
        cwd,
        profile,
        target_arch,
        sample_class,
        clap_verbosity_flag::Verbosity::new(1, 0),
        package_project_action.mock_wdk_build_provider(),
        package_project_action.mock_run_command(),
        package_project_action.mock_fs_provider(),
    );
    assert_eq!(package_project.is_ok(), true);

    let run_result = package_project.unwrap().run();

    assert_eq!(
        run_result.err().unwrap().type_id(),
        TypeId::of::<PackageProjectError>()
    );
}

#[test]
pub fn given_a_driver_project_when_inf2cat_command_execution_fails_then_package_should_fail() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = crate::cli::Profile::Debug;
    let target_arch = crate::cli::TargetArch::X86_64;
    let sample_class = true;

    // driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, &driver_name, &driver_version, Some(wdk_metadata));

    let expected_inf2cat_output = Output {
        status: ExitStatus::from_raw(1),
        stdout: vec![],
        stderr: vec![],
    };

    let package_project = TestPackageAction::new(
        cwd.clone(),
        profile.clone().to_string(),
        &target_arch,
        sample_class,
    );
    let package_project_action = package_project
        .set_up_simple_driver_project((workspace_member, package))
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_package_root()
        .expect_cargo_build(driver_name, None)
        .expect_final_package_dir_exists(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_map_file_to_package_folder(driver_name, &cwd, true)
        .expect_stampinf(driver_name, &cwd, None)
        .expect_inf2cat(driver_name, &cwd, Some(expected_inf2cat_output));

    let package_project = PackageAction::new(
        cwd,
        profile,
        target_arch,
        sample_class,
        clap_verbosity_flag::Verbosity::new(1, 0),
        package_project_action.mock_wdk_build_provider(),
        package_project_action.mock_run_command(),
        package_project_action.mock_fs_provider(),
    );
    assert_eq!(package_project.is_ok(), true);

    let run_result = package_project.unwrap().run();

    assert_eq!(
        run_result.err().unwrap().type_id(),
        TypeId::of::<PackageProjectError>()
    );
}

#[test]
pub fn given_a_driver_project_when_certmgr_command_execution_fails_then_package_should_fail() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = crate::cli::Profile::Debug;
    let target_arch = crate::cli::TargetArch::X86_64;
    let sample_class = true;

    // driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, &driver_name, &driver_version, Some(wdk_metadata));

    let expected_output = Output {
        status: ExitStatus::from_raw(1),
        stdout: vec![],
        stderr: vec![],
    };

    let package_project = TestPackageAction::new(
        cwd.clone(),
        profile.clone().to_string(),
        &target_arch,
        sample_class,
    );
    let package_project_action = package_project
        .set_up_simple_driver_project((workspace_member, package))
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_package_root()
        .expect_cargo_build(driver_name, None)
        .expect_final_package_dir_exists(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_map_file_to_package_folder(driver_name, &cwd, true)
        .expect_stampinf(driver_name, &cwd, None)
        .expect_inf2cat(driver_name, &cwd, None)
        .expect_self_signed_cert_file_exists(&cwd, false)
        .expect_certmgr_exists_check(Some(expected_output));

    let package_project = PackageAction::new(
        cwd,
        profile,
        target_arch,
        sample_class,
        clap_verbosity_flag::Verbosity::new(1, 0),
        package_project_action.mock_wdk_build_provider(),
        package_project_action.mock_run_command(),
        package_project_action.mock_fs_provider(),
    );
    assert_eq!(package_project.is_ok(), true);

    let run_result = package_project.unwrap().run();

    assert_eq!(
        run_result.err().unwrap().type_id(),
        TypeId::of::<PackageProjectError>()
    );
}

#[test]
pub fn given_a_driver_project_when_makecert_command_execution_fails_then_package_should_fail() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = crate::cli::Profile::Debug;
    let target_arch = crate::cli::TargetArch::X86_64;
    let sample_class = true;

    // driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, &driver_name, &driver_version, Some(wdk_metadata));

    let expected_output = Output {
        status: ExitStatus::from_raw(1),
        stdout: vec![],
        stderr: vec![],
    };

    let package_project = TestPackageAction::new(
        cwd.clone(),
        profile.clone().to_string(),
        &target_arch,
        sample_class,
    );
    let package_project_action = package_project
        .set_up_simple_driver_project((workspace_member, package))
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_package_root()
        .expect_cargo_build(driver_name, None)
        .expect_final_package_dir_exists(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_map_file_to_package_folder(driver_name, &cwd, true)
        .expect_stampinf(driver_name, &cwd, None)
        .expect_inf2cat(driver_name, &cwd, None)
        .expect_self_signed_cert_file_exists(&cwd, false)
        .expect_certmgr_exists_check(None)
        .expect_makecert(&cwd, Some(expected_output));

    let package_project = PackageAction::new(
        cwd,
        profile,
        target_arch,
        sample_class,
        clap_verbosity_flag::Verbosity::new(1, 0),
        package_project_action.mock_wdk_build_provider(),
        package_project_action.mock_run_command(),
        package_project_action.mock_fs_provider(),
    );
    assert_eq!(package_project.is_ok(), true);

    let run_result = package_project.unwrap().run();

    assert_eq!(
        run_result.err().unwrap().type_id(),
        TypeId::of::<PackageProjectError>()
    );
}

#[test]
pub fn given_a_driver_project_when_signtool_command_execution_fails_then_package_should_fail() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = crate::cli::Profile::Debug;
    let target_arch = crate::cli::TargetArch::X86_64;
    let sample_class = true;

    // driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, &driver_name, &driver_version, Some(wdk_metadata));

    let expected_output = Output {
        status: ExitStatus::from_raw(1),
        stdout: vec![],
        stderr: vec![],
    };

    let package_project = TestPackageAction::new(
        cwd.clone(),
        profile.clone().to_string(),
        &target_arch,
        sample_class,
    );
    let package_project_action = package_project
        .set_up_simple_driver_project((workspace_member, package))
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_package_root()
        .expect_cargo_build(driver_name, None)
        .expect_final_package_dir_exists(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_map_file_to_package_folder(driver_name, &cwd, true)
        .expect_stampinf(driver_name, &cwd, None)
        .expect_inf2cat(driver_name, &cwd, None)
        .expect_self_signed_cert_file_exists(&cwd, false)
        .expect_certmgr_exists_check(None)
        .expect_makecert(&cwd, None)
        .expect_copy_self_signed_cert_file_to_package_folder(driver_name, &cwd, true)
        .expect_signtool_sign_driver_binary_sys_file(driver_name, &cwd, Some(expected_output));

    let package_project = PackageAction::new(
        cwd,
        profile,
        target_arch,
        sample_class,
        clap_verbosity_flag::Verbosity::new(1, 0),
        package_project_action.mock_wdk_build_provider(),
        package_project_action.mock_run_command(),
        package_project_action.mock_fs_provider(),
    );
    assert_eq!(package_project.is_ok(), true);

    let run_result = package_project.unwrap().run();

    assert_eq!(
        run_result.err().unwrap().type_id(),
        TypeId::of::<PackageProjectError>()
    );
}

#[test]
pub fn given_a_driver_project_when_infverif_command_execution_fails_then_package_should_fail() {
    // Input CLI args
    let cwd = PathBuf::from("C:\\tmp");
    let profile = crate::cli::Profile::Debug;
    let target_arch = crate::cli::TargetArch::X86_64;
    let sample_class = true;

    // driver project data
    let driver_type = "KMDF";
    let driver_name = "sample-kmdf";
    let driver_version = "0.0.1";
    let wdk_metadata = get_cargo_metadata_wdk_metadata(driver_type, 1, 33);
    let (workspace_member, package) =
        get_cargo_metadata_package(&cwd, &driver_name, &driver_version, Some(wdk_metadata));

    let expected_output = Output {
        status: ExitStatus::from_raw(1),
        stdout: vec![],
        stderr: vec![],
    };

    let package_project = TestPackageAction::new(
        cwd.clone(),
        profile.clone().to_string(),
        &target_arch,
        sample_class,
    );
    let package_project_action = package_project
        .set_up_simple_driver_project((workspace_member, package))
        .expect_path_canonicalization_cwd()
        .expect_path_canonicalization_workspace_root()
        .expect_path_canonicalization_package_root()
        .expect_cargo_build(driver_name, None)
        .expect_final_package_dir_exists(driver_name, &cwd, true)
        .expect_inx_file_exists(driver_name, &cwd, true)
        .expect_rename_driver_binary_dll_to_sys(driver_name, &cwd)
        .expect_copy_driver_binary_sys_to_package_folder(driver_name, &cwd, true)
        .expect_copy_pdb_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_inx_file_to_package_folder(driver_name, &cwd, true)
        .expect_copy_map_file_to_package_folder(driver_name, &cwd, true)
        .expect_stampinf(driver_name, &cwd, None)
        .expect_inf2cat(driver_name, &cwd, None)
        .expect_self_signed_cert_file_exists(&cwd, false)
        .expect_certmgr_exists_check(None)
        .expect_makecert(&cwd, None)
        .expect_copy_self_signed_cert_file_to_package_folder(driver_name, &cwd, true)
        .expect_signtool_sign_driver_binary_sys_file(driver_name, &cwd, None)
        .expect_signtool_sign_cat_file(driver_name, &cwd, None)
        .expect_signtool_verify_driver_binary_sys_file(driver_name, &cwd, None)
        .expect_signtool_verify_cat_file(driver_name, &cwd, None)
        .expect_detect_wdk_build_number(25100u32)
        .expect_infverif(driver_name, &cwd, "KMDF", Some(expected_output));

    let package_project = PackageAction::new(
        cwd,
        profile,
        target_arch,
        sample_class,
        clap_verbosity_flag::Verbosity::new(1, 0),
        package_project_action.mock_wdk_build_provider(),
        package_project_action.mock_run_command(),
        package_project_action.mock_fs_provider(),
    );
    assert_eq!(package_project.is_ok(), true);

    let run_result = package_project.unwrap().run();

    assert_eq!(
        run_result.err().unwrap().type_id(),
        TypeId::of::<PackageProjectError>()
    );
}

fn from_target_arch_to_command_arg(target_arch: &TargetArch) -> String {
    match target_arch {
        TargetArch::X86_64 => "amd64".to_string(),
        TargetArch::Aarch64 => "arm64".to_string(),
    }
}
struct TestPackageAction {
    cwd: PathBuf,
    profile: String,
    target_arch: String,
    sample_class: bool,

    cargo_metadata: Option<Metadata>,
    inf2cat_arg: String,

    // mocks
    mock_run_command: MockRunCommand,
    mock_wdk_build_provider: MockWdkBuildProvider,
    mock_fs_provider: MockFSProvider,
}

// Presence of method ensures specific mock expectation is set
// Dir argument in any method means to operate on the relevant dir
// Output argument in any method means to override return output from default
// success with no stdout/stderr
trait TestSetupPackageExpectations {
    fn expect_path_canonicalization_package_root(self) -> Self;
    fn expect_path_canonicalization_workspace_root(self) -> Self;
    fn expect_path_canonicalization_cwd(self) -> Self;
    fn expect_self_signed_cert_file_exists(self, driver_dir: &PathBuf, does_exist: bool) -> Self;
    fn expect_final_package_dir_exists(
        self,
        driver_name: &str,
        driver_dir: &PathBuf,
        does_exist: bool,
    ) -> Self;
    fn expect_dir_created(self, driver_name: &str, driver_dir: &PathBuf, created: bool) -> Self;
    fn expect_cargo_build(self, driver_name: &str, override_output: Option<Output>) -> Self;
    fn expect_inx_file_exists(
        self,
        driver_name: &str,
        driver_dir: &PathBuf,
        does_exist: bool,
    ) -> Self;
    fn expect_rename_driver_binary_dll_to_sys(
        self,
        driver_name: &str,
        driver_dir: &PathBuf,
    ) -> Self;
    fn expect_copy_driver_binary_sys_to_package_folder(
        self,
        driver_name: &str,
        driver_dir: &PathBuf,
        is_success: bool,
    ) -> Self;
    fn expect_copy_pdb_file_to_package_folder(
        self,
        driver_name: &str,
        driver_dir: &PathBuf,
        is_success: bool,
    ) -> Self;
    fn expect_copy_inx_file_to_package_folder(
        self,
        driver_name: &str,
        driver_dir: &PathBuf,
        is_success: bool,
    ) -> Self;
    fn expect_copy_map_file_to_package_folder(
        self,
        driver_name: &str,
        driver_dir: &PathBuf,
        is_success: bool,
    ) -> Self;
    fn expect_copy_self_signed_cert_file_to_package_folder(
        self,
        driver_name: &str,
        driver_dir: &PathBuf,
        is_success: bool,
    ) -> Self;

    fn expect_stampinf(
        self,
        driver_name: &str,
        driver_dir: &PathBuf,
        override_output: Option<Output>,
    ) -> Self;
    fn expect_inf2cat(
        self,
        driver_name: &str,
        driver_dir: &PathBuf,
        override_output: Option<Output>,
    ) -> Self;
    fn expect_certmgr_exists_check(self, override_output: Option<Output>) -> Self;
    fn expect_certmgr_create_cert_from_store(
        self,
        driver_dir: &PathBuf,
        override_output: Option<Output>,
    ) -> Self;
    fn expect_makecert(self, driver_dir: &PathBuf, override_output: Option<Output>) -> Self;

    fn expect_signtool_sign_driver_binary_sys_file(
        self,
        driver_name: &str,
        driver_dir: &PathBuf,
        override_output: Option<Output>,
    ) -> Self;
    fn expect_signtool_sign_cat_file(
        self,
        driver_name: &str,
        driver_dir: &PathBuf,
        override_output: Option<Output>,
    ) -> Self;
    fn expect_signtool_verify_driver_binary_sys_file(
        self,
        driver_name: &str,
        driver_dir: &PathBuf,
        override_output: Option<Output>,
    ) -> Self;
    fn expect_signtool_verify_cat_file(
        self,
        driver_name: &str,
        driver_dir: &PathBuf,
        override_output: Option<Output>,
    ) -> Self;

    fn expect_detect_wdk_build_number(self, expected_wdk_build_number: u32) -> Self;
    fn expect_infverif(
        self,
        driver_name: &str,
        driver_dir: &PathBuf,
        driver_type: &str,
        override_output: Option<Output>,
    ) -> Self;

    fn mock_wdk_build_provider(&self) -> &MockWdkBuildProvider;
    fn mock_run_command(&self) -> &MockRunCommand;
    fn mock_fs_provider(&self) -> &MockFSProvider;
}

impl TestPackageAction {
    fn new(cwd: PathBuf, profile: String, target_arch: &TargetArch, sample_class: bool) -> Self {
        let mock_run_command = MockRunCommand::new();
        let mock_wdk_build_provider = MockWdkBuildProvider::new();
        let mock_fs_provider = MockFSProvider::new();
        let command_arg_arch = from_target_arch_to_command_arg(target_arch);
        let command_arg_os_mapping = match target_arch {
            TargetArch::X86_64 => "/os:10_x64",
            TargetArch::Aarch64 => "/os:Server10_arm64",
        };

        Self {
            cwd,
            profile,
            target_arch: command_arg_arch,
            sample_class,
            mock_run_command,
            mock_wdk_build_provider,
            mock_fs_provider,
            cargo_metadata: None,
            inf2cat_arg: command_arg_os_mapping.to_string(),
        }
    }

    fn set_up_simple_driver_project(
        mut self,
        package_metadata: (String, String),
    ) -> impl TestSetupPackageExpectations {
        let cargo_toml_metadata = get_cargo_metadata(
            &self.cwd,
            vec![package_metadata.1],
            vec![package_metadata.0],
            None,
        );
        // println!("cargo_toml_metadata: {}", cargo_toml_metadata);
        let cargo_toml_metadata =
            serde_json::from_str::<cargo_metadata::Metadata>(&cargo_toml_metadata).unwrap();
        let cargo_toml_metadata_clone = cargo_toml_metadata.clone();
        self.mock_wdk_build_provider
            .expect_get_cargo_metadata_at_path()
            .once()
            .returning(move |_| Ok(cargo_toml_metadata_clone.clone()));
        self.cargo_metadata = Some(cargo_toml_metadata.clone());
        self
    }

    fn set_up_workspace_with_multiple_driver_projects(
        self,
        workspace_additional_metadata: String,
        package_metadata_list: Vec<(String, String)>,
    ) -> impl TestSetupPackageExpectations {
        self
    }

    fn set_up_with_custom_toml(
        self,
        cargo_toml_metadata: Metadata,
    ) -> impl TestSetupPackageExpectations {
        self
    }
}

impl TestSetupPackageExpectations for TestPackageAction {
    fn expect_path_canonicalization_cwd(mut self) -> Self {
        let cwd: PathBuf = self.cwd.clone();
        let expected_cwd = cwd.clone();
        self.mock_fs_provider
            .expect_canonicalize_path()
            .withf(move |d: &PathBuf| d.eq(&expected_cwd))
            .once()
            .returning(move |_| Ok(cwd.to_owned()));
        self
    }

    fn expect_path_canonicalization_workspace_root(mut self) -> Self {
        let workspace_root_dir: PathBuf = self
            .cargo_metadata
            .as_ref()
            .unwrap()
            .workspace_root
            .clone()
            .into();
        let expected_workspace_root_dir = workspace_root_dir.clone();
        self.mock_fs_provider
            .expect_canonicalize_path()
            .withf(move |d: &PathBuf| d.eq(&expected_workspace_root_dir))
            .once()
            .returning(move |_| Ok(workspace_root_dir.to_owned()));
        self
    }

    fn expect_path_canonicalization_package_root(mut self) -> Self {
        self.cargo_metadata
            .as_ref()
            .unwrap()
            .workspace_packages()
            .iter()
            .for_each(|package| {
                let package_root_path: PathBuf = package.manifest_path.parent().unwrap().into();
                let expected_package_root_path = package_root_path.clone();
                self.mock_fs_provider
                    .expect_canonicalize_path()
                    .withf(move |d: &PathBuf| d.eq(&expected_package_root_path))
                    .once()
                    .returning(move |_| Ok(package_root_path.to_owned()));
            });
        self
    }

    fn expect_self_signed_cert_file_exists(
        mut self,
        driver_dir: &PathBuf,
        does_exist: bool,
    ) -> Self {
        let expected_target_dir = driver_dir.join("target").join(&self.profile);
        let expected_src_driver_cert_path =
            expected_target_dir.clone().join("WDRLocalTestCert.cer");
        self.mock_fs_provider
            .expect_exists()
            .with(eq(expected_src_driver_cert_path.clone()))
            .once()
            .returning(move |_| does_exist);
        self
    }

    fn expect_final_package_dir_exists(
        mut self,
        driver_name: &str,
        cwd: &PathBuf,
        does_exist: bool,
    ) -> Self {
        let expected_driver_name_underscored = driver_name.replace("-", "_");
        let expected_target_dir = cwd.join("target").join(&self.profile);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{}_package", expected_driver_name_underscored));
        self.mock_fs_provider
            .expect_exists()
            .with(eq(expected_final_package_dir_path.clone()))
            .once()
            .returning(move |_| does_exist);
        self
    }

    fn expect_dir_created(mut self, driver_name: &str, cwd: &PathBuf, created: bool) -> Self {
        let expected_driver_name_underscored = driver_name.replace("-", "_");
        let expected_target_dir = cwd.join("target").join(&self.profile);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{}_package", expected_driver_name_underscored));
        self.mock_fs_provider
            .expect_create_dir()
            .with(eq(expected_final_package_dir_path.clone()))
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

    fn expect_cargo_build(mut self, driver_name: &str, override_output: Option<Output>) -> Self {
        // cargo build on the package
        let expected_cargo_command: &'static str = "cargo";
        let expected_cargo_build_args: Vec<String> = vec!["build", "-v", "-p", &driver_name]
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        let expected_output = match override_output {
            Some(output) => output,
            None => Output {
                status: ExitStatus::default(),
                stdout: vec![],
                stderr: vec![],
            },
        };
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
        driver_dir: &PathBuf,
        does_exist: bool,
    ) -> Self {
        let expected_driver_name_underscored = driver_name.replace("-", "_");
        let expected_inx_file_path =
            driver_dir.join(format!("{}.inx", expected_driver_name_underscored));
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
        driver_dir: &PathBuf,
    ) -> Self {
        let expected_target_dir = driver_dir.join("target").join(&self.profile);
        let expected_driver_name_underscored = driver_name.replace("-", "_");
        let expected_src_driver_dll_path =
            expected_target_dir.join(format!("{}.dll", expected_driver_name_underscored));
        let expected_src_driver_sys_path =
            expected_target_dir.join(format!("{}.sys", expected_driver_name_underscored));
        self.mock_fs_provider
            .expect_rename()
            .with(
                eq(expected_src_driver_dll_path),
                eq(expected_src_driver_sys_path.clone()),
            )
            .once()
            .returning(|_, _| Ok(()));
        self
    }

    fn expect_copy_driver_binary_sys_to_package_folder(
        mut self,
        driver_name: &str,
        driver_dir: &PathBuf,
        is_success: bool,
    ) -> Self {
        let expected_driver_name_underscored = driver_name.replace("-", "_");
        let expected_target_dir = driver_dir.join("target").join(&self.profile);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{}_package", expected_driver_name_underscored));
        let mock_non_zero_bytes_copied_size = 1000u64;

        let expected_src_driver_sys_path =
            expected_target_dir.join(format!("{}.sys", expected_driver_name_underscored));
        let expected_dest_driver_binary_path = expected_final_package_dir_path
            .clone()
            .join(format!("{}.sys", expected_driver_name_underscored));
        let expected_src_driver_binary_path = expected_src_driver_sys_path.clone();
        self.mock_fs_provider
            .expect_copy()
            .with(
                eq(expected_src_driver_binary_path),
                eq(expected_dest_driver_binary_path.clone()),
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
        driver_dir: &PathBuf,
        is_success: bool,
    ) -> Self {
        let expected_driver_name_underscored = driver_name.replace("-", "_");
        let expected_target_dir = driver_dir.join("target").join(&self.profile);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{}_package", expected_driver_name_underscored));
        let mock_non_zero_bytes_copied_size = 1000u64;

        // copy pdb file to package directory
        let expected_src_driver_pdb_path =
            expected_target_dir.join(format!("{}.pdb", expected_driver_name_underscored));
        let expected_dest_driver_pdb_path = expected_final_package_dir_path
            .clone()
            .join(format!("{}.pdb", expected_driver_name_underscored));
        self.mock_fs_provider
            .expect_copy()
            .with(
                eq(expected_src_driver_pdb_path),
                eq(expected_dest_driver_pdb_path.clone()),
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
        driver_dir: &PathBuf,
        is_success: bool,
    ) -> Self {
        let expected_driver_name_underscored = driver_name.replace("-", "_");
        let expected_target_dir = driver_dir.join("target").join(&self.profile);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{}_package", expected_driver_name_underscored));
        let mock_non_zero_bytes_copied_size = 1000u64;

        // copy inx file to package directory
        let expected_src_driver_inx_path =
            driver_dir.join(format!("{}.inx", expected_driver_name_underscored));
        let expected_dest_driver_inf_path = expected_final_package_dir_path
            .clone()
            .join(format!("{}.inf", expected_driver_name_underscored));
        self.mock_fs_provider
            .expect_copy()
            .with(
                eq(expected_src_driver_inx_path),
                eq(expected_dest_driver_inf_path.clone()),
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
        driver_dir: &PathBuf,
        is_success: bool,
    ) -> Self {
        let expected_driver_name_underscored = driver_name.replace("-", "_");
        let expected_target_dir = driver_dir.join("target").join(&self.profile);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{}_package", expected_driver_name_underscored));
        let mock_non_zero_bytes_copied_size = 1000u64;

        // copy map file to package directory
        let expected_src_driver_map_path = expected_target_dir
            .join("deps")
            .join(format!("{}.map", expected_driver_name_underscored));
        let expected_dest_driver_map_path = expected_final_package_dir_path
            .clone()
            .join(format!("{}.map", expected_driver_name_underscored));
        self.mock_fs_provider
            .expect_copy()
            .with(
                eq(expected_src_driver_map_path),
                eq(expected_dest_driver_map_path.clone()),
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
        driver_dir: &PathBuf,
        is_success: bool,
    ) -> Self {
        let expected_driver_name_underscored = driver_name.replace("-", "_");
        let expected_target_dir = driver_dir.join("target").join(&self.profile);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{}_package", expected_driver_name_underscored));
        let mock_non_zero_bytes_copied_size = 1000u64;

        // copy self signed certificate to package directory
        let expected_src_cert_file_path = expected_target_dir.clone().join("WDRLocalTestCert.cer");
        let expected_dest_driver_cert_path = expected_final_package_dir_path
            .clone()
            .join("WDRLocalTestCert.cer");
        self.mock_fs_provider
            .expect_copy()
            .with(
                eq(expected_src_cert_file_path),
                eq(expected_dest_driver_cert_path.clone()),
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
        driver_dir: &PathBuf,
        override_output: Option<Output>,
    ) -> Self {
        // Run stampinf command
        let expected_driver_name_underscored = driver_name.replace("-", "_");
        let expected_target_dir = driver_dir.join("target").join(&self.profile);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{}_package", expected_driver_name_underscored));
        let expected_dest_driver_inf_path = expected_final_package_dir_path
            .clone()
            .join(format!("{}.inf", expected_driver_name_underscored));

        let expected_stampinf_command: &'static str = "stampinf";
        let wdk_metadata = Wdk::try_from(self.cargo_metadata.as_ref().unwrap()).unwrap();

        if let DriverConfig::Kmdf(kmdf_config) = wdk_metadata.driver_model {
            let expected_cat_file_name = format!("{}.cat", expected_driver_name_underscored);
            let expected_stampinf_args: Vec<String> = vec![
                "-f".to_string(),
                expected_dest_driver_inf_path
                    .clone()
                    .to_string_lossy()
                    .to_string(),
                "-d".to_string(),
                "*".to_string(),
                "-a".to_string(),
                self.target_arch.clone(),
                "-c".to_string(),
                expected_cat_file_name,
                "-v".to_string(),
                "*".to_string(),
                format!(
                    "-k {}.{}",
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
                        command == expected_stampinf_command && args == expected_stampinf_args
                    },
                )
                .once()
                .returning(move |_, _, _| match override_output.to_owned() {
                    Some(output) => match output.status.code() {
                        Some(0) => Ok(Output {
                            status: ExitStatus::from_raw(0),
                            stdout: vec![],
                            stderr: vec![],
                        }),
                        _ => Err(CommandError::from_output("stampinf", &vec![], output)),
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
        driver_dir: &PathBuf,
        override_output: Option<Output>,
    ) -> Self {
        // Run inf2cat command
        let expected_driver_name_underscored = driver_name.replace("-", "_");
        let expected_target_dir = driver_dir.join("target").join(&self.profile);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{}_package", expected_driver_name_underscored));

        let expected_inf2cat_command: &'static str = "inf2cat";
        let expected_inf2cat_args: Vec<String> = vec![
            format!(
                "/driver:{}",
                expected_final_package_dir_path.to_string_lossy()
            ),
            self.inf2cat_arg.clone(),
            "/uselocaltime".to_string(),
        ];

        self.mock_run_command
            .expect_run()
            .withf(
                move |command: &str,
                      args: &[&str],
                      _env_vars: &Option<&HashMap<&str, &str>>|
                      -> bool {
                    command == expected_inf2cat_command && args == expected_inf2cat_args
                },
            )
            .once()
            .returning(move |_, _, _| match override_output.to_owned() {
                Some(output) => match output.status.code() {
                    Some(0) => Ok(Output {
                        status: ExitStatus::from_raw(0),
                        stdout: vec![],
                        stderr: vec![],
                    }),
                    _ => Err(CommandError::from_output("inf2cat", &vec![], output)),
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
            .returning(move |_, _, _| match override_output.to_owned() {
                Some(output) => match output.status.code() {
                    Some(0) => Ok(Output {
                        status: ExitStatus::from_raw(0),
                        stdout: output.stdout,
                        stderr: output.stderr,
                    }),
                    _ => Err(CommandError::from_output("certmgr", &vec![], output)),
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
        driver_dir: &PathBuf,
        override_output: Option<Output>,
    ) -> Self {
        // create cert from store using certmgr
        let expected_target_dir = driver_dir.join("target").join(&self.profile);
        let expected_self_signed_cert_file_path =
            expected_target_dir.clone().join("WDRLocalTestCert.cer");

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
            .returning(move |_, _, _| match override_output.to_owned() {
                Some(output) => match output.status.code() {
                    Some(0) => Ok(Output {
                        status: ExitStatus::from_raw(0),
                        stdout: vec![],
                        stderr: vec![],
                    }),
                    _ => Err(CommandError::from_output("certmgr", &vec![], output)),
                },
                None => Ok(Output {
                    status: ExitStatus::default(),
                    stdout: vec![],
                    stderr: vec![],
                }),
            });
        self
    }

    fn expect_makecert(mut self, driver_dir: &PathBuf, override_output: Option<Output>) -> Self {
        // create self signed certificate using makecert
        let expected_target_dir = driver_dir.join("target").join(&self.profile);
        let expected_makecert_command: &'static str = "makecert";
        let expected_src_driver_cert_path =
            expected_target_dir.clone().join("WDRLocalTestCert.cer");
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
            .returning(move |_, _, _| match override_output.to_owned() {
                Some(output) => match output.status.code() {
                    Some(0) => Ok(Output {
                        status: ExitStatus::from_raw(0),
                        stdout: vec![],
                        stderr: vec![],
                    }),
                    _ => Err(CommandError::from_output("makecert", &vec![], output)),
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
        driver_dir: &PathBuf,
        override_output: Option<Output>,
    ) -> Self {
        let expected_driver_name_underscored = driver_name.replace("-", "_");
        let expected_target_dir = driver_dir.join("target").join(&self.profile);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{}_package", expected_driver_name_underscored));
        let expected_signtool_command: &'static str = "signtool";

        // sign driver binary using signtool
        let expected_dest_driver_binary_path = expected_final_package_dir_path
            .clone()
            .join(format!("{}.sys", expected_driver_name_underscored));
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
            .returning(move |_, _, _| match override_output.to_owned() {
                Some(output) => match output.status.code() {
                    Some(0) => Ok(Output {
                        status: ExitStatus::from_raw(0),
                        stdout: vec![],
                        stderr: vec![],
                    }),
                    _ => Err(CommandError::from_output("signtool", &vec![], output)),
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
        driver_dir: &PathBuf,
        override_output: Option<Output>,
    ) -> Self {
        let expected_driver_name_underscored = driver_name.replace("-", "_");
        let expected_target_dir = driver_dir.join("target").join(&self.profile);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{}_package", expected_driver_name_underscored));
        let expected_signtool_command: &'static str = "signtool";

        // sign driver cat file using signtool
        let expected_dest_driver_cat_file_path = expected_final_package_dir_path
            .clone()
            .join(format!("{}.cat", expected_driver_name_underscored));
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
            .returning(move |_, _, _| match override_output.to_owned() {
                Some(output) => match output.status.code() {
                    Some(0) => Ok(Output {
                        status: ExitStatus::from_raw(0),
                        stdout: vec![],
                        stderr: vec![],
                    }),
                    _ => Err(CommandError::from_output("signtool", &vec![], output)),
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
        driver_dir: &PathBuf,
        override_output: Option<Output>,
    ) -> Self {
        let expected_driver_name_underscored = driver_name.replace("-", "_");
        let expected_target_dir = driver_dir.join("target").join(&self.profile);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{}_package", expected_driver_name_underscored));
        let expected_signtool_command: &'static str = "signtool";

        // verify signed driver binary using signtool
        let expected_dest_driver_binary_path = expected_final_package_dir_path
            .clone()
            .join(format!("{}.sys", expected_driver_name_underscored));
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
            .returning(move |_, _, _| match override_output.to_owned() {
                Some(output) => match output.status.code() {
                    Some(0) => Ok(Output {
                        status: ExitStatus::from_raw(0),
                        stdout: vec![],
                        stderr: vec![],
                    }),
                    _ => Err(CommandError::from_output("signtool", &vec![], output)),
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
        driver_dir: &PathBuf,
        override_output: Option<Output>,
    ) -> Self {
        let expected_driver_name_underscored = driver_name.replace("-", "_");
        let expected_target_dir = driver_dir.join("target").join(&self.profile);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{}_package", expected_driver_name_underscored));
        let expected_signtool_command: &'static str = "signtool";

        // verify signed driver cat file using signtool
        let expected_dest_driver_cat_file_path = expected_final_package_dir_path
            .clone()
            .join(format!("{}.cat", expected_driver_name_underscored));
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
            .returning(move |_, _, _| match override_output.to_owned() {
                Some(output) => match output.status.code() {
                    Some(0) => Ok(Output {
                        status: ExitStatus::from_raw(0),
                        stdout: vec![],
                        stderr: vec![],
                    }),
                    _ => Err(CommandError::from_output("stampinf", &vec![], output)),
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
        driver_dir: &PathBuf,
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
        let expected_driver_name_underscored = driver_name.replace("-", "_");
        let expected_target_dir = driver_dir.join("target").join(&self.profile);
        let expected_final_package_dir_path =
            expected_target_dir.join(format!("{}_package", expected_driver_name_underscored));
        let expected_dest_inf_file_path = expected_final_package_dir_path
            .clone()
            .join(format!("{}.inf", expected_driver_name_underscored));
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
            .returning(move |_, _, _| match override_output.to_owned() {
                Some(output) => match output.status.code() {
                    Some(0) => Ok(Output {
                        status: ExitStatus::from_raw(0),
                        stdout: vec![],
                        stderr: vec![],
                    }),
                    _ => Err(CommandError::from_output("infverif", &vec![], output)),
                },
                None => Ok(Output {
                    status: ExitStatus::default(),
                    stdout: vec![],
                    stderr: vec![],
                }),
            });
        self
    }

    fn mock_wdk_build_provider(&self) -> &MockWdkBuildProvider {
        &self.mock_wdk_build_provider
    }

    fn mock_run_command(&self) -> &MockRunCommand {
        &self.mock_run_command
    }

    fn mock_fs_provider(&self) -> &MockFSProvider {
        &self.mock_fs_provider
    }
}
