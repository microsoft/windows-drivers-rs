use wdk_sys::{wdf::*, NTSTATUS, WDFSPINLOCK, WDF_OBJECT_ATTRIBUTES};

use crate::nt_success;

// private module + public re-export avoids the module name repetition: https://github.com/rust-lang/rust-clippy/issues/8524
#[allow(clippy::module_name_repetitions)]

/// WDF Spin Lock.
///
/// Use framework spin locks to synchronize access to driver data from code that
/// runs at `IRQL` <= `DISPATCH_LEVEL`. When a driver thread acquires a spin
/// lock, the system sets the thread's IRQL to `DISPATCH_LEVEL`. When the thread
/// releases the lock, the system restores the thread's IRQL to its previous
/// level. A driver that is not using automatic framework synchronization might
/// use a spin lock to synchronize access to a device object's context space, if
/// the context space is writable and if more than one of the driver's event
/// callback functions access the space. Before a driver can use a framework
/// spin lock it must call [`SpinLock::try_new()`] to create a [`SpinLock`]. The
/// driver can then call [`SpinLock::acquire`] to acquire the lock and
/// [`SpinLock::release()`] to release it.
pub struct SpinLock {
    wdf_spin_lock: WDFSPINLOCK,
}
impl SpinLock {
    /// Try to construct a WDF Spin Lock object
    ///
    /// # Errors
    ///
    /// This function will return an error if WDF fails to contruct a timer. The error variant will contain a [`NTSTATUS`] of the failure. Full error documentation is available in the [WDFSpinLock Documentation](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdfsync/nf-wdfsync-wdfspinlockcreate#return-value)
    pub fn try_new(attributes: &mut WDF_OBJECT_ATTRIBUTES) -> Result<Self, NTSTATUS> {
        let mut spin_lock = Self {
            wdf_spin_lock: core::ptr::null_mut(),
        };
        // SAFETY: The resulting ffi object is stored in a private member and not
        // accessible outside of this module, and this module guarantees that it is
        // always in a valid state.
        let nt_status = unsafe { WdfSpinLockCreate(attributes, &mut spin_lock.wdf_spin_lock) };
        nt_success(nt_status).then_some(spin_lock).ok_or(nt_status)
    }

    /// Try to construct a WDF Spin Lock object. This is an alias for
    /// [`SpinLock::try_new()`]
    ///
    /// # Errors
    ///
    /// This function will return an error if WDF fails to contruct a timer. The error variant will contain a [`NTSTATUS`] of the failure. Full error documentation is available in the [WDFSpinLock Documentation](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdfsync/nf-wdfsync-wdfspinlockcreate#return-value)
    pub fn create(attributes: &mut WDF_OBJECT_ATTRIBUTES) -> Result<Self, NTSTATUS> {
        Self::try_new(attributes)
    }

    /// Acquire the spinlock
    pub fn acquire(&self) {
        // SAFETY: `wdf_spin_lock` is a private member of `SpinLock`, originally created
        // by WDF, and this module guarantees that it is always in a valid state.
        unsafe { WdfSpinLockAcquire(self.wdf_spin_lock) };
    }

    /// Release the spinlock
    pub fn release(&self) {
        // SAFETY: `wdf_spin_lock` is a private member of `SpinLock`, originally created
        // by WDF, and this module guarantees that it is always in a valid state.
        unsafe { WdfSpinLockRelease(self.wdf_spin_lock) };
    }
}
