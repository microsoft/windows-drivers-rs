#![no_main]
#![deny(warnings)]
fn process_wdf_request(request: wdk_sys::WDFREQUEST) {
    let minimum_required_buffer_size = 32;
    let mut output_buffer_ptr = std::ptr::null_mut();
    let _nt_status = unsafe {
        {
            use wdk_sys::*;
            #[must_use]
            #[inline(always)]
            #[allow(non_snake_case)]
            unsafe fn wdf_request_retrieve_output_buffer_impl(
                Request: WDFREQUEST,
                MinimumRequiredSize: usize,
                Buffer: *mut PVOID,
                Length: *mut usize,
            ) -> NTSTATUS {
                let wdf_function: wdk_sys::PFN_WDFREQUESTRETRIEVEOUTPUTBUFFER = Some(unsafe {
                    core::mem::transmute(
                        wdk_sys::WDF_FUNCTION_TABLE[wdk_sys::_WDFFUNCENUM::WdfRequestRetrieveOutputBufferTableIndex
                            as usize],
                    )
                });
                if let Some(wdf_function) = wdf_function {
                    unsafe {
                        (wdf_function)(
                            wdk_sys::WdfDriverGlobals,
                            Request,
                            MinimumRequiredSize,
                            Buffer,
                            Length,
                        )
                    }
                } else {
                    {
                        ::core::panicking::panic_fmt(
                            format_args!(
                                "internal error: entered unreachable code: {0}",
                                format_args!("Option should never be None"),
                            ),
                        );
                    };
                }
            }
            wdf_request_retrieve_output_buffer_impl(
                request,
                minimum_required_buffer_size,
                &mut output_buffer_ptr,
                std::ptr::null_mut(),
            )
        }
    };
}
