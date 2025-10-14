// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
#![no_main]
#![deny(warnings)]

use wdk_sys::*;

// SAFETY: "DriverEntry" is the required symbol name for Windows driver entry points.
// No other function in this compilation unit exports this name, preventing symbol conflicts.
#[unsafe(export_name = "DriverEntry")] // WDF expects a symbol with the name DriverEntry
pub extern "system" fn driver_entry(
    driver: &mut DRIVER_OBJECT,
    registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    // WdfApiThatDoesNotExist is a WDF API that does not exist!
    unsafe { call_unsafe_wdf_function_binding!(WdfApiThatDoesNotExist, driver as PDRIVER_OBJECT,) }
}
