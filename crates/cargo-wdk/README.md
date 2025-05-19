# cargo-wdk

`cargo-wdk` is a Cargo extension (plugin) that provides a Cargo-like command line interface for developers to create and build Windows Rust Driver crates that depend on the Windows Driver Kit (WDK) and windows-drivers-rs. It extends `new` and `build` functionalities of Cargo and allows developers to start new projects and build existing or new projects with simple “Cargo-like" commands. 

## Features

`cargo-wdk` currently supports creating and building Windows Rust driver crates.

- **`new`** command takes a name, and the driver type (KMDF, UMDF or WDM) as input and creates a new Rust driver project. It relies on pre-defined templates (in `./crates/cargo-wdk/templates`) to scaffold the project with the required files and boilerplate code.

- **`build`** command compiles, builds and packages Rust driver projects. The build profile and the target architecture are optional arguments, and can be passed to the command invocation. Consists of a **`build_task`** that invokes `cargo build` and **`package_task`** that invokes WDK binaries - `StampInf`, `Inf2Cat`, `InfVerif`, `Signtool`, `CertMgr` in the correct order, and generates the final driver package. If no valid WDK configuration is found in the package/workspace `package_task` is skipped.

    The command can be run from:  
        
    1. Root of an individual/stand-alone crate: Final package available under the crate's **target** directory - `./target/[target_triple]/[profile]/<driver_crate_name>_package`. 

    2. Root of a workspace: Final package will be available under the workspace's `target` directory - `./target/[target_triple]/[profile]/<driver_crate_name>_package`.
            
    3. Root of a member crate of a workspace: Recognizes the workspace the member is part of, executes the build and package tasks for this member alone. Final package will be available under the workspace's `target` directory. 
        
    4. Root of an emulated workspace: An emulated workspace is a directory containing one or more Rust workspaces. In this case, `cargo-wdk` builds each workspace individually and the final driver packages can be found under the `target` directory of the specific workspaces/crates.

    **NOTE**: The `build` command can build workspaces containing both driver and non-driver crates: driver crates are built and packaged, while non-driver crates are only built and the packaging step is skipped.

## Installation

To install `cargo-wdk`, you need to have [Rust installed on your system](https://www.rust-lang.org/tools/install).

Once you have Rust installed, you can install `cargo-wdk` as follows:

```pwsh
cargo install --git https://github.com/microsoft/windows-drivers-rs.git --bin cargo-wdk --locked
```

The install command compiles the `cargo-wdk` binary and copies it to Cargo's bin directory - `%USERPROFILE%.cargo\bin`.

You can test the installation by running:
```pwsh
cargo wdk --version
```

For help, run:
```pwsh
cargo wdk --help
```

## Installing WDK

`cargo-wdk` builds the drivers using the WDK. Please ensure that the WDK is installed on the development system.
The recommended way to do this is to [enter an eWDK developer prompt](https://learn.microsoft.com/en-us/windows-hardware/drivers/develop/using-the-enterprise-wdk#getting-started).

## Usage Examples

1. `new` command to create a new Rust driver project: 
    ```pwsh
    cargo wdk new [OPTIONS] [DRIVER_PROJECT_NAME]
    ```
    
    Example Usage:
    ```pwsh
    cargo wdk new sample_driver UMDF
    ```

    Use `--help` for more information on arguments and options

2. `build` command to build and package driver projects:
    ```pwsh
    cargo wdk build [OPTIONS]
    ```
    
    Example Usage: 
    * Navigate to the project/workspace root and run

        ```pwsh 
        cargo wdk build 
        ```

    * With `--cwd`

        ```pwsh 
        cargo wdk build --cwd /path/to/project
        ```

    * With `--target-arch`

        ```pwsh 
        cargo wdk build --target-arch arm64
        ```

    * With `--profile`

        ```pwsh 
        cargo wdk build --profile Release
        ```

    Please use `--help` for more information on arguments and options.

## Driver Package Signature Verification

The `build` command can be run with `--verify-signature` option to enable the verification of the `.sys/.dll` and `.cat` files generated in the final package. Currently, the `build` command uses a **test certificate** named "WDRLocalTestCert" in a store named "WDRTestCertStore" to sign the files. Verification using the `signtool verify` command requires these certificates to be present in the host system's `Trusted Root Certification Authorities`. Typically, these test certificates are only installed into `Trusted Root Certification Authorities` on computers dedicated to testing drivers, and not personal development machines, given the security implications of installing your own root certificates.

If you understand these implications, and have installed the test certificate, then you may validate the signatures as follows:

    ```pwsh
    cargo wdk build --verify-signature
    ```

## Building Sample Class Drivers

The `build` command can be used to build drivers whose class is defined as `Sample` in its `.inx` file, for ex, [echo (kmdf) DriverSync](https://github.com/microsoft/Windows-rust-driver-samples/tree/main/general/echo/kmdf/driver/DriverSync). The command handles passing additional flags to `InfVerif` task based on the WDK Version being used. So, if you are building a `Sample` class driver, you may use the `build` command with the `--sample` flag as follows,

    ```pwsh
    cargo wdk build --sample
    ```

**NOTE**: Running `cargo wdk build --sample` from a workspace root will try to package **all** the driver crates in that workspace as `Sample` class drivers. If the workspace contains a non-sample class driver, it will result in an error. A workaround is to build each crate individually (pass `--sample` only for "Sample" class driver) or ensure all driver crates in a workspace are "Sample" class drivers.
