// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
#![no_main]
#![deny(warnings)]

fn process_wdf_request(request: wdk_sys::WDFREQUEST) {
    let minimum_required_buffer_size = 32;
    let mut output_buffer_ptr = std::ptr::null_mut();
    let _nt_status = unsafe {
        wdk_sys::call_unsafe_wdf_function_binding!(
            WdfRequestRetrieveOutputBuffer,
            request,
            minimum_required_buffer_size,
            &mut output_buffer_ptr,
            std::ptr::null_mut()
        )
    };
}
