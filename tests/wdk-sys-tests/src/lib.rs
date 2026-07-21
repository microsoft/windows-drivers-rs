// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Tests for `wdk-sys` that rely on a WDK configuration being
//! present in the build graph.
//!
//! The crate-level example below doubles as a doctest.
//! `cargo test` compiles and links each doctest as its own binary.
//! The generated `#[link]` directives are cfg-suppressed, so the
//! doctest links and runs without pulling libraries into it.
//!
//! This references a bindgen generated type so the doctest actually links
//! generated bindings while touching only a type (never a KM function
//! binding that is unlinked).
//!
//! ```
//! use wdk_sys::{DRIVER_OBJECT, NT_SUCCESS, STATUS_SUCCESS, ULONG};
//!
//! assert!(NT_SUCCESS(STATUS_SUCCESS));
//! assert!(core::mem::size_of::<DRIVER_OBJECT>() <= ULONG::MAX as usize);
//! ```
