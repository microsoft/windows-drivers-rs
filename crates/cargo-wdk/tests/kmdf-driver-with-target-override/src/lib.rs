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
use wdk_sys::ULONG;

// The value of memory tags are stored in little-endian order, so it is
// convenient to reverse the order for readability in tooling (i.e. Windbg)
#[cfg(not(test))]
const POOL_TAG: ULONG = ULONG::from_ne_bytes(*b"rust");

#[cfg(not(test))]
#[global_allocator]
static GLOBAL_ALLOCATOR: WdkAllocator<POOL_TAG> = WdkAllocator;

#[unsafe(export_name = "DriverEntry")] // WDF expects a symbol with the name DriverEntry
pub unsafe extern "system" fn driver_entry(
   _driver: PDRIVER_OBJECT,
   _registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
   0
}
