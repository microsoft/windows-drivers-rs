// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

#![allow(missing_docs)]

// allow wildcards for types module since underlying c code relies on all
// type definitions being in scope
#[allow(clippy::wildcard_imports)]
use crate::types::*;

include!(concat!(env!("OUT_DIR"), "/ntddk.rs"));
