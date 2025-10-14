// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
#![no_main]
#![deny(warnings)]
// SAFETY: "DriverEntry" is the required symbol name for Windows driver entry points.
// No other function in this compilation unit exports this name, preventing symbol conflicts.
#[unsafe(export_name = "DriverEntry")] // WDF expects a symbol with the name DriverEntry
pub extern "system" fn driver_entry(
    driver: wdk_sys::PDRIVER_OBJECT,
    registry_path: wdk_sys::PCUNICODE_STRING,
) -> wdk_sys::NTSTATUS {
    let mut driver_config = wdk_sys::WDF_DRIVER_CONFIG {
        Size: core::mem::size_of::<wdk_sys::WDF_DRIVER_CONFIG>() as wdk_sys::ULONG,
        ..Default::default()
    };
    let driver_handle_output = wdk_sys::WDF_NO_HANDLE as *mut wdk_sys::WDFDRIVER;

    unsafe {
        wdk_sys::call_unsafe_wdf_function_binding!(
            WdfDriverCreate,
            driver,
            registry_path,
            wdk_sys::WDF_NO_OBJECT_ATTRIBUTES,
            &mut driver_config,
            driver_handle_output,
        )
    }
}
