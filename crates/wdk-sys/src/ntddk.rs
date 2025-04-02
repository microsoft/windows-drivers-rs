// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Direct FFI bindings to NTDDK APIs from the Windows Driver Kit (WDK)
//!
//! This module contains all bindings to functions, constants, methods,
//! constructors and destructors in `ntddk.h`. Types are not included in this
//! module, but are available in the top-level `wdk_sys` module.

pub use bindings::*;
use crate::{PIRP, PIO_STACK_LOCATION};

#[allow(missing_docs)]
mod bindings {
    #[allow(
        clippy::wildcard_imports,
        reason = "the underlying c code relies on all type definitions being in scope, which \
                  results in the bindgen generated code relying on the generated types being in \
                  scope as well"
    )]
    use crate::types::*;

    include!(concat!(env!("OUT_DIR"), "/ntddk.rs"));
}

/// The IoGetCurrentIrpStackLocation routine returns a pointer to the caller's I/O stack location in 
/// the specified IRP.
/// 
/// # Parameters
/// - irp: PIRP - A pointer to the IRP.
/// 
/// # Returns
/// IoGetCurrentIrpStackLocation returns a pointer to an IO_STACK_LOCATION structure that contains 
/// the I/O stack location for the driver.
///
/// # Safety
/// This function directly accesses raw pointers and must only be used
/// when it is guaranteed that the provided `irp` is valid and properly
/// initialised. Using an invalid or uninitialised `irp` will result
/// in undefined behavior.
#[allow(non_snake_case)]
pub unsafe fn IoGetCurrentIrpStackLocation(irp: PIRP) -> PIO_STACK_LOCATION {
    unsafe { 
        assert!((*irp).CurrentLocation <= (*irp).StackCount + 1);
    
        // Access the union fields inside the IRP
        (*irp)
        .Tail
        .Overlay
        .__bindgen_anon_2
        .__bindgen_anon_1
        .CurrentStackLocation
    }
}