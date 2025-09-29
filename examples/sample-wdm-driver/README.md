# Sample WDM Rust Driver

## Pre-requisites

* WDK environment (either via eWDK or installed WDK)
* LLVM

## Build

* Run `cargo make` in this directory

## Install

1. Copy the following to the DUT (Device Under Test: the computer you want to test the driver on):
   1. The driver `package` folder located in the [Cargo Output Directory](https://doc.rust-lang.org/cargo/guide/build-cache.html). The Cargo Output Directory changes based off of build profile, target architecture, etc.
     * Ex. `<REPO_ROOT>\target\x86_64-pc-windows-msvc\debug\package`, `<REPO_ROOT>\target\x86_64-pc-windows-msvc\release\package`, `<REPO_ROOT>\target\aarch64-pc-windows-msvc\debug\package`, `<REPO_ROOT>\target\aarch64-pc-windows-msvc\release\package`,
     `<REPO_ROOT>\target\debug\package`,
     `<REPO_ROOT>\target\release\package`
   2. The version of `devgen.exe` from the WDK Developer Tools that matches the architecture of your DUT
     * Ex. `C:\Program Files\Windows Kits\10\Tools\10.0.22621.0\x64\devgen.exe`. Note: This path will vary based off your WDK environment
2. Install the Certificate on the DUT:
   1. Double click the certificate
   2. Click Install Certificate
   3. Store Location: Local Machine -> Next
   4. Place all certificates in the following Store -> Browse -> Trusted Root Certification Authorities -> Ok -> Next
   5. Repeat 2-4 for Store -> Browse -> Trusted Publishers -> Ok -> Next
   6. Finish
3. Install the driver:
   * In the package directory, run: `pnputil.exe /add-driver sample_wdm_driver.inf /install`
4. Create a software device:
   * In the directory that `devgen.exe` was copied to, run: `devgen.exe /add /hardwareid "root\SAMPLE_WDM_HW_ID"`

## Test

* To capture prints:
  * Start [DebugView](https://learn.microsoft.com/en-us/sysinternals/downloads/debugview)
    1. Enable `Capture Kernel`
    2. Enable `Enable Verbose Kernel Output`
  * Alternatively, you can see prints in an active Windbg session.
    1. Attach WinDBG
    2. `ed nt!Kd_DEFAULT_Mask 0xFFFFFFFF`
