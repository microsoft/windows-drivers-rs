// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

#![no_main]
#![feature(hint_must_use)]
use wdk_sys::*;

extern "C" fn evt_driver_device_add(
    _driver: WDFDRIVER,
    mut device_init: *mut WDFDEVICE_INIT,
) -> NTSTATUS {
    let mut device_handle_output: WDFDEVICE = WDF_NO_HANDLE.cast();

    unsafe {
        wdk_macros::call_unsafe_wdf_function_binding! {
            WdfDeviceCreate(
                &mut device_init,
                WDF_NO_OBJECT_ATTRIBUTES,
                &mut device_handle_output)
        }
    }
}
