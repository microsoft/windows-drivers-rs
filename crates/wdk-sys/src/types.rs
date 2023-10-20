// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

#[allow(missing_docs)]
#[allow(non_upper_case_globals)]
#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[allow(clippy::cast_lossless)]
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cognitive_complexity)]
#[allow(clippy::default_trait_access)]
#[rustversion::attr(before(2023-09-10), allow(clippy::incorrect_clone_impl_on_copy_type))]
#[rustversion::attr(since(2023-09-10), allow(clippy::non_canonical_clone_impl))]
#[allow(clippy::missing_safety_doc)]
#[allow(clippy::missing_const_for_fn)]
#[allow(clippy::module_name_repetitions)]
#[allow(clippy::must_use_candidate)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[allow(clippy::ptr_as_ptr)]
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
