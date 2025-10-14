// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

#![no_main]
#![deny(warnings)]

//! This is a regression test for a bug where the arguments to the
//! [`call_unsafe_wdf_function_binding`] macro would need to be brought into
//! scope, but rust-analyzer would treat them as unused imports. This resulted
//! in the following compilation error:
#[rustfmt::skip]
/// ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈
/// error: unused import: `WDF_NO_OBJECT_ATTRIBUTES`
///   --> C:/windows-drivers-rs/tests/wdk-macros-tests/tests/outputs/nightly/macrotest/bug_unused_imports.rs:31:49
///    |
/// 31 | use wdk_sys::{call_unsafe_wdf_function_binding, WDF_NO_OBJECT_ATTRIBUTES, NTSTATUS, PDRIVER_OBJECT, ULONG, PCUNICODE_STRING, WDF_DRIVER_C...
///    |                                                 ^^^^^^^^^^^^^^^^^^^^^^^^
///    |
/// note: the lint level is defined here
///   --> C:/windows-drivers-rs/tests/wdk-macros-tests/tests/wdk-macros-tests/tests/outputs/nightly/macrotest/bug_unused_imports.rs:5:9
///    |
/// 5  | #![deny(warnings)]
///    |         ^^^^^^^^
///    = note: `#[deny(unused_imports)]` implied by `#[deny(warnings)]`
/// ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈

use wdk_sys::{
    call_unsafe_wdf_function_binding,
    NTSTATUS,
    PCUNICODE_STRING, 
    PDRIVER_OBJECT,
    ULONG,
    WDFDRIVER,
    WDF_DRIVER_CONFIG,
    WDF_NO_HANDLE,
    WDF_NO_OBJECT_ATTRIBUTES,
};

// SAFETY: "DriverEntry" is the required symbol name for Windows driver entry points.
// No other function in this compilation unit exports this name, preventing symbol conflicts.
#[unsafe(export_name = "DriverEntry")] // WDF expects a symbol with the name DriverEntry
pub extern "system" fn driver_entry(
    driver: PDRIVER_OBJECT,
    registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    let mut driver_config = WDF_DRIVER_CONFIG {
        Size: core::mem::size_of::<WDF_DRIVER_CONFIG>() as ULONG,
        ..Default::default()
    };
    let driver_handle_output = WDF_NO_HANDLE as *mut WDFDRIVER;

    unsafe {
        call_unsafe_wdf_function_binding!(
            WdfDriverCreate,
            driver,
            registry_path,
            WDF_NO_OBJECT_ATTRIBUTES,
            &mut driver_config,
            driver_handle_output,
        )
    }
}
