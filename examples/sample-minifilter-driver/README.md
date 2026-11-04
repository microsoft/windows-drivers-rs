# Sample Minifilter Rust Driver

A port of the WDK [`passThrough`](https://github.com/microsoft/Windows-driver-samples/tree/main/filesys/miniFilter/passThrough) File System Minifilter sample, reduced to a single filtered operation (`IRP_MJ_CREATE`).

The driver registers itself with the Filter Manager via `FltRegisterFilter`, attaches to every volume, and prints the name of each file that is opened along with the `NTSTATUS` the file system returned for the open.

Unlike the other samples in this directory, this driver is not a Plug and Play device driver: it is a WDM kernel driver that links `fltMgr.lib` and is installed as a `SERVICE_FILE_SYSTEM_DRIVER`. This is enabled by the `minifilter` feature of `wdk-sys`, which is a member of the API subset axis (alongside `gpio`, `hid`, `storage`, etc.) rather than a distinct driver model.

## Pre-requisites

* WDK environment (either via eWDK or installed WDK)
* LLVM

## Build

* Run `cargo make` in this directory

## Install

1. Copy the driver `package` folder located in the [Cargo Output Directory](https://doc.rust-lang.org/cargo/guide/build-cache.html) to the DUT (Device Under Test: the computer you want to test the driver on). The Cargo Output Directory changes based off of build profile, target architecture, etc.
   * Ex. `<REPO_ROOT>\target\x86_64-pc-windows-msvc\debug\package`, `<REPO_ROOT>\target\x86_64-pc-windows-msvc\release\package`, `<REPO_ROOT>\target\aarch64-pc-windows-msvc\debug\package`, `<REPO_ROOT>\target\aarch64-pc-windows-msvc\release\package`,
   `<REPO_ROOT>\target\debug\package`,
   `<REPO_ROOT>\target\release\package`
2. Install the Certificate on the DUT:
   1. Double click the certificate
   2. Click Install Certificate
   3. Store Location: Local Machine -> Next
   4. Place all certificates in the following Store -> Browse -> Trusted Root Certification Authorities -> Ok -> Next
   5. Repeat 2-4 for Store -> Browse -> Trusted Publishers -> Ok -> Next
   6. Finish
3. Install the driver:
   * In the package directory, run: `pnputil.exe /add-driver sample_minifilter_driver.inf /install`
   * Minifilters are not Plug and Play drivers, so unlike the other samples there is no software device to create with `devgen.exe`.
   * On Windows 10 1703–1809, `pnputil /add-driver /install` stages the package but does not run its `DefaultInstall` sections, so the service is not created and `fltmc load` fails. On those builds, install the INF directly instead: `RUNDLL32.EXE SETUPAPI.DLL,InstallHinfSection DefaultInstall 132 <path>\sample_minifilter_driver.inf`.

## Test

1. Load the minifilter (from an elevated prompt):
   * `fltmc load SampleMinifilter`
2. Open some files (ex. `dir C:\`, or launch any application) to generate `IRP_MJ_CREATE` operations.
3. Confirm the minifilter is attached:
   * `fltmc filters` should list `SampleMinifilter` with altitude `370030`
   * `fltmc instances` should list an instance per volume
4. Unload the minifilter:
   * `fltmc unload SampleMinifilter`

* To capture prints:
  * Start [DebugView](https://learn.microsoft.com/en-us/sysinternals/downloads/debugview)
    1. Enable `Capture Kernel`
    2. Enable `Enable Verbose Kernel Output`
  * Alternatively, you can see prints in an active Windbg session.
    1. Attach WinDBG
    2. `ed nt!Kd_DEFAULT_Mask 0xFFFFFFFF`

**Note**: `IRP_MJ_CREATE` is a very high-frequency operation, and printing from the pre-operation callback of every file open is deliberately verbose. This is acceptable for a sample, but a production minifilter should not print on the hot path.
