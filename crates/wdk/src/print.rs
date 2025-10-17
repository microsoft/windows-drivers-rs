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
            let mut buffered_writer = dbg_print_buf_writer::DbgPrintBufWriter::new();

            if fmt::write(&mut buffered_writer, args).is_ok() {
                buffered_writer.flush();
            } else {
                unreachable!("DbgPrintBufWriter should never fail to write");
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

#[cfg(any(driver_model__driver_type = "WDM", driver_model__driver_type = "KMDF"))]
mod dbg_print_buf_writer {
    use core::fmt;

    /// Max size that can be transmitted by `DbgPrint` in single call:
    /// <https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/reading-and-filtering-debugging-messages#dbgprint-buffer-and-the-debugger>
    const DBG_PRINT_MAX_TXN_SIZE: usize = 512;

    /// Stack-based format buffer for `DbgPrint`
    ///
    /// This buffer is used to format strings via `fmt::write` without needing
    /// heap allocations. Whenever a new string would cause the buffer to exceed
    /// its max capacity, it will first empty its buffer via `DbgPrint`.
    /// The use of a stack-based buffer instead of `alloc::format!` allows for
    /// printing at IRQL <= DIRQL.
    pub struct DbgPrintBufWriter {
        buffer: [u8; DBG_PRINT_MAX_TXN_SIZE],
        used: usize,
    }

    impl Default for DbgPrintBufWriter {
        fn default() -> Self {
            Self {
                // buffer is initialized to all null
                buffer: [0; DBG_PRINT_MAX_TXN_SIZE],
                used: 0,
            }
        }
    }

    impl fmt::Write for DbgPrintBufWriter {
        // Traverses the string and writes all non-null bytes to the buffer.
        // If the buffer is full, flushes the buffer and continues writing.
        // Finishes with a non-flushed buffer containing the last
        // non-null bytes of the string.
        fn write_str(&mut self, s: &str) -> fmt::Result {
            let mut str_byte_slice = s.as_bytes();
            let mut remaining_buffer = &mut self.buffer[self.used..Self::USABLE_BUFFER_SIZE];
            let mut remaining_buffer_len = remaining_buffer.len();

            str_byte_slice = advance_slice_to_next_non_null_byte(str_byte_slice);

            while !str_byte_slice.is_empty() {
                // Get size of next chunk of string to write and copy to buffer.
                // Chunk is bounded by either the first null byte or the remaining buffer size.
                let chunk_size = str_byte_slice
                    .iter()
                    .take(remaining_buffer_len)
                    .take_while(|c| **c != b'\0')
                    .count();
                remaining_buffer[..chunk_size].copy_from_slice(&str_byte_slice[..chunk_size]);
                str_byte_slice = &str_byte_slice[chunk_size..];

                str_byte_slice = advance_slice_to_next_non_null_byte(str_byte_slice);
                self.used += chunk_size;

                // Flush buffer if full, otherwise update amount used
                if chunk_size == remaining_buffer_len && !str_byte_slice.is_empty() {
                    self.flush();
                }

                remaining_buffer = &mut self.buffer[self.used..Self::USABLE_BUFFER_SIZE];
                remaining_buffer_len = remaining_buffer.len();
            }
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

        // Null-terminates the buffer and calls `DbgPrint` with the buffer contents.
        // Resets `self.used` to 0 after flushing.
        pub fn flush(&mut self) {
            // Escape if the buffer is empty
            if self.used == 0 {
                return;
            }

            // Null-terminate the string
            self.buffer[self.used] = 0;

            // SAFETY: This is safe because:
            // 1. `self.buffer` contains a valid C-style string with the data placed in
            //    [0..self.used] by the `write_str` implementation
            // 2. The `write_str` method ensures `self.used` never exceeds
            //    `USABLE_BUFFER_SIZE`, leaving the last byte available for null termination
            // 3. The "%s" format specifier is used as a literal string to prevent
            //    `DbgPrint` from interpreting format specifiers in the message, which could
            //    lead to memory corruption or undefined behavior if the buffer contains
            //    printf-style formatting characters
            unsafe {
                wdk_sys::ntddk::DbgPrint(
                    c"%s".as_ptr().cast(),
                    self.buffer.as_ptr().cast::<wdk_sys::CHAR>(),
                );
            }

            self.used = 0;
        }
    }

    // Helper function to advance the start of a `u8` slice to the next non-null
    // byte. Returns an empty slice if all bytes are null.
    fn advance_slice_to_next_non_null_byte(slice: &[u8]) -> &[u8] {
        slice
            .iter()
            .position(|&b| b != b'\0')
            .map_or_else(|| &slice[slice.len()..], |pos| &slice[pos..])
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::print::dbg_print_buf_writer::DbgPrintBufWriter;

        #[test]
        fn write_that_fits_buffer() {
            const TEST_STRING: &str = "Hello, world!";
            const TEST_STRING_LEN: usize = TEST_STRING.len();

            let mut writer = DbgPrintBufWriter::new();
            fmt::write(&mut writer, format_args!("{TEST_STRING}"))
                .expect("fmt::write should succeed");
            assert_eq!(writer.used, TEST_STRING_LEN);
            assert_eq!(&writer.buffer[..writer.used], TEST_STRING.as_bytes());
            let old_used = writer.used;
            writer.flush();
            // FIXME: When this test is compiled, rustc automatically links the
            // usermode-version of DbgPrint. We should either figure out a way to prevent
            // this in order to stub in a mock implementation via something like `mockall`,
            // or have `DbgPrintBufWriter` be able to be instantiated with a different
            // implementation somehow. Ex. `DbgPrintBufWriter::new` can take in a closure
            // that gets called for flushing (real impl uses Dbgprint and test impl uses a
            // mock with a counter and some way to validate contents being sent to the flush
            // closure)

            // Check that the buffer is empty after flushing
            assert_eq!(writer.used, 0);
            // Check that the string is null-terminated at the end of the buffer.
            assert_eq!(writer.buffer[old_used], b'\0');
            // Check that the string isn't null-terminated at the beginning of the buffer.
            assert_ne!(writer.buffer[0], b'\0');
        }

        #[test]
        fn write_that_exceeds_buffer() {
            const TEST_STRING: &str =
                "This is a test string that exceeds the buffer size limit set for \
                 DbgPrintBufWriter. It should trigger multiple flushes to handle the overflow \
                 correctly. The buffer has a limited capacity of 511 bytes (512 minus 1 for null \
                 terminator), and this string is intentionally much longer. When writing this \
                 string to the DbgPrintBufWriter, the implementation should automatically chunk \
                 the content and flush each chunk separately. This ensures large debug messages \
                 can be properly displayed without being truncated. The current implementation \
                 handles this by filling the buffer as much as possible, flushing it using \
                 DbgPrint, then continuing with the remaining content until everything is \
                 processed. This approach allows debugging messages of arbitrary length without \
                 requiring heap allocations, which is particularly important in kernel mode where \
                 memory allocation constraints might be stricter. This test verifies that strings \
                 larger than the max buffer size are handled correctly, confirming that our \
                 buffer management logic works as expected. This string is now well over 1000 \
                 characters long to ensure that the DbgPrintBufWriter's buffer overflow handling \
                 is thoroughly tested.";
            const TEST_STRING_LEN: usize = TEST_STRING.len();
            const UNFLUSHED_STRING_CONTENTS_STARTING_INDEX: usize =
                TEST_STRING_LEN - (TEST_STRING_LEN % DbgPrintBufWriter::USABLE_BUFFER_SIZE);

            const {
                assert!(
                    TEST_STRING_LEN > DbgPrintBufWriter::USABLE_BUFFER_SIZE,
                    "TEST_STRING_LEN should be greater than buffer size for this test"
                );
            }

            let expected_unflushed_string_contents =
                &TEST_STRING[UNFLUSHED_STRING_CONTENTS_STARTING_INDEX..];

            let mut writer = DbgPrintBufWriter::new();
            fmt::write(&mut writer, format_args!("{TEST_STRING}"))
                .expect("fmt::write should succeed");
            assert_eq!(writer.used, expected_unflushed_string_contents.len());
            assert_eq!(
                &writer.buffer[..writer.used],
                expected_unflushed_string_contents.as_bytes()
            );
            let expected_null_byte_position = writer.used;
            // FIXME: When this test is compiled, rustc automatically links the
            // usermode-version of DbgPrint. We should either figure out a way to prevent
            // this in order to stub in a mock implementation via something like `mockall`,
            // or have `DbgPrintBufWriter` be able to be instantiated with a different
            // implementation somehow. Ex. `DbgPrintBufWriter::new` can take in a closure
            // that gets called for flushing (real impl uses Dbgprint and test impl uses a
            // mock with a counter and some way to validate contents being sent to the flush
            // closure)

            writer.flush();
            assert_eq!(writer.used, 0);
            assert_eq!(writer.buffer[expected_null_byte_position], 0);
        }

        #[test]
        fn write_that_exceeds_buffer_prints_all() {
            const TEST_STRING: &str =
                "This is a test string that exceeds the buffer size limit set for \
                 DbgPrintBufWriter. It should trigger multiple flushes to handle the overflow \
                 correctly. The buffer has a limited capacity of 511 bytes (512 minus 1 for null \
                 terminator), and this string is intentionally much longer. When writing this \
                 string to the DbgPrintBufWriter, the implementation should automatically chunk \
                 the content and flush each chunk separately. This ensures large debug messages \
                 can be properly displayed without being truncated. The current implementation \
                 handles this by filling the buffer as much as possible, flushing it using \
                 DbgPrint, then continuing with the remaining content until everything is \
                 processed. This approach allows debugging messages of arbitrary length without \
                 requiring heap allocations, which is particularly important in kernel mode where \
                 memory allocation constraints might be stricter. This test verifies that strings \
                 larger than the max buffer size are handled correctly, confirming that our \
                 buffer management logic works as expected. This string is now well over 1000 \
                 characters long to ensure that the DbgPrintBufWriter's buffer overflow handling \
                 is thoroughly tested.";
            const TEST_STRING_LEN: usize = TEST_STRING.len();

            const {
                assert!(
                    TEST_STRING_LEN > DbgPrintBufWriter::USABLE_BUFFER_SIZE,
                    "TEST_STRING_LEN should be greater than buffer size for this test"
                );
            }

            let mut writer = DbgPrintBufWriter::new();

            // set the last byte to 1 to ensure that the buffer is not automatically
            // null-terminated when full
            writer.buffer[DBG_PRINT_MAX_TXN_SIZE - 1] = 1;
            fmt::write(&mut writer, format_args!("{TEST_STRING}"))
                .expect("fmt::write should succeed");

            // if the last byte has been changed to the null terminator, we know that the
            // buffer was flushed with overflow correctly.
            assert_eq!(writer.buffer[DBG_PRINT_MAX_TXN_SIZE - 1], b'\0');
        }

        #[test]
        fn write_string_with_null_char_beginning() {
            const TEST_STRING: &str = "\0Hello, world!This is a test string with a null byte.";
            const TEST_STRING_NULL_REMOVED: &str =
                "Hello, world!This is a test string with a null byte.";
            const TEST_STRING_LEN: usize = TEST_STRING.len();
            const UNFLUSHED_STRING_CONTENTS_STARTING_INDEX: usize = TEST_STRING_LEN - 1;

            let mut writer = DbgPrintBufWriter::new();
            fmt::write(&mut writer, format_args!("{TEST_STRING}"))
                .expect("fmt::write should succeed");
            assert_eq!(writer.used, UNFLUSHED_STRING_CONTENTS_STARTING_INDEX);
            assert_eq!(
                &writer.buffer[..writer.used],
                TEST_STRING_NULL_REMOVED.as_bytes()
            );
            writer.flush();
            assert_eq!(writer.used, 0);
        }

        #[test]
        fn write_string_with_null_char_middle() {
            const TEST_STRING: &str = "Hello, world!\0This is a test string with a null byte.";
            const TEST_STRING_NULL_REMOVED: &str =
                "Hello, world!This is a test string with a null byte.";
            const TEST_STRING_LEN: usize = TEST_STRING.len();
            const UNFLUSHED_STRING_CONTENTS_STARTING_INDEX: usize = TEST_STRING_LEN - 1;

            let mut writer = DbgPrintBufWriter::new();
            fmt::write(&mut writer, format_args!("{TEST_STRING}"))
                .expect("fmt::write should succeed");
            assert_eq!(writer.used, UNFLUSHED_STRING_CONTENTS_STARTING_INDEX);
            assert_eq!(
                &writer.buffer[..writer.used],
                TEST_STRING_NULL_REMOVED.as_bytes()
            );
            writer.flush();
            assert_eq!(writer.used, 0);
        }

        #[test]
        fn write_string_with_null_char_end() {
            const TEST_STRING: &str = "Hello, world!This is a test string with a null byte.\0";
            const TEST_STRING_NULL_REMOVED: &str =
                "Hello, world!This is a test string with a null byte.";
            const TEST_STRING_LEN: usize = TEST_STRING.len();
            const UNFLUSHED_STRING_CONTENTS_STARTING_INDEX: usize = TEST_STRING_LEN - 1;

            let mut writer = DbgPrintBufWriter::new();
            fmt::write(&mut writer, format_args!("{TEST_STRING}"))
                .expect("fmt::write should succeed");
            assert_eq!(writer.used, UNFLUSHED_STRING_CONTENTS_STARTING_INDEX);
            assert_eq!(
                &writer.buffer[..writer.used],
                TEST_STRING_NULL_REMOVED.as_bytes()
            );
            writer.flush();
            assert_eq!(writer.used, 0);
        }

        #[test]
        fn write_string_with_null_char_beginning_middle_end() {
            const TEST_STRING: &str =
                "\0\0Hello, world!This is a\0\0 test string with a null byte.\0\0";
            const TEST_STRING_NULL_REMOVED: &str =
                "Hello, world!This is a test string with a null byte.";
            const TEST_STRING_LEN: usize = TEST_STRING.len();
            const UNFLUSHED_STRING_CONTENTS_STARTING_INDEX: usize = TEST_STRING_LEN - 6;

            let mut writer = DbgPrintBufWriter::new();
            fmt::write(&mut writer, format_args!("{TEST_STRING}"))
                .expect("fmt::write should succeed");
            assert_eq!(writer.used, UNFLUSHED_STRING_CONTENTS_STARTING_INDEX);
            assert_eq!(
                &writer.buffer[..writer.used],
                TEST_STRING_NULL_REMOVED.as_bytes()
            );
            writer.flush();
            assert_eq!(writer.used, 0);
        }

        #[test]
        fn write_null_string() {
            const TEST_STRING: &str = "\0";
            const TEST_STRING_NULL_REMOVED: &str = "";
            const UNFLUSHED_STRING_CONTENTS_STARTING_INDEX: usize = 0;

            let mut writer = DbgPrintBufWriter::new();
            fmt::write(&mut writer, format_args!("{TEST_STRING}"))
                .expect("fmt::write should succeed");
            assert_eq!(writer.used, UNFLUSHED_STRING_CONTENTS_STARTING_INDEX);
            assert_eq!(
                &writer.buffer[..writer.used],
                TEST_STRING_NULL_REMOVED.as_bytes()
            );
            writer.flush();
            assert_eq!(writer.used, 0);
        }

        #[test]
        fn write_max_buffer_string() {
            const TEST_STRING: &str = "sixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslon";
            assert_eq!(TEST_STRING.len(), DbgPrintBufWriter::USABLE_BUFFER_SIZE);

            let mut writer = DbgPrintBufWriter::new();
            fmt::write(&mut writer, format_args!("{TEST_STRING}"))
                .expect("fmt::write should succeed");
            assert_eq!(writer.used, DbgPrintBufWriter::USABLE_BUFFER_SIZE);
            assert_eq!(&writer.buffer[..writer.used], TEST_STRING.as_bytes());
            writer.flush();
            assert_eq!(writer.used, 0);
        }

        #[test]
        fn write_null_terminated_max_buffer_string() {
            const TEST_STRING: &str = "sixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslon\0";
            const TEST_STRING_WITHOUT_NULL_TERMINATION: &str = "sixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslon";
            assert_eq!(TEST_STRING.len(), DbgPrintBufWriter::USABLE_BUFFER_SIZE + 1);

            let mut writer = DbgPrintBufWriter::new();
            fmt::write(&mut writer, format_args!("{TEST_STRING}"))
                .expect("fmt::write should succeed");
            assert_eq!(writer.used, DbgPrintBufWriter::USABLE_BUFFER_SIZE);
            assert_eq!(
                &writer.buffer[..writer.used],
                TEST_STRING_WITHOUT_NULL_TERMINATION.as_bytes()
            );
            writer.flush();
            assert_eq!(writer.used, 0);
        }

        #[test]
        fn write_max_plus_one_buffer_string() {
            const TEST_STRING: &str = "sixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslong";
            const TEST_STRING_ENDING: &str = "g";
            assert_eq!(TEST_STRING.len(), DbgPrintBufWriter::USABLE_BUFFER_SIZE + 1);

            let mut writer = DbgPrintBufWriter::new();
            fmt::write(&mut writer, format_args!("{TEST_STRING}"))
                .expect("fmt::write should succeed");
            assert_eq!(writer.used, 1);
            assert_eq!(&writer.buffer[..writer.used], TEST_STRING_ENDING.as_bytes());
            writer.flush();
            assert_eq!(writer.used, 0);
        }

        #[test]
        fn write_max_plus_one_with_null_char_buffer_string() {
            const TEST_STRING: &str = "sixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslongsixteencharslon\0g";
            const TEST_STRING_ENDING: &str = "g";
            assert_eq!(TEST_STRING.len(), DbgPrintBufWriter::USABLE_BUFFER_SIZE + 2);

            let mut writer = DbgPrintBufWriter::new();
            fmt::write(&mut writer, format_args!("{TEST_STRING}"))
                .expect("fmt::write should succeed");
            assert_eq!(writer.used, 1);
            assert_eq!(&writer.buffer[..writer.used], TEST_STRING_ENDING.as_bytes());
            writer.flush();
            assert_eq!(writer.used, 0);
        }
    }
}
