#![no_main]
#![deny(warnings)]
fn process_wdf_request(request: wdk_sys::WDFREQUEST) {
    let minimum_required_buffer_size = 32;
    let mut output_buffer_ptr = std::ptr::null_mut();
    let _nt_status = unsafe {
        {
            mod private__ {
                use wdk_sys::*;
                #[must_use]
                #[inline(always)]
                pub unsafe fn wdf_request_retrieve_output_buffer_impl(
                    request__: WDFREQUEST,
                    minimum_required_size__: usize,
                    buffer__: *mut PVOID,
                    length__: *mut usize,
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
                                request__,
                                minimum_required_size__,
                                buffer__,
                                length__,
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
            }
            private__::wdf_request_retrieve_output_buffer_impl(
                request,
                minimum_required_buffer_size,
                &mut output_buffer_ptr,
                std::ptr::null_mut(),
            )
        }
    };
}
