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

// Note: to run tests, go to "tests/wdk-alloc-tests" of the root repository
// and run `cargo test --lib` command.
#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
mod kernel_mode {

    use core::alloc::{GlobalAlloc, Layout};

    use wdk_sys::{PAGE_SIZE, POOL_FLAGS, POOL_FLAG_NON_PAGED, PVOID, SIZE_T, ULONG};
    #[cfg(not(test))]
    use wdk_sys::ntddk::{ExAllocatePool2, ExFreePoolWithTag};

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

    /// The minimum alignment of pointer returned by ExAllocatePool2.
    pub const MIN_POOL_ALIGNMENT: usize = size_of::<*mut u8>() * 2;

    type ExAllocatePool2Fn = unsafe extern "C" fn(pool_flags: POOL_FLAGS, number_of_bytes: SIZE_T, tag: ULONG) -> PVOID;
    type ExFreePoolWithTagFn = unsafe extern "C" fn(p: PVOID, tag: ULONG);
    #[cfg(test)]
    const EX_ALLOCATE_POOL2_FN:ExAllocatePool2Fn=super::tests::mock_ex_allocate_pool;
    #[cfg(test)]
    const EX_FREE_POOL_WITH_TAG_FN:ExFreePoolWithTagFn=super::tests::mock_ex_free_pool_with_tag;
    #[cfg(not(test))]
    const EX_ALLOCATE_POOL2_FN:ExAllocatePool2Fn=ExAllocatePool2;
    #[cfg(not(test))]
    const EX_FREE_POOL_WITH_TAG_FN:ExFreePoolWithTagFn=ExFreePoolWithTag;

    #[inline]
    fn require_realignment(layout: Layout) -> bool {
        // `ExAllocatePool2` uses an alignment of `PAGE_SIZE` or minimum pool
        // alignment depending on the requested size. See documentation:
        // https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-exallocatepool2#remarks
        let chosen_alignment = if layout.size() >= PAGE_SIZE as usize {
            PAGE_SIZE as usize
        } else {
            MIN_POOL_ALIGNMENT
        };
        // Realignment needed only if requested alignment cannot
        // be satisfied by `ExAllocatePool2`'s chosen alignment
        layout.align() > chosen_alignment
    }

    unsafe impl GlobalAlloc for WdkAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let ptr =
                if require_realignment(layout) {
                    // If the requested alignment is too large, we'll have to waste some space as large as the alignment.
                    let size = layout.size() + layout.align();
                    // Note that `layout.align()` guarantees the alignment will always be a power of 2.
                    // So we can use the masking trick to find the aligned address.
                    let mask = !(layout.align() - 1);
                    // SAFETY: `ExAllocatePool2` is safe to call from any `IRQL` <= `DISPATCH_LEVEL`
                    // since its allocating from `POOL_FLAG_NON_PAGED`
                    let p = unsafe {
                        EX_ALLOCATE_POOL2_FN(POOL_FLAG_NON_PAGED, size as SIZE_T, RUST_TAG)
                    };
                    if !p.is_null() {
                        // Align the pointer up to the first address that meets alignment requirement.
                        let q = ((p as usize & mask) + layout.align()) as *mut PVOID;
                        // SAFETY: There are sufficient space to store a pointer. See the following graph:
                        //   | ......... | p | ..................... | ...... |
                        //                   ^                       ^
                        //             aligned start            aligned end
                        // "aligned start" is the address we will return in `alloc` method.
                        // The gap between the "aligned start" and the original address has a
                        // guaranteed minimum size of two pointers, so it's safe to store this pointer.
                        // "p" is where we store the original address returned by `ExAllocatePool2`.
                        // We will also pass "p" to `ExFreePoolWithTag` on `dealloc` method.
                        // This is also how `_aligned_free` in msvcrt.dll receives the original pointer.
                        unsafe {
                            // Store the original pointer right before the pointer to return,
                            // so that `ExFreePoolWithTag` can receive the correct pointer.
                            // This is also how `_aligned_malloc` is implemented in msvcrt.dll
                            q.sub(1).write(p);
                        }
                        q.cast()
                    } else {
                        p
                    }
                } else {
                    // SAFETY: `ExAllocatePool2` is safe to call from any `IRQL` <= `DISPATCH_LEVEL`
                    // since its allocating from `POOL_FLAG_NON_PAGED`
                    unsafe {
                        // Because we know the alignment while in both alloc and dealloc,
                        // we don't need to waste space if the default alignment is small enough.
                        EX_ALLOCATE_POOL2_FN(POOL_FLAG_NON_PAGED, layout.size() as SIZE_T, RUST_TAG)
                    }
                };
            if ptr.is_null() {
                return core::ptr::null_mut();
            }
            ptr.cast()
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            let p = if require_realignment(layout) {
                // The alignment is too large, so the original pointer is stored right before
                // the ptr. This is also how `_aligned_free` is implemented in msvcrt.dll
                let q: *mut PVOID = ptr.cast();
                // SAFETY: We stored the original pointer right before the aligned object. See the following graph:
                //   | ......... | p | ..................... | ...... |
                //                   ^                       ^
                //             aligned start            aligned end
                // "p" is where we store the original address returned by `ExAllocatePool2`.
                // So we will pass "p" to `ExFreePoolWithTag` on `dealloc` method.
                // "aligned start" is the address we received from `ptr` argument in `dealloc` method.
                unsafe { q.sub(1).read() }
            } else {
                // The alignment is normal, so the ptr is already the original pointer.
                ptr.cast()
            };
            // SAFETY: `ExFreePoolWithTag` is safe to call from any `IRQL` <= `DISPATCH_LEVEL`
            // since its freeing memory allocated from `POOL_FLAG_NON_PAGED` in `alloc`
            unsafe {
                EX_FREE_POOL_WITH_TAG_FN(p, RUST_TAG);
            }
        }
    }
}

#[cfg(all(test, any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF")))]
mod tests {
    use core::sync::atomic::{AtomicUsize, Ordering};
    use wdk_sys::{POOL_FLAGS, PVOID, SIZE_T, ULONG};

    extern crate alloc;
    use alloc::boxed::Box;

    #[repr(C,align(16))]
    struct MockPool {
        buffer:[u8;POOL_SIZE]
    }

    // For test cases, we use a dumb allocator to mock `ExAllocatePool2`:
    // Freed memory aren't reused.
    // The pool has a maximum size of 1MiB.
    const POOL_SIZE:usize=0x100000;
    static POOL_BUFFER:MockPool=MockPool{buffer:[0;POOL_SIZE]};
    // Use AtomicXxx type to grant thread-safety and interior mutability.
    static POOL_USAGE:AtomicUsize=AtomicUsize::new(0);
    // The `FREE_TIMES` static could be used for asserting leaks.
    static FREE_TIMES:AtomicUsize=AtomicUsize::new(0);

    pub(super) unsafe extern "C" fn mock_ex_allocate_pool(_pool_flags:POOL_FLAGS,number_of_bytes:SIZE_T,_tag:ULONG) -> PVOID {
        let i=POOL_USAGE.fetch_add(number_of_bytes as usize, Ordering::AcqRel);
        unsafe {
            POOL_BUFFER.buffer.as_ptr().add(i) as PVOID
        }
    }

    pub(super) unsafe extern "C" fn mock_ex_free_pool_with_tag(_p:PVOID,_tag:ULONG) {
        FREE_TIMES.fetch_add(1,Ordering::AcqRel);
    }

	use super::{WdkAllocator, MIN_POOL_ALIGNMENT};

	#[global_allocator]
	static MOCK_ALLOCATOR:WdkAllocator = WdkAllocator;

	#[test]
	fn basic_mock_and_leak() {
		let x:Box<u32> = Box::new(12345);
		assert_eq!((Box::into_raw(x) as usize) & (MIN_POOL_ALIGNMENT - 1), 0);
        assert_eq!(FREE_TIMES.load(Ordering::SeqCst), 0);
	}
}