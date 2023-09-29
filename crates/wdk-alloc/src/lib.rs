// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

#![no_std]
#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::cargo)]

use core::alloc::{GlobalAlloc, Layout};

use wdk_sys::{
    ntddk::{ExAllocatePool2, ExFreePool},
    POOL_FLAG_NON_PAGED,
    SIZE_T,
    ULONG,
};
pub struct WDKAllocator;

// The value of memory tags are stored in little-endian order, so it is
// convenient to reverse the order for readability in tooling (ie. Windbg)
const RUST_TAG: ULONG = u32::from_ne_bytes(*b"rust");

unsafe impl GlobalAlloc for WDKAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = ExAllocatePool2(POOL_FLAG_NON_PAGED, layout.size() as SIZE_T, RUST_TAG);
        assert!(!ptr.is_null(), "wdk-alloc failed to allocate memory.");
        ptr.cast()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        ExFreePool(ptr.cast());
    }
}
