#![no_main]
#![deny(warnings)]
#[allow(unused_imports)]
use wdk_sys::*;
fn foo() {
    unsafe {
        {
            #[inline(always)]
            unsafe fn wdf_verifier_dbg_break_point_impl() {
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
            wdf_verifier_dbg_break_point_impl()
        }
    }
}
