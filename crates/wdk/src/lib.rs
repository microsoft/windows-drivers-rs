// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Idiomatic Rust wrappers for the Windows Driver Kit (WDK) APIs. This crate is
//! built on top of the raw FFI bindings provided by [`wdk-sys`], and provides a
//! safe, idiomatic rust interface to the WDK.
#![no_std]
#![cfg_attr(feature = "nightly", feature(hint_must_use))]
#![deny(warnings)]
#![deny(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::cargo)]
#![deny(clippy::multiple_unsafe_ops_per_block)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![deny(clippy::unnecessary_safety_doc)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![deny(rustdoc::missing_crate_level_docs)]
#![deny(rustdoc::invalid_codeblock_attributes)]
#![deny(rustdoc::invalid_html_tags)]
#![deny(rustdoc::invalid_rust_codeblocks)]
#![deny(rustdoc::bare_urls)]
#![deny(rustdoc::unescaped_backticks)]
#![deny(rustdoc::redundant_explicit_links)]

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
