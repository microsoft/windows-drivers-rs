// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

/// Prints and returns the value of a given expression for quick and dirty
/// debugging.
/// This is the no_std equivalent of the std library's dbg! macro.
/// Instead of writing to stderr it routes output through the debugger using
/// the println! macro in wdk.
#[cfg_attr(
    any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"),
    doc = r"
The output is routed to the debugger via [`wdk_sys::ntddk::DbgPrint`], so the `IRQL`
requirements of that function apply. In particular, this should only be called at
`IRQL` <= `DIRQL`, and calling it at `IRQL` > `DIRQL` can cause deadlocks due to
the debugger's use of IPIs (Inter-Process Interrupts).

[`wdk_sys::ntddk::DbgPrint`]'s 512 byte limit does not apply to this macro, as it will
automatically buffer and chunk the output if it exceeds that limit.
"
)]
#[cfg_attr(
    driver_model__driver_type = "UMDF",
    doc = r"
The output is routed to the debugger via [`wdk_sys::windows::OutputDebugStringA`].

If there is no debugger attached to WUDFHost of the driver (i.e., user-mode debugging),
the output will be routed to the system debugger (i.e., kernel-mode debugging).
"
)]
#[macro_export]
macro_rules! dbg {
    // NOTE: We cannot use `concat!` to make a static string as a format argument
    // of `println!` because `file!` could contain a `{` or
    // `$val` expression could be a block (`{ .. }`), in which case the `println!`
    // will be malformed.
    // TODO: Consider replacing `println!` with a no_std implementation of `eprintln!`
    // to target different debug message levels.
    () => {
        $crate::println!("[{}:{}:{}]", core::file!(), core::line!(), core::column!())
    };
    ($val:expr $(,)?) => {
        // Use of `match` here is intentional because it affects the lifetimes
        // of temporaries - https://stackoverflow.com/a/48732525/1063961
        match $val {
            tmp => {
                $crate::println!(
                    "[{}:{}:{}] {} = {:#?}",
                    core::file!(),
                    core::line!(),
                    core::column!(),
                    core::stringify!($val),
                    // The `&T: Debug` check happens here (not in the format literal desugaring)
                    // to avoid format literal related messages and suggestions.
                    &&tmp as &dyn core::fmt::Debug,
                );
                tmp
            }
        }
    };
    ($($val:expr),+ $(,)?) => {
        ($($crate::dbg!($val)),+,)
    };
}
