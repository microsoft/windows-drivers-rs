// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
#![allow(clippy::too_many_lines)]
#![allow(clippy::ref_option_ref)]

use std::{
    collections::HashMap,
    fs,
    io,
    os::windows::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::{ExitStatus, Output},
    sync::atomic::{AtomicU64, Ordering},
};

use cargo_metadata::{Message, Metadata as CargoMetadata};
use clap_cargo::Features;
use mockall::predicate::eq;
use wdk_build::{CpuArchitecture, DriverConfig};

use super::{
    BuildAction,
    BuildActionError,
    BuildActionParams,
    SignMode,
    build_task::{BuildTaskParams, MockBuildTaskRunner},
    package_task::{MockPackageTaskRunner, PackageTaskParams},
};
use crate::{
    actions::{Profile, to_target_triple},
    providers::{
        error::{CommandError, FileError},
        exec::MockCommandExec,
        fs::MockFs,
        metadata::MockMetadata,
        wdk_build::MockWdkBuild,
    },
};

const DEFAULT_DRIVER_NAME: &str = "sample-driver";
const DEFAULT_DRIVER_VERSION: &str = "0.0.1";

type PackageSpec<'a> = (
    &'a str,
    PathBuf,
    Option<TestWdkMetadata>,
    &'a str,
    &'a str,
    &'a str,
);

#[test]
fn given_a_driver_project_when_target_arch_is_detected_then_build_action_orchestrates_build_and_package()
 {
    let cwd = PathBuf::from(r"C:\tmp\sample-driver");
    let target_dir = expected_target_dir(&cwd, None, None);
    let mut test_build_action = TestBuildAction::new(
        cwd.clone(),
        None,
        None,
        SignMode::Test {
            verify_signature: false,
        },
        false,
    )
    .set_up_standalone_driver_project(DEFAULT_DRIVER_NAME, Some(default_wdk_metadata()));

    test_build_action.expect_build_runner(
        DEFAULT_DRIVER_NAME,
        &cwd,
        None,
        None,
        Ok(cargo_build_messages(
            DEFAULT_DRIVER_NAME,
            DEFAULT_DRIVER_VERSION,
            &cwd,
            None,
            None,
        )),
    );
    test_build_action.expect_probe_target_arch_using_cargo_rustc(&cwd, CpuArchitecture::Amd64);
    test_build_action.expect_package_runner(
        DEFAULT_DRIVER_NAME,
        &cwd,
        &target_dir,
        CpuArchitecture::Amd64,
        SignMode::Test {
            verify_signature: false,
        },
        false,
    );

    let build_action = initialize_build_action(&mut test_build_action);
    let result = build_action.run_from_workspace_root(&cwd);

    assert!(
        result.is_ok(),
        "build action failed unexpectedly: {result:?}"
    );
}

#[test]
fn given_explicit_target_arch_when_building_then_probe_is_skipped_and_package_runner_uses_it() {
    let cwd = PathBuf::from(r"C:\tmp\sample-driver");
    let profile = Some(Profile::Release);
    let target_arch = CpuArchitecture::Arm64;
    let target_dir = expected_target_dir(&cwd, Some(target_arch), profile);
    let target_triple = to_target_triple(target_arch);
    let mut test_build_action = TestBuildAction::new(
        cwd.clone(),
        profile,
        Some(target_arch),
        SignMode::Test {
            verify_signature: true,
        },
        true,
    )
    .set_up_standalone_driver_project(DEFAULT_DRIVER_NAME, Some(default_wdk_metadata()));

    test_build_action.expect_build_runner(
        DEFAULT_DRIVER_NAME,
        &cwd,
        profile,
        Some(target_arch),
        Ok(cargo_build_messages(
            DEFAULT_DRIVER_NAME,
            DEFAULT_DRIVER_VERSION,
            &cwd,
            Some(target_triple.as_str()),
            profile,
        )),
    );
    test_build_action.expect_package_runner(
        DEFAULT_DRIVER_NAME,
        &cwd,
        &target_dir,
        target_arch,
        SignMode::Test {
            verify_signature: true,
        },
        true,
    );

    let build_action = initialize_build_action(&mut test_build_action);
    let result = build_action.run_from_workspace_root(&cwd);

    assert!(
        result.is_ok(),
        "build action failed unexpectedly: {result:?}"
    );
}

#[test]
fn given_a_non_driver_package_when_building_then_package_runner_is_skipped() {
    let cwd = PathBuf::from(r"C:\tmp\non-driver");
    let mut test_build_action = TestBuildAction::new(
        cwd.clone(),
        None,
        None,
        SignMode::Test {
            verify_signature: false,
        },
        false,
    )
    .set_up_standalone_driver_project(DEFAULT_DRIVER_NAME, None);

    test_build_action.expect_build_runner(
        DEFAULT_DRIVER_NAME,
        &cwd,
        None,
        None,
        Ok(cargo_build_messages(
            DEFAULT_DRIVER_NAME,
            DEFAULT_DRIVER_VERSION,
            &cwd,
            None,
            None,
        )),
    );
    test_build_action
        .mock_package_task_runner
        .expect_run()
        .never();

    let build_action = initialize_build_action(&mut test_build_action);
    let result = build_action.run_from_workspace_root(&cwd);

    assert!(
        result.is_ok(),
        "build action failed unexpectedly: {result:?}"
    );
}

#[test]
fn given_a_driver_package_without_cdylib_when_building_then_package_runner_is_skipped() {
    let cwd = PathBuf::from(r"C:\tmp\driver-lib");
    let mut test_build_action = TestBuildAction::new(
        cwd.clone(),
        None,
        None,
        SignMode::Test {
            verify_signature: false,
        },
        false,
    )
    .set_up_with_custom_toml(&get_cargo_metadata(
        &cwd,
        vec![
            get_cargo_metadata_package_with_target(
                &cwd,
                DEFAULT_DRIVER_NAME,
                DEFAULT_DRIVER_VERSION,
                Some(default_wdk_metadata()),
                "lib",
                "lib",
                "lib.rs",
            )
            .1,
        ],
        &[get_cargo_metadata_package_with_target(
            &cwd,
            DEFAULT_DRIVER_NAME,
            DEFAULT_DRIVER_VERSION,
            Some(default_wdk_metadata()),
            "lib",
            "lib",
            "lib.rs",
        )
        .0],
        None,
    ));

    test_build_action.expect_build_runner(DEFAULT_DRIVER_NAME, &cwd, None, None, Ok(Vec::new()));
    test_build_action
        .mock_package_task_runner
        .expect_run()
        .never();

    let build_action = initialize_build_action(&mut test_build_action);
    let result = build_action.run_from_workspace_root(&cwd);

    assert!(
        result.is_ok(),
        "build action failed unexpectedly: {result:?}"
    );
}

#[test]
fn given_a_workspace_root_when_one_member_fails_then_workspace_error_is_returned() {
    let workspace_root = PathBuf::from(r"C:\tmp\workspace");
    let driver_name_1 = "sample-kmdf-1";
    let driver_name_2 = "sample-kmdf-2";
    let driver_dir_1 = workspace_root.join(driver_name_1);
    let driver_dir_2 = workspace_root.join(driver_name_2);
    let target_arch = CpuArchitecture::Amd64;
    let target_dir_1 = expected_target_dir(&driver_dir_1, Some(target_arch), None);
    let target_triple = to_target_triple(target_arch);
    let mut test_build_action = TestBuildAction::new(
        workspace_root.clone(),
        None,
        Some(target_arch),
        SignMode::Test {
            verify_signature: false,
        },
        false,
    )
    .set_up_workspace_with_multiple_driver_projects(
        &workspace_root,
        vec![
            (
                driver_name_1,
                driver_dir_1.clone(),
                Some(default_wdk_metadata()),
                "cdylib",
                "cdylib",
                "main.rs",
            ),
            (
                driver_name_2,
                driver_dir_2.clone(),
                Some(default_wdk_metadata()),
                "cdylib",
                "cdylib",
                "main.rs",
            ),
        ],
    );

    test_build_action.expect_build_runner(
        driver_name_1,
        &driver_dir_1,
        None,
        Some(target_arch),
        Ok(cargo_build_messages(
            driver_name_1,
            DEFAULT_DRIVER_VERSION,
            &driver_dir_1,
            Some(target_triple.as_str()),
            None,
        )),
    );
    test_build_action.expect_build_runner(
        driver_name_2,
        &driver_dir_2,
        None,
        Some(target_arch),
        Err(build_task_error()),
    );
    test_build_action.expect_package_runner(
        driver_name_1,
        &driver_dir_1,
        &target_dir_1,
        target_arch,
        SignMode::Test {
            verify_signature: false,
        },
        false,
    );

    let build_action = initialize_build_action(&mut test_build_action);
    let result = build_action.run_from_workspace_root(&workspace_root);

    assert!(matches!(
        result,
        Err(BuildActionError::OneOrMoreWorkspaceMembersFailedToBuild(path))
        if path == workspace_root
    ));
}

#[test]
fn given_a_workspace_member_directory_when_building_then_only_that_member_is_orchestrated() {
    let workspace_root = PathBuf::from(r"C:\tmp\workspace");
    let driver_name_1 = "sample-kmdf-1";
    let driver_name_2 = "sample-kmdf-2";
    let driver_dir_1 = workspace_root.join(driver_name_1);
    let driver_dir_2 = workspace_root.join(driver_name_2);
    let target_arch = CpuArchitecture::Amd64;
    let target_dir_2 = expected_target_dir(&driver_dir_2, Some(target_arch), None);
    let target_triple = to_target_triple(target_arch);
    let mut test_build_action = TestBuildAction::new(
        driver_dir_2.clone(),
        None,
        Some(target_arch),
        SignMode::Test {
            verify_signature: false,
        },
        false,
    )
    .set_up_workspace_with_multiple_driver_projects(
        &workspace_root,
        vec![
            (
                driver_name_1,
                driver_dir_1,
                Some(default_wdk_metadata()),
                "cdylib",
                "cdylib",
                "main.rs",
            ),
            (
                driver_name_2,
                driver_dir_2.clone(),
                Some(default_wdk_metadata()),
                "cdylib",
                "cdylib",
                "main.rs",
            ),
        ],
    );

    test_build_action.expect_build_runner(
        driver_name_2,
        &driver_dir_2,
        None,
        Some(target_arch),
        Ok(cargo_build_messages(
            driver_name_2,
            DEFAULT_DRIVER_VERSION,
            &driver_dir_2,
            Some(target_triple.as_str()),
            None,
        )),
    );
    test_build_action.expect_package_runner(
        driver_name_2,
        &driver_dir_2,
        &target_dir_2,
        target_arch,
        SignMode::Test {
            verify_signature: false,
        },
        false,
    );

    let build_action = initialize_build_action(&mut test_build_action);
    let result = build_action.run_from_workspace_root(&driver_dir_2);

    assert!(
        result.is_ok(),
        "build action failed unexpectedly: {result:?}"
    );
}

#[test]
fn given_an_emulated_workspace_when_running_then_each_valid_project_is_built() {
    let emulated_workspace = TestWorkspaceRoot::new("build-action-emulated-workspace");
    let driver_name_1 = "driver-a";
    let driver_name_2 = "driver-b";
    let ignored_dir = "docs";
    let driver_dir_1 = emulated_workspace.root.join(driver_name_1);
    let driver_dir_2 = emulated_workspace.root.join(driver_name_2);
    let ignored_dir_path = emulated_workspace.root.join(ignored_dir);
    fs::create_dir_all(&driver_dir_1).expect("failed to create driver-a directory");
    fs::create_dir_all(&driver_dir_2).expect("failed to create driver-b directory");
    fs::create_dir_all(&ignored_dir_path).expect("failed to create docs directory");

    let target_arch = CpuArchitecture::Amd64;
    let target_dir_1 = expected_target_dir(&driver_dir_1, Some(target_arch), None);
    let target_dir_2 = expected_target_dir(&driver_dir_2, Some(target_arch), None);
    let target_triple = to_target_triple(target_arch);
    let mut test_build_action = TestBuildAction::new(
        emulated_workspace.root.clone(),
        None,
        Some(target_arch),
        SignMode::Test {
            verify_signature: false,
        },
        false,
    );

    test_build_action.expect_detect_wdk_build_number(25100);
    test_build_action.expect_root_manifest_exists(&emulated_workspace.root, false);
    test_build_action.expect_read_dir_entries(&emulated_workspace.root);
    test_build_action.expect_dir_cargo_toml_exists(&driver_dir_1, true);
    test_build_action.expect_dir_cargo_toml_exists(&driver_dir_1, true);
    test_build_action.expect_dir_cargo_toml_exists(&driver_dir_2, true);
    test_build_action.expect_dir_cargo_toml_exists(&ignored_dir_path, false);
    test_build_action.expect_dir_cargo_toml_exists(&ignored_dir_path, false);
    test_build_action.expect_metadata_for_paths(vec![
        (
            driver_dir_1.clone(),
            metadata_from_packages(
                &driver_dir_1,
                vec![(
                    driver_name_1,
                    driver_dir_1.clone(),
                    Some(default_wdk_metadata()),
                    "cdylib",
                    "cdylib",
                    "main.rs",
                )],
            ),
        ),
        (
            driver_dir_2.clone(),
            metadata_from_packages(
                &driver_dir_2,
                vec![(
                    driver_name_2,
                    driver_dir_2.clone(),
                    Some(default_wdk_metadata()),
                    "cdylib",
                    "cdylib",
                    "main.rs",
                )],
            ),
        ),
    ]);
    test_build_action.expect_build_runner(
        driver_name_1,
        &driver_dir_1,
        None,
        Some(target_arch),
        Ok(cargo_build_messages(
            driver_name_1,
            DEFAULT_DRIVER_VERSION,
            &driver_dir_1,
            Some(target_triple.as_str()),
            None,
        )),
    );
    test_build_action.expect_build_runner(
        driver_name_2,
        &driver_dir_2,
        None,
        Some(target_arch),
        Ok(cargo_build_messages(
            driver_name_2,
            DEFAULT_DRIVER_VERSION,
            &driver_dir_2,
            Some(target_triple.as_str()),
            None,
        )),
    );
    test_build_action.expect_package_runner(
        driver_name_1,
        &driver_dir_1,
        &target_dir_1,
        target_arch,
        SignMode::Test {
            verify_signature: false,
        },
        false,
    );
    test_build_action.expect_package_runner(
        driver_name_2,
        &driver_dir_2,
        &target_dir_2,
        target_arch,
        SignMode::Test {
            verify_signature: false,
        },
        false,
    );

    let build_action = initialize_build_action(&mut test_build_action);
    let result = crate::test_utils::with_env::<&str, &str, _, _>(&[], || build_action.run());

    assert!(
        result.is_ok(),
        "build action failed unexpectedly: {result:?}"
    );
}

#[test]
fn given_a_workspace_member_when_build_runner_fails_then_the_build_error_is_propagated() {
    let workspace_root = PathBuf::from(r"C:\tmp\workspace");
    let cwd = workspace_root.join(DEFAULT_DRIVER_NAME);
    let mut test_build_action = TestBuildAction::new(
        cwd.clone(),
        None,
        None,
        SignMode::Test {
            verify_signature: false,
        },
        false,
    )
    .set_up_workspace_with_multiple_driver_projects(
        &workspace_root,
        vec![(
            DEFAULT_DRIVER_NAME,
            cwd.clone(),
            Some(default_wdk_metadata()),
            "cdylib",
            "cdylib",
            "main.rs",
        )],
    );

    test_build_action.expect_build_runner(
        DEFAULT_DRIVER_NAME,
        &cwd,
        None,
        None,
        Err(build_task_error()),
    );

    let build_action = initialize_build_action(&mut test_build_action);
    let result = build_action.run_from_workspace_root(&cwd);

    assert!(matches!(result, Err(BuildActionError::BuildTask(_))));
}

fn initialize_build_action(test_build_action: &mut TestBuildAction) -> BuildAction<'_> {
    BuildAction::new_with_runners(
        &BuildActionParams {
            working_dir: &test_build_action.cwd,
            profile: test_build_action.profile.as_ref(),
            target_arch: test_build_action.target_arch,
            sign_mode: test_build_action.sign_mode,
            is_sample_class: test_build_action.sample_class,
            locked: test_build_action.locked,
            features: &test_build_action.features,
            verbosity_level: clap_verbosity_flag::Verbosity::new(1, 0),
        },
        &test_build_action.mock_wdk_build_provider,
        &test_build_action.mock_run_command,
        &test_build_action.mock_fs_provider,
        &test_build_action.mock_metadata_provider,
        std::mem::take(&mut test_build_action.mock_build_task_runner),
        std::mem::take(&mut test_build_action.mock_package_task_runner),
    )
    .expect("failed to initialize build action")
}

fn default_wdk_metadata() -> TestWdkMetadata {
    get_cargo_metadata_wdk_metadata("KMDF", 1, 33)
}

fn expected_target_dir(
    cwd: &Path,
    target_arch: Option<CpuArchitecture>,
    profile: Option<Profile>,
) -> PathBuf {
    let mut target_dir = cwd.join("target");
    if let Some(target_arch) = target_arch {
        target_dir = target_dir.join(to_target_triple(target_arch));
    }
    target_dir.join(match profile {
        Some(Profile::Release) => "release",
        _ => "debug",
    })
}

fn cargo_build_messages(
    package_name: &str,
    package_version: &str,
    cwd: &Path,
    target_triple: Option<&str>,
    profile: Option<Profile>,
) -> Vec<Result<Message, io::Error>> {
    let output =
        create_cargo_build_output_json(package_name, package_version, cwd, target_triple, profile);
    Message::parse_stream(io::Cursor::new(output.stdout)).collect()
}

fn build_task_error() -> super::error::BuildTaskError {
    super::error::BuildTaskError::CargoBuild(CommandError::from_output(
        "cargo",
        &["build"],
        &Output {
            status: ExitStatus::from_raw(1),
            stdout: vec![],
            stderr: b"cargo build failed".to_vec(),
        },
    ))
}

struct TestBuildAction {
    cwd: PathBuf,
    profile: Option<Profile>,
    target_arch: Option<CpuArchitecture>,
    sign_mode: SignMode,
    sample_class: bool,
    locked: bool,
    features: Features,
    mock_run_command: MockCommandExec,
    mock_wdk_build_provider: MockWdkBuild,
    mock_fs_provider: MockFs,
    mock_metadata_provider: MockMetadata,
    mock_build_task_runner: MockBuildTaskRunner,
    mock_package_task_runner: MockPackageTaskRunner,
}

impl TestBuildAction {
    fn new(
        cwd: PathBuf,
        profile: Option<Profile>,
        target_arch: Option<CpuArchitecture>,
        sign_mode: SignMode,
        sample_class: bool,
    ) -> Self {
        Self {
            cwd,
            profile,
            target_arch,
            sign_mode,
            sample_class,
            locked: false,
            features: Features::default(),
            mock_run_command: MockCommandExec::new(),
            mock_wdk_build_provider: MockWdkBuild::new(),
            mock_fs_provider: MockFs::new(),
            mock_metadata_provider: MockMetadata::new(),
            mock_build_task_runner: MockBuildTaskRunner::new(),
            mock_package_task_runner: MockPackageTaskRunner::new(),
        }
    }

    fn set_up_standalone_driver_project(
        mut self,
        package_name: &str,
        metadata: Option<TestWdkMetadata>,
    ) -> Self {
        let is_driver = metadata.is_some();
        let cargo_toml_metadata = metadata_from_packages(
            &self.cwd,
            vec![(
                package_name,
                self.cwd.clone(),
                metadata,
                if is_driver { "cdylib" } else { "lib" },
                if is_driver { "cdylib" } else { "lib" },
                if is_driver { "main.rs" } else { "lib.rs" },
            )],
        );
        let cargo_toml_metadata_for_closure = cargo_toml_metadata;
        self.mock_metadata_provider
            .expect_get_cargo_metadata_at_path()
            .once()
            .returning(move |_, _, _| Ok(cargo_toml_metadata_for_closure.clone()));
        self
    }

    fn set_up_workspace_with_multiple_driver_projects(
        mut self,
        workspace_root_dir: &Path,
        packages: Vec<PackageSpec<'_>>,
    ) -> Self {
        let cargo_toml_metadata = metadata_from_packages(workspace_root_dir, packages);
        let cargo_toml_metadata_for_closure = cargo_toml_metadata;
        self.mock_metadata_provider
            .expect_get_cargo_metadata_at_path()
            .once()
            .returning(move |_, _, _| Ok(cargo_toml_metadata_for_closure.clone()));
        self
    }

    fn set_up_with_custom_toml(mut self, cargo_toml_metadata: &str) -> Self {
        let cargo_toml_metadata = serde_json::from_str::<CargoMetadata>(cargo_toml_metadata)
            .expect("failed to parse cargo metadata");
        let cargo_toml_metadata_for_closure = cargo_toml_metadata;
        self.mock_metadata_provider
            .expect_get_cargo_metadata_at_path()
            .once()
            .returning(move |_, _, _| Ok(cargo_toml_metadata_for_closure.clone()));
        self
    }

    fn expect_metadata_for_paths(&mut self, metadata_by_path: Vec<(PathBuf, CargoMetadata)>) {
        self.mock_metadata_provider
            .expect_get_cargo_metadata_at_path()
            .times(metadata_by_path.len())
            .returning(move |path, _other_options, _features| {
                let requested_path = PathBuf::from(path);
                metadata_by_path
                    .iter()
                    .find_map(|(expected_path, metadata)| {
                        (requested_path == *expected_path).then_some(metadata.clone())
                    })
                    .ok_or_else(|| {
                        cargo_metadata::Error::from(io::Error::new(
                            io::ErrorKind::NotFound,
                            format!("unexpected metadata path: {}", requested_path.display()),
                        ))
                    })
            });
    }

    fn expect_detect_wdk_build_number(&mut self, build_number: u32) {
        self.mock_wdk_build_provider
            .expect_detect_wdk_build_number()
            .once()
            .returning(move || Ok(build_number));
    }

    fn expect_root_manifest_exists(&mut self, root_dir: &Path, exists: bool) {
        let cargo_toml_path = root_dir.join("Cargo.toml");
        self.mock_fs_provider
            .expect_exists()
            .with(eq(cargo_toml_path))
            .once()
            .returning(move |_| exists);
    }

    fn expect_read_dir_entries(&mut self, root_dir: &Path) {
        let root_dir = root_dir.to_path_buf();
        self.mock_fs_provider
            .expect_read_dir_entries()
            .with(eq(root_dir.clone()))
            .once()
            .returning(move |_| {
                let mut entries: Vec<crate::providers::fs::DirEntryInfo> = fs::read_dir(&root_dir)
                    .map_err(|e| FileError::ReadDirError(root_dir.clone(), e))?
                    .map(|entry_res| {
                        let entry = entry_res
                            .map_err(|e| FileError::ReadDirEntriesError(root_dir.clone(), e))?;
                        let entry_path = entry.path();
                        let is_dir = entry
                            .file_type()
                            .map_err(|e| FileError::DirFileTypeError(entry_path.clone(), e))?
                            .is_dir();
                        Ok(crate::providers::fs::DirEntryInfo {
                            path: entry_path,
                            is_dir,
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                entries.sort_by(|a, b| a.path.cmp(&b.path));
                Ok(entries)
            });
    }

    fn expect_dir_cargo_toml_exists(&mut self, dir: &Path, exists: bool) {
        let cargo_toml_path = dir.join("Cargo.toml");
        self.mock_fs_provider
            .expect_exists()
            .with(eq(cargo_toml_path))
            .once()
            .returning(move |_| exists);
    }

    fn expect_build_runner(
        &mut self,
        package_name: &str,
        working_dir: &Path,
        profile: Option<Profile>,
        target_arch: Option<CpuArchitecture>,
        result: Result<Vec<Result<Message, io::Error>>, super::error::BuildTaskError>,
    ) {
        let expected_package_name = package_name.to_string();
        let expected_working_dir = working_dir.to_path_buf();
        let expected_profile = profile;
        let expected_target_arch = target_arch;
        let expected_locked = self.locked;
        let expected_features = self.features.clone();
        self.mock_build_task_runner
            .expect_run()
            .withf(move |params: &BuildTaskParams<'_>, _command_exec| {
                params.package_name == expected_package_name
                    && params.working_dir == expected_working_dir
                    && params.profile.copied() == expected_profile
                    && params.target_arch == expected_target_arch
                    && params.locked == expected_locked
                    && params.features.all_features == expected_features.all_features
                    && params.features.no_default_features == expected_features.no_default_features
                    && params.features.features == expected_features.features
            })
            .once()
            .return_once(move |_, _| result);
    }

    fn expect_package_runner(
        &mut self,
        package_name: &str,
        working_dir: &Path,
        target_dir: &Path,
        target_arch: CpuArchitecture,
        sign_mode: SignMode,
        sample_class: bool,
    ) {
        let expected_package_name = package_name.to_string();
        let expected_working_dir = working_dir.to_path_buf();
        let expected_target_dir = target_dir.to_path_buf();
        self.mock_package_task_runner
            .expect_run()
            .withf(move |params: &PackageTaskParams<'_>, _, _, _| {
                params.package_name == expected_package_name
                    && params.working_dir == expected_working_dir
                    && params.target_dir == expected_target_dir
                    && *params.target_arch == target_arch
                    && params.sign_mode == sign_mode
                    && params.sample_class == sample_class
                    && matches!(params.driver_model, DriverConfig::Kmdf(_))
            })
            .once()
            .return_once(|_, _, _, _| Ok(()));
    }

    fn expect_probe_target_arch_using_cargo_rustc(
        &mut self,
        working_dir: &Path,
        detected_arch: CpuArchitecture,
    ) {
        let expected_working_dir = working_dir.to_path_buf();
        let arch_str = match detected_arch {
            CpuArchitecture::Amd64 => "x86_64",
            CpuArchitecture::Arm64 => "aarch64",
        };
        let mut expected_args: Vec<String> = vec!["rustc".to_string()];
        if self.locked {
            expected_args.push("--locked".to_string());
        }
        expected_args.extend(super::features_to_cargo_args(&self.features));
        expected_args.extend(["--".to_string(), "--print".to_string(), "cfg".to_string()]);
        self.mock_run_command
            .expect_run()
            .withf(
                move |command: &str,
                      args: &[&str],
                      _env_vars: &Option<&HashMap<&str, &str>>,
                      working_dir: &Option<&Path>| {
                    let expected_refs: Vec<&str> =
                        expected_args.iter().map(String::as_str).collect();
                    command == "cargo"
                        && args == expected_refs.as_slice()
                        && working_dir.is_some_and(|dir| dir == expected_working_dir.as_path())
                },
            )
            .once()
            .returning(move |_, _, _, _| {
                Ok(Output {
                    status: ExitStatus::default(),
                    stdout: format!("target_arch=\"{arch_str}\"\n").into_bytes(),
                    stderr: vec![],
                })
            });
    }
}

struct TestWorkspaceRoot {
    root: PathBuf,
}

impl TestWorkspaceRoot {
    fn new(prefix: &str) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let unique_id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-artifacts")
            .join(format!("{prefix}-{unique_id}"));
        if root.exists() {
            fs::remove_dir_all(&root).expect("failed to remove stale test workspace");
        }
        fs::create_dir_all(&root).expect("failed to create test workspace");
        Self { root }
    }
}

impl Drop for TestWorkspaceRoot {
    fn drop(&mut self) {
        if self.root.exists() {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

fn metadata_from_packages(
    workspace_root_dir: &Path,
    packages: Vec<PackageSpec<'_>>,
) -> CargoMetadata {
    let mut package_json = Vec::new();
    let mut workspace_member_ids = Vec::new();
    for (package_name, package_dir, metadata, target_kind, crate_type, source_file) in packages {
        let (workspace_member_id, package) = get_cargo_metadata_package_with_target(
            &package_dir,
            package_name,
            DEFAULT_DRIVER_VERSION,
            metadata,
            target_kind,
            crate_type,
            source_file,
        );
        workspace_member_ids.push(workspace_member_id);
        package_json.push(package);
    }
    serde_json::from_str::<CargoMetadata>(&get_cargo_metadata(
        workspace_root_dir,
        package_json,
        &workspace_member_ids,
        None,
    ))
    .expect("failed to parse cargo metadata")
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
        workspace_member_list
            .iter()
            .map(|s| format!("\"{}\"", s.0))
            .collect::<Vec<String>>()
            .join(", "),
        metadata_section
    )
}

fn get_cargo_metadata_package_with_target(
    root_dir: &Path,
    package_name: &str,
    package_version: &str,
    metadata: Option<TestWdkMetadata>,
    target_kind: &str,
    crate_type: &str,
    source_file: &str,
) -> (TestMetadataWorkspaceMemberId, TestMetadataPackage) {
    let normalized_root = root_dir.to_string_lossy().replace('\\', "/");
    let normalized_root = normalized_root.trim_start_matches("//?/");
    let package_id = format!("path+file:///{normalized_root}#{package_name}@{package_version}");
    let metadata_section = metadata.map_or_else(|| String::from("null"), |metadata| metadata.0);
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
            "name": "{package_name}",
            "version": "{package_version}",
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
                    "name": "{package_name}",
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

fn strip_windows_extended_prefix(path: &Path) -> String {
    let path_str = path.to_string_lossy();
    let Some(without_prefix) = path_str.strip_prefix(r"\\?\") else {
        return path_str.into_owned();
    };

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
    let profile_dir = match profile {
        Some(Profile::Release) => "release",
        _ => "debug",
    };
    let (kind, crate_types, file_ext) = if is_driver {
        ("cdylib", "cdylib", "dll")
    } else {
        ("lib", "lib", "rlib")
    };

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

mod get_target_dir_from_output {
    use std::{
        io,
        path::{Path, PathBuf},
    };

    use cargo_metadata::Message;

    use super::{BuildAction, BuildActionError};

    #[test]
    fn unparsable_output_fails() {
        let workspace_root_dir = PathBuf::from(r"C:\tmp\sample-kmdf");
        let wdk_metadata = super::get_cargo_metadata_wdk_metadata("KMDF", 1, 0);
        let (_workspace_member, package_json) = super::get_cargo_metadata_package_with_target(
            &workspace_root_dir,
            "sample-kmdf",
            "0.0.1",
            Some(wdk_metadata),
            "cdylib",
            "cdylib",
            "main.rs",
        );
        let package = serde_json::from_str::<cargo_metadata::Package>(&package_json.0)
            .expect("Failed to parse package json");

        let cargo_build_output = std::iter::once::<Result<Message, io::Error>>(Err(
            io::Error::new(io::ErrorKind::InvalidData, "unparsable cargo message"),
        ));

        let result = BuildAction::get_target_dir_from_output(&package, cargo_build_output);
        assert!(
            matches!(
                result,
                Err(BuildActionError::CannotDetermineTargetDir(ref message))
                if message.contains("Could not parse cargo build output message")
            ),
            "Expected CannotDetermineTargetDir parse error, got: {result:?}"
        );
    }

    #[test]
    fn no_matching_artifact_fails() {
        let workspace_root_dir = PathBuf::from(r"C:\tmp\sample-kmdf");
        let wdk_metadata = super::get_cargo_metadata_wdk_metadata("KMDF", 1, 0);
        let (_workspace_member, package_json) = super::get_cargo_metadata_package_with_target(
            &workspace_root_dir,
            "sample-kmdf",
            "0.0.1",
            Some(wdk_metadata),
            "cdylib",
            "cdylib",
            "main.rs",
        );
        let package = serde_json::from_str::<cargo_metadata::Package>(&package_json.0)
            .expect("Failed to parse package json");

        let output = super::create_cargo_build_output_json_with_manifest(
            "other",
            "9.9.9",
            &workspace_root_dir,
            &workspace_root_dir.join("Cargo.toml"),
            None,
            None,
            true,
        );
        let cargo_build_output = Message::parse_stream(io::Cursor::new(output.stdout));

        let result = BuildAction::get_target_dir_from_output(&package, cargo_build_output);
        assert!(
            matches!(
                result,
                Err(BuildActionError::CannotDetermineTargetDir(ref message))
                if message.contains("Could not find matching cdylib artifact")
            ),
            "Expected CannotDetermineTargetDir no-matching-artifact error, got: {result:?}"
        );
    }

    #[test]
    fn matching_artifact_without_dll_fails() {
        let workspace_root_dir = PathBuf::from(r"C:\tmp\sample-kmdf");
        let wdk_metadata = super::get_cargo_metadata_wdk_metadata("KMDF", 1, 0);
        let (_workspace_member, package_json) = super::get_cargo_metadata_package_with_target(
            &workspace_root_dir,
            "sample-kmdf",
            "0.0.1",
            Some(wdk_metadata),
            "cdylib",
            "cdylib",
            "main.rs",
        );
        let package = serde_json::from_str::<cargo_metadata::Package>(&package_json.0)
            .expect("Failed to parse package json");

        let output = super::create_cargo_build_output_json_with_manifest(
            "sample-kmdf",
            "0.0.1",
            &workspace_root_dir,
            &workspace_root_dir.join("Cargo.toml"),
            None,
            None,
            true,
        );

        let mut artifact_value: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("Failed to parse compiler-artifact JSON");

        if let Some(filenames) = artifact_value
            .get_mut("filenames")
            .and_then(|v| v.as_array_mut())
        {
            filenames.retain(|v| {
                v.as_str()
                    .is_some_and(|p| p.to_ascii_lowercase().ends_with(".pdb"))
            });
        }

        let artifact_json = format!("{artifact_value}\n");
        let cargo_build_output = Message::parse_stream(io::Cursor::new(artifact_json.into_bytes()));

        let result = BuildAction::get_target_dir_from_output(&package, cargo_build_output);
        assert!(
            matches!(
                result,
                Err(BuildActionError::CannotDetermineTargetDir(ref message))
                if message.contains("Could not find matching cdylib artifact")
            ),
            "Expected CannotDetermineTargetDir no-dll-filename error, got: {result:?}"
        );
    }

    #[test]
    fn matching_dll_resolves_target_dir() {
        let workspace_root_dir = PathBuf::from(r"C:\tmp\sample-kmdf");
        let wdk_metadata = super::get_cargo_metadata_wdk_metadata("KMDF", 1, 0);
        let (_workspace_member, package_json) = super::get_cargo_metadata_package_with_target(
            &workspace_root_dir,
            "sample-kmdf",
            "0.0.1",
            Some(wdk_metadata),
            "cdylib",
            "cdylib",
            "main.rs",
        );
        let package = serde_json::from_str::<cargo_metadata::Package>(&package_json.0)
            .expect("Failed to parse package json");

        let output = super::create_cargo_build_output_json_with_manifest(
            "sample-kmdf",
            "0.0.1",
            &workspace_root_dir,
            &workspace_root_dir.join("Cargo.toml"),
            None,
            None,
            true,
        );
        let artifact_value: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("Failed to parse compiler-artifact JSON");
        let dll_path = artifact_value
            .get("filenames")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.as_str())
            .map(Path::new)
            .expect("Expected a DLL path in compiler-artifact filenames");

        let cargo_build_output = Message::parse_stream(io::Cursor::new(output.stdout));

        let result = BuildAction::get_target_dir_from_output(&package, cargo_build_output)
            .expect("expected target dir to be resolved");

        let expected_target_dir = std::path::absolute(
            PathBuf::from(dll_path)
                .parent()
                .expect("expected dll parent"),
        )
        .expect("absolute path failed");

        assert_eq!(result, expected_target_dir);
    }
}

mod get_target_arch_from_cargo_rustc {
    use std::{
        collections::HashMap,
        path::{Path, PathBuf},
        process::{ExitStatus, Output},
    };

    use wdk_build::CpuArchitecture;

    use super::{BuildActionError, SignMode, TestBuildAction, initialize_build_action};

    fn run_parse_test(cfg_output: Vec<u8>, expected_arch: CpuArchitecture) {
        let cwd = PathBuf::from(r"C:\tmp");
        let mut test_build_action = TestBuildAction::new(
            cwd.clone(),
            None,
            None,
            SignMode::Test {
                verify_signature: false,
            },
            false,
        );
        expect_cargo_rustc_print_cfg(&mut test_build_action, cwd.clone(), cfg_output);

        let build_action = initialize_build_action(&mut test_build_action);
        let arch = build_action
            .get_target_arch_from_cargo_rustc(&cwd)
            .expect("Expected target arch to be detected");
        assert_eq!(arch, expected_arch);
    }

    #[test]
    fn parses_amd64() {
        run_parse_test(b"target_arch=\"x86_64\"\n".to_vec(), CpuArchitecture::Amd64);
    }

    #[test]
    fn parses_arm64_with_whitespace_and_crlf() {
        run_parse_test(
            b"  \ttarget_arch=\"aarch64\"\r\n".to_vec(),
            CpuArchitecture::Arm64,
        );
    }

    #[test]
    fn parses_arm64_with_internal_whitespace() {
        run_parse_test(
            b"target_arch=  \"aarch64\"\n".to_vec(),
            CpuArchitecture::Arm64,
        );
    }

    #[test]
    fn unsupported_arch_returns_error() {
        let cwd = PathBuf::from(r"C:\tmp");
        let mut test_build_action = TestBuildAction::new(
            cwd.clone(),
            None,
            None,
            SignMode::Test {
                verify_signature: false,
            },
            false,
        );
        expect_cargo_rustc_print_cfg(
            &mut test_build_action,
            cwd.clone(),
            b"target_arch=\"mips\"\n".to_vec(),
        );

        let build_action = initialize_build_action(&mut test_build_action);
        let err = build_action
            .get_target_arch_from_cargo_rustc(&cwd)
            .expect_err("Expected UnsupportedArchitecture error");
        assert!(matches!(err, BuildActionError::UnsupportedArchitecture(ref a) if a == "mips"));
    }

    #[test]
    fn missing_target_arch_returns_error() {
        let cwd = PathBuf::from(r"C:\tmp");
        let mut test_build_action = TestBuildAction::new(
            cwd.clone(),
            None,
            None,
            SignMode::Test {
                verify_signature: false,
            },
            false,
        );
        expect_cargo_rustc_print_cfg(
            &mut test_build_action,
            cwd.clone(),
            b"some_other_cfg=\"value\"\n".to_vec(),
        );

        let build_action = initialize_build_action(&mut test_build_action);
        let err = build_action
            .get_target_arch_from_cargo_rustc(&cwd)
            .expect_err("Expected CannotDetectTargetArch error");
        assert!(matches!(err, BuildActionError::CannotDetectTargetArch));
    }

    #[test]
    fn invalid_utf8_returns_error() {
        let cwd = PathBuf::from(r"C:\tmp");
        let mut test_build_action = TestBuildAction::new(
            cwd.clone(),
            None,
            None,
            SignMode::Test {
                verify_signature: false,
            },
            false,
        );
        expect_cargo_rustc_print_cfg(&mut test_build_action, cwd.clone(), vec![0xFF, 0xFE]);

        let build_action = initialize_build_action(&mut test_build_action);
        let err = build_action
            .get_target_arch_from_cargo_rustc(&cwd)
            .expect_err("Expected CannotDetectTargetArch error");
        assert!(matches!(err, BuildActionError::CannotDetectTargetArch));
    }

    fn expect_cargo_rustc_print_cfg(
        test_build_action: &mut TestBuildAction,
        cwd: PathBuf,
        stdout: Vec<u8>,
    ) {
        let mut expected_args: Vec<String> = vec!["rustc".to_string()];
        if test_build_action.locked {
            expected_args.push("--locked".to_string());
        }
        expected_args.extend(super::super::features_to_cargo_args(
            &test_build_action.features,
        ));
        expected_args.extend(["--".to_string(), "--print".to_string(), "cfg".to_string()]);
        test_build_action
            .mock_run_command
            .expect_run()
            .withf(
                move |command: &str,
                      args: &[&str],
                      _env_vars: &Option<&HashMap<&str, &str>>,
                      working_dir: &Option<&Path>| {
                    let expected_refs: Vec<&str> =
                        expected_args.iter().map(String::as_str).collect();
                    command == "cargo"
                        && args == expected_refs.as_slice()
                        && matches!(working_dir, Some(dir) if *dir == cwd.as_path())
                },
            )
            .once()
            .returning(move |_, _, _, _| {
                Ok(Output {
                    status: ExitStatus::default(),
                    stdout: stdout.clone(),
                    stderr: vec![],
                })
            });
    }
}
