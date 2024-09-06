#![no_main]
#![deny(warnings)]
extern "C" fn evt_driver_device_add(
    _driver: wdk_sys::WDFDRIVER,
    mut device_init: *mut wdk_sys::WDFDEVICE_INIT,
) -> wdk_sys::NTSTATUS {
    let mut device_handle_output: wdk_sys::WDFDEVICE = wdk_sys::WDF_NO_HANDLE.cast();
    unsafe {
        {
            mod private__ {
                use wdk_sys::*;
                #[must_use]
                #[inline(always)]
                pub unsafe fn wdf_device_create_impl(
                    device_init__: *mut PWDFDEVICE_INIT,
                    device_attributes__: PWDF_OBJECT_ATTRIBUTES,
                    device__: *mut WDFDEVICE,
                ) -> NTSTATUS {
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
                                device_init__,
                                device_attributes__,
                                device__,
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
            private__::wdf_device_create_impl(
                &mut device_init,
                wdk_sys::WDF_NO_OBJECT_ATTRIBUTES,
                &mut device_handle_output,
            )
        }
    }
}
