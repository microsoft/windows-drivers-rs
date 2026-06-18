// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Direct FFI bindings to network APIs from the Windows Driver Kit (WDK)
//!
//! This module contains all bindings for network headers. Types are not
//! included in this module, but are available in the top-level `wdk_sys`
//! module.
#[allow(
    missing_docs,
    reason = "most items in the WDK headers have no inline documentation, so bindgen is unable to \
              generate documentation for their bindings"
)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[allow(non_upper_case_globals)]
#[allow(unnecessary_transmutes)]
mod bindings {
    #[allow(
        clippy::wildcard_imports,
        reason = "the underlying c code relies on all type definitions being in scope, which \
                  results in the bindgen generated code relying on the generated types being in \
                  scope as well"
    )]
    use crate::types::*;

    include!(concat!(env!("OUT_DIR"), "/network.rs"));
}

pub use bindings::*;
