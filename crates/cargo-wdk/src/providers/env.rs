// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! This module provides a wrapper around a subset of `std::env` methods,
//! offering a simplified and testable interface for common env related
//! operations such as reading.
//! It also integrates with `mockall` to enable mocking for unit tests.

// Warns the methods are not used, however they are used.
// The intellisense confusion seems to come from automock
#![allow(dead_code)]
#![allow(clippy::unused_self)]

use mockall::automock;

/// Provides limited access to `std::env` methods
#[derive(Default)]
pub struct Env {}

#[automock]
impl Env {
    pub fn var(&self, var: &str) -> Result<String, std::env::VarError> {
        std::env::var(var)
    }
}
