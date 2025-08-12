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

use alloc::{boxed::Box, ffi::CString, slice, string::String, vec};

use wdk::println;
use wdk_alloc::WdkAllocator;
use wdk_sys::{ntddk::DbgPrint, DRIVER_OBJECT, NTSTATUS, PCUNICODE_STRING, STATUS_SUCCESS};

#[global_allocator]
static GLOBAL_ALLOCATOR: WdkAllocator = WdkAllocator;

// Example of using custom-aligned allocator.
// 256-byte boundary is sufficient to trigger realignment in global allocator.
#[derive(Debug)]
#[repr(C, align(256))]
struct BigAligned(u32);

/// `driver_entry` function required by WDM
///
/// # Panics
/// Can panic from unwraps of `CStrings` used internally
///
/// # Safety
/// Function is unsafe since it dereferences raw pointers passed to it from WDM
#[export_name = "DriverEntry"]
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

    // Translate UTF16 string to rust string
    let registry_path = String::from_utf16_lossy(unsafe {
        slice::from_raw_parts(
            (*registry_path).Buffer,
            (*registry_path).Length as usize / core::mem::size_of_val(&(*(*registry_path).Buffer)),
        )
    });
    {
        // Example of using WDK allocator.
        // Allocations will be properly aligned on their boundaries!
        // Allocate a single instance.
        let ah = Box::new(BigAligned(1234));
        // Check if the address is ended with "00" in hexadecimal!
        println!("ah is allocated at {:p}, value={ah:?}", &raw const *ah);
        // Verify its alignment.
        assert_eq!(
            (&raw const *ah) as usize & (align_of::<BigAligned>() - 1),
            0
        );
        // Allocate a vector that occupies more than a page.
        let vh = vec![
            BigAligned(1234),
            BigAligned(5678),
            BigAligned(9012),
            BigAligned(3456),
            BigAligned(7890),
        ];
        // Check if the address is ended with "00" in hexadecimal!
        println!("vh is allocated at {:p}, value={vh:?}", vh.as_ptr());
        // Verify their alignments.
        for x in &vh {
            assert_eq!(
                (core::ptr::from_ref::<BigAligned>(x) as usize)
                    & (align_of::<BigAligned>() - 1),
                0
            );
        }
    }
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
