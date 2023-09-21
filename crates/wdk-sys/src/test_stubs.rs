// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Any library dependency that depends on `wdk-sys` requires these stubs to
//! provide symobols to sucessfully compile and run tests. They can be brought
//! into scope by introducing `wdk-sys` with the `test-stubs` feature in the
//! `dev-dependencies` of the crate's `Cargo.toml`

use crate::{DRIVER_OBJECT, NTSTATUS, PCUNICODE_STRING};

/// Stubbed version of `DriverEntry` Symbol so that test targets will compile
///
/// # Safety
///
/// This function should never be called, so its safety is irrelevant
#[export_name = "DriverEntry"] // WDF expects a symbol with the name DriverEntry
pub unsafe extern "system" fn driver_entry_stub(
    _driver: &mut DRIVER_OBJECT,
    _registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    0
}
