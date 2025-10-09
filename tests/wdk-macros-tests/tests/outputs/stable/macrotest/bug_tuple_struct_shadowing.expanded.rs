#![no_main]
#![deny(warnings)]
/// This is a regression test for a bug where the
/// [`call_unsafe_wdf_function_binding`] macro would generate code that prevented
/// anything in scope from having the same name as one of the c function's
/// parameter names. This resulted in the following compilation error:
///
#[rustfmt::skip]
/// ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈
/// error[E0530]: function parameters cannot shadow tuple structs
///   --> C:/windows-drivers-rs/tests/wdk-macros-tests/tests/outputs/nightly/macrotest/bug_tuple_struct_shadowing.rs:34:9
///    |
/// 30 |   pub struct DeviceInit(wdk_sys::PWDFDEVICE_INIT);
///    |   ------------------------------------------------ the tuple struct `DeviceInit` is defined here
/// ...
/// 34 | /         call_unsafe_wdf_function_binding!(
/// 35 | |             WdfDeviceInitSetPnpPowerEventCallbacks,
/// 36 | |             device_init.0,
/// 37 | |             pnp_power_callbacks
/// 38 | |         )
///    | |_________^ cannot be named the same as a tuple struct
///    |
///    = note: this error originates in the macro `$crate::__proc_macros::call_unsafe_wdf_function_binding` which comes from the expansion of the macro `call_unsafe_wdf_function_binding` (in Nightly builds, run with -Z macro-backtrace for more info)
/// ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈
use wdk_sys::call_unsafe_wdf_function_binding;
#[repr(transparent)]
pub struct DeviceInit(wdk_sys::PWDFDEVICE_INIT);
fn foo(
    device_init: DeviceInit,
    pnp_power_callbacks: wdk_sys::PWDF_PNPPOWER_EVENT_CALLBACKS,
) {
    unsafe {
        {
            mod private__ {
                use wdk_sys::*;
                #[inline(always)]
                pub unsafe fn wdf_device_init_set_pnp_power_event_callbacks_impl(
                    device_init__: PWDFDEVICE_INIT,
                    pnp_power_event_callbacks__: PWDF_PNPPOWER_EVENT_CALLBACKS,
                ) {
                    let wdf_function: wdk_sys::PFN_WDFDEVICEINITSETPNPPOWEREVENTCALLBACKS = Some(unsafe {
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
                            wdf_function_table[wdk_sys::_WDFFUNCENUM::WdfDeviceInitSetPnpPowerEventCallbacksTableIndex
                                as usize],
                        )
                    });
                    if let Some(wdf_function) = wdf_function {
                        unsafe {
                            (wdf_function)(
                                wdk_sys::WdfDriverGlobals,
                                device_init__,
                                pnp_power_event_callbacks__,
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
            private__::wdf_device_init_set_pnp_power_event_callbacks_impl(
                device_init.0,
                pnp_power_callbacks,
            )
        }
    }
}
