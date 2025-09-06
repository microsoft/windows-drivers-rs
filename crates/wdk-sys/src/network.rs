// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Direct FFI bindings to Network APIs from the Windows Driver Kit (WDK)
//!
//! This module contains all bindings to functions, constants, methods,
//! constructors and destructors for wsk.h. Types are not included in
//! this module, but are available in the top-level `wdk_sys` module.
//!
//! Other headers from the Network subset are to be added in the future.

#[allow(missing_docs)]
#[allow(clippy::derive_partial_eq_without_eq)]
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
