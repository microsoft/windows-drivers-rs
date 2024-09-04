// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Any library dependency that depends on `wdk-sys` requires these stubs to
//! provide symobols to successfully compile and run tests.
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
#[export_name = "DriverEntry"] // WDF expects a symbol with the name DriverEntry
pub unsafe extern "system" fn driver_entry_stub(
    _driver: &mut DRIVER_OBJECT,
    _registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    0
}

#[cfg(any(driver_model__driver_type = "KMDF", driver_model__driver_type = "UMDF"))]
mod wdf {
    use crate::ULONG;

    /// Stubbed version of `WdfFunctionCount` Symbol so that test targets will
    /// compile
    #[no_mangle]
    pub static mut WdfFunctionCount: ULONG = 0;

    include!(concat!(env!("OUT_DIR"), "/test_stubs.rs"));
}
