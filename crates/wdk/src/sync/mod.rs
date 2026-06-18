//! Synchronization primitives backed by kernel-mode WDK objects.
//!
//! The wrappers in this module expose Rust guard-based access to common
//! shared/exclusive kernel locks:
//!
//! - [`RwLock`] uses `ERESOURCE` for waitable reader-writer locking at `IRQL <=
//!   APC_LEVEL`.
//! - [`PushLock`] uses `EX_PUSH_LOCK` for compact waitable reader-writer
//!   locking at `IRQL <= APC_LEVEL`.
//! - [`RwSpinLock`] uses `EX_SPIN_LOCK` for very short non-waiting sections
//!   that can run up to `DISPATCH_LEVEL`.

pub use push_lock::*;
pub use rw_lock::*;
pub use rw_spin_lock::*;

mod push_lock;
mod rw_lock;
mod rw_spin_lock;

// Stable Rust does not support negative `Send` impls for these guard types.
// This marker keeps guards from crossing threads, which ensures kernel lock
// release and IRQL restoration happen on the acquiring thread.
type NotSend = core::marker::PhantomData<alloc::rc::Rc<()>>;

const fn not_send() -> NotSend {
    core::marker::PhantomData
}
