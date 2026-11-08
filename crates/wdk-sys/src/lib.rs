// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Direct bindings to APIs available in the Windows Development Kit (WDK)

#![no_std]

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

#[cfg(driver_model__driver_type = "UMDF")]
pub mod windows;

#[cfg(any(driver_model__driver_type = "KMDF", driver_model__driver_type = "UMDF"))]
pub mod wdf;

#[cfg(all(
    any(
        driver_model__driver_type = "WDM",
        driver_model__driver_type = "KMDF",
        driver_model__driver_type = "UMDF"
    ),
    feature = "gpio"
))]
pub mod gpio;

#[cfg(all(
    any(
        driver_model__driver_type = "WDM",
        driver_model__driver_type = "KMDF",
        driver_model__driver_type = "UMDF"
    ),
    feature = "hid"
))]
pub mod hid;

// The Filter Manager API surface is only available in kernel-mode
#[cfg(all(
    any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"),
    feature = "minifilter"
))]
pub mod minifilter;

#[cfg(all(
    any(
        driver_model__driver_type = "WDM",
        driver_model__driver_type = "KMDF",
        driver_model__driver_type = "UMDF"
    ),
    feature = "parallel-ports"
))]
pub mod parallel_ports;

#[cfg(all(
    any(
        driver_model__driver_type = "WDM",
        driver_model__driver_type = "KMDF",
        driver_model__driver_type = "UMDF"
    ),
    feature = "spb"
))]
pub mod spb;

#[cfg(all(
    any(
        driver_model__driver_type = "WDM",
        driver_model__driver_type = "KMDF",
        driver_model__driver_type = "UMDF"
    ),
    feature = "storage"
))]
pub mod storage;

#[cfg(all(
    any(
        driver_model__driver_type = "WDM",
        driver_model__driver_type = "KMDF",
        driver_model__driver_type = "UMDF"
    ),
    feature = "usb"
))]
pub mod usb;

// The kernel-mode WFP API surface is only available in kernel-mode
#[cfg(all(
    any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"),
    feature = "wfp"
))]
pub mod wfp;

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
// SAFETY: _fltused is a required Windows linker symbol for floating point support.
// No other symbols in this crate export this name, preventing linker conflicts.
#[unsafe(no_mangle)]
pub static _fltused: () = ();

// FIXME: Is there any way to avoid these stubs? See https://github.com/rust-lang/rust/issues/101134
#[cfg(panic = "abort")]
#[allow(missing_docs)]
// SAFETY: __CxxFrameHandler3 is a required Windows C++ exception handler symbol.
// No other symbols in this crate export this name, preventing linker conflicts.
#[unsafe(no_mangle)]
pub const extern "system" fn __CxxFrameHandler3() -> i32 {
    0
}

#[cfg(panic = "abort")]
#[allow(missing_docs)]
// SAFETY: __CxxFrameHandler4 is a required Windows C++ exception handler symbol.
// No other symbols in this crate export this name, preventing linker conflicts.
#[unsafe(no_mangle)]
pub const extern "system" fn __CxxFrameHandler4() -> i32 {
    // This is a stub for the C++ exception handling frame handler. It's never
    // called but it needs to be distinct from __CxxFrameHandler3 to not confuse
    // binary analysis tools. We return a different value to prevent folding.
    1
}

#[cfg(panic = "abort")]
#[allow(missing_docs)]
// SAFETY: __GSHandlerCheck_EH4 is a required Windows C++ exception handler symbol.
// No other symbols in this crate export this name, preventing linker conflicts.
#[unsafe(no_mangle)]
pub const extern "system" fn __GSHandlerCheck_EH4() -> i32 {
    // This is a stub for the C++ exception handling frame handler. It's never
    // called but it needs to be distinct from __CxxFrameHandler3 and
    // __CxxFrameHandler4 to not confuse binary analysis tools. We return a
    // different value to prevent folding.
    2
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
#[must_use]
#[allow(non_snake_case)]
/// Returns an [`OBJECT_ATTRIBUTES`] initialized with the supplied object name,
/// attributes, root directory, and security descriptor, with `Length` set to
/// the size of the structure and `SecurityQualityOfService` set to null.
///
/// This is a port of the `InitializeObjectAttributes` macro from `ntdef.h` in
/// the WDK, which bindgen cannot generate because it is function-like: <https://github.com/rust-lang/rust-bindgen/issues/316>.
/// The C macro writes through a caller-supplied `POBJECT_ATTRIBUTES`; this
/// returns the structure by value instead, since that is both safe and how a
/// caller in Rust would initialize a local.
///
/// Note that the returned structure borrows `ObjectName`, `RootDirectory`, and
/// `SecurityDescriptor` as raw pointers: all three must remain valid for as
/// long as the routine the [`OBJECT_ATTRIBUTES`] is passed to may dereference
/// them.
pub const fn InitializeObjectAttributes(
    ObjectName: PUNICODE_STRING,
    Attributes: ULONG,
    RootDirectory: HANDLE,
    SecurityDescriptor: PVOID,
) -> OBJECT_ATTRIBUTES {
    OBJECT_ATTRIBUTES {
        Length: OBJECT_ATTRIBUTES_LENGTH,
        RootDirectory,
        ObjectName,
        Attributes,
        SecurityDescriptor,
        SecurityQualityOfService: core::ptr::null_mut(),
    }
}

/// `size_of::<OBJECT_ATTRIBUTES>()` narrowed to the [`ULONG`] that
/// `OBJECT_ATTRIBUTES::Length` expects.
///
/// `TryFrom` is not callable in a `const` context, so the value is narrowed via
/// its little-endian bytes and the build fails if any discarded byte is
/// non-zero. `OBJECT_ATTRIBUTES` is 48 bytes on every supported target, so this
/// is a guard against a future change rather than a live concern.
#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
const OBJECT_ATTRIBUTES_LENGTH: ULONG = {
    let bytes = size_of::<OBJECT_ATTRIBUTES>().to_le_bytes();

    let mut index = size_of::<ULONG>();
    while index < bytes.len() {
        assert!(
            bytes[index] == 0,
            "size_of::<OBJECT_ATTRIBUTES>() should fit in OBJECT_ATTRIBUTES::Length"
        );
        index += 1;
    }

    ULONG::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
};

#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
#[allow(missing_docs)]
#[macro_export]
#[allow(non_snake_case)]
macro_rules! PAGED_CODE {
    () => {
        debug_assert!(unsafe { $crate::ntddk::KeGetCurrentIrql() <= $crate::APC_LEVEL as u8 });
    };
}
