// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Direct FFI bindings to NTDDK APIs from the Windows Driver Kit (WDK)
//!
//! This module contains all bindings to functions, constants, methods,
//! constructors and destructors in `ntddk.h`. Types are not included in this
//! module, but are available in the top-level `wdk_sys` module.

pub use bindings::*;

use crate::{HANDLE, OBJECT_ATTRIBUTES, POBJECT_ATTRIBUTES, PSECURITY_DESCRIPTOR, PUNICODE_STRING, ULONG};
use core::ptr::null_mut;

#[allow(missing_docs)]
mod bindings {
    #[allow(
        clippy::wildcard_imports,
        reason = "the underlying c code relies on all type definitions being in scope, which \
                  results in the bindgen generated code relying on the generated types being in \
                  scope as well"
    )]
    use crate::types::*;

    include!(concat!(env!("OUT_DIR"), "/ntddk.rs"));
}

/// The InitializeObjectAttributes macro initializes the opaque OBJECT_ATTRIBUTES structure,
/// which specifies the properties of an object handle to routines that open handles.
///
/// # Parameters
/// - `p`: A pointer to an OBJECT_ATTRIBUTES structure to be initialised (output).
/// - `n`: A pointer to a UNICODE_STRING that specifies the object name (input).
/// - `a`: The object attributes (input).
/// - `r`: A pointer to the root directory (input).
/// - `s`: A pointer to the security descriptor (input, optional).
///
/// # Returns
/// This function returns:
/// - `Ok(())` if the `POBJECT_ATTRIBUTES` pointer was valid, indicating
/// the fields of the input OBJECT_ATTRIBUTES were assigned to.
/// - `Err(())` if the `POBJECT_ATTRIBUTES` pointer was null.
///
/// # Safety
/// This function accesses raw pointers and modifies memory. Ensure all pointers
/// are valid before calling this function. The function will check for an invalid
/// POBJECT_ATTRIBUTES before attempting to dereference it.
/// 
/// # Example
/// ```
/// // Define a UNICODE_STRING with the desired string
/// let mut log_path_unicode = UNICODE_STRING::default();
/// let src = "My Unicode string".encode_utf16().chain(once(0)).collect::<Vec<_>>();
/// unsafe { RtlInitUnicodeString(&mut log_path_unicode, src.as_ptr()) };
/// 
/// // Prepare the OBJECT_ATTRIBUTES structure
/// let mut oa: OBJECT_ATTRIBUTES = OBJECT_ATTRIBUTES::default();
/// 
/// // Initialise the OBJECT_ATTRIBUTES structure
/// let result = unsafe {
///     InitializeObjectAttributes(
///         &mut oa,
///         &mut log_path_unicode,
///         OBJ_CASE_INSENSITIVE | OBJ_KERNEL_HANDLE,
///         null_mut(),
///         null_mut(),      
///     )
/// };
/// 
/// // Handle the result
/// match result {
///     Ok(()) => println!("OBJECT_ATTRIBUTES initialized successfully"),
///     Err(()) => eprintln!("Failed to initialize OBJECT_ATTRIBUTES"),
/// }
/// ```
#[allow(non_snake_case)]
pub unsafe fn InitializeObjectAttributes(
    p: POBJECT_ATTRIBUTES,
    n: PUNICODE_STRING,
    a: ULONG,
    r: HANDLE,
    s: PSECURITY_DESCRIPTOR,
) -> Result<(), ()>{
    // Check the validity of the OBJECT_ATTRIBUTES pointer
    if p.is_null() {
        return Err(());
    }

    // Assign values to the callers OBJECT_ATTRIBUTES structure
    unsafe {
        (*p).Length = size_of::<OBJECT_ATTRIBUTES>() as u32;
        (*p).RootDirectory = r;
        (*p).Attributes = a;
        (*p).ObjectName = n;
        (*p).SecurityDescriptor = s;
        (*p).SecurityQualityOfService = null_mut();
    }

    Ok(())
}