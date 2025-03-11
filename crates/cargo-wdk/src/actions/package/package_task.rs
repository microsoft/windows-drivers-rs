//! Module for handling low-level driver packaging tasks.
//!
//! This module defines the `PackageTask` struct and its associated methods
//! for packaging driver projects.  It handles file system
//! operations and interacting with WDK tools to generate the driver package. It
//! includes functions that invoke various WDK Tools involved in signing,
//! validating, verifying and generating artefacts for the driver package.

use std::{
    ops::RangeFrom,
    path::{Path, PathBuf},
    result::Result,
};

use mockall_double::double;
use tracing::{debug, info};
use wdk_build::DriverConfig;

use super::error::PackageTaskError;
use crate::actions::TargetArch;
#[double]
use crate::providers::{exec::CommandExec, fs::Fs, wdk_build::WdkBuild};

// FIXME: This range is inclusive of 25798. Update with range end after /sample
// flag is added to InfVerif CLI
const MISSING_SAMPLE_FLAG_WDK_BUILD_NUMBER_RANGE: RangeFrom<u32> = 25798..;
const WDR_TEST_CERT_STORE: &str = "WDRTestCertStore";
const WDR_LOCAL_TEST_CERT: &str = "WDRLocalTestCert";

pub struct PackageTaskParams<'a> {
    pub package_name: &'a str,
    pub working_dir: &'a Path,
    pub target_dir: &'a Path,
    pub target_arch: TargetArch,
    pub verify_signature: bool,
    pub sample_class: bool,
    pub driver_model: DriverConfig,
}

/// Suports low level driver packaging operations
pub struct PackageTask<'a> {
    package_name: String,
    verify_signature: bool,
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

    arch: &'a str,
    os_mapping: &'a str,
    driver_model: DriverConfig,

    // Injected deps
    wdk_build_provider: &'a WdkBuild,
    command_exec: &'a CommandExec,
    fs_provider: &'a Fs,
}

impl<'a> PackageTask<'a> {
    /// Creates a new instance of `PackageTask`.
    /// # Arguments
    /// * `package_name` - The name of the driver package.
    /// * `working_dir` - The working directory of the driver project.
    /// * `target_dir` - The target directory for the artifacts.
    /// * `target_arch` - The target architecture.
    /// * `sample_class` - Whether the driver class is a sample class.
    /// * `driver_model` - The driver model configuration.
    /// * `wdk_build_provider` - The provider for WDK build related methods.
    /// * `command_exec` - The provider for command execution.
    /// * `fs_provider` - The provider for file system operations.
    /// # Returns
    /// * `Result<Self, PackageTaskError>` - A result containing the new
    ///   instance or an error.
    /// # Errors
    /// * `PackageTaskError::IoError` - If there is an IO error while creating
    ///   the final package directory.
    pub fn new(
        params: PackageTaskParams<'a>,
        wdk_build_provider: &'a WdkBuild,
        command_exec: &'a CommandExec,
        fs_provider: &'a Fs,
    ) -> Result<Self, PackageTaskError> {
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
        let dest_driver_binary_extension = if matches!(
            params.driver_model,
            DriverConfig::Kmdf(_) | DriverConfig::Wdm
        ) {
            "sys"
        } else {
            "dll"
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

        if !fs_provider.exists(&dest_root_package_folder) {
            fs_provider.create_dir(&dest_root_package_folder)?;
        }

        let arch = match params.target_arch {
            TargetArch::X64 => "amd64",
            TargetArch::Arm64 => "arm64",
        };

        let os_mapping = match params.target_arch {
            TargetArch::X64 => "10_x64",
            TargetArch::Arm64 => "Server10_arm64",
        };

        Ok(Self {
            package_name,
            verify_signature: params.verify_signature,
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
            arch,
            os_mapping,
            driver_model: params.driver_model,
            wdk_build_provider,
            command_exec,
            fs_provider,
        })
    }

    fn check_inx_exists(&self) -> Result<(), PackageTaskError> {
        debug!(
            "Checking for .inx file, path: {}",
            self.src_inx_file_path.to_string_lossy()
        );
        if !self.fs_provider.exists(&self.src_inx_file_path) {
            return Err(PackageTaskError::MissingInxSrcFile(
                self.src_inx_file_path.clone(),
            ));
        }
        Ok(())
    }

    fn rename_driver_binary_extension(&self) -> Result<(), PackageTaskError> {
        debug!("Renaming driver binary extension from .dll to .sys");
        if let Err(e) = self.fs_provider.rename(
            &self.src_driver_binary_file_path,
            &self.src_renamed_driver_binary_file_path,
        ) {
            return Err(PackageTaskError::CopyFile(
                self.src_driver_binary_file_path.clone(),
                self.src_renamed_driver_binary_file_path.clone(),
                e,
            ));
        }
        Ok(())
    }

    fn copy(
        &self,
        src_file_path: &'a Path,
        dest_file_path: &'a Path,
    ) -> Result<(), PackageTaskError> {
        debug!(
            "Copying src file {} to dest folder {}",
            src_file_path.to_string_lossy(),
            dest_file_path.to_string_lossy()
        );
        if let Err(e) = self.fs_provider.copy(src_file_path, dest_file_path) {
            return Err(PackageTaskError::CopyFile(
                src_file_path.to_path_buf(),
                dest_file_path.to_path_buf(),
                e,
            ));
        }
        Ok(())
    }

    fn run_stampinf(&self) -> Result<(), PackageTaskError> {
        info!("Running stampinf command");
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

        // TODO: Does it generate cat file relative to inf file path or we need to
        // provide the absolute path?
        let cat_file_path = format!("{}.cat", self.package_name);
        let dest_inf_file_path = self.dest_inf_file_path.to_string_lossy();

        let mut args: Vec<&str> = vec![
            "-f",
            &dest_inf_file_path,
            "-d",
            "*",
            "-a",
            &self.arch,
            "-c",
            &cat_file_path,
            "-v",
            "*",
        ];

        if !wdf_version_flags.is_empty() {
            args.append(&mut wdf_version_flags.iter().map(String::as_str).collect());
        }

        if let Err(e) = self.command_exec.run("stampinf", &args, None) {
            return Err(PackageTaskError::StampinfCommand(e));
        }

        Ok(())
    }

    fn run_inf2cat(&self) -> Result<(), PackageTaskError> {
        info!("Running inf2cat command");
        let args = [
            &format!(
                "/driver:{}",
                self.dest_root_package_folder
                    .to_string_lossy()
                    .trim_start_matches("\\\\?\\")
            ),
            &format!("/os:{}", self.os_mapping),
            "/uselocaltime",
        ];

        if let Err(e) = self.command_exec.run("inf2cat", &args, None) {
            return Err(PackageTaskError::Inf2CatCommand(e));
        }

        Ok(())
    }

    fn generate_certificate(&self) -> Result<(), PackageTaskError> {
        if self.fs_provider.exists(&self.src_cert_file_path) {
            return Ok(());
        }

        if self.is_self_signed_certificate_in_store()? {
            self.create_cert_file_from_store()?;
        } else {
            self.create_self_signed_cert_in_store()?;
        }

        Ok(())
    }

    fn is_self_signed_certificate_in_store(&self) -> Result<bool, PackageTaskError> {
        let args = ["-s", WDR_TEST_CERT_STORE];

        match self.command_exec.run("certmgr.exe", &args, None) {
            Ok(output) => {
                if output.status.success() {
                    match String::from_utf8(output.stdout) {
                        Ok(stdout) => {
                            if stdout.contains(WDR_LOCAL_TEST_CERT) {
                                return Ok(true);
                            }
                        }
                        Err(e) => {
                            return Err(
                                PackageTaskError::VerifyCertExistsInStoreInvalidCommandOutput(e),
                            );
                        }
                    }
                }
                Ok(false)
            }
            Err(e) => Err(PackageTaskError::VerifyCertExistsInStoreCommand(e)),
        }
    }

    fn create_self_signed_cert_in_store(&self) -> Result<(), PackageTaskError> {
        info!("Creating self signed certificate in WDRTestCertStore store using makecert");
        let cert_path = self.src_cert_file_path.to_string_lossy();
        let args = [
            "-r",
            "-pe",
            "-a",
            "SHA256",
            "-eku",
            "1.3.6.1.5.5.7.3.3",
            "-ss",
            WDR_TEST_CERT_STORE, // FIXME: this should be a parameter
            "-n",
            &format!("CN={WDR_LOCAL_TEST_CERT}"), // FIXME: this should be a parameter
            &cert_path,
        ];

        if let Err(e) = self.command_exec.run("makecert", &args, None) {
            return Err(PackageTaskError::CertGenerationInStoreCommand(e));
        }

        Ok(())
    }

    fn create_cert_file_from_store(&self) -> Result<(), PackageTaskError> {
        info!("Creating certificate file from WDRTestCertStore store using certmgr");
        let cert_path = self.src_cert_file_path.to_string_lossy();

        let args = [
            "-put",
            "-s",
            WDR_TEST_CERT_STORE,
            "-c",
            "-n",
            WDR_LOCAL_TEST_CERT,
            &cert_path,
        ];

        if let Err(e) = self.command_exec.run("certmgr.exe", &args, None) {
            return Err(PackageTaskError::CreateCertFileFromStoreCommand(e));
        }

        Ok(())
    }

    /// Runs the signtool sign command with the specified file path, certificate
    /// store, and certificate name.
    ///
    /// # Arguments
    ///
    /// * `file_path` - The path to the file to be signed.
    /// * `cert_store` - The certificate store to use for signing.
    /// * `cert_name` - The name of the certificate to use for signing. TODO:
    ///   Add parameters for certificate store and name
    fn run_signtool_sign(
        &self,
        file_path: &Path,
        cert_store: &str,
        cert_name: &str,
    ) -> Result<(), PackageTaskError> {
        info!(
            "Signing {} using signtool",
            file_path
                .file_name()
                .expect("Unable to read file name from the path")
                .to_string_lossy()
        );
        let driver_binary_file_path = file_path.to_string_lossy();
        let args = [
            "sign",
            "/v",
            "/s",
            cert_store,
            "/n",
            cert_name,
            "/t",
            "http://timestamp.digicert.com",
            "/fd",
            "SHA256",
            &driver_binary_file_path,
        ];

        if let Err(e) = self.command_exec.run("signtool", &args, None) {
            return Err(PackageTaskError::DriverBinarySignCommand(e));
        }

        std::result::Result::Ok(())
    }

    fn run_signtool_verify(&self, file_path: &Path) -> std::result::Result<(), PackageTaskError> {
        info!(
            "Verifying {} using signtool",
            file_path
                .file_name()
                .expect("Unable to read file name from the path")
                .to_string_lossy()
        );
        let driver_binary_file_path = file_path.to_string_lossy();
        let args = ["verify", "/v", "/pa", &driver_binary_file_path];

        // TODO: Differentiate between command exec failure and signature verification
        // failure
        if let Err(e) = self.command_exec.run("signtool", &args, None) {
            return Err(PackageTaskError::DriverBinarySignVerificationCommand(e));
        }

        std::result::Result::Ok(())
    }

    fn run_infverif(&self) -> Result<(), PackageTaskError> {
        info!("Running InfVerif command");
        let additional_args = if self.sample_class {
            let wdk_build_number = self.wdk_build_provider.detect_wdk_build_number()?;
            if MISSING_SAMPLE_FLAG_WDK_BUILD_NUMBER_RANGE.contains(&wdk_build_number) {
                debug!(
                    "Skipping InfVerif. InfVerif in WDK Build {} is bugged and does not contain \
                     the /samples flag.",
                    wdk_build_number
                );
                return Ok(());
            }
            // FIXME: Update the range end and the logic after /sample flag is added to
            // InfVerif CLI
            "/msft"
        } else {
            ""
        };
        let mut args = vec![
            "/v",
            match self.driver_model {
                DriverConfig::Kmdf(_) | DriverConfig::Wdm => "/w",
                DriverConfig::Umdf(_) => "/u",
            },
        ];

        let inf_path = self.dest_inf_file_path.to_string_lossy();

        if self.sample_class {
            args.push(additional_args);
        }
        args.push(&inf_path);

        if let Err(e) = self.command_exec.run("infverif", &args, None) {
            return Err(PackageTaskError::InfVerificationCommand(e));
        }

        Ok(())
    }

    /// Entry point method to run the low level driver packaging operations.
    /// # Returns
    /// * `Result<(), PackageTaskError>` - A result indicating success or
    ///   failure.
    /// # Errors
    /// * `PackageTaskError::CopyFileError` - If there is an error copying a
    ///   file.
    /// * `PackageTaskError::CertGenerationInStoreError` - If there is an error
    ///   generating a certificate in the store.
    /// * `PackageTaskError::CreateCertFileFromStoreError` - If there is an
    ///   error creating a certificate file from the store.
    /// * `PackageTaskError::DriverBinarySignError` - If there is an error
    ///   signing the driver binary.
    /// * `PackageTaskError::DriverBinarySignVerificationError` - If there is an
    ///   error verifying the driver binary signature.
    /// * `PackageTaskError::Inf2CatError` - If there is an error running the
    ///   inf2cat command.
    /// * `PackageTaskError::InfVerificationError` - If there is an error
    ///   verifying the inf file.
    /// * `PackageTaskError::MissingInxSrcFileError` - If the .inx source file
    ///   is missing.
    /// * `PackageTaskError::StampinfError` - If there is an error running the
    ///   stampinf command.
    /// * `PackageTaskError::VerifyCertExistsInStoreError` - If there is an
    ///   error verifying if the certificate exists in the store.
    /// * `PackageTaskError::VerifyCertExistsInStoreInvalidCommandOutputError`
    ///   - If the command output is invalid when verifying if the certificate
    ///     exists in the store.
    /// * `PackageTaskError::WdkBuildConfigError` - If there is an error with
    ///   the WDK build config.
    /// * `PackageTaskError::IoError` - If there is an IO error.
    pub fn run(&self) -> Result<(), PackageTaskError> {
        self.check_inx_exists()?;
        // TODO: rename is not necessary, but should confirm
        info!(
            "Copying files to target package folder: {}",
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
        self.generate_certificate()?;
        self.copy(&self.src_cert_file_path, &self.dest_cert_file_path)?;
        self.run_signtool_sign(
            &self.dest_driver_binary_path,
            WDR_TEST_CERT_STORE,
            WDR_LOCAL_TEST_CERT,
        )?;
        self.run_signtool_sign(
            &self.dest_cat_file_path,
            WDR_TEST_CERT_STORE,
            WDR_LOCAL_TEST_CERT,
        )?;
        self.run_infverif()?;
        // Verify signatures only when --verify-signature flag = true is passed
        if self.verify_signature {
            info!("Verifying signatures for driver binary and cat file using signtool");
            self.run_signtool_verify(&self.dest_driver_binary_path)?;
            self.run_signtool_verify(&self.dest_cat_file_path)?;
        }
        Ok(())
    }
}
