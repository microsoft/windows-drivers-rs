#![no_main]
#![deny(warnings)]
use wdk_sys::*;
fn acquire_lock(wdf_spin_lock: WDFSPINLOCK) {
    unsafe {
        #![allow(clippy::multiple_unsafe_ops_per_block)]
        {
            use wdk_sys::*;
            unsafe fn force_unsafe() {}
            force_unsafe();
            #[inline(always)]
            fn wdf_spin_lock_acquire_impl(SpinLock: WDFSPINLOCK) {
                let wdf_function: wdk_sys::PFN_WDFSPINLOCKACQUIRE = Some(
                    #[allow(unused_unsafe)]
                    #[allow(clippy::multiple_unsafe_ops_per_block)]
                    unsafe {
                        core::mem::transmute(
                            wdk_sys::WDF_FUNCTION_TABLE[wdk_sys::_WDFFUNCENUM::WdfSpinLockAcquireTableIndex
                                as usize],
                        )
                    },
                );
                if let Some(wdf_function) = wdf_function {
                    #[allow(unused_unsafe)]
                    #[allow(clippy::multiple_unsafe_ops_per_block)]
                    unsafe { (wdf_function)(wdk_sys::WdfDriverGlobals, SpinLock) }
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
            wdf_spin_lock_acquire_impl(wdf_spin_lock)
        };
    }
}
