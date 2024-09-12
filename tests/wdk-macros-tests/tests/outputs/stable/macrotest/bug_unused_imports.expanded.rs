#![no_main]
#![deny(warnings)]
//! This is a regression test for a bug where the arguments to the
//! [`call_unsafe_wdf_function_binding`] macro would need to be brought into
//! scope, but rust-analyzer would treat them as unused imports. This resulted
//! in the following compilation error:
#[rustfmt::skip]
/// ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈
/// error: unused import: `WDF_NO_OBJECT_ATTRIBUTES`
///   --> C:/windows-drivers-rs/tests/wdk-macros-tests/tests/outputs/nightly/macrotest/bug_unused_imports.rs:31:49
///    |
/// 31 | use wdk_sys::{call_unsafe_wdf_function_binding, WDF_NO_OBJECT_ATTRIBUTES, NTSTATUS, PDRIVER_OBJECT, ULONG, PCUNICODE_STRING, WDF_DRIVER_C...
///    |                                                 ^^^^^^^^^^^^^^^^^^^^^^^^
///    |
/// note: the lint level is defined here
///   --> C:/windows-drivers-rs/tests/wdk-macros-tests/tests/wdk-macros-tests/tests/outputs/nightly/macrotest/bug_unused_imports.rs:5:9
///    |
/// 5  | #![deny(warnings)]
///    |         ^^^^^^^^
///    = note: `#[deny(unused_imports)]` implied by `#[deny(warnings)]`
/// ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈
use wdk_sys::{
    call_unsafe_wdf_function_binding, NTSTATUS, PCUNICODE_STRING, PDRIVER_OBJECT, ULONG,
    WDFDRIVER, WDF_DRIVER_CONFIG, WDF_NO_HANDLE, WDF_NO_OBJECT_ATTRIBUTES,
};
#[export_name = "DriverEntry"]
pub extern "system" fn driver_entry(
    driver: PDRIVER_OBJECT,
    registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    let mut driver_config = WDF_DRIVER_CONFIG {
        Size: core::mem::size_of::<WDF_DRIVER_CONFIG>() as ULONG,
        ..Default::default()
    };
    let driver_handle_output = WDF_NO_HANDLE as *mut WDFDRIVER;
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
                        core::mem::transmute(
                            wdk_sys::WDF_FUNCTION_TABLE[wdk_sys::_WDFFUNCENUM::WdfDriverCreateTableIndex
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
                                    format_args!("Option should never be None"),
                                ),
                            );
                        };
                    }
                }
            }
            private__::wdf_driver_create_impl(
                driver,
                registry_path,
                WDF_NO_OBJECT_ATTRIBUTES,
                &mut driver_config,
                driver_handle_output,
            )
        }
    }
}
