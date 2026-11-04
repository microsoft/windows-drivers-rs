// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Tests that the `wfp` API subset of `wdk-sys` generates the bindings that a
//! kernel-mode WFP callout driver needs to register a callout with the filter
//! engine.
//!
//! The filter engine routines cannot be called outside of kernel-mode, so these
//! tests instead exercise the *contract* the generated bindings must uphold.
//! Struct layouts are already validated against the WDK headers by the layout
//! assertions bindgen emits into `types.rs`, so they are deliberately not
//! re-asserted with hardcoded offsets here. What these tests pin down is the set
//! of properties bindgen configuration can silently break: that the WFP items are
//! generated at all, that they are reachable at the paths a consumer imports them
//! from, and that the types line up such that a callout registration can be built
//! without any casts.
//!
//! The WFP `GUID` constants are deliberately not referenced here. `fwpmk.h` only
//! declares them, so `wdk-sys` supplies the definitions from a C shim compiled
//! with `INITGUID`; naming one from this test would add a link dependency that a
//! user-mode test executable cannot satisfy. Their linkage is covered by the
//! sample callout driver instead, which is a kernel binary.

#[cfg(test)]
mod tests {
    use wdk_sys::{
        FWP_ACTION_BLOCK,
        FWP_ACTION_CALLOUT_INSPECTION,
        FWP_ACTION_CALLOUT_TERMINATING,
        FWP_ACTION_CONTINUE,
        FWP_ACTION_PERMIT,
        FWP_ACTION_TYPE,
        FWPS_CALLOUT3,
        FWPS_CALLOUT_CLASSIFY_FN3,
        FWPS_CALLOUT_FLOW_DELETE_NOTIFY_FN0,
        FWPS_CALLOUT_NOTIFY_FN3,
        FWPS_CLASSIFY_OUT0,
        FWPS_CLASSIFY_OUT_FLAG_ABSORB,
        FWPS_FILTER3,
        FWPS_INCOMING_METADATA_VALUES0,
        FWPS_INCOMING_VALUES0,
        FWPS_RIGHT_ACTION_WRITE,
        FWPM_CALLOUT0,
        FWPM_DISPLAY_DATA0,
        FWPM_FILTER0,
        GUID,
        NTSTATUS,
        STATUS_SUCCESS,
        UINT32,
        UINT64,
        UINT16,
    };

    /// `FWP_ACTION_TYPE` is a `UINT32` typedef rather than an enum, so the
    /// `FWP_ACTION_*` codes must be generated with a type that can initialize
    /// `FWPS_CLASSIFY_OUT0::actionType` without a cast. A callout that cannot
    /// write an action into `classifyOut` cannot affect traffic at all.
    #[test]
    const fn action_codes_are_typed_for_the_classify_out_field() {
        let block: FWP_ACTION_TYPE = FWP_ACTION_BLOCK;
        let permit: FWP_ACTION_TYPE = FWP_ACTION_PERMIT;
        let continue_action: FWP_ACTION_TYPE = FWP_ACTION_CONTINUE;

        assert!(block != permit);
        assert!(block != continue_action);
        assert!(permit != continue_action);
    }

    /// A callout registers itself against a layer as either terminating or
    /// inspection-only, and the filter engine distinguishes the two by these
    /// codes. They must be distinct, and must carry the `FWP_ACTION_FLAG_*` bits
    /// the WDK encodes into them rather than being renumbered.
    #[test]
    const fn terminating_and_inspection_callout_actions_are_distinct() {
        let terminating: FWP_ACTION_TYPE = FWP_ACTION_CALLOUT_TERMINATING;
        let inspection: FWP_ACTION_TYPE = FWP_ACTION_CALLOUT_INSPECTION;

        assert!(terminating != inspection);
    }

    /// A callout may only write an action into `classifyOut` when the filter
    /// engine has granted it the right to do so, which it signals through
    /// `FWPS_CLASSIFY_OUT0::rights`. Both the right and the flags a callout sets
    /// must be generated as the same integer type as the fields they feed.
    #[test]
    const fn classify_out_rights_and_flags_are_typed_for_their_fields() {
        let mut classify_out = FWPS_CLASSIFY_OUT0 {
            actionType: FWP_ACTION_CONTINUE,
            outContext: 0,
            filterId: 0,
            rights: FWPS_RIGHT_ACTION_WRITE,
            flags: 0,
            reserved: 0,
        };

        // This is the exact sequence a terminating callout performs: check the
        // right, write the action, then clear the right so that no callout further
        // down the chain overrides it.
        assert!(classify_out.rights & FWPS_RIGHT_ACTION_WRITE != 0);
        classify_out.actionType = FWP_ACTION_BLOCK;
        classify_out.rights &= !FWPS_RIGHT_ACTION_WRITE;
        classify_out.flags |= FWPS_CLASSIFY_OUT_FLAG_ABSORB;

        assert!(classify_out.rights & FWPS_RIGHT_ACTION_WRITE == 0);
        assert!(classify_out.flags & FWPS_CLASSIFY_OUT_FLAG_ABSORB != 0);
    }

    /// A callout's entire interaction with the filter engine goes through an
    /// `FWPS_CALLOUT3` built at registration time, so it must be constructible
    /// from the generated items alone, with no casts. This test fails to compile
    /// if bindgen drops a field or changes a callback typedef's signature.
    #[test]
    fn a_callout_registration_is_constructible_from_generated_items_alone() {
        unsafe extern "C" fn classify(
            _in_fixed_values: *const FWPS_INCOMING_VALUES0,
            _in_meta_values: *const FWPS_INCOMING_METADATA_VALUES0,
            _layer_data: *mut core::ffi::c_void,
            _classify_context: *const core::ffi::c_void,
            _filter: *const FWPS_FILTER3,
            _flow_context: UINT64,
            _classify_out: *mut FWPS_CLASSIFY_OUT0,
        ) {
        }

        unsafe extern "C" fn notify(
            _notify_type: wdk_sys::FWPS_CALLOUT_NOTIFY_TYPE,
            _filter_key: *const GUID,
            _filter: *mut FWPS_FILTER3,
        ) -> NTSTATUS {
            STATUS_SUCCESS
        }

        unsafe extern "C" fn flow_delete(
            _layer_id: UINT16,
            _callout_id: UINT32,
            _flow_context: UINT64,
        ) {
        }

        // The callback typedefs must carry their full argument lists, otherwise a
        // callback with the wrong signature would be accepted here and corrupt the
        // stack in kernel-mode.
        let classify_fn: FWPS_CALLOUT_CLASSIFY_FN3 = Some(classify);
        let notify_fn: FWPS_CALLOUT_NOTIFY_FN3 = Some(notify);
        let flow_delete_fn: FWPS_CALLOUT_FLOW_DELETE_NOTIFY_FN0 = Some(flow_delete);

        let callout = FWPS_CALLOUT3 {
            // A real driver uses one of the `GUID` constants or its own callout key
            // here; the value is irrelevant to the layout being checked.
            calloutKey: GUID::default(),
            flags: 0,
            classifyFn: classify_fn,
            notifyFn: notify_fn,
            flowDeleteFn: flow_delete_fn,
        };

        assert!(callout.classifyFn.is_some());
        assert!(callout.notifyFn.is_some());
        assert!(callout.flowDeleteFn.is_some());
    }

    /// Adding a callout and a filter to the engine goes through the `Fwpm*`
    /// management structures, which must be defaultable so that a driver only has
    /// to populate the fields it cares about, and must expose the fields it does.
    #[test]
    fn management_structs_are_defaultable_and_expose_their_fields() {
        let callout = FWPM_CALLOUT0::default();
        assert!(callout.providerKey.is_null());
        assert_eq!(callout.calloutId, 0);

        let filter = FWPM_FILTER0::default();
        assert_eq!(filter.filterId, 0);
        assert_eq!(filter.numFilterConditions, 0);
        assert!(filter.filterCondition.is_null());

        // `displayData` is a required field on both, so its own fields have to be
        // reachable for a registration to be describable at all.
        let display_data = FWPM_DISPLAY_DATA0::default();
        assert!(display_data.name.is_null());
        assert!(display_data.description.is_null());
    }

    /// The structures the filter engine passes into `classifyFn` are only useful
    /// if their fields were generated too, since a callout reads the layer's
    /// values through `FWPS_INCOMING_VALUES0::incomingValue` and the filter's
    /// context through `FWPS_FILTER3`.
    #[test]
    fn classify_parameter_structs_expose_the_fields_a_callout_reads() {
        let incoming_values = FWPS_INCOMING_VALUES0::default();
        assert_eq!(incoming_values.layerId, 0);
        assert_eq!(incoming_values.valueCount, 0);
        assert!(incoming_values.incomingValue.is_null());

        let filter = FWPS_FILTER3::default();
        assert_eq!(filter.filterId, 0);
        assert_eq!(filter.context, 0);
        assert!(filter.providerContext.is_null());
    }
}
