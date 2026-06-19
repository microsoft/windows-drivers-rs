// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
};

use wdk_sys::{
    EX_SPIN_LOCK,
    KIRQL,
    ntddk::{
        ExAcquireSpinLockExclusive,
        ExAcquireSpinLockExclusiveAtDpcLevel,
        ExAcquireSpinLockShared,
        ExAcquireSpinLockSharedAtDpcLevel,
        ExReleaseSpinLockExclusive,
        ExReleaseSpinLockExclusiveFromDpcLevel,
        ExReleaseSpinLockShared,
        ExReleaseSpinLockSharedFromDpcLevel,
    },
};

/// Reader-writer spin lock backed by `EX_SPIN_LOCK`.
///
/// This lock is intended for very short critical sections that may be accessed
/// from elevated-IRQL paths. [`RwSpinLock::read`] and [`RwSpinLock::write`] can
/// be called at `IRQL <= DISPATCH_LEVEL`; they raise to `DISPATCH_LEVEL` and
/// restore the previous IRQL when the guard is dropped.
///
/// The `*_at_dpc_level` methods are for callers that are already running at
/// `DISPATCH_LEVEL`. They do not save or restore IRQL. Code while holding this
/// lock must not allocate, wait, call pageable code, or touch pageable data.
/// Because this lock can be held at `DISPATCH_LEVEL`, the lock object and the
/// protected value must be resident in nonpaged memory whenever they can be
/// accessed through this API.
///
/// Guards are deliberately `!Send` so IRQL restoration happens on the same
/// thread that acquired the lock.
pub struct RwSpinLock<T: ?Sized> {
    spin_lock: UnsafeCell<EX_SPIN_LOCK>,
    value: UnsafeCell<T>,
}

// SAFETY: `RwSpinLock` owns `T`, and moving the lock to another thread is safe
// when `T` can be moved between threads.
unsafe impl<T: ?Sized + Send> Send for RwSpinLock<T> {}

// SAFETY: Shared access requires `T: Sync`, and mutable access is serialized by
// the underlying spin lock. `T: Send` is required because a writer can move
// values out of the protected data.
unsafe impl<T: ?Sized + Send + Sync> Sync for RwSpinLock<T> {}

#[derive(Clone, Copy)]
enum ReleaseMode {
    RestoreIrql(KIRQL),
    AtDpcLevel,
}

impl<T> RwSpinLock<T> {
    /// Construct a new reader-writer spin lock protecting `value`.
    #[must_use]
    pub const fn new(value: T) -> Self {
        Self {
            spin_lock: UnsafeCell::new(0),
            value: UnsafeCell::new(value),
        }
    }
}

impl<T: ?Sized> RwSpinLock<T> {
    /// Lock this `RwSpinLock` with shared read access.
    ///
    /// This raises the current IRQL to `DISPATCH_LEVEL` and restores the
    /// previous IRQL when the returned guard is dropped.
    #[must_use]
    pub fn read(&self) -> RwSpinLockReadGuard<'_, T> {
        let old_irql;
        // SAFETY: `spin_lock_ptr` returns the initialized spin lock owned by
        // this object. The caller is at `IRQL <= DISPATCH_LEVEL`.
        unsafe {
            old_irql = ExAcquireSpinLockShared(self.spin_lock_ptr());
        }

        RwSpinLockReadGuard {
            lock: self,
            release_mode: ReleaseMode::RestoreIrql(old_irql),
            _not_send: super::not_send(),
        }
    }

    /// Lock this `RwSpinLock` with shared read access at `DISPATCH_LEVEL`.
    ///
    /// # Safety
    ///
    /// The caller must already be running at `DISPATCH_LEVEL`. The returned
    /// guard will not restore IRQL when it is dropped.
    #[must_use]
    pub unsafe fn read_at_dpc_level(&self) -> RwSpinLockReadGuard<'_, T> {
        // SAFETY: The caller guarantees current IRQL is `DISPATCH_LEVEL`, and
        // `spin_lock_ptr` returns the initialized spin lock owned by this
        // object.
        unsafe {
            ExAcquireSpinLockSharedAtDpcLevel(self.spin_lock_ptr());
        }

        RwSpinLockReadGuard {
            lock: self,
            release_mode: ReleaseMode::AtDpcLevel,
            _not_send: super::not_send(),
        }
    }

    /// Lock this `RwSpinLock` with exclusive write access.
    ///
    /// This raises the current IRQL to `DISPATCH_LEVEL` and restores the
    /// previous IRQL when the returned guard is dropped.
    #[must_use]
    pub fn write(&self) -> RwSpinLockWriteGuard<'_, T> {
        let old_irql;
        // SAFETY: `spin_lock_ptr` returns the initialized spin lock owned by
        // this object. The caller is at `IRQL <= DISPATCH_LEVEL`.
        unsafe {
            old_irql = ExAcquireSpinLockExclusive(self.spin_lock_ptr());
        }

        RwSpinLockWriteGuard {
            lock: self,
            release_mode: ReleaseMode::RestoreIrql(old_irql),
            _not_send: super::not_send(),
        }
    }

    /// Lock this `RwSpinLock` with exclusive write access at `DISPATCH_LEVEL`.
    ///
    /// # Safety
    ///
    /// The caller must already be running at `DISPATCH_LEVEL`. The returned
    /// guard will not restore IRQL when it is dropped.
    #[must_use]
    pub unsafe fn write_at_dpc_level(&self) -> RwSpinLockWriteGuard<'_, T> {
        // SAFETY: The caller guarantees current IRQL is `DISPATCH_LEVEL`, and
        // `spin_lock_ptr` returns the initialized spin lock owned by this
        // object.
        unsafe {
            ExAcquireSpinLockExclusiveAtDpcLevel(self.spin_lock_ptr());
        }

        RwSpinLockWriteGuard {
            lock: self,
            release_mode: ReleaseMode::AtDpcLevel,
            _not_send: super::not_send(),
        }
    }

    /// Get mutable access to the protected value without locking.
    ///
    /// This is safe because the mutable borrow proves no other borrow of the
    /// lock exists.
    pub fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }

    fn spin_lock_ptr(&self) -> *mut EX_SPIN_LOCK {
        self.spin_lock.get()
    }

    fn release_shared(&self, release_mode: ReleaseMode) {
        match release_mode {
            ReleaseMode::RestoreIrql(old_irql) => {
                // SAFETY: The guard was constructed by acquiring this spin lock
                // with `ExAcquireSpinLockShared`, which returned `old_irql`.
                unsafe {
                    ExReleaseSpinLockShared(self.spin_lock_ptr(), old_irql);
                }
            }
            ReleaseMode::AtDpcLevel => {
                // SAFETY: The guard was constructed by acquiring this spin lock
                // with `ExAcquireSpinLockSharedAtDpcLevel`.
                unsafe {
                    ExReleaseSpinLockSharedFromDpcLevel(self.spin_lock_ptr());
                }
            }
        }
    }

    fn release_exclusive(&self, release_mode: ReleaseMode) {
        match release_mode {
            ReleaseMode::RestoreIrql(old_irql) => {
                // SAFETY: The guard was constructed by acquiring this spin lock
                // with `ExAcquireSpinLockExclusive`, which returned `old_irql`.
                unsafe {
                    ExReleaseSpinLockExclusive(self.spin_lock_ptr(), old_irql);
                }
            }
            ReleaseMode::AtDpcLevel => {
                // SAFETY: The guard was constructed by acquiring this spin lock
                // with `ExAcquireSpinLockExclusiveAtDpcLevel`.
                unsafe {
                    ExReleaseSpinLockExclusiveFromDpcLevel(self.spin_lock_ptr());
                }
            }
        }
    }
}

/// Shared read guard returned by [`RwSpinLock::read`] and
/// [`RwSpinLock::read_at_dpc_level`].
pub struct RwSpinLockReadGuard<'a, T: ?Sized> {
    lock: &'a RwSpinLock<T>,
    release_mode: ReleaseMode,
    _not_send: super::NotSend,
}

impl<T: ?Sized> Deref for RwSpinLockReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: Shared guards are only created while the spin lock is held in
        // shared mode, and writers are excluded by the spin lock.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T: ?Sized> Drop for RwSpinLockReadGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.release_shared(self.release_mode);
    }
}

/// Exclusive write guard returned by [`RwSpinLock::write`] and
/// [`RwSpinLock::write_at_dpc_level`].
pub struct RwSpinLockWriteGuard<'a, T: ?Sized> {
    lock: &'a RwSpinLock<T>,
    release_mode: ReleaseMode,
    _not_send: super::NotSend,
}

impl<T: ?Sized> Deref for RwSpinLockWriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: Write guards are only created while the spin lock is held in
        // exclusive mode, and all other access is excluded by the spin lock.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T: ?Sized> DerefMut for RwSpinLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: Write guards are only created while the spin lock is held in
        // exclusive mode, and all other access is excluded by the spin lock.
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T: ?Sized> Drop for RwSpinLockWriteGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.release_exclusive(self.release_mode);
    }
}
