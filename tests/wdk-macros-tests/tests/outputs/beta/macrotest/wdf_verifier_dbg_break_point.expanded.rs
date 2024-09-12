#![no_main]
#![deny(warnings)]
fn foo() {
    unsafe {
        {
            mod private__ {
                use wdk_sys::*;
                #[inline(always)]
                pub unsafe fn wdf_verifier_dbg_break_point_impl() {
                    let wdf_function: wdk_sys::PFN_WDFVERIFIERDBGBREAKPOINT = Some(unsafe {
                        core::mem::transmute(
                            wdk_sys::WDF_FUNCTION_TABLE[wdk_sys::_WDFFUNCENUM::WdfVerifierDbgBreakPointTableIndex
                                as usize],
                        )
                    });
                    if let Some(wdf_function) = wdf_function {
                        unsafe { (wdf_function)(wdk_sys::WdfDriverGlobals) }
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
            private__::wdf_verifier_dbg_break_point_impl()
        }
    }
}
