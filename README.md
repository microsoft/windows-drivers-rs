# windows-drivers-rs

This repo is a collection of Rust crates that enable developers to develop Windows Drivers in Rust. It is the intention to support both WDM and WDF driver development models. This repo contains the following crates:

* [wdk-build](./crates/wdk-build): A library to configure a Cargo build script for binding generation and downstream linking of the WDK (Windows Driver Kit). While this crate is written to be flexible with different WDK releases and different WDF version, it is currently only tested for NI eWDK, KMDF 1.33, UMDF 2.33, and WDM Drivers. There may be missing linker options for older DDKs.
* [wdk-sys](./crates/wdk-sys): Direct FFI bindings to APIs available in the Windows Development Kit (WDK). This includes both autogenerated ffi bindings from `bindgen`, and also manual re-implementations of macros that bindgen fails to generate.
* [wdk](./crates/wdk): Safe idiomatic bindings to APIs available in the Windows Development Kit (WDK)
* [wdk-panic](./crates/wdk-panic/): Default panic handler implementations for programs built with WDK
* [wdk-alloc](./crates/wdk-alloc): alloc support for binaries compiled with the Windows Development Kit (WDK)
* [wdk-macros](./crates/wdk-macros): A collection of macros that help make it easier to interact with wdk-sys's direct bindings. This crate is re-exported via `wdk-sys` and crates should typically never need to directly depend on `wdk-macros`

To see an example of this repo used to create drivers, see [Windows-rust-driver-samples](https://github.com/microsoft/Windows-rust-driver-samples).

Note: This project is still in early stages of development and is not yet recommended for production use. We encourage community experimentation, suggestions and discussions! We will be using our [GitHub Discussions forum](https://github.com/microsoft/windows-drivers-rs/discussions) as the main form of engagement with the community!

## <a name="supported-configs">Supported Configurations

This project was built with support of WDM, KMDF, and UMDF drivers in mind, as well as Win32 Services. This includes support for all versions of WDF included in WDK 22H2 and newer. Currently, the crates available on [`crates.io`](https://crates.io) only support KMDF v1.33, but bindings can be generated for everything else by cloning `windows-drivers-rs` and modifying the config specified in [`build.rs` of `wdk-sys`](./crates/wdk-sys/build.rs). Crates.io support for other WDK configurations is planned in the near future.

## Getting Started

### Build Requirements

* Binding generation via `bindgen` requires `libclang`. The easiest way to acquire this is via `winget`
  * `winget install -i LLVM.LLVM --version 17.0.6 --force`
    * Ensure you select the GUI option to add LLVM to the PATH
    * LLVM 18 has a bug that causes bindings to fail to generate for ARM64. Continue using LLVM 17 until LLVM 19 comes out with [the fix](https://github.com/llvm/llvm-project/pull/93235). See [this](https://github.com/rust-lang/rust-bindgen/issues/2842) for more details.
* To execute post-build tasks (ie. `inf2cat`, `infverif`, etc.), `cargo make` is used
  * `cargo install --locked cargo-make --no-default-features --features tls-native`

* Building programs with the WDK also requires being in a valid WDK environment. The recommended way to do this is to [enter an eWDK developer prompt](https://learn.microsoft.com/en-us/windows-hardware/drivers/develop/using-the-enterprise-wdk#getting-started)

### Adding windows-drivers-rs to Your Driver Package

The crates in this repository are available from [`crates.io`](https://crates.io), but take into account the current limitations outlined in [Supported Configurations](#supported-configs). If you need to support a different config, try cloning this repo and using [path dependencies](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#specifying-path-dependencies)

1. Create a new Cargo package with a lib crate:

   ```pwsh
   cargo new <driver_name> --lib
   ```

2. Add dependencies on `windows-drivers-rs` crates:

   ```pwsh
   cd <driver_name>
   cargo add --build wdk-build
   cargo add wdk wdk-sys wdk-alloc wdk-panic
   ```

3. Set the crate type to `cdylib` by adding the following snippet to `Cargo.toml`:

   ```toml
   [lib]
   crate-type = ["cdylib"]
   ```

4. Mark the crate as a driver with a wdk metadata section. This lets the cargo-make tasks know that the package is a driver and that the driver packaging steps need to run.

   ```toml
   [package.metadata.wdk]
   ```

5. Set crate panic strategy to `abort` in `Cargo.toml`:

   ```toml
   [profile.dev]
   panic = "abort"
   lto = true # optional setting to enable Link Time Optimizations

   [profile.release]
   panic = "abort"
   lto = true # optional setting to enable Link Time Optimizations
   ```

6. Create a `build.rs` and add the following snippet:

   ```rust
   fn main() -> Result<(), wdk_build::ConfigError> {
      wdk_build::Config::from_env_auto()?.configure_binary_build();
      Ok(())
   }
   ```

7. Mark your driver crate as `no_std` in `lib.rs`:

   ```rust
   #![no_std]
   ```

8. Add a panic handler in `lib.rs`:

   ```rust
   #[cfg(not(test))]
   extern crate wdk_panic;

   ```

9. Optional: Add a global allocator in `lib.rs`:

   ```rust
   #[cfg(not(test))]
   use wdk_alloc::WDKAllocator;

   #[cfg(not(test))]
   #[global_allocator]
   static GLOBAL_ALLOCATOR: WDKAllocator = WDKAllocator;
   ```

   This is only required if you want to be able to use the [`alloc` modules](https://doc.rust-lang.org/alloc/) in the rust standard library. You are also free to use your own implementations of global allocators.

10. Add a DriverEntry in `lib.rs`:

   ```rust
   use wdk_sys::{
      DRIVER_OBJECT,
      NTSTATUS,
      PCUNICODE_STRING,
   };

   #[export_name = "DriverEntry"] // WDF expects a symbol with the name DriverEntry
   pub unsafe extern "system" fn driver_entry(
      driver: &mut DRIVER_OBJECT,
      registry_path: PCUNICODE_STRING,
   ) -> NTSTATUS {
      0
   }
   ```

11. Add a `Makefile.toml`:
   ```toml
   extend = "target/rust-driver-makefile.toml"

   [env]
   CARGO_MAKE_EXTEND_WORKSPACE_MAKEFILE = true

   [config]
   load_script = '''
   #!@rust
   //! ```cargo
   //! [dependencies]
   //! wdk-build = "0.2.0"
   //! ```
   #![allow(unused_doc_comments)]

   wdk_build::cargo_make::load_rust_driver_makefile()?
   '''
   ```

12. Add an inx file that matches the name of your `cdylib` crate.

13. Build the driver:

   ```pwsh
   cargo make
   ```

A signed driver package, including a `WDRLocalTestCert.cer` file, will be generated at `target/<Cargo profile>/package`. If a specific target architecture was specified, the driver package will be generated at `target/<target architecture>/<Cargo profile>/package`

## Cargo Make

[`cargo-make`](https://github.com/sagiegurari/cargo-make) is used to facilitate builds using `windows-drivers-rs`, including for executing post-build driver packaging steps.

To execute the default action (build and package driver):

`cargo make default`

When executing the default task, just `cargo make` make also works since the `default` task is implied.

### Argument Forwarding

`windows-drivers-rs` extends `cargo make` to forward specific arguments to the underlying `cargo` commands. In order to specify arguments to forward, they must be provided **after explicitly specifying the `cargo-make` task name** (ie. omitting the name for the `default` task is not supported).

#### Examples

For a specific target:

`cargo make default --target <TARGET TRIPLE>`

For release builds:

`cargo make default --release` or `cargo make default --profile release`

To specify specific features:

`cargo make default --features <FEATURES>`

To specify a specific rust toolchain:

`cargo make default +<TOOLCHAIN>`

To display help and see the full list of supported CLI args to forward to Cargo:

`cargo make help`

### Driver Package Signature Verification

The `WDK_BUILD_ENABLE_SIGNTOOL_VERIFY` [cargo-make environment variable](https://github.com/sagiegurari/cargo-make?tab=readme-ov-file#environment-variables) can be set to `true` to enable tasks that handle signature verification of the generated `.sys` and `.cat` files. `signtool verify` requires the certificate to be installed as in the `Trusted Root Certification Authorities` for this verification to function. These tasks are not enabled by default as the default behavior of `WDR` is to sign with a generated test certificate. These test certificates are typically only installed into `Trusted Root Certification Authorities` on computers dedicated to testing drivers, and not personal development machines, given the security implications of installing your own root certificates.

If you understand these implications, and have installed the test certificate, then you may validate the signatures as follows:

```
cargo make --env WDK_BUILD_ENABLE_SIGNTOOL_VERIFY=true
```

## Crates.io Release Policy

Releases to crates.io are not made after every change merged to main. Releases will only be made when requested by the community, or when the `windows-drivers-rs` team believes there is sufficient value in pushing a release.

## Trademark Notice

Trademarks This project may contain trademarks or logos for projects, products, or services. Authorized use of Microsoft trademarks or logos is subject to and must follow Microsoft’s Trademark & Brand Guidelines. Use of Microsoft trademarks or logos in modified versions of this project must not cause confusion or imply Microsoft sponsorship. Any use of third-party trademarks or logos are subject to those third-party’s policies.
