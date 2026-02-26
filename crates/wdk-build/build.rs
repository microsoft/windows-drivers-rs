// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Build script for the `wdk-build` crate.
//!
//! This provides a temporary fix for using `assert_matches!` in specific Rust
//! versions in the `wdk-build` crate.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(assert_matches_stabilized)");
    setup_assert_matches_stabilized_cfg();
}

// Custom attributes cannot be applied to expressions yet, so separate functions
// are required for `rustversion` conditional compilation: https://github.com/rust-lang/rust/issues/15701
// TODO: Remove the `setup_assert_matches_stabilized_cfg` feature and related
// code once the minimum supported Rust version is 1.95.0 or later.
#[rustversion::since(1.95.0)]
fn setup_assert_matches_stabilized_cfg() {
    println!("cargo::rustc-cfg=assert_matches_stabilized");
}

#[rustversion::before(1.95.0)]
const fn setup_assert_matches_stabilized_cfg() {}
