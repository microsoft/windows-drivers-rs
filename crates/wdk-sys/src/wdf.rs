// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Direct FFI bindings to WDF APIs from the Windows Driver Kit (WDK)

#[allow(missing_docs)]
#[allow(clippy::unreadable_literal)]
mod bindings {
    // allow wildcards for types module since underlying c code relies on all
    // type definitions being in scope
    #[allow(clippy::wildcard_imports)]
    use crate::types::*;

    include!(concat!(env!("OUT_DIR"), "/wdf.rs"));
}
pub use bindings::*;
