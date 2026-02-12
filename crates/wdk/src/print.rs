// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

use core::fmt;
#[cfg(driver_model__driver_type = "UMDF")]
use std::ffi::CString;

use crate::fmt::WdkFormatBuffer;

/// Prints to the debugger.
///
/// Equivalent to the println! macro except that a newline is not printed at the
/// end of the message.
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
    doc = r#"
The output is routed to the debugger via [`wdk_sys::windows::OutputDebugStringA`].

If there is no debugger attached to WUDFHost of the driver (i.e., user-mode debugging),
the output will be routed to the system debugger (i.e., kernel-mode debugging).
"#
)]
/// See the formatting documentation in [`core::fmt`] for details of the macro
/// argument syntax.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
      ($crate::_print(format_args!($($arg)*)))
    };
}

/// Prints to the debugger, with a newline.
///
/// This macro uses the same syntax as [`core::format!`], but writes to the
/// debugger instead. See [`core::fmt`] for more information.
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
/// See the formatting documentation in [`core::fmt`] for details of the macro
/// argument syntax.
#[macro_export]
macro_rules! println {
    () => {
      ($crate::print!("\n"));
    };

    ($($arg:tt)*) => {
      ($crate::print!("{}\n", format_args!($($arg)*)))
    };
}

/// Internal implementation of print macros. This function is an implementation
/// detail and should never be called directly, but must be public to be useable
/// by the print! and println! macro
///
/// # Panics
///
/// Panics if an internal null byte is passed in
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    cfg_if::cfg_if! {
        if #[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))] {
            let mut buffered_writer: WdkFormatBuffer = WdkFormatBuffer::new();
            // We do not care whether this is `Ok` or `Err` right now. If `Err` will simply write all until overflow.
            // TODO: create custom `flush` param in `WdkFormatBuffer` to preserve old functionality
            let _ = fmt::write(&mut buffered_writer, args);

            let cstr_buffer = match buffered_writer.as_cstr() {
                Ok(cstr) => cstr,
                Err(_e) => return, // silently return on error, no null terminator. (Should this be a placeholder string for debugging purposes?)
            };

            unsafe {
                wdk_sys::ntddk::DbgPrint(
                    c"%s".as_ptr().cast(),
                    cstr_buffer.as_ptr().cast::<wdk_sys::CHAR>(),
                );
            }


        } else if #[cfg(driver_model__driver_type = "UMDF")] {
            match CString::new(format!("{args}")) {
                Ok(c_string) => {
                    // SAFETY: `CString` guarantees a valid null-terminated string
                    unsafe {
                        wdk_sys::windows::OutputDebugStringA(c_string.as_ptr());
                    }
                },
                Err(nul_error) => {
                    let nul_position = nul_error.nul_position();
                    let string_vec = nul_error.into_vec();
                    let c_string = CString::new(&string_vec[..nul_position]).expect("string_vec[..nul_position] should have no internal null bytes");
                    let remaining_string = String::from_utf8(string_vec[nul_position+1 ..].to_vec()).expect("string_vec should always be valid UTF-8 because `format!` returns a String");

                    // SAFETY: `CString` guarantees a valid null-terminated string
                    unsafe {
                        wdk_sys::windows::OutputDebugStringA(c_string.as_ptr());
                    }

                    print!("{remaining_string}");
                }
            }
        }
    }
}
