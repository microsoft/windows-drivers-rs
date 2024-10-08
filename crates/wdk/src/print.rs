// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

// Note: DbgPrint can work in <= DIRQL, so there is no reason using alloc
// crate which may limit the debug printer to work in <= DISPATCH_IRQL.
use core::fmt;

// We will allocate the format buffer on stack instead of heap
// so that debug printer won't be subject to DISPATCH_IRQL restriction.
struct DebugPrintFormatBuffer {
    // Limit buffer to 512 bytes because DbgPrint can only transport 512 bytes per call.
    // See: https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/reading-and-filtering-debugging-messages
    buffer: [u8; 512],
    used: usize,
}

impl DebugPrintFormatBuffer {
    fn new() -> Self {
        DebugPrintFormatBuffer {
            buffer: [0; 512],
            used: 0,
        }
    }
}

impl fmt::Write for DebugPrintFormatBuffer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let remainder = &mut self.buffer[self.used..];
        let current = s.as_bytes();
        if remainder.len() < current.len() {
            return Err(fmt::Error);
        }
        remainder[..current.len()].copy_from_slice(current);
        self.used += current.len();
        return Ok(());
    }
}

/// print to kernel debugger via [`wdk_sys::ntddk::DbgPrint`]
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
      ($crate::_print(format_args!($($arg)*)))
    };
}

/// print with newline to debugger via [`wdk_sys::ntddk::DbgPrint`]
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
    // Use stack-based formatter. Avoid heap allocation.
    let mut w = DebugPrintFormatBuffer::new();
    let r = fmt::write(&mut w, args);
    if let Ok(_) = r {
        let formatted_string = &mut w.buffer;
        let formatted_string_pointer = formatted_string.as_ptr() as *const i8;
        // No need to append a null-terminator in that the formatted string buffer was zero-initialized.
        unsafe {
            #[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
            {
                // Use "%s" to prevent the system from reformatting our message.
                // It's possible the message can contain keywords like "%s" "%d" etc.
                wdk_sys::ntddk::DbgPrint("%s\0".as_ptr() as *const i8, formatted_string_pointer);
            }

            #[cfg(driver_model__driver_type = "UMDF")]
            {
                wdk_sys::windows::OutputDebugStringA(formatted_string_pointer);
            }
        }
    }
}
