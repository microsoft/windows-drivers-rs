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
    ops::RangeFrom,
    path::{Path, PathBuf},
    result::Result,
};

use mockall_double::double;
use tracing::{debug, info, warn};
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
use crate::{
    actions::build::error::PackageTaskError,
    providers::{error::FileError, exec::CaptureStream},
};

// FIXME: This range is inclusive of 25798. Update with range end after /sample
// flag is added to InfVerif CLI
const MISSING_SAMPLE_FLAG_WDK_BUILD_NUMBER_RANGE: RangeFrom<u32> = 25798..;
const WDR_TEST_CERT_STORE: &str = "WDRTestCertStore";
const WDR_LOCAL_TEST_CERT: &str = "WDRLocalTestCert";
const STAMPINF_VERSION_ENV_VAR: &str = "STAMPINF_VERSION";

#[derive(Debug)]
pub struct PackageTaskParams<'a> {
    pub package_name: &'a str,
    pub working_dir: &'a Path,
    pub target_dir: &'a Path,
    pub target_arch: &'a CpuArchitecture,
    pub verify_signature: bool,
    pub sample_class: bool,
    pub driver_model: DriverConfig,
}

/// Supports low level driver packaging operations
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

    arch: &'a CpuArchitecture,
    os_mapping: &'a str,
    driver_model: DriverConfig,

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
            arch: params.target_arch,
            os_mapping,
            driver_model: params.driver_model,
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
    /// * `PackageTaskError::DriverBinarySignCommand` - If there is an error
    ///   signing the driver binary.
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
        debug!("Creating final package directory if it doesn't exist");
        if !self.fs.exists(&self.dest_root_package_folder) {
            self.fs.create_dir(&self.dest_root_package_folder)?;
        }
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
        // TODO: Does it generate cat file relative to inf file path or we need to
        // provide the absolute path?
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
                // When STAMPINF_VERSION is set to a non-empty, non-whitespace value, we
                // intentionally omit -v so stampinf reads it and populates
                // DriverVer. (Whitespace-only values are ignored.)
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
        if let Err(e) = self
            .command_exec
            .run("stampinf", &args, None, None, CaptureStream::StdOut)
        {
            return Err(PackageTaskError::StampinfCommand(e));
        }
        Ok(())
    }

    fn run_inf2cat(&self) -> Result<(), PackageTaskError> {
        info!("Running inf2cat");
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

        if let Err(e) = self
            .command_exec
            .run("inf2cat", &args, None, None, CaptureStream::StdOut)
        {
            return Err(PackageTaskError::Inf2CatCommand(e));
        }

        Ok(())
    }

    fn generate_certificate(&self) -> Result<(), PackageTaskError> {
        debug!("Generating certificate");
        if self.fs.exists(&self.src_cert_file_path) {
            return Ok(());
        }
        if self.is_self_signed_certificate_in_store()? {
            self.create_cert_file_from_store()?;
        } else {
            // This mutex prevents multiple instances of this app from racing to
            // create a cert in the store. It is not a correctness problem. We
            // just don't want to litter the store with certs especially during
            // tests when there are lots of parallel runs
            let mutex_name = CString::new("WDRCertStoreMutex_bd345cf9330") // Unique enough
                .expect("string is a valid C string");
            let mutex = NamedMutex::acquire(&mutex_name)
                .map_err(|e| PackageTaskError::CertMutexError(e.code().0))?;
            debug!("Acquired cert store mutex");

            // Check again for an existing cert. Another instance might have
            // created it while we waited for the mutex
            if self.is_self_signed_certificate_in_store()? {
                drop(mutex);
                self.create_cert_file_from_store()?;
            } else {
                self.create_self_signed_cert_in_store()?;
            }
        }

        Ok(())
    }

    fn is_self_signed_certificate_in_store(&self) -> Result<bool, PackageTaskError> {
        debug!("Checking if self signed certificate exists in WDRTestCertStore store");
        let args = ["-s", WDR_TEST_CERT_STORE];

        match self
            .command_exec
            .run("certmgr.exe", &args, None, None, CaptureStream::StdOut)
        {
            Ok(output) if output.status.success() => String::from_utf8(output.stdout).map_or_else(
                |e| Err(PackageTaskError::VerifyCertExistsInStoreInvalidCommandOutput(e)),
                |stdout| Ok(stdout.contains(WDR_LOCAL_TEST_CERT)),
            ),
            Ok(_) => Ok(false),
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
        if let Err(e) = self
            .command_exec
            .run("makecert", &args, None, None, CaptureStream::StdOut)
        {
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
        if let Err(e) =
            self.command_exec
                .run("certmgr.exe", &args, None, None, CaptureStream::StdOut)
        {
            return Err(PackageTaskError::CreateCertFileFromStoreCommand(e));
        }
        Ok(())
    }

    /// Signs the specified file using signtool command using certificate from
    /// certificate store.
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
        if let Err(e) = self
            .command_exec
            .run("signtool", &args, None, None, CaptureStream::StdOut)
        {
            return Err(PackageTaskError::DriverBinarySignCommand(e));
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
        // TODO: Differentiate between command exec failure and signature verification
        // failure
        if let Err(e) = self
            .command_exec
            .run("signtool", &args, None, None, CaptureStream::StdOut)
        {
            return Err(PackageTaskError::DriverBinarySignVerificationCommand(e));
        }
        Ok(())
    }

    fn run_infverif(&self) -> Result<(), PackageTaskError> {
        let additional_args = if self.sample_class {
            let wdk_build_number = self.wdk_build.detect_wdk_build_number()?;
            if MISSING_SAMPLE_FLAG_WDK_BUILD_NUMBER_RANGE.contains(&wdk_build_number) {
                debug!(
                    "InfVerif in WDK Build {wdk_build_number} is bugged and does not contain the \
                     /samples flag."
                );
                warn!("InfVerif skipped for samples class. WDK Build: {wdk_build_number}");
                return Ok(());
            }
            "/msft"
        } else {
            ""
        };

        info!("Running infverif");
        let mut args = vec![
            "/v",
            match self.driver_model {
                DriverConfig::Kmdf(_) | DriverConfig::Wdm => "/w",
                // TODO: This should be /u if WDK <= GE && DRIVER_MODEL == UMDF, otherwise it should
                // be /w
                DriverConfig::Umdf(_) => "/u",
            },
        ];
        let inf_path = self.dest_inf_file_path.to_string_lossy();

        if self.sample_class {
            args.push(additional_args);
        }
        args.push(&inf_path);

        if let Err(e) = self
            .command_exec
            .run("infverif", &args, None, None, CaptureStream::StdOut)
        {
            return Err(PackageTaskError::InfVerificationCommand(e));
        }

        Ok(())
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

    use wdk_build::{CpuArchitecture, KmdfConfig};

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
            verify_signature: false,
        };
        let dest_root = target_dir.join(format!("{package_name}_package"));

        let command_exec = CommandExec::default();
        let wdk_build = WdkBuild::default();
        let fs = Fs::default();
        let task = PackageTask::new(package_task_params, &wdk_build, &command_exec, &fs);
        assert_eq!(task.package_name, package_name.replace('-', "_"));
        assert!(!task.verify_signature);
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
            verify_signature: false,
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
            verify_signature: false,
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
                        verify_signature: false,
                    };

                    let wdk_build = WdkBuild::default();
                    let fs = Fs::default();
                    let mut command_exec = CommandExec::default();

                    command_exec
                        .expect_run()
                        .withf(move |cmd: &str, args: &[&str], _, _, _| {
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
                        .return_once(|_, _, _, _, _| {
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
            // 3. Both also increment another counter `completed` when they finish
            // 4. We verify that `active` never exceeds 1 i.e. there's no concurrent
            // execution and `completed` is 2 at the end i.e. both threads run to completion

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
                    // Simulate an abnormal exit while still holding the mutex to trigger the
                    // WAIT_ABANDONED path for the next owner.
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
