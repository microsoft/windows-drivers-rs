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
    doc = r#"
The output is routed to the debugger via [`wdk_sys::ntddk::DbgPrint`], so the `IRQL` 
requirements of that function apply. In particular, this should only be called at 
`IRQL` <= `DIRQL`, and calling it at `IRQL` > `DIRQL` can cause deadlocks due to
the debugger's use of IPIs (Inter-Process Interrupts).

[`wdk_sys::ntddk::DbgPrint`]'s 512 byte limit does not apply to this macro, as it will
automatically buffer and chunk the output if it exceeds that limit.
"#
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
    doc = r#"
The output is routed to the debugger via [`wdk_sys::ntddk::DbgPrint`], so the `IRQL` 
requirements of that function apply. In particular, this should only be called at 
`IRQL` <= `DIRQL`, and calling it at `IRQL` > `DIRQL` can cause deadlocks due to
the debugger's use of IPIs (Inter-Process Interrupts).

[`wdk_sys::ntddk::DbgPrint`]'s 512 byte limit does not apply to this macro, as it will
automatically buffer and chunk the output if it exceeds that limit.
"#
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
            let mut buffered_writer = dbg_print_buf_writer::DbgPrintBufWriter::new();

            if let Ok(_) = fmt::write(&mut buffered_writer, args) {
                buffered_writer.flush();
            } else {
                unreachable!("DbgPrintBufWriter should never fail write");
            }

        } else if #[cfg(driver_model__driver_type = "UMDF")] {
            let formatted_string = CString::new(format!("{args}"))
                .expect("CString should be able to be created from a String."); // TODO: remove panic

            // SAFETY: `CString` guarantees that `formatted_string` is a valid null terminated string
            unsafe {
                wdk_sys::windows::OutputDebugStringA(formatted_string.as_ptr());
            }
        }
    }
}

#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
mod dbg_print_buf_writer {

    // Note: DbgPrint can work in <= DIRQL, so there is no reason using alloc
    // crate which may limit the debug printer to work in <= DISPATCH_IRQL.
    // TODO: move this comment

    use core::fmt;

    /// Max size that can be transmitted by DbgPrint in single call:
    /// https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/reading-and-filtering-debugging-messages#dbgprint-buffer-and-the-debugger
    const DBG_PRINT_MAX_TXN_SIZE: usize = 512;

    // We will allocate the format buffer on stack instead of heap
    // so that debug printer won't be subject to DISPATCH_IRQL restriction.

    /// Stack-based format buffer for DbgPrint
    ///
    /// This buffer is used to format strings via `fmt::write` without needing
    /// heap allocations. Whenever a new string would cause the buffer to exceed
    /// its max capacity, it will first empty its buffer via `DbgPrint`.
    pub(crate) struct DbgPrintBufWriter {
        buffer: [u8; DBG_PRINT_MAX_TXN_SIZE],
        used: usize,
    }

    impl Default for DbgPrintBufWriter {
        fn default() -> Self {
            Self {
                buffer: [0; DBG_PRINT_MAX_TXN_SIZE],
                used: 0,
            }
        }
    }

    impl fmt::Write for DbgPrintBufWriter {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            let mut str_byte_slice = s.as_bytes();
            let mut remaining_buffer = &mut self.buffer[self.used..Self::USABLE_BUFFER_SIZE];
            let mut remaining_buffer_size = remaining_buffer.len();

            // If the string is too large for the buffer, keep chunking the string and
            // flushing the buffer until the entire string is handled
            while str_byte_slice.len() > remaining_buffer_size {
                // Fill buffer
                remaining_buffer[..].copy_from_slice(&str_byte_slice[..remaining_buffer_size]);

                // Flush buffer
                self.flush();

                // Update remaining string slice to handle and reset remaining buffer
                str_byte_slice = &str_byte_slice[remaining_buffer_size..];
                remaining_buffer = &mut self.buffer[self.used..];
                remaining_buffer_size = remaining_buffer.len();
            }
            remaining_buffer[..str_byte_slice.len()].copy_from_slice(str_byte_slice);
            self.used += str_byte_slice.len();

            Ok(())
        }
    }

    impl DbgPrintBufWriter {
        /// The maximum size of the buffer that can be used for formatting
        /// strings
        ///
        /// The last byte is reserved for the null terminator
        const USABLE_BUFFER_SIZE: usize = DBG_PRINT_MAX_TXN_SIZE - 1;

        pub fn new() -> Self {
            Self::default()
        }

        pub fn flush(&mut self) {
            // TODO: some comment here about null term and guarantee the format specifier is
            // valid
            unsafe {
                // Pass the formatted string to DbgPrint with "%s" format specifier.
                // This prevents DbgPrint from interpreting format specifiers within our
                // message, which could cause buffer overflows or crashes.
                wdk_sys::ntddk::DbgPrint(
                    c"%s".as_ptr().cast(),
                    self.buffer.as_ptr().cast::<wdk_sys::PCSTR>(),
                );
            }

            self.used = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
    mod dbg_print_buf_writer {
        use super::*;
    }
}
