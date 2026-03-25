// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

use core::fmt;
#[cfg(driver_model__driver_type = "UMDF")]
use std::ffi::CString;

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
automatically buffer and chunk the output if it exceeds that limit. Interior NUL bytes
in the formatted output will cause each chunk to be truncated at the first NUL.
"
)]
#[cfg_attr(
    driver_model__driver_type = "UMDF",
    doc = r#"
The output is routed to the debugger via [`wdk_sys::windows::OutputDebugStringA`].

If there is no debugger attached to WUDFHost of the driver (i.e., user-mode debugging),
the output will be routed to the system debugger (i.e., kernel-mode debugging).

Interior NUL bytes in the formatted output will be stripped and the remaining
content will be printed.
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
automatically buffer and chunk the output if it exceeds that limit. Interior NUL bytes
in the formatted output will cause each chunk to be truncated at the first NUL.
"
)]
#[cfg_attr(
    driver_model__driver_type = "UMDF",
    doc = r"
The output is routed to the debugger via [`wdk_sys::windows::OutputDebugStringA`].

If there is no debugger attached to WUDFHost of the driver (i.e., user-mode debugging),
the output will be routed to the system debugger (i.e., kernel-mode debugging).

Interior NUL bytes in the formatted output will be stripped and the remaining
content will be printed.
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
automatically buffer and chunk the output if it exceeds that limit. Interior NUL bytes
in the formatted output will cause each chunk to be truncated at the first NUL.
"
)]
#[cfg_attr(
    driver_model__driver_type = "UMDF",
    doc = r"
The output is routed to the debugger via [`wdk_sys::windows::OutputDebugStringA`].

If there is no debugger attached to WUDFHost of the driver (i.e., user-mode debugging),
the output will be routed to the system debugger (i.e., kernel-mode debugging).

Interior NUL bytes in the formatted output will be stripped and the remaining
content will be printed.
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

/// Internal implementation of print macros. This function is an implementation
/// detail and should never be called directly, but must be public to be useable
/// by the print! and println! macro
///
/// Interior NUL bytes in the formatted output are handled differently per
/// driver model: WDM/KMDF truncates at the first NUL (via `as_cstr()`),
/// while UMDF strips NUL bytes and prints the remaining content.
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    cfg_if::cfg_if! {
        if #[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))] {
            let mut writer = crate::WdkFlushableFormatBuffer::<_, 512>::new(|buf| {
                let cstr = buf.as_cstr();

                // SAFETY:
                // - `c"%s"` is a compile-time NUL-terminated format literal.
                // - `cstr` is a valid NUL-terminated CStr from `WdkFormatBuffer::as_cstr`.
                // - Using `%s` prevents `DbgPrint` from interpreting format specifiers
                //   in the buffer contents, which could cause UB.
                // - IRQL requirements (must be <= DIRQL) are the caller's responsibility,
                //   as documented on the print! macro.
                unsafe {
                    wdk_sys::ntddk::DbgPrint(
                        c"%s".as_ptr().cast(),
                        cstr.as_ptr().cast::<wdk_sys::CHAR>(),
                    );
                }
            });

            // For N=512, write_str cannot fail: the largest UTF-8 code point
            // is 4 bytes, which always fits in the 511-byte capacity. Overflow
            // is handled by flushing. Errors from Display impls are silently
            // dropped — partial output is acceptable for debug printing.
            let _ = fmt::write(&mut writer, args);
            writer.flush();

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
