// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Tests for `wdk-sys` that rely on a WDK configuration being
//! present in the build graph.
//!
//! The crate-level example below doubles as a **doctest**.
//! `cargo test` compiles and links each doctest as its own user-mode binary. It
//! links against `wdk-sys`, which is pulled in here with the `test-stubs`
//! feature (see this crate's dependencies). That feature cfg-suppresses the
//! generated WDK `#[link]` directives, so the doctest links and runs without
//! pulling kernel-mode libraries into it.
//!
//! Like the driver unit test in `tests/mixed-package-kmdf-workspace`, it
//! references a bindgen generated WDM type (`DRIVER_OBJECT`) via a const
//! `size_of` — so the doctest actually links `wdk-sys`'s generated bindings
//!  while touching only a type (never a KM function
//! binding that `test-stubs` leaves unlinked).
//!
//! ```
//! use wdk_sys::{DRIVER_OBJECT, NT_SUCCESS, STATUS_SUCCESS, ULONG};
//!
//! assert!(NT_SUCCESS(STATUS_SUCCESS));
//! assert!(core::mem::size_of::<DRIVER_OBJECT>() <= ULONG::MAX as usize);
//! ```
