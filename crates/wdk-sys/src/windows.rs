// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Direct FFI bindings to WIN32 APIs from the Windows Driver Kit (WDK)
//!
//! This module contains all bindings to functions, constants, methods,
//! constructors and destructors in `windows.h`. Types are not included in this
//! module, but are available in the top-level `wdk_sys` module.

pub use bindings::*;

#[allow(missing_docs)]
mod bindings {
    #[allow(
        clippy::wildcard_imports,
        reason = "the underlying c code relies on all type definitions being in scope, which \
                  results in the bindgen generated code relying on the generated types being in \
                  scope as well"
    )]
    use crate::types::*;

    include!(concat!(env!("OUT_DIR"), "/windows.rs"));
}
