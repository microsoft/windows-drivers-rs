#![no_main]
#![deny(warnings)]
fn acquire_lock(wdf_spin_lock: wdk_sys::WDFSPINLOCK) {
    unsafe {
        {
            mod private__ {
                use wdk_sys::*;
                #[inline(always)]
                pub unsafe fn wdf_spin_lock_acquire_impl(spin_lock__: WDFSPINLOCK) {
                    let wdf_function: wdk_sys::PFN_WDFSPINLOCKACQUIRE = Some(unsafe {
                        core::mem::transmute(
                            wdk_sys::WDF_FUNCTION_TABLE[wdk_sys::_WDFFUNCENUM::WdfSpinLockAcquireTableIndex
                                as usize],
                        )
                    });
                    if let Some(wdf_function) = wdf_function {
                        unsafe { (wdf_function)(wdk_sys::WdfDriverGlobals, spin_lock__) }
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
            private__::wdf_spin_lock_acquire_impl(wdf_spin_lock)
        };
    }
}
