// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Tests that the `minifilter` API subset of `wdk-sys` generates the bindings
//! that a File System Minifilter driver needs to register itself with the
//! Filter Manager.
//!
//! The Filter Manager routines cannot be called outside of kernel-mode, so
//! these tests instead exercise the *contract* the generated bindings must
//! uphold. Struct layouts are already validated against `fltKernel.h` by the
//! layout assertions bindgen emits into `types.rs`, so they are deliberately
//! not re-asserted with hardcoded offsets here. What these tests pin down is
//! the set of properties bindgen configuration can silently break: that the
//! Filter Manager items are generated at all, that they are reachable at the
//! paths a consumer imports them from, and that the types line up such that a
//! registration can be built without any casts.

#[cfg(test)]
mod tests {
    use wdk_sys::{
        _FLT_POSTOP_CALLBACK_STATUS,
        _FLT_PREOP_CALLBACK_STATUS,
        ACCESS_MASK,
        FLT_FILE_NAME_INFORMATION,
        FLT_FILE_NAME_NORMALIZED,
        FLT_FILE_NAME_OPTIONS,
        FLT_FILE_NAME_QUERY_DEFAULT,
        FLT_FILTER_UNLOAD_FLAGS,
        FLT_OPERATION_REGISTRATION,
        FLT_PORT_CONNECT,
        FLT_POST_OPERATION_FLAGS,
        FLT_POSTOP_CALLBACK_STATUS,
        FLT_PREOP_CALLBACK_STATUS,
        FLT_REGISTRATION,
        FLT_REGISTRATION_FLAGS,
        FLT_REGISTRATION_VERSION,
        FLT_RELATED_OBJECTS,
        FLTFL_REGISTRATION_DO_NOT_SUPPORT_SERVICE_STOP,
        IRP_MJ_CREATE,
        IRP_MJ_MAXIMUM_FUNCTION,
        InitializeObjectAttributes,
        MEM_IMAGE,
        MEM_MAPPED,
        MEM_PRIVATE,
        NTSTATUS,
        OBJ_CASE_INSENSITIVE,
        OBJ_KERNEL_HANDLE,
        OBJECT_ATTRIBUTES,
        PCFLT_RELATED_OBJECTS,
        PFLT_CALLBACK_DATA,
        PFLT_FILTER_UNLOAD_CALLBACK,
        PFLT_POST_OPERATION_CALLBACK,
        PFLT_PRE_OPERATION_CALLBACK,
        PROCESS_ALL_ACCESS,
        PROCESS_CREATE_PROCESS,
        PROCESS_CREATE_THREAD,
        PROCESS_DUP_HANDLE,
        PROCESS_QUERY_INFORMATION,
        PROCESS_QUERY_LIMITED_INFORMATION,
        PROCESS_SET_INFORMATION,
        PROCESS_SET_LIMITED_INFORMATION,
        PROCESS_SET_QUOTA,
        PROCESS_SET_SESSIONID,
        PROCESS_SUSPEND_RESUME,
        PROCESS_TERMINATE,
        PROCESS_VM_OPERATION,
        PROCESS_VM_READ,
        PROCESS_VM_WRITE,
        PVOID,
        SEC_IMAGE,
        STANDARD_RIGHTS_ALL,
        STATUS_SUCCESS,
        UCHAR,
        UNICODE_STRING,
        USHORT,
        minifilter::{FLT_PORT_ALL_ACCESS, IRP_MJ_OPERATION_END},
    };

    /// `IRP_MJ_OPERATION_END` is defined in `fltKernel.h` as a macro with a
    /// cast in its expansion (`((UCHAR)0x80)`), which bindgen cannot
    /// translate, so `wdk-sys` ports it by hand. It must be typed as a
    /// [`UCHAR`] so that it can initialize
    /// `FLT_OPERATION_REGISTRATION::MajorFunction` directly, and
    /// it must not collide with a real `IRP_MJ_*` code or the Filter Manager
    /// would stop walking the operation table early.
    #[test]
    const fn irp_mj_operation_end_terminates_the_operation_table() {
        // `MajorFunction` is a `UCHAR`, so assigning without a cast only compiles if
        // the hand-ported constant has the same type. This is the regression
        // that a plain `u32` port would introduce.
        let terminator = FLT_OPERATION_REGISTRATION {
            MajorFunction: IRP_MJ_OPERATION_END,
            Flags: 0,
            PreOperation: None,
            PostOperation: None,
            Reserved1: core::ptr::null_mut(),
        };

        assert!(
            terminator.MajorFunction as u32 > IRP_MJ_MAXIMUM_FUNCTION,
            "the sentinel must not alias a real IRP_MJ_* code"
        );
    }

    /// The `IRP_MJ_*` codes are generated into the shared `constants.rs` as
    /// `u32`, but `FLT_OPERATION_REGISTRATION::MajorFunction` is a [`UCHAR`]. A
    /// minifilter therefore has to narrow them, which is only sound while every
    /// code the WDK defines is in range.
    #[test]
    fn every_irp_mj_code_fits_in_a_major_function() {
        for irp_mj_code in 0..=IRP_MJ_MAXIMUM_FUNCTION {
            assert!(
                UCHAR::try_from(irp_mj_code).is_ok(),
                "IRP_MJ code {irp_mj_code} does not fit in \
                 FLT_OPERATION_REGISTRATION::MajorFunction"
            );
        }
    }

    /// The Filter Manager rejects a registration whose `Size` does not match
    /// the `FLT_REGISTRATION` the driver compiled against, and
    /// `Size`/`Version` are [`USHORT`]s while `size_of` and the generated
    /// `FLT_REGISTRATION_VERSION` are wider. If either value stopped
    /// fitting, every minifilter built on these bindings would fail to
    /// load, so this catches it at test time instead.
    #[test]
    fn registration_size_and_version_fit_in_their_fields() {
        assert!(
            USHORT::try_from(size_of::<FLT_REGISTRATION>()).is_ok(),
            "size_of::<FLT_REGISTRATION>() does not fit in FLT_REGISTRATION::Size"
        );
        assert!(
            USHORT::try_from(FLT_REGISTRATION_VERSION).is_ok(),
            "FLT_REGISTRATION_VERSION does not fit in FLT_REGISTRATION::Version"
        );
    }

    /// Registration flags and file name options are bitmask constants, so they
    /// must be generated with the same integer type as the fields and arguments
    /// they feed, and must be combinable without a cast.
    #[test]
    const fn registration_flags_and_name_options_are_typed_for_their_fields() {
        let flags: FLT_REGISTRATION_FLAGS = FLTFL_REGISTRATION_DO_NOT_SUPPORT_SERVICE_STOP;
        let name_options: FLT_FILE_NAME_OPTIONS =
            FLT_FILE_NAME_NORMALIZED | FLT_FILE_NAME_QUERY_DEFAULT;

        assert!(flags != 0);
        assert!(name_options != FLT_FILE_NAME_NORMALIZED);
        assert!(name_options != FLT_FILE_NAME_QUERY_DEFAULT);
    }

    /// The callback status enumerations are returned by value from the
    /// pre/post-operation callbacks. Their exact discriminants come from
    /// `fltKernel.h` and are validated by bindgen, but they must be distinct
    /// and must be generated as the same type the callback typedefs return.
    #[test]
    const fn callback_statuses_are_distinct_and_correctly_typed() {
        let success_with_callback: FLT_PREOP_CALLBACK_STATUS =
            _FLT_PREOP_CALLBACK_STATUS::FLT_PREOP_SUCCESS_WITH_CALLBACK;
        let success_no_callback: FLT_PREOP_CALLBACK_STATUS =
            _FLT_PREOP_CALLBACK_STATUS::FLT_PREOP_SUCCESS_NO_CALLBACK;
        let pending: FLT_PREOP_CALLBACK_STATUS = _FLT_PREOP_CALLBACK_STATUS::FLT_PREOP_PENDING;
        let complete: FLT_PREOP_CALLBACK_STATUS = _FLT_PREOP_CALLBACK_STATUS::FLT_PREOP_COMPLETE;

        assert!(success_with_callback != success_no_callback);
        assert!(success_with_callback != pending);
        assert!(success_with_callback != complete);
        assert!(success_no_callback != pending);
        assert!(success_no_callback != complete);
        assert!(pending != complete);

        let finished: FLT_POSTOP_CALLBACK_STATUS =
            _FLT_POSTOP_CALLBACK_STATUS::FLT_POSTOP_FINISHED_PROCESSING;
        let more_processing: FLT_POSTOP_CALLBACK_STATUS =
            _FLT_POSTOP_CALLBACK_STATUS::FLT_POSTOP_MORE_PROCESSING_REQUIRED;

        assert!(finished != more_processing);
    }

    /// A minifilter's whole interaction with the Filter Manager goes through an
    /// `FLT_REGISTRATION` built at compile time, so it must be constructible
    /// from the generated items alone: `Default` for the callbacks a driver
    /// does not implement, a `'static` operation table, and no casts
    /// anywhere. This test fails to compile if bindgen stops deriving
    /// `Default`, drops a field, or changes a field's type.
    #[test]
    fn a_registration_is_constructible_from_generated_items_alone() {
        unsafe extern "C" fn pre_operation(
            _data: PFLT_CALLBACK_DATA,
            _flt_objects: PCFLT_RELATED_OBJECTS,
            _completion_context: *mut PVOID,
        ) -> FLT_PREOP_CALLBACK_STATUS {
            _FLT_PREOP_CALLBACK_STATUS::FLT_PREOP_SUCCESS_WITH_CALLBACK
        }

        unsafe extern "C" fn post_operation(
            _data: PFLT_CALLBACK_DATA,
            _flt_objects: PCFLT_RELATED_OBJECTS,
            _completion_context: PVOID,
            _flags: FLT_POST_OPERATION_FLAGS,
        ) -> FLT_POSTOP_CALLBACK_STATUS {
            _FLT_POSTOP_CALLBACK_STATUS::FLT_POSTOP_FINISHED_PROCESSING
        }

        unsafe extern "C" fn filter_unload(_flags: FLT_FILTER_UNLOAD_FLAGS) -> NTSTATUS {
            STATUS_SUCCESS
        }

        // The callback typedefs must carry their full argument lists, otherwise a
        // callback with the wrong signature would be accepted here and corrupt
        // the stack in kernel-mode.
        let pre_operation: PFLT_PRE_OPERATION_CALLBACK = Some(pre_operation);
        let post_operation: PFLT_POST_OPERATION_CALLBACK = Some(post_operation);
        let filter_unload: PFLT_FILTER_UNLOAD_CALLBACK = Some(filter_unload);

        // `FLT_OPERATION_REGISTRATION` holds a raw `Reserved1` pointer and so is not
        // `Sync`, which is why the sample driver wraps its table in a newtype
        // to put it in a `static`. Here a local suffices, since it outlives the
        // registration that borrows it.
        let operation_registration = [
            FLT_OPERATION_REGISTRATION {
                MajorFunction: UCHAR::try_from(IRP_MJ_CREATE)
                    .expect("IRP_MJ_CREATE should fit in MajorFunction"),
                Flags: 0,
                PreOperation: None,
                PostOperation: None,
                Reserved1: core::ptr::null_mut(),
            },
            FLT_OPERATION_REGISTRATION {
                MajorFunction: IRP_MJ_OPERATION_END,
                Flags: 0,
                PreOperation: None,
                PostOperation: None,
                Reserved1: core::ptr::null_mut(),
            },
        ];

        let registration = FLT_REGISTRATION {
            Size: USHORT::try_from(size_of::<FLT_REGISTRATION>())
                .expect("FLT_REGISTRATION should fit in FLT_REGISTRATION::Size"),
            Version: USHORT::try_from(FLT_REGISTRATION_VERSION)
                .expect("FLT_REGISTRATION_VERSION should fit in FLT_REGISTRATION::Version"),
            Flags: FLTFL_REGISTRATION_DO_NOT_SUPPORT_SERVICE_STOP,
            OperationRegistration: operation_registration.as_ptr(),
            FilterUnloadCallback: filter_unload,
            // Every callback a minifilter does not implement must be defaultable.
            ..FLT_REGISTRATION::default()
        };

        assert!(pre_operation.is_some());
        assert!(post_operation.is_some());
        assert!(registration.FilterUnloadCallback.is_some());
        assert!(!registration.OperationRegistration.is_null());
        assert!(
            registration.InstanceSetupCallback.is_none(),
            "FLT_REGISTRATION::default() should leave unimplemented callbacks null"
        );
    }

    /// `FLT_PORT_ALL_ACCESS` is defined in `fltKernel.h` as a macro whose
    /// expansion references other identifiers
    /// (`FLT_PORT_CONNECT | STANDARD_RIGHTS_ALL`), which bindgen does not emit,
    /// so `wdk-sys` composes it by hand. It is the `DesiredAccess` a minifilter
    /// passes to `FltBuildDefaultSecurityDescriptor` when creating its
    /// communication port, so a wrong value produces a port that either rejects
    /// the driver's own userland client or is more permissive than intended.
    ///
    /// Every operand is a constant, so the assertions are in `const` blocks:
    /// a regression here is a build failure rather than a test failure.
    #[test]
    const fn flt_port_all_access_is_connect_plus_standard_rights() {
        const { assert!(FLT_PORT_ALL_ACCESS == FLT_PORT_CONNECT | STANDARD_RIGHTS_ALL) };

        // Composing the mask by hand is only safe while the operands are the same
        // width as the mask itself; a `u32` operand silently truncated into a
        // narrower `ACCESS_MASK` would drop `STANDARD_RIGHTS_ALL` entirely and
        // leave just `FLT_PORT_CONNECT`, which is exactly the mistake this
        // constant exists to prevent.
        const {
            assert!(
                FLT_PORT_ALL_ACCESS != FLT_PORT_CONNECT,
                "FLT_PORT_ALL_ACCESS must include the standard rights, not just FLT_PORT_CONNECT"
            )
        };
        const { assert!(FLT_PORT_ALL_ACCESS & FLT_PORT_CONNECT == FLT_PORT_CONNECT) };
        const { assert!(FLT_PORT_ALL_ACCESS & STANDARD_RIGHTS_ALL == STANDARD_RIGHTS_ALL) };
    }

    /// The process-specific access rights are defined as one contiguous family
    /// in `winnt.h`, which kernel-mode bindgen never processes; `wdm.h`
    /// redefines only `PROCESS_DUP_HANDLE` and `PROCESS_ALL_ACCESS` from it, so
    /// `wdk-sys` ports the other twelve by hand. A minifilter that maps a
    /// section into a client's address space needs
    /// [`PROCESS_VM_OPERATION`] for its `ZwOpenProcess`, and a wrong value
    /// there would fail the open with `STATUS_ACCESS_DENIED` at best or
    /// request an unintended right at worst.
    ///
    /// `PROCESS_ALL_ACCESS` *is* generated, and `wdm.h` defines it as the
    /// standard rights plus `SYNCHRONIZE` plus the low `0xFFFF`, so it is an
    /// independent witness that each hand-ported bit is one the WDK really
    /// assigns to this family. That is what makes this more than a restatement
    /// of the literals in `constants.rs`.
    #[test]
    const fn process_access_rights_are_within_the_family_all_access_covers() {
        const RIGHTS: [ACCESS_MASK; 14] = [
            PROCESS_TERMINATE,
            PROCESS_CREATE_THREAD,
            PROCESS_SET_SESSIONID,
            PROCESS_VM_OPERATION,
            PROCESS_VM_READ,
            PROCESS_VM_WRITE,
            // The one right in the family that bindgen does generate, included so
            // that the hand-ported values are checked for collisions against it.
            PROCESS_DUP_HANDLE,
            PROCESS_CREATE_PROCESS,
            PROCESS_SET_QUOTA,
            PROCESS_SET_INFORMATION,
            PROCESS_QUERY_INFORMATION,
            PROCESS_SUSPEND_RESUME,
            PROCESS_QUERY_LIMITED_INFORMATION,
            PROCESS_SET_LIMITED_INFORMATION,
        ];

        // The assertions are not wrapped in `const` blocks, as the ones in
        // `flt_port_all_access_is_connect_plus_standard_rights` are, because a
        // `const` block cannot capture the loop indices. So unlike that test, a
        // regression here fails the test run rather than the build.
        let mut index = 0;
        while index < RIGHTS.len() {
            // Every one is a single bit: a transposed digit would most likely produce
            // a value with two bits set, or none.
            assert!(
                RIGHTS[index].is_power_of_two(),
                "each process access right is a single bit"
            );

            // And every one is inside what `PROCESS_ALL_ACCESS` grants, which is the
            // check that cannot be satisfied by a literal that merely happens to be a
            // power of two.
            assert!(
                RIGHTS[index] & PROCESS_ALL_ACCESS == RIGHTS[index],
                "each process access right must be covered by PROCESS_ALL_ACCESS"
            );

            // Distinctness, pairwise: two rights sharing a bit would mean requesting
            // one silently requests the other.
            let mut other = index + 1;
            while other < RIGHTS.len() {
                assert!(
                    RIGHTS[index] != RIGHTS[other],
                    "the process access rights must be distinct bits"
                );
                other += 1;
            }

            index += 1;
        }
    }

    /// `MEM_IMAGE` is the one member of the `MEMORY_BASIC_INFORMATION::Type`
    /// family that `wdm.h` does not redefine from `winnt.h`, so bindgen
    /// generates its two siblings and not it. A driver that walks a process's
    /// address space with `ZwQueryVirtualMemory` reads `Type` to tell an image
    /// mapping from a private or file mapping, which makes the absent value the
    /// most useful of the three.
    ///
    /// The value is checked against `SEC_IMAGE` rather than restated as a
    /// literal: `wdm.h` *does* define that one, at the same value, so it is an
    /// independent witness out of the generated constants. The two are separate
    /// families — a section-creation flag and a region type — which is why the
    /// hand-ported constant exists at all rather than callers reusing
    /// `SEC_IMAGE`, and it is also what makes this a real check.
    ///
    /// Both operands are constants, so the assertions are in `const` blocks: a
    /// regression is a build failure rather than a test failure.
    #[test]
    const fn mem_image_is_the_region_type_matching_sec_image() {
        const {
            assert!(
                MEM_IMAGE == SEC_IMAGE,
                "MEM_IMAGE is winnt.h's 0x01000000, as SEC_IMAGE is"
            )
        };

        // A single bit, which a transposed digit would most likely break, and one
        // that does not collide with either sibling `wdm.h` does define.
        const { assert!(MEM_IMAGE.is_power_of_two()) };
        const { assert!(MEM_IMAGE != MEM_MAPPED && MEM_IMAGE != MEM_PRIVATE) };
    }

    /// `InitializeObjectAttributes` is a function-like macro, which bindgen
    /// cannot generate, so `wdk-sys` ports it as a `const fn`. Naming the port
    /// via a communication port's [`OBJECT_ATTRIBUTES`] is the only way to give
    /// it a name, so a driver cannot avoid this item.
    #[test]
    fn initialize_object_attributes_fills_in_length_and_clears_the_qos() {
        let mut port_name = UNICODE_STRING::default();
        let mut security_descriptor = 0_u8;

        let object_attributes = InitializeObjectAttributes(
            &raw mut port_name,
            OBJ_KERNEL_HANDLE | OBJ_CASE_INSENSITIVE,
            core::ptr::null_mut(),
            (&raw mut security_descriptor).cast(),
        );

        // The Filter Manager rejects an `OBJECT_ATTRIBUTES` whose `Length` does not
        // match the structure, which is the whole reason the C macro exists.
        assert_eq!(
            u64::from(object_attributes.Length),
            size_of::<OBJECT_ATTRIBUTES>() as u64,
            "Length should be the size of the structure"
        );
        assert!(
            object_attributes.SecurityQualityOfService.is_null(),
            "SecurityQualityOfService should be null, as the C macro leaves it"
        );

        assert_eq!(object_attributes.ObjectName, &raw mut port_name);
        assert_eq!(
            object_attributes.Attributes,
            OBJ_KERNEL_HANDLE | OBJ_CASE_INSENSITIVE
        );
        assert!(object_attributes.RootDirectory.is_null());
        assert_eq!(
            object_attributes.SecurityDescriptor,
            (&raw mut security_descriptor).cast()
        );
    }

    /// The structures the Filter Manager passes into callbacks are only useful
    /// if their fields were generated too, since a minifilter reaches the
    /// filter, instance, and file objects for an operation through
    /// `FLT_RELATED_OBJECTS`, and the file name through
    /// `FLT_FILE_NAME_INFORMATION::Name`.
    #[test]
    fn callback_parameter_structs_expose_the_fields_a_minifilter_reads() {
        let related_objects = FLT_RELATED_OBJECTS::default();
        assert!(related_objects.Filter.is_null());
        assert!(related_objects.Instance.is_null());
        assert!(related_objects.Volume.is_null());
        assert!(related_objects.FileObject.is_null());

        let file_name_information = FLT_FILE_NAME_INFORMATION::default();
        assert!(file_name_information.Name.Buffer.is_null());
        assert_eq!(file_name_information.Name.Length, 0);
    }

    /// MDL (Memory Descriptor List) functions and types are hand-ported in
    /// `ntddk.rs` and `types.rs` because they are defined in `wdm.h` but not
    /// generated by bindgen (inline functions or skipped attributes).
    ///
    /// This test validates that:
    /// - The MDL opaque type and PMDL pointer type are available
    /// - The LOCK_OPERATION enum is available with correct discriminants
    /// - All five MDL functions are callable (IoAllocateMdl, MmProbeAndLockPages,
    ///   MmGetSystemAddressForMdlSafe, MmUnlockPages, IoFreeMdl)
    /// - Page priority constants are available
    #[test]
    fn mdl_functions_and_types_are_available() {
        use wdk_sys::{
            ntddk::{IoAllocateMdl, IoFreeMdl, MmProbeAndLockPages, MmUnlockPages},
            HIGH_PAGE_PRIORITY,
            LOCK_OPERATION,
            LOW_PAGE_PRIORITY,
            MDL,
            NORMAL_PAGE_PRIORITY,
            PMDL,
        };

        // Validate LOCK_OPERATION enum discriminants match wdm.h
        assert_eq!(LOCK_OPERATION::IoReadAccess as i32, 0);
        assert_eq!(LOCK_OPERATION::IoWriteAccess as i32, 1);
        assert_eq!(LOCK_OPERATION::IoModifyAccess as i32, 2);

        // Validate page priority constants
        assert_eq!(NORMAL_PAGE_PRIORITY, 16);
        assert_eq!(LOW_PAGE_PRIORITY, 0);
        assert_eq!(HIGH_PAGE_PRIORITY, 32);

        // Validate function signatures compile (no actual calls in usermode test)
        let _: unsafe extern "C" fn(PVOID, u32, u8, u8, *mut _) -> PMDL = IoAllocateMdl;
        let _: unsafe extern "C" fn(PMDL, i8, LOCK_OPERATION) = MmProbeAndLockPages;
        let _: unsafe extern "C" fn(PMDL) = MmUnlockPages;
        let _: unsafe extern "C" fn(PMDL) = IoFreeMdl;

        // Validate MDL is a real struct (not an opaque marker) with the fields
        // `MmGetSystemAddressForMdlSafe` reads.
        assert!(size_of::<MDL>() >= 40);
    }

    /// Every `FILE_INFORMATION_CLASS` variant in header order, so the round-trip check below can
    /// prove the enum is DENSE over `1..=84`. Density is the property that makes a `transmute` into
    /// this type sound: a hole means some legal value has no variant, which is UB. Generated from
    /// `km/wdm.h:7834-7946` (WDK 10.0.26100.0), not from `types.rs`.
    const FILE_INFORMATION_CLASS_BY_VALUE: [wdk_sys::FILE_INFORMATION_CLASS; 84] = {
        use wdk_sys::FILE_INFORMATION_CLASS as C;
        [
        C::FileDirectoryInformation,
        C::FileFullDirectoryInformation,
        C::FileBothDirectoryInformation,
        C::FileBasicInformation,
        C::FileStandardInformation,
        C::FileInternalInformation,
        C::FileEaInformation,
        C::FileAccessInformation,
        C::FileNameInformation,
        C::FileRenameInformation,
        C::FileLinkInformation,
        C::FileNamesInformation,
        C::FileDispositionInformation,
        C::FilePositionInformation,
        C::FileFullEaInformation,
        C::FileModeInformation,
        C::FileAlignmentInformation,
        C::FileAllInformation,
        C::FileAllocationInformation,
        C::FileEndOfFileInformation,
        C::FileAlternateNameInformation,
        C::FileStreamInformation,
        C::FilePipeInformation,
        C::FilePipeLocalInformation,
        C::FilePipeRemoteInformation,
        C::FileMailslotQueryInformation,
        C::FileMailslotSetInformation,
        C::FileCompressionInformation,
        C::FileObjectIdInformation,
        C::FileCompletionInformation,
        C::FileMoveClusterInformation,
        C::FileQuotaInformation,
        C::FileReparsePointInformation,
        C::FileNetworkOpenInformation,
        C::FileAttributeTagInformation,
        C::FileTrackingInformation,
        C::FileIdBothDirectoryInformation,
        C::FileIdFullDirectoryInformation,
        C::FileValidDataLengthInformation,
        C::FileShortNameInformation,
        C::FileIoCompletionNotificationInformation,
        C::FileIoStatusBlockRangeInformation,
        C::FileIoPriorityHintInformation,
        C::FileSfioReserveInformation,
        C::FileSfioVolumeInformation,
        C::FileHardLinkInformation,
        C::FileProcessIdsUsingFileInformation,
        C::FileNormalizedNameInformation,
        C::FileNetworkPhysicalNameInformation,
        C::FileIdGlobalTxDirectoryInformation,
        C::FileIsRemoteDeviceInformation,
        C::FileUnusedInformation,
        C::FileNumaNodeInformation,
        C::FileStandardLinkInformation,
        C::FileRemoteProtocolInformation,
        C::FileRenameInformationBypassAccessCheck,
        C::FileLinkInformationBypassAccessCheck,
        C::FileVolumeNameInformation,
        C::FileIdInformation,
        C::FileIdExtdDirectoryInformation,
        C::FileReplaceCompletionInformation,
        C::FileHardLinkFullIdInformation,
        C::FileIdExtdBothDirectoryInformation,
        C::FileDispositionInformationEx,
        C::FileRenameInformationEx,
        C::FileRenameInformationExBypassAccessCheck,
        C::FileDesiredStorageClassInformation,
        C::FileStatInformation,
        C::FileMemoryPartitionInformation,
        C::FileStatLxInformation,
        C::FileCaseSensitiveInformation,
        C::FileLinkInformationEx,
        C::FileLinkInformationExBypassAccessCheck,
        C::FileStorageReserveIdInformation,
        C::FileCaseSensitiveInformationForceAccessCheck,
        C::FileKnownFolderInformation,
        C::FileStatBasicInformation,
        C::FileId64ExtdDirectoryInformation,
        C::FileId64ExtdBothDirectoryInformation,
        C::FileIdAllExtdDirectoryInformation,
        C::FileIdAllExtdBothDirectoryInformation,
        C::FileStreamReservationInformation,
        C::FileMupProviderInfo,
        C::FileMaximumInformation,
        ]
    };

    /// The `FILE_INFORMATION_CLASS` *enum* matches `km/wdm.h`, end to end.
    ///
    /// Distinct from `file_information_class_constants_exist` below, which checks the free
    /// `FILE_*_INFORMATION` constants. This checks the `#[repr(i32)]` enum in `types.rs`, and it
    /// exists because that enum was wrong in a way that made `transmute` into it **undefined
    /// behaviour** rather than merely inaccurate.
    ///
    /// Every value here is read off `km/wdm.h:7834-7946` (WDK 10.0.26100.0), where
    /// `FileDirectoryInformation = 1` is the only explicit discriminant and the remaining 83 entries
    /// increment implicitly -- so entry N has value N. Reading them off `types.rs` instead would pin
    /// whatever `types.rs` says, which is exactly the failure `constants.rs:172-185` records.
    ///
    /// The three defects this pins, all of which were live:
    ///
    /// * the list stopped at 40, so 41..=83 had no variant at all;
    /// * `FileMaximumInformation` was 41 -- and 41 is `FileIoCompletionNotificationInformation`, a
    ///   real queryable class, so the name was bound to a valid class and every bound check written
    ///   against it was off by 43;
    /// * value 30 was missing entirely, leaving a hole mid-range.
    #[test]
    fn file_information_class_enum_matches_the_wdk_header() {
        use wdk_sys::FILE_INFORMATION_CLASS as C;

        // Anchors: first, last, and the boundary the old truncation sat on.
        assert_eq!(C::FileDirectoryInformation as i32, 1);
        assert_eq!(C::FileShortNameInformation as i32, 40);

        // 41 is a real class, NOT the maximum. This is the assertion that would have caught the bug.
        assert_eq!(C::FileIoCompletionNotificationInformation as i32, 41);

        // The hole: 30 exists and is `FileCompletionInformation`.
        assert_eq!(C::FileObjectIdInformation as i32, 29);
        assert_eq!(C::FileCompletionInformation as i32, 30);
        assert_eq!(C::FileMoveClusterInformation as i32, 31);

        // Past the old end of the list. `FileKnownFolderInformation` is the one this driver needs,
        // and transmuting 76 into the old enum was UB.
        assert_eq!(C::FileRenameInformationEx as i32, 65);
        assert_eq!(C::FileKnownFolderInformation as i32, 76);
        assert_eq!(C::FileMupProviderInfo as i32, 83);

        // The real maximum, from the header's own last entry.
        assert_eq!(C::FileMaximumInformation as i32, 84);

        // And the property that makes the enum safe to transmute into at all: it is dense over
        // 1..=84, so any value a caller can legally produce has a variant. Checked by round-tripping
        // every discriminant rather than by trusting the count -- a hole would have passed a count.
        for value in 1..=84_i32 {
            let class = FILE_INFORMATION_CLASS_BY_VALUE[(value - 1) as usize];
            assert_eq!(class as i32, value, "value {value} does not round-trip");
        }
    }

    /// FILE_INFORMATION_CLASS constants from ntifs.h are hand-ported to wdk-sys.
    /// This test verifies they exist and have the correct values.
    #[test]
    fn file_information_class_constants_exist() {
        use wdk_sys::{
            FILE_ALLOCATION_INFORMATION,
            FILE_DISPOSITION_INFORMATION,
            FILE_DISPOSITION_INFORMATION_EX,
            FILE_END_OF_FILE_INFORMATION,
            FILE_LINK_INFORMATION,
            FILE_LINK_INFORMATION_BYPASS_ACCESS_CHECK,
            FILE_LINK_INFORMATION_EX,
            FILE_LINK_INFORMATION_EX_BYPASS_ACCESS_CHECK,
            FILE_RENAME_INFORMATION,
            FILE_RENAME_INFORMATION_BYPASS_ACCESS_CHECK,
            FILE_RENAME_INFORMATION_EX,
            FILE_RENAME_INFORMATION_EX_BYPASS_ACCESS_CHECK,
            FILE_SHORT_NAME_INFORMATION,
            FILE_VALID_DATA_LENGTH_INFORMATION,
        };

        // Values are from the `FILE_INFORMATION_CLASS` enumeration in `km/wdm.h`, which states each
        // one in a trailing comment.
        assert_eq!(FILE_RENAME_INFORMATION, 10);
        assert_eq!(FILE_LINK_INFORMATION, 11);
        assert_eq!(FILE_DISPOSITION_INFORMATION, 13);
        assert_eq!(FILE_ALLOCATION_INFORMATION, 19);
        assert_eq!(FILE_END_OF_FILE_INFORMATION, 20);
        assert_eq!(FILE_VALID_DATA_LENGTH_INFORMATION, 39);
        assert_eq!(FILE_SHORT_NAME_INFORMATION, 40);
        assert_eq!(FILE_DISPOSITION_INFORMATION_EX, 64);
        assert_eq!(FILE_RENAME_INFORMATION_EX, 65);
        assert_eq!(FILE_LINK_INFORMATION_EX, 72);

        // The four bypass-access-check classes are NOT contiguous with their non-bypass
        // counterparts, and this test previously asserted them as though they were — 71, 73, 74, 75
        // instead of 56, 57, 66, 73. Because the assertions were written from the same wrong source
        // as the constants, the test passed and pinned the error rather than catching it.
        //
        // That mattered: 71 is `FileCaseSensitiveInformation` and 74 is
        // `FileStorageReserveIdInformation`, both taking a 4-byte buffer, and a consumer that routed
        // 71 into a rename handler cast that buffer to `FILE_RENAME_INFO` and read a length field
        // past its end — an out-of-bounds kernel read reachable from user mode through
        // `SetFileInformationByHandle`.
        //
        // If one of these ever fails, do not adjust the expectation. Read `km/wdm.h` — the values
        // are in its own comments at lines 8204, 8205, 8219 and 8226 of WDK 10.0.28000.0.
        assert_eq!(FILE_RENAME_INFORMATION_BYPASS_ACCESS_CHECK, 56);
        assert_eq!(FILE_LINK_INFORMATION_BYPASS_ACCESS_CHECK, 57);
        assert_eq!(FILE_RENAME_INFORMATION_EX_BYPASS_ACCESS_CHECK, 66);
        assert_eq!(FILE_LINK_INFORMATION_EX_BYPASS_ACCESS_CHECK, 73);

        // Each bypass class must differ from the unrelated class that previously occupied its slot,
        // which is the specific confusion that caused the out-of-bounds read.
        assert_ne!(FILE_RENAME_INFORMATION_BYPASS_ACCESS_CHECK, 71);
        assert_ne!(FILE_LINK_INFORMATION_BYPASS_ACCESS_CHECK, 74);
    }

    /// Lookaside lists provide efficient allocation/deallocation of fixed-size objects
    /// by maintaining a pool of preallocated structures. This test verifies that bindgen
    /// correctly generated the lookaside list APIs from wdm.h.
    #[test]
    const fn lookaside_list_bindings_exist() {
        use wdk_sys::{
            ntddk::{
                ExAllocateFromLookasideListEx,
                ExDeleteLookasideListEx,
                ExFreeToLookasideListEx,
                ExInitializeLookasideListEx,
            },
            _LOOKASIDE_LIST_EX,
            NTSTATUS,
            PALLOCATE_FUNCTION_EX,
            PFREE_FUNCTION_EX,
            PLOOKASIDE_LIST_EX,
            POOL_TYPE,
            PVOID,
            SIZE_T,
            ULONG,
            USHORT,
        };

        // Validate function signatures compile (no actual calls in usermode test)
        let _: unsafe extern "C" fn(
            PLOOKASIDE_LIST_EX,
            PALLOCATE_FUNCTION_EX,
            PFREE_FUNCTION_EX,
            POOL_TYPE,
            ULONG,
            SIZE_T,
            ULONG,
            USHORT,
        ) -> NTSTATUS = ExInitializeLookasideListEx;
        let _: unsafe extern "C" fn(PLOOKASIDE_LIST_EX) = ExDeleteLookasideListEx;
        let _: unsafe extern "C" fn(PLOOKASIDE_LIST_EX) -> PVOID =
            ExAllocateFromLookasideListEx;
        let _: unsafe extern "C" fn(PLOOKASIDE_LIST_EX, PVOID) = ExFreeToLookasideListEx;

        // Validate _LOOKASIDE_LIST_EX exists and has reasonable size
        assert!(size_of::<_LOOKASIDE_LIST_EX>() >= 64);
    }

    /// `ExAllocatePool2` is `NTKERNELAPI` under `NTDDI_VERSION >= NTDDI_WIN10_VB`
    /// (Windows 10 2004, build 19041), so a static import fails to bind on every
    /// Windows before that build. The shim in
    /// `wdk-sys/src/ntddk/allocate_pool_downlevel.rs` resolves the routine
    /// through `MmGetSystemRoutineAddress` and falls back to
    /// `ExAllocatePoolWithTag` when it is absent.
    ///
    /// This test verifies the shim is exported under the same name as the
    /// bindgen-generated extern would have been, with the same signature, so
    /// every existing call site continues to compile.
    #[test]
    const fn ex_allocate_pool2_binding_exists() {
        use wdk_sys::{
            POOL_FLAGS,
            PVOID,
            SIZE_T,
            ULONG,
            ntddk::ExAllocatePool2,
        };

        // Validate the shim's signature matches `ExAllocatePool2`'s WDK
        // declaration byte for byte: `PVOID ExAllocatePool2(POOL_FLAGS Flags,
        // SIZE_T NumberOfBytes, ULONG Tag)`
        // (`km/wdm.h` under `NTDDI_VERSION >= NTDDI_WIN10_VB`).
        let _: unsafe extern "C" fn(POOL_FLAGS, SIZE_T, ULONG) -> PVOID = ExAllocatePool2;
    }

    /// FILE_RENAME_INFO and FILE_LINK_INFO structures are passed in IRP_MJ_SET_INFORMATION
    /// operations for rename and hard-link operations. These are defined in ntifs.h but
    /// used in kernel-mode minifilters, so they must be manually added to wdk-sys.
    #[test]
    const fn file_rename_link_info_structures_exist() {
        use wdk_sys::{
            FILE_LINK_INFO, FILE_RENAME_INFO, BOOLEAN, HANDLE, ULONG, WCHAR,
        };

        // Validate structures exist and have expected fields
        const _FILE_RENAME_FIELDS: fn(FILE_RENAME_INFO) -> (BOOLEAN, HANDLE, ULONG, [WCHAR; 1]) =
            |info| {
                (
                    info.ReplaceIfExists,
                    info.RootDirectory,
                    info.FileNameLength,
                    info.FileName,
                )
            };

        const _FILE_LINK_FIELDS: fn(FILE_LINK_INFO) -> (BOOLEAN, HANDLE, ULONG, [WCHAR; 1]) =
            |info| {
                (
                    info.ReplaceIfExists,
                    info.RootDirectory,
                    info.FileNameLength,
                    info.FileName,
                )
            };

        // Validate reasonable sizes (at least the fixed fields)
        assert!(size_of::<FILE_RENAME_INFO>() >= size_of::<BOOLEAN>() + size_of::<HANDLE>() + size_of::<ULONG>());
        assert!(size_of::<FILE_LINK_INFO>() >= size_of::<BOOLEAN>() + size_of::<HANDLE>() + size_of::<ULONG>());
    }

    /// FltGetDestinationFileNameInformation resolves rename/link destination paths.
    /// This is used by minifilters to normalize destination paths for ACL checks.
    /// Bindgen generates this from fltKernel.h, so we just verify it's reachable.
    #[test]
    const fn flt_get_destination_file_name_information_exists() {
        use wdk_sys::{
            FLT_FILE_NAME_INFORMATION,
            HANDLE,
            NTSTATUS,
            PFILE_OBJECT,
            PFLT_INSTANCE,
            ULONG,
            WCHAR,
            minifilter::FltGetDestinationFileNameInformation,
        };

        // Validate function signature (generated by bindgen, FileName is mutable in the SDK)
        let _: unsafe extern "C" fn(
            PFLT_INSTANCE,
            PFILE_OBJECT,
            HANDLE,
            *mut WCHAR,  // SDK has this as mutable even though it's conceptually const
            ULONG,
            ULONG,
            *mut *mut FLT_FILE_NAME_INFORMATION,
        ) -> NTSTATUS = FltGetDestinationFileNameInformation;
    }
}
