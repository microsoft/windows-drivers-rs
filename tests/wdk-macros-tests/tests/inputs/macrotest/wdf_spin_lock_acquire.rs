// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
#![no_main]
#![deny(warnings)]

fn acquire_lock(wdf_spin_lock: wdk_sys::WDFSPINLOCK) {
    // This demonstrates that the macro won't trigger a must_use warning on WDF APIs that don't return a value
    unsafe {
        wdk_sys::call_unsafe_wdf_function_binding!(WdfSpinLockAcquire, wdf_spin_lock);
    }
}
