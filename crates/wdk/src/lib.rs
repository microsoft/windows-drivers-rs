// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Idiomatic Rust wrappers for the Windows Driver Kit (WDK) APIs. This crate is
//! built on top of the raw FFI bindings provided by [`wdk-sys`], and provides a
//! safe, idiomatic rust interface to the WDK.

#![no_std]

#[cfg(feature = "alloc")]
mod print;
#[cfg(feature = "alloc")]
pub use print::_print;
pub use wdk_sys::{NT_SUCCESS as nt_success, PAGED_CODE as paged_code};
pub mod wdf;

/// Trigger a breakpoint in debugger via architecture-specific inline assembly.
///
/// Implementations derived from details outlined in [MSVC `__debugbreak` intrinsic documentation](https://learn.microsoft.com/en-us/cpp/intrinsics/debugbreak?view=msvc-170#remarks)
///
/// # Panics
/// Will Panic if called on an unsupported architecture
pub fn dbg_break() {
    // SAFETY: Abides all rules outlined in https://doc.rust-lang.org/reference/inline-assembly.html#rules-for-inline-assembly
    unsafe {
        #[cfg(target_arch = "aarch64")]
        {
            core::arch::asm!("brk #0xF000");
            return;
        }

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            core::arch::asm!("int 3");
            return;
        }
    }

    #[allow(unreachable_code)] // Code is not dead because of conditional compilation
    {
        panic!("dbg_break function called from unsupported architecture");
    }
}
