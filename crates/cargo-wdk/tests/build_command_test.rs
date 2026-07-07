//! System level tests for cargo wdk build flow
mod test_utils;
use std::{
    env,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use assert_cmd::prelude::*;
use sha2::{Digest, Sha256};
use test_utils::{create_cargo_wdk_cmd, with_mutex};

const STAMPINF_VERSION_ENV_VAR: &str = "STAMPINF_VERSION";
const WDK_CONTENT_ROOT_ENV_VAR: &str = "WDKContentRoot";
const NUGET_PACKAGES_ROOT_ENV_VAR: &str = "NugetPackagesRoot";
const FULL_VERSION_NUMBER_ENV_VAR: &str = "FullVersionNumber";
const X86_64_TARGET_TRIPLE_NAME: &str = "x86_64-pc-windows-msvc";
const AARCH64_TARGET_TRIPLE_NAME: &str = "aarch64-pc-windows-msvc";

#[test]
fn mixed_package_kmdf_workspace_builds_successfully() {
    clean_build_and_verify_project(
        "kmdf",
        "driver",
        Some("tests/mixed-package-kmdf-workspace"),
        None,
        None,
        None,
        None,
        None,
        None,
    );
}

#[test]
fn kmdf_driver_builds_successfully() {
    // Setup for executables
    wdk_build::cargo_make::setup_path().expect("failed to set up paths for executables");
    let driver = "kmdf-driver";
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

    clean_build_and_verify_project("kmdf", driver, None, None, None, None, None, None, None);
}

#[test]
fn kmdf_driver_builds_successfully_with_locked_flag() {
    let driver = "kmdf-driver";
    clean_build_and_verify_project(
        "kmdf",
        driver,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(&["--locked"]),
    );
}

#[test]
fn kmdf_driver_cross_compiles_with_cli_option_successfully() {
    let driver = "kmdf-driver";
    let target_arch = cross_compile_target_arch();
    let env = nuget_wdk_content_root_path(target_arch)
        .map(|path| vec![(WDK_CONTENT_ROOT_ENV_VAR, Some(path))]);
    clean_build_and_verify_project(
        "kmdf",
        driver,
        None,
        None,
        Some(target_arch),
        None,
        env.as_deref(),
        Some(target_arch),
        None,
    );
}

#[test]
fn umdf_driver_builds_successfully() {
    let driver = "umdf-driver";
    clean_build_and_verify_project("umdf", driver, None, None, None, None, None, None, None);
}

#[test]
fn umdf_driver_cross_compiles_with_cli_option_successfully() {
    let driver = "umdf-driver";
    let target_arch = cross_compile_target_arch();
    let env = nuget_wdk_content_root_path(target_arch)
        .map(|path| vec![(WDK_CONTENT_ROOT_ENV_VAR, Some(path))]);
    clean_build_and_verify_project(
        "umdf",
        driver,
        None,
        None,
        Some(target_arch),
        None,
        env.as_deref(),
        Some(target_arch),
        None,
    );
}

#[test]
fn umdf_driver_with_target_arch_cli_option_and_release_profile_builds_successfully() {
    let driver = "umdf-driver";
    let target_arch = "ARM64";
    let profile = "release";
    let env = nuget_wdk_content_root_path(target_arch)
        .map(|path| vec![(WDK_CONTENT_ROOT_ENV_VAR, Some(path))]);
    clean_build_and_verify_project(
        "umdf",
        driver,
        None,
        None,
        Some(target_arch),
        Some(profile),
        env.as_deref(),
        Some(target_arch),
        None,
    );
}

#[test]
fn wdm_driver_builds_successfully() {
    let driver = "wdm-driver";
    clean_build_and_verify_project("wdm", driver, None, None, None, None, None, None, None);
}

#[test]
fn wdm_driver_builds_successfully_with_given_version() {
    let driver = "wdm-driver";
    let env = [(STAMPINF_VERSION_ENV_VAR, Some("5.1.0".to_string()))];
    clean_build_and_verify_project(
        "wdm",
        driver,
        None,
        Some("5.1.0.0"),
        None,
        None,
        Some(&env),
        None,
        None,
    );
}

#[test]
fn wdm_driver_cross_compiles_with_cli_option_successfully() {
    let driver = "wdm-driver";
    let target_arch = cross_compile_target_arch();
    let env = nuget_wdk_content_root_path(target_arch)
        .map(|path| vec![(WDK_CONTENT_ROOT_ENV_VAR, Some(path))]);
    clean_build_and_verify_project(
        "wdm",
        driver,
        None,
        None,
        Some(target_arch),
        None,
        env.as_deref(),
        Some(target_arch),
        None,
    );
}

#[test]
fn emulated_workspace_builds_successfully() {
    let emulated_workspace_path = "tests/emulated-workspace";
    let umdf_driver_workspace_path = format!("{emulated_workspace_path}/umdf-driver-workspace");
    with_mutex(emulated_workspace_path, || {
        run_clean_cmd(emulated_workspace_path);
        assert_target_dir_does_not_exist(&umdf_driver_workspace_path);
        assert_target_dir_does_not_exist(&format!("{emulated_workspace_path}/rust-project"));

        let stderr = run_build_cmd(emulated_workspace_path, None, None);
        assert!(stderr.contains("Building package driver_1"));
        assert!(stderr.contains("Building package driver_2"));
        assert!(stderr.contains("Build completed successfully"));

        verify_driver_package_files(
            &umdf_driver_workspace_path,
            "driver_1",
            "dll",
            None,
            None,
            None,
        );
        verify_driver_package_files(
            &umdf_driver_workspace_path,
            "driver_2",
            "dll",
            None,
            None,
            None,
        );

        run_clean_cmd(emulated_workspace_path);
        assert_target_dir_does_not_exist(&umdf_driver_workspace_path);
        assert_target_dir_does_not_exist(&format!("{emulated_workspace_path}/rust-project"));
        assert_package_dir_does_not_exist(&umdf_driver_workspace_path, "driver_1", None, "debug");
        assert_package_dir_does_not_exist(&umdf_driver_workspace_path, "driver_2", None, "debug");
    });
}

mod kmdf_driver_with_target_override {
    use super::*;

    const DRIVER_PROJECT: &str = "kmdf-driver-with-target-override";
    const CARGO_BUILD_TARGET_ENV_VAR: &str = "CARGO_BUILD_TARGET";

    fn configure_env(
        wdk_target_arch: &str,
        cargo_build_target: Option<&str>,
    ) -> Vec<(&'static str, Option<String>)> {
        let mut env: Vec<(&'static str, Option<String>)> = Vec::new();

        if let Some(target_triple) = cargo_build_target {
            env.push((CARGO_BUILD_TARGET_ENV_VAR, Some(target_triple.to_string())));
        }

        if let Some(path) = nuget_wdk_content_root_path(wdk_target_arch) {
            env.push((WDK_CONTENT_ROOT_ENV_VAR, Some(path)));
        }
        env
    }

    // `config.toml` with `build.target` = "x86_64-pc-windows-msvc"
    #[test]
    fn via_config_toml_builds_successfully() {
        let target_arch = "x64";
        let env = configure_env(target_arch, None);
        clean_build_and_verify_project(
            "kmdf",
            DRIVER_PROJECT,
            None,
            None,
            None,
            None,
            Some(env.as_slice()),
            Some(target_arch),
            None,
        );
    }

    #[test]
    fn via_env_wins_over_config_toml() {
        let target_arch = "ARM64";
        let env = configure_env(target_arch, Some(AARCH64_TARGET_TRIPLE_NAME));
        clean_build_and_verify_project(
            "kmdf",
            DRIVER_PROJECT,
            None,
            None,
            None,
            None,
            Some(env.as_slice()),
            Some(target_arch),
            None,
        );
    }

    #[test]
    fn via_cli_wins_over_env() {
        let target_arch = "x64";
        let cli_target_arch = "amd64";
        let env = configure_env(target_arch, Some(AARCH64_TARGET_TRIPLE_NAME));
        clean_build_and_verify_project(
            "kmdf",
            DRIVER_PROJECT,
            None,
            None,
            Some(cli_target_arch),
            None,
            Some(env.as_slice()),
            Some(cli_target_arch),
            None,
        );
    }

    #[test]
    fn via_cli_wins_over_env_and_config() {
        let target_arch = "ARM64";
        let env = configure_env(target_arch, Some(X86_64_TARGET_TRIPLE_NAME));
        clean_build_and_verify_project(
            "kmdf",
            DRIVER_PROJECT,
            None,
            None,
            Some(target_arch),
            None,
            Some(env.as_slice()),
            Some(target_arch),
            None,
        );
    }
}

#[test]
fn kmdf_driver_builds_successfully_with_sign_mode_off() {
    let driver = "kmdf-driver";
    let project_path = format!("tests/{driver}");
    with_mutex(&project_path, || {
        run_clean_cmd(&project_path);

        let stderr = run_build_cmd(&project_path, Some(&["--sign-mode", "off"]), None);
        assert!(stderr.contains(&format!("Building package {driver}")));
        assert!(stderr.contains(&format!("Finished building {driver}")));

        let driver_name = driver.replace('-', "_");
        let target_dir = format!("{project_path}/target/debug");
        let package_dir = format!("{target_dir}/{driver_name}_package");

        assert_dir_exists(&package_dir);
        for ext in ["cat", "inf", "map", "pdb", "sys"] {
            assert_file_exists(&format!("{package_dir}/{driver_name}.{ext}"));
        }

        let cert_in_package = PathBuf::from(format!("{package_dir}/WDRLocalTestCert.cer"));
        assert!(
            !cert_in_package.exists(),
            "Cert file must not be present in the final package folder when --sign-mode=off, but \
             found {}",
            cert_in_package.display()
        );

        let staged_cert = PathBuf::from(format!("{target_dir}/WDRLocalTestCert.cer"));
        assert!(
            !staged_cert.exists(),
            "Cert file must not be present in the `target` dir when --sign-mode=off, but found {}",
            staged_cert.display()
        );
    });
}

/// Regression test for issue #660: because the package folder is assembled
/// fresh (the tools run in a staging dir and the folder is replaced last),
/// switching from `--sign-mode=test` to `--sign-mode=off` on a subsequent build
/// (without cleaning) must drop the stale `WDRLocalTestCert.cer` from the
/// package folder.
#[test]
fn rebuild_with_sign_mode_off_drops_stale_test_cert() {
    let driver = "kmdf-driver";
    let project_path = format!("tests/{driver}");
    with_mutex(&project_path, || {
        run_clean_cmd(&project_path);

        let driver_name = driver.replace('-', "_");
        let package_dir = format!("{project_path}/target/debug/{driver_name}_package");
        let cert_in_package = PathBuf::from(format!("{package_dir}/WDRLocalTestCert.cer"));

        // First build test-signs, so the package folder contains the cert.
        run_build_cmd(&project_path, Some(&["--sign-mode", "test"]), None);
        assert!(
            cert_in_package.exists(),
            "test-sign build should place the cert in the package folder: {}",
            cert_in_package.display()
        );

        // Rebuild with signing off (no clean in between). The package folder is
        // reassembled fresh, so the stale cert must be gone.
        run_build_cmd(&project_path, Some(&["--sign-mode", "off"]), None);
        assert!(
            !cert_in_package.exists(),
            "stale cert must be removed after rebuilding with --sign-mode=off, but found {}",
            cert_in_package.display()
        );

        // The rest of the package is still assembled.
        assert_dir_exists(&package_dir);
        for ext in ["cat", "inf", "map", "pdb", "sys"] {
            assert_file_exists(&format!("{package_dir}/{driver_name}.{ext}"));
        }
    });
}

/// `--sign-mode=off` together with `--verify-signature` is rejected at the CLI
/// layer
#[test]
fn sign_mode_off_with_verify_signature_is_rejected() {
    let driver = "kmdf-driver";
    let project_path = format!("tests/{driver}");
    let mut cmd = create_cargo_wdk_cmd(
        "build",
        Some(&["--sign-mode", "off", "--verify-signature"]),
        None,
        Some(&project_path),
    );
    let assertion = cmd.assert().failure();
    let stderr = String::from_utf8_lossy(&assertion.get_output().stderr).to_string();
    assert!(
        stderr.contains("`--verify-signature` cannot be used with `--sign-mode=off`."),
        "expected validation error mentioning both flags, got: {stderr}"
    );
}

/// `--signtool-args` passthrough with a caller-provided store and certificate.
/// A dedicated custom store/cert (distinct from the auto-generated WDR test
/// cert) proves the passthrough drives signtool's certificate selection rather
/// than cargo-wdk's defaults.
#[test]
fn kmdf_driver_signs_with_custom_store_and_cert_via_signtool_args() {
    let driver = "kmdf-driver";
    let project_path = format!("tests/{driver}");
    with_mutex(&project_path, || {
        setup_wdk_tool_path();
        ensure_cert_in_store("WDRCustomTestStore", "WDRCustomTestCert");
        run_clean_cmd(&project_path);

        let stderr = run_build_cmd(
            &project_path,
            Some(&[
                "--signtool-args",
                "/s WDRCustomTestStore /n WDRCustomTestCert /fd SHA256",
            ]),
            None,
        );
        assert!(stderr.contains(&format!("Finished building {driver}")));

        let driver_name = driver.replace('-', "_");
        let package_dir = format!("{project_path}/target/debug/{driver_name}_package");
        assert_dir_exists(&package_dir);
        for ext in ["cat", "inf", "sys"] {
            assert_file_exists(&format!("{package_dir}/{driver_name}.{ext}"));
        }
        // Passthrough signing does not generate/copy the WDR test cert file.
        assert!(
            !PathBuf::from(format!("{package_dir}/WDRLocalTestCert.cer")).exists(),
            "passthrough signing should not emit WDRLocalTestCert.cer"
        );

        // The freshly built driver binary must carry the custom cert's signature.
        let sys = format!("{package_dir}/{driver_name}.sys");
        let signer =
            authenticode_signer_subject(Path::new(&sys)).expect("driver binary should be signed");
        assert!(
            signer.contains("WDRCustomTestCert"),
            "driver binary signed by unexpected cert: {signer}"
        );
    });
}

/// signtool signs every file operand it is given. A file path inside
/// `--signtool-args` is therefore signed in addition to cargo-wdk's own driver
/// binary. Exercised on a UMDF driver (whose binary is a `.dll`).
#[test]
fn umdf_driver_signs_extra_file_operand_via_signtool_args() {
    let driver = "umdf-driver";
    let project_path = format!("tests/{driver}");
    with_mutex(&project_path, || {
        setup_wdk_tool_path();
        ensure_cert_in_store("WDRTestCertStore", "WDRLocalTestCert");
        run_clean_cmd(&project_path);

        let driver_name = driver.replace('-', "_");
        let package_dir = format!("{project_path}/target/debug/{driver_name}_package");
        let common_args = "/s WDRTestCertStore /n WDRLocalTestCert /fd SHA256";

        // First build produces a signed driver binary we can reuse as an
        // independent second file operand.
        run_build_cmd(&project_path, Some(&["--signtool-args", common_args]), None);
        let built = format!("{package_dir}/{driver_name}.dll");
        assert_file_exists(&built);

        // Copy it out and strip the signature, so a passing assertion proves
        // THIS build signed it (not a pre-existing signature).
        let extra = env::current_dir()
            .expect("cwd")
            .join(&project_path)
            .join("extra_to_sign.dll");
        fs::copy(&built, &extra).expect("copy extra file");
        strip_signature(&extra);
        assert!(
            authenticode_signer_subject(&extra).is_none(),
            "precondition: extra file should be unsigned after strip"
        );

        // Pass the extra file as an additional operand inside --signtool-args.
        let args_with_extra = format!("{common_args} {}", extra.to_string_lossy());
        let stderr = run_build_cmd(
            &project_path,
            Some(&["--signtool-args", &args_with_extra]),
            None,
        );
        assert!(stderr.contains(&format!("Finished building {driver}")));

        let signer = authenticode_signer_subject(&extra)
            .expect("extra file operand should be signed after build");
        assert!(
            signer.contains("WDRLocalTestCert"),
            "extra file operand signed by unexpected cert: {signer}"
        );

        fs::remove_file(&extra).ok();
    });
}

/// A certificate selector in `--signtool-args` that matches nothing must fail
/// the build (signtool exits non-zero, surfaced as a signing error). Exercised
/// on a WDM driver.
#[test]
fn wdm_driver_build_fails_when_signtool_args_select_unknown_cert() {
    let driver = "wdm-driver";
    let project_path = format!("tests/{driver}");
    with_mutex(&project_path, || {
        setup_wdk_tool_path();
        run_clean_cmd(&project_path);

        let mut cmd = create_cargo_wdk_cmd(
            "build",
            Some(&[
                "--signtool-args",
                "/s WDRTestCertStore /n NoSuchCert /fd SHA256",
            ]),
            None,
            Some(&project_path),
        );
        let assertion = cmd.assert().failure();
        let stderr = String::from_utf8_lossy(&assertion.get_output().stderr).to_string();
        assert!(
            stderr.contains("No certificates were found")
                || stderr.contains("signing driver binary"),
            "expected a signtool certificate-selection failure, got: {stderr}"
        );
    });
}

/// A stray `sign` verb inside `--signtool-args` derails signtool's argument
/// parser (the second `sign` is treated as a file operand, so `/fd` is never
/// registered) and the build fails.
#[test]
fn kmdf_driver_build_fails_with_duplicate_sign_verb_in_signtool_args() {
    let driver = "kmdf-driver";
    let project_path = format!("tests/{driver}");
    with_mutex(&project_path, || {
        setup_wdk_tool_path();
        ensure_cert_in_store("WDRTestCertStore", "WDRLocalTestCert");
        run_clean_cmd(&project_path);

        let mut cmd = create_cargo_wdk_cmd(
            "build",
            Some(&[
                "--signtool-args",
                "sign /s WDRTestCertStore /n WDRLocalTestCert /fd SHA256",
            ]),
            None,
            Some(&project_path),
        );
        let assertion = cmd.assert().failure();
        let stderr = String::from_utf8_lossy(&assertion.get_output().stderr).to_string();
        assert!(
            stderr.contains("No file digest algorithm specified")
                || stderr.contains("signing driver binary"),
            "expected a signtool failure from the duplicate `sign` verb, got: {stderr}"
        );
    });
}

/// `--signtool-args` together with `--sign-mode=off` is rejected at the CLI
/// layer (nothing would be signed), before any build work happens.
#[test]
fn sign_mode_off_with_signtool_args_is_rejected() {
    let driver = "kmdf-driver";
    let project_path = format!("tests/{driver}");
    let mut cmd = create_cargo_wdk_cmd(
        "build",
        Some(&["--sign-mode", "off", "--signtool-args", "/fd SHA256"]),
        None,
        Some(&project_path),
    );
    let assertion = cmd.assert().failure();
    let stderr = String::from_utf8_lossy(&assertion.get_output().stderr).to_string();
    assert!(
        stderr.contains("`--signtool-args` cannot be used with `--sign-mode=off`."),
        "expected validation error mentioning both flags, got: {stderr}"
    );
}

/// Workspace behavior: building a workspace with `--signtool-args` signs each
/// packaged driver member with the caller-provided certificate. Uses a mixed
/// workspace (a KMDF `driver` member plus a non-driver crate); only the driver
/// member is packaged and signed.
#[test]
fn mixed_package_kmdf_workspace_signs_driver_member_via_signtool_args() {
    let project_path = "tests/mixed-package-kmdf-workspace";
    with_mutex(project_path, || {
        setup_wdk_tool_path();
        ensure_cert_in_store("WDRTestCertStore", "WDRLocalTestCert");
        run_clean_cmd(project_path);

        run_build_cmd(
            project_path,
            Some(&[
                "--signtool-args",
                "/s WDRTestCertStore /n WDRLocalTestCert /fd SHA256",
            ]),
            None,
        );

        let package_dir = format!("{project_path}/target/debug/driver_package");
        assert_dir_exists(&package_dir);
        for ext in ["cat", "inf", "sys"] {
            assert_file_exists(&format!("{package_dir}/driver.{ext}"));
        }
        // Passthrough signing does not generate/copy the WDR test cert file.
        assert!(
            !PathBuf::from(format!("{package_dir}/WDRLocalTestCert.cer")).exists(),
            "passthrough signing should not emit WDRLocalTestCert.cer"
        );

        let signer = authenticode_signer_subject(Path::new(&format!("{package_dir}/driver.sys")))
            .expect("workspace driver member should be signed");
        assert!(
            signer.contains("WDRLocalTestCert"),
            "workspace driver member signed by unexpected cert: {signer}"
        );
    });
}

#[allow(clippy::too_many_arguments)]
fn clean_build_and_verify_project(
    driver_type: &str,
    driver_name: &str,
    project_path: Option<&str>,
    driver_version: Option<&str>,
    input_target_arch: Option<&str>,
    profile: Option<&str>,
    env_overrides: Option<&[(&str, Option<String>)]>,
    target_arch_for_verification: Option<&str>,
    additional_build_args: Option<&[&str]>,
) {
    let project_path =
        project_path.map_or_else(|| format!("tests/{driver_name}"), ToString::to_string);
    let mutex_name = project_path.clone();
    with_mutex(&mutex_name, || {
        run_clean_cmd(&project_path);
        assert_target_dir_does_not_exist(&project_path);

        let mut args: Vec<&str> = Vec::new();
        if let Some(target_arch) = input_target_arch {
            args.push("--target-arch");
            args.push(target_arch);
        }
        if let Some(profile) = profile {
            args.push("--profile");
            args.push(profile);
        }
        if let Some(additional_args) = additional_build_args {
            args.extend_from_slice(additional_args);
        }
        let cmd_args = if args.is_empty() {
            None
        } else {
            Some(args.as_slice())
        };
        let stderr = run_build_cmd(&project_path, cmd_args, env_overrides);

        assert!(stderr.contains(&format!("Building package {driver_name}")));
        assert!(stderr.contains(&format!("Finished building {driver_name}")));

        let driver_binary_extension = match driver_type {
            "kmdf" | "wdm" => "sys",
            "umdf" => "dll",
            _ => panic!("Unsupported driver type: {driver_type}"),
        };

        let target_triple = target_arch_for_verification.and_then(to_target_triple);

        verify_driver_package_files(
            &project_path,
            driver_name,
            driver_binary_extension,
            driver_version,
            target_triple,
            profile,
        );

        run_clean_cmd(&project_path);
        assert_target_dir_does_not_exist(&project_path);
        assert_package_dir_does_not_exist(
            &project_path,
            driver_name,
            target_triple,
            profile.unwrap_or("debug"),
        );
    });
}

fn to_target_triple(target_arch: &str) -> Option<&'static str> {
    match target_arch.to_ascii_lowercase().as_str() {
        "x64" | "amd64" => Some(X86_64_TARGET_TRIPLE_NAME),
        "arm64" | "aarch64" => Some(AARCH64_TARGET_TRIPLE_NAME),
        _ => None,
    }
}

fn run_clean_cmd(path: &str) {
    let mut cmd = create_cargo_wdk_cmd("clean", None, None, Some(path));
    cmd.assert().success();
}

fn assert_target_dir_does_not_exist(project_path: &str) {
    let target_dir = Path::new(project_path).join("target");
    assert!(
        !target_dir.exists(),
        "Expected target directory to not exist after clean: {}",
        target_dir.display()
    );
}

fn assert_package_dir_does_not_exist(
    project_path: &str,
    driver_name: &str,
    target_triple: Option<&str>,
    profile: &str,
) {
    let driver_name = driver_name.replace('-', "_");
    let target_folder_path = target_triple.map_or_else(
        || format!("{project_path}/target/{profile}"),
        |triple| format!("{project_path}/target/{triple}/{profile}"),
    );
    let package_dir = PathBuf::from(&target_folder_path).join(format!("{driver_name}_package"));
    assert!(
        !package_dir.exists(),
        "Expected package directory to not exist after clean: {}",
        package_dir.display()
    );
}

fn run_build_cmd(
    path: &str,
    args: Option<&[&str]>,
    env_vars: Option<&[(&str, Option<String>)]>,
) -> String {
    // assert command output
    let mut cmd = create_cargo_wdk_cmd("build", args, env_vars, Some(path));
    let cmd_assertion = cmd.assert().success();
    let output = cmd_assertion.get_output();

    String::from_utf8_lossy(&output.stderr).to_string()
}

/// Puts the WDK tools (`signtool`, `makecert`, `certmgr`, `stampinf`,
/// `inf2cat`, ...) on `PATH` for this test process (and hence any child
/// `cargo-wdk` invocation).
fn setup_wdk_tool_path() {
    wdk_build::cargo_make::setup_path().expect("failed to set up WDK tool paths");
}

/// Ensures a self-signed code-signing certificate `CN=<cn>` exists in the given
/// certificate store, creating it with `makecert` if missing. Passthrough
/// signing (`--signtool-args`) does not auto-generate certificates, so tests
/// must provision them up front.
fn ensure_cert_in_store(store: &str, cn: &str) {
    let output = Command::new("certmgr.exe")
        .args(["-s", store])
        .output()
        .expect("failed to query certificate store");
    assert!(output.status.success(), "certmgr query failed for {store}");

    if !String::from_utf8_lossy(&output.stdout).contains(cn) {
        let subject = format!("CN={cn}");
        let out = Command::new("makecert")
            .args([
                "-r",
                "-pe",
                "-a",
                "SHA256",
                "-eku",
                "1.3.6.1.5.5.7.3.3",
                "-ss",
                store,
                "-n",
                &subject,
            ])
            .output()
            .expect("failed to run makecert");
        assert!(
            out.status.success(),
            "makecert failed for {store}/{cn}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// Removes the primary Authenticode signature from a PE file using
/// `signtool remove`.
fn strip_signature(path: &Path) {
    let out = Command::new("signtool")
        .args(["remove", "/s"])
        .arg(path)
        .output()
        .expect("failed to run signtool remove");
    assert!(
        out.status.success(),
        "signtool remove failed for {}: {}",
        path.display(),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Returns the subject of the certificate that signed `path`, or `None` if the
/// file is not signed. Reads the embedded PE signature (trust-independent) via
/// `Get-AuthenticodeSignature`, so it works without the signer being trusted.
fn authenticode_signer_subject(path: &Path) -> Option<String> {
    let script = format!(
        "$s = Get-AuthenticodeSignature -LiteralPath '{}'; if ($s.SignerCertificate) {{ \
         Write-Output $s.SignerCertificate.Subject }}",
        path.display()
    );
    let out = Command::new("powershell")
        // Clear PSModulePath so Windows PowerShell uses its default module path.
        // When the test runner is launched from PowerShell 7 (pwsh), the child
        // Windows PowerShell inherits pwsh's PSModulePath and fails to load
        // `Microsoft.PowerShell.Security` (which provides Get-AuthenticodeSignature).
        .env_remove("PSModulePath")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .expect("failed to run Get-AuthenticodeSignature");
    let subject = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if subject.is_empty() {
        None
    } else {
        Some(subject)
    }
}

fn verify_driver_package_files(
    driver_or_workspace_path: &str,
    driver_name: &str,
    driver_binary_extension: &str,
    driver_version: Option<&str>,
    target_triple: Option<&str>,
    profile: Option<&str>,
) {
    let driver_name = driver_name.replace('-', "_");
    let profile = profile.unwrap_or("debug");
    let target_folder_path = target_triple.map_or_else(
        || format!("{driver_or_workspace_path}/target/{profile}"),
        |target_triple| format!("{driver_or_workspace_path}/target/{target_triple}/{profile}"),
    );
    let package_path = PathBuf::from(&target_folder_path)
        .join(format!("{driver_name}_package"))
        .to_string_lossy()
        .to_string();

    // Verify files exist in package folder
    assert_dir_exists(&package_path);

    for ext in ["cat", "inf", "map", "pdb", driver_binary_extension] {
        assert_file_exists(&format!("{package_path}/{driver_name}.{ext}"));
    }

    assert_file_exists(&format!("{package_path}/WDRLocalTestCert.cer"));

    // Verify hashes of files copied from debug to package folder
    assert_file_hash(
        &format!("{package_path}/{driver_name}.map"),
        &format!("{target_folder_path}/deps/{driver_name}.map"),
    );

    assert_file_hash(
        &format!("{package_path}/{driver_name}.pdb"),
        &format!("{target_folder_path}/{driver_name}.pdb"),
    );

    assert_file_hash(
        &format!("{package_path}/WDRLocalTestCert.cer"),
        &format!("{target_folder_path}/WDRLocalTestCert.cer"),
    );

    assert_driver_ver(&package_path, &driver_name, driver_version);
}

fn assert_dir_exists(path: &str) {
    let path = PathBuf::from(path);
    assert!(path.exists(), "Expected {} to exist", path.display());
    assert!(
        path.is_dir(),
        "Expected {} to be a directory",
        path.display()
    );
}

fn assert_file_exists(path: &str) {
    let path = PathBuf::from(path);
    assert!(path.exists(), "Expected {} to exist", path.display());
    assert!(path.is_file(), "Expected {} to be a file", path.display());
}

fn assert_file_hash(path1: &str, path2: &str) {
    assert_file_exists(path1);
    assert_file_exists(path2);

    assert_eq!(
        digest_file(PathBuf::from(path1)),
        digest_file(PathBuf::from(path2)),
        "Hash mismatch between {path1} and {path2}"
    );
}

fn assert_driver_ver(package_path: &str, driver_name: &str, driver_version: Option<&str>) {
    // Read the INF file as raw bytes and produce a best-effort UTF-8 string.
    let file_content =
        fs::read(format!("{package_path}/{driver_name}.inf")).expect("Unable to read inf file");
    let file_content = if file_content.starts_with(&[0xFF, 0xFE]) {
        // Handle UTF-16 LE (BOM 0xFF 0xFE).
        let file_content = file_content
            .chunks(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<u16>>();
        String::from_utf16_lossy(&file_content)
    } else {
        // Otherwise, treat the content as UTF-8; our test setups do not include
        // UTF16-BE encoded .inx files.
        String::from_utf8_lossy(&file_content).to_string()
    };

    // Example: DriverVer = 09/13/2023,1.0.0.0
    let driver_version_regex =
        driver_version.map_or_else(|| r"\d+\.\d+\.\d+\.\d+".to_string(), regex::escape);
    let re = regex::Regex::new(&format!(
        r"^DriverVer\s+=\s+\d+/\d+/\d+,{driver_version_regex}$"
    ))
    .unwrap();

    let line = file_content
        .lines()
        .find(|line| line.starts_with("DriverVer"))
        .expect("DriverVer line not found in inf file");
    assert!(re.captures(line).is_some());
}

// Helper to hash a file
fn digest_file<P: AsRef<Path>>(path: P) -> String {
    let file_contents = fs::read(&path)
        .unwrap_or_else(|e| panic!("Failed to read file at {}: {}", path.as_ref().display(), e));
    let result = Sha256::digest(&file_contents);
    format!("{result:x}")
}

/// Returns the `WDKContentRoot` path derived from the Nuget WDK package, if the
/// Nuget WDK and Full Version Number env vars are present.
///
/// Behavior:
/// - If `NugetPackagesRoot` and `FullVersionNumber` are set, returns
///   `Some(path)`.
/// - Otherwise, returns `None` so tests do not override (or remove)
///   `WDKContentRoot` in non-Nuget environments.
///
/// # Panics:
/// - If the Nuget env vars are set but the expected WDK package folder cannot
///   be found.
fn nuget_wdk_content_root_path(target_arch: &str) -> Option<String> {
    let Ok(nuget_packages_root) = std::env::var(NUGET_PACKAGES_ROOT_ENV_VAR) else {
        return None;
    };
    let Ok(full_version_number) = std::env::var(FULL_VERSION_NUMBER_ENV_VAR) else {
        return None;
    };

    // NuGet WDK package folder names use `x64` (lowercase and not `amd64`) and
    // `ARM64` (uppercase) whereas `cargo-wdk` CLI uses `amd64` / `arm64`
    // (case-insensitive), so we normalize here in order to locate the right package
    // folder during tests.
    let target_arch_lower = target_arch.to_ascii_lowercase();
    let nuget_arch = match target_arch_lower.as_str() {
        "amd64" | "x64" | "x86_64" => "x64",
        "arm64" | "aarch64" => "ARM64",
        other => other,
    };

    let expected_wdk_package_dir_name =
        format!("Microsoft.Windows.WDK.{nuget_arch}.{full_version_number}");

    let wdk_package_dir = fs::read_dir(Path::new(&nuget_packages_root))
        .unwrap_or_else(|err| {
            panic!("Failed to read Nuget package root '{nuget_packages_root}': {err}")
        })
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name == expected_wdk_package_dir_name)
        })
        .unwrap_or_else(|| {
            panic!(
                "Unable to locate WDK package for target architecture {target_arch} (NuGet arch: \
                 {nuget_arch}) under '{nuget_packages_root}'"
            )
        });

    let wdk_content_root_path = wdk_package_dir.join("c");
    assert!(
        wdk_content_root_path.is_dir(),
        "Expected WDK content root '{}' to exist",
        wdk_content_root_path.display()
    );
    Some(wdk_content_root_path.to_string_lossy().into_owned())
}

/// Returns the cross-compilation target architecture for the current host.
///
/// - If host is `x86_64`, we cross-compile to `ARM64`.
/// - If host is `aarch64`, we cross-compile to `AMD64`.
fn cross_compile_target_arch() -> &'static str {
    match env::consts::ARCH {
        "x86_64" => "ARM64",
        "aarch64" => "AMD64",
        other => panic!(
            "Unsupported host architecture '{other}' for cross-compilation tests. Expected \
             'x86_64' or 'aarch64'."
        ),
    }
}
