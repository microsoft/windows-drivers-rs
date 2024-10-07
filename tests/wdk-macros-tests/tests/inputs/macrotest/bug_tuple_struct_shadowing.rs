// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

#![no_main]
#![deny(warnings)]

/// This is a regression test for a bug where the
/// [`call_unsafe_wdf_function_binding`] macro would generate code that prevented
/// anything in scope from having the same name as one of the c function's
/// parameter names. This resulted in the following compilation error:
/// 
#[rustfmt::skip]
/// ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈
/// error[E0530]: function parameters cannot shadow tuple structs
///   --> C:/windows-drivers-rs/tests/wdk-macros-tests/tests/outputs/nightly/macrotest/bug_tuple_struct_shadowing.rs:34:9
///    |
/// 30 |   pub struct DeviceInit(wdk_sys::PWDFDEVICE_INIT);
///    |   ------------------------------------------------ the tuple struct `DeviceInit` is defined here
/// ...
/// 34 | /         call_unsafe_wdf_function_binding!(
/// 35 | |             WdfDeviceInitSetPnpPowerEventCallbacks,
/// 36 | |             device_init.0,
/// 37 | |             pnp_power_callbacks
/// 38 | |         )
///    | |_________^ cannot be named the same as a tuple struct
///    |
///    = note: this error originates in the macro `$crate::__proc_macros::call_unsafe_wdf_function_binding` which comes from the expansion of the macro `call_unsafe_wdf_function_binding` (in Nightly builds, run with -Z macro-backtrace for more info)
/// ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈


use wdk_sys::call_unsafe_wdf_function_binding;

#[repr(transparent)]
pub struct DeviceInit(wdk_sys::PWDFDEVICE_INIT);

fn foo(device_init: DeviceInit, pnp_power_callbacks: wdk_sys::PWDF_PNPPOWER_EVENT_CALLBACKS) {
    unsafe {
        call_unsafe_wdf_function_binding!(
            WdfDeviceInitSetPnpPowerEventCallbacks,
            device_init.0,
            pnp_power_callbacks
        )
    }
}
