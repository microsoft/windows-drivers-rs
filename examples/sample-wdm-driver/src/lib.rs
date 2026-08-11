// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! # Sample WDM Driver
//!
//! This is a sample WDM driver that demonstrates how to use the crates in
//! windows-driver-rs to create a skeleton of a WDM driver.

#![no_std]
extern crate alloc;

#[cfg(not(test))]
extern crate wdk_panic;

use alloc::{ffi::CString, slice, string::String};
use core::mem::size_of;

use wdk::{
    println,
    sync::{PushLock, RwLock, RwSpinLock},
};
#[cfg(not(test))]
use wdk_alloc::WdkAllocator;
use wdk_sys::{DRIVER_OBJECT, NTSTATUS, PCUNICODE_STRING, STATUS_SUCCESS, ntddk::DbgPrint};

#[cfg(not(test))]
#[global_allocator]
static GLOBAL_ALLOCATOR: WdkAllocator = WdkAllocator;

/// `driver_entry` function required by WDM
///
/// # Panics
/// Can panic from unwraps of `CStrings` used internally
///
/// # Safety
/// Function is unsafe since it dereferences raw pointers passed to it from WDM
// SAFETY: "DriverEntry" is the required symbol name for Windows driver entry points.
// No other function in this compilation unit exports this name, preventing symbol conflicts.
#[unsafe(export_name = "DriverEntry")]
pub unsafe extern "system" fn driver_entry(
    driver: &mut DRIVER_OBJECT,
    registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    // This is an example of directly using DbgPrint binding to print
    let string = CString::new("Hello World!\n").unwrap();

    // SAFETY: This is safe because `string` is a valid pointer to a null-terminated
    // string (`CString` guarantees null-termination)
    unsafe {
        DbgPrint(c"%s".as_ptr().cast(), string.as_ptr());
    }

    driver.DriverUnload = Some(driver_exit);

    let rw_lock = match RwLock::try_new(0_u32) {
        Ok(rw_lock) => rw_lock,
        Err(status) => return status,
    };
    {
        let mut sample_value = rw_lock.write();
        *sample_value = 42;
    }
    let sample_value = *rw_lock.read();
    println!("RwLock sample value: {sample_value}");

    let push_lock = PushLock::new(0_u32);
    {
        let mut sample_value = push_lock.write();
        *sample_value = 7;
    }
    let sample_value = *push_lock.read();
    println!("PushLock sample value: {sample_value}");

    let rw_spin_lock = RwSpinLock::new(0_u32);
    {
        let mut sample_value = rw_spin_lock.write();
        *sample_value = 3;
    }
    let sample_value = *rw_spin_lock.read();
    println!("RwSpinLock sample value: {sample_value}");

    // Translate UTF16 string to rust string
    // SAFETY: WDM provides `registry_path` as a valid `UNICODE_STRING` pointer
    // for the duration of `DriverEntry`.
    let registry_path = unsafe { &*registry_path };
    let registry_path_len = registry_path.Length as usize / size_of::<u16>();
    let registry_path_buffer = if registry_path_len == 0 {
        &[]
    } else {
        // SAFETY: `registry_path.Buffer` points to `Length` bytes of UTF-16
        // code units for the duration of `DriverEntry`.
        unsafe { slice::from_raw_parts(registry_path.Buffer, registry_path_len) }
    };
    let registry_path = String::from_utf16_lossy(registry_path_buffer);

    // It is much better to use the println macro that has an implementation in
    // wdk::print.rs to call DbgPrint. The println! implementation in
    // wdk::print.rs has the same features as the one in std (ex. format args
    // support).
    println!("WDM Driver Entry Complete! Driver Registry Parameter Key: {registry_path}");

    STATUS_SUCCESS
}

extern "C" fn driver_exit(_driver: *mut DRIVER_OBJECT) {
    println!("Goodbye World!");
    println!("Driver Exit Complete!");
}
