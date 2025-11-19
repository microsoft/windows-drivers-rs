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

#[unsafe(export_name = "DriverEntry")] // WDF expects a symbol with the name DriverEntry
pub unsafe extern "system" fn driver_entry(
   _driver: PDRIVER_OBJECT,
   _registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
   0
}
