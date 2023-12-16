// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

#![no_main]
#![feature(hint_must_use)]
use wdk_sys::*;

// {86E0D1E0-8089-11D0-9CE4-08003E301F73}
const GUID_DEVINTERFACE_COMPORT: GUID = GUID {
    Data1: 0x86E0D1E0u32,
    Data2: 0x8089u16,
    Data3: 0x11D0u16,
    Data4: [
        0x9Cu8, 0xE4u8, 0x08u8, 0x00u8, 0x3Eu8, 0x30u8, 0x1Fu8, 0x73u8,
    ],
};

fn create_device_interface(wdf_device: WDFDEVICE) -> NTSTATUS {
    unsafe {
        wdk_macros::call_unsafe_wdf_function_binding! {
            WdfDeviceCreateDeviceInterface(
                wdf_device,
                &GUID_DEVINTERFACE_COMPORT,
                core::ptr::null())
        }
    }
}
