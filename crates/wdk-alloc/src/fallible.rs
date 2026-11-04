// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Fallible allocation primitives for driver code.
//!
//! # Why this module exists
//!
//! Rust's collection APIs are designed for a hosted environment where running
//! out of memory is fatal-but-orderly: [`Vec::push`], [`Vec::with_capacity`],
//! [`alloc::boxed::Box::new`] and [`Iterator::collect`] all route an allocation
//! failure to [`alloc::alloc::handle_alloc_error`], which in a `no_std` binary
//! panics.
//!
//! In a kernel-mode driver a panic is not an abort. `wdk-panic`'s handler calls
//! `KeBugCheckEx`, so *every infallible allocation in a driver is a blue screen
//! on a customer machine under memory pressure*. Allocation failure in the
//! kernel is an ordinary, expected condition — `STATUS_INSUFFICIENT_RESOURCES`
//! exists precisely for it — so the infallible constructors are the wrong
//! default for this environment, not merely a risky one.
//!
//! This module provides the fallible counterparts, so that driver code can
//! build its data structures with no path that reaches
//! [`alloc::alloc::handle_alloc_error`] at all.
//!
//! # What is available on stable
//!
//! Only [`Vec::try_reserve`] and [`Vec::try_reserve_exact`] are stable fallible
//! allocation APIs. `Box::try_new`, `Arc::try_new`, `Vec::try_with_capacity`
//! and `Vec::push_within_capacity` are all still unstable (they sit behind
//! `allocator_api`, `try_with_capacity` and `vec_push_within_capacity`).
//!
//! Everything in this module is therefore built from two stable pieces:
//!
//! 1. [`Vec::try_reserve`], for growing a `Vec`. A [`Vec::push`] that follows a
//!    *successful* `try_reserve(1)` provably cannot allocate, which is what
//!    makes [`FallibleVec::try_push`] infallible-free rather than merely
//!    unlikely to fail.
//! 2. [`alloc::alloc::alloc`], which reports failure by returning null rather
//!    than by calling [`alloc::alloc::handle_alloc_error`]. That is the
//!    difference that lets [`try_box`] exist on stable at all.
//!
//! Nothing here requires the `nightly` feature.
//!
//! # An `alloc_error_handler` cannot substitute for any of this
//!
//! It is tempting to think a custom `#[alloc_error_handler]` would make the
//! infallible constructors safe. It cannot, for a reason that has nothing to do
//! with which channel it is on: the handler's signature is `fn(Layout) -> !`.
//! It is not allowed to return, so it cannot turn `Vec::push` into an operation
//! that reports failure to its caller. The most a handler can do is choose
//! *which* bugcheck happens. Fallibility has to come from the call, which is
//! why this module exists.

use alloc::{boxed::Box, collections::TryReserveError, vec::Vec};
use core::{alloc::Layout, fmt};

/// An allocation failed.
///
/// This is deliberately a zero-sized type rather than a wrapper around
/// [`TryReserveError`]. Kernel-mode callers turn an allocation failure into
/// `STATUS_INSUFFICIENT_RESOURCES` regardless of whether the underlying cause
/// was a capacity overflow or an exhausted pool, so preserving the distinction
/// would cost every caller a `match` that has one arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct AllocError;

impl fmt::Display for AllocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("memory allocation failed")
    }
}

impl core::error::Error for AllocError {}

impl From<TryReserveError> for AllocError {
    fn from(_: TryReserveError) -> Self {
        Self
    }
}

/// Fallible allocation of a [`Box`].
///
/// This is the stable equivalent of the unstable `Box::try_new`. Unlike
/// [`Box::new`], a failure to allocate is returned to the caller instead of
/// reaching [`alloc::alloc::handle_alloc_error`].
///
/// A zero-sized `T` never allocates, so it cannot fail and is constructed
/// directly.
///
/// # Errors
///
/// Returns [`AllocError`] if the global allocator could not supply
/// [`Layout::new::<T>()`](Layout::new) bytes.
///
/// # Examples
///
/// ```
/// use wdk_alloc::fallible::try_box;
///
/// let boxed = try_box(42u32)?;
/// assert_eq!(*boxed, 42);
/// # Ok::<(), wdk_alloc::fallible::AllocError>(())
/// ```
pub fn try_box<T>(value: T) -> Result<Box<T>, AllocError> {
    let layout = Layout::new::<T>();
    if layout.size() == 0 {
        // A zero-sized allocation is never made at all, so `Box::new` has no
        // failure path here and cannot reach `handle_alloc_error`.
        return Ok(Box::new(value));
    }

    // SAFETY: `layout` is checked immediately above to have non-zero size,
    // which is `alloc`'s only precondition.
    let ptr = unsafe { alloc::alloc::alloc(layout) }.cast::<T>();

    if ptr.is_null() {
        return Err(AllocError);
    }

    // SAFETY: `ptr` is non-null (checked above) and was allocated with
    // `Layout::new::<T>()`, so it is sized and aligned for `T`. It is
    // uninitialized, so the write does not drop a previously live value.
    unsafe { ptr.write(value) };

    // SAFETY: `ptr` was allocated by the global allocator with
    // `Layout::new::<T>()`, which is exactly the layout `Box<T>` deallocates
    // with, and it now holds an initialized `T`. Ownership is transferred to
    // the `Box`.
    Ok(unsafe { Box::from_raw(ptr) })
}

/// Allocates a [`Vec`] with room for at least `capacity` elements.
///
/// This is the fallible replacement for [`Vec::with_capacity`], and the stable
/// equivalent of the unstable `Vec::try_with_capacity`.
///
/// It is deliberately a free function rather than a method on [`Vec`]. std has
/// an unstable inherent `Vec::try_with_capacity`, so a trait method of that
/// name would raise `unstable_name_collisions` at *every consumer's* call site
/// — and consumers of this crate build with warnings denied, so the collision
/// would arrive as a build failure in their code rather than in ours.
///
/// [`Vec::try_reserve_exact`] is the inherent stable alternative if you already
/// hold the `Vec`.
///
/// # Errors
///
/// Returns [`AllocError`] if `capacity` elements could not be allocated.
///
/// # Examples
///
/// ```
/// use wdk_alloc::fallible::{AllocError, try_vec_with_capacity};
///
/// let vec: Vec<u32> = try_vec_with_capacity(16)?;
/// assert!(vec.capacity() >= 16);
/// assert!(vec.is_empty());
/// # Ok::<(), AllocError>(())
/// ```
pub fn try_vec_with_capacity<T>(capacity: usize) -> Result<Vec<T>, AllocError> {
    let mut vec = Vec::new();
    // `try_reserve_exact` rather than `try_reserve`: the caller asked for a
    // specific capacity, and the amortised-growth slack `try_reserve` is
    // allowed to add is not wanted when the length is already known.
    vec.try_reserve_exact(capacity)?;
    Ok(vec)
}

/// Fallible counterparts to [`Vec`]'s allocating operations.
///
/// # Examples
///
/// A site that would otherwise be a bugcheck path under memory pressure:
///
/// ```
/// use wdk_alloc::fallible::{AllocError, FallibleVec, try_vec_with_capacity};
///
/// fn collect_indices(matches: &[usize]) -> Result<Vec<usize>, AllocError> {
///     let mut out = try_vec_with_capacity(matches.len())?;
///     for &index in matches {
///         out.try_push(index)?;
///     }
///     Ok(out)
/// }
///
/// assert_eq!(collect_indices(&[1, 2, 3])?, vec![1, 2, 3]);
/// # Ok::<(), AllocError>(())
/// ```
pub trait FallibleVec<T> {
    /// Appends `value`, returning [`AllocError`] instead of bugchecking if the
    /// `Vec` needed to grow and could not.
    ///
    /// # Errors
    ///
    /// Returns [`AllocError`] if the `Vec` was at capacity and could not grow.
    fn try_push(&mut self, value: T) -> Result<(), AllocError>;

    /// Appends every item of `iter`, returning [`AllocError`] instead of
    /// bugchecking if the `Vec` needed to grow and could not.
    ///
    /// This is the fallible replacement for [`Vec::extend`], which has no
    /// fallible form on any channel.
    ///
    /// # Errors
    ///
    /// Returns [`AllocError`] if the `Vec` could not grow to hold the items.
    /// Items already appended are retained; this is the same partial-progress
    /// behaviour [`Vec::extend`] has when it panics.
    fn try_extend<I>(&mut self, iter: I) -> Result<(), AllocError>
    where
        I: IntoIterator<Item = T>;
}

impl<T> FallibleVec<T> for Vec<T> {
    fn try_push(&mut self, value: T) -> Result<(), AllocError> {
        self.try_reserve(1)?;
        // `push` cannot allocate here: `try_reserve(1)` returned `Ok`, so there
        // is spare capacity for exactly this element. This is what keeps the
        // whole operation off `handle_alloc_error`.
        self.push(value);
        Ok(())
    }

    fn try_extend<I>(&mut self, iter: I) -> Result<(), AllocError>
    where
        I: IntoIterator<Item = T>,
    {
        let iter = iter.into_iter();
        // Reserve the lower bound up front so a well-behaved iterator costs one
        // allocation rather than one per element. `size_hint` is not trusted
        // for correctness -- each `try_push` below re-checks -- so a wrong hint
        // costs performance, never soundness.
        let (lower_bound, _) = iter.size_hint();
        self.try_reserve(lower_bound)?;
        for item in iter {
            self.try_push(item)?;
        }
        Ok(())
    }
}

/// Fallible [`Iterator::collect`] into a [`Vec`].
///
/// [`Iterator::collect`] allocates infallibly, so in driver code it is a
/// bugcheck path. This trait is implemented for every [`Iterator`], so the fix
/// at a call site is to change `.collect()` to `.try_collect_vec()?`.
///
/// # Examples
///
/// The UTF-8 to UTF-16 conversion that WDK string APIs require, without a
/// bugcheck path:
///
/// ```
/// use wdk_alloc::fallible::{AllocError, TryCollectVec};
///
/// let path = r"\Device\HarddiskVolume1";
/// let utf16 = path.encode_utf16().try_collect_vec()?;
///
/// // Same result as the infallible `collect()`, without the bugcheck path.
/// assert_eq!(utf16, path.encode_utf16().collect::<Vec<u16>>());
/// # Ok::<(), AllocError>(())
/// ```
pub trait TryCollectVec: Iterator {
    /// Collects the iterator into a [`Vec`], returning [`AllocError`] instead
    /// of bugchecking if an allocation fails.
    ///
    /// # Errors
    ///
    /// Returns [`AllocError`] if the `Vec` could not be grown to hold every
    /// item.
    fn try_collect_vec(self) -> Result<Vec<Self::Item>, AllocError>
    where
        Self: Sized,
    {
        let mut out = Vec::new();
        out.try_extend(self)?;
        Ok(out)
    }
}

impl<I: Iterator> TryCollectVec for I {}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use super::*;

    #[test]
    fn try_box_round_trips_a_value() {
        let boxed = try_box(0xDEAD_BEEF_u32).expect("small allocation should succeed");
        assert_eq!(*boxed, 0xDEAD_BEEF);
    }

    #[test]
    fn try_box_supports_zero_sized_types() {
        // A ZST takes the non-allocating path, which must still produce a
        // usable `Box`.
        let boxed = try_box(()).expect("zero-sized allocation cannot fail");
        assert_eq!(*boxed, ());
    }

    #[test]
    fn try_box_runs_drop_glue() {
        use alloc::rc::Rc;

        let witness = Rc::new(());
        let boxed = try_box(Rc::clone(&witness)).expect("small allocation should succeed");
        assert_eq!(Rc::strong_count(&witness), 2);
        drop(boxed);
        // Dropping the `Box` must drop the `Rc` inside it, not leak it.
        assert_eq!(Rc::strong_count(&witness), 1);
    }

    #[test]
    fn try_box_reports_failure_and_drops_input_once() {
        use alloc::rc::Rc;
        use core::{cell::Cell, sync::atomic::Ordering};

        // A value with drop glue so the test can observe that the input is
        // dropped exactly once on the failure path rather than leaked.
        struct DropCounter(Rc<Cell<usize>>);

        impl Drop for DropCounter {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }

        let drops = Rc::new(Cell::new(0));
        let value = DropCounter(Rc::clone(&drops));

        // Fail the next allocation, which is `try_box`'s box allocation.
        crate::test_allocator::GLOBAL
            .fail_next
            .store(true, Ordering::SeqCst);

        let result: Result<Box<DropCounter>, AllocError> = try_box(value);
        assert_eq!(result.err(), Some(AllocError));
        // The input was dropped exactly once on the failure path, not leaked.
        assert_eq!(drops.get(), 1);
    }

    #[test]
    fn try_push_appends() {
        let mut vec = Vec::new();
        for index in 0..64_usize {
            vec.try_push(index)
                .expect("small allocation should succeed");
        }
        assert_eq!(vec.len(), 64);
        assert_eq!(vec[63], 63);
    }

    #[test]
    fn try_vec_with_capacity_reserves_without_growing_length() {
        let vec: Vec<u64> = try_vec_with_capacity(128).expect("small allocation should succeed");
        assert_eq!(vec.len(), 0);
        assert!(vec.capacity() >= 128);
    }

    #[test]
    fn try_vec_with_capacity_reports_failure_instead_of_panicking() {
        // A capacity this large cannot be satisfied, so this exercises the
        // error path rather than the success path. Without the fallible API
        // this would be a bugcheck.
        let result: Result<Vec<u64>, AllocError> = try_vec_with_capacity(usize::MAX);
        assert_eq!(result.err(), Some(AllocError));
    }

    #[test]
    fn try_extend_appends_every_item() {
        let mut vec = alloc::vec![1_u8, 2];
        vec.try_extend([3, 4, 5])
            .expect("small allocation should succeed");
        assert_eq!(vec, alloc::vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn try_extend_tolerates_an_understated_size_hint() {
        // `filter` reports a lower bound of 0, so every element arrives through
        // `try_push`'s own reservation. The result must still be complete.
        let mut vec = Vec::new();
        vec.try_extend((0..32_u32).filter(|value| value % 2 == 0))
            .expect("small allocation should succeed");
        assert_eq!(vec.len(), 16);
    }

    #[test]
    fn try_collect_vec_matches_collect() {
        let collected = "abc"
            .encode_utf16()
            .try_collect_vec()
            .expect("small allocation");
        assert_eq!(
            collected,
            alloc::vec![b'a'.into(), b'b'.into(), b'c'.into()]
        );
    }

    #[test]
    fn alloc_error_displays_without_allocating() {
        // `AllocError` is reported on the OOM path, so its `Display` must not
        // itself need to allocate. A static string is the only safe choice.
        assert_eq!(AllocError.to_string(), "memory allocation failed");
    }
}
