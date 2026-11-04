// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! # Sample Minifilter Driver
//!
//! This is a sample File System Minifilter driver that demonstrates how to use
//! the crates in windows-driver-rs to create a skeleton of a minifilter driver.
//!
//! It is a port of the [`passThrough` sample](https://github.com/microsoft/Windows-driver-samples/tree/main/filesys/miniFilter/passThrough)
//! from the WDK driver samples, reduced to a single filtered operation
//! (`IRP_MJ_CREATE`). It registers itself with the Filter Manager, attaches to
//! every volume, and prints the name of each file that is opened.

#![no_std]

extern crate alloc;

#[cfg(not(test))]
extern crate wdk_panic;

use alloc::{slice, string::String};
use core::sync::atomic::{AtomicPtr, Ordering};

use wdk::{nt_success, println};
#[cfg(not(test))]
use wdk_alloc::WdkAllocator;
use wdk_sys::{
    _FLT_FILTER,
    _FLT_POSTOP_CALLBACK_STATUS::FLT_POSTOP_FINISHED_PROCESSING,
    _FLT_PREOP_CALLBACK_STATUS::FLT_PREOP_SUCCESS_WITH_CALLBACK,
    DRIVER_OBJECT,
    FLT_FILE_NAME_NORMALIZED,
    FLT_FILE_NAME_QUERY_DEFAULT,
    FLT_FILTER_UNLOAD_FLAGS,
    FLT_OPERATION_REGISTRATION,
    FLT_POST_OPERATION_FLAGS,
    FLT_POSTOP_CALLBACK_STATUS,
    FLT_PREOP_CALLBACK_STATUS,
    FLT_REGISTRATION,
    FLT_REGISTRATION_VERSION,
    FLTFL_REGISTRATION_DO_NOT_SUPPORT_SERVICE_STOP,
    IRP_MJ_CREATE,
    NTSTATUS,
    PCFLT_RELATED_OBJECTS,
    PCUNICODE_STRING,
    PDRIVER_OBJECT,
    PFLT_CALLBACK_DATA,
    PFLT_FILE_NAME_INFORMATION,
    PFLT_FILTER,
    PVOID,
    STATUS_SUCCESS,
    UCHAR,
    UNICODE_STRING,
    USHORT,
    WCHAR,
    minifilter::{
        FltGetFileNameInformation,
        FltParseFileNameInformation,
        FltRegisterFilter,
        FltReleaseFileNameInformation,
        FltStartFiltering,
        FltUnregisterFilter,
        IRP_MJ_OPERATION_END,
    },
};

#[cfg(not(test))]
#[global_allocator]
static GLOBAL_ALLOCATOR: WdkAllocator = WdkAllocator;

/// Handle to this minifilter's registration with the Filter Manager, returned
/// by [`FltRegisterFilter`] and consumed by [`FltUnregisterFilter`].
///
/// The Filter Manager serializes `DriverEntry` against the unload callback, so
/// an [`AtomicPtr`] with [`Ordering::Relaxed`] accesses is sufficient here; it
/// is used in place of a `static mut` so that reading the handle is safe.
static FILTER_HANDLE: AtomicPtr<_FLT_FILTER> = AtomicPtr::new(core::ptr::null_mut());

/// Newtype that allows the operation registration table to live in a `static`.
///
/// [`FLT_OPERATION_REGISTRATION`] contains a raw `Reserved1` pointer, so it
/// does not implement [`Sync`], but the table below is immutable and the Filter
/// Manager only ever reads it.
struct OperationRegistration([FLT_OPERATION_REGISTRATION; 2]);

// SAFETY: The only field of `FLT_OPERATION_REGISTRATION` that is not `Sync` is
// `Reserved1`, which is null in every entry of `OPERATION_REGISTRATION` and is
// never written to afterwards.
unsafe impl Sync for OperationRegistration {}

/// The operations this minifilter registers callbacks for. The Filter Manager
/// walks this array until it reaches the [`IRP_MJ_OPERATION_END`] sentinel, so
/// the terminating entry is required.
static OPERATION_REGISTRATION: OperationRegistration = OperationRegistration([
    FLT_OPERATION_REGISTRATION {
        MajorFunction: IRP_MJ_CREATE_AS_MAJOR_FUNCTION,
        Flags: 0,
        PreOperation: Some(pre_create_operation),
        PostOperation: Some(post_create_operation),
        Reserved1: core::ptr::null_mut(),
    },
    FLT_OPERATION_REGISTRATION {
        MajorFunction: IRP_MJ_OPERATION_END,
        Flags: 0,
        PreOperation: None,
        PostOperation: None,
        Reserved1: core::ptr::null_mut(),
    },
]);

/// [`IRP_MJ_CREATE`] narrowed to the [`UCHAR`] that
/// `FLT_OPERATION_REGISTRATION::MajorFunction` expects.
const IRP_MJ_CREATE_AS_MAJOR_FUNCTION: UCHAR = uchar_from_u32(IRP_MJ_CREATE);

// The `IRP_MJ_*` constants and `FLT_REGISTRATION_VERSION` are generated as
// `u32`, and `core::mem::size_of` returns a `usize`, but the `FLT_REGISTRATION`
// and `FLT_OPERATION_REGISTRATION` fields they initialize are narrower. `as`
// casts would silently truncate, and `TryFrom` is not yet callable in a `const`
// context, so these helpers narrow via the value's little-endian bytes and fail
// the build if any discarded byte is non-zero.

/// Narrows a `u32` to a [`UCHAR`], failing to compile if the value does not
/// fit.
const fn uchar_from_u32(value: u32) -> UCHAR {
    match value.to_le_bytes() {
        [uchar, 0, 0, 0] => uchar,
        _ => panic!("value should fit in a UCHAR"),
    }
}

/// Narrows a `u32` to a [`USHORT`], failing to compile if the value does not
/// fit.
const fn ushort_from_u32(value: u32) -> USHORT {
    match value.to_le_bytes() {
        [low, high, 0, 0] => USHORT::from_le_bytes([low, high]),
        _ => panic!("value should fit in a USHORT"),
    }
}

/// Narrows a `usize` to a [`USHORT`], failing to compile if the value does not
/// fit. `usize` is a different width per target, so the discarded bytes are
/// checked with a loop instead of a pattern.
const fn ushort_from_usize(value: usize) -> USHORT {
    let bytes = value.to_le_bytes();

    let mut index = core::mem::size_of::<USHORT>();
    while index < bytes.len() {
        assert!(bytes[index] == 0, "value should fit in a USHORT");
        index += 1;
    }

    USHORT::from_le_bytes([bytes[0], bytes[1]])
}

/// `DriverEntry` function required by the Filter Manager
///
/// # Safety
/// Function is unsafe since it dereferences raw pointers passed to it by the
/// Filter Manager
// SAFETY: "DriverEntry" is the required symbol name for Windows driver entry points.
// No other function in this compilation unit exports this name, preventing symbol conflicts.
#[unsafe(export_name = "DriverEntry")]
pub unsafe extern "system" fn driver_entry(
    driver: &mut DRIVER_OBJECT,
    registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    let registry_path =
        // SAFETY: `registry_path` is provided by the Filter Manager, is never null, and points to
        // a valid `UNICODE_STRING` for the duration of `DriverEntry`
        unicode_string_to_string(unsafe { *registry_path });
    println!("Minifilter Driver Entry! Driver Registry Parameter Key: {registry_path}");

    let filter_registration = FLT_REGISTRATION {
        Size: FLT_REGISTRATION_SIZE,
        Version: FLT_REGISTRATION_VERSION_AS_USHORT,
        Flags: FLTFL_REGISTRATION_DO_NOT_SUPPORT_SERVICE_STOP,
        OperationRegistration: OPERATION_REGISTRATION.0.as_ptr(),
        FilterUnloadCallback: Some(filter_unload),
        ..FLT_REGISTRATION::default()
    };

    let mut filter_handle: PFLT_FILTER = core::ptr::null_mut();
    let flt_register_filter_ntstatus =
        // SAFETY: This is safe because:
        //         1. `driver` is provided by `DriverEntry` and is never null
        //         2. `filter_registration` is a valid, fully initialized `FLT_REGISTRATION` whose
        //            `OperationRegistration` points at a `'static` array terminated by
        //            `IRP_MJ_OPERATION_END`
        //         3. `filter_handle` is a valid pointer to a `PFLT_FILTER` for the Filter Manager
        //            to write the resulting handle into
        unsafe {
            FltRegisterFilter(
                driver as PDRIVER_OBJECT,
                &raw const filter_registration,
                &raw mut filter_handle,
            )
        };
    if !nt_success(flt_register_filter_ntstatus) {
        println!("FltRegisterFilter failed: {flt_register_filter_ntstatus:#010x}");
        return flt_register_filter_ntstatus;
    }
    FILTER_HANDLE.store(filter_handle, Ordering::Relaxed);

    let flt_start_filtering_ntstatus =
        // SAFETY: `filter_handle` was successfully initialized by `FltRegisterFilter` above
        unsafe { FltStartFiltering(filter_handle) };
    if !nt_success(flt_start_filtering_ntstatus) {
        println!("FltStartFiltering failed: {flt_start_filtering_ntstatus:#010x}");

        // SAFETY: `filter_handle` was successfully initialized by `FltRegisterFilter`
        // above, and has not been unregistered yet
        unsafe {
            FltUnregisterFilter(filter_handle);
        }
        return flt_start_filtering_ntstatus;
    }

    println!("Minifilter Driver Entry Complete!");
    STATUS_SUCCESS
}

/// [`FLT_REGISTRATION`]'s size, which the Filter Manager uses to detect that
/// the driver was built against a compatible revision of the structure.
const FLT_REGISTRATION_SIZE: USHORT = ushort_from_usize(core::mem::size_of::<FLT_REGISTRATION>());

/// [`FLT_REGISTRATION_VERSION`] narrowed to the [`USHORT`] that
/// `FLT_REGISTRATION::Version` expects.
const FLT_REGISTRATION_VERSION_AS_USHORT: USHORT = ushort_from_u32(FLT_REGISTRATION_VERSION);

/// Called by the Filter Manager when this minifilter is being unloaded (ex. via
/// `fltmc unload SampleMinifilter`).
extern "C" fn filter_unload(_flags: FLT_FILTER_UNLOAD_FLAGS) -> NTSTATUS {
    println!("Minifilter Unload Entered!");

    let filter_handle = FILTER_HANDLE.swap(core::ptr::null_mut(), Ordering::Relaxed);

    // SAFETY: `filter_handle` was successfully initialized by `FltRegisterFilter`
    // in `DriverEntry`, which the Filter Manager guarantees has completed
    // successfully before it calls this callback. The `swap` above ensures it
    // is only unregistered once.
    unsafe {
        FltUnregisterFilter(filter_handle);
    }

    println!("Minifilter Unload Complete!");
    STATUS_SUCCESS
}

/// Pre-operation callback for `IRP_MJ_CREATE`, which prints the name of the
/// file being opened.
///
/// Returning `FLT_PREOP_SUCCESS_WITH_CALLBACK` passes the operation down to the
/// next minifilter in the stack and requests that [`post_create_operation`] be
/// invoked once it completes.
extern "C" fn pre_create_operation(
    data: PFLT_CALLBACK_DATA,
    _flt_objects: PCFLT_RELATED_OBJECTS,
    _completion_context: *mut PVOID,
) -> FLT_PREOP_CALLBACK_STATUS {
    // Name queries can require paging I/O, so they are only safe at passive `IRQL`.
    // Skip the print rather than the operation itself if the name cannot be
    // retrieved.
    if let Some(file_name) = file_name(data) {
        println!("Minifilter PreCreate: {file_name}");
    }

    FLT_PREOP_SUCCESS_WITH_CALLBACK
}

/// Post-operation callback for `IRP_MJ_CREATE`, which prints the `NTSTATUS` the
/// file system returned for the open.
extern "C" fn post_create_operation(
    data: PFLT_CALLBACK_DATA,
    _flt_objects: PCFLT_RELATED_OBJECTS,
    _completion_context: PVOID,
    _flags: FLT_POST_OPERATION_FLAGS,
) -> FLT_POSTOP_CALLBACK_STATUS {
    let io_status =
        // SAFETY: `data` is provided by the Filter Manager and points to a valid
        // `FLT_CALLBACK_DATA` for the duration of this callback
        unsafe { *data }.IoStatus;

    let ntstatus =
        // SAFETY: `Status` is the active member of `IoStatus`'s union for an operation that the
        // file system has completed, which is the only case in which this callback is invoked
        unsafe { io_status.__bindgen_anon_1.Status };
    println!("Minifilter PostCreate NTSTATUS: {ntstatus:#010x}");

    FLT_POSTOP_FINISHED_PROCESSING
}

/// Retrieves the normalized name of the file an operation targets, or [`None`]
/// if the Filter Manager could not provide one (ex. because the current `IRQL`
/// is too high, or the file has no name).
fn file_name(data: PFLT_CALLBACK_DATA) -> Option<String> {
    let mut file_name_information: PFLT_FILE_NAME_INFORMATION = core::ptr::null_mut();

    let ntstatus =
        // SAFETY: `data` is provided by the Filter Manager and points to a valid
        // `FLT_CALLBACK_DATA` for the duration of the callback that called this function
        unsafe {
            FltGetFileNameInformation(
                data,
                FLT_FILE_NAME_NORMALIZED | FLT_FILE_NAME_QUERY_DEFAULT,
                &raw mut file_name_information,
            )
        };
    if !nt_success(ntstatus) {
        return None;
    }

    // SAFETY: `file_name_information` was successfully initialized by
    // `FltGetFileNameInformation` above
    let parse_ntstatus = unsafe { FltParseFileNameInformation(file_name_information) };

    let file_name = if nt_success(parse_ntstatus) {
        // SAFETY: `file_name_information` was successfully initialized by
        // `FltGetFileNameInformation` above, and remains valid until it is released
        // below
        let name = unsafe { *file_name_information }.Name;
        Some(unicode_string_to_string(name))
    } else {
        None
    };

    // SAFETY: `file_name_information` was successfully initialized by
    // `FltGetFileNameInformation` above, and is not used after this point
    unsafe {
        FltReleaseFileNameInformation(file_name_information);
    }

    file_name
}

/// Translates a [`UNICODE_STRING`] into an owned [`String`], replacing any
/// unpaired surrogates with [`char::REPLACEMENT_CHARACTER`].
///
/// This allocates through the global allocator and is therefore infallible: an
/// allocation failure would bugcheck. A production driver should use
/// `wdk_alloc::fallible` instead. The sample accepts the tradeoff to keep the
/// print path simple.
fn unicode_string_to_string(unicode_string: UNICODE_STRING) -> String {
    let number_of_slice_elements = unicode_string.Length as usize / core::mem::size_of::<WCHAR>();

    String::from_utf16_lossy(
        // SAFETY: This is safe because:
        //         1. `unicode_string.Buffer` is valid for reads for `number_of_slice_elements` *
        //            `core::mem::size_of::<WCHAR>()` bytes, and `UNICODE_STRING` guarantees that
        //            `Buffer` is properly aligned.
        //         2. `unicode_string.Buffer` points to `number_of_slice_elements` consecutive
        //            properly initialized values of type `WCHAR`, since `Length` is the length of
        //            the string in bytes.
        //         3. Windows does not mutate the memory referenced by the returned slice for its
        //            entire lifetime.
        //         4. The total size, `number_of_slice_elements` * `core::mem::size_of::<WCHAR>()`,
        //            of the slice is no larger than `isize::MAX`, since `Length` is a `u16`.
        unsafe { slice::from_raw_parts(unicode_string.Buffer, number_of_slice_elements) },
    )
}
