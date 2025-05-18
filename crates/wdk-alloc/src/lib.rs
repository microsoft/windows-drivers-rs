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
        ntddk::{ExAllocatePool2, ExFreePoolWithTag}, MEMORY_ALLOCATION_ALIGNMENT, PAGE_SIZE, POOL_FLAG_NON_PAGED, PVOID, SIZE_T, ULONG
    };

    /// Allocator implementation to use with `#[global_allocator]` to allow use
    /// of [`core::alloc`].
    ///
    /// # Safety
    /// This allocator is only safe to use for allocations happening at `IRQL`
    /// <= `DISPATCH_LEVEL`
    pub struct WdkAllocator;

    // The value of memory tags are stored in little-endian order, so it is
    // convenient to reverse the order for readability in tooling (ie. Windbg).
    const RUST_TAG: ULONG = u32::from_ne_bytes(*b"rust");

    #[inline] fn require_realignment(layout: Layout) -> bool {
        // `ExAllocatePool2` uses an alignment of `PAGE_SIZE` or minimum pool
        // alignment depending on the requested size. See documentation:
        // https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-exallocatepool2#remarks
        let chosen_alignment = if layout.size() >= PAGE_SIZE as usize { PAGE_SIZE } else { MEMORY_ALLOCATION_ALIGNMENT } as usize;
        // Realignment needed only if requested alignment cannot
        // be satisfied by `ExAllocatePool2`'s chosen alignment
        layout.align() > chosen_alignment
    }

    // SAFETY: This is safe because the Wdk allocator:
    //         1. can never unwind since it can never panic
    //         2. has implementations of alloc and dealloc that maintain layout
    //            constraints (including size and alignment)
    unsafe impl GlobalAlloc for WdkAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let ptr =
                // SAFETY: `ExAllocatePool2` is safe to call from any `IRQL` <= `DISPATCH_LEVEL` since its allocating from `POOL_FLAG_NON_PAGED`
                if require_realignment(layout) {
                    // If the requested alignment is too large, we'll have to waste some space as large as the alignment.
                    let size = layout.size() + layout.align();
                    let mask = !(layout.align() - 1);
                    let p = unsafe {
                        ExAllocatePool2(POOL_FLAG_NON_PAGED, size as SIZE_T, RUST_TAG)
                    };
                    if !p.is_null() {
                        // Align the pointer up to the first address that meets alignment requirement.
                        let q = ((p as usize & mask) + layout.align()) as *mut PVOID;
                        // Store the original pointer right before the pointer to return,
                        // so that ExFreePoolWithTag can receive the correct pointer.
                        // This is also how `_aligned_malloc` is implemented in msvcrt.dll
                        unsafe {
                            q.sub(1).write(p);
                        }
                        q.cast()
                    } else {
                        p
                    }
                } else {
                    // Because we know the alignment while in both alloc and dealloc,
                    // we don't need to waste space if the default alignment is small enough.
                    unsafe {
                        ExAllocatePool2(POOL_FLAG_NON_PAGED, layout.size() as SIZE_T, RUST_TAG)
                    }
                };
            if ptr.is_null() {
                return core::ptr::null_mut();
            }
            ptr.cast()
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            // SAFETY: `ExFreePool` is safe to call from any `IRQL` <= `DISPATCH_LEVEL`
            // since its freeing memory allocated from `POOL_FLAG_NON_PAGED` in `alloc`
            let p = if require_realignment(layout) {
                // The alignment is too large, so the original pointer is stored right before
                // the ptr. This is also how `_aligned_free` is implemented in msvcrt.dll
                let q: *mut PVOID = ptr.cast();
                unsafe {
                    q.sub(1).read()
                }
            } else {
                // The alignment is normal, so the ptr is already the original pointer.
                ptr.cast()
            };
            unsafe {
                ExFreePoolWithTag(p, RUST_TAG);
            }
        }
    }
}
