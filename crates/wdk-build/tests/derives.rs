// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Integration tests for [`wdk_build::derives::DerivesMap`] driven through
//! [`DerivesMap::from_file`]: writes a representative bindgen source snippet
//! to a temp file and asserts the recovered derive sets match each documented
//! bindgen output shape.

use assert_fs::{NamedTempFile, fixture::FileWriteStr};
use bindgen::callbacks::DeriveTrait;
use wdk_build::derives::{DerivesError, DerivesMap};

const ALL_TRAITS: &[DeriveTrait] = &[
    DeriveTrait::Copy,
    DeriveTrait::Debug,
    DeriveTrait::Default,
    DeriveTrait::Hash,
    DeriveTrait::PartialEqOrPartialOrd,
];

/// Writes `src` to a temp file and parses it through the public
/// [`DerivesMap::from_file`] entry point.
fn parse(src: &str) -> DerivesMap {
    let tmp = NamedTempFile::new("bindgen_output.rs").expect("create temp file");
    tmp.write_str(src).expect("write temp file");
    DerivesMap::from_file(tmp.path()).expect("parses")
}

/// Assert that `map` reports `satisfies(name, t) == true` for exactly the
/// traits in `expected`, and `false` for every other trait in [`ALL_TRAITS`].
fn assert_derives(map: &DerivesMap, name: &str, expected: &[DeriveTrait]) {
    for &t in ALL_TRAITS {
        let want = expected.contains(&t);
        let got = map.satisfies(name, t);
        assert_eq!(
            got, want,
            "{name}: satisfies({t:?}) = {got}, expected {want}"
        );
    }
}

#[test]
fn parses_representative_bindgen_output() {
    use DeriveTrait::{Copy, Debug, Default, Hash, PartialEqOrPartialOrd};

    // Shapes observed in real bindgen output for wdk-sys:
    //   - POD struct with the common four-trait derive
    //   - Union with only Copy/Clone (Rust unions can't auto-derive Debug/Default)
    //   - Bindgen's `__BindgenUnionField` wrapper — PartialEq without PartialOrd
    //   - Bindgen's `__IncompleteArrayField` wrapper — the full nine-trait derive
    //   - Type alias chain: `PodAliasChain = PodAlias = Pod` should inherit Pod's
    //     derives.
    let src = r#"
        #[repr(C)]
        #[derive(Debug, Default, Copy, Clone)]
        pub struct Pod { pub x: u32 }

        #[repr(C)]
        #[derive(Copy, Clone)]
        pub union Uni { pub a: u32, pub b: u64 }

        #[derive(PartialEq, Copy, Clone, Debug, Hash)]
        pub struct UnionField;

        #[derive(Copy, Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct ArrayField;

        pub type PodAlias = Pod;
        pub type PodAliasChain = PodAlias;

        pub type UCHAR = ::core::ffi::c_uchar;
        pub type ULONG = ::core::ffi::c_ulong;
        pub type PVOID = *mut ::core::ffi::c_void;
        pub type PULONG = *mut ULONG;

        // Option<fn>: fn contributes all-except-Default, Option adds Default back — ends up with all 5.
        pub type OptFn = ::core::option::Option<unsafe extern "C" fn(x: u32) -> u32>;

        // Bindgen module-enum pattern: inner `Type` aliases a primitive, and a use-rename re-exports it under a friendly name. The re-export must resolve to the inner `Type`'s derive set.
        pub mod _INTERFACE_TYPE {
            pub type Type = ::core::ffi::c_int;
            pub const Isa: Type = 1;
        }
        pub use self::_INTERFACE_TYPE::Type as INTERFACE_TYPE;
    "#;
    let map = parse(src);

    assert_derives(&map, "Pod", &[Copy, Debug, Default]);
    assert_derives(&map, "Uni", &[Copy]);
    assert_derives(
        &map,
        "UnionField",
        &[Copy, Debug, Hash, PartialEqOrPartialOrd],
    );
    assert_derives(
        &map,
        "ArrayField",
        &[Copy, Debug, Default, Hash, PartialEqOrPartialOrd],
    );

    // Type alias chain resolves through to Pod's derives.
    assert_derives(&map, "PodAlias", &[Copy, Debug, Default]);
    assert_derives(&map, "PodAliasChain", &[Copy, Debug, Default]);

    // Primitive-target type aliases: terminal shapes get the full standard derive
    // set directly, without chain resolution.
    for name in ["UCHAR", "ULONG", "PVOID", "PULONG"] {
        assert_derives(&map, name, ALL_TRAITS);
    }

    // Unknown type name: returns false for every trait, does not panic.
    assert_derives(&map, "Nonexistent", &[]);

    // Option<fn> — fn gives 4, Option adds Default → all 5.
    assert_derives(&map, "OptFn", ALL_TRAITS);

    // Module-enum pattern — both the compound key (`_INTERFACE_TYPE::Type`) and
    // the re-exported friendly name (`INTERFACE_TYPE`) inherit the primitive's
    // full derive set.
    assert_derives(&map, "_INTERFACE_TYPE::Type", ALL_TRAITS);
    assert_derives(&map, "INTERFACE_TYPE", ALL_TRAITS);
}

#[test]
fn from_file_missing_path_returns_io_error() {
    let err = DerivesMap::from_file(std::path::Path::new(
        "/this/path/does/not/exist/bindgen_output.rs",
    ))
    .expect_err("missing file must error");
    assert!(
        matches!(err, DerivesError::Io { .. }),
        "expected Io, got {err:?}"
    );
}

#[test]
fn from_file_invalid_rust_returns_parse_error() {
    let tmp = NamedTempFile::new("bad.rs").expect("create temp file");
    tmp.write_str("not @ valid @ rust @@@")
        .expect("write temp file");
    let err = DerivesMap::from_file(tmp.path()).expect_err("invalid syntax must error");
    assert!(
        matches!(err, DerivesError::Parse(_)),
        "expected Parse, got {err:?}"
    );
}

#[test]
fn from_file_foreign_mod_returns_unsupported_error() {
    let tmp = NamedTempFile::new("foreign.rs").expect("create temp file");
    tmp.write_str("extern \"C\" { pub fn f(); }")
        .expect("write temp file");
    let err = DerivesMap::from_file(tmp.path()).expect_err("foreign mod must error");
    assert!(
        matches!(err, DerivesError::UnsupportedSynNode { .. }),
        "expected UnsupportedSynNode, got {err:?}"
    );
}
