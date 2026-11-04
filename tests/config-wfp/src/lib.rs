// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Compile-time checks that the `wfp` API subset of `wdk-sys` generates the
//! Windows Filtering Platform routines a kernel-mode callout driver needs, with
//! signatures that accept the generated types.
//!
//! These live in the library rather than in `tests/` because naming a `Fwps*` or
//! `Fwpm*` routine adds a real import on a kernel image: the `#[test]` harness
//! produces a user-mode executable that would fail to load with
//! `STATUS_DLL_NOT_FOUND`. Coercing each routine to an explicitly written `fn`
//! type in a `const` gets the same signature check from `rustc` with no runtime
//! component at all, so a bindgen regression that drops a routine or changes an
//! argument type is a build failure.

/// Signatures of the routines a callout driver uses to register with the filter
/// engine and to classify packets. Written out longhand so that they are checked
/// against `fwpsk.h` and `fwpmk.h` via the generated bindings rather than
/// inferred from them.
mod wfp_routine_signatures {
    use wdk_sys::{
        FWPM_CALLOUT0,
        FWPM_FILTER0,
        FWPM_SESSION0,
        FWPS_CALLOUT3,
        GUID,
        HANDLE,
        NET_BUFFER_LIST,
        NTSTATUS,
        PSECURITY_DESCRIPTOR,
        SEC_WINNT_AUTH_IDENTITY_W,
        UINT16,
        UINT32,
        UINT64,
        wchar_t,
        wfp,
    };

    // Callout registration and teardown (`fwpsk.h`). `FwpsCalloutRegister3` is
    // what `fwpvi.h` resolves the unsuffixed `FwpsCalloutRegister` to for a
    // Windows 10 RS3 or later target.
    const _FWPS_CALLOUT_REGISTER3: unsafe extern "C" fn(
        *mut core::ffi::c_void,
        *const FWPS_CALLOUT3,
        *mut UINT32,
    ) -> NTSTATUS = wfp::FwpsCalloutRegister3;

    const _FWPS_CALLOUT_UNREGISTER_BY_KEY0: unsafe extern "C" fn(*const GUID) -> NTSTATUS =
        wfp::FwpsCalloutUnregisterByKey0;

    // Filter engine session management (`fwpmk.h`). A callout driver opens a
    // session, then adds its callout and filters inside a transaction.
    const _FWPM_ENGINE_OPEN0: unsafe extern "C" fn(
        *const wchar_t,
        UINT32,
        *mut SEC_WINNT_AUTH_IDENTITY_W,
        *const FWPM_SESSION0,
        *mut HANDLE,
    ) -> NTSTATUS = wfp::FwpmEngineOpen0;

    const _FWPM_ENGINE_CLOSE0: unsafe extern "C" fn(HANDLE) -> NTSTATUS = wfp::FwpmEngineClose0;

    const _FWPM_TRANSACTION_BEGIN0: unsafe extern "C" fn(HANDLE, UINT32) -> NTSTATUS =
        wfp::FwpmTransactionBegin0;

    const _FWPM_TRANSACTION_COMMIT0: unsafe extern "C" fn(HANDLE) -> NTSTATUS =
        wfp::FwpmTransactionCommit0;

    const _FWPM_TRANSACTION_ABORT0: unsafe extern "C" fn(HANDLE) -> NTSTATUS =
        wfp::FwpmTransactionAbort0;

    const _FWPM_CALLOUT_ADD0: unsafe extern "C" fn(
        HANDLE,
        *const FWPM_CALLOUT0,
        PSECURITY_DESCRIPTOR,
        *mut UINT32,
    ) -> NTSTATUS = wfp::FwpmCalloutAdd0;

    const _FWPM_FILTER_ADD0: unsafe extern "C" fn(
        HANDLE,
        *const FWPM_FILTER0,
        PSECURITY_DESCRIPTOR,
        *mut UINT64,
    ) -> NTSTATUS = wfp::FwpmFilterAdd0;

    // Packet inspection (`fwpsk.h`). These are the routines that force `fwpsk.h`
    // to see a complete `NET_BUFFER_LIST`, so they cover the NDIS contract that
    // `wdk-build` declares on the subset's behalf.
    const _FWPS_NET_BUFFER_LIST_ASSOCIATE_CONTEXT1: unsafe extern "C" fn(
        *mut NET_BUFFER_LIST,
        UINT16,
        UINT64,
        UINT64,
        *mut GUID,
        *mut core::ffi::c_void,
        wdk_sys::FWPS_NET_BUFFER_LIST_NOTIFY_FN1,
        UINT32,
    ) -> NTSTATUS = wfp::FwpsNetBufferListAssociateContext1;
}
