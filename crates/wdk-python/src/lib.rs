// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Python support for the Windows Driver Kit (WDK).
//!
//! Closes <https://github.com/microsoft/windows-drivers-rs/issues/635>.
//!
//! # Example
//!
//! ```python
//! import wdk
//! wdk.driver_entry()
//! ```
//!
//! Just kidding. Use Rust.

/// The Python driver source code, embedded at compile time for your convenience.
pub const DRIVER_SOURCE: &str = include_str!("../driver.py");
