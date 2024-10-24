use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use cargo_metadata::MetadataCommand;

pub fn generate_and_compile_rcfile(include_paths: Vec<PathBuf>) {
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

    // Print the modified include arguments
    println!("IncludeArgs: {:?}", includeargs);

    let (companyname, copyright, productname) = get_packagemetadatadetails();
    let (productversion, description, fileversion) = get_packagedetails();
    getandset_rcfile(companyname, copyright, productname, productversion ,description, fileversion, &includeargs);
}
fn getandset_rcfile(s1: String, s2: String, s3: String, s4:String, s5:String, s6:String, s7: &Vec<String>) {
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
    let ver_internalname = "sample-kmdf-driver.sys";
    let ver_originalfilename = "VER_INTERNALNAME_STR";

    // Create the RC file content
    let rc_content = format!(
        r#"#include <windows.h>
#include <ntverp.h>
#define	VER_FILETYPE	            {file_type}
#define	VER_FILESUBTYPE	            {file_subtype}
#define VER_INTERNALNAME_STR        "{internal_name}"
#define VER_ORIGINALFILENAME_STR    {original_filename}

#undef VER_FILEDESCRIPTION_STR     
#define VER_FILEDESCRIPTION_STR "{s5}"

#undef  VER_PRODUCTNAME_STR
#define VER_PRODUCTNAME_STR    VER_FILEDESCRIPTION_STR

#define VER_FILEVERSION        {s6},0
#define VER_FILEVERSION_STR    "{s4}.0"

#undef  VER_PRODUCTVERSION
#define VER_PRODUCTVERSION          VER_FILEVERSION

#undef  VER_PRODUCTVERSION_STR
#define VER_PRODUCTVERSION_STR      VER_FILEVERSION_STR

#define VER_LEGALCOPYRIGHT_STR      {s2}
#ifdef  VER_COMPANYNAME_STR

#undef  VER_COMPANYNAME_STR
#define VER_COMPANYNAME_STR         {s1}
#endif

#undef  VER_PRODUCTNAME_STR
#define VER_PRODUCTNAME_STR    {s3}

#include "common.ver""#,
        file_type = ver_filetype,
        file_subtype = ver_filesubtype,
        internal_name = ver_internalname,
        original_filename = ver_originalfilename
    );

    // Print the RC file content
    //println!("{}", env!("CARGO_PKG_VERSION"));
    //println!("{}", env!("CARGO_PKG_METADATA.WDK"));
    //println!("cargopkgcrate:{}", env!("CARGO_PKG_CRATE_NAME"));
    
   
    std::fs::write("resources.rc", rc_content).expect("Unable to write RC file");
    invoke_rc(&s7);
}

fn invoke_rc(s7: &Vec<String>) {
    // Replace with the actual path to rc.exe
    let rc_path = env::var("PATH_TO_RC").unwrap_or_else(|_| {
        // Default path if environment variable is not set
        r#"D:\EWDK\content\Program Files\Windows Kits\10\bin\10.0.22621.0\x86\rc.exe"#.to_string()
    });

    println!("Using rc.exe path: {}", rc_path);

    // Replace "resource.rc" with the name of your resource script file
    let resource_script = "resources.rc";
    //for value in s7.into_iter() {
      //  println!("Got: {}", value.to_string());
    //}
    //println!("include args: {:?}", s7);
    let um_path = r#"D:\EWDK\content\Program Files\Windows Kits\10\Include\10.0.22621.0\um"#.to_string();
    let include_string = "/I";
    //println!("Modified Path: {}", modified_path);
   // let s8 = "/I r#"D:\EWDK\content\Program Files\Windows Kits\10\Include\10.0.22621.0\um"#".to_string();
    let mut command = Command::new(&rc_path);
    command.args(s7).arg(include_string).arg(um_path).arg(resource_script);
    println!("Command executed: {:?}", command); 
    
   //let status = Command::new(&rc_path).args(s7).arg(resource_script).status();
   let status = command.status();

   //let mut command = Command::new(&rc_path);
   //command.args(s7).arg(resource_script);
    //println!("Command executed: {:?}", command); 

    match status {
        Ok(exit_status) => {
            if exit_status.success() {
                println!("Resource compilation successful!");
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

fn get_packagedetails() -> (String, String, String) {
    let mut fileversion = String::new();
    let mut description = String::new();
    let mut productversion = String::new();

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
            }
        }
        Err(_) => {
            eprintln!("Error reading Cargo.toml");
        }
    }
    (productversion, description, fileversion)
}
