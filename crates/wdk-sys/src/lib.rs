// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Direct bindings to APIs available in the Windows Development Kit (WDK)

#![no_std]
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
#[allow(missing_docs)]
#[no_mangle]
pub static _fltused: () = ();

// FIXME: Is there any way to avoid this stub? See https://github.com/rust-lang/rust/issues/101134
#[allow(missing_docs)]
#[allow(clippy::missing_const_for_fn)] // const extern is not yet supported: https://github.com/rust-lang/rust/issues/64926
#[no_mangle]
pub extern "system" fn __CxxFrameHandler3() -> i32 {
    0
}

// FIXME: dynamically find name of this struct based off of wdk-build settings
// FIXME: replace lazy_static with std::Lazy once available: https://github.com/rust-lang/rust/issues/109736
lazy_static! {
    #[allow(missing_docs)]
    pub static ref WDF_FUNCTION_TABLE: &'static [WDFFUNC] = {
        // SAFETY: `WdfFunctions_01033` is generated as a mutable static, but is not supposed to be ever mutated by WDF.
        let wdf_function_table = unsafe { WdfFunctions_01033 };

        // SAFETY: `WdfFunctionCount` is generated as a mutable static, but is not supposed to be ever mutated by WDF.
        let wdf_function_count = unsafe { WdfFunctionCount } as usize;

        // SAFETY: This is safe because:
        //         1. `WdfFunctions_01033` is valid for reads for `WdfFunctionCount` * `core::mem::size_of::<WDFFUNC>()`
        //            bytes, and is guaranteed to be aligned and it must be properly aligned.
        //         2. `WdfFunctions_01033` points to `WdfFunctionCount` consecutive properly initialized values of
        //            type `WDFFUNC`.
        //         3. WDF does not mutate the memory referenced by the returned slice for for its entire `'static' lifetime.
        //         4. The total size, `WdfFunctionCount` * `core::mem::size_of::<WDFFUNC>()`, of the slice must be no
        //            larger than `isize::MAX`. This is proven by the below `debug_assert!`.
        unsafe {
            debug_assert!(isize::try_from(wdf_function_count * core::mem::size_of::<WDFFUNC>()).is_ok());
            core::slice::from_raw_parts(wdf_function_table, wdf_function_count)
        }
    };
}

#[allow(missing_docs)]
#[must_use]
#[allow(non_snake_case)]
pub const fn NT_SUCCESS(nt_status: NTSTATUS) -> bool {
    nt_status >= 0
}

#[allow(missing_docs)]
#[macro_export]
#[allow(non_snake_case)]
macro_rules! PAGED_CODE {
    () => {
        debug_assert!(unsafe { KeGetCurrentIrql() <= APC_LEVEL as u8 });
    };
}
