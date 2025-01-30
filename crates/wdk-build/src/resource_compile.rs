use std::{
    env,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use cargo_metadata::MetadataCommand;

// Function to generate and compile RC file
pub fn generate_and_compile_rc_file(include_paths: Vec<PathBuf>, rc_exe_root_path: String) {
    // Initialize an empty vector to store modified include arguments
    let mut include_args: Vec<String> = Vec::new();

    // Iterate over each include path
    for include_path in include_paths {
        // Convert the include path to a string
        if let Some(include_str) = include_path.to_str() {
            // Append "/I" and the include path to the modified vector
            include_args.push("/I".to_string());
            include_args.push(include_str.to_string());
        } else {
            println!("Non-Unicode path is not supported: {:?}", include_path);
        }
    }

    let (company_name, copyright, product_name) = get_package_metadata_details();
    let (product_version, description, file_version, name) = get_package_details();

    get_and_set_rc_file(
        company_name, 
        copyright, 
        product_name, 
        product_version,
        description, 
        file_version, 
        name, 
        &include_args,
        rc_exe_root_path,
    );
}

// Function to get and set RC File with package metadata
fn get_and_set_rc_file(
    company_name: String, 
    copyright: String, 
    product_name: String, 
    product_version: String, 
    description: String, 
    file_version: String, 
    name: String, 
    include_args: &Vec<String>, 
    rc_exe_root_path: String,
) {
    println!("Set and create rc file... ");
    let rc_file_path = "resources.rc";
    if fs::metadata(&rc_file_path).is_ok() {
        // File exists, so let's remove it
        if let Err(err) = fs::remove_file(&rc_file_path) {
            eprintln!("Error deleting file: {}", err);
        } else {
            println!("File deleted successfully!");
        }
    } else {
        println!("File does not exist.");
    }

    let ver_file_type = "VFT_DRV";
    let ver_file_subtype = "VFT2_DRV_SYSTEM";
    let ver_original_filename = "VER_INTERNALNAME_STR";

    // Create the RC file content
    let rc_content = format!(
        r#"#include <windows.h>
#include <ntverp.h>
#define VER_FILETYPE                {file_type}
#define VER_FILESUBTYPE             {file_subtype}
#define VER_INTERNALNAME_STR        "{name}"
#define VER_ORIGINALFILENAME_STR    {original_filename}

#undef VER_FILEDESCRIPTION_STR     
#define VER_FILEDESCRIPTION_STR "{description}"

#undef  VER_PRODUCTNAME_STR
#define VER_PRODUCTNAME_STR    VER_FILEDESCRIPTION_STR

#define VER_FILEVERSION        {file_version},0
#define VER_FILEVERSION_STR    "{product_version}.0"

#undef  VER_PRODUCTVERSION
#define VER_PRODUCTVERSION          VER_FILEVERSION

#undef  VER_PRODUCTVERSION_STR
#define VER_PRODUCTVERSION_STR      VER_FILEVERSION_STR

#define VER_LEGALCOPYRIGHT_STR      {copyright}
#ifdef  VER_COMPANYNAME_STR

#undef  VER_COMPANYNAME_STR
#define VER_COMPANYNAME_STR         {company_name}
#endif

#undef  VER_PRODUCTNAME_STR
#define VER_PRODUCTNAME_STR    {product_name}

#include "common.ver""#,
        file_type = ver_file_type,
        file_subtype = ver_file_subtype,
        original_filename = ver_original_filename
    );
   
    std::fs::write("resources.rc", rc_content).expect("Unable to write RC file");
    invoke_rc(&include_args, rc_exe_root_path);
}

// Function to invoke RC.exe
fn invoke_rc(include_args: &Vec<String>, rc_exe_root_path: String) {
    let resource_script = "resources.rc";
    let rc_exe_path = format!("{}\\rc.exe", rc_exe_root_path);
    let rc_exe_path = Path::new(&rc_exe_path);
    if !rc_exe_path.exists() {
        eprintln!(
            "Error: rc.exe path does not exist : {}", 
            rc_exe_path.display()
        );
        std::process::exit(1); // Exit with a non-zero status code
    }

    let mut command = Command::new(rc_exe_path);
    command.args(include_args).arg(resource_script);
    println!("Command executed: {:?}", command);
    
    let status = command.status();

    match status {
        Ok(exit_status) => {
            if exit_status.success() {
                println!("Resource compilation successful!");
                println!("cargo:rustc-link-arg=resources.res");
            } else {
                println!("Resource compilation failed.");
                std::process::exit(1); // Exit with a non-zero status code
            }
        }
        Err(err) => {
            eprintln!("Error running rc.exe: {}", err);
            std::process::exit(1); // Exit with a non-zero status code
        }
    }
}

// Function to get package metadata details
fn get_package_metadata_details() -> (String, String, String) {
    // Run the 'cargo metadata' command and capture its output
    let path = env::var("CARGO_MANIFEST_DIR").unwrap();
    let meta = MetadataCommand::new()
        .manifest_path("./Cargo.toml")
        .current_dir(&path)
        .exec()
        .unwrap();
    let root = meta.root_package().unwrap();
    let metadata = &root.metadata;

    // Extract metadata values with default fallbacks
    let company_name = metadata
        .get("wdk")
        .and_then(|wdk| wdk.get("driver-model"))
        .and_then(|driver_model| driver_model.get("companyname"))
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Company name not found in metadata".to_string());

    let copyright_name = metadata
        .get("wdk")
        .and_then(|wdk| wdk.get("driver-model"))
        .and_then(|driver_model| driver_model.get("copyright"))
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Copyright name not found in metadata".to_string());

    let product_name = metadata
        .get("wdk")
        .and_then(|wdk| wdk.get("driver-model"))
        .and_then(|driver_model| driver_model.get("productname"))
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Product name not found in metadata".to_string());

    (company_name, copyright_name, product_name)
}

// Function to get package details
fn get_package_details() -> (String, String, String, String) {
    let mut file_version = String::new();
    let mut description = String::new();
    let mut product_version = String::new();
    let mut name = String::new();

    match fs::read_to_string("Cargo.toml") {
        Ok(text) => {
            for line in text.lines() {
                if line.starts_with("version") {
                    let start = line.find('"').unwrap_or(0) + 1;
                    let end = line.rfind('"').unwrap_or(0);
                    product_version = line[start..end].to_string();
                    let version_parts: Vec<&str> = product_version.split('.').collect();
                    file_version = version_parts.join(",");
                }
                if line.starts_with("description") {
                    let start = line.find('"').unwrap_or(0) + 1;
                    let end = line.rfind('"').unwrap_or(0);
                    description = line[start..end].to_string();
                }
                if line.starts_with("name") {
                    let start = line.find('"').unwrap_or(0) + 1;
                    let end = line.rfind('"').unwrap_or(0);
                    name = line[start..end].to_string();
                }
            }
        }
        Err(_) => {
            eprintln!("Error reading Cargo.toml");
        }
    }

    (product_version, description, file_version, name)
}