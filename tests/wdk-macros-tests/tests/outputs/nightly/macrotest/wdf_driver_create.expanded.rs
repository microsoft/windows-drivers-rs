#![no_main]
#![deny(warnings)]
#[unsafe(export_name = "DriverEntry")]
pub extern "system" fn driver_entry(
    driver: wdk_sys::PDRIVER_OBJECT,
    registry_path: wdk_sys::PCUNICODE_STRING,
) -> wdk_sys::NTSTATUS {
    let mut driver_config = wdk_sys::WDF_DRIVER_CONFIG {
        Size: core::mem::size_of::<wdk_sys::WDF_DRIVER_CONFIG>() as wdk_sys::ULONG,
        ..Default::default()
    };
    let driver_handle_output = wdk_sys::WDF_NO_HANDLE as *mut wdk_sys::WDFDRIVER;
    unsafe {
        {
            mod private__ {
                use wdk_sys::*;
                #[must_use]
                #[inline(always)]
                pub unsafe fn wdf_driver_create_impl(
                    driver_object__: PDRIVER_OBJECT,
                    registry_path__: PCUNICODE_STRING,
                    driver_attributes__: PWDF_OBJECT_ATTRIBUTES,
                    driver_config__: PWDF_DRIVER_CONFIG,
                    driver__: *mut WDFDRIVER,
                ) -> NTSTATUS {
                    let wdf_function: wdk_sys::PFN_WDFDRIVERCREATE = Some(unsafe {
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
                            wdf_function_table[wdk_sys::_WDFFUNCENUM::WdfDriverCreateTableIndex
                                as usize],
                        )
                    });
                    if let Some(wdf_function) = wdf_function {
                        unsafe {
                            (wdf_function)(
                                wdk_sys::WdfDriverGlobals,
                                driver_object__,
                                registry_path__,
                                driver_attributes__,
                                driver_config__,
                                driver__,
                            )
                        }
                    } else {
                        {
                            ::core::panicking::panic_fmt(
                                format_args!(
                                    "internal error: entered unreachable code: {0}",
                                    format_args!("Option should never be None")
                                ),
                            );
                        };
                    }
                }
            }
            private__::wdf_driver_create_impl(
                driver,
                registry_path,
                wdk_sys::WDF_NO_OBJECT_ATTRIBUTES,
                &mut driver_config,
                driver_handle_output,
            )
        }
    }
}
