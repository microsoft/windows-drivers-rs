use std::{ops::RangeFrom, path::PathBuf, result::Result};

use log::{debug, info};
use wdk_build::DriverConfig;
use crate::providers::exec::RunCommand;

use super::{error::PackageDriverError, FSProvider, TargetTriplet, WdkBuildProvider};

/// The first WDK version with the new `InfVerif` behavior.
const MINIMUM_SAMPLES_FLAG_WDK_VERSION: u32 = 25798;
// This range is inclusive of 25798. FIXME: update with range end after /sample
// flag is added to InfVerif CLI
const MISSING_SAMPLE_FLAG_WDK_BUILD_NUMBER_RANGE: RangeFrom<u32> = 25798..;
const WDR_TEST_CERT_STORE: &str = "WDRTestCertStore";
const WDR_LOCAL_TEST_CERT: &str = "WDRLocalTestCert";

pub struct PackageDriver<'a> {
    package_name: String,
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
    wdk_build_provider: &'a dyn WdkBuildProvider,
    command_exec: &'a dyn RunCommand,
    fs_provider: &'a dyn FSProvider,
}

impl<'a> PackageDriver<'a> {
    pub fn new(
        package_name: &'a str,
        working_dir: &'a PathBuf,
        target_dir: &'a PathBuf,
        target_triplet: &TargetTriplet,
        sample_class: bool,
        driver_model: DriverConfig,
        wdk_build_provider: &'a dyn WdkBuildProvider,
        command_exec: &'a dyn RunCommand,
        fs_provider: &'a dyn FSProvider,
    ) -> Result<Self, PackageDriverError> {
        let package_name = package_name.replace("-", "_");
        // src paths
        let src_driver_binary_extension = "dll";
        let src_inx_file_path = working_dir.join(format!("{}.inx", package_name));

        // all paths inside target directory
        let src_driver_binary_file_path = target_dir.join(format!("{}.{}", package_name, src_driver_binary_extension));
        let src_pdb_file_path = target_dir.join(format!("{}.pdb", package_name));
        let src_map_file_path = target_dir.join("deps").join(format!("{}.map", package_name));
        let src_cert_file_path = target_dir.join(format!("{}.cer", WDR_LOCAL_TEST_CERT));


        // destination paths
        let dest_driver_binary_extension = if matches!(driver_model, DriverConfig::Kmdf(_) | DriverConfig::Wdm) {
            "sys"
        } else {
            "dll"
        };
        let src_renamed_driver_binary_file_path = target_dir.join(format!("{}.{}", package_name, dest_driver_binary_extension));
        let dest_root_package_folder: PathBuf = target_dir.join(format!("{}_package", package_name));
        let dest_inf_file_path = dest_root_package_folder.join(format!("{}.inf", package_name));
        let dest_driver_binary_path = dest_root_package_folder.join(format!("{}.{}", package_name, dest_driver_binary_extension));
        let dest_pdb_file_path = dest_root_package_folder.join(format!("{}.pdb", package_name));
        let dest_map_file_path = dest_root_package_folder.join(format!("{}.map", package_name));
        let dest_cert_file_path = dest_root_package_folder.join(format!("{}.cer", WDR_LOCAL_TEST_CERT));
        let dest_cat_file_path = dest_root_package_folder.join(format!("{}.cat", package_name));

        if !fs_provider.exists(&dest_root_package_folder) {
            fs_provider.create_dir(&dest_root_package_folder)?;
        }

        let arch = match target_triplet.to_string().as_str() {
            "x86_64-pc-windows-msvc" => "amd64",
            "aarch64-pc-windows-msvc" => "arm64",
            _ => "UNKNOWN",
        };

        let os_mapping = match target_triplet.to_string().as_str() {
            "x86_64-pc-windows-msvc" => "10_x64",
            "aarch64-pc-windows-msvc" => "Server10_arm64",
            _ => "UNKNOWN",
        };

        Ok(Self {
            package_name,
            sample_class,
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
            driver_model,
            wdk_build_provider,
            command_exec,
            fs_provider,
        })
    }

    fn check_inx_exists(&self) -> Result<(), PackageDriverError> {
        debug!("Checking for .inx file, path: {}", self.src_inx_file_path.to_string_lossy());
        if !self.fs_provider.exists(&self.src_inx_file_path) {
            return Err(PackageDriverError::MissingInxSrcFileError(self.src_inx_file_path.clone()))
        }
        Ok(())
    }

    fn rename_driver_binary_extension(&self) -> Result<(), PackageDriverError> {
        debug!("Renaming driver binary extension from .dll to .sys");
        if let Err(e) = self.fs_provider.rename(&self.src_driver_binary_file_path, &self.src_renamed_driver_binary_file_path) {
            return Err(PackageDriverError::CopyFileError {
                src: self.src_driver_binary_file_path.clone(),
                dest: self.src_renamed_driver_binary_file_path.clone(),
                error: e,
            })
        }
        Ok(())
    }

    fn copy(&self, src_file_path: &'a PathBuf, dest_file_path: &'a PathBuf) -> Result<(), PackageDriverError> {
        debug!("Copying src file {} to dest folder {}", src_file_path.to_string_lossy(), dest_file_path.to_string_lossy());
        if let Err(e) = self.fs_provider.copy(src_file_path, dest_file_path) {
            return Err(PackageDriverError::CopyFileError {
                src: src_file_path.clone(),
                dest: dest_file_path.clone(),
                error: e,
            })
        }
        Ok(())
    }

    fn run_stampinf(&self) -> Result<(), PackageDriverError> {
        info!("Running stampinf command");
        let wdf_flags = match self.driver_model {
            DriverConfig::Kmdf(kmdf_config) => format!(
                "-k {}.{}",
                kmdf_config.kmdf_version_major, kmdf_config.target_kmdf_version_minor
            ),
            DriverConfig::Umdf(umdf_config) => format!(
                "-u {}.{}",
                umdf_config.umdf_version_major, umdf_config.target_umdf_version_minor
            ),
            DriverConfig::Wdm => "".to_string(),
        };

        //TODO: Does it generate cat file relative to inf file path or we need to provide the absolute path?
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

        if !wdf_flags.is_empty() {
            args.push(&wdf_flags);
        }

        if let Err(e) = self.command_exec.run("stampinf", &args, None) {
            return Err(PackageDriverError::StampinfError(e));
        }

        Ok(())
    }

    
    fn run_inf2cat(&self) -> Result<(), PackageDriverError> {
        info!("Running inf2cat command");
        let args = [
            &format!("/driver:{}", self.dest_root_package_folder.to_string_lossy().trim_start_matches("\\\\?\\")),
            &format!("/os:{}", self.os_mapping),
            "/uselocaltime",
        ];

        if let Err(e) = self.command_exec.run("inf2cat", &args, None) {
            return Err(PackageDriverError::Inf2CatError(e));
        }

        Ok(())
    }

    fn generate_certificate(&self) -> Result<(), PackageDriverError> {
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

    fn is_self_signed_certificate_in_store(&self) -> Result<bool, PackageDriverError> {
        let args = [
            "-s",
            WDR_TEST_CERT_STORE,
        ];

        match self.command_exec.run("certmgr.exe", &args, None) {
            Ok(output) => {
                if output.status.success() {
                    match String::from_utf8(output.stdout) {
                        Ok(stdout) => {
                            if stdout.contains(WDR_LOCAL_TEST_CERT) {
                                return Ok(true);
                            }
                        },
                        Err(e) => {
                            return Err(PackageDriverError::VerifyCertExistsInStoreInvalidCommandOutputError(e));
                        },
                    }
                }
                return Ok(false);
            },
            Err(e) => {
                Err(PackageDriverError::VerifyCertExistsInStoreError(e))
            }
        }
    }

    fn create_self_signed_cert_in_store(&self) -> Result<(), PackageDriverError> {
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
            &format!("CN={}", WDR_LOCAL_TEST_CERT), // FIXME: this should be a parameter
            &cert_path,
        ];
    
    
        if let Err(e) = self.command_exec.run("makecert", &args, None) {
            return Err(PackageDriverError::CertGenerationInStoreError(e));
        }

        Ok(())
    }
    
    fn create_cert_file_from_store(&self) -> Result<(), PackageDriverError> {
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
            return Err(PackageDriverError::CreateCertFileFromStoreError(e));
        }

        Ok(())
    }

    /// Runs the signtool sign command with the specified file path, certificate store, and certificate name.
    ///
    /// # Arguments
    ///
    /// * `file_path` - The path to the file to be signed.
    /// * `cert_store` - The certificate store to use for signing.
    /// * `cert_name` - The name of the certificate to use for signing.
    /// TODO: Add parameters for certificate store and name
    fn run_signtool_sign(&self, file_path: &PathBuf, cert_store: &str, cert_name: &str) -> Result<(), PackageDriverError> {
        info!("Signing {} using signtool", file_path.file_name().expect("Unable to read file name from the path").to_string_lossy());
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
            return Err(PackageDriverError::DriverBinarySignError(e));
        }

        std::result::Result::Ok(())
    }

    fn run_signtool_verify(&self, file_path: &PathBuf) -> std::result::Result<(), PackageDriverError> {
        info!("Verifying {} using signtool", file_path.file_name().expect("Unable to read file name from the path").to_string_lossy());
        let driver_binary_file_path = file_path.to_string_lossy();
        let args = ["verify", "/v", "/pa", &driver_binary_file_path];

        // TODO: Differentiate between command exec failure and signature verification failure
        if let Err(e) = self.command_exec.run("signtool", &args, None) {
            return Err(PackageDriverError::DriverBinarySignVerificationError(e));
        }

        std::result::Result::Ok(())
    }

    fn run_infverif(&self) -> Result<(), PackageDriverError> {
        info!("Running InfVerif command");
        let mut additional_args = "";
        if self.sample_class {
            let wdk_build_number = self.wdk_build_provider.detect_wdk_build_number()?;
            if MISSING_SAMPLE_FLAG_WDK_BUILD_NUMBER_RANGE.contains(&wdk_build_number) {
                debug!("Skipping InfVerif. InfVerif in WDK Build {} is bugged and does not contain the /samples flag.", wdk_build_number);
                return Ok(());
            }
            let sample_flag = if wdk_build_number > MINIMUM_SAMPLES_FLAG_WDK_VERSION {
                "/samples"
            } else {
                "/msft"
            };
            additional_args = sample_flag;
        }
        let mut args = vec![
            "/v",
            match self.driver_model {
                DriverConfig::Kmdf(_) | DriverConfig::Wdm => "/w",
                DriverConfig::Umdf(_) => "/u",
            },
        ];

        let inf_path = self.dest_inf_file_path.to_string_lossy();

        args.push(additional_args);
        args.push(&inf_path);

        if let Err(e) = self.command_exec.run("infverif", &args, None) {
            return Err(PackageDriverError::InfVerificationError(e));
        }

        Ok(())
    }

    pub fn run(&self) -> Result<(), PackageDriverError> {
        self.check_inx_exists()?;
        //TODO: rename is not necessary, but should confirm
        info!("Copying files to target package folder: {}", self.dest_root_package_folder.to_string_lossy());
        self.rename_driver_binary_extension()?;
        self.copy(&self.src_renamed_driver_binary_file_path, &self.dest_driver_binary_path)?;
        self.copy(&self.src_pdb_file_path, &self.dest_pdb_file_path)?;
        self.copy(&self.src_inx_file_path, &self.dest_inf_file_path)?;
        self.copy(&self.src_map_file_path, &self.dest_map_file_path)?;
        self.run_stampinf()?;
        self.run_inf2cat()?;
        self.generate_certificate()?;
        self.copy(&self.src_cert_file_path, &self.dest_cert_file_path)?;
        self.run_signtool_sign(&self.dest_driver_binary_path, WDR_TEST_CERT_STORE, WDR_LOCAL_TEST_CERT)?;
        self.run_signtool_verify(&self.dest_driver_binary_path)?;
        self.run_signtool_sign(&self.dest_cat_file_path, WDR_TEST_CERT_STORE, WDR_LOCAL_TEST_CERT)?;
        self.run_signtool_verify(&self.dest_cat_file_path)?;
        self.run_infverif()?;
        Ok(())
    }
}

