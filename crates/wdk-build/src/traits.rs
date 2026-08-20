// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! Parser of `bindgen` output to compute the traits that emitted types
//! implement.

use std::{
    collections::HashMap,
    path::{Path as FsPath, PathBuf},
};

use bindgen::callbacks::{DeriveTrait, ImplementsTrait, ParseCallbacks};
use syn::{Attribute, Item, ItemImpl, ItemUse, Path, PathArguments, Type, UseTree};
use thiserror::Error;
use tracing::{Level, trace};

/// Primitives that implement every tracked trait.
const PRIMITIVES_DERIVE_ALL: &[&str] = &[
    "bool", "char", "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128",
    "usize",
];

/// Primitives that implement every tracked trait except `Hash`.
const PRIMITIVES_DERIVE_ALL_EXCEPT_HASH: &[&str] = &["f16", "f32", "f64", "f128"];

/// Primitives that implement every tracked trait except `Copy`.
const PRIMITIVES_DERIVE_ALL_EXCEPT_COPY: &[&str] = &["str"];

/// [`core::ffi`] types that implement every tracked trait.
const FFI_DERIVE_ALL: &[&str] = &[
    "c_char",
    "c_int",
    "c_long",
    "c_longlong",
    "c_ptrdiff_t",
    "c_schar",
    "c_short",
    "c_size_t",
    "c_ssize_t",
    "c_uchar",
    "c_uint",
    "c_ulong",
    "c_ulonglong",
    "c_ushort",
];

/// [`core::ffi`] types that implement every tracked trait except `Hash`.
const FFI_DERIVE_ALL_EXCEPT_HASH: &[&str] = &["c_double", "c_float"];

const FFI_DERIVE_ONLY_DEBUG: &[&str] = &["c_void"];

/// Errors returned when parsing a bindgen-emitted source file into a
/// [`TraitsMap`].
#[derive(Debug, Error)]
pub enum TraitsError {
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

    /// Encountered a `syn` AST node whose variant is not explicitly handled.
    #[error("unsupported node variant: {node}")]
    UnsupportedNodeVariant {
        /// Debug-formatted representation of the unsupported node.
        node: String,
    },

    /// Encountered a syn node whose shape is not explicitly handled.
    #[error("unsupported node shape: {reason}: {node}")]
    UnsupportedNodeShape {
        /// Why the node shape is unsupported.
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

    /// Traits were encountered not belonging to the set of traits that we
    /// track.
    #[error("traits not tracked: {trait_names:?}")]
    UntrackedTraits {
        /// The untracked trait name.
        trait_names: Vec<String>,
    },
}

/// The set of standard traits a bindgen-generated type implements.
///
/// Each field records whether the type implements the corresponding trait.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "type represents an independent set of flags, not a state machine"
)]
pub struct TraitsSet {
    copy: bool,
    debug: bool,
    default: bool,
    hash: bool,
    partial_eq_or_partial_ord: bool,
}

impl TraitsSet {
    const fn all() -> Self {
        Self {
            copy: true,
            debug: true,
            default: true,
            hash: true,
            partial_eq_or_partial_ord: true,
        }
    }

    const fn insert(&mut self, derive_trait: DeriveTrait) {
        match derive_trait {
            DeriveTrait::Copy => self.copy = true,
            DeriveTrait::Debug => self.debug = true,
            DeriveTrait::Default => self.default = true,
            DeriveTrait::Hash => self.hash = true,
            DeriveTrait::PartialEqOrPartialOrd => self.partial_eq_or_partial_ord = true,
        }
    }

    fn try_insert(&mut self, trait_name: &str) -> Result<(), TraitsError> {
        match trait_name {
            "Copy" => self.copy = true,
            "Debug" => self.debug = true,
            "Default" => self.default = true,
            "Hash" => self.hash = true,
            "PartialEq" | "PartialOrd" => self.partial_eq_or_partial_ord = true,
            _ => {
                return Err(TraitsError::UntrackedTraits {
                    trait_names: vec![trait_name.to_string()],
                });
            }
        }

        Ok(())
    }

    const fn remove(&mut self, derive_trait: DeriveTrait) -> bool {
        if !self.contains(derive_trait) {
            return false;
        }

        match derive_trait {
            DeriveTrait::Copy => self.copy = false,
            DeriveTrait::Debug => self.debug = false,
            DeriveTrait::Default => self.default = false,
            DeriveTrait::Hash => self.hash = false,
            DeriveTrait::PartialEqOrPartialOrd => self.partial_eq_or_partial_ord = false,
        }

        true
    }

    #[must_use]
    /// Returns `true` if the `TraitsSet` contains the given `DeriveTrait`.
    pub const fn contains(&self, derive_trait: DeriveTrait) -> bool {
        match derive_trait {
            DeriveTrait::Copy => self.copy,
            DeriveTrait::Debug => self.debug,
            DeriveTrait::Default => self.default,
            DeriveTrait::Hash => self.hash,
            DeriveTrait::PartialEqOrPartialOrd => self.partial_eq_or_partial_ord,
        }
    }

    const fn append(&mut self, other: Self) {
        self.copy |= other.copy;
        self.debug |= other.debug;
        self.default |= other.default;
        self.hash |= other.hash;
        self.partial_eq_or_partial_ord |= other.partial_eq_or_partial_ord;
    }
}

impl TryFrom<Vec<String>> for TraitsSet {
    type Error = TraitsError;

    /// Builds a `TraitsSet` from a list of implemented traits.
    ///
    /// Returns [`TraitsError::UntrackedTraits`] if all traits are not tracked.
    fn try_from(trait_names: Vec<String>) -> Result<Self, TraitsError> {
        let mut set = Self::default();

        let mut found_tracked_trait = false;
        for d in &trait_names {
            if set.try_insert(d).is_ok() {
                found_tracked_trait = true;
            }
        }

        if !found_tracked_trait {
            return Err(TraitsError::UntrackedTraits { trait_names });
        }

        Ok(set)
    }
}

impl TryFrom<String> for TraitsSet {
    type Error = TraitsError;

    /// Builds a `TraitsSet` from a single string containing the identity of an
    /// implemented trait.
    ///
    /// Returns [`TraitsError::UntrackedTraits`] if string does not correspond
    /// to a tracked trait.
    fn try_from(trait_str: String) -> Result<Self, TraitsError> {
        let mut set = Self::default();
        set.try_insert(&trait_str)?;
        Ok(set)
    }
}

#[derive(Debug, PartialEq)]
enum TraitsSource {
    Direct(TraitsSet),
    TypeAlias(String),
}

/// Map storing Rust source type names to the set of traits the type
/// implements.
#[derive(Debug)]
pub struct TraitsMap {
    types: HashMap<String, TraitsSet>,
}

impl ParseCallbacks for TraitsMap {
    fn blocklisted_type_implements_trait(
        &self,
        name: &str,
        derive_trait: DeriveTrait,
    ) -> Option<ImplementsTrait> {
        if !self.types.contains_key(name) {
            return None;
        }
        if self.types[name].contains(derive_trait) {
            return Some(ImplementsTrait::Yes);
        }
        Some(ImplementsTrait::No)
    }
}

impl TraitsMap {
    /// Reads a Rust source file from disk and parses the traits each type
    /// implements.
    ///
    /// # Errors
    ///
    /// Returns:
    /// - [`TraitsError::Io`] if the file cannot be read
    /// - [`TraitsError::Parse`] if the file does not contain valid Rust
    /// - [`TraitsError::UnsupportedNodeVariant`] or
    ///   [`TraitsError::UnsupportedNodeShape`] if a classified construct does
    ///   not match any recognized bindgen output shape
    /// - [`TraitsError::UnresolvedTypeAlias`] or
    ///   [`TraitsError::TypeAliasCycle`] if a type alias cannot be resolved to
    ///   a recorded type
    #[tracing::instrument(level = "debug")]
    pub fn from_file(path: &FsPath) -> Result<Self, TraitsError> {
        let source = std::fs::read_to_string(path).map_err(|source| TraitsError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_source(&source)
    }

    /// Reads a Rust source string and parses the traits each type implements.
    ///
    /// This does not support bindgen output containing function declarations
    /// (`CodegenConfig::FUNCTIONS`).
    ///
    /// # Errors
    ///
    /// Returns:
    /// - [`TraitsError::Parse`] if `source` is not valid Rust
    /// - [`TraitsError::UnsupportedSynNodeVariant`] or
    ///   [`TraitsError::UnsupportedNodeShape`] if a classified construct does
    ///   not match any recognized bindgen output shape
    /// - [`TraitsError::UnresolvedTypeAlias`] or
    ///   [`TraitsError::TypeAliasCycle`] if a type alias cannot be resolved to
    ///   a recorded type
    #[tracing::instrument(level = "trace", err(level = Level::TRACE ))]
    fn from_source(source: &str) -> Result<Self, TraitsError> {
        let file = syn::parse_str::<syn::File>(source).map_err(TraitsError::Parse)?;

        let mut traits_map = Self {
            types: HashMap::default(),
        };

        let mut type_aliases: HashMap<String, String> = HashMap::default();
        for (key, source) in extract_idents_and_traits_from_items(&file.items)? {
            match source {
                TraitsSource::Direct(traits_set) => {
                    if let Some(value) = traits_map.types.get_mut(&key) {
                        value.append(traits_set);
                    } else {
                        traits_map.types.insert(key, traits_set);
                    }
                }
                TraitsSource::TypeAlias(aliased_to) => {
                    type_aliases.insert(key, aliased_to);
                }
            }
        }

        traits_map.resolve_type_aliases(&type_aliases)?;

        Ok(traits_map)
    }

    /// Resolve every type alias in `type_aliases` by walking its chain to a
    /// recorded type and copying that type's `TraitsSet` onto each type
    /// alias along the way.
    ///
    /// # Errors
    ///
    /// Returns:
    /// - [`TraitsError::UnresolvedTypeAlias`] if a chain terminates at a name
    ///   that is neither a recorded type nor a queued type alias
    /// - [`TraitsError::TypeAliasCycle`] if a chain revisits a name it has
    ///   already walked through
    #[tracing::instrument(level = "trace", err(level = "trace"))]
    fn resolve_type_aliases(
        &mut self,
        type_aliases: &HashMap<String, String>,
    ) -> Result<(), TraitsError> {
        for key in type_aliases.keys() {
            if self.types.contains_key(key) {
                continue;
            }

            let mut curr = key;
            let mut walked = vec![curr];
            while !self.types.contains_key(curr) {
                let Some(next) = type_aliases.get(curr) else {
                    return Err(TraitsError::UnresolvedTypeAlias {
                        target: curr.clone(),
                    });
                };
                if walked.contains(&next) {
                    return Err(TraitsError::TypeAliasCycle {
                        names: walked.into_iter().cloned().collect(),
                    });
                }
                walked.push(next);
                curr = next;
            }

            let target_traits_set = *self
                .types
                .get(curr)
                .expect("self.types should contain the key curr");

            for new_traits_key in walked {
                self.types.insert(new_traits_key.clone(), target_traits_set);
            }
        }

        Ok(())
    }
}

/// Classify the type-defining [`syn::Item`]s in `items`, returning their
/// type names and [`TraitsSource`]s.
///
/// Each `Item` has the potential to return any number of [`TraitsSource`]s
/// depending on its variant. There is no consolidation for type names; a type
/// may appear multiple times in the returned `Vec`.
///
/// If a type's implemented traits can be inferred from the `Item`, those traits
/// are added via [`TraitsSource::Direct`]. Otherwise, a type's implemented
/// traits must be reliant on another type, which is represented with
/// `[TraitsSource::TypeAlias]`.
///
/// # Bindgen shapes
///
/// Struct / Union / Enum -- traits come from the `#[derive(...)]` attrs:
///
/// ```ignore
/// #[repr(C)]
/// #[derive(Debug, Copy, Clone)]
/// pub struct _UNICODE_STRING {
///     pub Length: USHORT,
///     pub MaximumLength: USHORT,
///     pub Buffer: PWCH,
/// }
///
/// #[repr(C)]
/// #[derive(Copy, Clone)]
/// pub union _FILE_SEGMENT_ELEMENT {
///     pub Buffer: *mut ::core::ffi::c_void,
///     pub Alignment: ULONGLONG,
/// }
/// ```
///
/// Type definitions -- traits come from `extract_traits_from_type`, which can
/// either be inferred if a base type is given or stored as a type alias for
/// later resolution:
/// ```ignore
/// pub type CHAR = ::core::ffi::c_char; // type definition comes from base type
///
/// pub type LPCH = *mut CHAR; // type definition comes from pointer
///
/// // type alias comes from function pointer
/// pub type EX_CALLBACK_FUNCTION = ::core::option::Option<
///     unsafe extern "C" fn(
///         CallbackContext: PVOID,
///         Argument1: PVOID,
///         Argument2: PVOID,
///     ) -> NTSTATUS,
/// >;
///
/// pub type PTCH = LPCH; // type definition comes from other defined type, stored as `TraitsSource::TypeAlias`
/// ```
///
/// Module / Use -- `bindgen`'s representation of C-style enums. Defines a
/// discriminant type inside a `mod` alongside variants, and exports that type
/// via a `use` statement.
///
/// ```ignore
/// pub mod _DEVICE_POWER_STATE {
///     pub type Type = ::core::ffi::c_int;
///     pub const PowerDeviceUnspecified: Type = 0;
///     pub const PowerDeviceD0: Type = 1;
///     pub const PowerDeviceD1: Type = 2;
///     pub const PowerDeviceD2: Type = 3;
///     pub const PowerDeviceD3: Type = 4;
///     pub const PowerDeviceMaximum: Type = 5;
/// }
/// pub use self::_DEVICE_POWER_STATE::Type as DEVICE_POWER_STATE;
/// ```
///
/// Impl -- `bindgen`'s manual implementation of individual traits. This is
/// added as a separate key/pair value from the initial definition.
///
/// ```ignore
/// #[repr(C)]
/// #[derive(Copy, Clone)]
/// pub union _LARGE_INTEGER { /* definition hidden */ }
///
/// impl Default for _LARGE_INTEGER {
///     fn default() -> Self {
///         let mut s = ::core::mem::MaybeUninit::<Self>::uninit();
///         unsafe {
///             ::core::ptr::write_bytes(s.as_mut_ptr(), 0, 1);
///             s.assume_init()
///         }
///     }
/// }
/// ```
///
/// # Errors
///
/// Returns:
/// - [`TraitsError::UnsupportedNodeVariant`] for `Item` variants other than
///   Struct/Union/Enum/Type/Mod/Use/Impl/Const
/// - any error propagated from the per-shape classifiers
#[tracing::instrument(level = "trace", skip(items), err(level = "trace"))]
fn extract_idents_and_traits_from_items(
    items: &[Item],
) -> Result<Vec<(String, TraitsSource)>, TraitsError> {
    let mut traits: Vec<(String, TraitsSource)> = vec![];

    for item in items {
        trace!(?item);
        match item {
            Item::Struct(s) => {
                trace!("Item recognized as a struct");
                traits.push((
                    s.ident.to_string(),
                    extract_derived_traits_from_attrs(&s.attrs),
                ));
            }
            Item::Union(u) => {
                trace!("Item recognized as a union");
                traits.push((
                    u.ident.to_string(),
                    extract_derived_traits_from_attrs(&u.attrs),
                ));
            }
            Item::Enum(e) => {
                trace!("Item recognized as an enum");
                traits.push((
                    e.ident.to_string(),
                    extract_derived_traits_from_attrs(&e.attrs),
                ));
            }
            Item::Type(t) => {
                trace!("Item recognized as a type");
                traits.push((t.ident.to_string(), extract_traits_from_type(&t.ty)?));
            }
            Item::Mod(m) => {
                trace!("Item recognized as a mod");
                traits.extend(extract_idents_and_traits_from_mod(m)?);
            }
            Item::Use(u) => {
                trace!("Item recognized as a use");
                traits.push(extract_ident_and_traits_from_use(u)?);
            }
            Item::Impl(i) => {
                trace!("Item recognized as an impl");
                if let Some(key_trait_pair) = parse_impl_for_ident_and_trait(i) {
                    traits.push(key_trait_pair);
                }
            }
            Item::Const(_) => {
                trace!("No traits can be extracted from item.");
            }
            other => {
                return Err(TraitsError::UnsupportedNodeVariant {
                    node: format!("{other:?}"),
                });
            }
        }
    }
    Ok(traits)
}

/// Collects the trait names from `#[derive]` attributes.
///
/// If no tracked traits are detected, returns the default `TraitsSet`;
#[tracing::instrument(level = "trace", ret)]
fn extract_derived_traits_from_attrs(attrs: &[Attribute]) -> TraitsSource {
    let derives_vec: Vec<String> = attrs
        .iter()
        // gather only attributes that are have ident "derive"
        .filter(|attr| attr.path().is_ident("derive"))
        // apply a comma separated parser to each attribute
        .filter_map(|attr| {
            attr.parse_args_with(
                syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
            )
            .ok()
        })
        // flatten into a single iterator of Paths
        .flatten()
        // take last segment of every Path and convert to String
        .filter_map(|path| {
            path.segments
                .into_iter()
                .next_back()
                .map(|seg| seg.ident.to_string())
        })
        .collect();

    let Ok(traits_set) = TraitsSet::try_from(derives_vec) else {
        trace!("Derive attributes contain no tracked traits");
        return TraitsSource::Direct(TraitsSet::default());
    };

    TraitsSource::Direct(traits_set)
}

/// Classifies a [`syn::Type`] into the [`TraitsSource`] it represents.
///
/// # Bindgen shapes
///
/// Pointer types implement `Copy`, `Debug`, `Default`, `Hash`, and
/// `PartialEq/PartialOrd`. See [primitive pointer documentation](https://doc.rust-lang.org/core/primitive.pointer.html).
/// ```ignore
/// pub type LPCH = *mut CHAR;
/// ```
///
/// Array types implement `Copy`, `Debug`, `Hash`, and `PartialOrd`
/// only if their element type implements the trait. Types implement `Default`
/// if the element type implements the trait AND if the array is at or under
/// length 32.  See [primitive array documentation](https://doc.rust-lang.org/core/primitive.array.html)
/// ```ignore
/// pub type __C_ASSERT__ = [::core::ffi::c_char; 1usize];
/// ```
///
/// Function types implement `Copy`, `Debug`, `Hash`, and
/// `PartialEq/PartialOrd`. See [primitive fn documentation](https://doc.rust-lang.org/core/primitive.fn.html#trait-implementations-1)
///
/// `Option` implements `Copy`, `Debug`, `Hash`, and `PartialEq/PartialOrd` only
/// if the payload type implements the trait. Additionally, `Option`
/// unconditionally implements `Default`. See [`core::option::Option` documentation](https://doc.rust-lang.org/core/option/enum.Option.html)
///
/// The interaction between these two types means that an `Option` of an `unsafe
/// extern "C" fn` implements all tracked traits.
/// ```ignore
/// // type alias comes from function pointer
/// pub type EX_CALLBACK_FUNCTION = ::core::option::Option< // Type::Path (function pointer in `Option`)
///     unsafe extern "C" fn(
///         CallbackContext: PVOID,
///         Argument1: PVOID,
///         Argument2: PVOID,
///     ) -> NTSTATUS,
/// >;
/// ```
///
/// `core::ffi` types are mostly type aliases to Rust primitives. The traits
/// each implements is encoded in constant `&str` arrays.
/// ```ignore
/// pub type CHAR = ::core::ffi::c_char;                    // Type::Path (ffi)
/// ```
///
/// Primitive types implement different traits depending on the type. The traits
/// each implements is encoded in constant `&str` arrays.
/// ```ignore
/// pub type rsize_t = usize;                               // Type::Path (primitive)
/// ```
///
/// Type aliases are detected and stored for later resolution.
/// ```ignore
/// pub type PTCH = LPCH;                                   // Type::Path (alias)
/// ```
///
/// # Errors
///
/// Returns:
/// - [`TraitsError::UnsupportedNodeVariant`] if `ty` is a `syn::Type` variant
///   other than Ptr/Path/Array, or if the path has generic arguments
/// - [`TraitsError::UnsupportedNodeShape`] if the node is not recognized by any
///   classifiers, or if the node is an untracked [`core::ffi`] type.
#[tracing::instrument(level = "trace", ret, err(level = "trace"))]
fn extract_traits_from_type(ty: &Type) -> Result<TraitsSource, TraitsError> {
    match ty {
        Type::Ptr(_) => {
            trace!("Type recognized as a `Ptr`");
            Ok(TraitsSource::Direct(TraitsSet::all()))
        }
        Type::Array(arr) => {
            trace!("Type recognized as a `Array`");
            let inner_traits = extract_traits_from_type(&arr.elem)?;

            let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Int(len_expr),
                ..
            }) = &arr.len
            else {
                return Err(TraitsError::UnsupportedNodeVariant {
                    node: format!("{arr:?}"),
                });
            };

            let Ok(len) = len_expr.base10_parse::<u32>() else {
                return Err(TraitsError::UnsupportedNodeShape {
                    reason: "array length is unparsable".to_string(),
                    node: format!("{arr:?}"),
                });
            };

            if len <= 32 {
                return Ok(inner_traits);
            }

            // If TraitsSource is Alias, we do not know the current type's traits. However,
            // because the length of the array is above 32 we know we need to
            // remove Default if it is implemented by the base type.
            // This case not currently supported by this library.
            let TraitsSource::Direct(mut traits) = inner_traits else {
                return Err(TraitsError::UnsupportedNodeShape {
                    reason: "arrays whose element type is an alias and length is greater than 32 \
                             is not supported"
                        .to_string(),
                    node: format!("{arr:?}"),
                });
            };

            traits.remove(DeriveTrait::Default);
            Ok(TraitsSource::Direct(traits))
        }
        Type::Path(tp) => {
            trace!("Type recognized as a `TypePath`");
            if parse_path_for_option_type(&tp.path).is_some_and(type_is_unsafe_extern_c_fn) {
                return Ok(TraitsSource::Direct(TraitsSet::all()));
            }

            if let Some(ffi_traits) = parse_path_for_ffi_traits(&tp.path) {
                return Ok(TraitsSource::Direct(ffi_traits?));
            }

            if let Some(primitive_traits) = parse_path_for_primitive_traits(&tp.path) {
                return Ok(TraitsSource::Direct(primitive_traits));
            }

            if let Some(type_alias) = parse_path_for_type_alias(&tp.path) {
                return Ok(TraitsSource::TypeAlias(type_alias));
            }

            Err(TraitsError::UnsupportedNodeShape {
                reason: "TypePath is not a function pointer, core ffi type, primitive, or \
                         parsable type alias"
                    .to_string(),
                node: format!("{tp:?}"),
            })
        }
        other => Err(TraitsError::UnsupportedNodeVariant {
            node: format!("{other:?}"),
        }),
    }
}

/// Classifies the type-defining items inside a [`syn::ItemMod`], prepending
/// their type names with the `mod`'s prefix.
///
/// # Bindgen shapes
///
/// ```ignore
/// pub mod _DEVICE_POWER_STATE {
///     pub type Type = ::core::ffi::c_int;
///     pub const PowerDeviceUnspecified: Type = 0;
///     pub const PowerDeviceD0: Type = 1;
///     pub const PowerDeviceD1: Type = 2;
///     pub const PowerDeviceD2: Type = 3;
///     pub const PowerDeviceD3: Type = 4;
///     pub const PowerDeviceMaximum: Type = 5;
/// }
/// ```
///
/// # Errors
///
/// Returns any error propagated from [`extract_idents_and_traits_from_items`]
/// on the module's inner items.
#[tracing::instrument(level = "trace", ret, err(level = "trace"))]
fn extract_idents_and_traits_from_mod(
    m: &syn::ItemMod,
) -> Result<Vec<(String, TraitsSource)>, TraitsError> {
    let Some((_, mod_items)) = &m.content else {
        trace!("Mod has no content");
        return Ok(vec![]);
    };
    let prefix = format!("{}::", m.ident);

    let mut mod_items_traits = extract_idents_and_traits_from_items(mod_items)?;

    for (key, _) in &mut mod_items_traits {
        key.insert_str(0, &prefix);
    }
    Ok(mod_items_traits)
}

/// Classifies a [`syn::ItemUse`], returning the renamed type as a
/// [`TraitsSource::TypeAlias`].
///
/// # Bindgen shapes
///
/// ```ignore
/// pub use self::_DEVICE_POWER_STATE::Type as DEVICE_POWER_STATE;
/// ```
///
/// # Errors
///
/// Returns:
/// - [`TraitsError::UnsupportedNodeVariant`] if the `UseTree` does not contain
///   any number of `UseTree::Path`s followed by a `UseTree::Rename`
#[tracing::instrument(level = "trace", ret, err(level = "trace"))]
fn extract_ident_and_traits_from_use(
    item_use: &ItemUse,
) -> Result<(String, TraitsSource), TraitsError> {
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
        return Err(TraitsError::UnsupportedNodeVariant {
            node: format!("{use_tree:?}"),
        });
    };

    segments.push(use_rename.ident.to_string());
    Ok((
        use_rename.rename.to_string(),
        TraitsSource::TypeAlias(segments.join("::")),
    ))
}

/// Parses a [`syn::ItemImpl`] for a manual implementation of a trait.
///
/// Returns `None` if a manual implementation of a tracked trait is not found.
///
/// # Bindgen shapes
///
/// ```ignore
/// impl Default for _LARGE_INTEGER {
///     fn default() -> Self {
///         let mut s = ::core::mem::MaybeUninit::<Self>::uninit();
///         unsafe {
///             ::core::ptr::write_bytes(s.as_mut_ptr(), 0, 1);
///             s.assume_init()
///         }
///     }
/// }
/// ```
#[tracing::instrument(level = "trace", ret)]
fn parse_impl_for_ident_and_trait(item_impl: &ItemImpl) -> Option<(String, TraitsSource)> {
    let Some((_not_token, trait_path, _for_token)) = &item_impl.trait_ else {
        trace!("Impl did not implement a trait.");
        return None;
    };

    let Some(trait_ident) = trait_path.get_ident() else {
        trace!("Impl trait path is not a single ident");
        return None;
    };

    let Type::Path(type_path) = &*item_impl.self_ty else {
        trace!("Impl ident is not a path");
        return None;
    };

    let Some(type_ident) = type_path.path.get_ident() else {
        trace!("Impl ident is not a single ident");
        return None;
    };

    let Ok(trait_set) = TraitsSet::try_from(trait_ident.to_string()) else {
        trace!("Impl trait is not a tracked trait");
        return None;
    };

    trace!("Impl is recognized as implementing a tracked trait");
    Some((type_ident.to_string(), TraitsSource::Direct(trait_set)))
}

/// Parses a [`syn::Path`] for the type within a `core::option::Option`.
///
/// Returns `None` if the `Path` is not recognized as a `core::option::Option`.
#[tracing::instrument(level = "trace", ret)]
fn parse_path_for_option_type(path: &Path) -> Option<&Type> {
    // check if path is core::option::Option
    if path.segments.len() != 3 {
        trace!("Path does not have 3 segments");
        return None;
    }

    let mut segs = path.segments.iter();
    let core = segs.next()?;
    let option_mod = segs.next()?;
    let option = segs.next()?;

    if core.ident != "core" || option_mod.ident != "option" || option.ident != "Option" {
        trace!("Path does not equal `core::option::Option`");
        return None;
    }

    let PathArguments::AngleBracketed(angle_args) = &option.arguments else {
        trace!("Path does not have angle bracketed arguments");
        return None;
    };
    if angle_args.args.len() != 1 {
        trace!("Angle bracketed arguments length is not equal to 1");
        return None;
    }

    let Some(syn::GenericArgument::Type(ty)) = angle_args.args.first() else {
        trace!("Angle bracketed argument is not a Type");
        return None;
    };

    trace!("Path is recognized as a `core::option::Option` with a valid type");
    Some(ty)
}

/// Parses a [`syn::Path`] for the traits associated with a `core::ffi`
/// primitive.
///
/// Returns `None` if the `Path` is not recognized as a `core::ffi` primitive.
///
/// Returns a `TraitsError::UnsupportedNodeShape` in the inner `Result` if the
/// type is an untracked `core::ffi` primitive.
#[tracing::instrument(level = "trace", ret)]
fn parse_path_for_ffi_traits(path: &Path) -> Option<Result<TraitsSet, TraitsError>> {
    let segs = &path.segments;

    if segs.len() != 3 {
        trace!("Path does not have 3 segments");
        return None;
    }

    if segs[0].ident != "core" || segs[1].ident != "ffi" {
        trace!("Path does not match the pattern `core::ffi::<type>");
        return None;
    }

    trace!("Path is recognized as a core::ffi type");

    if FFI_DERIVE_ALL.iter().any(|s| segs[2].ident == s) {
        trace!("Path is recognized as a primitive in the `FFI_DERIVE_ALL` array");
        return Some(Ok(TraitsSet::all()));
    }

    if FFI_DERIVE_ALL_EXCEPT_HASH
        .iter()
        .any(|s| segs[2].ident == s)
    {
        trace!("Path is recognized as a primitive in the `FFI_DERIVE_ALL_EXCEPT_HASH` array");
        let mut set = TraitsSet::all();
        set.remove(DeriveTrait::Hash);
        return Some(Ok(set));
    }

    if FFI_DERIVE_ONLY_DEBUG.iter().any(|s| segs[2].ident == s) {
        trace!("Path is recognized as a primitive in the `FFI_DERIVE_ONLY_DEBUG` array");
        let mut set = TraitsSet::default();
        set.insert(DeriveTrait::Debug);
        return Some(Ok(set));
    }

    Some(Err(TraitsError::UnsupportedNodeShape {
        reason: "Type belongs to `core::ffi` but is not an explicitly handled type".to_string(),
        node: format!("{path:?}"),
    }))
}

/// Parses a [`syn::Path`] for the traits a primitive implements.
///
/// Returns `None` if the `Path` is not recognized as a Rust primitive.
#[tracing::instrument(level = "trace", ret)]
fn parse_path_for_primitive_traits(path: &Path) -> Option<TraitsSet> {
    let segs = &path.segments;

    if segs.len() != 1 {
        trace!("Path does not have 1 segment");
        return None;
    }

    if PRIMITIVES_DERIVE_ALL.iter().any(|s| segs[0].ident == s) {
        trace!("Path is recognized as a primitive in the `PRIMITIVES_DERIVE_ALL` array");
        return Some(TraitsSet::all());
    }

    if PRIMITIVES_DERIVE_ALL_EXCEPT_HASH
        .iter()
        .any(|s| segs[0].ident == s)
    {
        trace!(
            "Path is recognized as a primitive in the `PRIMITIVES_DERIVE_ALL_EXCEPT_HASH` array"
        );
        let mut set = TraitsSet::all();
        set.remove(DeriveTrait::Hash);
        return Some(set);
    }

    if PRIMITIVES_DERIVE_ALL_EXCEPT_COPY
        .iter()
        .any(|s| segs[0].ident == s)
    {
        trace!(
            "Path is recognized as a primitive in the `PRIMITIVES_DERIVE_ALL_EXCEPT_COPY` array"
        );
        let mut set = TraitsSet::all();
        set.remove(DeriveTrait::Copy);

        return Some(set);
    }

    trace!("Path is not recognized as a primitive");
    None
}

/// Parses a [`syn::Path`] for a type alias.
///
/// Returns `None` if any segment of the alias contains a bracketed or
/// parenthesized path.
#[tracing::instrument(level = "trace", ret)]
fn parse_path_for_type_alias(path: &Path) -> Option<String> {
    if path
        .segments
        .iter()
        .any(|segment| !matches!(segment.arguments, PathArguments::None))
    {
        trace!("Path contains a segment with bracketed or parenthesized path arguments");
        return None;
    }

    let qualified_name = path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::");

    trace!("Path is recognized as a type alias: {qualified_name}");
    Some(qualified_name)
}

/// Checks whether the given [`syn::Type`] is an `unsafe extern "C"` function.
#[tracing::instrument(level = "trace", ret)]
fn type_is_unsafe_extern_c_fn(ty: &Type) -> bool {
    // check if it is a function
    let Type::BareFn(bare_fn) = ty else {
        trace!("Type is not a BareFn");
        return false;
    };

    // check if the function is marked as unsafe
    let Some(_) = bare_fn.unsafety else {
        trace!("BareFn type is not marked as unsafe");
        return false;
    };

    // check if the function has extern "C" specified
    let Some(bare_fn_abi) = &bare_fn.abi else {
        trace!("BareFn type does not have an ABI");
        return false;
    };
    let Some(bare_fn_abi_extern_name) = &bare_fn_abi.name else {
        trace!("BareFn ABI does not have a name");
        return false;
    };
    if bare_fn_abi_extern_name.value() != "C" {
        trace!("BareFn ABI does not specify \"C\"");
        return false;
    }

    trace!("Type is recognized as an `unsafe extern \"C\"` function");
    true
}

#[cfg(test)]
mod tests {
    use syn::{Path, parse_str};

    use super::*;

    #[track_caller]
    fn assert_direct_all(source: TraitsSource) {
        match source {
            TraitsSource::Direct(set) => assert_eq!(set, TraitsSet::all()),
            TraitsSource::TypeAlias(name) => {
                panic!("expected Direct(all), got TypeAlias({name:?})")
            }
        }
    }

    #[track_caller]
    fn assert_type_alias(source: TraitsSource, expected: &str) {
        match source {
            TraitsSource::TypeAlias(s) => assert_eq!(s, expected),
            TraitsSource::Direct(set) => {
                panic!("expected TypeAlias({expected:?}), got Direct({set:?})")
            }
        }
    }

    mod parse_impl_for_ident_and_trait {
        use super::*;

        #[test]
        fn rejects_no_trait() {
            let i: ItemImpl = parse_str("impl NoTrait {}").unwrap();

            assert!(parse_impl_for_ident_and_trait(&i).is_none());
        }

        #[test]
        fn rejects_trait_multi_ident_path() {
            let i: ItemImpl = parse_str("impl long::path::Trait for Type {}").unwrap();

            assert!(parse_impl_for_ident_and_trait(&i).is_none());
        }

        #[test]
        fn rejects_type_ident_not_path() {
            let i: ItemImpl = parse_str("impl Trait for *mut PointerType {}").unwrap();

            assert!(parse_impl_for_ident_and_trait(&i).is_none());
        }

        #[test]
        fn rejects_type_ident_multi_ident_path() {
            let i: ItemImpl = parse_str("impl Trait for *mut long::path::Type {}").unwrap();

            assert!(parse_impl_for_ident_and_trait(&i).is_none());
        }

        #[test]
        fn rejects_non_tracked_trait() {
            let i: ItemImpl = parse_str("impl Clone for Type {}").unwrap();

            assert!(parse_impl_for_ident_and_trait(&i).is_none());
        }

        #[test]
        fn accepts_well_formed_ident() {
            let i: ItemImpl = parse_str("impl Default for Type {}").unwrap();

            let (ident, trait_set) = parse_impl_for_ident_and_trait(&i).expect("should be Some");
            assert_eq!(ident, "Type");
            assert_eq!(
                trait_set,
                TraitsSource::Direct(TraitsSet {
                    default: true,
                    ..TraitsSet::default()
                })
            );
        }
    }

    mod parse_path_for_option_type {
        use super::*;

        #[test]
        fn recognizes_full_path() {
            let p: Path = parse_str("::core::option::Option<T>").unwrap();
            assert!(parse_path_for_option_type(&p).is_some());
            let p: Path = parse_str("core::option::Option<T>").unwrap();
            assert!(parse_path_for_option_type(&p).is_some());
        }

        #[test]
        fn rejects_short_paths() {
            let p: Path = parse_str("Option<T>").unwrap();
            assert!(parse_path_for_option_type(&p).is_none());
            let p: Path = parse_str("option::Option<T>").unwrap();
            assert!(parse_path_for_option_type(&p).is_none());
        }

        #[test]
        fn rejects_wrong_paths() {
            let p: Path = parse_str("std::option::Option<T>").unwrap();
            assert!(parse_path_for_option_type(&p).is_none());
            let p: Path = parse_str("core::wrong_path::Option<T>").unwrap();
            assert!(parse_path_for_option_type(&p).is_none());
            let p: Path = parse_str("core::option::c_void<T>").unwrap();
            assert!(parse_path_for_option_type(&p).is_none());
        }

        #[test]
        fn rejects_no_brackets() {
            let p: Path = parse_str("core::option::Option").unwrap();
            assert!(parse_path_for_option_type(&p).is_none());
        }

        #[test]
        fn rejects_multiple_types() {
            let p: Path = parse_str("core::option::Option<T, U>").unwrap();
            assert!(parse_path_for_option_type(&p).is_none());
        }

        #[test]
        fn rejects_lifetime_as_argument() {
            let p: Path = parse_str("core::option::Option<\'a>").unwrap();
            assert!(parse_path_for_option_type(&p).is_none());
        }

        #[test]
        fn rejects_no_type() {
            let p: Path = parse_str("core::option::Option<>").unwrap();
            assert!(parse_path_for_option_type(&p).is_none());
        }

        #[test]
        fn returns_type_correctly() {
            let p: Path = parse_str("core::option::Option<TypeName>").unwrap();
            let ty = parse_path_for_option_type(&p).unwrap();

            let Type::Path(type_path) = ty else {
                panic!("expected Type::Path");
            };
            assert!(type_path.path.is_ident("TypeName"));
        }
    }

    mod parse_path_for_primitive_traits {
        use super::*;

        #[test]
        fn rejects_non_primitive() {
            assert!(
                parse_path_for_primitive_traits(&parse_str("core::ffi::c_int").unwrap()).is_none()
            );
            assert!(
                parse_path_for_primitive_traits(&parse_str("NonPrimitiveType").unwrap()).is_none()
            );
        }

        #[test]
        fn correct_traits_for_all() {
            let traits = parse_path_for_primitive_traits(&parse_str("u16").unwrap());
            assert!(traits.is_some());

            let generated_traits = traits.unwrap();

            assert_eq!(generated_traits, TraitsSet::all());
        }

        #[test]
        fn correct_traits_for_all_except_hash() {
            let traits = parse_path_for_primitive_traits(&parse_str("f16").unwrap());
            assert!(traits.is_some());

            let generated_traits = traits.unwrap();

            assert_eq!(
                generated_traits,
                TraitsSet {
                    hash: false,
                    ..TraitsSet::all()
                }
            );
        }

        #[test]
        fn correct_traits_for_all_except_copy() {
            let traits = parse_path_for_primitive_traits(&parse_str("str").unwrap());
            assert!(traits.is_some());

            let generated_traits = traits.unwrap();

            assert_eq!(
                generated_traits,
                TraitsSet {
                    copy: false,
                    ..TraitsSet::all()
                }
            );
        }
    }

    mod parse_path_for_type_alias {
        use super::*;

        #[test]
        fn rejects_generics_in_path() {
            assert!(
                parse_path_for_type_alias(&parse_str("std::option::Option<T>").unwrap()).is_none()
            );
            assert!(
                parse_path_for_type_alias(&parse_str("some::generic<In>::Middle").unwrap())
                    .is_none()
            );
            assert!(
                parse_path_for_type_alias(&parse_str("some<Generic>::at::Start").unwrap())
                    .is_none()
            );
        }

        #[test]
        fn parses_types() {
            let path = parse_path_for_type_alias(&parse_str("std::option::Option").unwrap());
            assert!(path.is_some());
            assert_eq!(path.unwrap(), "std::option::Option");

            let alias = parse_path_for_type_alias(&parse_str("TypeName").unwrap());
            assert!(alias.is_some());
            assert_eq!(alias.unwrap(), "TypeName");
        }
    }

    mod parse_path_for_ffi_traits {
        use super::*;

        #[test]
        fn recognizes_ffi_types_derive_all() {
            let p: Path = parse_str("::core::ffi::c_char").unwrap();
            let ffi = parse_path_for_ffi_traits(&p)
                .expect("parser should find ffi primitive")
                .expect("ffi type should be recognized");
            assert_eq!(ffi, TraitsSet::all());

            let p: Path = parse_str("core::ffi::c_int").unwrap();
            let ffi = parse_path_for_ffi_traits(&p)
                .expect("parser should find ffi primitive")
                .expect("ffi type should be recognized");
            assert_eq!(ffi, TraitsSet::all());

            let p: Path = parse_str("::core::ffi::c_uint").unwrap();
            let ffi = parse_path_for_ffi_traits(&p)
                .expect("parser should find ffi primitive")
                .expect("ffi type should be recognized");
            assert_eq!(ffi, TraitsSet::all());
        }

        #[test]
        fn recognizes_ffi_types_derive_all_except_hash() {
            let p: Path = parse_str("::core::ffi::c_float").unwrap();
            let ffi = parse_path_for_ffi_traits(&p)
                .expect("parser should find ffi primitive")
                .expect("ffi type should be recognized");
            assert_eq!(
                ffi,
                TraitsSet {
                    hash: false,
                    ..TraitsSet::all()
                }
            );

            let p: Path = parse_str("core::ffi::c_double").unwrap();
            let ffi = parse_path_for_ffi_traits(&p)
                .expect("parser should find ffi primitive")
                .expect("ffi type should be recognized");
            assert_eq!(
                ffi,
                TraitsSet {
                    hash: false,
                    ..TraitsSet::all()
                }
            );
        }

        #[test]
        fn recognizes_ffi_type_derive_only_debug() {
            let p: Path = parse_str("::core::ffi::c_void").unwrap();
            let ffi = parse_path_for_ffi_traits(&p)
                .expect("parser should find ffi primitive")
                .expect("ffi type should be recognized");
            assert_eq!(
                ffi,
                TraitsSet {
                    debug: true,
                    ..TraitsSet::default()
                }
            );
        }
        #[test]
        fn error_for_ffi_types_not_recognized() {
            let p: Path = parse_str("::core::ffi::c_str").unwrap();
            let ffi = parse_path_for_ffi_traits(&p).expect("parser should find ffi primitive");
            assert!(ffi.is_err());

            let err = ffi.err().unwrap();
            assert!(matches!(err, TraitsError::UnsupportedNodeShape { .. }));
        }

        #[test]
        fn rejects_non_core_ffi_types() {
            let p: Path = parse_str("core::option::Option").unwrap();
            assert!(parse_path_for_ffi_traits(&p).is_none());
            let p: Path = parse_str("std::ffi::CStr").unwrap();
            assert!(parse_path_for_ffi_traits(&p).is_none());
            let p: Path = parse_str("c_int").unwrap();
            assert!(parse_path_for_ffi_traits(&p).is_none());
        }
    }

    mod type_is_unsafe_extern_c_fn {
        use super::*;

        #[test]
        fn accepts_unsafe_extern_c_fn() {
            assert!(type_is_unsafe_extern_c_fn(
                &parse_str("unsafe extern \"C\" fn()").unwrap()
            ));
            assert!(type_is_unsafe_extern_c_fn(
                &parse_str("unsafe extern \"C\" fn() -> bool").unwrap()
            ));
            assert!(type_is_unsafe_extern_c_fn(
                &parse_str("unsafe extern \"C\" fn(Type) -> bool").unwrap()
            ));
            assert!(type_is_unsafe_extern_c_fn(
                &parse_str("unsafe extern \"C\" fn(Type1, Type2) -> bool").unwrap()
            ));
        }

        #[test]
        fn rejects_non_bare_fn() {
            assert!(!type_is_unsafe_extern_c_fn(&parse_str("bool").unwrap()));
        }

        #[test]
        fn rejects_incorrect_abi() {
            assert!(!type_is_unsafe_extern_c_fn(
                &parse_str("unsafe fn(Type1, Type2) -> bool").unwrap()
            ));
            assert!(!type_is_unsafe_extern_c_fn(
                &parse_str("unsafe extern fn() -> bool").unwrap()
            ));
            assert!(!type_is_unsafe_extern_c_fn(
                &parse_str("unsafe extern \"system\" fn() -> bool").unwrap()
            ));
        }

        #[test]
        fn rejects_safe_fn() {
            assert!(!type_is_unsafe_extern_c_fn(
                &parse_str("fn() -> bool").unwrap()
            ));
        }
    }

    mod extract_derived_traits_from_attrs {
        use super::*;

        #[test]
        fn extracts_idents() {
            let item: syn::ItemStruct =
                parse_str("#[derive(Copy, Clone, Debug)] pub struct S;").unwrap();
            let derives = extract_derived_traits_from_attrs(&item.attrs);
            assert_eq!(
                derives,
                TraitsSource::Direct(TraitsSet {
                    copy: true,
                    debug: true,
                    ..TraitsSet::default()
                })
            );
        }

        #[test]
        fn ignores_non_derive_attrs() {
            let item: syn::ItemStruct =
                parse_str("#[repr(C)] #[derive(Copy)] #[allow(dead_code)] pub struct S;").unwrap();
            let derives = extract_derived_traits_from_attrs(&item.attrs);
            assert_eq!(
                derives,
                TraitsSource::Direct(TraitsSet {
                    copy: true,
                    ..TraitsSet::default()
                })
            );
        }

        #[test]
        fn uses_last_path_segment() {
            let item: syn::ItemStruct =
                parse_str("#[derive(::core::marker::Copy)] pub struct S;").unwrap();
            let derives = extract_derived_traits_from_attrs(&item.attrs);
            assert_eq!(
                derives,
                TraitsSource::Direct(TraitsSet {
                    copy: true,
                    ..TraitsSet::default()
                })
            );
        }

        #[test]
        fn no_derives_returns_default() {
            let item: syn::ItemStruct = parse_str("#[repr(C)] pub struct S;").unwrap();
            assert_eq!(
                extract_derived_traits_from_attrs(&item.attrs),
                TraitsSource::Direct(TraitsSet::default())
            );
        }

        #[test]
        fn irrelevant_derives_returns_default() {
            let item: syn::ItemStruct =
                parse_str("#[derive(Clone, Eq, Ord)] pub struct S;").unwrap();
            assert_eq!(
                extract_derived_traits_from_attrs(&item.attrs),
                TraitsSource::Direct(TraitsSet::default())
            );
        }
    }

    mod extract_traits_from_type {
        use super::*;

        #[test]
        fn pointer_gets_all() {
            let ty: Type = parse_str("*mut u32").unwrap();
            assert_direct_all(extract_traits_from_type(&ty).unwrap());
            let ty: Type = parse_str("*const ::core::ffi::c_void").unwrap();
            assert_direct_all(extract_traits_from_type(&ty).unwrap());
        }

        #[test]
        fn small_array_recurses_into_element() {
            let ty: Type = parse_str("[*mut u32; 8]").unwrap();
            assert_direct_all(extract_traits_from_type(&ty).unwrap());

            let ty: Type =
                parse_str("[core::option::Option<unsafe extern \"C\" fn()>; 8]").unwrap();
            assert_direct_all(extract_traits_from_type(&ty).unwrap());

            let ty: Type = parse_str("[u32; 4]").unwrap();
            assert_direct_all(extract_traits_from_type(&ty).unwrap());

            let ty: Type = parse_str("[f32; 8]").unwrap();
            match extract_traits_from_type(&ty).unwrap() {
                TraitsSource::Direct(set) => assert_eq!(
                    set,
                    TraitsSet {
                        hash: false,
                        ..TraitsSet::all()
                    }
                ),
                TraitsSource::TypeAlias(t) => panic!("expected Direct, got TypeAlias({t:?})"),
            }

            let ty: Type = parse_str("[SomeAlias; 8]").unwrap();
            assert_type_alias(extract_traits_from_type(&ty).unwrap(), "SomeAlias");

            let ty: Type = parse_str("[[u32; 8]; 8]").unwrap();
            assert_direct_all(extract_traits_from_type(&ty).unwrap());
        }

        #[test]
        fn large_array_recurses_into_element_and_drops_default() {
            let ty: Type = parse_str("[*mut u32; 33]").unwrap();
            match extract_traits_from_type(&ty).unwrap() {
                TraitsSource::Direct(set) => assert_eq!(
                    set,
                    TraitsSet {
                        default: false,
                        ..TraitsSet::all()
                    }
                ),
                TraitsSource::TypeAlias(t) => panic!("expected Direct, got TypeAlias({t:?})"),
            }
        }

        #[test]
        fn core_ffi_path_gets_all() {
            let ty: Type = parse_str("::core::ffi::c_int").unwrap();
            assert_direct_all(extract_traits_from_type(&ty).unwrap());
        }

        #[test]
        fn primitives_implement_all() {
            for name in PRIMITIVES_DERIVE_ALL {
                let ty: Type = parse_str(name).unwrap();
                match extract_traits_from_type(&ty).unwrap() {
                    TraitsSource::Direct(set) => assert_eq!(set, TraitsSet::all(), "{name}"),
                    TraitsSource::TypeAlias(t) => {
                        panic!("{name}: expected Direct, got TypeAlias({t:?})")
                    }
                }
            }
        }

        #[test]
        fn float_primitives_implement_all_except_hash() {
            let expected = TraitsSet {
                hash: false,
                ..TraitsSet::all()
            };
            for name in PRIMITIVES_DERIVE_ALL_EXCEPT_HASH {
                let ty: Type = parse_str(name).unwrap();
                match extract_traits_from_type(&ty).unwrap() {
                    TraitsSource::Direct(set) => assert_eq!(set, expected, "{name}"),
                    TraitsSource::TypeAlias(t) => {
                        panic!("{name}: expected Direct, got TypeAlias({t:?})")
                    }
                }
            }
        }

        #[test]
        fn str_implements_all_except_copy_and_default() {
            let expected = TraitsSet {
                copy: false,
                ..TraitsSet::all()
            };
            for name in PRIMITIVES_DERIVE_ALL_EXCEPT_COPY {
                let ty: Type = parse_str(name).unwrap();
                match extract_traits_from_type(&ty).unwrap() {
                    TraitsSource::Direct(set) => assert_eq!(set, expected, "{name}"),
                    TraitsSource::TypeAlias(t) => {
                        panic!("{name}: expected Direct, got TypeAlias({t:?})")
                    }
                }
            }
        }

        #[test]
        fn option_fn_gets_all() {
            let ty: Type =
                parse_str("::core::option::Option<unsafe extern \"C\" fn() -> u32>").unwrap();
            assert_direct_all(extract_traits_from_type(&ty).unwrap());
        }

        #[test]
        fn named_type_alias_returns_type_alias_source() {
            let ty: Type = parse_str("SomeAlias").unwrap();
            assert_type_alias(extract_traits_from_type(&ty).unwrap(), "SomeAlias");
        }

        #[test]
        fn module_qualified_alias_keeps_full_path() {
            // A module-qualified target keeps every segment, matching the
            // compound key registered for the module's inner type instead of
            // truncating to the last segment.
            let ty: Type = parse_str("_INTERFACE_TYPE::Type").unwrap();
            assert_type_alias(
                extract_traits_from_type(&ty).unwrap(),
                "_INTERFACE_TYPE::Type",
            );
        }

        #[test]
        fn path_with_unsupported_generics_errors() {
            let ty: Type = parse_str("Vec<u8>").unwrap();
            match extract_traits_from_type(&ty).unwrap_err() {
                TraitsError::UnsupportedNodeShape { .. } => {}
                other => panic!("expected UnsupportedNodeVariant, got {other:?}"),
            }
        }

        #[test]
        fn unsupported_variants_error() {
            let ty: Type = parse_str("(u32, u64)").unwrap();
            assert!(matches!(
                extract_traits_from_type(&ty),
                Err(TraitsError::UnsupportedNodeVariant { .. })
            ));

            let ty: Type = parse_str("&u32").unwrap();
            assert!(matches!(
                extract_traits_from_type(&ty),
                Err(TraitsError::UnsupportedNodeVariant { .. })
            ));

            let ty: Type = parse_str("dyn Send").unwrap();
            assert!(matches!(
                extract_traits_from_type(&ty),
                Err(TraitsError::UnsupportedNodeVariant { .. })
            ));
        }
    }

    mod extract_ident_and_traits_from_use {
        use super::*;

        #[test]
        fn self_path_rename_succeeds_and_removes_self() {
            let item: ItemUse = parse_str("pub use self::_FOO::Type as BAR;").unwrap();
            let (key, source) = extract_ident_and_traits_from_use(&item).unwrap();
            assert_eq!(key, "BAR");
            assert_type_alias(source, "_FOO::Type");
        }

        #[test]
        fn no_self_segment_rename_succeeds() {
            let item: ItemUse = parse_str("pub use _FOO::Type as BAR;").unwrap();
            let (key, source) = extract_ident_and_traits_from_use(&item).unwrap();
            assert_eq!(key, "BAR");
            assert_type_alias(source, "_FOO::Type");
        }

        #[test]
        fn glob_errors() {
            let item: ItemUse = parse_str("pub use foo::*;").unwrap();
            assert!(matches!(
                extract_ident_and_traits_from_use(&item),
                Err(TraitsError::UnsupportedNodeVariant { .. })
            ));
        }

        #[test]
        fn no_rename_errors() {
            let item: ItemUse = parse_str("pub use foo::Bar;").unwrap();
            assert!(matches!(
                extract_ident_and_traits_from_use(&item),
                Err(TraitsError::UnsupportedNodeVariant { .. })
            ));
        }

        #[test]
        fn no_rename_group_errors() {
            let item: ItemUse = parse_str("pub use foo::{Bar, Baz};").unwrap();
            assert!(matches!(
                extract_ident_and_traits_from_use(&item),
                Err(TraitsError::UnsupportedNodeVariant { .. })
            ));
        }
    }

    mod extract_idents_and_traits_from_mod {
        use super::*;

        #[test]
        fn prefixes_inner_idents() {
            let m: syn::ItemMod =
                parse_str("pub mod _OUTER { pub type Type = ::core::ffi::c_int; }").unwrap();
            let mut result = extract_idents_and_traits_from_mod(&m).unwrap();
            assert_eq!(result.len(), 1);
            let (key, source) = result.remove(0);
            assert_eq!(key, "_OUTER::Type");
            assert_direct_all(source);
        }

        #[test]
        fn empty_content_returns_empty() {
            // External mod declaration (no inline body) — `m.content` is `None`.
            let m: syn::ItemMod = parse_str("pub mod foo;").unwrap();
            assert_eq!(
                extract_idents_and_traits_from_mod(&m).unwrap(),
                [] as [(String, TraitsSource); 0]
            );
        }
    }

    mod extract_idents_and_traits_from_items {
        use super::*;

        #[track_caller]
        fn extract(source: &str) -> Result<Vec<(String, TraitsSource)>, TraitsError> {
            let file: syn::File = parse_str(source).expect("test source should parse");
            extract_idents_and_traits_from_items(&file.items)
        }

        #[track_caller]
        fn extract_single(source: &str) -> (String, TraitsSource) {
            let mut output = extract(source).expect("item extraction should succeed");
            assert_eq!(output.len(), 1);
            output.pop().expect("output should contain one item")
        }

        #[track_caller]
        fn assert_direct_copy_only(source: TraitsSource) {
            match source {
                TraitsSource::Direct(set) => assert_eq!(
                    set,
                    TraitsSet {
                        copy: true,
                        ..TraitsSet::default()
                    }
                ),
                TraitsSource::TypeAlias(name) => {
                    panic!("expected Direct(copy), got TypeAlias({name:?})")
                }
            }
        }

        #[test]
        fn struct_returns_ident_and_direct_traits() {
            let (ident, source) =
                extract_single("#[derive(Copy, Debug, Default, Hash, PartialEq)] pub struct S;");

            assert_eq!(ident, "S");
            assert_direct_all(source);
        }

        #[test]
        fn union_returns_ident_and_direct_traits() {
            let (ident, source) = extract_single("#[derive(Copy)] pub union U { pub value: u32 }");

            assert_eq!(ident, "U");
            assert_direct_copy_only(source);
        }

        #[test]
        fn enum_returns_ident_and_direct_traits() {
            let (ident, source) =
                extract_single("#[derive(Copy, Debug, Default, Hash, PartialEq)] pub enum E { A }");

            assert_eq!(ident, "E");
            assert_direct_all(source);
        }

        #[test]
        fn type_returns_ident_and_type_alias() {
            let (ident, source) = extract_single("pub type Alias = Target;");

            assert_eq!(ident, "Alias");
            assert_type_alias(source, "Target");
        }

        #[test]
        fn mod_returns_prefixed_inner_items() {
            let (ident, source) = extract_single("pub mod _MOD { pub type Type = u32; }");

            assert_eq!(ident, "_MOD::Type");
            assert_direct_all(source);
        }

        #[test]
        fn use_returns_renamed_type_alias() {
            let (ident, source) = extract_single("pub use self::_MOD::Type as PublicType;");

            assert_eq!(ident, "PublicType");
            assert_type_alias(source, "_MOD::Type");
        }

        #[test]
        fn impl_returns_ident_and_implemented_trait() {
            let (ident, source) = extract_single("impl Copy for I {}");

            assert_eq!(ident, "I");
            assert_direct_copy_only(source);
        }

        #[test]
        fn const_is_ignored() {
            assert_eq!(
                extract("pub const VALUE: u32 = 1;").unwrap(),
                [] as [(String, TraitsSource); 0]
            );
        }

        #[test]
        fn all_supported_item_types_are_processed() {
            let output = extract(
                r"
                #[derive(Copy, Debug, Default, Hash, PartialEq)]
                pub struct S;

                #[derive(Copy)]
                pub union U {
                    pub value: u32,
                }

                #[derive(Copy, Debug, Default, Hash, PartialEq)]
                pub enum E {
                    A,
                }

                pub type Alias = S;

                pub mod _MOD {
                    pub type Type = u32;
                }

                pub use self::_MOD::Type as PublicType;

                impl Copy for I {}

                pub const VALUE: u32 = 1;
                ",
            )
            .unwrap();

            let mut output = output.into_iter();

            let (ident, source) = output.next().unwrap();
            assert_eq!(ident, "S");
            assert_direct_all(source);

            let (ident, source) = output.next().unwrap();
            assert_eq!(ident, "U");
            assert_direct_copy_only(source);

            let (ident, source) = output.next().unwrap();
            assert_eq!(ident, "E");
            assert_direct_all(source);

            let (ident, source) = output.next().unwrap();
            assert_eq!(ident, "Alias");
            assert_type_alias(source, "S");

            let (ident, source) = output.next().unwrap();
            assert_eq!(ident, "_MOD::Type");
            assert_direct_all(source);

            let (ident, source) = output.next().unwrap();
            assert_eq!(ident, "PublicType");
            assert_type_alias(source, "_MOD::Type");

            let (ident, source) = output.next().unwrap();
            assert_eq!(ident, "I");
            assert_direct_copy_only(source);

            assert!(output.next().is_none());
        }

        #[test]
        fn unsupported_item_returns_error() {
            assert!(matches!(
                extract("pub trait Unsupported {}"),
                Err(TraitsError::UnsupportedNodeVariant { .. })
            ));
        }

        #[test]
        fn unsupported_item_with_supported_items_returns_error() {
            assert!(matches!(
                extract(
                    r"
                    #[derive(Copy, Debug, Default, Hash, PartialEq)]
                    pub struct S;

                    #[derive(Copy)]
                    pub union U {
                        pub value: u32,
                    }

                    #[derive(Copy, Debug, Default, Hash, PartialEq)]
                    pub enum E {
                        A,
                    }

                    pub type Alias = S;

                    pub mod _MOD {
                        pub type Type = u32;
                    }

                    pub use self::_MOD::Type as PublicType;

                    impl Copy for I {}

                    pub const VALUE: u32 = 1;

                    pub trait Unsupported {}
                    ",
                ),
                Err(TraitsError::UnsupportedNodeVariant { .. })
            ));
        }
    }

    mod constants_and_types {
        use super::*;

        const ALL_PRIMITIVES: &[&str] = &[
            "bool", "char", "str", "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32",
            "u64", "u128", "usize", "f16", "f32", "f64", "f128",
        ];

        const ALL_FFI: &[&str] = &[
            "c_void",
            "c_char",
            "c_double",
            "c_float",
            "c_int",
            "c_long",
            "c_longlong",
            "c_ptrdiff_t",
            "c_schar",
            "c_short",
            "c_size_t",
            "c_ssize_t",
            "c_uchar",
            "c_uint",
            "c_ulong",
            "c_ulonglong",
            "c_ushort",
        ];

        #[test]
        fn primitive_lists_cover_every_primitive_exactly_once() {
            let mut union: Vec<&str> = PRIMITIVES_DERIVE_ALL
                .iter()
                .chain(PRIMITIVES_DERIVE_ALL_EXCEPT_HASH)
                .chain(PRIMITIVES_DERIVE_ALL_EXCEPT_COPY)
                .copied()
                .collect();
            union.sort_unstable();
            let mut all = ALL_PRIMITIVES.to_vec();
            all.sort_unstable();
            assert_eq!(union, all);
        }

        #[test]
        fn ffi_lists_cover_every_primitive_exactly_once() {
            let mut union: Vec<&str> = FFI_DERIVE_ALL
                .iter()
                .chain(FFI_DERIVE_ALL_EXCEPT_HASH)
                .chain(FFI_DERIVE_ONLY_DEBUG)
                .copied()
                .collect();
            union.sort_unstable();
            let mut all = ALL_FFI.to_vec();
            all.sort_unstable();
            assert_eq!(union, all);
        }

        #[test]
        fn correct_const_array_membership_for_primitives() {
            let all: &[&str] = &[
                "bool", "char", "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32",
                "u64", "u128", "usize",
            ];

            let all_except_hash: &[&str] = &["f16", "f32", "f64", "f128"];

            let all_except_copy_and_default: &[&str] = &["str"];

            for s in all {
                assert!(PRIMITIVES_DERIVE_ALL.contains(s));
            }
            for s in all_except_hash {
                assert!(PRIMITIVES_DERIVE_ALL_EXCEPT_HASH.contains(s));
            }
            for s in all_except_copy_and_default {
                assert!(PRIMITIVES_DERIVE_ALL_EXCEPT_COPY.contains(s));
            }
        }

        #[test]
        fn correct_const_array_membership_for_ffi() {
            let all: &[&str] = &[
                "c_char",
                "c_int",
                "c_long",
                "c_longlong",
                "c_ptrdiff_t",
                "c_schar",
                "c_short",
                "c_size_t",
                "c_ssize_t",
                "c_uchar",
                "c_uint",
                "c_ulong",
                "c_ulonglong",
                "c_ushort",
            ];

            let all_except_hash: &[&str] = &["c_float", "c_double"];

            let debug: &[&str] = &["c_void"];

            for s in all {
                assert!(FFI_DERIVE_ALL.contains(s));
            }
            for s in all_except_hash {
                assert!(FFI_DERIVE_ALL_EXCEPT_HASH.contains(s));
            }
            for s in debug {
                assert!(FFI_DERIVE_ONLY_DEBUG.contains(s));
            }
        }
    }

    mod traits_set_from {
        use super::*;

        #[test]
        fn maps_every_tracked_trait_name() {
            let names = vec![
                "Copy".to_string(),
                "Debug".to_string(),
                "Default".to_string(),
                "Hash".to_string(),
                "PartialEq".to_string(),
                "PartialOrd".to_string(),
            ];
            let set = TraitsSet::try_from(names).unwrap();
            for derive_trait in [
                DeriveTrait::Copy,
                DeriveTrait::Debug,
                DeriveTrait::Default,
                DeriveTrait::Hash,
                DeriveTrait::PartialEqOrPartialOrd,
            ] {
                assert!(set.contains(derive_trait), "{derive_trait:?}");
            }
        }
        #[test]
        fn errors_for_irrelevant_traits_vec() {
            let untracked = TraitsSet::try_from(vec![
                "Clone".to_string(),
                "Eq".to_string(),
                "Ord".to_string(),
            ]);
            assert!(untracked.is_err());
            assert!(matches!(
                untracked.err().unwrap(),
                TraitsError::UntrackedTraits { .. }
            ));
        }

        #[test]
        fn errors_for_irrelevant_trait() {
            let clone_set = TraitsSet::try_from("Clone".to_string());
            assert!(clone_set.is_err());
            assert!(matches!(
                clone_set.err().unwrap(),
                TraitsError::UntrackedTraits { .. }
            ));
        }

        #[test]
        fn no_error_for_irrelevant_and_relevant_combination() {
            let traits = vec!["Clone".to_string(), "Copy".to_string()];
            let set = TraitsSet::try_from(traits).expect("try_from should return Ok");

            assert_eq!(
                set,
                TraitsSet {
                    copy: true,
                    ..TraitsSet::default()
                }
            );
        }

        #[test]
        fn partial_eq_or_partial_ord_triggers_partial_eq_or_partial_ord() {
            let only_partial_eq_or_partial_ord = TraitsSet {
                partial_eq_or_partial_ord: true,
                ..TraitsSet::default()
            };

            assert_eq!(
                TraitsSet::try_from("PartialEq".to_string()).unwrap(),
                only_partial_eq_or_partial_ord
            );

            assert_eq!(
                TraitsSet::try_from("PartialOrd".to_string()).unwrap(),
                only_partial_eq_or_partial_ord
            );

            let partial_eq_and_partial_ord =
                vec!["PartialEq".to_string(), "PartialOrd".to_string()];
            assert_eq!(
                TraitsSet::try_from(partial_eq_and_partial_ord).unwrap(),
                only_partial_eq_or_partial_ord
            );
        }
    }

    mod resolve_type_aliases {
        use super::*;

        #[test]
        fn chain_of_three_inherits_target_set() {
            let mut map = TraitsMap {
                types: HashMap::default(),
            };
            map.types.insert("C".into(), TraitsSet::all());
            let mut type_aliases = HashMap::new();
            type_aliases.insert("A".into(), "B".into());
            type_aliases.insert("B".into(), "C".into());
            map.resolve_type_aliases(&type_aliases).unwrap();
            assert_eq!(map.types.get("A"), Some(&TraitsSet::all()));
            assert_eq!(map.types.get("B"), Some(&TraitsSet::all()));
        }

        #[test]
        fn resolve_type_aliases_skips_already_recorded_keys() {
            let mut map = TraitsMap {
                types: HashMap::default(),
            };
            let traits = TraitsSet {
                copy: true,
                ..TraitsSet::default()
            };
            map.types.insert("A".into(), traits);
            let mut type_aliases = HashMap::new();
            // A is already recorded; the type alias entry must be skipped (no overwrite).
            type_aliases.insert("A".into(), "NeverResolved".into());
            map.resolve_type_aliases(&type_aliases).unwrap();
            assert_eq!(map.types.get("A").unwrap().to_owned(), traits);
            assert!(!map.types.contains_key("NeverResolved"));
        }

        #[test]
        fn empty_input_is_noop() {
            let mut map = TraitsMap {
                types: HashMap::default(),
            };
            let snapshot = map.types.clone();
            map.resolve_type_aliases(&HashMap::new()).unwrap();
            assert_eq!(map.types, snapshot);
        }

        /// A cyclic type alias chain (`A = B; B = C; C = A;`) must surface as
        /// `TypeAliasCycle` — the chain-walking loop detects it when a step
        /// revisits a name already in the walked set.
        #[test]
        fn type_alias_cycle_errors() {
            let mut map = TraitsMap {
                types: HashMap::default(),
            };
            let mut type_aliases = HashMap::new();
            type_aliases.insert("A".into(), "B".into());
            type_aliases.insert("B".into(), "C".into());
            type_aliases.insert("C".into(), "A".into());
            let err = map
                .resolve_type_aliases(&type_aliases)
                .expect_err("cycle must error");
            match err {
                TraitsError::TypeAliasCycle { mut names } => {
                    names.sort();
                    assert_eq!(
                        names,
                        vec!["A".to_string(), "B".to_string(), "C".to_string()]
                    );
                }
                other => panic!("expected TypeAliasCycle, got {other:?}"),
            }
        }

        /// A type alias whose target is neither a recorded type nor another
        /// pending type alias must surface as `UnresolvedTypeAlias`.
        #[test]
        fn unresolvable_type_alias_errors() {
            let mut map = TraitsMap {
                types: HashMap::default(),
            };
            let mut type_aliases = HashMap::new();
            type_aliases.insert("A".into(), "B".into());
            let err = map
                .resolve_type_aliases(&type_aliases)
                .expect_err("unresolvable must error");
            match err {
                TraitsError::UnresolvedTypeAlias { target } => {
                    assert_eq!(target, "B");
                }
                other => panic!("expected UnresolvedTypeAlias, got {other:?}"),
            }
        }
    }

    mod from_source {
        use super::*;

        #[test]
        fn module_qualified_alias_resolves_to_inner_type() {
            let map: TraitsMap = TraitsMap::from_source(
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
                    map.types["Alias"].contains(trait_),
                    "Alias should inherit {trait_:?} from _MOD::Type"
                );
            }
        }
    }

    mod base_callback {
        use super::*;

        /// `BaseTraitsCallback` must translate `bool` into the bindgen
        /// `Some(Yes)` / `Some(No)` answers expected for blocklisted types.
        #[test]
        fn known_positive_returns_yes() {
            let src = r"
                #[derive(Copy, Clone, Debug)]
                pub struct Pod;
            ";
            let map = TraitsMap::from_source(src).expect("parses");

            assert!(matches!(
                map.blocklisted_type_implements_trait("Pod", DeriveTrait::Copy),
                Some(ImplementsTrait::Yes)
            ));

            assert!(matches!(
                map.blocklisted_type_implements_trait("Pod", DeriveTrait::Debug),
                Some(ImplementsTrait::Yes)
            ));

            assert!(matches!(
                map.blocklisted_type_implements_trait("Pod", DeriveTrait::Hash),
                Some(ImplementsTrait::No)
            ));
        }

        #[test]
        fn known_negative_returns_no() {
            let src = r"
                #[derive(Copy, Clone)]
                pub struct Pod;
            ";
            let map = TraitsMap::from_source(src).expect("parses");
            assert!(matches!(
                map.blocklisted_type_implements_trait("Pod", DeriveTrait::Debug),
                Some(ImplementsTrait::No)
            ));
        }

        #[test]
        fn unknown_key_returns_none() {
            let map = TraitsMap::from_source("").expect("parses");
            assert_eq!(
                map.blocklisted_type_implements_trait("Nonexistent", DeriveTrait::Debug),
                None
            );
        }
    }
}
