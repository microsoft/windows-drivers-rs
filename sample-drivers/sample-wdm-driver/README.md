# Sample WDM Rust Driver

## Pre-requisites

* WDK environment (either via eWDK or installed WDK)
* LLVM

## Build

* Run `cargo make` in this directory

## Install

1. Copy the following to the DUT (Device Under Test: the computer you want to test the driver on):
   1. [`.\target\x86_64-pc-windows-msvc\debug\sample_wdm_driver.sys`](.\target\x86_64-pc-windows-msvc\debug\sample_wdm_driver.sys)
   2. [`.\DriverCertificate.cer`](.\DriverCertificate.cer)
2. Install the Certificate on the DUT:
   1. Double click the certificate
   2. Click Install Certificate
   3. Select a Store Location __(Either Store Location is Fine)__ -> Next
   4. Place all certificates in the following Store -> Browse -> Trusted Root Certification Authorities -> Ok -> Next
   5. Finish
3. Install the driver:
   * In the package directory, run: `sc.exe create sample-wdm-rust-driver binPath=sample_wdm_driver.sys type= kernel`

## Test

1. To capture prints:
   * Start [DebugView](https://learn.microsoft.com/en-us/sysinternals/downloads/debugview)
      1. Enable `Capture Kernel`
      2. Enable `Enable Verbose Kernel Output`
   * Alternatively, you can see prints in an active Windbg session.
     1. Attach WinDBG
     2. `ed nt!Kd_DEFAULT_Mask 0xFFFFFFFF`

2. Load and Start Driver:
   * `sc.exe start sample-wdm-rust-driver`

3. Post-testing cleanup:
   * Stop Driver: `sc.exe stop sample-wdm-rust-driver`
   * Delete Driver: `sc.exe delete sample-wdm-rust-driver`
