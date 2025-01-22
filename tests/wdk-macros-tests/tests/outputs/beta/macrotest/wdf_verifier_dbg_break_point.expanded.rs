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
                        let wdf_function_table = wdk_sys::WdfFunctions;
                        let wdf_function_count = wdk_sys::wdf::__private::get_wdf_function_count();
                        if true {
                            if !isize::try_from(
                                    wdf_function_count
                                        * core::mem::size_of::<wdk_sys::WDFFUNC>(),
                                )
                                .is_ok()
                            {
                                ::core::panicking::panic(
                                    "assertion failed: isize::try_from(wdf_function_count *\n            core::mem::size_of::<wdk_sys::WDFFUNC>()).is_ok()",
                                )
                            }
                        }
                        let wdf_function_table = core::slice::from_raw_parts(
                            wdf_function_table,
                            wdf_function_count,
                        );
                        core::mem::transmute(
                            wdf_function_table[wdk_sys::_WDFFUNCENUM::WdfVerifierDbgBreakPointTableIndex
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
