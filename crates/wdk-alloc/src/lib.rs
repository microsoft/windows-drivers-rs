// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Allocator implementation to use with `#[global_allocator]` to allow use of
//! [`core::alloc`], plus the fallible allocation APIs that make it usable
//! without a bugcheck path.
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
//!
//! # Installing the allocator is not enough
//!
//! `WdkAllocator` (not linked here: it only exists for the WDM and KMDF driver
//! types, so the link would not resolve in a library documentation build)
//! reports failure by returning null, as
//! [`core::alloc::GlobalAlloc`] requires. What turns that null into a bugcheck
//! is the *caller*: [`alloc::vec::Vec::push`],
//! [`alloc::vec::Vec::with_capacity`], [`alloc::boxed::Box::new`],
//! [`alloc::sync::Arc::new`] and [`Iterator::collect`] all respond to a null by
//! calling [`alloc::alloc::handle_alloc_error`], which in a driver is
//! `KeBugCheckEx`.
//!
//! Driver code therefore needs the fallible counterparts in [`fallible`],
//! which are available on a stable toolchain and require no crate features:
//!
//! * [`fallible::FallibleVec`] — `try_push` and `try_extend`, in place of
//!   [`alloc::vec::Vec::push`] and [`alloc::vec::Vec::extend`]
//! * [`fallible::TryCollectVec`] — `try_collect_vec` in place of
//!   [`Iterator::collect`]
//! * [`fallible::try_vec_with_capacity`] — in place of
//!   [`alloc::vec::Vec::with_capacity`]
//! * [`fallible::try_box`] — in place of [`alloc::boxed::Box::new`]
//!
//! [`alloc::sync::Arc`] has no counterpart here. There used to be a
//! hand-written strong-count-only `sync::Arc`, because `Arc::try_new` sits
//! behind the unstable `allocator_api` feature and a stable toolchain therefore
//! had no fallible way to build a shared pointer at all. A driver that can
//! enable `#![feature(allocator_api)]` should use `Arc::try_new` and
//! `Arc::try_new_in` from `alloc` instead: they cover `Arc<T>`, `Weak`, and
//! `make_mut`, none of which a hand-written substitute did, and they are not
//! 300 lines of unsafe refcounting for this crate to keep correct. Note that
//! `Arc::try_new_uninit_slice` does not exist, so a fallible `Arc<[T]>` still
//! has to go through an `Arc<Vec<T>>`.
//!
//! Two near-misses that look like they would give a stable fallible `Arc` and
//! do not, recorded so they are not retried:
//!
//! * `Arc::from(try_box(value)?)` still allocates. `From<Box<T>>` has to build
//!   the strong/weak count block and move the value into it, and that second
//!   allocation is infallible.
//! * A one-element [`alloc::vec::Vec`] reserved with
//!   [`alloc::vec::Vec::try_reserve_exact`] and then `into_boxed_slice`d is not
//!   reliable either: `try_reserve_exact` is documented to be allowed to
//!   over-allocate, and when capacity exceeds length `into_boxed_slice` shrinks
//!   — an infallible reallocation.

#![no_std]

extern crate alloc;

#[cfg(test)]
extern crate std;

pub mod fallible;

#[cfg(test)]
mod test_allocator {
    use core::{
        alloc::{GlobalAlloc, Layout},
        cell::Cell,
    };

    /// A test-only global allocator that forwards allocations to the system
    /// allocator but can be told to fail the next allocation on the calling
    /// thread, so the fallible APIs' failure paths are exercised
    /// deterministically.
    pub struct FailingAllocator;

    std::thread_local! {
        /// When set, the next allocation on this thread fails by returning null.
        static FAIL_NEXT: Cell<bool> = const { Cell::new(false) };
    }

    /// Arm the allocator so the next allocation on the calling thread fails.
    pub fn fail_next_allocation() {
        FAIL_NEXT.with(|flag| flag.set(true));
    }

    // SAFETY: Successful allocations and all deallocations/reallocations are
    // delegated to `System` with the caller's layout; injected failures return
    // null. The methods do not unwind, and the failure flag is thread-local, so
    // a test arms and consumes the failure on a single thread without racing
    // the harness or other tests.
    unsafe impl GlobalAlloc for FailingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let fail = FAIL_NEXT.with(|flag| flag.replace(false));
            if fail {
                core::ptr::null_mut()
            } else {
                // SAFETY: forwarding the caller's layout to the system
                // allocator.
                unsafe { std::alloc::System.alloc(layout) }
            }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            // SAFETY: forwarding to the system allocator.
            unsafe { std::alloc::System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            // SAFETY: forwarding to the system allocator.
            unsafe { std::alloc::System.realloc(ptr, layout, new_size) }
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            // SAFETY: forwarding to the system allocator.
            unsafe { std::alloc::System.alloc_zeroed(layout) }
        }
    }

    #[global_allocator]
    pub static GLOBAL: FailingAllocator = FailingAllocator;
}

#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
pub use kernel_mode::*;

#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
mod kernel_mode {

    use core::alloc::{GlobalAlloc, Layout};

    use wdk_sys::{
        POOL_FLAG_NON_PAGED,
        POOL_FLAG_NON_PAGED_EXECUTE,
        POOL_FLAGS,
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

    /// Pool flags every allocation from this allocator is made with.
    ///
    /// `POOL_FLAG_NON_PAGED` is the *non-executable* non-paged pool -- the
    /// header spells it "Non paged pool NX" (`km/wdm.h`). Its sibling
    /// `POOL_FLAG_NON_PAGED_EXECUTE` is executable kernel memory, and handing
    /// that out for general Rust allocations would give every heap buffer in
    /// every consumer of this crate the executable bit for no reason -- a
    /// security regression that no functional test would notice, because
    /// executable memory works perfectly well for storing data.
    const POOL_FLAGS_USED: POOL_FLAGS = POOL_FLAG_NON_PAGED;

    // Enforce the choice above at build time rather than trusting the constant
    // name to stay meaningful. This catches both a `NON_PAGED` ->
    // `NON_PAGED_EXECUTE` edit and the subtler case of the executable bit being
    // OR-ed into a flag set that still mentions NX.
    const _: () = assert!(
        POOL_FLAGS_USED & POOL_FLAG_NON_PAGED_EXECUTE == 0,
        "wdk-alloc must never allocate from executable non-paged pool: general-purpose Rust \
         allocations have no need of the executable bit, and granting it weakens every consumer"
    );

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
                    ExAllocatePool2(POOL_FLAGS_USED, layout.size() as SIZE_T, RUST_TAG)
                };
            if ptr.is_null() {
                return core::ptr::null_mut();
            }
            ptr.cast()
        }

        unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
            // SAFETY: `ExFreePool` is safe to call from any `IRQL` <=
            // `DISPATCH_LEVEL` since its freeing memory allocated
            // from `POOL_FLAG_NON_PAGED` in `alloc`
            unsafe {
                ExFreePool(ptr.cast());
            }
        }
    }
}
