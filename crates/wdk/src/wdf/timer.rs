// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

use wdk_sys::{
    NTSTATUS,
    WDF_OBJECT_ATTRIBUTES,
    WDF_TIMER_CONFIG,
    WDFTIMER,
    call_unsafe_wdf_function_binding,
};

use crate::nt_success;

/// WDF Timer.
pub struct Timer {
    wdf_timer: WDFTIMER,
}
impl Timer {
    /// Try to construct a WDF Timer object
    ///
    /// # Errors
    ///
    /// This function will return an error if WDF fails to construct a timer.
    /// The error variant will contain a [`NTSTATUS`] of the failure. Full error
    /// documentation is available in the [WDFTimer Documentation](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdftimer/nf-wdftimer-wdftimercreate#return-value)
    pub fn try_new(
        timer_config: &mut WDF_TIMER_CONFIG,
        attributes: &mut WDF_OBJECT_ATTRIBUTES,
    ) -> Result<Self, NTSTATUS> {
        let mut timer = Self {
            wdf_timer: core::ptr::null_mut(),
        };

        let nt_status;
        // SAFETY: The resulting ffi object is stored in a private member and not
        // accessible outside of this module, and this module guarantees that it is
        // always in a valid state.
        unsafe {
            nt_status = call_unsafe_wdf_function_binding!(
                WdfTimerCreate,
                timer_config,
                attributes,
                &mut timer.wdf_timer as *mut WDFTIMER,
            );
        }
        nt_success(nt_status).then_some(timer).ok_or(nt_status)
    }

    /// Try to construct a WDF Timer object
    ///
    /// # Errors
    ///
    /// This function will return an error if WDF fails to construct a timer.
    /// The error variant will contain a [`NTSTATUS`] of the failure. Full error
    /// documentation is available in the [WDFTimer Documentation](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdftimer/nf-wdftimer-wdftimercreate#return-value)
    pub fn create(
        timer_config: &mut WDF_TIMER_CONFIG,
        attributes: &mut WDF_OBJECT_ATTRIBUTES,
    ) -> Result<Self, NTSTATUS> {
        Self::try_new(timer_config, attributes)
    }

    /// Start the [`Timer`]'s clock
    #[must_use]
    pub fn start(&self, due_time: i64) -> bool {
        let result;
        // SAFETY: `wdf_timer` is a private member of `Timer`, originally created by
        // WDF, and this module guarantees that it is always in a valid state.
        unsafe {
            result = call_unsafe_wdf_function_binding!(WdfTimerStart, self.wdf_timer, due_time);
        }
        result != 0
    }

    /// Stop the [`Timer`]'s clock
    #[must_use]
    pub fn stop(&self, wait: bool) -> bool {
        let result;
        // SAFETY: `wdf_timer` is a private member of `Timer`, originally created by
        // WDF, and this module guarantees that it is always in a valid state.
        unsafe {
            result =
                call_unsafe_wdf_function_binding!(WdfTimerStop, self.wdf_timer, u8::from(wait));
        }
        result != 0
    }
}
