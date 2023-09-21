use wdk_sys::{macros, NTSTATUS, WDFSPINLOCK, WDF_OBJECT_ATTRIBUTES};

use crate::nt_success;

#[allow(clippy::module_name_repetitions)]
// private module + public re-export avoids the module name repetition: https://github.com/rust-lang/rust-clippy/issues/8524
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
        let nt_status = unsafe {
            macros::call_unsafe_wdf_function_binding!(
                WdfSpinLockCreate,
                attributes,
                &mut spin_lock.wdf_spin_lock,
            )
        };
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

    pub fn acquire(&self) {
        let [()] = unsafe {
            [macros::call_unsafe_wdf_function_binding!(
                WdfSpinLockAcquire,
                self.wdf_spin_lock
            )]
        };
    }

    pub fn release(&self) {
        let [()] = unsafe {
            [macros::call_unsafe_wdf_function_binding!(
                WdfSpinLockRelease,
                self.wdf_spin_lock
            )]
        };
    }
}
