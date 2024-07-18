// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Direct bindings to APIs available in the Windows Development Kit (WDK)

#![no_std]

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

#[must_use]
#[allow(non_snake_case)]
/// Evaluates to TRUE if the return value specified by `nt_status` is a success
/// type (0 − 0x3FFFFFFF) or an informational type (0x40000000 − 0x7FFFFFFF).
/// This function is taken from ntdef.h in the WDK.
///
/// See the [NTSTATUS reference](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-erref/87fba13e-bf06-450e-83b1-9241dc81e781) and
/// [Using NTSTATUS values](https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/using-ntstatus-values) for details.
pub const fn NT_SUCCESS(nt_status: NTSTATUS) -> bool {
    nt_status >= 0
}

#[must_use]
#[allow(non_snake_case)]
/// Evaluates to TRUE if the return value specified by `nt_status` is an
/// informational type (0x40000000 − 0x7FFFFFFF). This function is taken from
/// ntdef.h in the WDK.
///
/// See the [NTSTATUS reference](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-erref/87fba13e-bf06-450e-83b1-9241dc81e781) and
/// [Using NTSTATUS values](https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/using-ntstatus-values) for details.
pub const fn NT_INFORMATION(nt_status: NTSTATUS) -> bool {
    (nt_status as u32 >> 30) == 1
}

#[must_use]
#[allow(non_snake_case)]
/// Evaluates to TRUE if the return value specified by `nt_status` is a warning
/// type (0x80000000 − 0xBFFFFFFF).  This function is taken from ntdef.h in the
/// WDK.
///
/// See the [NTSTATUS reference](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-erref/87fba13e-bf06-450e-83b1-9241dc81e781) and
/// [Using NTSTATUS values](https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/using-ntstatus-values) for details.
pub const fn NT_WARNING(nt_status: NTSTATUS) -> bool {
    (nt_status as u32 >> 30) == 2
}

#[must_use]
#[allow(non_snake_case)]
/// Evaluates to TRUE if the return value specified by `nt_status` is an error
/// type (0xC0000000 - 0xFFFFFFFF). This function is taken from ntdef.h in the
/// WDK.
///
/// See the [NTSTATUS reference](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-erref/87fba13e-bf06-450e-83b1-9241dc81e781) and
/// [Using NTSTATUS values](https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/using-ntstatus-values) for details.
pub const fn NT_ERROR(nt_status: NTSTATUS) -> bool {
    (nt_status as u32 >> 30) == 3
}

#[allow(missing_docs)]
#[macro_export]
#[allow(non_snake_case)]
macro_rules! PAGED_CODE {
    () => {
        debug_assert!(unsafe { KeGetCurrentIrql() <= APC_LEVEL as u8 });
    };
}

#[cfg(test)]
mod tests {
    use crate::{
        NT_ERROR,
        NT_INFORMATION,
        NT_SUCCESS,
        NT_WARNING,
        STATUS_BREAKPOINT,
        STATUS_HIBERNATED,
        STATUS_PRIVILEGED_INSTRUCTION,
        STATUS_SUCCESS,
    };
    #[test]
    pub fn status_tests() {
        const {
            assert!(NT_SUCCESS(STATUS_SUCCESS));
            assert!(NT_SUCCESS(STATUS_HIBERNATED));
            assert!(NT_INFORMATION(STATUS_HIBERNATED));
            assert!(NT_WARNING(STATUS_BREAKPOINT));
            assert!(NT_ERROR(STATUS_PRIVILEGED_INSTRUCTION));
            assert!(!(NT_INFORMATION(STATUS_SUCCESS)));
        }
    }
}
