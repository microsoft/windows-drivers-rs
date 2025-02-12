use mockall::predicate::eq;
use std::{
    collections::HashMap,
    path::PathBuf,
    process::{ExitStatus, Output},
    result::Result::Ok,
};
use wdk_build::{metadata::Wdk, DriverConfig};

use crate::{
    actions::package::error,
    cli::PackageProjectArgs,
    providers::{exec::MockRunCommand, fs::MockFSProvider, wdk_build::MockWdkBuildProvider},
};

use super::PackageAction;

// Test name is of form Given When Then
// Given: A simple driver project
// When: Default values are provided
// Then: It builds successfully
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
        .times(1)
        .returning(move |_| Ok(cargo_toml_metadata_clone.clone()));
    // working dir
    let cwd_clone = cwd.clone();
    let expected_cwd = cwd.clone();
    fs_provider_mock
        .expect_canonicalize_path()
        .withf(move |d: &PathBuf| d.eq(&expected_cwd))
        .times(1)
        .returning(move |_| Ok(cwd_clone.to_owned()));

    // Mock expectation for run
    // workspace root
    let workspace_root = cargo_toml_metadata.workspace_root.clone();
    let expected_workspace_root = workspace_root.clone();
    fs_provider_mock
        .expect_canonicalize_path()
        .withf(move |d: &PathBuf| d.eq(&expected_workspace_root))
        .times(1)
        .returning(move |_| Ok(workspace_root.clone().into()));

    // package root path
    let cwd_clone = cwd.clone();
    let expected_cwd = cwd.clone();
    fs_provider_mock
        .expect_canonicalize_path()
        .withf(move |d: &PathBuf| d.eq(&expected_cwd))
        .times(1)
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
        .times(1)
        .returning(move |_, _, _| Ok(expected_output.clone()));

    // check if final package directory is created
    let expected_driver_name_underscored = driver_name.replace("-", "_");
    let expected_target_dir = cwd.join("target").join(&profile.to_string());
    let expected_final_package_dir_path =
        expected_target_dir.join(format!("{}_package", expected_driver_name_underscored));
    fs_provider_mock
        .expect_exists()
        .with(eq(expected_final_package_dir_path.clone()))
        .times(1)
        .returning(|_| true);

    // check if inx file exists
    let expected_inx_file_path = cwd.join(format!("{}.inx", expected_driver_name_underscored));
    fs_provider_mock
        .expect_exists()
        .with(eq(expected_inx_file_path))
        .times(1)
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
        .times(1)
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
        .times(1)
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
        .times(1)
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
        .times(1)
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
        .times(1)
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
            .times(1)
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
        .times(1)
        .returning(move |_, _, _| Ok(expected_output.to_owned()));

    // Copy map file to package directory
    let expected_src_map_file_path = expected_target_dir
        .join("deps")
        .join(format!("{}.map", expected_driver_name_underscored));
    let expected_dest_map_file_path = expected_final_package_dir_path
        .clone()
        .join(format!("{}.map", expected_driver_name_underscored));
    fs_provider_mock
        .expect_copy()
        .with(
            eq(expected_src_map_file_path),
            eq(expected_dest_map_file_path.clone()),
        )
        .times(1)
        .returning(move |_, _| Ok(mock_non_zero_bytes_copied_size));

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
        .times(1)
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
        .times(1)
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
        .times(1)
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
        .times(1)
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
        .times(1)
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
        .times(1)
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
        .times(1)
        .returning(move |_, _, _| Ok(expected_output.to_owned()));

    // Make sure infverif command is never called for specific build number
    wdk_build_provider_mock
        .expect_detect_wdk_build_number()
        .times(1)
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
    matches!(
        run_result.err(),
        Some(error::PackageProjectError::WdkBuildConfigError(_))
    );
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
