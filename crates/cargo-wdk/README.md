# cargo-wdk

A development tool for Windows Rust drivers based on [windows-drivers-rs](https://github.com/microsoft/windows-drivers-rs).

## Installation

To install, run:

```pwsh
cargo install cargo-wdk
```

## Commands

`cargo-wdk` exposes two commands `new` and `build`.

`new` creates new driver projects from pre-defined templates and helps you get started faster. It invokes `cargo new` to create the project structure and then adds all the necessary files from a template.

`build` compiles the source code of a driver project and creates a [driver package](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/driver-packages). It invokes `cargo build` to compile the code and then runs other required tools like `stampinf`, `inf2cat` and `signtool` in the correct order to produce the final driver package.

## Usage

### `new` Command

```pwsh
Usage: cargo wdk new [OPTIONS] <--kmdf|--umdf|--wdm> <PATH>

Arguments:
  <PATH>  Path at which the new driver crate should be created

Options:
      --kmdf  Create a KMDF driver crate
      --umdf  Create a UMDF driver crate
      --wdm   Create a WDM driver crate
  -h, --help  Print help

Verbosity:
  -v, --verbose...  Increase logging verbosity
  -q, --quiet...    Decrease logging verbosity
```

`new` takes the type of driver project you want to create (`kmdf`, `umdf` or `wdm`) and its destination path (`PATH`) as inputs along with flags specifying log verbosity.

The last component of `PATH` is used as the name of the crate.

#### Examples

- To create a new KMDF project called `my_driver` under the current folder run:

    ```pwsh  
    cargo wdk new my_driver --kmdf  
    ```

- To create a new UMDF project called `my_driver` under the folder `my_projects` run:  

    ```pwsh  
    cargo wdk new my_projects\my_driver --umdf  
    ```  

### `build` Command

```pwsh
Usage: cargo wdk build [OPTIONS]

Options:
      --profile <PROFILE>          Build artifacts with the specified profile
      --target-arch <TARGET_ARCH>  Build for the target architecture
      --sign-mode <SIGN_MODE>      Driver signing mode [default: test] [possible values: off, test]
      --verify-signature           Verify the signature
      --sample                     Build sample class driver project
      --locked                     Assert that `Cargo.lock` will remain unchanged
  -h, --help                       Print help

Driver Signing:
      --file-digest-algorithm <ALGORITHM>
                                        File digest algorithm [default: SHA256]
                                        [possible values: SHA1, SHA256, SHA384, SHA512]
      --cert-store <STORE>              Certificate store name (use with `--cert-name`)
      --cert-name <NAME>                Certificate subject name (use with `--cert-store`)
      --cert-file <PATH>                PFX certificate file to sign with
      --cert-password-env <ENV_VAR>     Env var holding the PFX password (use with `--cert-file`)

Feature Selection:
      --all-features               Activate all available features
      --no-default-features        Do not activate the `default` feature
  -F, --features <FEATURES>        Space-separated list of features to activate

Verbosity:
  -v, --verbose...  Increase logging verbosity
  -q, --quiet...    Decrease logging verbosity
```

`build` takes a number of inputs specifying build profile (`dev` or `release`), target architecture (`amd64` or `arm64`), the driver signing mode, a flag enabling signature verification and a flag indicating a sample driver along with verbosity flags.

When the command completes the packaged driver artifacts are emitted at the path `target\<profile>\<project-name>-package`.

#### Workspace support

`build` supports workspaces. If run at the root of a workspace, it will build and package all driver projects in it. If the workspace contains any non-driver projects they will also be built but not packaged.

#### Sample Drivers

Building a sample driver requires the `--sample` flag. If it is not specified, the build will fail.

If you have a workspace with a mix of sample and non-sample driver projects, the build will fail as that scenario is not supported yet. In the future `build` will be able to automatically detect sample projects. That will remove the need for the `--sample` flag and enable support for this scenario.

#### Signing and Verification

The `build` command has a `--sign-mode` flag that controls how driver artifacts are signed. It accepts the following values:

- `test` (default): Sign with a test certificate. The command looks for a certificate called `WDRLocalTestCert` in a store called `WDRTestCertStore`. If you wish to use your own certificate, add it to the same store with the same name. Otherwise a self-signed certificate will be automatically generated, added, and used for signing.
- `off`: Skip signing entirely. This is useful when you intend to sign the artifacts later with your own toolchain.

If the `--verify-signature` flag is provided, the signatures are verified after signing. For verification to work, make sure you add a copy of the signing certificate in the `Trusted Root Certification Authorities` store. For security reasons `build` does not automatically do this even when it automatically generates the cert. You will have to always perform this step manually.

`--verify-signature` cannot be combined with `--sign-mode=off` because if signing is off there is nothing to verify. Passing both will cause `build` to fail with an error.

##### Selecting a certificate

By default test signing uses the auto-generated `WDRLocalTestCert` certificate. You can instead point `build` at your own certificate. These options apply only when signing (i.e. `--sign-mode=test`); supplying any of them with `--sign-mode=off` is an error.

- `--cert-store <STORE>` together with `--cert-name <NAME>` selects an existing certificate from a certificate store by store name and subject name (maps to `signtool /s <STORE> /n <NAME>`). Both flags must be provided together.
- `--cert-file <PATH>` signs with a certificate from a PFX file (maps to `signtool /f <PATH>`); the path must exist. `--cert-file` is mutually exclusive with `--cert-store`/`--cert-name`.
- `--cert-password-env <ENV_VAR>` names an environment variable that holds the PFX password. The password is read from the environment (never accepted as a plaintext CLI value, so it never leaks into process listings, shell history, or CI logs) and mapped to `signtool /p`. It requires `--cert-file`.
- `--file-digest-algorithm <ALGORITHM>` controls the file digest algorithm passed to `signtool /fd`. Default `SHA256`; one of `SHA1`, `SHA256`, `SHA384`, `SHA512`.

Consistent with the WDK MSBuild `TestSign` target, test signing does **not** timestamp the signature (no `/t`, `/tr`, or `/td` switches). Timestamping is a production-signing concern and is planned for the production-signing follow-up.

If signing fails, `build` best-effort removes the produced `.sys`, `.cat`, and `.cer` artifacts from the package directory so it never leaves unsigned output behind.

#### Examples

- To build a driver project with default options, navigate to the root of the project and run:

    ```pwsh
    cargo wdk build 
    ```

- To build for target `arm64` and the `release` profile, navigate to the root of the project and run:

    ```pwsh
    cargo wdk build --target-arch arm64  --profile release
    ```

- To build projects in a workspace for target `amd64`, navigate to the root of the workspace and run:

    ```pwsh
    cargo wdk build --target-arch amd64
    ```

- To build a driver project with signing off, navigate to the root of the project and run:

    ```pwsh
    cargo wdk build --sign-mode off
    ```

- To test-sign with a certificate selected from a store by subject name, run:

    ```pwsh
    cargo wdk build --cert-store MyStore --cert-name MyCert
    ```

- To test-sign with a PFX file whose password comes from an environment variable, run:

    ```pwsh
    $env:PFX_PW = "<password>"
    cargo wdk build --cert-file C:\certs\my.pfx --cert-password-env PFX_PW
    ```

- To test-sign with a specific file digest algorithm, run:

    ```pwsh
    cargo wdk build --file-digest-algorithm SHA384
    ```
