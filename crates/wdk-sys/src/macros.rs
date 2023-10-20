// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Macros for use in the `wdk-sys` crate. This is especially useful for
//! interacting with WDK apis which are inlined, and so are impossible to
//! generate with [bindgen](https://docs.rs/bindgen/latest/bindgen/).

pub use wdk_macros::*;
