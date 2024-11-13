use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use cargo_metadata::MetadataCommand;

// function to generate and compile RC file
pub fn generate_and_compile_rcfile(include_paths: Vec<PathBuf>, rc_exe_rootpath: String) {
    // Initialize an empty vector to store modified include arguments
    let mut includeargs: Vec<String> = Vec::new();

    // Iterate over each include path
    for include_path in include_paths {
        // Convert the include path to a string
        if let Some(include_str) = include_path.to_str() {
            // Append "/I" and the include path to the modified vector
            includeargs.push("/I".to_string());
            includeargs.push(include_str.to_string());
        } else {
            println!("Non-Unicode path is not supported: {:?}", include_path);
        }
    }

    let (companyname, copyright, productname) = get_packagemetadatadetails();
    let (productversion, description, fileversion, name) = get_packagedetails();
    getandset_rcfile(
        companyname, 
        copyright, 
        productname, 
        productversion,
        description, 
        fileversion, 
        name, 
        &includeargs, 
        rc_exe_rootpath,
    );
}
// function to get and set RC File with package metadata
fn getandset_rcfile(
    companyname: String, 
    copyright: String, 
    productname: String, 
    productversion:String, 
    description:String, 
    fileversion:String, 
    name:String, 
    includeargs: &Vec<String>, 
    rc_exe_rootpath:String,
) {
    println!("Set and create rc file... ");
    let rcfile_path = "resources.rc";
    if fs::metadata(&rcfile_path).is_ok() {
        // File exists, so let's remove it
        if let Err(err) = fs::remove_file(&rcfile_path) {
            eprintln!("Error deleting file: {}", err);
        } else {
            println!("File deleted successfully!");
        }
    } else {
        println!("File does not exist.");
    }

    let ver_filetype = "VFT_DRV";
    let ver_filesubtype = "VFT2_DRV_SYSTEM";
    let ver_originalfilename = "VER_INTERNALNAME_STR";

    // Create the RC file content
    let rc_content = format!(
        r#"#include <windows.h>
#include <ntverp.h>
#define	VER_FILETYPE	            {file_type}
#define	VER_FILESUBTYPE	            {file_subtype}
#define VER_INTERNALNAME_STR        "{name}"
#define VER_ORIGINALFILENAME_STR    {original_filename}

#undef VER_FILEDESCRIPTION_STR     
#define VER_FILEDESCRIPTION_STR "{description}"

#undef  VER_PRODUCTNAME_STR
#define VER_PRODUCTNAME_STR    VER_FILEDESCRIPTION_STR

#define VER_FILEVERSION        {fileversion},0
#define VER_FILEVERSION_STR    "{productversion}.0"

#undef  VER_PRODUCTVERSION
#define VER_PRODUCTVERSION          VER_FILEVERSION

#undef  VER_PRODUCTVERSION_STR
#define VER_PRODUCTVERSION_STR      VER_FILEVERSION_STR

#define VER_LEGALCOPYRIGHT_STR      {copyright}
#ifdef  VER_COMPANYNAME_STR

#undef  VER_COMPANYNAME_STR
#define VER_COMPANYNAME_STR         {companyname}
#endif

#undef  VER_PRODUCTNAME_STR
#define VER_PRODUCTNAME_STR    {productname}

#include "common.ver""#,
        file_type = ver_filetype,
        file_subtype = ver_filesubtype,
        original_filename = ver_originalfilename
    );
   
    std::fs::write("resources.rc", rc_content).expect("Unable to write RC file");
    invoke_rc(&includeargs, rc_exe_rootpath);
}

// function to invoke RC.exe
fn invoke_rc(includeargs: &Vec<String>, rc_exe_rootpath: String) {

    let resource_script = "resources.rc";
    let rc_exe_path = format!("{}\\rc.exe", rc_exe_rootpath);
    let rc_exe_path = Path::new(&rc_exe_path);
    if !rc_exe_path.exists() {
        eprintln!(
            "Error: rc.exe path does not exist : {}", 
            rc_exe_path.display()
        );
        std::process::exit(1); // Exit with a non-zero status code
    }

    let mut command = Command::new(rc_exe_path);
    command.args(includeargs).arg(resource_script);
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
// function to get package metadata details
fn get_packagemetadatadetails() -> (String, String, String) {
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
    let companyname = metadata.get("wdk")
        .and_then(|wdk| wdk.get("driver-model"))
        .and_then(|driver_model| driver_model.get("companyname"))
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Company name not found in metadata".to_string());
    let copyrightname = metadata.get("wdk")
        .and_then(|wdk| wdk.get("driver-model"))
        .and_then(|driver_model| driver_model.get("copyright"))
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Copyright name not found in metadata".to_string());
    let productname = metadata.get("wdk")
        .and_then(|wdk| wdk.get("driver-model"))
        .and_then(|driver_model| driver_model.get("productname"))
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Product name not found in metadata".to_string());

    (companyname, copyrightname, productname)
}
// function to get package details
fn get_packagedetails() -> (String, String, String, String) {
    let mut fileversion = String::new();
    let mut description = String::new();
    let mut productversion = String::new();
    let mut name = String::new();

    match fs::read_to_string("Cargo.toml") {
        Ok(text1) => {
            for line in text1.lines() {
                if line.starts_with("version") {
                    let start = line.find('"').unwrap_or(0) + 1;
                    let end = line.rfind('"').unwrap_or(0);
                    productversion = line[start..end].to_string();
                    let versionparts: Vec<&str> = productversion.split('.').collect();
                    fileversion = versionparts.join(",");
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
    (productversion, description, fileversion, name)
}
