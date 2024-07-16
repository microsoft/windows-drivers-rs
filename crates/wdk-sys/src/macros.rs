// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Macros for use in the `wdk-sys` crate. This is especially useful for
//! interacting with WDK apis which are inlined, and so are impossible to
//! generate with [bindgen](https://docs.rs/bindgen/latest/bindgen/).

#[cfg(any(driver_model__driver_type = "KMDF", driver_model__driver_type = "UMDF"))]
mod wdf {
    include!(concat!(
        env!("OUT_DIR"),
        "/call_unsafe_wdf_function_binding.rs"
    ));
}
