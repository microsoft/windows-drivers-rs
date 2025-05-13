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

    use core::{
        alloc::{GlobalAlloc, Layout},
        sync::atomic::{AtomicU32, Ordering},
    };

    use wdk_sys::{
        ntddk::{ExAllocatePool2, ExFreePoolWithTag},
        POOL_FLAG_NON_PAGED,
        SIZE_T,
    };

    /// Allocator implementation to use with `#[global_allocator]` to allow use
    /// of [`core::alloc`].
    ///
    /// # Safety
    /// This allocator is only safe to use for allocations happening at `IRQL`
    /// <= `DISPATCH_LEVEL`
    pub struct WdkAllocator;

    /// The value of memory tags are stored in little-endian order, so it is
    /// convenient to reverse the order for readability in tooling (ie. Windbg).
    /// The default tag value is `rust`. You may use `store` method to mutate its value.
    /// However, it's strongly recommended that you mutate the tag for only once.
    pub static GLOBAL_POOL_TAG: AtomicU32 = AtomicU32::new(u32::from_ne_bytes(*b"rust"));
    
    // The minimum alignment of pointer returned by ExAllocatePool2.
    const POOL_ALIGNMENT: usize = size_of::<*mut u8>() * 2;

    // SAFETY: This is safe because the Wdk allocator:
    //         1. can never unwind since it can never panic
    //         2. has implementations of alloc and dealloc that maintain layout
    //            constraints (including size and alignment)
    unsafe impl GlobalAlloc for WdkAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let ptr =
                // SAFETY: `ExAllocatePool2` is safe to call from any `IRQL` <= `DISPATCH_LEVEL` since its allocating from `POOL_FLAG_NON_PAGED`
                unsafe {
                    if layout.align() > POOL_ALIGNMENT {
                        // If the requested alignment is too large, we'll have to waste some space as large as the alignment.
                        let size = layout.size() + layout.align();
                        let mask = !(layout.align() - 1);
                        let p = ExAllocatePool2(POOL_FLAG_NON_PAGED, size as SIZE_T, GLOBAL_POOL_TAG.load(Ordering::Relaxed));
                        if !p.is_null() {
                            let q = ((p as usize & mask) + layout.align()) as *mut *mut u8;
                            // Store the original pointer right before the pointer to return,
                            // so that ExFreePoolWithTag can receive the correct pointer.
                            // This is also how `_aligned_malloc` is implemented in msvcrt.dll
                            q.sub(1).write(p.cast());
                            q.cast()
                        } else {
                            p
                        }
                    } else {
                        // Because we know the alignment while in both alloc and dealloc,
                        // we don't need to waste space if the default alignment is small enough.
                        ExAllocatePool2(POOL_FLAG_NON_PAGED, layout.size() as SIZE_T, GLOBAL_POOL_TAG.load(Ordering::Relaxed))
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
            unsafe {
                let p = if layout.align() > POOL_ALIGNMENT {
                    // The alignment is too large, the original pointer is stored right before the ptr.
                    // This is also how `_aligned_free` is implemented in msvcrt.dll
                    let q: *mut *mut u8 = ptr.cast();
                    q.sub(1).read()
                } else {
                    // The alignment is normal, so the ptr is already the original pointer.
                    ptr.cast()
                };
                ExFreePoolWithTag(p.cast(), GLOBAL_POOL_TAG.load(Ordering::Relaxed));
            }
        }
    }
}
