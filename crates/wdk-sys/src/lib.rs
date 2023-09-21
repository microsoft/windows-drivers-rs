// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

#![no_std]
#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::cargo)]

mod constants;
mod types;

pub use crate::{constants::*, types::*};

pub mod macros;
pub mod ntddk;
pub mod wdf;

#[cfg(feature = "test-stubs")]
pub mod test_stubs;

use lazy_static::lazy_static;

// This is fine because we don't actually have any floating point instruction in
// our binary, thanks to our target defining soft-floats. fltused symbol is
// necessary due to LLVM being too eager to set it: it checks the LLVM IR for
// floating point instructions - even if soft-float is enabled!
#[no_mangle]
pub static _fltused: () = ();

// FIXME: Is there any way to avoid this stub? See https://github.com/rust-lang/rust/issues/101134
#[allow(clippy::missing_const_for_fn)] // const extern is not yet supported: https://github.com/rust-lang/rust/issues/64926
#[no_mangle]
pub extern "system" fn __CxxFrameHandler3() -> i32 {
    0
}

// FIXME: dynamically find name of this struct based off of wdk-build settings
// FIXME: replace lazy_static with std::Lazy once available: https://github.com/rust-lang/rust/issues/109736
lazy_static! {
    pub static ref WDF_FUNCTION_TABLE: &'static [WDFFUNC] =
        unsafe { core::slice::from_raw_parts(WdfFunctions_01033, WdfFunctionCount as usize) };
}

#[must_use]
#[allow(non_snake_case)]
pub const fn NT_SUCCESS(nt_status: NTSTATUS) -> bool {
    nt_status >= 0
}

#[macro_export]
#[allow(non_snake_case)]
macro_rules! PAGED_CODE {
    () => {
        debug_assert!(unsafe { KeGetCurrentIrql() <= APC_LEVEL as u8 });
    };
}
