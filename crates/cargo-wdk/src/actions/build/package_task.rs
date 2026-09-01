// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! Module that handles low-level driver packaging operations.
//! This module defines the `PackageTask` struct and its associated methods
//! for packaging driver projects.  It handles file system
//! operations and interacting with WDK tools to generate the driver package. It
//! includes functions that invoke various WDK Tools involved in signing,
//! validating, verifying and generating artefacts for the driver package.

use std::{
    ffi::{CStr, CString},
    marker::PhantomData,
    ops::RangeInclusive,
    path::{Path, PathBuf},
    result::Result,
};

use certmgr_parser::{parse_certificates, today_in_days};
use mockall_double::double;
use tracing::{debug, info, trace, warn};
use wdk_build::{CpuArchitecture, DriverConfig};
use windows::{
    Win32::{
        Foundation::{CloseHandle, GetLastError, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0},
        System::Threading::{CreateMutexA, INFINITE, ReleaseMutex, WaitForSingleObject},
    },
    core::{Error as WinError, PCSTR},
};

#[double]
use crate::providers::{exec::CommandExec, fs::Fs, wdk_build::WdkBuild};
use crate::{actions::build::error::PackageTaskError, providers::error::FileError};

// InfVerif in WDK builds in this range is buggy and does not contain the
// /samples flag.
const MISSING_SAMPLE_FLAG_WDK_BUILD_NUMBER_RANGE: RangeInclusive<u32> = 25798..=26100;
const WDR_TEST_CERT_STORE: &str = "WDRTestCertStore";
const WDR_LOCAL_TEST_CERT: &str = "WDRLocalTestCert";
const STAMPINF_VERSION_ENV_VAR: &str = "STAMPINF_VERSION";
const CERT_VALIDITY_MONTHS: &str = "120";
/// Enhanced key usage OID a certificate must carry to sign code.
const CODE_SIGNING_EKU_OID: &str = "1.3.6.1.5.5.7.3.3";
/// Test signatures are not timestamped, so they are only trusted while the
/// signing certificate itself is valid. Replace the certificate well before it
/// expires instead of at the last moment.
const MIN_REMAINING_VALIDITY_DAYS: i64 = 90;

/// Signing mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignMode {
    /// Skip signing entirely.
    Off,
    /// Test-sign the driver artifacts.
    Test {
        /// When `true`, run `signtool verify` on the signed driver binary and
        /// catalog file after signing.
        verify_signature: bool,
        /// Additional `signtool sign` arguments.
        ///
        /// When empty, run `signtool sign` with the auto-generated WDR test
        /// certificate and default switches. When non-empty, auto generation
        /// is skipped and the caller owns the full signtool command line
        /// (certificate selection, digest, etc.).
        signtool_args: Vec<String>,
    },
}

/// Platform at which the device driver is targeted. See <https://learn.microsoft.com/en-us/windows-hardware/drivers/develop/target-platforms>
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetPlatform {
    Universal,
    Desktop,
    Windows,
}

impl TargetPlatform {
    /// Returns the `InfVerif` mode flag for this target platform.
    const fn as_infverif_flag(self) -> &'static str {
        match self {
            Self::Universal => "/u",
            Self::Desktop => "/h",
            Self::Windows => "/w",
        }
    }
}

/// A code signing certificate listed in the `WDRTestCertStore` store.
struct Certificate {
    thumbprint: String,
    expiry_in_days: i64,
}

impl Certificate {
    /// Days left before the certificate expires, negative once it has.
    const fn remaining_validity_days(&self, today_in_days: i64) -> i64 {
        self.expiry_in_days - today_in_days
    }

    const fn is_valid(&self, today_in_days: i64) -> bool {
        self.remaining_validity_days(today_in_days) >= MIN_REMAINING_VALIDITY_DAYS
    }
}

#[derive(Debug)]
pub struct PackageTaskParams<'a> {
    pub package_name: &'a str,
    pub working_dir: &'a Path,
    pub target_dir: &'a Path,
    pub target_arch: &'a CpuArchitecture,
    pub sign_mode: SignMode,
    pub inf2cat_args: Option<Vec<String>>,
    pub sample_class: bool,
    pub driver_model: DriverConfig,
    pub target_platform: TargetPlatform,
}

/// Supports low level driver packaging operations
pub struct PackageTask<'a> {
    package_name: String,
    sign_mode: SignMode,
    inf2cat_args: Option<Vec<String>>,
    sample_class: bool,

    // src paths
    src_inx_file_path: PathBuf,
    src_driver_binary_file_path: PathBuf,
    src_renamed_driver_binary_file_path: PathBuf,
    src_pdb_file_path: PathBuf,
    src_map_file_path: PathBuf,
    src_cert_file_path: PathBuf,

    // destination paths
    dest_root_package_folder: PathBuf,
    dest_inf_file_path: PathBuf,
    dest_driver_binary_path: PathBuf,
    dest_pdb_file_path: PathBuf,
    dest_map_file_path: PathBuf,
    dest_cert_file_path: PathBuf,
    dest_cat_file_path: PathBuf,

    arch: &'a CpuArchitecture,
    os_mapping: &'a str,
    driver_model: DriverConfig,
    target_platform: TargetPlatform,

    // Injected deps
    wdk_build: &'a WdkBuild,
    command_exec: &'a CommandExec,
    fs: &'a Fs,
}

impl<'a> PackageTask<'a> {
    /// Creates a new instance of `PackageTask`.
    ///
    /// # Arguments
    /// * `params` - Struct containing the parameters for the package task.
    /// * `wdk_build` - The provider for WDK build related methods.
    /// * `command_exec` - The provider for command execution.
    /// * `fs` - The provider for file system operations.
    ///
    /// # Returns
    /// * `Result<Self, PackageTaskError>` - A result containing the new
    ///   instance or an error.
    ///
    /// # Errors
    /// * `PackageTaskError::Io` - If there is an IO error while creating the
    ///   final package directory.
    ///
    /// # Panics
    /// * If `params.working_dir` is not absolute
    /// * If `params.target_dir` is not absolute
    pub fn new(
        params: PackageTaskParams<'a>,
        wdk_build: &'a WdkBuild,
        command_exec: &'a CommandExec,
        fs: &'a Fs,
    ) -> Self {
        debug!("Package task params: {params:?}");
        assert!(
            params.working_dir.is_absolute(),
            "Working directory path must be absolute. Input path: {}",
            params.working_dir.display()
        );
        assert!(
            params.target_dir.is_absolute(),
            "Target directory path must be absolute. Input path: {}",
            params.target_dir.display()
        );
        let package_name = params.package_name.replace('-', "_");
        // src paths
        let src_driver_binary_extension = "dll";
        let src_inx_file_path = params.working_dir.join(format!("{package_name}.inx"));

        // all paths inside target directory
        let src_driver_binary_file_path = params
            .target_dir
            .join(format!("{package_name}.{src_driver_binary_extension}"));
        let src_pdb_file_path = params.target_dir.join(format!("{package_name}.pdb"));
        let src_map_file_path = params
            .target_dir
            .join("deps")
            .join(format!("{package_name}.map"));
        let src_cert_file_path = params.target_dir.join(format!("{WDR_LOCAL_TEST_CERT}.cer"));

        // destination paths
        let dest_driver_binary_extension = match params.driver_model {
            DriverConfig::Kmdf(_) | DriverConfig::Wdm => "sys",
            DriverConfig::Umdf(_) => "dll",
        };

        let src_renamed_driver_binary_file_path = params
            .target_dir
            .join(format!("{package_name}.{dest_driver_binary_extension}"));
        let dest_root_package_folder: PathBuf =
            params.target_dir.join(format!("{package_name}_package"));
        let dest_inf_file_path = dest_root_package_folder.join(format!("{package_name}.inf"));
        let dest_driver_binary_path =
            dest_root_package_folder.join(format!("{package_name}.{dest_driver_binary_extension}"));
        let dest_pdb_file_path = dest_root_package_folder.join(format!("{package_name}.pdb"));
        let dest_map_file_path = dest_root_package_folder.join(format!("{package_name}.map"));
        let dest_cert_file_path =
            dest_root_package_folder.join(format!("{WDR_LOCAL_TEST_CERT}.cer"));
        let dest_cat_file_path = dest_root_package_folder.join(format!("{package_name}.cat"));

        let os_mapping = match params.target_arch {
            CpuArchitecture::Amd64 => "10_x64",
            CpuArchitecture::Arm64 => "Server10_arm64",
        };

        Self {
            package_name,
            sign_mode: params.sign_mode,
            inf2cat_args: params.inf2cat_args,
            sample_class: params.sample_class,
            src_inx_file_path,
            src_driver_binary_file_path,
            src_renamed_driver_binary_file_path,
            src_pdb_file_path,
            src_map_file_path,
            src_cert_file_path,
            dest_root_package_folder,
            dest_inf_file_path,
            dest_driver_binary_path,
            dest_pdb_file_path,
            dest_map_file_path,
            dest_cert_file_path,
            dest_cat_file_path,
            arch: params.target_arch,
            os_mapping,
            driver_model: params.driver_model,
            target_platform: params.target_platform,
            wdk_build,
            command_exec,
            fs,
        }
    }

    /// Entry point method to run the low level driver packaging operations.
    /// # Returns
    /// * `Result<(), PackageTaskError>` - A result indicating success or
    ///   failure.
    /// # Errors
    /// * `PackageTaskError::CopyFile` - If there is an error copying artifacts
    ///   to final package directory.
    /// * `PackageTaskError::CertGenerationInStoreCommand` - If there is an
    ///   error generating a certificate in the store.
    /// * `PackageTaskError::CreateCertFileFromStoreCommand` - If there is an
    ///   error creating a certificate file from the store.
    /// * `PackageTaskError::SigntoolSignCommand` - If there is an error signing
    ///   the driver binary or catalog file.
    /// * `PackageTaskError::DriverBinarySignVerificationCommand` - If there is
    ///   an error verifying the driver binary signature.
    /// * `PackageTaskError::Inf2CatCommand` - If there is an error running the
    ///   inf2cat command to generate the cat file.
    /// * `PackageTaskError::InfVerificationCommand` - If there is an error
    ///   verifying the inf file.
    /// * `PackageTaskError::MissingInxSrcFile` - If the .inx source file is
    ///   missing.
    /// * `PackageTaskError::StampinfCommand` - If there is an error running the
    ///   stampinf command to generate the inf file from the .inx template file.
    /// * `PackageTaskError::VerifyCertExistsInStoreCommand` - If there is an
    ///   error verifying if the certificate exists in the store.
    /// * `PackageTaskError::VerifyCertExistsInStoreInvalidCommandOutput`
    ///   - If the command output is invalid when verifying if the certificate
    ///     exists in the store.
    /// * `PackageTaskError::WdkBuildConfig` - If there is an error detecting
    ///   the WDK build number.
    /// * `PackageTaskError::Io` - Wraps all possible IO errors.
    pub fn run(&self) -> Result<(), PackageTaskError> {
        self.check_inx_exists()?;
        if self.fs.exists(&self.dest_root_package_folder) {
            debug!("Removing existing package folder");
            self.fs.remove_dir_all(&self.dest_root_package_folder)?;
        }
        debug!("Creating package folder");
        self.fs.create_dir(&self.dest_root_package_folder)?;
        info!(
            "Copying files to package folder: {}",
            self.dest_root_package_folder.to_string_lossy()
        );
        self.rename_driver_binary_extension()?;
        self.copy(
            &self.src_renamed_driver_binary_file_path,
            &self.dest_driver_binary_path,
        )?;
        self.copy(&self.src_pdb_file_path, &self.dest_pdb_file_path)?;
        self.copy(&self.src_inx_file_path, &self.dest_inf_file_path)?;
        self.copy(&self.src_map_file_path, &self.dest_map_file_path)?;
        self.run_stampinf()?;
        self.run_inf2cat()?;
        self.run_infverif()?;
        self.sign_and_verify()?;
        Ok(())
    }

    fn sign_and_verify(&self) -> Result<(), PackageTaskError> {
        let SignMode::Test {
            verify_signature,
            signtool_args,
        } = &self.sign_mode
        else {
            info!("Sign mode is 'off'; skipping signing");
            return Ok(());
        };
        let sign_args = if signtool_args.is_empty() {
            let thumbprint = self.generate_certificate()?;
            self.copy(&self.src_cert_file_path, &self.dest_cert_file_path)?;
            // Default WDR test-cert switches. The signature is deliberately not
            // timestamped: the certificate is generated locally for test
            // signing only, and requiring a timestamp server would
            // make every build depend on network access.
            [
                "/v",
                "/s",
                WDR_TEST_CERT_STORE,
                "/sha1",
                &thumbprint,
                "/fd",
                "SHA256",
            ]
            .map(ToString::to_string)
            .to_vec()
        } else {
            signtool_args.clone()
        };
        self.run_signtool_sign(&self.dest_driver_binary_path, &sign_args)?;
        self.run_signtool_sign(&self.dest_cat_file_path, &sign_args)?;
        if *verify_signature {
            info!("Verifying signatures for driver binary and cat file using signtool");
            self.run_signtool_verify(&self.dest_driver_binary_path)?;
            self.run_signtool_verify(&self.dest_cat_file_path)?;
        }
        Ok(())
    }

    fn check_inx_exists(&self) -> Result<(), PackageTaskError> {
        debug!(
            "Checking for .inx file, path: {}",
            self.src_inx_file_path.to_string_lossy()
        );
        if !self.fs.exists(&self.src_inx_file_path) {
            return Err(PackageTaskError::MissingInxSrcFile(
                self.src_inx_file_path.clone(),
            ));
        }
        Ok(())
    }

    fn rename_driver_binary_extension(&self) -> Result<(), FileError> {
        debug!("Renaming driver binary extension from .dll to .sys");
        self.fs.rename(
            &self.src_driver_binary_file_path,
            &self.src_renamed_driver_binary_file_path,
        )
    }

    fn copy(&self, src_file_path: &'a Path, dest_file_path: &'a Path) -> Result<u64, FileError> {
        debug!(
            "Copying src file {} to dest folder {}",
            src_file_path.to_string_lossy(),
            dest_file_path.to_string_lossy()
        );
        self.fs.copy(src_file_path, dest_file_path)
    }

    fn run_stampinf(&self) -> Result<(), PackageTaskError> {
        info!("Running stampinf");
        let wdf_version_flags = match self.driver_model {
            DriverConfig::Kmdf(kmdf_config) => {
                vec![
                    "-k".to_string(),
                    format!(
                        "{}.{}",
                        kmdf_config.kmdf_version_major, kmdf_config.target_kmdf_version_minor
                    ),
                ]
            }
            DriverConfig::Umdf(umdf_config) => vec![
                "-u".to_string(),
                format!(
                    "{}.{}.0",
                    umdf_config.umdf_version_major, umdf_config.target_umdf_version_minor
                ),
            ],
            DriverConfig::Wdm => vec![],
        };
        // TODO: Does it generate cat file relative to inf file path or we need
        // to provide the absolute path?
        let cat_file_path = format!("{}.cat", self.package_name);
        let dest_inf_file_path = self.dest_inf_file_path.to_string_lossy();
        let arch = self.arch.to_string();
        let mut args: Vec<&str> = vec![
            "-f",
            &dest_inf_file_path,
            "-d",
            "*",
            "-a",
            &arch,
            "-c",
            &cat_file_path,
        ];

        match std::env::var(STAMPINF_VERSION_ENV_VAR) {
            Ok(version) if !version.trim().is_empty() => {
                // When STAMPINF_VERSION is set to a non-empty, non-whitespace
                // value, we intentionally omit -v so stampinf
                // reads it and populates DriverVer.
                // (Whitespace-only values are ignored.)
                debug!(
                    DriverVer = version,
                    "Using {STAMPINF_VERSION_ENV_VAR} env var to set DriverVer"
                );
            }
            _ => {
                args.extend(["-v", "*"]);
            }
        }

        if !wdf_version_flags.is_empty() {
            args.append(&mut wdf_version_flags.iter().map(String::as_str).collect());
        }
        if let Err(e) = self.command_exec.run("stampinf", &args, None, None) {
            return Err(PackageTaskError::StampinfCommand(e));
        }
        Ok(())
    }

    fn run_inf2cat(&self) -> Result<(), PackageTaskError> {
        info!("Running inf2cat");
        let driver_arg = format!(
            "/driver:{}",
            self.dest_root_package_folder
                .to_string_lossy()
                .trim_start_matches("\\\\?\\")
        );

        let mut args: Vec<String> = vec![driver_arg];
        if let Some(inf2cat_args) = &self.inf2cat_args {
            args.extend(inf2cat_args.iter().cloned());
        } else {
            args.extend([
                format!("/os:{}", self.os_mapping),
                "/uselocaltime".to_string(),
            ]);
        }

        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        if let Err(e) = self.command_exec.run("inf2cat", &arg_refs, None, None) {
            return Err(PackageTaskError::Inf2CatCommand(e));
        }

        Ok(())
    }

    fn run_infverif(&self) -> Result<(), PackageTaskError> {
        let additional_args = if self.sample_class {
            let wdk_build_number = self.wdk_build.detect_wdk_build_number()?;
            match wdk_build_number {
                n if MISSING_SAMPLE_FLAG_WDK_BUILD_NUMBER_RANGE.contains(&n) => {
                    debug!(
                        "InfVerif in WDK Build {wdk_build_number} is buggy and does not contain \
                         the /samples flag."
                    );
                    warn!("InfVerif skipped for samples class. WDK Build: {wdk_build_number}");
                    return Ok(());
                }
                // Use the `/samples` flag after the range and the `/msft` flag before the range
                n if n > *MISSING_SAMPLE_FLAG_WDK_BUILD_NUMBER_RANGE.end() => "/samples",
                _ => "/msft",
            }
        } else {
            ""
        };

        info!("Running infverif");

        let mode_flag = self.target_platform.as_infverif_flag();

        let mut args = vec!["/v", mode_flag];
        let inf_path = self.dest_inf_file_path.to_string_lossy();

        if self.sample_class {
            args.push(additional_args);
        }
        args.push(&inf_path);

        if let Err(e) = self.command_exec.run("infverif", &args, None, None) {
            return Err(PackageTaskError::InfVerificationCommand(e));
        }

        Ok(())
    }

    /// Selects a usable test certificate from the store, creating one when
    /// none is left, exports it to the source certificate file and returns its
    /// SHA-1 thumbprint.
    fn generate_certificate(&self) -> Result<String, PackageTaskError> {
        // This mutex prevents multiple instances of this app from racing to
        // create a cert in the store. It is not a correctness problem. We
        // just don't want to litter the store with certs especially during
        // tests when there are lots of parallel runs
        let mutex_name = CString::new("WDRCertStoreMutex_bd345cf9330") // Unique enough
            .expect("string is a valid C string");
        debug!("Acquiring cert store mutex.");
        let _mutex = NamedMutex::acquire(&mutex_name)
            .map_err(|e| PackageTaskError::CertMutexError(e.code().0))?;
        debug!("Acquired cert store mutex");

        let certificate = if let Some(certificate) = self.find_valid_certificate_in_store()? {
            info!(
                "Using test certificate {} from {WDR_TEST_CERT_STORE} store",
                certificate.thumbprint
            );
            certificate
        } else {
            self.create_self_signed_cert_in_store()?;
            let certificate = self
                .find_valid_certificate_in_store()?
                .ok_or(PackageTaskError::NoUsableCertificate)?;
            info!(
                "Created test certificate {} in {WDR_TEST_CERT_STORE} store",
                certificate.thumbprint
            );
            certificate
        };

        self.export_certificate(&certificate.thumbprint)?;
        Ok(certificate.thumbprint)
    }

    /// Returns the first certificate in the store that is still usable for
    /// test signing, or `None` if none is found, or `PackageTaskError` if the
    /// store query fails.
    fn find_valid_certificate_in_store(&self) -> Result<Option<Certificate>, PackageTaskError> {
        debug!("Checking for a usable self signed certificate in {WDR_TEST_CERT_STORE} store");
        let args = ["-v", "-s", WDR_TEST_CERT_STORE];

        let output = self
            .command_exec
            .run("certmgr.exe", &args, None, None)
            .map_err(PackageTaskError::VerifyCertExistsInStoreCommand)?;
        if !output.status.success() {
            return Ok(None);
        }
        let stdout = String::from_utf8(output.stdout)
            .map_err(PackageTaskError::VerifyCertExistsInStoreInvalidCommandOutput)?;

        let today = today_in_days();
        let certificates = parse_certificates(&stdout, WDR_LOCAL_TEST_CERT);
        for certificate in &certificates {
            trace!(
                thumbprint = %certificate.thumbprint,
                remaining_validity_days = certificate.remaining_validity_days(today),
                "Found test certificate in store"
            );
        }
        Ok(certificates
            .into_iter()
            .find(|certificate| certificate.is_valid(today)))
    }

    fn create_self_signed_cert_in_store(&self) -> Result<(), PackageTaskError> {
        info!("Creating self signed certificate in WDRTestCertStore store using makecert");
        let args = [
            "-r",
            "-pe",
            "-a",
            "SHA256",
            "-eku",
            CODE_SIGNING_EKU_OID,
            "-m",
            CERT_VALIDITY_MONTHS,
            "-ss",
            WDR_TEST_CERT_STORE, // FIXME: this should be a parameter
            "-n",
            &format!("CN={WDR_LOCAL_TEST_CERT}"), // FIXME: this should be a parameter
        ];
        if let Err(e) = self.command_exec.run("makecert", &args, None, None) {
            return Err(PackageTaskError::CertGenerationInStoreCommand(e));
        }
        Ok(())
    }

    fn export_certificate(&self, thumbprint: &str) -> Result<(), PackageTaskError> {
        info!("Exporting test certificate {thumbprint} from {WDR_TEST_CERT_STORE} store");
        let cert_path = self.src_cert_file_path.to_string_lossy();
        let args = [
            "-put",
            "-s",
            WDR_TEST_CERT_STORE,
            "-c",
            "-sha1",
            thumbprint,
            &cert_path,
        ];
        if let Err(e) = self.command_exec.run("certmgr.exe", &args, None, None) {
            return Err(PackageTaskError::CreateCertFileFromStoreCommand(e));
        }
        Ok(())
    }

    /// Signs the file with `signtool` by executing the following command:
    /// `sign <signtool_args...> <file_path>`
    ///
    /// # Arguments
    ///
    /// * `file_path` - The path to the file to be signed.
    /// * `signtool_args` - The full `signtool sign` argument list to use.
    ///
    /// # Errors
    /// * `PackageTaskError::SigntoolSignCommand` - If there is an error signing
    ///   the file with `signtool`.
    fn run_signtool_sign(
        &self,
        file_path: &Path,
        signtool_args: &[String],
    ) -> Result<(), PackageTaskError> {
        info!(
            "Signing {} using signtool",
            file_path
                .file_name()
                .expect("Unable to read file name from the path")
                .to_string_lossy()
        );
        let file_path_buf = file_path.to_path_buf();
        let file_path = file_path.to_string_lossy().into_owned();

        let mut args: Vec<String> = vec!["sign".to_string()];
        args.extend(signtool_args.iter().cloned());
        // File operand (must be last).
        args.push(file_path);

        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();

        // Determine the indices of password values (the token right after each
        // `/p`) so they can be redacted by `run_with_redaction` in the logs.
        // `value_index < file_operand_index` ensures a value token
        // actually follows `/p` and that it is never the trailing file operand.
        let file_operand_index = arg_refs.len() - 1;
        let redaction_indices: Vec<usize> = arg_refs
            .iter()
            .enumerate()
            .filter_map(|(i, arg)| {
                let value_index = i + 1;
                (arg.eq_ignore_ascii_case("/p") && value_index < file_operand_index)
                    .then_some(value_index)
            })
            .collect();
        if let Err(e) = self.command_exec.run_with_redaction(
            "signtool",
            &arg_refs,
            &redaction_indices,
            None,
            None,
        ) {
            return Err(PackageTaskError::SigntoolSignCommand {
                file: file_path_buf,
                source: e,
            });
        }
        Ok(())
    }

    fn run_signtool_verify(&self, file_path: &Path) -> Result<(), PackageTaskError> {
        info!(
            "Verifying {} using signtool",
            file_path
                .file_name()
                .expect("Unable to read file name from the path")
                .to_string_lossy()
        );
        let driver_binary_file_path = file_path.to_string_lossy();
        let args = ["verify", "/v", "/pa", &driver_binary_file_path];
        // TODO: Differentiate between command exec failure and signature
        // verification failure
        if let Err(e) = self.command_exec.run("signtool", &args, None, None) {
            return Err(PackageTaskError::DriverBinarySignVerificationCommand(e));
        }
        Ok(())
    }
}

/// Module that contains code that parses the output of `certmgr -v -s <store>`.
mod certmgr_parser {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{CODE_SIGNING_EKU_OID, Certificate};

    /// Certificates in `listing`, the output of `certmgr -v -s <store>`, whose
    /// subject is exactly `subject` and that are able to sign code.
    pub fn parse_certificates(listing: &str, subject: &str) -> Vec<Certificate> {
        listing
            .split("==============Certificate #")
            .skip(1)
            .filter_map(|record| parse_certificate(record, subject))
            .collect()
    }

    /// Days since the Unix epoch, used as the reference point for validity
    /// checks.
    pub fn today_in_days() -> i64 {
        let days = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since_epoch| since_epoch.as_secs() / 86_400);
        i64::try_from(days).unwrap_or(0)
    }

    fn parse_certificate(record: &str, subject: &str) -> Option<Certificate> {
        if !subject_matches(record, subject)
            // Printed only when the certificate has private key provider info.
            || !record.contains("Provider Type::")
            || !record.contains(CODE_SIGNING_EKU_OID)
        {
            return None;
        }
        Some(Certificate {
            thumbprint: thumbprint(record)?,
            expiry_in_days: not_after_in_days(record)?,
        })
    }

    /// Reads the `SHA1 Thumbprint::` value, which `certmgr` prints on the
    /// following line in space separated groups.
    fn thumbprint(record: &str) -> Option<String> {
        let thumbprint_index = record.find("SHA1 Thumbprint::")?;
        let value = record[thumbprint_index..]
            .lines()
            .skip(1)
            .map(str::trim)
            .find(|line| !line.is_empty())?;
        Some(value.split_whitespace().collect::<String>().to_uppercase())
    }

    /// Matches a subject of exactly `CN=<subject>`, so a certificate that
    /// merely contains that text in a longer name is not reused.
    fn subject_matches(record: &str, subject: &str) -> bool {
        let Some(subject_section) = section_between(record, "Subject::", "Issuer::") else {
            return false;
        };
        let mut values = subject_section.lines().filter_map(rdn_value);
        values.next() == Some(subject) && values.next().is_none()
    }

    fn not_after_in_days(record: &str) -> Option<i64> {
        let not_after_index = record.find("NotAfter::")?;
        let value = record[not_after_index..]
            .lines()
            .skip(1)
            .map(str::trim)
            .find(|line| !line.is_empty())?;
        parse_certmgr_date(value)
    }

    /// Parses a `certmgr` timestamp such as `Sun Jan 01 05:29:59 2040` into
    /// days since the Unix epoch. `certmgr` prints local time, which is precise
    /// enough for a validity margin measured in months.
    fn parse_certmgr_date(value: &str) -> Option<i64> {
        const MONTHS: [&str; 12] = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];

        let mut fields = value.split_whitespace();
        let _weekday = fields.next()?;
        let month_name = fields.next()?;
        let month = u32::try_from(MONTHS.iter().position(|month| *month == month_name)?).ok()? + 1;
        let day = fields.next()?.parse::<u32>().ok()?;
        let _time_of_day = fields.next()?;
        let year = fields.next()?.parse::<i64>().ok()?;

        Some(days_from_civil(year, month, day))
    }

    /// Days from the Unix epoch to `year-month-day`, using Howard Hinnant's
    /// `days_from_civil` algorithm.
    fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
        let year = if month <= 2 { year - 1 } else { year };
        let era = if year >= 0 { year } else { year - 399 } / 400;
        let year_of_era = year - era * 400;
        let shifted_month = i64::from((month + 9) % 12);
        let day_of_year = (153 * shifted_month + 2) / 5 + i64::from(day) - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        era * 146_097 + day_of_era - 719_468
    }

    fn section_between<'a>(record: &'a str, start: &str, end: &str) -> Option<&'a str> {
        let start_index = record.find(start)? + start.len();
        let rest = &record[start_index..];
        let end_index = rest.find(end)?;
        Some(&rest[..end_index])
    }

    /// Extracts the ASCII rendering `certmgr` prints beside each RDN value.
    fn rdn_value(line: &str) -> Option<&str> {
        let start_index = line.find('\'')? + 1;
        let rest = &line[start_index..];
        let end_index = rest.rfind('\'')?;
        Some(&rest[..end_index])
    }

    #[cfg(test)]
    mod tests {
        use super::{super::MIN_REMAINING_VALIDITY_DAYS, *};

        const TODAY: i64 = 20_689; // 2026-08-24
        const SUBJECT: &str = "WDRLocalTestCert";
        const THUMBPRINT: &str = "32CA754DC16D56FB15363275147EDB3D211A91FC";
        const PROVIDER: &str = "Provider Type:: 1 Provider Name:: Microsoft Strong Cryptographic \
                                Provider Container: c75ab649 KeySpec: 2\n";

        fn listing(subject: &str, not_after: &str, extras: &str) -> String {
            format!(
                "==============Certificate # 1 ==========\nSubject::\n[0,0] 2.5.4.3 (CN) \
                 ValueType: 4\n57 44 52    '{subject}'\nIssuer::\n[0,0] 2.5.4.3 (CN) ValueType: \
                 4\n57 44 52    '{subject}'\nSHA1 Thumbprint::\n32CA754D C16D56FB 15363275 \
                 147EDB3D 211A91FC\n{extras}NotBefore::\nTue Jan 20 20:48:04 \
                 2026\nNotAfter::\n{not_after}\nExtension[0] 2.5.29.37(Enhanced Key Usage) \
                 Critical:  False::\nCode Signing \
                 (1.3.6.1.5.5.7.3.3)\n==============================================\nCertMgr \
                 Succeeded"
            )
        }

        fn parse_one(listing: &str) -> Option<Certificate> {
            parse_certificates(listing, SUBJECT).into_iter().next()
        }

        #[test]
        fn parses_thumbprint_and_expiry() {
            let listing = listing(SUBJECT, "Sun Jan 01 05:29:59 2040", PROVIDER);
            let certificate = parse_one(&listing).expect("certificate should be parsed");
            assert_eq!(certificate.thumbprint, THUMBPRINT);
            assert_eq!(certificate.expiry_in_days, days_from_civil(2040, 1, 1));
        }

        #[test]
        fn accepts_certificate_with_long_validity() {
            let listing = listing(SUBJECT, "Sun Jan 01 05:29:59 2040", PROVIDER);
            assert!(parse_one(&listing).is_some_and(|c| c.is_valid(TODAY)));
        }

        #[test]
        fn rejects_expired_certificate() {
            let listing = listing(SUBJECT, "Wed Jan 01 05:29:59 2020", PROVIDER);
            assert!(!parse_one(&listing).is_some_and(|c| c.is_valid(TODAY)));
        }

        #[test]
        fn rejects_certificate_expiring_within_the_margin() {
            let listing = listing(SUBJECT, "Mon Sep 21 05:29:59 2026", PROVIDER);
            assert!(!parse_one(&listing).is_some_and(|c| c.is_valid(TODAY)));
        }

        #[test]
        fn accepts_certificate_exactly_at_the_margin() {
            let listing = listing(SUBJECT, "Sun Nov 22 05:29:59 2026", PROVIDER);
            let certificate = parse_one(&listing).expect("certificate should be parsed");
            assert_eq!(
                certificate.remaining_validity_days(TODAY),
                MIN_REMAINING_VALIDITY_DAYS
            );
            assert!(certificate.is_valid(TODAY));
        }

        #[test]
        fn skips_certificate_without_private_key() {
            let listing = listing(SUBJECT, "Sun Jan 01 05:29:59 2040", "");
            assert!(parse_certificates(&listing, SUBJECT).is_empty());
        }

        #[test]
        fn skips_certificate_with_different_subject() {
            let listing = listing("WDR Test", "Sun Jan 01 05:29:59 2040", PROVIDER);
            assert!(parse_certificates(&listing, SUBJECT).is_empty());
        }

        #[test]
        fn skips_subject_that_merely_contains_the_expected_name() {
            let listing = listing("WDRLocalTestCertOld", "Sun Jan 01 05:29:59 2040", PROVIDER);
            assert!(parse_certificates(&listing, SUBJECT).is_empty());
        }

        #[test]
        fn skips_certificate_without_code_signing_eku() {
            let listing = listing(SUBJECT, "Sun Jan 01 05:29:59 2040", PROVIDER)
                .replace(CODE_SIGNING_EKU_OID, "1.3.6.1.5.5.7.3.1");
            assert!(parse_certificates(&listing, SUBJECT).is_empty());
        }

        #[test]
        fn parses_empty_store() {
            let listing = "==============No Certificates \
                           ==========\n==============================================\nCertMgr \
                           Succeeded";
            assert!(parse_certificates(listing, SUBJECT).is_empty());
        }

        #[test]
        fn parses_only_matching_certificates_when_the_store_has_several() {
            let unusable = listing("WDR Test", "Sun Jan 01 05:29:59 2040", PROVIDER);
            let usable = listing(SUBJECT, "Sun Jan 01 05:29:59 2040", PROVIDER);
            let certificates = parse_certificates(&format!("{unusable}{usable}"), SUBJECT);
            assert_eq!(certificates.len(), 1);
            assert_eq!(certificates[0].thumbprint, THUMBPRINT);
        }

        #[test]
        fn parses_certmgr_dates() {
            assert_eq!(
                parse_certmgr_date("Sun Jan 01 05:29:59 2040"),
                Some(days_from_civil(2040, 1, 1))
            );
            assert_eq!(
                parse_certmgr_date("Tue Jan 20 20:48:04 2026"),
                Some(days_from_civil(2026, 1, 20))
            );
            assert_eq!(parse_certmgr_date("not a date"), None);
        }

        #[test]
        fn days_from_civil_matches_known_epochs() {
            assert_eq!(days_from_civil(1970, 1, 1), 0);
            assert_eq!(days_from_civil(1970, 1, 2), 1);
            assert_eq!(days_from_civil(2000, 3, 1), 11_017);
            assert_eq!(days_from_civil(2026, 8, 24), TODAY);
        }
    }
}

/// An RAII wrapper over a Win API named mutex
struct NamedMutex {
    handle: HANDLE,
    // `ReleaseMutex` requires that it is called
    // only by threads that own the mutex handle.
    // Being `!Send` ensures that's always the case.
    _not_send: PhantomData<*const ()>,
}

impl NamedMutex {
    /// Acquires named mutex
    pub fn acquire(name: &CStr) -> Result<Self, WinError> {
        fn get_last_error() -> WinError {
            // SAFETY: We have to just assume this function is safe to call
            // because the windows crate has no documentation for it and
            // the MSDN documentation does not specify any preconditions
            // for calling it
            unsafe { GetLastError().into() }
        }

        // SAFETY: The name ptr is valid because it comes from a CStr
        let handle = unsafe { CreateMutexA(None, false, PCSTR(name.as_ptr().cast()))? };
        if handle.is_invalid() {
            return Err(get_last_error());
        }

        // SAFETY: The handle is valid since it was created right above
        match unsafe { WaitForSingleObject(handle, INFINITE) } {
            res if res == WAIT_OBJECT_0 || res == WAIT_ABANDONED => Ok(Self {
                handle,
                _not_send: PhantomData,
            }),
            _ => {
                // SAFETY: The handle is valid since it was created right above
                unsafe { CloseHandle(handle)? };
                Err(get_last_error())
            }
        }
    }
}

impl Drop for NamedMutex {
    fn drop(&mut self) {
        // SAFETY: the handle is guaranteed to be valid
        // because this type itself created it and it
        // was never exposed outside. Also the requirement
        // that the calling thread must own the handle
        // is upheld because this type is `!Send`
        let _ = unsafe { ReleaseMutex(self.handle) };

        // SAFETY: the handle is valid as explained above.
        let _ = unsafe { CloseHandle(self.handle) };
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        process::{ExitStatus, Output},
    };

    use wdk_build::{CpuArchitecture, KmdfConfig, UmdfConfig};

    use super::*;

    #[test]
    fn new_succeeds_for_valid_args() {
        let package_name = "test_package";
        let working_dir = PathBuf::from("D:/absolute/path/to/working/dir");
        let target_dir = PathBuf::from("C:/absolute/path/to/target/dir");
        let arch = CpuArchitecture::Amd64;

        let package_task_params = PackageTaskParams {
            package_name,
            working_dir: &working_dir,
            target_dir: &target_dir,
            target_arch: &arch,
            driver_model: DriverConfig::Kmdf(KmdfConfig::default()),
            sample_class: false,
            sign_mode: SignMode::Test {
                verify_signature: false,
                signtool_args: Vec::new(),
            },
            inf2cat_args: None,
            target_platform: TargetPlatform::Universal,
        };
        let dest_root = target_dir.join(format!("{package_name}_package"));

        let command_exec = CommandExec::default();
        let wdk_build = WdkBuild::default();
        let fs = Fs::default();
        let task = PackageTask::new(package_task_params, &wdk_build, &command_exec, &fs);
        assert_eq!(task.package_name, package_name.replace('-', "_"));
        assert_eq!(
            task.sign_mode,
            SignMode::Test {
                verify_signature: false,
                signtool_args: Vec::new(),
            }
        );
        assert!(!task.sample_class);
        assert_eq!(task.src_inx_file_path, working_dir.join("test_package.inx"));
        assert_eq!(
            task.src_driver_binary_file_path,
            target_dir.join("test_package.dll")
        );
        assert_eq!(
            task.src_renamed_driver_binary_file_path,
            target_dir.join("test_package.sys")
        );
        assert_eq!(task.src_pdb_file_path, target_dir.join("test_package.pdb"));
        assert_eq!(
            task.src_map_file_path,
            target_dir.join("deps").join("test_package.map")
        );
        assert_eq!(
            task.src_cert_file_path,
            target_dir.join("WDRLocalTestCert.cer")
        );
        assert_eq!(task.dest_root_package_folder, dest_root);
        assert_eq!(task.dest_inf_file_path, dest_root.join("test_package.inf"));
        assert_eq!(
            task.dest_driver_binary_path,
            dest_root.join("test_package.sys")
        );
        assert_eq!(task.dest_pdb_file_path, dest_root.join("test_package.pdb"));
        assert_eq!(task.dest_map_file_path, dest_root.join("test_package.map"));
        assert_eq!(
            task.dest_cert_file_path,
            dest_root.join("WDRLocalTestCert.cer")
        );
        assert_eq!(task.dest_cat_file_path, dest_root.join("test_package.cat"));
        assert_eq!(*task.arch, arch);
        assert_eq!(task.os_mapping, "10_x64");
        assert!(matches!(task.driver_model, DriverConfig::Kmdf(_)));
    }

    #[test]
    #[should_panic(expected = "Target directory path must be absolute. Input path: \
                               ../relative/path/to/target/dir")]
    fn new_panics_when_target_dir_is_not_absolute() {
        let package_name = "test_package";
        let working_dir = PathBuf::from("C:/absolute/path/to/working/dir");
        let target_dir = PathBuf::from("../relative/path/to/target/dir");
        let arch = CpuArchitecture::Amd64;

        let package_task_params = PackageTaskParams {
            package_name,
            working_dir: &working_dir,
            target_dir: &target_dir,
            target_arch: &arch,
            driver_model: DriverConfig::Kmdf(KmdfConfig::default()),
            sample_class: false,
            sign_mode: SignMode::Test {
                verify_signature: false,
                signtool_args: Vec::new(),
            },
            inf2cat_args: None,
            target_platform: TargetPlatform::Universal,
        };

        let command_exec = CommandExec::default();
        let wdk_build = WdkBuild::default();
        let fs = Fs::default();

        PackageTask::new(package_task_params, &wdk_build, &command_exec, &fs);
    }

    #[test]
    #[should_panic(expected = "Working directory path must be absolute. Input path: \
                               relative/path/to/working/dir")]
    fn new_panics_when_working_dir_is_not_absolute() {
        let package_name = "test_package";
        let working_dir = PathBuf::from("relative/path/to/working/dir");
        let target_dir = PathBuf::from("E:/absolute/path/to/target/dir");
        let arch = CpuArchitecture::Amd64;

        let package_task_params = PackageTaskParams {
            package_name,
            working_dir: &working_dir,
            target_dir: &target_dir,
            target_arch: &arch,
            driver_model: DriverConfig::Kmdf(KmdfConfig::default()),
            sample_class: false,
            sign_mode: SignMode::Test {
                verify_signature: false,
                signtool_args: Vec::new(),
            },
            inf2cat_args: None,
            target_platform: TargetPlatform::Universal,
        };

        let command_exec = CommandExec::default();
        let wdk_build = WdkBuild::default();
        let fs = Fs::default();

        PackageTask::new(package_task_params, &wdk_build, &command_exec, &fs);
    }

    #[test]
    fn stampinf_version_overrides_with_env_var() {
        // verify both with and without the env var set scenarios
        let scenarios = [
            ("env_set", Some("1.2.3.4"), true),
            ("env_empty", Some(""), false),
            ("env_spaces", Some("  "), false),
            ("env_unset", None, false),
        ];

        for (name, env_val, expect_skip_v) in scenarios {
            let result =
                crate::test_utils::with_env(&[(STAMPINF_VERSION_ENV_VAR, env_val)], || {
                    let package_name = "driver";
                    let working_dir = PathBuf::from("C:/abs/driver");
                    let target_dir = PathBuf::from("C:/abs/driver/target/debug");
                    let arch = CpuArchitecture::Amd64;

                    let params = PackageTaskParams {
                        package_name,
                        working_dir: &working_dir,
                        target_dir: &target_dir,
                        target_arch: &arch,
                        driver_model: DriverConfig::Kmdf(KmdfConfig::default()),
                        sample_class: false,
                        sign_mode: SignMode::Test {
                            verify_signature: false,
                            signtool_args: Vec::new(),
                        },
                        inf2cat_args: None,
                        target_platform: TargetPlatform::Universal,
                    };

                    let wdk_build = WdkBuild::default();
                    let fs = Fs::default();
                    let mut command_exec = CommandExec::default();

                    command_exec
                        .expect_run()
                        .withf(move |cmd: &str, args: &[&str], _, _| {
                            if cmd != "stampinf" {
                                return false;
                            }
                            let has_v = args.contains(&"-v");
                            if expect_skip_v {
                                !has_v
                            } else {
                                args.windows(2).any(|w| w == ["-v", "*"])
                            }
                        })
                        .once()
                        .return_once(|_, _, _, _| {
                            Ok(Output {
                                status: ExitStatus::default(),
                                stdout: vec![],
                                stderr: vec![],
                            })
                        });

                    let task = PackageTask::new(params, &wdk_build, &command_exec, &fs);
                    task.run_stampinf()
                });

            assert!(
                result.is_ok(),
                "scenario {name} failed (env_set={env_val:?})"
            );
        }
    }

    #[test]
    fn run_inf2cat_with_no_args_uses_arch_os_and_uselocaltime() {
        let working_dir = PathBuf::from("C:/abs/driver");
        let target_dir = PathBuf::from("C:/abs/driver/target/debug");
        let arch = CpuArchitecture::Amd64;

        let params = PackageTaskParams {
            package_name: "driver",
            working_dir: &working_dir,
            target_dir: &target_dir,
            target_arch: &arch,
            driver_model: DriverConfig::Kmdf(KmdfConfig::default()),
            sample_class: false,
            sign_mode: SignMode::Test {
                verify_signature: false,
                signtool_args: Vec::new(),
            },
            inf2cat_args: None,
            target_platform: TargetPlatform::Universal,
        };

        let wdk_build = WdkBuild::default();
        let fs = Fs::default();
        let mut command_exec = CommandExec::default();
        command_exec
            .expect_run()
            .withf(move |cmd: &str, args: &[&str], _, _| {
                cmd == "inf2cat"
                    && args[0].starts_with("/driver:")
                    && args.contains(&"/os:10_x64")
                    && args.contains(&"/uselocaltime")
            })
            .once()
            .return_once(|_, _, _, _| {
                Ok(Output {
                    status: ExitStatus::default(),
                    stdout: vec![],
                    stderr: vec![],
                })
            });

        let task = PackageTask::new(params, &wdk_build, &command_exec, &fs);
        assert!(task.run_inf2cat().is_ok());
    }

    #[test]
    fn run_inf2cat_with_empty_custom_args_passes_only_driver_arg() {
        let working_dir = PathBuf::from("C:/abs/driver");
        let target_dir = PathBuf::from("C:/abs/driver/target/debug");
        let arch = CpuArchitecture::Amd64;

        let params = PackageTaskParams {
            package_name: "driver",
            working_dir: &working_dir,
            target_dir: &target_dir,
            target_arch: &arch,
            driver_model: DriverConfig::Kmdf(KmdfConfig::default()),
            sample_class: false,
            sign_mode: SignMode::Test {
                verify_signature: false,
                signtool_args: Vec::new(),
            },
            inf2cat_args: Some(Vec::new()),
            target_platform: TargetPlatform::Universal,
        };

        let wdk_build = WdkBuild::default();
        let fs = Fs::default();
        let mut command_exec = CommandExec::default();
        command_exec
            .expect_run()
            .withf(move |cmd: &str, args: &[&str], _, _| {
                cmd == "inf2cat" && args.len() == 1 && args[0].starts_with("/driver:")
            })
            .once()
            .return_once(|_, _, _, _| {
                Ok(Output {
                    status: ExitStatus::default(),
                    stdout: vec![],
                    stderr: vec![],
                })
            });

        let task = PackageTask::new(params, &wdk_build, &command_exec, &fs);
        assert!(task.run_inf2cat().is_ok());
    }

    #[test]
    fn run_inf2cat_with_custom_args_forwards_them_verbatim() {
        let working_dir = PathBuf::from("C:/abs/driver");
        let target_dir = PathBuf::from("C:/abs/driver/target/debug");
        let arch = CpuArchitecture::Amd64;

        let params = PackageTaskParams {
            package_name: "driver",
            working_dir: &working_dir,
            target_dir: &target_dir,
            target_arch: &arch,
            driver_model: DriverConfig::Kmdf(KmdfConfig::default()),
            sample_class: false,
            sign_mode: SignMode::Test {
                verify_signature: false,
                signtool_args: Vec::new(),
            },
            inf2cat_args: Some(vec![
                "/os:10_x64,10_CO_X64".to_string(),
                "/verbose".to_string(),
            ]),
            target_platform: TargetPlatform::Universal,
        };

        let wdk_build = WdkBuild::default();
        let fs = Fs::default();
        let mut command_exec = CommandExec::default();
        command_exec
            .expect_run()
            .withf(move |cmd: &str, args: &[&str], _, _| {
                cmd == "inf2cat"
                    && args[0].starts_with("/driver:")
                    && args.contains(&"/os:10_x64,10_CO_X64")
                    && args.contains(&"/verbose")
                    && !args.contains(&"/uselocaltime")
                    && !args.contains(&"/os:10_x64")
            })
            .once()
            .return_once(|_, _, _, _| {
                Ok(Output {
                    status: ExitStatus::default(),
                    stdout: vec![],
                    stderr: vec![],
                })
            });

        let task = PackageTask::new(params, &wdk_build, &command_exec, &fs);
        assert!(task.run_inf2cat().is_ok());
    }

    #[test]
    fn target_platform_maps_to_infverif_flag() {
        assert_eq!(TargetPlatform::Universal.as_infverif_flag(), "/u");
        assert_eq!(TargetPlatform::Desktop.as_infverif_flag(), "/h");
        assert_eq!(TargetPlatform::Windows.as_infverif_flag(), "/w");
    }

    mod signtool {
        use super::*;

        // Builds a minimal `PackageTask` suitable for exercising
        // `run_signtool_sign` in isolation. The signing method does not
        // read `sign_mode`, so `Off` is used here; the caller passes
        // the signtool argument slice under test directly.
        fn create_package_task<'a>(
            wdk_build: &'a WdkBuild,
            command_exec: &'a CommandExec,
            fs: &'a Fs,
            arch: &'a CpuArchitecture,
        ) -> PackageTask<'a> {
            let params = PackageTaskParams {
                package_name: "driver",
                working_dir: Path::new("C:/abs/working"),
                target_dir: Path::new("C:/abs/target"),
                target_arch: arch,
                driver_model: DriverConfig::Kmdf(KmdfConfig::default()),
                sample_class: false,
                sign_mode: SignMode::Off,
                inf2cat_args: None,
                target_platform: TargetPlatform::Universal,
            };
            PackageTask::new(params, wdk_build, command_exec, fs)
        }

        // Returns a mocked `CommandExec` that expects a single `signtool`
        // invocation with exactly the provided argument vector and
        // redaction indices.
        fn expect_signtool_args(
            expected: Vec<String>,
            expected_redaction_indices: Vec<usize>,
        ) -> CommandExec {
            let mut command_exec = CommandExec::default();
            command_exec
                .expect_run_with_redaction()
                .withf(move |command, args, redaction_indices, _env, _cwd| {
                    command == "signtool"
                        && args == expected
                        && redaction_indices == expected_redaction_indices.as_slice()
                })
                .once()
                .returning(|_, _, _, _, _| {
                    Ok(Output {
                        status: ExitStatus::default(),
                        stdout: vec![],
                        stderr: vec![],
                    })
                });
            command_exec
        }

        #[test]
        fn sign_with_custom_args_forwards_them_verbatim() {
            let arch = CpuArchitecture::Amd64;
            let command_exec = expect_signtool_args(
                [
                    "sign",
                    "/fd",
                    "SHA384",
                    "/f",
                    "C:/certs/my.pfx",
                    "/p",
                    "secret",
                    "C:/pkg/driver.sys",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                vec![6],
            );
            let wdk_build = WdkBuild::default();
            let fs = Fs::default();
            let task = create_package_task(&wdk_build, &command_exec, &fs, &arch);

            let signtool_args = [
                "/fd".to_string(),
                "SHA384".to_string(),
                "/f".to_string(),
                "C:/certs/my.pfx".to_string(),
                "/p".to_string(),
                "secret".to_string(),
            ];
            task.run_signtool_sign(Path::new("C:/pkg/driver.sys"), &signtool_args)
                .expect("signing should succeed");
        }

        #[test]
        fn sign_redacts_password_value_arg_by_redaction_index() {
            let arch = CpuArchitecture::Amd64;
            let command_exec = expect_signtool_args(
                [
                    "sign",
                    "/f",
                    "cert.pfx",
                    "/p",
                    "secret",
                    "/fd",
                    "SHA256",
                    "C:/pkg/driver.sys",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                vec![4],
            );
            let wdk_build = WdkBuild::default();
            let fs = Fs::default();
            let task = create_package_task(&wdk_build, &command_exec, &fs, &arch);

            let signtool_args = [
                "/f".to_string(),
                "cert.pfx".to_string(),
                "/p".to_string(),
                "secret".to_string(),
                "/fd".to_string(),
                "SHA256".to_string(),
            ];
            task.run_signtool_sign(Path::new("C:/pkg/driver.sys"), &signtool_args)
                .expect("signing should succeed");
        }

        #[test]
        fn sign_redacts_password_value_case_insensitively() {
            let arch = CpuArchitecture::Amd64;
            let command_exec = expect_signtool_args(
                [
                    "sign",
                    "/f",
                    "cert.pfx",
                    "/P",
                    "secret",
                    "/fd",
                    "SHA256",
                    "C:/pkg/driver.sys",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                vec![4],
            );
            let wdk_build = WdkBuild::default();
            let fs = Fs::default();
            let task = create_package_task(&wdk_build, &command_exec, &fs, &arch);

            let signtool_args = [
                "/f".to_string(),
                "cert.pfx".to_string(),
                "/P".to_string(),
                "secret".to_string(),
                "/fd".to_string(),
                "SHA256".to_string(),
            ];
            task.run_signtool_sign(Path::new("C:/pkg/driver.sys"), &signtool_args)
                .expect("signing should succeed");
        }

        #[test]
        fn sign_does_not_redact_file_operand_if_password_is_missing() {
            let arch = CpuArchitecture::Amd64;
            let command_exec = expect_signtool_args(
                ["sign", "/f", "cert.pfx", "/p", "C:/pkg/driver.sys"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                vec![],
            );
            let wdk_build = WdkBuild::default();
            let fs = Fs::default();
            let task = create_package_task(&wdk_build, &command_exec, &fs, &arch);

            let signtool_args = ["/f".to_string(), "cert.pfx".to_string(), "/p".to_string()];
            task.run_signtool_sign(Path::new("C:/pkg/driver.sys"), &signtool_args)
                .expect("signing should succeed");
        }

        #[test]
        fn sign_and_verify_fails_when_signtool_fails() {
            let arch = CpuArchitecture::Amd64;
            let mut command_exec = CommandExec::default();
            command_exec
                .expect_run_with_redaction()
                .withf(|command, args, _redaction_indices, _env, _cwd| {
                    command == "signtool" && args.first() == Some(&"sign")
                })
                .once()
                .returning(|_, _, _, _, _| {
                    Err(crate::providers::error::CommandError::CommandFailed {
                        command: "signtool".to_string(),
                        args: vec![],
                        stdout: String::new(),
                    })
                });

            let fs = Fs::default();

            let wdk_build = WdkBuild::default();
            let working_dir = PathBuf::from("C:/abs/working");
            let target_dir = PathBuf::from("C:/abs/target");
            let params = PackageTaskParams {
                package_name: "driver",
                working_dir: &working_dir,
                target_dir: &target_dir,
                target_arch: &arch,
                driver_model: DriverConfig::Kmdf(KmdfConfig::default()),
                sample_class: false,
                sign_mode: SignMode::Test {
                    verify_signature: false,
                    signtool_args: vec![
                        "/s".to_string(),
                        "MyStore".to_string(),
                        "/n".to_string(),
                        "MyCert".to_string(),
                        "/fd".to_string(),
                        "SHA256".to_string(),
                    ],
                },
                inf2cat_args: None,
                target_platform: TargetPlatform::Universal,
            };
            let task = PackageTask::new(params, &wdk_build, &command_exec, &fs);

            assert!(task.sign_and_verify().is_err());
        }
    }

    fn assert_infverif_mode_flag(
        driver_model: DriverConfig,
        target_platform: TargetPlatform,
        expected_mode_flag: &'static str,
    ) {
        let package_name = "driver";
        let working_dir = PathBuf::from("C:/abs/driver");
        let target_dir = PathBuf::from("C:/abs/driver/target/debug");
        let arch = CpuArchitecture::Amd64;

        let params = PackageTaskParams {
            package_name,
            working_dir: &working_dir,
            target_dir: &target_dir,
            target_arch: &arch,
            driver_model,
            sample_class: false,
            sign_mode: SignMode::Off,
            inf2cat_args: None,
            target_platform,
        };

        let fs = Fs::default();
        let wdk_build = WdkBuild::default();

        let mut command_exec = CommandExec::default();
        command_exec
            .expect_run()
            .withf(move |cmd: &str, args: &[&str], _, _| {
                cmd == "infverif"
                    && args.len() >= 2
                    && args[0] == "/v"
                    && args[1] == expected_mode_flag
            })
            .once()
            .returning(|_, _, _, _| {
                Ok(Output {
                    status: ExitStatus::default(),
                    stdout: vec![],
                    stderr: vec![],
                })
            });

        let task = PackageTask::new(params, &wdk_build, &command_exec, &fs);
        assert!(task.run_infverif().is_ok());
    }

    #[test]
    fn run_infverif_defaults_to_universal_for_all_driver_models() {
        assert_infverif_mode_flag(
            DriverConfig::Kmdf(KmdfConfig::default()),
            TargetPlatform::Universal,
            "/u",
        );
        assert_infverif_mode_flag(DriverConfig::Wdm, TargetPlatform::Universal, "/u");
        assert_infverif_mode_flag(
            DriverConfig::Umdf(UmdfConfig::default()),
            TargetPlatform::Universal,
            "/u",
        );
    }

    #[test]
    fn run_infverif_uses_target_platform_mode_flag() {
        assert_infverif_mode_flag(
            DriverConfig::Kmdf(KmdfConfig::default()),
            TargetPlatform::Universal,
            "/u",
        );
        assert_infverif_mode_flag(
            DriverConfig::Kmdf(KmdfConfig::default()),
            TargetPlatform::Desktop,
            "/h",
        );
        assert_infverif_mode_flag(
            DriverConfig::Kmdf(KmdfConfig::default()),
            TargetPlatform::Windows,
            "/w",
        );
    }

    mod named_mutex {
        use std::{
            ffi::CString,
            sync::{
                Barrier,
                atomic::{AtomicUsize, Ordering},
            },
            thread,
            time::Duration,
        };

        use super::super::NamedMutex;

        /// Tests that two threads successfully acquire `NamedMutex`
        /// and it prevents them from running concurrently.
        #[test]
        fn acquire_works_correctly() {
            // The way this test work is:
            // 1. We create two threads that start at the same time thanks
            // to a barrier
            // 2. Both increment a counter `active` while they run holding
            // the mutex
            // 3. Both also increment another counter `completed` when they
            //    finish
            // 4. We verify that `active` never exceeds 1 i.e. there's no
            //    concurrent
            // execution and `completed` is 2 at the end i.e. both threads run
            // to completion

            let barrier = Barrier::new(2);
            let active = AtomicUsize::new(0);
            let completed = AtomicUsize::new(0);

            thread::scope(|s| {
                for _ in 0..2 {
                    s.spawn(|| {
                        let name =
                            CString::new("happy_path_d44f8b8a817").expect("it is a valid C string");

                        barrier.wait();
                        let guard = NamedMutex::acquire(name.as_c_str())
                            .expect("thread should acquire mutex");

                        let active_prev = active.fetch_add(1, Ordering::SeqCst);
                        assert_eq!(active_prev, 0, "named mutex allowed concurrent access");

                        thread::sleep(Duration::from_millis(100));

                        let active_prev = active.fetch_sub(1, Ordering::SeqCst);
                        assert_eq!(active_prev, 1, "active counter should drop back to zero");

                        drop(guard);

                        completed.fetch_add(1, Ordering::SeqCst);
                    });
                }
            });

            assert_eq!(completed.load(Ordering::SeqCst), 2);
            assert_eq!(active.load(Ordering::SeqCst), 0);
        }

        /// Tests that `NamedMutex` can be acquired even after the previous
        /// owner abandoned it (e.g. crashed) without releasing
        ///
        /// What we are really testing here is `WaitForSingleObject`
        /// inside `NamedMutex::acquire` returning `WAIT_ABANDONED`
        #[test]
        fn acquire_works_when_abandoned() {
            fn acquire_mutex() -> NamedMutex {
                let name =
                    CString::new("abandoned_owner_d44f8b8a817").expect("it is a valid C string");
                NamedMutex::acquire(name.as_c_str()).expect("thread should acquire mutex")
            }

            // Acquire the mutex on a thread and abandon it
            thread::scope(|s| {
                s.spawn(|| {
                    let guard = acquire_mutex();
                    // Simulate an abnormal exit while still holding the mutex
                    // to trigger the WAIT_ABANDONED path
                    // for the next owner.
                    std::mem::forget(guard);
                });
            });

            // Try to acquire the same mutex from the main thread
            // which should succeed despite the abandonment above
            let guard = acquire_mutex();
            drop(guard);
        }
    }
}
