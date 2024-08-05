// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
#![no_main]
#![deny(warnings)]

extern "C" fn evt_driver_device_add(
    _driver: wdk_sys::WDFDRIVER,
    mut device_init: *mut wdk_sys::WDFDEVICE_INIT,
) -> wdk_sys::NTSTATUS {
    let mut device_handle_output: wdk_sys::WDFDEVICE = wdk_sys::WDF_NO_HANDLE.cast();

    unsafe {
        wdk_sys::call_unsafe_wdf_function_binding!(
            WdfDeviceCreate,
            &mut device_init,
            wdk_sys::WDF_NO_OBJECT_ATTRIBUTES,
            &mut device_handle_output,
        )
    }
}
