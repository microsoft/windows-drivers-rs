// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Idiomatic Rust wrappers for the Windows Driver Kit (WDK) APIs. This crate is
//! built on top of the raw FFI bindings provided by [`wdk_sys`], and provides a
//! safe, idiomatic rust interface to the WDK.

#![cfg_attr(
    any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"),
    no_std
)]

#[cfg(any(
    driver_model__driver_type = "WDM",
    driver_model__driver_type = "KMDF",
    driver_model__driver_type = "UMDF",
))]
pub use print::_print;
#[cfg(any(
    driver_model__driver_type = "WDM",
    driver_model__driver_type = "KMDF",
    driver_model__driver_type = "UMDF"
))]
pub use wdk_sys::NT_SUCCESS as nt_success;
#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
pub use wdk_sys::PAGED_CODE as paged_code;

#[cfg(any(
    driver_model__driver_type = "WDM",
    driver_model__driver_type = "KMDF",
    driver_model__driver_type = "UMDF",
))]
mod print;

/// Fixed-size formatting types for heap-free `fmt::Write` in driver
/// environments.
pub mod fmt;

#[cfg(any(driver_model__driver_type = "KMDF", driver_model__driver_type = "UMDF"))]
pub mod wdf;

/// Trigger a breakpoint in debugger via architecture-specific inline assembly.
///
/// Implementations derived from details outlined in [MSVC `__debugbreak` intrinsic documentation](https://learn.microsoft.com/en-us/cpp/intrinsics/debugbreak?view=msvc-170#remarks)
///
/// # Unsupported architectures
///
/// An architecture with no breakpoint instruction here fails to *compile*
/// rather than panicking at runtime, which is what this used to do. In kernel
/// mode a panic is a `KeBugCheckEx` call, so the old fallback meant a crate
/// that could not implement `dbg_break` for a target shipped a blue screen
/// inside a debugging helper -- and it put a bugcheck path in every driver
/// image built from it, since the panic machinery is reachable code as far as
/// the linker is concerned. There is no target this can be reached on in any
/// case: [`CpuArchitecture`] accepts only `x86_64` and `aarch64`, so a build
/// for anything else has already failed by the time this matters.
///
/// [`CpuArchitecture`]: https://docs.rs/wdk-build/latest/wdk_build/enum.CpuArchitecture.html
pub fn dbg_break() {
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")))]
    compile_error!(
        "dbg_break has no breakpoint instruction for this target architecture. Add one above \
         rather than reaching for a runtime panic: in kernel mode that is a bugcheck."
    );

    // SAFETY: Abides all rules outlined in https://doc.rust-lang.org/reference/inline-assembly.html#rules-for-inline-assembly
    unsafe {
        #[cfg(target_arch = "aarch64")]
        core::arch::asm!("brk #0xF000");

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        core::arch::asm!("int 3");
    }
}
