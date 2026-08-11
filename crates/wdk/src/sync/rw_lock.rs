// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

use alloc::alloc::{Layout, alloc, dealloc};
use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use wdk_sys::{
    ERESOURCE,
    NTSTATUS,
    STATUS_INSUFFICIENT_RESOURCES,
    ntddk::{
        ExAcquireResourceExclusiveLite,
        ExAcquireResourceSharedLite,
        ExDeleteResourceLite,
        ExInitializeResourceLite,
        ExReleaseResourceLite,
        KeEnterCriticalRegion,
        KeLeaveCriticalRegion,
    },
};

use crate::nt_success;

/// Reader-writer lock backed by an executive resource (`ERESOURCE`).
///
/// The lock can be acquired at `IRQL <= APC_LEVEL`. The underlying executive
/// resource is initialized by [`RwLock::try_new`] and deleted when the lock is
/// dropped. Each successful acquisition enters a critical region before taking
/// the resource and leaves it when the guard is dropped.
///
/// The backing `ERESOURCE` is allocated separately through the configured
/// global allocator so its kernel object address stays stable for the lifetime
/// of the lock.
pub struct RwLock<T: ?Sized> {
    resource: NonNull<ERESOURCE>,
    value: UnsafeCell<T>,
}

// SAFETY: `RwLock` owns `T`, and moving the lock to another thread is safe when
// `T` can be moved between threads.
unsafe impl<T: ?Sized + Send> Send for RwLock<T> {}

// SAFETY: Shared access requires `T: Sync`, and mutable access is serialized by
// the underlying executive resource. `T: Send` is required because a writer can
// move values out of the protected data.
unsafe impl<T: ?Sized + Send + Sync> Sync for RwLock<T> {}

impl<T> RwLock<T> {
    /// Try to construct a new reader-writer lock.
    ///
    /// # Errors
    ///
    /// Returns the failing [`NTSTATUS`] if `ExInitializeResourceLite` cannot
    /// initialize the underlying `ERESOURCE`, or
    /// [`STATUS_INSUFFICIENT_RESOURCES`] if the backing resource allocation
    /// fails.
    pub fn try_new(value: T) -> Result<Self, NTSTATUS> {
        let resource = allocate_resource()?;

        let status;
        // SAFETY: `resource` points to writable, uninitialized storage for
        // exactly one `ERESOURCE`, which `ExInitializeResourceLite` initializes.
        unsafe {
            status = ExInitializeResourceLite(resource.as_ptr());
        }
        if !nt_success(status) {
            // SAFETY: `resource` came from `allocate_resource`, and
            // initialization failed, so no kernel teardown is required.
            unsafe {
                deallocate_resource(resource);
            }
            return Err(status);
        }

        Ok(Self {
            resource,
            value: UnsafeCell::new(value),
        })
    }
}

impl<T: ?Sized> RwLock<T> {
    /// Lock this `RwLock` with shared read access.
    #[must_use]
    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        let acquired = self.acquire_shared(true);
        assert!(acquired);

        RwLockReadGuard {
            lock: self,
            _not_send: super::not_send(),
        }
    }

    /// Try to lock this `RwLock` with shared read access without waiting.
    #[must_use]
    pub fn try_read(&self) -> Option<RwLockReadGuard<'_, T>> {
        self.acquire_shared(false).then_some(RwLockReadGuard {
            lock: self,
            _not_send: super::not_send(),
        })
    }

    /// Lock this `RwLock` with exclusive write access.
    #[must_use]
    pub fn write(&self) -> RwLockWriteGuard<'_, T> {
        let acquired = self.acquire_exclusive(true);
        assert!(acquired);

        RwLockWriteGuard {
            lock: self,
            _not_send: super::not_send(),
        }
    }

    /// Try to lock this `RwLock` with exclusive write access without waiting.
    #[must_use]
    pub fn try_write(&self) -> Option<RwLockWriteGuard<'_, T>> {
        self.acquire_exclusive(false).then_some(RwLockWriteGuard {
            lock: self,
            _not_send: super::not_send(),
        })
    }

    /// Get mutable access to the protected value without locking.
    ///
    /// This is safe because the mutable borrow proves no other borrow of the
    /// lock exists.
    pub fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }

    fn resource_ptr(&self) -> *mut ERESOURCE {
        self.resource.as_ptr()
    }

    fn acquire_shared(&self, wait: bool) -> bool {
        self.enter_critical_region();

        let acquired;
        // SAFETY: `resource_ptr` returns the initialized `ERESOURCE` owned by
        // this lock. The caller is at `IRQL <= APC_LEVEL`, and the critical
        // region has been entered for this acquisition attempt.
        unsafe {
            acquired = ExAcquireResourceSharedLite(self.resource_ptr(), u8::from(wait));
        }

        if acquired == 0 {
            self.leave_critical_region();
        }

        acquired != 0
    }

    fn acquire_exclusive(&self, wait: bool) -> bool {
        self.enter_critical_region();

        let acquired;
        // SAFETY: `resource_ptr` returns the initialized `ERESOURCE` owned by
        // this lock. The caller is at `IRQL <= APC_LEVEL`, and the critical
        // region has been entered for this acquisition attempt.
        unsafe {
            acquired = ExAcquireResourceExclusiveLite(self.resource_ptr(), u8::from(wait));
        }

        if acquired == 0 {
            self.leave_critical_region();
        }

        acquired != 0
    }

    fn release(&self) {
        // SAFETY: Each guard is constructed only after a successful acquisition
        // of this `ERESOURCE`, and this releases exactly that acquisition.
        unsafe {
            ExReleaseResourceLite(self.resource_ptr());
        }
        self.leave_critical_region();
    }

    fn enter_critical_region(&self) {
        // SAFETY: Callers acquire `RwLock` only at `IRQL <= APC_LEVEL`, which is
        // the WDK contract for entering a critical region.
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

impl<T: ?Sized> Drop for RwLock<T> {
    fn drop(&mut self) {
        // SAFETY: The resource was initialized by `try_new`, is owned by this
        // `RwLock`, and no guards can outlive `self`.
        unsafe {
            let _ = ExDeleteResourceLite(self.resource.as_ptr());
        }

        // SAFETY: `self.resource` was allocated by `allocate_resource` and has
        // not already been deallocated.
        unsafe {
            deallocate_resource(self.resource);
        }
    }
}

fn allocate_resource() -> Result<NonNull<ERESOURCE>, NTSTATUS> {
    let layout = Layout::new::<ERESOURCE>();

    // SAFETY: `layout` describes exactly one `ERESOURCE`. A null result is
    // handled below.
    let resource = unsafe { alloc(layout).cast::<ERESOURCE>() };

    NonNull::new(resource).ok_or(STATUS_INSUFFICIENT_RESOURCES)
}

unsafe fn deallocate_resource(resource: NonNull<ERESOURCE>) {
    // SAFETY: Callers pass a pointer returned by `allocate_resource` that has
    // not already been deallocated.
    unsafe {
        dealloc(resource.as_ptr().cast::<u8>(), Layout::new::<ERESOURCE>());
    }
}

/// Shared read guard returned by [`RwLock::read`] and [`RwLock::try_read`].
pub struct RwLockReadGuard<'a, T: ?Sized> {
    lock: &'a RwLock<T>,
    _not_send: super::NotSend,
}

impl<T: ?Sized> Deref for RwLockReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: Shared guards are only created while the resource is held in
        // shared mode, and writers are excluded by the `ERESOURCE`.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T: ?Sized> Drop for RwLockReadGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.release();
    }
}

/// Exclusive write guard returned by [`RwLock::write`] and
/// [`RwLock::try_write`].
pub struct RwLockWriteGuard<'a, T: ?Sized> {
    lock: &'a RwLock<T>,
    _not_send: super::NotSend,
}

impl<T: ?Sized> Deref for RwLockWriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: Write guards are only created while the resource is held in
        // exclusive mode, and all other access is excluded by the `ERESOURCE`.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T: ?Sized> DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: Write guards are only created while the resource is held in
        // exclusive mode, and all other access is excluded by the `ERESOURCE`.
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T: ?Sized> Drop for RwLockWriteGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.release();
    }
}
