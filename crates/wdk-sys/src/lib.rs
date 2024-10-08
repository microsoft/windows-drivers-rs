// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Direct bindings to APIs available in the Windows Development Kit (WDK)

#![no_std]

#[cfg(any(driver_model__driver_type = "KMDF", driver_model__driver_type = "UMDF"))]
pub use wdf::WDF_FUNCTION_TABLE;
#[cfg(any(
    driver_model__driver_type = "WDM",
    driver_model__driver_type = "KMDF",
    driver_model__driver_type = "UMDF"
))]
#[doc(hidden)]
pub use wdk_macros as __proc_macros;

#[cfg(any(
    driver_model__driver_type = "WDM",
    driver_model__driver_type = "KMDF",
    driver_model__driver_type = "UMDF"
))]
pub use crate::{constants::*, types::*};

#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
pub mod ntddk;
#[cfg(any(driver_model__driver_type = "KMDF", driver_model__driver_type = "UMDF"))]
pub mod wdf;

#[cfg(driver_model__driver_type = "UMDF")]
pub mod windows;

#[cfg(feature = "test-stubs")]
pub mod test_stubs;

#[cfg(any(
    driver_model__driver_type = "WDM",
    driver_model__driver_type = "KMDF",
    driver_model__driver_type = "UMDF"
))]
mod constants;
#[cfg(any(
    driver_model__driver_type = "WDM",
    driver_model__driver_type = "KMDF",
    driver_model__driver_type = "UMDF"
))]
mod types;

#[cfg(any(
    driver_model__driver_type = "WDM",
    driver_model__driver_type = "KMDF",
    driver_model__driver_type = "UMDF"
))]
mod macros;

// This is fine because we don't actually have any floating point instruction in
// our binary, thanks to our target defining soft-floats. fltused symbol is
// necessary due to LLVM being too eager to set it: it checks the LLVM IR for
// floating point instructions - even if soft-float is enabled!
#[allow(missing_docs)]
#[no_mangle]
pub static _fltused: () = ();

// FIXME: Is there any way to avoid this stub? See https://github.com/rust-lang/rust/issues/101134
#[cfg(panic = "abort")]
#[allow(missing_docs)]
#[allow(clippy::missing_const_for_fn)] // const extern is not yet supported: https://github.com/rust-lang/rust/issues/64926
#[no_mangle]
pub extern "system" fn __CxxFrameHandler3() -> i32 {
    0
}

#[cfg(any(
    driver_model__driver_type = "WDM",
    driver_model__driver_type = "KMDF",
    driver_model__driver_type = "UMDF"
))]
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

#[cfg(any(
    driver_model__driver_type = "WDM",
    driver_model__driver_type = "KMDF",
    driver_model__driver_type = "UMDF"
))]
#[must_use]
#[allow(non_snake_case)]
#[allow(clippy::cast_sign_loss)]
/// Evaluates to TRUE if the return value specified by `nt_status` is an
/// informational type (0x40000000 − 0x7FFFFFFF). This function is taken from
/// ntdef.h in the WDK.
///
/// See the [NTSTATUS reference](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-erref/87fba13e-bf06-450e-83b1-9241dc81e781) and
/// [Using NTSTATUS values](https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/using-ntstatus-values) for details.
pub const fn NT_INFORMATION(nt_status: NTSTATUS) -> bool {
    (nt_status as u32 >> 30) == 1
}

#[cfg(any(
    driver_model__driver_type = "WDM",
    driver_model__driver_type = "KMDF",
    driver_model__driver_type = "UMDF"
))]
#[must_use]
#[allow(non_snake_case)]
#[allow(clippy::cast_sign_loss)]
/// Evaluates to TRUE if the return value specified by `nt_status` is a warning
/// type (0x80000000 − 0xBFFFFFFF).  This function is taken from ntdef.h in the
/// WDK.
///
/// See the [NTSTATUS reference](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-erref/87fba13e-bf06-450e-83b1-9241dc81e781) and
/// [Using NTSTATUS values](https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/using-ntstatus-values) for details.
pub const fn NT_WARNING(nt_status: NTSTATUS) -> bool {
    (nt_status as u32 >> 30) == 2
}

#[cfg(any(
    driver_model__driver_type = "WDM",
    driver_model__driver_type = "KMDF",
    driver_model__driver_type = "UMDF"
))]
#[must_use]
#[allow(non_snake_case)]
#[allow(clippy::cast_sign_loss)]
/// Evaluates to TRUE if the return value specified by `nt_status` is an error
/// type (0xC0000000 - 0xFFFFFFFF). This function is taken from ntdef.h in the
/// WDK.
///
/// See the [NTSTATUS reference](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-erref/87fba13e-bf06-450e-83b1-9241dc81e781) and
/// [Using NTSTATUS values](https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/using-ntstatus-values) for details.
pub const fn NT_ERROR(nt_status: NTSTATUS) -> bool {
    (nt_status as u32 >> 30) == 3
}

#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
#[allow(missing_docs)]
#[macro_export]
#[allow(non_snake_case)]
macro_rules! PAGED_CODE {
    () => {
        debug_assert!(unsafe { KeGetCurrentIrql() <= APC_LEVEL as u8 });
    };
}
