// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Direct FFI bindings to kernel-mode Windows Filtering Platform (WFP) APIs
//! from the Windows Driver Kit (WDK)
//!
//! This module contains all bindings to functions, constants, methods,
//! constructors and destructors for the kernel-mode WFP headers (`fwpsk.h` and
//! `fwpmk.h`). Types are not included in this module, but are available in the
//! top-level `wdk_sys` module.
//!
//! The user-mode WFP management API (`fwpmu.h`) ships in the Windows SDK rather
//! than the WDK, so it is not exposed here.

//! # Versioned names
//!
//! WFP versions its API surface by suffixing each routine and structure with a
//! revision number (`FwpsCalloutRegister3`, `FWPS_CALLOUT3`), and `fwpvi.h`
//! then `#define`s the unsuffixed spelling used throughout the WFP
//! documentation to whichever revision the driver's `NTDDI_VERSION` selects.
//!
//! Only the suffixed names are exposed here. bindgen does not emit object-like
//! macros that alias another identifier ([rust-bindgen#316]), and the
//! unsuffixed spellings cannot be reproduced faithfully in their place: they
//! are resolved per-consumer from `NTDDI_VERSION` in C, whereas a `pub use`
//! would have to pick one revision for every consumer at
//! binding-generation time. The suffixed names are also what the import
//! libraries actually export, which is the level a `-sys` crate is meant to
//! expose.
//!
//! To follow a WFP sample that says `FwpsCalloutRegister`, name the revision it
//! was written against ([`FwpsCalloutRegister3`] for Windows 10 RS3 and later).
//!
//! [rust-bindgen#316]: https://github.com/rust-lang/rust-bindgen/issues/316

pub use bindings::*;

#[allow(
    missing_docs,
    reason = "most items in the WDK headers have no inline documentation, so bindgen is unable to \
              generate documentation for their bindings"
)]
mod bindings {
    #[allow(
        clippy::wildcard_imports,
        reason = "the underlying c code relies on all type definitions being in scope, which \
                  results in the bindgen generated code relying on the generated types being in \
                  scope as well"
    )]
    use crate::types::*;

    include!(concat!(env!("OUT_DIR"), "/wfp.rs"));
}
