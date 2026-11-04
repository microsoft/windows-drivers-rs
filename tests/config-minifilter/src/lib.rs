// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Compile-time checks that the `minifilter` API subset of `wdk-sys` generates
//! the Filter Manager routines a File System Minifilter driver needs, with
//! signatures that accept the generated types.
//!
//! These live in the library rather than in `tests/` because naming a `Flt*`
//! routine adds a real import on `FLTMGR.SYS`, which is a kernel image: the
//! `#[test]` harness produces a user-mode executable that would fail to load
//! with `STATUS_DLL_NOT_FOUND`. Coercing each routine to an explicitly written
//! `fn` type in a `const` gets the same signature check from `rustc` with no
//! runtime component at all, so a bindgen regression that drops a routine or
//! changes an argument type is a build failure.

/// Signatures of the Filter Manager routines a minifilter cannot avoid calling.
/// Written out longhand so that they are checked against `fltKernel.h` via the
/// generated bindings rather than inferred from them.
mod filter_manager_routine_signatures {
    use wdk_sys::{
        FLT_FILE_NAME_OPTIONS,
        FLT_REGISTRATION,
        NTSTATUS,
        PDRIVER_OBJECT,
        PFLT_CALLBACK_DATA,
        PFLT_FILE_NAME_INFORMATION,
        PFLT_FILTER,
        minifilter,
    };

    const _FLT_REGISTER_FILTER: unsafe extern "C" fn(
        PDRIVER_OBJECT,
        *const FLT_REGISTRATION,
        *mut PFLT_FILTER,
    ) -> NTSTATUS = minifilter::FltRegisterFilter;

    const _FLT_START_FILTERING: unsafe extern "C" fn(PFLT_FILTER) -> NTSTATUS =
        minifilter::FltStartFiltering;

    const _FLT_UNREGISTER_FILTER: unsafe extern "C" fn(PFLT_FILTER) =
        minifilter::FltUnregisterFilter;

    const _FLT_GET_FILE_NAME_INFORMATION: unsafe extern "C" fn(
        PFLT_CALLBACK_DATA,
        FLT_FILE_NAME_OPTIONS,
        *mut PFLT_FILE_NAME_INFORMATION,
    ) -> NTSTATUS = minifilter::FltGetFileNameInformation;

    const _FLT_PARSE_FILE_NAME_INFORMATION: unsafe extern "C" fn(
        PFLT_FILE_NAME_INFORMATION,
    ) -> NTSTATUS = minifilter::FltParseFileNameInformation;

    const _FLT_RELEASE_FILE_NAME_INFORMATION: unsafe extern "C" fn(PFLT_FILE_NAME_INFORMATION) =
        minifilter::FltReleaseFileNameInformation;
}

/// Signatures of the routines a minifilter uses to talk to a user-mode
/// component over a filter communication port. A minifilter that reports
/// anything to userland needs all of these, and each one is a place where a
/// wrong argument type would only surface as stack corruption in kernel-mode.
mod communication_port_routine_signatures {
    use wdk_sys::{
        ACCESS_MASK,
        LONG,
        NTSTATUS,
        PFLT_CONNECT_NOTIFY,
        PFLT_DISCONNECT_NOTIFY,
        PFLT_FILTER,
        PFLT_MESSAGE_NOTIFY,
        PFLT_PORT,
        PLARGE_INTEGER,
        POBJECT_ATTRIBUTES,
        PSECURITY_DESCRIPTOR,
        PULONG,
        PVOID,
        ULONG,
        minifilter,
    };

    const _FLT_CREATE_COMMUNICATION_PORT: unsafe extern "C" fn(
        PFLT_FILTER,
        *mut PFLT_PORT,
        POBJECT_ATTRIBUTES,
        PVOID,
        PFLT_CONNECT_NOTIFY,
        PFLT_DISCONNECT_NOTIFY,
        PFLT_MESSAGE_NOTIFY,
        LONG,
    ) -> NTSTATUS = minifilter::FltCreateCommunicationPort;

    const _FLT_CLOSE_COMMUNICATION_PORT: unsafe extern "C" fn(PFLT_PORT) =
        minifilter::FltCloseCommunicationPort;

    const _FLT_CLOSE_CLIENT_PORT: unsafe extern "C" fn(PFLT_FILTER, *mut PFLT_PORT) =
        minifilter::FltCloseClientPort;

    // The `DesiredAccess` parameter is why `minifilter::FLT_PORT_ALL_ACCESS` is
    // typed as an `ACCESS_MASK` rather than left as a `u32`.
    const _FLT_BUILD_DEFAULT_SECURITY_DESCRIPTOR: unsafe extern "C" fn(
        *mut PSECURITY_DESCRIPTOR,
        ACCESS_MASK,
    ) -> NTSTATUS = minifilter::FltBuildDefaultSecurityDescriptor;

    const _FLT_FREE_SECURITY_DESCRIPTOR: unsafe extern "C" fn(PSECURITY_DESCRIPTOR) =
        minifilter::FltFreeSecurityDescriptor;

    const _FLT_SEND_MESSAGE: unsafe extern "C" fn(
        PFLT_FILTER,
        *mut PFLT_PORT,
        PVOID,
        ULONG,
        PVOID,
        PULONG,
        PLARGE_INTEGER,
    ) -> NTSTATUS = minifilter::FltSendMessage;
}
