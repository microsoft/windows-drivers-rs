// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Parses bindgen-emitted Rust source to recover the set of derives bindgen
//! applied to each generated type. Used by the per-subsystem bindgen pipeline
//! to implement `blocklisted_type_implements_trait` for base types.

use std::{
    collections::HashMap,
    path::{Path as FsPath, PathBuf},
    sync::Arc,
};

use bindgen::callbacks::{DeriveTrait, ImplementsTrait, ParseCallbacks};
use syn::{Attribute, Item, ItemUse, Path, PathArguments, Type, UseTree};
use thiserror::Error;

/// Primitives that derive every tracked trait.
const PRIMITIVES_DERIVE_ALL: &[&str] = &[
    "bool", "char", "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128",
    "usize",
];

/// Primitives that derive every tracked trait except `Hash`.
const PRIMITIVES_DERIVE_ALL_EXCEPT_HASH: &[&str] = &["f16", "f32", "f64", "f128"];

/// Primitives that derive every tracked trait except `Copy` and `Default`.
const PRIMITIVES_DERIVE_ALL_EXCEPT_COPY_AND_DEFAULT: &[&str] = &["str"];

/// C stdint names that bindgen lowers to Rust integer primitives internally.
/// Bindgen never emits these as `pub type` aliases, so they have to be seeded
/// into the map directly. Mirrors bindgen 0.72.1's [`is_stdint_type`](https://github.com/rust-lang/rust-bindgen/blob/d874de8d646d9b8a3e7ba2db2bcd52f2fba8f1f5/bindgen/ir/context.rs#L2378-L2386)
/// allowlist.
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
    /// Reading the bindgen-emitted source file from disk failed.
    #[error("failed to read {path}", path = path.display())]
    Io {
        /// Path to the file that could not be read.
        path: PathBuf,
        /// Underlying I/O error from the filesystem operation.
        #[source]
        source: std::io::Error,
    },

    /// `syn` failed to parse the source as Rust.
    #[error("failed to parse source as Rust")]
    Parse(#[source] syn::Error),

    /// Encountered a syn AST node variant this parser does not support.
    #[error("unsupported syn node: {node}")]
    UnsupportedSynNode {
        /// Debug-formatted representation of the unsupported node.
        node: String,
    },

    /// A recognized item kind whose internal shape did not match what the
    /// parser expects from bindgen output.
    #[error("malformed shape: {reason}: {node}")]
    MalformedShape {
        /// Why the node shape is considered malformed.
        reason: String,
        /// Debug-formatted representation of the malformed node.
        node: String,
    },

    /// Type alias chain visited the same name twice while walking type aliases
    /// to their target type.
    #[error("type alias cycle among: {names:?}")]
    TypeAliasCycle {
        /// Names participating in the detected cycle, in walk order.
        names: Vec<String>,
    },

    /// Type alias chain terminated at a name that is neither a recorded type
    /// nor another pending type alias.
    #[error("type alias target not found: {target}")]
    UnresolvedTypeAlias {
        /// The unresolved target name.
        target: String,
    },
}

/// The set of standard traits a bindgen-generated type derives. Each field
/// records whether the type derives the corresponding trait.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "type represents an independent set of flags, not a state machine"
)]
struct DerivesSet {
    copy: bool,
    debug: bool,
    default: bool,
    hash: bool,
    partial_eq_or_partial_ord: bool,
}

impl DerivesSet {
    /// A set in which every tracked trait is derived.
    const fn all() -> Self {
        Self {
            copy: true,
            debug: true,
            default: true,
            hash: true,
            partial_eq_or_partial_ord: true,
        }
    }

    /// Record that the type derives `derive_trait`.
    const fn insert(&mut self, derive_trait: DeriveTrait) {
        match derive_trait {
            DeriveTrait::Copy => self.copy = true,
            DeriveTrait::Debug => self.debug = true,
            DeriveTrait::Default => self.default = true,
            DeriveTrait::Hash => self.hash = true,
            DeriveTrait::PartialEqOrPartialOrd => self.partial_eq_or_partial_ord = true,
        }
    }

    const fn implements(self, derive_trait: DeriveTrait) -> bool {
        match derive_trait {
            DeriveTrait::Copy => self.copy,
            DeriveTrait::Debug => self.debug,
            DeriveTrait::Default => self.default,
            DeriveTrait::Hash => self.hash,
            DeriveTrait::PartialEqOrPartialOrd => self.partial_eq_or_partial_ord,
        }
    }
}

/// Map a bindgen `#[derive(...)]` ident to the tracked [`DeriveTrait`] it
/// represents, or `None` if the parser does not track it.
fn derive_trait_from_name(name: &str) -> Option<DeriveTrait> {
    Some(match name {
        "Copy" => DeriveTrait::Copy,
        "Debug" => DeriveTrait::Debug,
        "Default" => DeriveTrait::Default,
        "Hash" => DeriveTrait::Hash,
        "PartialEq" | "PartialOrd" => DeriveTrait::PartialEqOrPartialOrd,
        _ => return None,
    })
}

impl From<Vec<String>> for DerivesSet {
    /// Build a `DerivesSet` from a list of derive trait names. Names the parser
    /// does not track are ignored.
    fn from(derives: Vec<String>) -> Self {
        let mut set = Self::default();
        for derive in &derives {
            if let Some(derive_trait) = derive_trait_from_name(derive) {
                set.insert(derive_trait);
            }
        }
        set
    }
}

#[derive(Debug)]
enum DerivesSource {
    Direct(DerivesSet),
    TypeAlias(String),
}

/// Bindgen parse callback for `blocklisted_type_implements_trait` from a
/// pre-built [`DerivesMap`].
#[derive(Debug)]
pub struct BaseDerivesCallback {
    map: Arc<DerivesMap>,
}

impl BaseDerivesCallback {
    /// Wrap a shared [`DerivesMap`] for use as a `bindgen` [`ParseCallbacks`].
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

    /// Parses the given Rust source file and records the derive set for
    /// every top-level `struct`, `union`, `enum`, and type alias. Unknown
    /// derive idents are ignored.
    ///
    /// This does not support bindgen output containing function declarations
    /// (`CodegenConfig::FUNCTIONS`).
    ///
    /// # Errors
    ///
    /// Returns:
    /// - [`DerivesError::Parse`] if `source` is not valid Rust
    /// - [`DerivesError::UnsupportedSynNode`] or
    ///   [`DerivesError::MalformedShape`] if a classified construct does not
    ///   match any recognized bindgen output shape
    /// - [`DerivesError::UnresolvedTypeAlias`] or
    ///   [`DerivesError::TypeAliasCycle`] if a type alias cannot be resolved to
    ///   a recorded type
    fn from_source(source: &str) -> Result<Self, DerivesError> {
        let file = syn::parse_str::<syn::File>(source).map_err(DerivesError::Parse)?;
        let mut derives_map = Self::with_std_types();

        let mut type_aliases: HashMap<String, String> = HashMap::default();
        for (key, source) in idents_and_derives_for_items(&file.items)? {
            match source {
                DerivesSource::Direct(derives_set) => {
                    derives_map.types.insert(key, derives_set);
                }
                DerivesSource::TypeAlias(aliased_to) => {
                    type_aliases.insert(key, aliased_to);
                }
            }
        }

        derives_map.resolve_type_aliases(&type_aliases)?;

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

    /// Resolve every type alias in `type_aliases` by walking its chain to a
    /// recorded type and copying that type's derive set onto each type
    /// alias along the way.
    ///
    /// # Errors
    ///
    /// Returns:
    /// - [`DerivesError::UnresolvedTypeAlias`] if a chain terminates at a name
    ///   that is neither a recorded type nor a queued type alias
    /// - [`DerivesError::TypeAliasCycle`] if a chain revisits a name it has
    ///   already walked through
    fn resolve_type_aliases(
        &mut self,
        type_aliases: &HashMap<String, String>,
    ) -> Result<(), DerivesError> {
        for key in type_aliases.keys() {
            if self.types.contains_key(key) {
                continue;
            }

            let mut curr = key;
            let mut walked = vec![curr];
            while !self.types.contains_key(curr) {
                let Some(next) = type_aliases.get(curr) else {
                    return Err(DerivesError::UnresolvedTypeAlias {
                        target: curr.clone(),
                    });
                };
                if walked.contains(&next) {
                    return Err(DerivesError::TypeAliasCycle {
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
/// - [`DerivesError::UnsupportedSynNode`] for `Item` variants other than
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
                return Err(DerivesError::UnsupportedSynNode {
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

/// The tracked-derive set for a primitive or `core::ffi` type `path`, or `None`
/// if `path` is not a primitive.
fn primitive_derives(path: &Path) -> Option<DerivesSet> {
    let seg = path.segments.last()?;
    if PRIMITIVES_DERIVE_ALL.iter().any(|&p| seg.ident == p) || path_is_core_ffi_type(path) {
        Some(DerivesSet::all())
    } else if PRIMITIVES_DERIVE_ALL_EXCEPT_HASH
        .iter()
        .any(|&p| seg.ident == p)
    {
        Some(DerivesSet {
            hash: false,
            ..DerivesSet::all()
        })
    } else if PRIMITIVES_DERIVE_ALL_EXCEPT_COPY_AND_DEFAULT
        .iter()
        .any(|&p| seg.ident == p)
    {
        Some(DerivesSet {
            copy: false,
            default: false,
            ..DerivesSet::all()
        })
    } else {
        None
    }
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
/// - [`DerivesError::UnsupportedSynNode`] if `ty` is a `syn::Type` variant
///   other than Ptr/Path/Array, or if the path has generic arguments
/// - [`DerivesError::MalformedShape`] if the path has no segments
fn derives_for_type(ty: &Type) -> Result<DerivesSource, DerivesError> {
    match ty {
        Type::Ptr(_) => Ok(DerivesSource::Direct(DerivesSet::all())),
        Type::Array(arr) => derives_for_type(&arr.elem),
        Type::Path(tp) => {
            // bindgen's callback shape: `Option<unsafe extern "C" fn(...)>`.
            // A nullable function pointer derives every tracked trait.
            if path_is_option(&tp.path) && inner_is_bare_fn(&tp.path) {
                return Ok(DerivesSource::Direct(DerivesSet::all()));
            }

            let Some(last) = tp.path.segments.last() else {
                return Err(DerivesError::MalformedShape {
                    reason: "type alias path has no segments".to_owned(),
                    node: format!("{tp:?}"),
                });
            };

            let PathArguments::None = last.arguments else {
                return Err(DerivesError::UnsupportedSynNode {
                    node: format!("{:?}", last.arguments),
                });
            };

            if let Some(set) = primitive_derives(&tp.path) {
                return Ok(DerivesSource::Direct(set));
            }

            let qualified_name = tp
                .path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            Ok(DerivesSource::TypeAlias(qualified_name))
        }
        other => Err(DerivesError::UnsupportedSynNode {
            node: format!("{other:?}"),
        }),
    }
}

/// Classify the type-defining items inside a [`syn::ItemMod`] (bindgen's
/// C-enum-as-module pattern), returning their prefixed type names and
/// [`DerivesSource`]s.
///
/// Registers the inner `Type` under a compound key like
/// `_INTERFACE_TYPE::Type` so other types can link to it via a type alias.
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
/// - [`DerivesError::UnsupportedSynNode`] for `UseTree` variants other than
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
        return Err(DerivesError::UnsupportedSynNode {
            node: format!("{use_tree:?}"),
        });
    };

    segments.push(use_rename.ident.to_string());
    Ok((
        use_rename.rename.to_string(),
        DerivesSource::TypeAlias(segments.join("::")),
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

    #[track_caller]
    fn assert_direct_all(source: DerivesSource) {
        match source {
            DerivesSource::Direct(set) => assert_eq!(set, DerivesSet::all()),
            DerivesSource::TypeAlias(name) => {
                panic!("expected Direct(all), got TypeAlias({name:?})")
            }
        }
    }

    #[track_caller]
    fn assert_type_alias(source: DerivesSource, expected: &str) {
        match source {
            DerivesSource::TypeAlias(s) => assert_eq!(s, expected),
            DerivesSource::Direct(set) => {
                panic!("expected TypeAlias({expected:?}), got Direct({set:?})")
            }
        }
    }

    mod path_checks {
        use syn::parse_str;

        use super::*;

        #[test]
        fn path_is_option_recognizes_full_path() {
            let p: Path = parse_str("::core::option::Option").unwrap();
            assert!(path_is_option(&p));
            let p: Path = parse_str("core::option::Option").unwrap();
            assert!(path_is_option(&p));
        }

        #[test]
        fn path_is_option_rejects_short_or_wrong_paths() {
            let p: Path = parse_str("Option").unwrap();
            assert!(!path_is_option(&p));
            let p: Path = parse_str("std::option::Option").unwrap();
            assert!(!path_is_option(&p));
            let p: Path = parse_str("core::ffi::c_void").unwrap();
            assert!(!path_is_option(&p));
        }

        #[test]
        fn path_is_core_ffi_type_recognizes() {
            let p: Path = parse_str("::core::ffi::c_void").unwrap();
            assert!(path_is_core_ffi_type(&p));
            let p: Path = parse_str("core::ffi::c_int").unwrap();
            assert!(path_is_core_ffi_type(&p));
        }

        #[test]
        fn path_is_core_ffi_type_rejects_non_ffi() {
            let p: Path = parse_str("core::option::Option").unwrap();
            assert!(!path_is_core_ffi_type(&p));
            let p: Path = parse_str("std::ffi::CStr").unwrap();
            assert!(!path_is_core_ffi_type(&p));
            let p: Path = parse_str("c_int").unwrap();
            assert!(!path_is_core_ffi_type(&p));
        }

        #[test]
        fn inner_is_bare_fn_true_for_option_fn() {
            let p: Path =
                parse_str("::core::option::Option<unsafe extern \"C\" fn() -> u32>").unwrap();
            assert!(inner_is_bare_fn(&p));
        }

        #[test]
        fn inner_is_bare_fn_false_for_other_generics() {
            let p: Path = parse_str("Option<u32>").unwrap();
            assert!(!inner_is_bare_fn(&p));
            let p: Path = parse_str("Vec<u8>").unwrap();
            assert!(!inner_is_bare_fn(&p));
        }

        #[test]
        fn inner_is_bare_fn_false_for_no_generics() {
            let p: Path = parse_str("u32").unwrap();
            assert!(!inner_is_bare_fn(&p));
        }
    }

    mod classifiers {
        use syn::parse_str;

        use super::*;

        #[test]
        fn derives_from_attrs_extracts_idents() {
            let item: syn::ItemStruct =
                parse_str("#[derive(Copy, Clone, Debug)] pub struct S;").unwrap();
            let derives = derives_from_attrs(&item.attrs);
            assert_eq!(derives, vec!["Copy", "Clone", "Debug"]);
        }

        #[test]
        fn derives_from_attrs_ignores_non_derive_attrs() {
            let item: syn::ItemStruct =
                parse_str("#[repr(C)] #[derive(Copy)] #[allow(dead_code)] pub struct S;").unwrap();
            let derives = derives_from_attrs(&item.attrs);
            assert_eq!(derives, vec!["Copy"]);
        }

        #[test]
        fn derives_from_attrs_uses_last_path_segment() {
            let item: syn::ItemStruct =
                parse_str("#[derive(::core::marker::Copy)] pub struct S;").unwrap();
            let derives = derives_from_attrs(&item.attrs);
            assert_eq!(derives, vec!["Copy"]);
        }

        #[test]
        fn derives_from_attrs_no_derives_returns_empty() {
            let item: syn::ItemStruct = parse_str("#[repr(C)] pub struct S;").unwrap();
            assert!(derives_from_attrs(&item.attrs).is_empty());
        }

        #[test]
        fn from_maps_every_tracked_derive_name() {
            let names = [
                "Copy",
                "Debug",
                "Default",
                "Hash",
                "PartialEq",
                "PartialOrd",
            ]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>();
            let set = DerivesSet::from(names);
            for derive_trait in [
                DeriveTrait::Copy,
                DeriveTrait::Debug,
                DeriveTrait::Default,
                DeriveTrait::Hash,
                DeriveTrait::PartialEqOrPartialOrd,
            ] {
                assert!(set.implements(derive_trait), "{derive_trait:?}");
            }

            // Untracked derive names are dropped, not errors.
            let untracked = vec!["Clone".to_owned(), "Eq".to_owned(), "Ord".to_owned()];
            assert_eq!(DerivesSet::from(untracked), DerivesSet::default());
        }

        #[test]
        fn derives_for_type_pointer_gets_all() {
            let ty: Type = parse_str("*mut u32").unwrap();
            assert_direct_all(derives_for_type(&ty).unwrap());
            let ty: Type = parse_str("*const ::core::ffi::c_void").unwrap();
            assert_direct_all(derives_for_type(&ty).unwrap());
        }

        #[test]
        fn derives_for_type_array_recurses_into_element() {
            let ty: Type = parse_str("[u32; 4]").unwrap();
            assert_direct_all(derives_for_type(&ty).unwrap());

            let ty: Type = parse_str("[SomeAlias; 8]").unwrap();
            assert_type_alias(derives_for_type(&ty).unwrap(), "SomeAlias");

            // Element with a partial derive set propagates that set, not `all()`.
            let ty: Type = parse_str("[f32; 8]").unwrap();
            match derives_for_type(&ty).unwrap() {
                DerivesSource::Direct(set) => assert_eq!(
                    set,
                    DerivesSet {
                        hash: false,
                        ..DerivesSet::all()
                    }
                ),
                DerivesSource::TypeAlias(t) => panic!("expected Direct, got TypeAlias({t:?})"),
            }
        }

        #[test]
        fn derives_for_type_core_ffi_path_gets_all() {
            let ty: Type = parse_str("::core::ffi::c_int").unwrap();
            assert_direct_all(derives_for_type(&ty).unwrap());
        }

        #[test]
        fn derives_for_type_primitives_derive_all() {
            for name in PRIMITIVES_DERIVE_ALL {
                let ty: Type = parse_str(name).unwrap();
                match derives_for_type(&ty).unwrap() {
                    DerivesSource::Direct(set) => assert_eq!(set, DerivesSet::all(), "{name}"),
                    DerivesSource::TypeAlias(t) => {
                        panic!("{name}: expected Direct, got TypeAlias({t:?})")
                    }
                }
            }
        }

        #[test]
        fn derives_for_type_float_primitives_derive_all_except_hash() {
            let expected = DerivesSet {
                hash: false,
                ..DerivesSet::all()
            };
            for name in PRIMITIVES_DERIVE_ALL_EXCEPT_HASH {
                let ty: Type = parse_str(name).unwrap();
                match derives_for_type(&ty).unwrap() {
                    DerivesSource::Direct(set) => assert_eq!(set, expected, "{name}"),
                    DerivesSource::TypeAlias(t) => {
                        panic!("{name}: expected Direct, got TypeAlias({t:?})")
                    }
                }
            }
        }

        #[test]
        fn derives_for_type_str_derives_all_except_copy_and_default() {
            let expected = DerivesSet {
                copy: false,
                default: false,
                ..DerivesSet::all()
            };
            for name in PRIMITIVES_DERIVE_ALL_EXCEPT_COPY_AND_DEFAULT {
                let ty: Type = parse_str(name).unwrap();
                match derives_for_type(&ty).unwrap() {
                    DerivesSource::Direct(set) => assert_eq!(set, expected, "{name}"),
                    DerivesSource::TypeAlias(t) => {
                        panic!("{name}: expected Direct, got TypeAlias({t:?})")
                    }
                }
            }
        }

        /// Every Rust primitive name.
        const ALL_PRIMITIVES: &[&str] = &[
            "bool", "char", "str", "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32",
            "u64", "u128", "usize", "f16", "f32", "f64", "f128",
        ];

        #[test]
        fn primitive_lists_cover_every_primitive_exactly_once() {
            let mut union: Vec<&str> = PRIMITIVES_DERIVE_ALL
                .iter()
                .chain(PRIMITIVES_DERIVE_ALL_EXCEPT_HASH)
                .chain(PRIMITIVES_DERIVE_ALL_EXCEPT_COPY_AND_DEFAULT)
                .copied()
                .collect();
            union.sort_unstable();
            let mut all = ALL_PRIMITIVES.to_vec();
            all.sort_unstable();
            assert_eq!(union, all);
        }

        #[test]
        fn derives_for_type_option_fn_gets_all() {
            let ty: Type =
                parse_str("::core::option::Option<unsafe extern \"C\" fn() -> u32>").unwrap();
            assert_direct_all(derives_for_type(&ty).unwrap());
        }

        #[test]
        fn derives_for_type_named_type_alias_returns_type_alias_source() {
            let ty: Type = parse_str("SomeAlias").unwrap();
            assert_type_alias(derives_for_type(&ty).unwrap(), "SomeAlias");
        }

        #[test]
        fn derives_for_type_module_qualified_alias_keeps_full_path() {
            // A module-qualified target keeps every segment, matching the
            // compound key registered for the module's inner type instead of
            // truncating to the last segment.
            let ty: Type = parse_str("_INTERFACE_TYPE::Type").unwrap();
            assert_type_alias(derives_for_type(&ty).unwrap(), "_INTERFACE_TYPE::Type");
        }

        #[test]
        fn derives_for_type_path_with_unsupported_generics_errors() {
            // Vec<u8> is not the Option<fn> shape, so the `PathArguments::None`
            // check fires and surfaces UnsupportedSynNode.
            let ty: Type = parse_str("Vec<u8>").unwrap();
            match derives_for_type(&ty).unwrap_err() {
                DerivesError::UnsupportedSynNode { .. } => {}
                other => panic!("expected UnsupportedSynNode, got {other:?}"),
            }
        }

        #[test]
        fn derives_for_type_unsupported_variant_errors() {
            let ty: Type = parse_str("(u32, u64)").unwrap();
            assert!(matches!(
                derives_for_type(&ty),
                Err(DerivesError::UnsupportedSynNode { .. })
            ));

            let ty: Type = parse_str("&u32").unwrap();
            assert!(matches!(
                derives_for_type(&ty),
                Err(DerivesError::UnsupportedSynNode { .. })
            ));

            let ty: Type = parse_str("dyn Send").unwrap();
            assert!(matches!(
                derives_for_type(&ty),
                Err(DerivesError::UnsupportedSynNode { .. })
            ));
        }

        #[test]
        fn ident_and_derives_for_use_self_path_rename() {
            let item: ItemUse = parse_str("pub use self::_FOO::Type as FOO;").unwrap();
            let (key, source) = ident_and_derives_for_use(&item).unwrap();
            assert_eq!(key, "FOO");
            assert_type_alias(source, "_FOO::Type");
        }

        #[test]
        fn ident_and_derives_for_use_no_self_segment() {
            let item: ItemUse = parse_str("pub use _FOO::Type as FOO;").unwrap();
            let (key, source) = ident_and_derives_for_use(&item).unwrap();
            assert_eq!(key, "FOO");
            assert_type_alias(source, "_FOO::Type");
        }

        #[test]
        fn ident_and_derives_for_use_glob_errors() {
            let item: ItemUse = parse_str("pub use foo::*;").unwrap();
            assert!(matches!(
                ident_and_derives_for_use(&item),
                Err(DerivesError::UnsupportedSynNode { .. })
            ));
        }

        #[test]
        fn ident_and_derives_for_use_no_rename_errors() {
            let item: ItemUse = parse_str("pub use foo::Bar;").unwrap();
            assert!(matches!(
                ident_and_derives_for_use(&item),
                Err(DerivesError::UnsupportedSynNode { .. })
            ));
        }

        #[test]
        fn ident_and_derives_for_use_group_errors() {
            let item: ItemUse = parse_str("pub use foo::{Bar, Baz};").unwrap();
            assert!(matches!(
                ident_and_derives_for_use(&item),
                Err(DerivesError::UnsupportedSynNode { .. })
            ));
        }

        #[test]
        fn idents_and_derives_for_mod_prefixes_inner_idents() {
            let m: syn::ItemMod =
                parse_str("pub mod _OUTER { pub type Type = ::core::ffi::c_int; }").unwrap();
            let mut result = idents_and_derives_for_mod(&m).unwrap();
            assert_eq!(result.len(), 1);
            let (key, source) = result.remove(0);
            assert_eq!(key, "_OUTER::Type");
            assert_direct_all(source);
        }

        #[test]
        fn idents_and_derives_for_mod_empty_content_returns_empty() {
            // External mod declaration (no inline body) — `m.content` is `None`.
            let m: syn::ItemMod = parse_str("pub mod foo;").unwrap();
            assert!(idents_and_derives_for_mod(&m).unwrap().is_empty());
        }

        #[test]
        fn unsupported_item_kind_errors() {
            // Item::Trait is not part of the supported Struct/Union/Enum/Type/Mod/
            // Use/Impl/Const set, so the catch-all arm fires.
            assert!(matches!(
                DerivesMap::from_source("pub trait T {}"),
                Err(DerivesError::UnsupportedSynNode { .. })
            ));
        }

        #[test]
        fn foreign_mod_errors() {
            // `extern "C" { ... }` parses as Item::ForeignMod, outside the supported
            // set, so the catch-all arm fires.
            let src = r#"
                extern "C" {
                    pub fn some_ffi_func();
                    pub static mut some_const: SOME_TYPE;
                }
            "#;
            assert!(matches!(
                DerivesMap::from_source(src),
                Err(DerivesError::UnsupportedSynNode { .. })
            ));
        }
    }

    mod type_alias_resolution {
        use super::*;

        #[test]
        fn resolve_type_aliases_chain_of_three_inherits_target_set() {
            // A → B → C, where C is the only recorded type.
            let mut map = DerivesMap::with_std_types();
            map.types.insert("C".into(), DerivesSet::all());
            let mut type_aliases = HashMap::new();
            type_aliases.insert("A".into(), "B".into());
            type_aliases.insert("B".into(), "C".into());
            map.resolve_type_aliases(&type_aliases).unwrap();
            assert_eq!(map.types.get("A"), Some(&DerivesSet::all()));
            assert_eq!(map.types.get("B"), Some(&DerivesSet::all()));
        }

        #[test]
        fn module_qualified_alias_resolves_to_inner_type() {
            let map = DerivesMap::from_source(
                r"
                pub mod _MOD {
                    pub type Type = ::core::ffi::c_int;
                }
                pub type Alias = _MOD::Type;
                ",
            )
            .expect("parses");
            for trait_ in [
                DeriveTrait::Copy,
                DeriveTrait::Debug,
                DeriveTrait::Default,
                DeriveTrait::Hash,
                DeriveTrait::PartialEqOrPartialOrd,
            ] {
                assert!(
                    map.satisfies("Alias", trait_),
                    "Alias should inherit {trait_:?} from _MOD::Type"
                );
            }
        }

        #[test]
        fn resolve_type_aliases_skips_already_recorded_keys() {
            let mut map = DerivesMap::with_std_types();
            map.types.insert(
                "A".into(),
                DerivesSet {
                    copy: true,
                    ..DerivesSet::default()
                },
            );
            let mut type_aliases = HashMap::new();
            // A is already recorded; the type alias entry must be skipped (no overwrite).
            type_aliases.insert("A".into(), "NeverResolved".into());
            map.resolve_type_aliases(&type_aliases).unwrap();
            assert_eq!(
                map.types.get("A"),
                Some(&DerivesSet {
                    copy: true,
                    ..DerivesSet::default()
                })
            );
        }

        #[test]
        fn resolve_type_aliases_empty_input_is_noop() {
            let mut map = DerivesMap::with_std_types();
            let snapshot = map.types.clone();
            map.resolve_type_aliases(&HashMap::new()).unwrap();
            assert_eq!(map.types, snapshot);
        }

        /// Every seeded stdint name derives the full standard set. Guards the
        /// hand-maintained `STDINT_NAMES` list against accidental deletion and
        /// keeps the `satisfies` result shape in sync with the seed.
        #[test]
        fn stdint_names_all_derive_standard_set() {
            let map = DerivesMap::from_source("").expect("parses");
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

        /// A cyclic type alias chain (`A = B; B = C; C = A;`) must surface as
        /// `TypeAliasCycle` — the chain-walking loop detects it when a step
        /// revisits a name already in the walked set.
        #[test]
        fn type_alias_cycle_terminates() {
            let src = r"
                pub type A = B;
                pub type B = C;
                pub type C = A;
            ";
            let err = DerivesMap::from_source(src).expect_err("cycle must error");
            match err {
                DerivesError::TypeAliasCycle { mut names } => {
                    names.sort();
                    assert_eq!(names, vec!["A".to_owned(), "B".to_owned(), "C".to_owned()]);
                }
                other => panic!("expected TypeAliasCycle, got {other:?}"),
            }
        }

        /// A type alias whose target is neither a recorded type nor another
        /// pending type alias must surface as `UnresolvedTypeAlias`.
        #[test]
        fn unresolvable_type_alias_errors() {
            let src = r"
                pub type UnknownAlias = SomeUnparsedType;
            ";
            let err = DerivesMap::from_source(src).expect_err("unresolvable must error");
            match err {
                DerivesError::UnresolvedTypeAlias { target } => {
                    assert_eq!(target, "SomeUnparsedType");
                }
                other => panic!("expected UnresolvedTypeAlias, got {other:?}"),
            }
        }
    }

    mod base_callback {
        use super::*;

        /// `BaseDerivesCallback` must translate `bool` into the bindgen
        /// `Some(Yes)` / `Some(No)` answers expected for blocklisted types.
        #[test]
        fn base_callback_known_positive_returns_yes() {
            let src = r"
                #[derive(Copy, Clone, Debug)]
                pub struct Pod;
            ";
            let map = Arc::new(DerivesMap::from_source(src).expect("parses"));
            let cb = BaseDerivesCallback::new(map);

            assert!(matches!(
                cb.blocklisted_type_implements_trait("Pod", DeriveTrait::Copy),
                Some(ImplementsTrait::Yes)
            ));

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
            let map = Arc::new(DerivesMap::from_source(src).expect("parses"));
            let cb = BaseDerivesCallback::new(map);
            assert!(matches!(
                cb.blocklisted_type_implements_trait("Pod", DeriveTrait::Debug),
                Some(ImplementsTrait::No)
            ));
        }

        #[test]
        fn base_callback_unknown_returns_no() {
            let map = Arc::new(DerivesMap::from_source("").expect("parses"));
            let cb = BaseDerivesCallback::new(map);
            assert!(matches!(
                cb.blocklisted_type_implements_trait("Nonexistent", DeriveTrait::Debug),
                Some(ImplementsTrait::No)
            ));
        }
    }
}
