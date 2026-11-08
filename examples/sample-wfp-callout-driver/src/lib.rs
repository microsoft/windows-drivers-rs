// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! # Sample WFP Callout Driver
//!
//! This is a sample Windows Filtering Platform (WFP) callout driver that
//! demonstrates how to use the crates in windows-driver-rs to create a skeleton
//! of a callout driver.
//!
//! It is a port of the [`inspect` sample](https://github.com/microsoft/Windows-driver-samples/tree/main/network/trans/inspect)
//! from the WDK driver samples, reduced to a single inspection-only callout at
//! the ALE authorize-connect layer for IPv4 (`FWPM_LAYER_ALE_AUTH_CONNECT_V4`).
//! It registers a callout with the filter engine, adds a sublayer, a callout
//! entry, and a filter that matches every outbound TCP connection, and prints
//! the remote address and port of each connection attempt before permitting it.
//!
//! The `inspect` sample performs out-of-band inspection on a worker thread so
//! that it can pend and later re-authorize connections. That machinery is
//! deliberately omitted here: this sample makes its decision inline in
//! `classifyFn`, which keeps the port focused on the parts specific to
//! registering with the filter engine.
//!
//! This sample registers with [`FwpsCalloutRegister2`], which is available on
//! Windows 10 RS1 (1607) and later, so it stays loadable on the RS2 (1703)
//! floor that the downlevel allocation shims in `wdk-sys` target.

#![no_std]

extern crate alloc;

#[cfg(not(test))]
extern crate wdk_panic;

use core::sync::atomic::{AtomicPtr, AtomicU32, Ordering};

use wdk::{nt_success, println};
#[cfg(not(test))]
use wdk_alloc::WdkAllocator;
use wdk_sys::{
    _DEVICE_OBJECT,
    DRIVER_OBJECT,
    FILE_DEVICE_NETWORK,
    FILE_DEVICE_SECURE_OPEN,
    FWP_ACTION_CALLOUT_INSPECTION,
    FWP_ACTION_CONTINUE,
    FWP_CONDITION_VALUE0,
    FWP_CONDITION_VALUE0___bindgen_ty_1,
    FWP_DATA_TYPE_::{FWP_EMPTY, FWP_UINT8, FWP_UINT16, FWP_UINT32},
    FWP_MATCH_TYPE_::FWP_MATCH_EQUAL,
    FWP_VALUE0,
    FWPM_ACTION0,
    FWPM_ACTION0___bindgen_ty_1,
    FWPM_CALLOUT0,
    FWPM_CONDITION_IP_PROTOCOL,
    FWPM_DISPLAY_DATA0,
    FWPM_FILTER_CONDITION0,
    FWPM_FILTER0,
    FWPM_LAYER_ALE_AUTH_CONNECT_V4,
    FWPM_SESSION_FLAG_DYNAMIC,
    FWPM_SESSION0,
    FWPM_SUBLAYER0,
    FWPS_CALLOUT_NOTIFY_TYPE,
    FWPS_CALLOUT2,
    FWPS_CLASSIFY_OUT0,
    FWPS_FIELDS_ALE_AUTH_CONNECT_V4,
    FWPS_FIELDS_ALE_AUTH_CONNECT_V4_::{
        FWPS_FIELD_ALE_AUTH_CONNECT_V4_IP_REMOTE_ADDRESS,
        FWPS_FIELD_ALE_AUTH_CONNECT_V4_IP_REMOTE_PORT,
    },
    FWPS_FILTER_FLAG_CLEAR_ACTION_RIGHT,
    FWPS_FILTER2,
    FWPS_INCOMING_METADATA_VALUES0,
    FWPS_INCOMING_VALUES0,
    FWPS_RIGHT_ACTION_WRITE,
    GUID,
    HANDLE,
    IPPROTO::IPPROTO_TCP,
    NTSTATUS,
    PCUNICODE_STRING,
    PDEVICE_OBJECT,
    PDRIVER_OBJECT,
    RPC_C_AUTHN_WINNT,
    STATUS_SUCCESS,
    UINT8,
    UINT16,
    UINT32,
    UINT64,
    WCHAR,
    ntddk::{IoCreateDevice, IoDeleteDevice},
    wfp::{
        FwpmCalloutAdd0,
        FwpmEngineClose0,
        FwpmEngineOpen0,
        FwpmFilterAdd0,
        FwpmSubLayerAdd0,
        FwpmTransactionAbort0,
        FwpmTransactionBegin0,
        FwpmTransactionCommit0,
        FwpsCalloutRegister2,
        FwpsCalloutUnregisterById0,
    },
};

#[cfg(not(test))]
#[global_allocator]
static GLOBAL_ALLOCATOR: WdkAllocator = WdkAllocator;

/// Key identifying this driver's callout, which ties the `FWPS_CALLOUT2`
/// registered with the filter engine to the `FWPM_CALLOUT0` entry and to the
/// filter that invokes it.
///
/// A real driver must generate its own key. `fwpmk.h` only declares the
/// `FWPM_*` keys the platform defines, so `wdk-sys` compiles their definitions
/// from a shim; a driver's own keys have no such indirection and are written
/// directly as a [`GUID`] literal.
// aa57d438-eabd-4f4b-ab03-8da91641019e
const SAMPLE_CALLOUT_KEY: GUID = GUID {
    Data1: 0xAA57_D438,
    Data2: 0xEABD,
    Data3: 0x4F4B,
    Data4: [0xAB, 0x03, 0x8D, 0xA9, 0x16, 0x41, 0x01, 0x9E],
};

/// Key identifying the sublayer this driver's filter is added to.
// 18fe36f6-54d6-4b08-936f-a77c8714791a
const SAMPLE_SUBLAYER_KEY: GUID = GUID {
    Data1: 0x18FE_36F6,
    Data2: 0x54D6,
    Data3: 0x4B08,
    Data4: [0x93, 0x6F, 0xA7, 0x7C, 0x87, 0x14, 0x79, 0x1A],
};

/// Device object that the filter engine associates the registered callout with,
/// created by [`IoCreateDevice`] and deleted by [`driver_unload`].
///
/// A callout driver must supply a device object to [`FwpsCalloutRegister2`] so
/// that the filter engine can take a reference on the driver for as long as the
/// callout is registered. `DriverEntry` is serialized against the unload
/// routine, so an [`AtomicPtr`] with [`Ordering::Relaxed`] accesses is
/// sufficient here; it is used in place of a `static mut` so that reading the
/// pointer is safe.
static DEVICE_OBJECT: AtomicPtr<_DEVICE_OBJECT> = AtomicPtr::new(core::ptr::null_mut());

/// Handle to this driver's session with the filter engine, returned by
/// [`FwpmEngineOpen0`] and closed by [`driver_unload`].
static ENGINE_HANDLE: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(core::ptr::null_mut());

/// Runtime identifier the filter engine assigns to the registered callout,
/// which [`FwpsCalloutUnregisterById0`] uses to unregister it.
///
/// The filter engine never assigns `0`, so it doubles as "not registered".
static CALLOUT_ID: AtomicU32 = AtomicU32::new(0);

/// `DriverEntry` function required by WDM
///
/// # Safety
/// Function is unsafe since it dereferences raw pointers passed to it by the
/// I/O manager
// SAFETY: "DriverEntry" is the required symbol name for Windows driver entry points.
// No other function in this compilation unit exports this name, preventing symbol conflicts.
#[unsafe(export_name = "DriverEntry")]
pub unsafe extern "system" fn driver_entry(
    driver: &mut DRIVER_OBJECT,
    _registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    println!("WFP Callout Driver Entry!");

    driver.DriverUnload = Some(driver_unload);

    let mut device_object: PDEVICE_OBJECT = core::ptr::null_mut();
    let io_create_device_ntstatus =
        // SAFETY: This is safe because:
        //         1. `driver` is provided by `DriverEntry` and is never null
        //         2. A null `DeviceName` is valid, and produces an unnamed device object, which is
        //            all the filter engine requires
        //         3. `device_object` is a valid pointer to a `PDEVICE_OBJECT` for the I/O manager
        //            to write the resulting pointer into
        unsafe {
            IoCreateDevice(
                driver as PDRIVER_OBJECT,
                0,
                core::ptr::null_mut(),
                FILE_DEVICE_NETWORK,
                FILE_DEVICE_SECURE_OPEN,
                0,
                &raw mut device_object,
            )
        };
    if !nt_success(io_create_device_ntstatus) {
        println!("IoCreateDevice failed: {io_create_device_ntstatus:#010x}");
        return io_create_device_ntstatus;
    }
    DEVICE_OBJECT.store(device_object, Ordering::Relaxed);

    let register_callouts_ntstatus = register_callouts(device_object);
    if !nt_success(register_callouts_ntstatus) {
        println!("register_callouts failed: {register_callouts_ntstatus:#010x}");

        // SAFETY: `device_object` was successfully initialized by `IoCreateDevice`
        // above, and no callout is registered against it since `register_callouts`
        // unwinds its own registrations on failure
        unsafe {
            IoDeleteDevice(device_object);
        }
        DEVICE_OBJECT.store(core::ptr::null_mut(), Ordering::Relaxed);
        return register_callouts_ntstatus;
    }

    println!("WFP Callout Driver Entry Complete!");
    STATUS_SUCCESS
}

/// Registers this driver's callout with the filter engine, and adds the
/// sublayer, callout entry, and filter that cause it to be invoked.
///
/// The sublayer, callout entry, and filter are all added inside a single
/// transaction so that a failure part-way through leaves nothing behind in the
/// filter engine. Because the session is opened with
/// [`FWPM_SESSION_FLAG_DYNAMIC`], the filter engine also discards everything
/// added through it when the session is closed, so [`driver_unload`] only has
/// to close the engine handle rather than delete each object.
fn register_callouts(device_object: PDEVICE_OBJECT) -> NTSTATUS {
    let callout = FWPS_CALLOUT2 {
        calloutKey: SAMPLE_CALLOUT_KEY,
        flags: 0,
        classifyFn: Some(classify),
        notifyFn: Some(notify),
        // Flow contexts are only needed by callouts that associate state with a data flow, which
        // this sample does not do.
        flowDeleteFn: None,
    };

    let mut callout_id: UINT32 = 0;
    let register_ntstatus =
        // SAFETY: This is safe because:
        //         1. `device_object` was successfully initialized by `IoCreateDevice` in
        //            `DriverEntry`
        //         2. `callout` is a fully initialized `FWPS_CALLOUT2` whose callbacks have the
        //            signatures the filter engine expects
        //         3. `callout_id` is a valid pointer to a `UINT32` for the filter engine to write
        //            the assigned identifier into
        unsafe {
            FwpsCalloutRegister2(
                device_object.cast::<core::ffi::c_void>(),
                &raw const callout,
                &raw mut callout_id,
            )
        };
    if !nt_success(register_ntstatus) {
        println!("FwpsCalloutRegister2 failed: {register_ntstatus:#010x}");
        return register_ntstatus;
    }
    CALLOUT_ID.store(callout_id, Ordering::Relaxed);

    let add_filter_engine_objects_ntstatus = add_filter_engine_objects();
    if !nt_success(add_filter_engine_objects_ntstatus) {
        // SAFETY: `callout_id` was successfully initialized by `FwpsCalloutRegister2`
        // above, and has not been unregistered yet
        let unregister_ntstatus = unsafe { FwpsCalloutUnregisterById0(callout_id) };
        if !nt_success(unregister_ntstatus) {
            println!("FwpsCalloutUnregisterById0 failed: {unregister_ntstatus:#010x}");
        }
        CALLOUT_ID.store(0, Ordering::Relaxed);
        return add_filter_engine_objects_ntstatus;
    }

    STATUS_SUCCESS
}

/// Opens a dynamic session with the filter engine and adds this driver's
/// sublayer, callout entry, and filter to it within a transaction.
///
/// On success the engine handle is left open in [`ENGINE_HANDLE`], since
/// closing it would tear down everything that was just added.
#[allow(
    clippy::too_many_lines,
    reason = "the sequence of filter engine calls, each with its own status check and cleanup, is \
              clearer read top to bottom than split across helpers that would each need the \
              engine handle and transaction state threaded through them"
)]
fn add_filter_engine_objects() -> NTSTATUS {
    let session = FWPM_SESSION0 {
        // A dynamic session is bound to this driver's lifetime: the filter engine deletes every
        // object added through it once the session is closed, so an abnormal unload cannot leave
        // stale filters behind.
        flags: FWPM_SESSION_FLAG_DYNAMIC,
        ..FWPM_SESSION0::default()
    };

    let mut engine_handle: HANDLE = core::ptr::null_mut();
    let engine_open_ntstatus =
        // SAFETY: This is safe because:
        //         1. A null `serverName` is required for a kernel-mode session
        //         2. A null `authIdentity` selects the calling thread's credentials, which for a
        //            driver is the system context
        //         3. `session` is a fully initialized `FWPM_SESSION0`
        //         4. `engine_handle` is a valid pointer to a `HANDLE` for the filter engine to
        //            write the resulting handle into
        unsafe {
            FwpmEngineOpen0(
                core::ptr::null(),
                RPC_C_AUTHN_WINNT,
                core::ptr::null_mut(),
                &raw const session,
                &raw mut engine_handle,
            )
        };
    if !nt_success(engine_open_ntstatus) {
        println!("FwpmEngineOpen0 failed: {engine_open_ntstatus:#010x}");
        return engine_open_ntstatus;
    }
    ENGINE_HANDLE.store(engine_handle, Ordering::Relaxed);

    let transaction_begin_ntstatus =
        // SAFETY: `engine_handle` was successfully initialized by `FwpmEngineOpen0` above
        unsafe { FwpmTransactionBegin0(engine_handle, 0) };
    if !nt_success(transaction_begin_ntstatus) {
        println!("FwpmTransactionBegin0 failed: {transaction_begin_ntstatus:#010x}");
        close_engine();
        return transaction_begin_ntstatus;
    }

    let add_ntstatus = add_sublayer_callout_and_filter(engine_handle);
    if !nt_success(add_ntstatus) {
        // SAFETY: A transaction was successfully begun by `FwpmTransactionBegin0`
        // above, and has neither been committed nor aborted yet
        let transaction_abort_ntstatus = unsafe { FwpmTransactionAbort0(engine_handle) };
        if !nt_success(transaction_abort_ntstatus) {
            println!("FwpmTransactionAbort0 failed: {transaction_abort_ntstatus:#010x}");
        }
        close_engine();
        return add_ntstatus;
    }

    let transaction_commit_ntstatus =
        // SAFETY: A transaction was successfully begun by `FwpmTransactionBegin0` above, and has
        // neither been committed nor aborted yet
        unsafe { FwpmTransactionCommit0(engine_handle) };
    if !nt_success(transaction_commit_ntstatus) {
        println!("FwpmTransactionCommit0 failed: {transaction_commit_ntstatus:#010x}");

        // A failed commit leaves the transaction open, so it still has to be aborted.
        // SAFETY: A transaction was successfully begun by `FwpmTransactionBegin0`
        // above, and the failed commit did not end it
        let transaction_abort_ntstatus = unsafe { FwpmTransactionAbort0(engine_handle) };
        if !nt_success(transaction_abort_ntstatus) {
            println!("FwpmTransactionAbort0 failed: {transaction_abort_ntstatus:#010x}");
        }
        close_engine();
        return transaction_commit_ntstatus;
    }

    STATUS_SUCCESS
}

/// Adds this driver's sublayer, callout entry, and filter to the open
/// transaction on `engine_handle`.
///
/// The caller aborts the transaction if this fails, so no cleanup is performed
/// here.
fn add_sublayer_callout_and_filter(engine_handle: HANDLE) -> NTSTATUS {
    let sublayer = FWPM_SUBLAYER0 {
        subLayerKey: SAMPLE_SUBLAYER_KEY,
        displayData: FWPM_DISPLAY_DATA0 {
            name: SUBLAYER_NAME.as_ptr().cast_mut(),
            description: SUBLAYER_DESCRIPTION.as_ptr().cast_mut(),
        },
        // A weight below that of `FWPM_SUBLAYER_UNIVERSAL` keeps this sublayer from taking
        // precedence over the platform's own filtering, as the `inspect` sample requires for IPsec
        // compatibility.
        weight: 0,
        ..FWPM_SUBLAYER0::default()
    };

    let sublayer_add_ntstatus =
        // SAFETY: This is safe because:
        //         1. `engine_handle` was successfully initialized by `FwpmEngineOpen0`
        //         2. `sublayer` is a fully initialized `FWPM_SUBLAYER0` whose display strings point
        //            at `'static` NUL-terminated UTF-16 arrays
        //         3. A null `sd` applies the filter engine's default security descriptor
        unsafe { FwpmSubLayerAdd0(engine_handle, &raw const sublayer, core::ptr::null_mut()) };
    if !nt_success(sublayer_add_ntstatus) {
        println!("FwpmSubLayerAdd0 failed: {sublayer_add_ntstatus:#010x}");
        return sublayer_add_ntstatus;
    }

    let management_callout = FWPM_CALLOUT0 {
        calloutKey: SAMPLE_CALLOUT_KEY,
        displayData: FWPM_DISPLAY_DATA0 {
            name: CALLOUT_NAME.as_ptr().cast_mut(),
            description: CALLOUT_DESCRIPTION.as_ptr().cast_mut(),
        },
        // SAFETY: `FWPM_LAYER_ALE_AUTH_CONNECT_V4` is a `'static` `GUID` defined by the filter
        // engine's headers, so reading it is always valid
        applicableLayer: unsafe { FWPM_LAYER_ALE_AUTH_CONNECT_V4 },
        ..FWPM_CALLOUT0::default()
    };

    let callout_add_ntstatus =
        // SAFETY: This is safe because:
        //         1. `engine_handle` was successfully initialized by `FwpmEngineOpen0`
        //         2. `management_callout` is a fully initialized `FWPM_CALLOUT0` whose display
        //            strings point at `'static` NUL-terminated UTF-16 arrays
        //         3. A null `sd` applies the filter engine's default security descriptor
        //         4. A null `id` declines the assigned identifier, which is not needed since the
        //            entry is deleted with the session
        unsafe {
            FwpmCalloutAdd0(
                engine_handle,
                &raw const management_callout,
                core::ptr::null_mut(),
                core::ptr::null_mut(),
            )
        };
    if !nt_success(callout_add_ntstatus) {
        println!("FwpmCalloutAdd0 failed: {callout_add_ntstatus:#010x}");
        return callout_add_ntstatus;
    }

    // Restricting the filter to TCP keeps the sample's output readable; a callout
    // that inspected every protocol would print for each UDP datagram as well.
    let mut filter_condition = FWPM_FILTER_CONDITION0 {
        // SAFETY: `FWPM_CONDITION_IP_PROTOCOL` is a `'static` `GUID` defined by the filter engine's
        // headers, so reading it is always valid
        fieldKey: unsafe { FWPM_CONDITION_IP_PROTOCOL },
        matchType: FWP_MATCH_EQUAL,
        conditionValue: FWP_CONDITION_VALUE0 {
            type_: FWP_UINT8,
            __bindgen_anon_1: FWP_CONDITION_VALUE0___bindgen_ty_1 {
                uint8: IPPROTO_TCP_AS_UINT8,
            },
        },
    };

    let filter = FWPM_FILTER0 {
        displayData: FWPM_DISPLAY_DATA0 {
            name: FILTER_NAME.as_ptr().cast_mut(),
            description: FILTER_DESCRIPTION.as_ptr().cast_mut(),
        },
        // SAFETY: `FWPM_LAYER_ALE_AUTH_CONNECT_V4` is a `'static` `GUID` defined by the filter
        // engine's headers, so reading it is always valid
        layerKey: unsafe { FWPM_LAYER_ALE_AUTH_CONNECT_V4 },
        subLayerKey: SAMPLE_SUBLAYER_KEY,
        // An empty weight lets the filter engine assign one automatically from the filter's
        // conditions.
        weight: FWP_VALUE0 {
            type_: FWP_EMPTY,
            ..FWP_VALUE0::default()
        },
        numFilterConditions: 1,
        filterCondition: &raw mut filter_condition,
        action: FWPM_ACTION0 {
            // An inspection callout observes connections without deciding their fate, so it never
            // needs the right to write an action. A terminating callout would use
            // `FWP_ACTION_CALLOUT_TERMINATING` instead.
            type_: FWP_ACTION_CALLOUT_INSPECTION,
            __bindgen_anon_1: FWPM_ACTION0___bindgen_ty_1 {
                calloutKey: SAMPLE_CALLOUT_KEY,
            },
        },
        ..FWPM_FILTER0::default()
    };

    let filter_add_ntstatus =
        // SAFETY: This is safe because:
        //         1. `engine_handle` was successfully initialized by `FwpmEngineOpen0`
        //         2. `filter` is a fully initialized `FWPM_FILTER0` whose display strings point at
        //            `'static` NUL-terminated UTF-16 arrays, and whose `filterCondition` points at
        //            `numFilterConditions` initialized conditions that outlive this call
        //         3. A null `sd` applies the filter engine's default security descriptor
        //         4. A null `id` declines the assigned identifier, which is not needed since the
        //            filter is deleted with the session
        unsafe {
            FwpmFilterAdd0(
                engine_handle,
                &raw const filter,
                core::ptr::null_mut(),
                core::ptr::null_mut(),
            )
        };
    if !nt_success(filter_add_ntstatus) {
        println!("FwpmFilterAdd0 failed: {filter_add_ntstatus:#010x}");
        return filter_add_ntstatus;
    }

    STATUS_SUCCESS
}

/// Closes this driver's session with the filter engine, if one is open, which
/// also deletes every object added through it.
fn close_engine() {
    let engine_handle = ENGINE_HANDLE.swap(core::ptr::null_mut(), Ordering::Relaxed);
    if engine_handle.is_null() {
        return;
    }

    // SAFETY: `engine_handle` was successfully initialized by `FwpmEngineOpen0`,
    // and the `swap` above ensures it is only closed once
    let engine_close_ntstatus = unsafe { FwpmEngineClose0(engine_handle) };
    if !nt_success(engine_close_ntstatus) {
        println!("FwpmEngineClose0 failed: {engine_close_ntstatus:#010x}");
    }
}

/// Called by the I/O manager when this driver is being unloaded (ex. via `sc
/// stop SampleWfpCallout`).
extern "C" fn driver_unload(_driver: *mut DRIVER_OBJECT) {
    println!("WFP Callout Driver Unload Entered!");

    // The engine session is closed before the callout is unregistered so that the
    // filter engine deletes the filter referencing the callout first; unregistering
    // a callout that a filter still refers to fails with `STATUS_DEVICE_BUSY`.
    close_engine();

    let callout_id = CALLOUT_ID.swap(0, Ordering::Relaxed);
    if callout_id != 0 {
        // SAFETY: `callout_id` was successfully initialized by `FwpsCalloutRegister2`
        // in `register_callouts`, and the `swap` above ensures it is only
        // unregistered once
        let unregister_ntstatus = unsafe { FwpsCalloutUnregisterById0(callout_id) };
        if !nt_success(unregister_ntstatus) {
            println!("FwpsCalloutUnregisterById0 failed: {unregister_ntstatus:#010x}");
        }
    }

    let device_object = DEVICE_OBJECT.swap(core::ptr::null_mut(), Ordering::Relaxed);
    if !device_object.is_null() {
        // SAFETY: `device_object` was successfully initialized by `IoCreateDevice` in
        // `DriverEntry`, the callout registered against it has been unregistered above,
        // and the `swap` ensures it is only deleted once
        unsafe {
            IoDeleteDevice(device_object);
        }
    }

    println!("WFP Callout Driver Unload Complete!");
}

/// `classifyFn` callout function, which the filter engine invokes for every
/// outbound TCP connection attempt that this driver's filter matches.
///
/// Since the filter's action is [`FWP_ACTION_CALLOUT_INSPECTION`], this
/// callout only prints what it saw and makes no permit or block decision. The
/// [`FWPS_RIGHT_ACTION_WRITE`] right is still honoured because inspection does
/// not guarantee it is clear: when the filter engine grants it, the callout
/// writes [`FWP_ACTION_CONTINUE`] to leave the decision to the next filter.
extern "C" fn classify(
    in_fixed_values: *const FWPS_INCOMING_VALUES0,
    _in_meta_values: *const FWPS_INCOMING_METADATA_VALUES0,
    _layer_data: *mut core::ffi::c_void,
    _classify_context: *const core::ffi::c_void,
    filter: *const FWPS_FILTER2,
    _flow_context: UINT64,
    classify_out: *mut FWPS_CLASSIFY_OUT0,
) {
    // SAFETY: `in_fixed_values` is provided by the filter engine and points to a
    // valid `FWPS_INCOMING_VALUES0` for the duration of this callback
    let in_fixed_values = unsafe { &*in_fixed_values };

    // SAFETY: `classify_out` is provided by the filter engine and points to a valid
    // `FWPS_CLASSIFY_OUT0` for the duration of this callback
    let classify_out = unsafe { &mut *classify_out };

    // The layer's values are indexed by the `FWPS_FIELD_*` constants for the layer
    // named in `layerId`, which is `FWPM_LAYER_ALE_AUTH_CONNECT_V4` here since that
    // is the only layer this driver's callout is registered at.
    if let Some(remote_address) = incoming_value_as_u32(
        in_fixed_values,
        FWPS_FIELD_ALE_AUTH_CONNECT_V4_IP_REMOTE_ADDRESS,
    ) && let Some(remote_port) = incoming_value_as_u16(
        in_fixed_values,
        FWPS_FIELD_ALE_AUTH_CONNECT_V4_IP_REMOTE_PORT,
    ) {
        // The ALE layers report IPv4 addresses in host byte order.
        let [a, b, c, d] = remote_address.to_be_bytes();
        println!("WFP Callout AuthConnect: TCP {a}.{b}.{c}.{d}:{remote_port}");
    }

    // An inspection callout makes no permit or block decision, so it returns
    // `FWP_ACTION_CONTINUE` to leave the classification to the next filter.
    // Inspection does not guarantee the write right is clear, so the action is
    // only written when the filter engine grants it.
    if classify_out.rights & FWPS_RIGHT_ACTION_WRITE == 0 {
        return;
    }

    classify_out.actionType = FWP_ACTION_CONTINUE;

    // SAFETY: `filter` is provided by the filter engine and points to a valid
    // `FWPS_FILTER2` for the duration of this callback
    let filter_flags = unsafe { &*filter }.flags;

    // The right to write an action is only surrendered when the filter asks for it,
    // so that a callout further down the chain can still override a permit.
    if UINT32::from(filter_flags) & FWPS_FILTER_FLAG_CLEAR_ACTION_RIGHT != 0 {
        classify_out.rights &= !FWPS_RIGHT_ACTION_WRITE;
    }
}

/// `notifyFn` callout function, which the filter engine invokes when a filter
/// referring to this driver's callout is added or deleted.
///
/// A callout that allocated per-filter state would do so here; this one has
/// none, so it only has to succeed. Returning a failure status from an add
/// notification causes the filter engine to reject the filter.
const extern "C" fn notify(
    _notify_type: FWPS_CALLOUT_NOTIFY_TYPE,
    _filter_key: *const GUID,
    _filter: *mut FWPS_FILTER2,
) -> NTSTATUS {
    STATUS_SUCCESS
}

/// Reads the [`UINT32`] a layer reported for `field`, or [`None`] if the layer
/// did not report that field.
///
/// A callout must not assume that every field its layer defines was populated:
/// `valueCount` reflects the fields the running version of the platform
/// provides, which can be fewer than the `FWPS_FIELD_*` constants the driver
/// was compiled against.
fn incoming_value_as_u32(
    in_fixed_values: &FWPS_INCOMING_VALUES0,
    field: FWPS_FIELDS_ALE_AUTH_CONNECT_V4,
) -> Option<UINT32> {
    let value = incoming_value(in_fixed_values, field)?;
    if value.type_ != FWP_UINT32 {
        return None;
    }

    // SAFETY: `uint32` is the active member of the union, since the layer reported
    // this field's type as `FWP_UINT32`
    Some(unsafe { value.__bindgen_anon_1.uint32 })
}

/// Reads the [`UINT16`] a layer reported for `field`, or [`None`] if the layer
/// did not report that field.
fn incoming_value_as_u16(
    in_fixed_values: &FWPS_INCOMING_VALUES0,
    field: FWPS_FIELDS_ALE_AUTH_CONNECT_V4,
) -> Option<UINT16> {
    let value = incoming_value(in_fixed_values, field)?;
    if value.type_ != FWP_UINT16 {
        return None;
    }

    // SAFETY: `uint16` is the active member of the union, since the layer reported
    // this field's type as `FWP_UINT16`
    Some(unsafe { value.__bindgen_anon_1.uint16 })
}

/// Reads the raw [`FWP_VALUE0`] a layer reported for `field`, or [`None`] if
/// `field` is beyond the values the layer populated.
fn incoming_value(
    in_fixed_values: &FWPS_INCOMING_VALUES0,
    field: FWPS_FIELDS_ALE_AUTH_CONNECT_V4,
) -> Option<FWP_VALUE0> {
    let index = usize::try_from(field).ok()?;
    if index >= usize::try_from(in_fixed_values.valueCount).ok()? {
        return None;
    }

    // SAFETY: `incomingValue` is provided by the filter engine and points to
    // `valueCount` consecutive `FWPS_INCOMING_VALUE0`s, and `index` was checked to
    // be less than `valueCount` above, so the offset stays in bounds
    let incoming_value = unsafe { in_fixed_values.incomingValue.add(index) };

    // SAFETY: `incoming_value` points to an initialized `FWPS_INCOMING_VALUE0`
    // within the array the filter engine provided, which remains valid for the
    // duration of the callback that is reading it
    let value = unsafe { *incoming_value }.value;

    Some(value)
}

/// [`IPPROTO_TCP`] narrowed to the [`UINT8`] that an `FWP_UINT8` condition
/// value holds.
///
/// `IPPROTO_*` is generated as a C `int`, but the IP protocol field of a WFP
/// layer is a single byte. An `as` cast would silently truncate, and `TryFrom`
/// is not yet callable in a `const` context, so this narrows via the value's
/// little-endian bytes and fails the build if any discarded byte is non-zero.
const IPPROTO_TCP_AS_UINT8: UINT8 = match IPPROTO_TCP.to_le_bytes() {
    [protocol, 0, 0, 0] => protocol,
    _ => panic!("IPPROTO_TCP should fit in a UINT8"),
};

// The display strings the filter engine shows for this driver's objects (ex. in
// `netsh wfp show state` output). `FWPM_DISPLAY_DATA0` holds `wchar_t`
// pointers, so these have to be NUL-terminated UTF-16 arrays rather than Rust
// string literals.

/// Encodes an ASCII string literal into the NUL-terminated [`WCHAR`] array that
/// [`FWPM_DISPLAY_DATA0`] expects, failing to compile if the literal is not
/// ASCII.
///
/// One UTF-16 code unit per byte only holds for ASCII, which is what lets the
/// array's length be derived from the literal instead of hardcoded alongside
/// it.
const fn ascii_to_utf16<const LENGTH: usize>(ascii: &str) -> [WCHAR; LENGTH] {
    let bytes = ascii.as_bytes();
    assert!(
        bytes.len() + 1 == LENGTH,
        "LENGTH should be the literal's length plus a NUL"
    );

    let mut utf16 = [0; LENGTH];

    let mut index = 0;
    while index < bytes.len() {
        assert!(bytes[index].is_ascii(), "display strings should be ASCII");
        utf16[index] = bytes[index] as WCHAR;
        index += 1;
    }

    utf16
}

/// Declares a `static` holding an ASCII string literal encoded as a
/// NUL-terminated [`WCHAR`] array, sized from the literal itself.
macro_rules! display_string {
    ($(#[$attribute:meta])* $name:ident = $string:literal) => {
        $(#[$attribute])*
        static $name: [WCHAR; $string.len() + 1] = ascii_to_utf16($string);
    };
}

display_string! {
    /// Display name of this driver's sublayer.
    SUBLAYER_NAME = "Sample WFP Callout Sublayer"
}

display_string! {
    /// Display description of this driver's sublayer.
    SUBLAYER_DESCRIPTION = "Sublayer for use by the sample WFP callout"
}

display_string! {
    /// Display name of this driver's callout.
    CALLOUT_NAME = "Sample WFP Callout"
}

display_string! {
    /// Display description of this driver's callout.
    CALLOUT_DESCRIPTION = "Prints outbound TCP connection attempts"
}

display_string! {
    /// Display name of this driver's filter.
    FILTER_NAME = "Sample WFP Filter"
}

display_string! {
    /// Display description of this driver's filter.
    FILTER_DESCRIPTION = "Matches every outbound TCP connection"
}
