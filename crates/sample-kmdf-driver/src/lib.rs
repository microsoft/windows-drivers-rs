// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::cargo)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![deny(clippy::unnecessary_safety_doc)]
#![deny(clippy::multiple_unsafe_ops_per_block)]

extern crate alloc;

#[cfg(not(test))]
extern crate wdk_panic;

use alloc::{ffi::CString, slice, string::String};

use static_assertions::const_assert;
use wdk::println;
#[cfg(not(test))]
use wdk_alloc::WDKAllocator;
use wdk_macros::call_unsafe_wdf_function_binding;
use wdk_sys::{
    ntddk::DbgPrint,
    DRIVER_OBJECT,
    NTSTATUS,
    PCUNICODE_STRING,
    ULONG,
    UNICODE_STRING,
    WCHAR,
    WDFDEVICE,
    WDFDEVICE_INIT,
    WDFDRIVER,
    WDF_DRIVER_CONFIG,
    WDF_NO_HANDLE,
    WDF_NO_OBJECT_ATTRIBUTES,
};

#[cfg(not(test))]
#[global_allocator]
static GLOBAL_ALLOCATOR: WDKAllocator = WDKAllocator;

/// `DriverEntry` function required by WDF
///
/// # Panics
/// Can panic from unwraps of `CStrings` used internally
///
/// # Safety
/// Function is unsafe since it dereferences raw pointers passed to it from WDF
#[export_name = "DriverEntry"] // WDF expects a symbol with the name DriverEntry
pub unsafe extern "system" fn driver_entry(
    driver: &mut DRIVER_OBJECT,
    registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    // This is an example of directly using DbgPrint binding to print
    let string = CString::new("Hello World!\n").unwrap();

    // SAFETY: This is safe because `string` is a valid pointer to a null-terminated
    // string
    unsafe {
        DbgPrint(string.as_ptr());
    }

    driver.DriverUnload = Some(driver_exit);

    let mut driver_config = {
        // const_assert required since clippy::cast_possible_truncation must be silenced because of a false positive (since it currently doesn't handle checking compile-time constants): https://github.com/rust-lang/rust-clippy/issues/9613
        const WDF_DRIVER_CONFIG_SIZE: usize = core::mem::size_of::<WDF_DRIVER_CONFIG>();
        const_assert!(WDF_DRIVER_CONFIG_SIZE <= ULONG::MAX as usize);
        let wdf_driver_config_size: ULONG;
        // truncation not possible because of above const_assert
        #[allow(clippy::cast_possible_truncation)]
        {
            wdf_driver_config_size = WDF_DRIVER_CONFIG_SIZE as ULONG;
        }

        WDF_DRIVER_CONFIG {
            Size: wdf_driver_config_size,
            EvtDriverDeviceAdd: Some(evt_driver_device_add),
            ..WDF_DRIVER_CONFIG::default()
        }
    };

    let driver_attributes = WDF_NO_OBJECT_ATTRIBUTES;
    let driver_handle_output = WDF_NO_HANDLE.cast::<*mut wdk_sys::WDFDRIVER__>();

    let wdf_driver_create_ntstatus;
    // SAFETY: This is safe because:
    //         1. `driver` is provided by `DriverEntry` and is never null
    //         2. `registry_path` is provided by `DriverEntry` and is never null
    //         3. `driver_attributes` is allowed to be null
    //         4. `driver_config` is a valid pointer to a valid `WDF_DRIVER_CONFIG`
    //         5. `driver_handle_output` is expected to be null
    unsafe {
        wdf_driver_create_ntstatus = call_unsafe_wdf_function_binding!(
            WdfDriverCreate,
            driver as wdk_sys::PDRIVER_OBJECT,
            registry_path,
            driver_attributes,
            &mut driver_config,
            driver_handle_output,
        );
    }

    // Translate UTF16 string to rust string
    let registry_path: UNICODE_STRING =
        // SAFETY: This dereference is safe since `registry_path` is:
        //         * provided by `DriverEntry` and is never null
        //         * a valid pointer to a `UNICODE_STRING`
        unsafe { *registry_path };
    let number_of_slice_elements = {
        registry_path.Length as usize
            / core::mem::size_of_val(
                // SAFETY: This dereference is safe since `Buffer` is:
                //         * provided by `DriverEntry` and is never null
                //         * a valid pointer to `Buffer`'s type
                &unsafe { *registry_path.Buffer },
            )
    };

    let registry_path = String::from_utf16_lossy(
        // SAFETY: This is safe because:
        //         1. `registry_path.Buffer` is valid for reads for `number_of_slice_elements` *
        //            `core::mem::size_of::<WCHAR>()` bytes, and is guaranteed to be aligned and it
        //            must be properly aligned.
        //         2. `registry_path.Buffer` points to `number_of_slice_elements` consecutive
        //            properly initialized values of type `WCHAR`.
        //         3. Windows does not mutate the memory referenced by the returned slice for for
        //            its entire lifetime.
        //         4. The total size, `number_of_slice_elements` * `core::mem::size_of::<WCHAR>()`,
        //            of the slice must be no larger than `isize::MAX`. This is proven by the below
        //            `debug_assert!`.
        unsafe {
            debug_assert!(
                isize::try_from(number_of_slice_elements * core::mem::size_of::<WCHAR>()).is_ok()
            );
            slice::from_raw_parts(registry_path.Buffer, number_of_slice_elements)
        },
    );

    // It is much better to use the println macro that has an implementation in
    // wdk::print.rs to call DbgPrint. The println! implementation in
    // wdk::print.rs has the same features as the one in std (ex. format args
    // support).
    println!("KMDF Driver Entry Complete! Driver Registry Parameter Key: {registry_path}");

    wdf_driver_create_ntstatus
}

extern "C" fn evt_driver_device_add(
    _driver: WDFDRIVER,
    mut device_init: *mut WDFDEVICE_INIT,
) -> NTSTATUS {
    println!("EvtDriverDeviceAdd Entered!");

    let mut device_handle_output: WDFDEVICE = WDF_NO_HANDLE.cast();

    let ntstatus;
    // SAFETY: This is safe because:
    //       1. `device_init` is provided by `EvtDriverDeviceAdd` and is never null
    //       2. the argument receiving `WDF_NO_OBJECT_ATTRIBUTES` is allowed to be
    //          null
    //       3. `device_handle_output` is expected to be null
    unsafe {
        ntstatus = wdk_macros::call_unsafe_wdf_function_binding!(
            WdfDeviceCreate,
            &mut device_init,
            WDF_NO_OBJECT_ATTRIBUTES,
            &mut device_handle_output,
        );
    }

    println!("WdfDeviceCreate NTSTATUS: {ntstatus:#02x}");
    ntstatus
}

extern "C" fn driver_exit(_driver: *mut DRIVER_OBJECT) {
    println!("Goodbye World!");
    println!("Driver Exit Complete!");
}
