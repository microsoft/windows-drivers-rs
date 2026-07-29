// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Any library dependency that depends on `wdk-sys` requires these stubs to
//! provide symbols to successfully compile and run tests.
//!
//! Some scenarios where these stubs can be helpful:
//!
//! - Default cargo profiles
//!
//! - `wdk-sys` dependent crates that need to enable compilation for `test`
//!   targets
//!   - enabling these stubs bypasses the need to manually define symbols
//!     expected to be present by the bindings (usually symbols provided by the
//!     final binary like `DriverEntry`)
//!
//! - lib crate usage that depends on wdk-sys, eg. not a driver bin crate
//!   - if used with a driver bin crate you may need to cfg gate your
//!     DriverEntry since these stubs provide one
//!
//! - crate tests that don't rely on the stubbed symbols' functionality
//!   - if you want to write tests where these symbols are exercised (ex: tests
//!     that call WDF functions via our macros) then you must provide your own
//!     mocks in the test
//!
//! NOTE: Enabling fat LTO in your dev profile may lead to Linker errors
//! even if you sufficiently cfg gate WDF function usage. This is because
//! the dev profile defaults to `opt-level = 0`, and in combination with fat
//! LTO may cause dead code to not be optimized out (fat LTO merges all upstream
//! crates' code into the final binary rather than letting the linker pull in
//! only what's needed, so WDF wrappers from an ungated `use wdk::` get
//! included even if you never call them).
//!   - If "test-stubs" is enabled the Linker may complain that
//!     `WdfDriverGlobals` is missing, it is intentionally not stubbed here
//!     because it is needed when a WDF function is called, landing it outside
//!     the intent of test-stubs.
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
// SAFETY: "DriverEntry" is the required symbol name for Windows driver entry points.
// No other function in this compilation unit exports this name, preventing symbol conflicts.
#[unsafe(export_name = "DriverEntry")] // WDF expects a symbol with the name DriverEntry
pub const unsafe extern "system" fn driver_entry_stub(
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
    // SAFETY: WdfFunctionCount is a required WDF symbol for test compilation.
    // No other symbols in this crate export this name, preventing linker conflicts.
    #[unsafe(no_mangle)]
    pub static mut WdfFunctionCount: ULONG = 0;

    include!(concat!(env!("OUT_DIR"), "/test_stubs.rs"));
}
