// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Build script for the `wdk-build` crate
//!
//! This provides a `nightly_feature` to the `wdk-build` crate, so that it can
//! conditionally enable nightly features.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(nightly_toolchain)");
    setup_nightly_cfgs();
}

// Custom attributes cannot be applied to expressions yet, so separate functions are required for nightly/non-nightly: https://github.com/rust-lang/rust/issues/15701
#[rustversion::nightly]
fn setup_nightly_cfgs() {
    println!("cargo::rustc-cfg=nightly_toolchain");
}

#[rustversion::not(nightly)]
const fn setup_nightly_cfgs() {}
