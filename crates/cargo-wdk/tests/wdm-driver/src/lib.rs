#![no_std]

use wdk_sys::{
   PDRIVER_OBJECT,
   NTSTATUS,
   PCUNICODE_STRING,
};

#[cfg(not(test))]
extern crate wdk_panic;

#[cfg(not(test))]
use wdk_alloc::WdkAllocator;

#[cfg(not(test))]
#[global_allocator]
static GLOBAL_ALLOCATOR: WdkAllocator = WdkAllocator;

extern crate alloc;
use alloc::ffi::CString;
use core::hint::black_box;

#[unsafe(export_name = "DriverEntry")] // WDF expects a symbol with the name DriverEntry
pub unsafe extern "system" fn driver_entry(
   _driver: PDRIVER_OBJECT,
   _registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
   // Prevent optimization: ensure allocator is linked (workaround for ARM64 linker issues)
   black_box(CString::new("Hello World!\n").unwrap());
   0
}
