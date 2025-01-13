// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Direct FFI bindings to HID APIs from the Windows Driver Kit (WDK)
//!
//! This module contains all bindings to functions, constants, methods,
//! constructors and destructors in the following headers: `hidpddi.h`,
//! `hidport.h`, `HidSpiCx/1.0/hidspicx.h`, `kbdmou.h`, `ntdd8042.h`,
//! `hidclass.h`, `hidsdi.h`, `hidpi.h`, `vhf.h`. Types are not included in this
//! module, but are available in the top-level `wdk_sys` module.

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

    include!(concat!(env!("OUT_DIR"), "/hid.rs"));
}
pub use bindings::*;
