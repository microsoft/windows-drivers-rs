#![no_main]
use wdk_sys::*;
#[export_name = "DriverEntry"]
pub extern "system" fn driver_entry(
    driver: &mut DRIVER_OBJECT,
    registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    let mut driver_config = WDF_DRIVER_CONFIG {
        Size: core::mem::size_of::<WDF_DRIVER_CONFIG>() as ULONG,
        ..WDF_DRIVER_CONFIG::default()
    };
    let driver_handle_output = WDF_NO_HANDLE as *mut WDFDRIVER;
    unsafe {
        {
            let wdf_function: wdk_sys::PFN_WDFDRIVERCREATE = Some(
                core::mem::transmute(
                    wdk_sys::WDF_FUNCTION_TABLE[wdk_sys::_WDFFUNCENUM::WdfDriverCreateTableIndex
                        as usize],
                ),
            );
            if let Some(wdf_function) = wdf_function {
                (wdf_function)(
                    wdk_sys::WdfDriverGlobals,
                    driver as PDRIVER_OBJECT,
                    registry_path,
                    WDF_NO_OBJECT_ATTRIBUTES,
                    &mut driver_config,
                    driver_handle_output,
                )
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
}
