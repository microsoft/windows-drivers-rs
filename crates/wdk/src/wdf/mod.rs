// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Safe abstractions over WDF APIs

mod spinlock;
mod timer;

pub use spinlock::*;
pub use timer::*;
