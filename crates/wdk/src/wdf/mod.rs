//! Safe abstractions over WDF APIs

mod spinlock;
mod timer;

pub use spinlock::*;
pub use timer::*;
