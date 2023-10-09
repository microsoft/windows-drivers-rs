// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Direct FFI bindings to WDF APIs from the Windows Driver Kit (WDK)

use crate::types::ULONG;

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

// FIXME: UMDF >= 2.25 & KMDF >= 1.25 define this in wdffuncenum with
// _declspec(selectany) so they don't generate symbols
#[no_mangle]
static WdfMinimumVersionRequired: ULONG = 33;
