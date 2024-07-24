// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Direct FFI bindings to WDF APIs from the Windows Driver Kit (WDK)
//!
//! This module contains all bindings to functions, constants, methods,
//! constructors and destructors in `wdf.h`. Types are not included in this
//! module, but are available in the top-level `wdk_sys` module.

pub use bindings::*;

#[allow(missing_docs)]
#[allow(clippy::unreadable_literal)]
mod bindings {
    // allow wildcards for types module since underlying c code relies on all
    // type definitions being in scope
    #[allow(clippy::wildcard_imports)]
    use crate::types::*;

    include!(concat!(env!("OUT_DIR"), "/wdf.rs"));
}

include!(concat!(env!("OUT_DIR"), "/wdf_function_table.rs"));
