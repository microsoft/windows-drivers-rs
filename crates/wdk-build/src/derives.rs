// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Parses bindgen-emitted Rust source to recover the set of derives bindgen
//! applied to each generated type. Used by the per-subsystem bindgen pipeline
//! to answer `blocklisted_type_implements_trait` for base types.

use std::{
    collections::HashMap,
    path::{Path as FsPath, PathBuf},
    sync::Arc,
};

use bindgen::callbacks::{DeriveTrait, ImplementsTrait, ParseCallbacks};
use syn::{Attribute, Item, ItemUse, Path, PathArguments, Type, UseTree};
use thiserror::Error;

/// Rust language primitives that can appear as a bare identifier in a `pub type
/// X = Y;` target.
const PRIMITIVES: &[&str] = &[
    "bool", "char", "f32", "f64", "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32",
    "u64", "u128", "usize",
];

/// C stdint names that bindgen lowers to Rust integer primitives internally.
/// Bindgen never emits these as `pub type` aliases, so they have to be seeded
/// into the map directly. Mirrors bindgen 0.72.1's `is_stdint_type` allowlist —
/// re-verify on bindgen upgrades.
const STDINT_NAMES: &[&str] = &[
    "int8_t",
    "uint8_t",
    "int16_t",
    "uint16_t",
    "int32_t",
    "uint32_t",
    "int64_t",
    "uint64_t",
    "uintptr_t",
    "intptr_t",
    "ptrdiff_t",
    "size_t",
    "ssize_t",
];

/// Errors returned when parsing a bindgen-emitted source file into a
/// [`DerivesMap`].
#[derive(Debug, Error)]
pub enum DerivesError {
    #[error("failed to read {path}", path = path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse source as Rust")]
    Parse(#[source] syn::Error),

    #[error("unhandled syn node: {node}")]
    UnhandledSynCase { node: String },

    #[error("malformed shape: {reason}: {node}")]
    MalformedShape { reason: String, node: String },

    #[error("alias cycle among: {names:?}")]
    AliasCycle { names: Vec<String> },

    #[error("alias targets not found: {names:?}")]
    UnresolvedAlias { names: Vec<String> },
}

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    struct DerivesSet: u8 {
        const COPY                      = 1 << 0;
        const DEBUG                     = 1 << 1;
        const DEFAULT                   = 1 << 2;
        const HASH                      = 1 << 3;
        const PARTIAL_EQ_OR_PARTIAL_ORD = 1 << 4;
    }
}

impl DerivesSet {
    const fn implements(self, derive_trait: DeriveTrait) -> bool {
        let flag = match derive_trait {
            DeriveTrait::Copy => Self::COPY,
            DeriveTrait::Debug => Self::DEBUG,
            DeriveTrait::Default => Self::DEFAULT,
            DeriveTrait::Hash => Self::HASH,
            DeriveTrait::PartialEqOrPartialOrd => Self::PARTIAL_EQ_OR_PARTIAL_ORD,
        };
        self.contains(flag)
    }
}

impl From<Vec<String>> for DerivesSet {
    /// Build a `DerivesSet` from a list of derive trait names.
    fn from(derives: Vec<String>) -> Self {
        let mut set = Self::empty();
        for derive in &derives {
            set |= match derive.as_str() {
                "Copy" => Self::COPY,
                "Debug" => Self::DEBUG,
                "Default" => Self::DEFAULT,
                "Hash" => Self::HASH,
                "PartialEq" | "PartialOrd" => Self::PARTIAL_EQ_OR_PARTIAL_ORD,
                _ => Self::empty(),
            };
        }
        set
    }
}

enum DerivesSource {
    Direct(DerivesSet),
    Alias(String),
}

/// Bindgen parse callback for `blocklisted_type_implements_trait` from a
/// pre-built [`DerivesMap`].
#[derive(Debug)]
pub struct BaseDerivesCallback {
    map: Arc<DerivesMap>,
}

impl BaseDerivesCallback {
    #[must_use]
    pub const fn new(map: Arc<DerivesMap>) -> Self {
        Self { map }
    }
}

impl ParseCallbacks for BaseDerivesCallback {
    fn blocklisted_type_implements_trait(
        &self,
        name: &str,
        derive_trait: DeriveTrait,
    ) -> Option<ImplementsTrait> {
        Some(if self.map.satisfies(name, derive_trait) {
            ImplementsTrait::Yes
        } else {
            ImplementsTrait::No
        })
    }
}

/// Map storing Rust source type names to the set of derives the type
/// implements.
#[derive(Debug)]
pub struct DerivesMap {
    types: HashMap<String, DerivesSet>,
}

impl DerivesMap {
    /// Reads a Rust source file from disk and parses its derive
    /// information. See [`DerivesMap::from_source`] for the parsing behavior.
    ///
    /// # Errors
    ///
    /// Returns:
    /// - [`DerivesError::Io`] if the file cannot be read
    /// - any variant returned by [`DerivesMap::from_source`] if the contents
    ///   cannot be parsed
    pub fn from_file(path: &FsPath) -> Result<Self, DerivesError> {
        let source = std::fs::read_to_string(path).map_err(|source| DerivesError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_source(&source)
    }

    /// Returns whether `name`'s recorded derive set contains `derive_trait`.
    /// Returns `false` if `name` is not recorded.
    #[must_use]
    pub fn satisfies(&self, name: &str, derive_trait: DeriveTrait) -> bool {
        self.types
            .get(name)
            .is_some_and(|&set| set.implements(derive_trait))
    }

    /// Parses a Rust source file and records the derive set for every
    /// top-level `struct`, `union`, `enum`, and type alias. Unknown derive
    /// idents are ignored.
    ///
    /// # Errors
    ///
    /// Returns:
    /// - [`DerivesError::Parse`] if `source` is not valid Rust
    /// - [`DerivesError::UnhandledSynCase`] or [`DerivesError::MalformedShape`]
    ///   if a classified construct does not match any recognized bindgen output
    ///   shape
    /// - [`DerivesError::UnresolvedAlias`] or [`DerivesError::AliasCycle`] if
    ///   an alias cannot be resolved to a recorded type
    fn from_source(source: &str) -> Result<Self, DerivesError> {
        let file = syn::parse_str::<syn::File>(source).map_err(DerivesError::Parse)?;
        let mut derives_map = Self::with_std_types();

        let mut aliases: HashMap<String, String> = HashMap::default();
        for (key, source) in idents_and_derives_for_items(&file.items)? {
            match source {
                DerivesSource::Direct(derives_set) => {
                    derives_map.types.insert(key, derives_set);
                }
                DerivesSource::Alias(aliased_to) => {
                    aliases.insert(key, aliased_to);
                }
            }
        }

        derives_map.resolve_aliases(&aliases)?;

        Ok(derives_map)
    }

    fn with_std_types() -> Self {
        Self {
            types: STDINT_NAMES
                .iter()
                .map(|&n| (n.to_owned(), DerivesSet::all()))
                .collect(),
        }
    }

    /// Resolve every alias in `aliases` by walking its chain to a recorded
    /// type and copying that type's derive set onto each alias along the way.
    ///
    /// # Errors
    ///
    /// Returns:
    /// - [`DerivesError::UnresolvedAlias`] if a chain terminates at a name that
    ///   is neither a recorded type nor a queued alias
    /// - [`DerivesError::AliasCycle`] if a chain revisits a name it has already
    ///   walked through
    fn resolve_aliases(&mut self, aliases: &HashMap<String, String>) -> Result<(), DerivesError> {
        for key in aliases.keys() {
            if self.types.contains_key(key) {
                continue;
            }

            let mut curr = key;
            let mut walked = vec![curr];
            while !self.types.contains_key(curr) {
                let Some(next) = aliases.get(curr) else {
                    return Err(DerivesError::UnresolvedAlias {
                        names: walked.into_iter().cloned().collect(),
                    });
                };
                if walked.contains(&next) {
                    return Err(DerivesError::AliasCycle {
                        names: walked.into_iter().cloned().collect(),
                    });
                }
                walked.push(next);
                curr = next;
            }

            let target_derive_set = *self
                .types
                .get(curr)
                .expect("`self.types.contains_key(curr)` just returned true");

            for new_derive_key in walked {
                self.types.insert(new_derive_key.clone(), target_derive_set);
            }
        }

        Ok(())
    }
}

/// Classify the type-defining [`syn::Item`]s in `items`, returning their
/// type names and [`DerivesSource`]s.
///
/// # Bindgen shapes
///
/// Struct / Union / Enum: derives come from the `#[derive(...)]` attrs:
///
/// ```ignore
/// #[derive(Debug, Default, Copy, Clone, Hash, PartialOrd, Ord, PartialEq, Eq)]
/// pub struct _DMF_MODULE_DESCRIPTOR { pub Size: u32, /* ... */ }
/// ```
///
/// Type alias / Module / Use: dispatched to the corresponding classifier.
///
/// Impl / Const: bindgen helper blocks and anonymous layout assertions.
/// Neither contributes derive information; both are ignored.
///
/// # Errors
///
/// Returns:
/// - [`DerivesError::UnhandledSynCase`] for `Item` variants other than
///   Struct/Union/Enum/Type/Mod/Use/Impl/Const
/// - any error propagated from the per-shape classifiers
fn idents_and_derives_for_items(
    items: &[Item],
) -> Result<Vec<(String, DerivesSource)>, DerivesError> {
    let mut derives: Vec<(String, DerivesSource)> = vec![];

    for item in items {
        match item {
            Item::Struct(s) => derives.push((
                s.ident.to_string(),
                DerivesSource::Direct(derives_from_attrs(&s.attrs).into()),
            )),
            Item::Union(u) => derives.push((
                u.ident.to_string(),
                DerivesSource::Direct(derives_from_attrs(&u.attrs).into()),
            )),
            Item::Enum(e) => derives.push((
                e.ident.to_string(),
                DerivesSource::Direct(derives_from_attrs(&e.attrs).into()),
            )),
            Item::Type(t) => derives.push((t.ident.to_string(), derives_for_type(&t.ty)?)),
            Item::Mod(m) => derives.extend(idents_and_derives_for_mod(m)?),
            Item::Use(u) => derives.push(ident_and_derives_for_use(u)?),
            Item::Impl(_) | Item::Const(_) => {}
            other => {
                return Err(DerivesError::UnhandledSynCase {
                    node: format!("{other:?}"),
                });
            }
        }
    }
    Ok(derives)
}

/// Collects the derive trait names from a `#[derive(...)]` attribute list.
fn derives_from_attrs(attrs: &[Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("derive"))
        .filter_map(|attr| {
            attr.parse_args_with(
                syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
            )
            .ok()
        })
        .flatten()
        .filter_map(|path| {
            path.segments
                .into_iter()
                .next_back()
                .map(|seg| seg.ident.to_string())
        })
        .collect()
}

/// Classify a [`syn::Type`] into the [`DerivesSource`] it represents.
///
/// # Bindgen shapes
///
/// ```ignore
/// pub type DMFMODULE = *mut DMFMODULE__;                   // Type::Ptr
///
/// pub type __C_ASSERT__ = [::core::ffi::c_char; 1usize];   // Type::Array
///
///
/// pub type EVT_DMF_CALLBACK = ::core::option::Option<      // Type::Path (Option)
///     unsafe extern "C" fn(/* ... */) -> NTSTATUS,
/// >;
///
/// pub type DMF_TIME_FIELDS = _DMF_TIME_FIELDS;             // Type::Path (named)
///
/// pub type WCHAR = u16;                                    // Type::Path (primitive)
/// ```
///
/// # Errors
///
/// Returns:
/// - [`DerivesError::UnhandledSynCase`] if `ty` is a `syn::Type` variant other
///   than Ptr/Path/Array
/// - [`DerivesError::MalformedShape`] if the path has no segments
/// - [`DerivesError::UnhandledSynCase`] if the path has generic arguments
fn derives_for_type(ty: &Type) -> Result<DerivesSource, DerivesError> {
    match ty {
        Type::Ptr(_) => Ok(DerivesSource::Direct(DerivesSet::all())),
        Type::Array(arr) => derives_for_type(&arr.elem),
        Type::Path(tp) => {
            if path_is_option(&tp.path) && inner_is_bare_fn(&tp.path) {
                return Ok(DerivesSource::Direct(DerivesSet::all()));
            }

            let Some(last) = tp.path.segments.last() else {
                return Err(DerivesError::MalformedShape {
                    reason: "alias path has no segments".to_owned(),
                    node: format!("{tp:?}"),
                });
            };

            let PathArguments::None = last.arguments else {
                return Err(DerivesError::UnhandledSynCase {
                    node: format!("{:?}", last.arguments),
                });
            };

            if PRIMITIVES.iter().any(|&p| last.ident == p) || path_is_core_ffi_type(&tp.path) {
                return Ok(DerivesSource::Direct(DerivesSet::all()));
            }

            Ok(DerivesSource::Alias(last.ident.to_string()))
        }
        other => Err(DerivesError::UnhandledSynCase {
            node: format!("{other:?}"),
        }),
    }
}

/// Classify the type-defining items inside a [`syn::ItemMod`] (bindgen's
/// C-enum-as-module pattern), returning their prefixed type names and
/// [`DerivesSource`]s.
///
/// Registers the inner `Type` under a compound key like
/// `_INTERFACE_TYPE::Type` so other types can link to it via an alias.
///
/// # Bindgen shapes
///
/// ```ignore
/// pub mod _INTERFACE_TYPE {
///     pub type Type = ::core::ffi::c_int;
///     pub const Isa: Type = 1;
///     pub const Eisa: Type = 2;
///     // ...
/// }
/// pub use self::_INTERFACE_TYPE::Type as INTERFACE_TYPE;
/// ```
///
/// # Errors
///
/// Returns any error propagated from [`idents_and_derives_for_items`] on
/// the module's inner items.
fn idents_and_derives_for_mod(
    m: &syn::ItemMod,
) -> Result<Vec<(String, DerivesSource)>, DerivesError> {
    let Some((_, mod_items)) = &m.content else {
        return Ok(vec![]);
    };
    let prefix = format!("{}::", m.ident);

    let mut mod_items_derives = idents_and_derives_for_items(mod_items)?;

    for (key, _) in &mut mod_items_derives {
        key.insert_str(0, &prefix);
    }
    Ok(mod_items_derives)
}

/// Classify a [`syn::ItemUse`] (bindgen's `pub use self::_FOO::Type as
/// FOO;` rename), returning the type name and the classified
/// [`DerivesSource`].
///
/// # Bindgen shapes
///
/// ```ignore
/// pub use self::_INTERFACE_TYPE::Type as INTERFACE_TYPE;
/// pub use self::_POWER_STATE_TYPE::Type as POWER_STATE_TYPE;
/// pub use self::_DEVICE_POWER_STATE::Type as DEVICE_POWER_STATE;
/// ```
///
/// # Errors
///
/// Returns:
/// - [`DerivesError::UnhandledSynCase`] for `UseTree` variants other than
///   `Path`/`Rename`
fn ident_and_derives_for_use(item_use: &ItemUse) -> Result<(String, DerivesSource), DerivesError> {
    let mut segments: Vec<String> = Vec::new();
    let mut use_tree = &item_use.tree;

    while let UseTree::Path(path) = use_tree {
        let seg = path.ident.to_string();
        if seg != "self" {
            segments.push(seg);
        }
        use_tree = &path.tree;
    }

    let UseTree::Rename(use_rename) = use_tree else {
        return Err(DerivesError::UnhandledSynCase {
            node: format!("{use_tree:?}"),
        });
    };

    segments.push(use_rename.ident.to_string());
    Ok((
        use_rename.rename.to_string(),
        DerivesSource::Alias(segments.join("::")),
    ))
}

/// True when the last segment of `path` has a bare-fn type as its first
/// generic argument.
fn inner_is_bare_fn(path: &Path) -> bool {
    let Some(last) = path.segments.last() else {
        return false;
    };
    let PathArguments::AngleBracketed(args) = &last.arguments else {
        return false;
    };
    matches!(
        args.args.first(),
        Some(syn::GenericArgument::Type(Type::BareFn(_)))
    )
}

/// True when `path` ends in `core::option::Option`.
fn path_is_option(path: &Path) -> bool {
    let segs = &path.segments;
    segs.len() >= 3
        && segs[segs.len() - 3].ident == "core"
        && segs[segs.len() - 2].ident == "option"
        && segs[segs.len() - 1].ident == "Option"
}

/// True when `path` ends in `core::ffi::*`.
fn path_is_core_ffi_type(path: &Path) -> bool {
    let segs = &path.segments;
    segs.len() >= 3 && segs[segs.len() - 3].ident == "core" && segs[segs.len() - 2].ident == "ffi"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> DerivesMap {
        DerivesMap::from_source(src).expect("parses")
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn parses_representative_bindgen_output() {
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

        assert!(map.satisfies("Pod", DeriveTrait::Debug));
        assert!(map.satisfies("Pod", DeriveTrait::Default));
        assert!(map.satisfies("Pod", DeriveTrait::Copy));
        assert!(!map.satisfies("Pod", DeriveTrait::Hash));
        assert!(!map.satisfies("Pod", DeriveTrait::PartialEqOrPartialOrd));

        assert!(map.satisfies("Uni", DeriveTrait::Copy));
        assert!(!map.satisfies("Uni", DeriveTrait::Debug));
        assert!(!map.satisfies("Uni", DeriveTrait::Default));

        // PartialEq alone now satisfies the grouped bindgen query.
        assert!(map.satisfies("UnionField", DeriveTrait::PartialEqOrPartialOrd));
        assert!(map.satisfies("UnionField", DeriveTrait::Hash));

        assert!(map.satisfies("ArrayField", DeriveTrait::PartialEqOrPartialOrd));
        assert!(map.satisfies("ArrayField", DeriveTrait::Hash));

        // Alias chain resolves through to Pod's derives.
        assert!(map.satisfies("PodAlias", DeriveTrait::Debug));
        assert!(map.satisfies("PodAlias", DeriveTrait::Default));
        assert!(map.satisfies("PodAliasChain", DeriveTrait::Debug));
        assert!(map.satisfies("PodAliasChain", DeriveTrait::Default));

        // Primitive-target aliases: terminal shapes get the full standard derive set
        // directly, without chain resolution.
        for trait_ in [
            DeriveTrait::Copy,
            DeriveTrait::Debug,
            DeriveTrait::Default,
            DeriveTrait::Hash,
            DeriveTrait::PartialEqOrPartialOrd,
        ] {
            assert!(map.satisfies("UCHAR", trait_));
            assert!(map.satisfies("ULONG", trait_));
            assert!(map.satisfies("PVOID", trait_));
            assert!(map.satisfies("PULONG", trait_));
        }

        // Unknown type name: returns false, does not panic.
        assert!(!map.satisfies("Nonexistent", DeriveTrait::Debug));

        // Option<fn> — fn gives 4, Option adds Default → all 5.
        for trait_ in [
            DeriveTrait::Copy,
            DeriveTrait::Debug,
            DeriveTrait::Default,
            DeriveTrait::Hash,
            DeriveTrait::PartialEqOrPartialOrd,
        ] {
            assert!(map.satisfies("OptFn", trait_));
        }

        // Module-enum pattern — both the compound key (`_INTERFACE_TYPE::Type`) and the
        // re-exported friendly name (`INTERFACE_TYPE`) inherit the primitive's full
        // derive set.
        for trait_ in [
            DeriveTrait::Copy,
            DeriveTrait::Debug,
            DeriveTrait::Default,
            DeriveTrait::Hash,
            DeriveTrait::PartialEqOrPartialOrd,
        ] {
            assert!(map.satisfies("_INTERFACE_TYPE::Type", trait_));
            assert!(map.satisfies("INTERFACE_TYPE", trait_));
        }
    }

    /// Every seeded stdint name derives the full standard set. Guards the
    /// hand-maintained `STDINT_NAMES` list against accidental deletion and
    /// keeps the `satisfies` result shape in sync with the seed.
    #[test]
    fn stdint_names_all_derive_standard_set() {
        let map = parse("");
        for name in STDINT_NAMES {
            for trait_ in [
                DeriveTrait::Copy,
                DeriveTrait::Debug,
                DeriveTrait::Default,
                DeriveTrait::Hash,
                DeriveTrait::PartialEqOrPartialOrd,
            ] {
                assert!(
                    map.satisfies(name, trait_),
                    "stdint {name} missing {trait_:?}"
                );
            }
        }
    }

    /// A cyclic alias pair (`A = B; B = A;`) must surface as `AliasCycle` —
    /// the chain-walking loop detects it when a step revisits a name already
    /// in the walked set.
    #[test]
    fn alias_cycle_terminates() {
        let src = r"
            pub type A = B;
            pub type B = A;
        ";
        let err = DerivesMap::from_source(src).expect_err("cycle must error");
        match err {
            DerivesError::AliasCycle { mut names } => {
                names.sort();
                assert_eq!(names, vec!["A".to_owned(), "B".to_owned()]);
            }
            other => panic!("expected AliasCycle, got {other:?}"),
        }
    }

    /// An alias whose target is neither a recorded type nor another pending
    /// alias must surface as `UnresolvedAlias`.
    #[test]
    fn unresolvable_alias_errors() {
        let src = r"
            pub type UnknownAlias = SomeUnparsedType;
        ";
        let err = DerivesMap::from_source(src).expect_err("unresolvable must error");
        match err {
            DerivesError::UnresolvedAlias { names } => {
                assert_eq!(
                    names,
                    vec!["UnknownAlias".to_owned(), "SomeUnparsedType".to_owned()]
                );
            }
            other => panic!("expected UnresolvedAlias, got {other:?}"),
        }
    }

    /// `BaseDerivesCallback` must translate `bool` into the bindgen
    /// `Some(Yes)` / `Some(No)` answers expected for blocklisted types.
    #[test]
    fn base_callback_known_positive_returns_yes() {
        let src = r"
            #[derive(Copy, Clone, Debug)]
            pub struct Pod;
        ";
        let map = Arc::new(parse(src));
        let cb = BaseDerivesCallback::new(map);
        assert!(matches!(
            cb.blocklisted_type_implements_trait("Pod", DeriveTrait::Debug),
            Some(ImplementsTrait::Yes)
        ));
    }

    #[test]
    fn base_callback_known_negative_returns_no() {
        let src = r"
            #[derive(Copy, Clone)]
            pub struct Pod;
        ";
        let map = Arc::new(parse(src));
        let cb = BaseDerivesCallback::new(map);
        assert!(matches!(
            cb.blocklisted_type_implements_trait("Pod", DeriveTrait::Debug),
            Some(ImplementsTrait::No)
        ));
    }

    #[test]
    fn base_callback_unknown_returns_no() {
        let map = Arc::new(parse(""));
        let cb = BaseDerivesCallback::new(map);
        assert!(matches!(
            cb.blocklisted_type_implements_trait("Nonexistent", DeriveTrait::Debug),
            Some(ImplementsTrait::No)
        ));
    }
}
