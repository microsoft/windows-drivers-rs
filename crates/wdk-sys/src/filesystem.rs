// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Bindings for the filesystem (IFSK) APIs from the Windows Driver Kit
//!
//! This module contains all bindings to functions, constants, methods,
//! constructors and destructors for IFSK headers. Types are not included in
//! this module, but are available in the top-level `wdk_sys` module.

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

    include!(concat!(env!("OUT_DIR"), "/filesystem.rs"));
}
pub use bindings::*;
