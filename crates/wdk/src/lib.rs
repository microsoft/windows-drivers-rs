// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

#![no_std]
#![cfg_attr(feature = "nightly", feature(hint_must_use))]
#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::cargo)]

#[cfg(feature = "alloc")]
mod print;
#[cfg(feature = "alloc")]
pub use print::_print;
pub use wdk_sys::{NT_SUCCESS as nt_success, PAGED_CODE as paged_code};
pub mod wdf;

/// Trigger a breakpoint in debugger via architecture-specific inline assembly
///
/// # Panics
/// Will Panic if called on an unsupported architecture
pub fn dbg_break() {
    unsafe {
        #[cfg(target_arch = "aarch64")]
        {
            core::arch::asm!("int 3");
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
