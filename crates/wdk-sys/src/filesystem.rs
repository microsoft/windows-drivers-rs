pub use bindings::*;

#[allow(missing_docs)]
#[allow(clippy::derive_partial_eq_without_eq)]
mod bindings {
    use crate::types::*;
    include!(concat!(env!("OUT_DIR"), "/filesystem.rs"));
}
