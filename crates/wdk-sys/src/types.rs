// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

#[allow(missing_docs)]
#[allow(non_upper_case_globals)]
#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[allow(unsafe_op_in_unsafe_fn)]
#[allow(clippy::cast_lossless)]
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cognitive_complexity)]
#[allow(clippy::default_trait_access)]
#[rustversion::attr(
    any(
        all(not(nightly), before(1.74)),
        all(nightly, before(2023-09-13)),
    ),
    allow(clippy::incorrect_clone_impl_on_copy_type)
)]
#[rustversion::attr(
    any(
        all(not(nightly), since(1.74)),
        all(nightly, since(2023-09-13)),
    ),
    allow(clippy::non_canonical_clone_impl)
)]
#[allow(clippy::missing_safety_doc)]
#[allow(clippy::missing_const_for_fn)]
#[allow(clippy::module_name_repetitions)]
#[allow(clippy::multiple_unsafe_ops_per_block)]
#[allow(clippy::must_use_candidate)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[allow(clippy::ptr_as_ptr)]
#[rustversion::attr(
    any(
        all(not(nightly), since(1.77)),
        all(nightly, since(2024-01-11)),
    ),
    allow(clippy::pub_underscore_fields)
)]
#[allow(clippy::semicolon_if_nothing_returned)]
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_lines)]
#[allow(clippy::transmute_ptr_to_ptr)]
#[allow(clippy::undocumented_unsafe_blocks)]
#[allow(clippy::unnecessary_cast)]
#[allow(clippy::unreadable_literal)]
#[allow(clippy::used_underscore_binding)]
#[allow(clippy::useless_transmute)]
#[allow(clippy::use_self)]
mod bindings {
    include!(concat!(env!("OUT_DIR"), "/types.rs"));
}
pub use bindings::*;
