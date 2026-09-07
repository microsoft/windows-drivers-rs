//! Synchronization primitives backed by kernel-mode WDK objects.
//!
//! The wrappers in this module expose Rust guard-based access to common
//! shared/exclusive kernel locks:
//!
//! - `RwLock` uses `ERESOURCE` for waitable reader-writer locking at `IRQL <=
//!   APC_LEVEL` and is available with the `rw-lock` feature.
//! - [`PushLock`] uses `EX_PUSH_LOCK` for compact waitable reader-writer
//!   locking at `IRQL <= APC_LEVEL`.
//! - [`RwSpinLock`] uses `EX_SPIN_LOCK` for very short non-waiting sections
//!   that can run up to `DISPATCH_LEVEL`; its acquisition methods are unsafe
//!   because callers must uphold the WDK's IRQL and release-order contracts.

pub use push_lock::*;
#[cfg(feature = "rw-lock")]
pub use rw_lock::*;
pub use rw_spin_lock::*;

mod push_lock;
#[cfg(feature = "rw-lock")]
mod rw_lock;
mod rw_spin_lock;

// Stable Rust does not support negative `Send` impls for these guard types.
// Raw pointers are neither `Send` nor `Sync`, so this marker keeps guards from
// crossing threads without requiring the `alloc` crate. This ensures kernel
// lock release and IRQL restoration happen on the acquiring thread.
type NotSend = core::marker::PhantomData<*const ()>;

const fn not_send() -> NotSend {
    core::marker::PhantomData
}
