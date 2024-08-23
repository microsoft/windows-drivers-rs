// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
#![no_main]
#![deny(warnings)]

fn foo() {
    unsafe { wdk_sys::call_unsafe_wdf_function_binding!(WdfVerifierDbgBreakPoint) }
}
