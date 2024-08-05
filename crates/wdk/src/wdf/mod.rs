// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Safe abstractions over WDF APIs

pub use spinlock::*;
pub use timer::*;

mod spinlock;
mod timer;
