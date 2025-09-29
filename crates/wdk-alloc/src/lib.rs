// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Allocator implementation to use with `#[global_allocator]` to allow use of
//! [`core::alloc`].
//!
//! # Example
//! ```rust, no_run
//! #[cfg(all(
//!     any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"),
//!     not(test)
//! ))]
//! use wdk_alloc::WdkAllocator;
//!
//! #[cfg(all(
//!     any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"),
//!     not(test)
//! ))]
//! #[global_allocator]
//! static GLOBAL_ALLOCATOR: WdkAllocator = WdkAllocator;
//! ```

#![no_std]

#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
pub use kernel_mode::*;

#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
mod kernel_mode {

    use core::alloc::{GlobalAlloc, Layout};

    use wdk_sys::{
        POOL_FLAG_NON_PAGED,
        SIZE_T,
        ULONG,
        ntddk::{ExAllocatePool2, ExFreePool},
    };

    /// Allocator implementation to use with `#[global_allocator]` to allow use
    /// of [`core::alloc`].
    ///
    /// # Safety
    /// This allocator is only safe to use for allocations happening at `IRQL`
    /// <= `DISPATCH_LEVEL`
    pub struct WdkAllocator;

    // The value of memory tags are stored in little-endian order, so it is
    // convenient to reverse the order for readability in tooling (ie. Windbg)
    const RUST_TAG: ULONG = u32::from_ne_bytes(*b"rust");

    // SAFETY: This is safe because the Wdk allocator:
    //         1. can never unwind since it can never panic
    //         2. has implementations of alloc and dealloc that maintain layout
    //            constraints (FIXME: Alignment of the layout is currently not
    //            supported)
    unsafe impl GlobalAlloc for WdkAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let ptr =
                // SAFETY: `ExAllocatePool2` is safe to call from any `IRQL` <= `DISPATCH_LEVEL` since its allocating from `POOL_FLAG_NON_PAGED`
                unsafe {
                    ExAllocatePool2(POOL_FLAG_NON_PAGED, layout.size() as SIZE_T, RUST_TAG)
                };
            if ptr.is_null() {
                return core::ptr::null_mut();
            }
            ptr.cast()
        }

        unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
            // SAFETY: `ExFreePool` is safe to call from any `IRQL` <= `DISPATCH_LEVEL`
            // since its freeing memory allocated from `POOL_FLAG_NON_PAGED` in `alloc`
            unsafe {
                ExFreePool(ptr.cast());
            }
        }
    }
}
