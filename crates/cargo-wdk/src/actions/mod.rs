// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0
//! This module contains the core functionality of the cargo-wdk utility. It
//! include modules which implement the business logic and common types that can
//! be shared across different actions. The `action` modules that implement the
//! business logic of the cargo-wdk utility are:
//! * `new` - New action module
//! * `build` - Build action module
//! * `clean` - Clean action module
pub mod build;
pub mod clean;
pub mod new;
