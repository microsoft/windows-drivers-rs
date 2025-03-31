# cargo-wdk

A Command-Line Interface (CLI) utility to create and build Windows driver projects written in Rust. 
`cargo-wdk` is a cargo extension (plugin) with "cargo-like" commands to
- create new projects from scratch 
- build new or existing projects

**NOTE**: `cargo-wdk` utility is designed to work with projects that depend on the Windows Driver Kit (WDK) and the "windows-drivers-rs" family of crates.

## Features

- `new` command uses templates in `crates/cargo-wdk/templates` directory to scaffold the project appropriately.

- `build` command can be used for individual projects and workspaces. Workspaces with both driver and non-driver members are also supported.

    1. For individual projects, a signed driver package, including a `WDRLocalTestCert.cer` file, will be generated at `target/<Cargo profile>/_package`. If a specific target architecture was specified, the driver package will be generated at `target/<target architecture>/<Cargo profile>/_package`
    2. For workspaces, 
        - Non-driver members are compiled (using standard cargo build), 
        - Driver members are built and packaged. The final driver packages will be generated in the workspace's `/target/` directory.

## Installation

To install `cargo-wdk`, you need to have Rust (and Cargo) installed on your system. You can install Rust by following the instructions on the [Rust website](https://www.rust-lang.org/tools/install).

Once you have Rust installed, you can install `cargo-wdk` using the cargo install command in one of the following ways:

### Install by cloning the source
    - Clone the windows-drivers-rs repository.
    - Navigate to crates\cargo-wdk directory.
    - Run the following command:
        ```pwsh
        cargo install --path . --locked
        ```

### Install by specifying Git repository
    - Run the following command:
        ```pwsh
        cargo install --git https://github.com/microsoft/windows-drivers-rs.git --bin cargo-wdk --locked
        ```

The install command compiles the `cargo-wdk` binary and copies the compiled binary to Cargo's bin directory (%USERPROFILE%.cargo\bin).

You can test the installation by running the following command:
```pwsh
cargo wdk --version
```

For help on usage, run the command:
```pwsh
cargo wdk --help
```

## Installing WDK

`cargo-wdk` builds the drivers using the WDK. Please ensure that the WDK is installed on the development system.
The recommended way to do this is to [enter an eWDK developer prompt](https://learn.microsoft.com/en-us/windows-hardware/drivers/develop/using-the-enterprise-wdk#getting-started)

## Usage

1. Use the `new` command to create a new Rust driver project - 
    ```pwsh
    cargo wdk new [OPTIONS] [DRIVER_PROJECT_NAME]
    ```
    
    Example Usage:
    ```pwsh
    cargo wdk new sample_driver UMDF
    ```
    ```pwsh
    cargo wdk new sample_driver --driver-type KMDF
    ```

    Use `--help` for more information on arguments and options

2. Use the `build` command to build and package driver projects.
    ```pwsh
    cargo wdk build [OPTIONS]
    ```
    
    Example Usage: 
    * Navigate to the project/workspace root and run - 
    ```pwsh 
    cargo wdk build 
    ```
    ```pwsh 
    cargo wdk build --cwd /path/to/project
    ```

    Use `--help` for more information on arguments and options

## Driver Package Signature Verification

Verification using the `signtool verify` command requires the certificate to be installed in the development system's `Trusted Root Certification Authorities` to succeed. Therefore, the verification tasks are not enabled by default. However, verification can be enabled as follows: 

```pwsh
cargo wdk build --verify-signature=true
```

**NOTE**: The above command creates a **test certificate** in the `Trusted Root Certification Authorities` and uses the same certificate for the verification. The `build` command does not allow passing own certificates at the moment.