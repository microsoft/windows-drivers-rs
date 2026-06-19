// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
};

use wdk_sys::{
    ULONG,
    ULONG_PTR,
    ntddk::{
        ExAcquirePushLockExclusiveEx,
        ExAcquirePushLockSharedEx,
        ExInitializePushLock,
        ExReleasePushLockExclusiveEx,
        ExReleasePushLockSharedEx,
        KeEnterCriticalRegion,
        KeLeaveCriticalRegion,
    },
};

const EX_DEFAULT_PUSH_LOCK_FLAGS: ULONG = 0;

/// Reader-writer lock backed by an executive push lock (`EX_PUSH_LOCK`).
///
/// This lock is smaller than [`RwLock`](super::RwLock) and is useful for short
/// shared/exclusive critical sections at `IRQL <= APC_LEVEL`. Push locks can
/// wait while acquiring the lock, so they must not be used from DPC or ISR
/// contexts. Each acquisition enters a critical region before taking the push
/// lock and leaves it when the guard is dropped. Normal kernel APC delivery is
/// disabled while a guard is held.
///
/// Push locks do not support recursive exclusive acquisition. Attempting to
/// acquire the lock exclusively while it is already held by the current thread
/// can hang the system. Each successful acquisition must be released by
/// dropping the returned guard.
///
/// This wrapper uses the modern `ExAcquirePushLock*Ex` bindings generated from
/// the active WDK configuration.
pub struct PushLock<T: ?Sized> {
    push_lock: UnsafeCell<ULONG_PTR>,
    value: UnsafeCell<T>,
}

// SAFETY: `PushLock` owns `T`, and moving the lock to another thread is safe
// when `T` can be moved between threads.
unsafe impl<T: ?Sized + Send> Send for PushLock<T> {}

// SAFETY: Shared access requires `T: Sync`, and mutable access is serialized by
// the underlying push lock. `T: Send` is required because a writer can move
// values out of the protected data.
unsafe impl<T: ?Sized + Send + Sync> Sync for PushLock<T> {}

impl<T> PushLock<T> {
    /// Construct a new push lock protecting `value`.
    pub fn new(value: T) -> Self {
        let mut push_lock_storage = MaybeUninit::<ULONG_PTR>::uninit();
        let push_lock_ptr = push_lock_storage.as_mut_ptr();

        // SAFETY: `push_lock_ptr` points to writable, uninitialized storage for
        // exactly one `EX_PUSH_LOCK`, which `ExInitializePushLock` initializes.
        unsafe {
            ExInitializePushLock(push_lock_ptr);
        }

        // SAFETY: `ExInitializePushLock` initialized the storage.
        let push_lock = unsafe { push_lock_storage.assume_init() };

        Self {
            push_lock: UnsafeCell::new(push_lock),
            value: UnsafeCell::new(value),
        }
    }
}

impl<T: ?Sized> PushLock<T> {
    /// Lock this `PushLock` with shared read access.
    #[must_use]
    pub fn read(&self) -> PushLockReadGuard<'_, T> {
        self.enter_critical_region();

        // SAFETY: `push_lock_ptr` returns the initialized push lock owned by
        // this object. The caller is at `IRQL <= APC_LEVEL`, and the critical
        // region has been entered for this acquisition.
        unsafe {
            ExAcquirePushLockSharedEx(self.push_lock_ptr(), EX_DEFAULT_PUSH_LOCK_FLAGS);
        }

        PushLockReadGuard {
            lock: self,
            _not_send: super::not_send(),
        }
    }

    /// Lock this `PushLock` with exclusive write access.
    #[must_use]
    pub fn write(&self) -> PushLockWriteGuard<'_, T> {
        self.enter_critical_region();

        // SAFETY: `push_lock_ptr` returns the initialized push lock owned by
        // this object. The caller is at `IRQL <= APC_LEVEL`, and the critical
        // region has been entered for this acquisition.
        unsafe {
            ExAcquirePushLockExclusiveEx(self.push_lock_ptr(), EX_DEFAULT_PUSH_LOCK_FLAGS);
        }

        PushLockWriteGuard {
            lock: self,
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

    fn push_lock_ptr(&self) -> *mut ULONG_PTR {
        self.push_lock.get()
    }

    fn release_shared(&self) {
        // SAFETY: Each read guard is constructed only after a successful shared
        // acquisition of this push lock.
        unsafe {
            ExReleasePushLockSharedEx(self.push_lock_ptr(), EX_DEFAULT_PUSH_LOCK_FLAGS);
        }
        self.leave_critical_region();
    }

    fn release_exclusive(&self) {
        // SAFETY: Each write guard is constructed only after a successful
        // exclusive acquisition of this push lock.
        unsafe {
            ExReleasePushLockExclusiveEx(self.push_lock_ptr(), EX_DEFAULT_PUSH_LOCK_FLAGS);
        }
        self.leave_critical_region();
    }

    fn enter_critical_region(&self) {
        // SAFETY: Callers acquire `PushLock` only at `IRQL <= APC_LEVEL`, which
        // is the WDK contract for entering a critical region.
        unsafe {
            KeEnterCriticalRegion();
        }
    }

    fn leave_critical_region(&self) {
        // SAFETY: This is paired with a prior successful `KeEnterCriticalRegion`
        // call from the same thread.
        unsafe {
            KeLeaveCriticalRegion();
        }
    }
}

/// Shared read guard returned by [`PushLock::read`].
pub struct PushLockReadGuard<'a, T: ?Sized> {
    lock: &'a PushLock<T>,
    _not_send: super::NotSend,
}

impl<T: ?Sized> Deref for PushLockReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: Shared guards are only created while the push lock is held in
        // shared mode, and writers are excluded by the push lock.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T: ?Sized> Drop for PushLockReadGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.release_shared();
    }
}

/// Exclusive write guard returned by [`PushLock::write`].
pub struct PushLockWriteGuard<'a, T: ?Sized> {
    lock: &'a PushLock<T>,
    _not_send: super::NotSend,
}

impl<T: ?Sized> Deref for PushLockWriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: Write guards are only created while the push lock is held in
        // exclusive mode, and all other access is excluded by the push lock.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T: ?Sized> DerefMut for PushLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: Write guards are only created while the push lock is held in
        // exclusive mode, and all other access is excluded by the push lock.
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T: ?Sized> Drop for PushLockWriteGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.release_exclusive();
    }
}
