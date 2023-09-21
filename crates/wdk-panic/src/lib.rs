// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

#![no_std]
#![deny(warnings)]
#![deny(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::cargo)]

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
