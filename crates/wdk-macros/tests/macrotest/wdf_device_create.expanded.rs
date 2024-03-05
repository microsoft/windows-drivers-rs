#![no_main]
use wdk_sys::*;
extern "C" fn evt_driver_device_add(
    _driver: WDFDRIVER,
    mut device_init: *mut WDFDEVICE_INIT,
) -> NTSTATUS {
    let mut device_handle_output: WDFDEVICE = WDF_NO_HANDLE.cast();
    unsafe {
        {
            use wdk_sys::*;
            unsafe fn force_unsafe() {}
            force_unsafe();
            #[must_use]
            #[inline(always)]
            fn unsafe_imp(
                DeviceInit: *mut PWDFDEVICE_INIT,
                DeviceAttributes: PWDF_OBJECT_ATTRIBUTES,
                Device: *mut WDFDEVICE,
            ) -> wdk_sys::NTSTATUS {
                let wdf_function: wdk_sys::PFN_WDFDEVICECREATE = Some(
                    #[allow(unused_unsafe)]
                    #[allow(clippy::multiple_unsafe_ops_per_block)]
                    unsafe {
                        core::mem::transmute(
                            wdk_sys::WDF_FUNCTION_TABLE[wdk_sys::_WDFFUNCENUM::WdfDeviceCreateTableIndex
                                as usize],
                        )
                    },
                );
                if let Some(wdf_function) = wdf_function {
                    #[allow(unused_unsafe)]
                    #[allow(clippy::multiple_unsafe_ops_per_block)]
                    unsafe {
                        (wdf_function)(
                            wdk_sys::WdfDriverGlobals,
                            DeviceInit,
                            DeviceAttributes,
                            Device,
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
            unsafe_imp(
                &mut device_init,
                WDF_NO_OBJECT_ATTRIBUTES,
                &mut device_handle_output,
            )
        }
    }
}
