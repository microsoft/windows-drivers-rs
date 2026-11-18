# Sample WFP Callout Rust Driver

A port of the WDK [`inspect`](https://github.com/microsoft/Windows-driver-samples/tree/main/network/trans/inspect) Windows Filtering Platform callout sample, reduced to a single inspection callout at the `FWPM_LAYER_ALE_AUTH_CONNECT_V4` layer.

The driver registers a callout with the filter engine via `FwpsCalloutRegister3`, then adds a sublayer, a callout object, and a filter that matches outbound TCP connections. For each connection the classify callback prints the remote address and port, and permits the connection.

Unlike the other samples in this directory, this driver is not a Plug and Play device driver: it is a WDM kernel driver that links `fwpkclnt.lib` and `netio.lib` and is installed as a `SERVICE_KERNEL_DRIVER` in the `WFPCALLOUTS` setup class. This is enabled by the `wfp` feature of `wdk-sys`, which is a member of the API subset axis (alongside `gpio`, `hid`, `storage`, etc.) rather than a distinct driver model.

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
   * In the package directory, run: `pnputil.exe /add-driver sample_wfp_callout_driver.inf /install`
   * WFP callout drivers are not Plug and Play drivers, so unlike the other samples there is no software device to create with `devgen.exe`.
   * On Windows 10 1703–1809, `pnputil /add-driver /install` stages the package but does not run its `DefaultInstall` sections, so the service is not created and `sc start` fails. On those builds, install the INF directly instead: `RUNDLL32.EXE SETUPAPI.DLL,InstallHinfSection DefaultInstall 132 <path>\sample_wfp_callout_driver.inf`.

## Test

1. Start the driver (from an elevated prompt):
   * `sc start SampleWfpCallout`
2. Make some outbound TCP connections (ex. `curl https://example.com`, or open a browser) to trigger the callout.
3. Confirm the callout and filter were added to the filter engine:
   * `netsh wfp show state` writes `wfpstate.xml` to the current directory; it should contain a callout named `Sample WFP Callout` and a filter named `Sample WFP Filter`.
4. Stop the driver:
   * `sc stop SampleWfpCallout`

* To capture prints:
  * Start [DebugView](https://learn.microsoft.com/en-us/sysinternals/downloads/debugview)
    1. Enable `Capture Kernel`
    2. Enable `Enable Verbose Kernel Output`
  * Alternatively, you can see prints in an active Windbg session.
    1. Attach WinDBG
    2. `ed nt!Kd_DEFAULT_Mask 0xFFFFFFFF`

**Note**: the filter engine invokes `classifyFn` at `DISPATCH_LEVEL` on the connection path, and printing from it is deliberately verbose. This is acceptable for a sample, but a production callout should not print on the hot path.

The sublayer, callout object, and filter are added under a dynamic filter engine session (`FWPM_SESSION_FLAG_DYNAMIC`), so the filter engine deletes them automatically when the driver closes its engine handle during unload. That is also why unload closes the engine handle *before* calling `FwpsCalloutUnregisterById0`: the filter referencing the callout must be gone before the callout can be unregistered.
