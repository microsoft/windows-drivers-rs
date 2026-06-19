// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Any library dependency that depends on `wdk-sys` requires these stubs to
//! provide symbols to successfully compile and run tests.
//!
//! These stubs can be brought into scope by introducing `wdk-sys` with the
//! `test-stubs` feature in the `dev-dependencies` of the crate's `Cargo.toml`

#[cfg(any(driver_model__driver_type = "KMDF", driver_model__driver_type = "UMDF"))]
pub use wdf::*;

#[cfg(any(
    driver_model__driver_type = "WDM",
    driver_model__driver_type = "KMDF",
    driver_model__driver_type = "UMDF"
))]
use crate::{DRIVER_OBJECT, NTSTATUS, PCUNICODE_STRING};
#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
use crate::{ERESOURCE, EX_SPIN_LOCK, KIRQL, LOGICAL, ULONG, ULONG_PTR};

/// Stubbed version of `DriverEntry` Symbol so that test targets will compile
///
/// # Safety
///
/// This function should never be called, so its safety is irrelevant
#[cfg(any(
    driver_model__driver_type = "WDM",
    driver_model__driver_type = "KMDF",
    driver_model__driver_type = "UMDF"
))]
// SAFETY: "DriverEntry" is the required symbol name for Windows driver entry points.
// No other function in this compilation unit exports this name, preventing symbol conflicts.
#[unsafe(export_name = "DriverEntry")] // WDF expects a symbol with the name DriverEntry
pub const unsafe extern "system" fn driver_entry_stub(
    _driver: &mut DRIVER_OBJECT,
    _registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    0
}

/// Stubbed version of `ExInitializeResourceLite` so test targets can link
#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
#[unsafe(no_mangle)]
pub extern "system" fn ExInitializeResourceLite(_resource: *mut ERESOURCE) -> NTSTATUS {
    crate::STATUS_SUCCESS
}

/// Stubbed version of `ExAcquireResourceSharedLite` so test targets can link
#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
#[unsafe(no_mangle)]
pub extern "system" fn ExAcquireResourceSharedLite(
    _resource: *mut ERESOURCE,
    _wait: crate::BOOLEAN,
) -> crate::BOOLEAN {
    1
}

/// Stubbed version of `ExAcquireResourceExclusiveLite` so test targets can link
#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
#[unsafe(no_mangle)]
pub extern "system" fn ExAcquireResourceExclusiveLite(
    _resource: *mut ERESOURCE,
    _wait: crate::BOOLEAN,
) -> crate::BOOLEAN {
    1
}

/// Stubbed version of `ExReleaseResourceLite` so test targets can link
#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
#[unsafe(no_mangle)]
pub extern "system" fn ExReleaseResourceLite(_resource: *mut ERESOURCE) {}

/// Stubbed version of `ExDeleteResourceLite` so test targets can link
#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
#[unsafe(no_mangle)]
pub extern "system" fn ExDeleteResourceLite(_resource: *mut ERESOURCE) -> NTSTATUS {
    crate::STATUS_SUCCESS
}

/// Stubbed version of `KeEnterCriticalRegion` so test targets can link
#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
#[unsafe(no_mangle)]
pub extern "system" fn KeEnterCriticalRegion() {}

/// Stubbed version of `KeLeaveCriticalRegion` so test targets can link
#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
#[unsafe(no_mangle)]
pub extern "system" fn KeLeaveCriticalRegion() {}

/// Stubbed version of `ExInitializePushLock` so test targets can link
///
/// # Safety
///
/// `push_lock` must point to valid writable push lock storage.
#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ExInitializePushLock(push_lock: *mut ULONG_PTR) {
    // SAFETY: Test callers pass a valid pointer to push lock storage.
    unsafe {
        push_lock.write(0);
    }
}

/// Stubbed version of `ExAcquirePushLockSharedEx` so test targets can link
#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
#[unsafe(no_mangle)]
pub extern "C" fn ExAcquirePushLockSharedEx(_push_lock: *mut ULONG_PTR, _flags: ULONG) {}

/// Stubbed version of `ExAcquirePushLockExclusiveEx` so test targets can link
#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
#[unsafe(no_mangle)]
pub extern "C" fn ExAcquirePushLockExclusiveEx(_push_lock: *mut ULONG_PTR, _flags: ULONG) {}

/// Stubbed version of `ExReleasePushLockSharedEx` so test targets can link
#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
#[unsafe(no_mangle)]
pub extern "C" fn ExReleasePushLockSharedEx(_push_lock: *mut ULONG_PTR, _flags: ULONG) {}

/// Stubbed version of `ExReleasePushLockExclusiveEx` so test targets can link
#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
#[unsafe(no_mangle)]
pub extern "C" fn ExReleasePushLockExclusiveEx(_push_lock: *mut ULONG_PTR, _flags: ULONG) {}

/// Stubbed version of `ExAcquireSpinLockShared` so test targets can link
#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
#[unsafe(no_mangle)]
pub extern "C" fn ExAcquireSpinLockShared(_spin_lock: *mut EX_SPIN_LOCK) -> KIRQL {
    0
}

/// Stubbed version of `ExAcquireSpinLockSharedAtDpcLevel` so test targets can
/// link
#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
#[unsafe(no_mangle)]
pub extern "C" fn ExAcquireSpinLockSharedAtDpcLevel(_spin_lock: *mut EX_SPIN_LOCK) {}

/// Stubbed version of `ExReleaseSpinLockShared` so test targets can link
#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
#[unsafe(no_mangle)]
pub extern "C" fn ExReleaseSpinLockShared(_spin_lock: *mut EX_SPIN_LOCK, _old_irql: KIRQL) {}

/// Stubbed version of `ExReleaseSpinLockSharedFromDpcLevel` so test targets can
/// link
#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
#[unsafe(no_mangle)]
pub extern "C" fn ExReleaseSpinLockSharedFromDpcLevel(_spin_lock: *mut EX_SPIN_LOCK) {}

/// Stubbed version of `ExAcquireSpinLockExclusive` so test targets can link
#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
#[unsafe(no_mangle)]
pub extern "C" fn ExAcquireSpinLockExclusive(_spin_lock: *mut EX_SPIN_LOCK) -> KIRQL {
    0
}

/// Stubbed version of `ExAcquireSpinLockExclusiveAtDpcLevel` so test targets
/// can link
#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
#[unsafe(no_mangle)]
pub extern "C" fn ExAcquireSpinLockExclusiveAtDpcLevel(_spin_lock: *mut EX_SPIN_LOCK) {}

/// Stubbed version of `ExReleaseSpinLockExclusive` so test targets can link
#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
#[unsafe(no_mangle)]
pub extern "C" fn ExReleaseSpinLockExclusive(_spin_lock: *mut EX_SPIN_LOCK, _old_irql: KIRQL) {}

/// Stubbed version of `ExReleaseSpinLockExclusiveFromDpcLevel` so test targets
/// can link
#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
#[unsafe(no_mangle)]
pub extern "C" fn ExReleaseSpinLockExclusiveFromDpcLevel(_spin_lock: *mut EX_SPIN_LOCK) {}

/// Stubbed version of `ExTryConvertSharedSpinLockExclusive` so test targets can
/// link
#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
#[unsafe(no_mangle)]
pub extern "C" fn ExTryConvertSharedSpinLockExclusive(_spin_lock: *mut EX_SPIN_LOCK) -> LOGICAL {
    1
}

#[cfg(any(driver_model__driver_type = "KMDF", driver_model__driver_type = "UMDF"))]
mod wdf {
    use crate::ULONG;

    /// Stubbed version of `WdfFunctionCount` Symbol so that test targets will
    /// compile
    // SAFETY: WdfFunctionCount is a required WDF symbol for test compilation.
    // No other symbols in this crate export this name, preventing linker conflicts.
    #[unsafe(no_mangle)]
    pub static mut WdfFunctionCount: ULONG = 0;

    include!(concat!(env!("OUT_DIR"), "/test_stubs.rs"));
}
