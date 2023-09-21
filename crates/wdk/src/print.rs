// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

extern crate alloc;

use alloc::ffi::CString;

use wdk_sys::ntddk::DbgPrint;

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
      ($crate::_print(format_args!($($arg)*)))
    };
}

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
pub fn _print(args: core::fmt::Arguments) {
    let formatted_string = CString::new(alloc::format!("{args}"))
        .expect("CString should be able to be created from a String.");

    unsafe {
        DbgPrint(formatted_string.as_ptr());
    }
}
