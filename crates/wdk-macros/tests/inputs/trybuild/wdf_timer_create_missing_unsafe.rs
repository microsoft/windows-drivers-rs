// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
#![no_main]
#![deny(warnings)]

use wdk_sys::*;

fn foo(timer_config: &mut WDF_TIMER_CONFIG, attributes: &mut WDF_OBJECT_ATTRIBUTES,) {
    let mut timer = core::ptr::null_mut();
    let _nt_status = macros::call_unsafe_wdf_function_binding!(
        WdfTimerCreate,
        timer_config,
        attributes,
        &mut timer,
    );
}
