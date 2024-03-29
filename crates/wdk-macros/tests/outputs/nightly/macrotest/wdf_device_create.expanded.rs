#![no_main]
#![deny(warnings)]
use wdk_sys::*;
extern "C" fn evt_driver_device_add(
    _driver: WDFDRIVER,
    mut device_init: *mut WDFDEVICE_INIT,
) -> NTSTATUS {
    let mut device_handle_output: WDFDEVICE = WDF_NO_HANDLE.cast();
    unsafe {
        {
            #[must_use]
            #[inline(always)]
            unsafe fn wdf_device_create_impl(
                DeviceInit: *mut wdk_sys::PWDFDEVICE_INIT,
                DeviceAttributes: wdk_sys::PWDF_OBJECT_ATTRIBUTES,
                Device: *mut wdk_sys::WDFDEVICE,
            ) -> wdk_sys::NTSTATUS {
                let wdf_function: wdk_sys::PFN_WDFDEVICECREATE = Some(unsafe {
                    core::mem::transmute(
                        wdk_sys::WDF_FUNCTION_TABLE[wdk_sys::_WDFFUNCENUM::WdfDeviceCreateTableIndex
                            as usize],
                    )
                });
                if let Some(wdf_function) = wdf_function {
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
            wdf_device_create_impl(
                &mut device_init,
                WDF_NO_OBJECT_ATTRIBUTES,
                &mut device_handle_output,
            )
        }
    }
}
