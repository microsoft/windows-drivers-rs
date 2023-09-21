// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

#[rustversion::nightly]
fn main() {
    println!("cargo:rustc-cfg=nightly_toolchain");
}

#[rustversion::not(nightly)]
fn main() {}
