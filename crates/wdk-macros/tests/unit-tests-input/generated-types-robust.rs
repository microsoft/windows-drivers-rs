// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Snippet of a bindgen-generated file containing types information used by tests for [`wdk_macros::call_unsafe_wdf_function_binding!`]

pub mod _WDFFUNCENUM {
    pub const WdfDriverCreateTableIndex: Type = 116;
    pub const WdfVerifierDbgBreakPointTableIndex: Type = 367;
    // Some entries in `WDFFUNCENUM` do not correspond to bindgen-generated functions (ex. private WDF APIs).
    // Parsing these entries should ignore the generated error from `generate_parameters_and_return_type()`.
    pub const WdfCxDeviceInitAllocateTableIndex: Type = 396;
    // Some entries in `WDFFUNCENUM` do not correspond to a legitimate TableIndex, like `WdfFunctionTableNumEntries`. 
    // Build script should ignore any entry that doesn't end in `TableIndex`.
    pub const WdfFunctionTableNumEntries: Type = 458;
}

pub type PFN_WDFDRIVERCREATE = ::core::option::Option<
    unsafe extern "C" fn(
        DriverGlobals: PWDF_DRIVER_GLOBALS,
        DriverObject: PDRIVER_OBJECT,
        RegistryPath: PCUNICODE_STRING,
        DriverAttributes: PWDF_OBJECT_ATTRIBUTES,
        DriverConfig: PWDF_DRIVER_CONFIG,
        Driver: *mut WDFDRIVER,
    ) -> NTSTATUS,
>;

pub type PFN_WDFVERIFIERDBGBREAKPOINT = ::core::option::Option<
    unsafe extern "C" fn(DriverGlobals: PWDF_DRIVER_GLOBALS),
>;
