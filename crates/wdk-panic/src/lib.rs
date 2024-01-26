// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Default Panic Handlers for programs built with the WDK (Windows Drivers Kit)
#![no_std]
#![deny(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::cargo)]
#![deny(clippy::multiple_unsafe_ops_per_block)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![deny(clippy::unnecessary_safety_doc)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![deny(rustdoc::missing_crate_level_docs)]
#![deny(rustdoc::invalid_codeblock_attributes)]
#![deny(rustdoc::invalid_html_tags)]
#![deny(rustdoc::invalid_rust_codeblocks)]
#![deny(rustdoc::bare_urls)]
#![deny(rustdoc::unescaped_backticks)]
#![deny(rustdoc::redundant_explicit_links)]

#[cfg(not(test))]
use core::panic::PanicInfo;

#[cfg(all(
    debug_assertions,
    // Disable inclusion of panic handlers when compiling tests for wdk crate
    not(test)
))]
#[panic_handler]
const fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[cfg(all(
    not(debug_assertions),
    // Disable inclusion of panic handlers when compiling tests for wdk crate
    not(test)
))]
#[panic_handler]
const fn panic(_info: &PanicInfo) -> ! {
    loop {}
    // FIXME: Should this trigger Bugcheck via KeBugCheckEx?
}
