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
    #[allow(
        clippy::wildcard_imports,
        reason = "the underlying c code relies on all type definitions being in scope, which \
                  results in the bindgen generated code relying on the generated types being in \
                  scope as well"
    )]
    use crate::types::*;

    include!(concat!(env!("OUT_DIR"), "/wdf.rs"));
}

// This is a workaround to expose the generated function count to the
// `call_unsafe_wdf_function_binding` proc-macro, so that the macro-generated
// code can determine the slice size at runtime. When we are able to
// conditionally compile based off a cfg range for WDF version, this module
// can be removed and the runtime check can be replaced with a conditional
// compilation: https://github.com/microsoft/windows-drivers-rs/issues/276
#[doc(hidden)]
pub mod __private {
    include!(concat!(env!("OUT_DIR"), "/wdf_function_count.rs"));
}
