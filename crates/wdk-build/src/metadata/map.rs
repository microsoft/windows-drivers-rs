// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

use std::{
    collections::{BTreeMap, HashMap, btree_map, hash_map},
    hash::{BuildHasher, Hash},
};

/// Trait for map-like type that is returned by
/// [`metadata::to_map`](crate::metadata::to_map)
/// and [`metadata::to_map_with_prefix`](crate::metadata::to_map_with_prefix).
pub trait Map<K, V>: Default {
    /// Creates a new, empty map
    #[must_use]
    fn new() -> Self {
        Self::default()
    }

    /// Inserts a new key-value pair into the map, or calls a function/closure
    /// if the key already exists.
    ///
    /// The function/closure is called with the existing key, the existing
    /// value, and the new value it tried to insert. The closure can decide
    /// whether the function will return an `Err` or if it will still return a
    /// `Ok` despite not inserting the value.
    ///
    /// # Errors
    /// This function returns an error if the key already exists and `f` returns
    /// an `Err` value
    fn insert_or_else<F, E>(&mut self, key: K, value: V, f: F) -> Result<(), E>
    where
        F: FnMut(&K, &V, V) -> Result<(), E>;
}

impl<K: Eq + Hash, V, S: BuildHasher + Default> Map<K, V> for HashMap<K, V, S> {
    fn insert_or_else<F, E>(&mut self, key: K, value: V, mut f: F) -> Result<(), E>
    where
        F: FnMut(&K, &V, V) -> Result<(), E>,
    {
        match self.entry(key) {
            hash_map::Entry::Occupied(entry) => f(entry.key(), entry.get(), value),
            hash_map::Entry::Vacant(entry) => {
                entry.insert(value);
                Ok(())
            }
        }
    }
}

impl<K: Ord, V> Map<K, V> for BTreeMap<K, V> {
    fn insert_or_else<F, E>(&mut self, key: K, value: V, mut f: F) -> Result<(), E>
    where
        F: FnMut(&K, &V, V) -> Result<(), E>,
    {
        match self.entry(key) {
            btree_map::Entry::Occupied(entry) => f(entry.key(), entry.get(), value),
            btree_map::Entry::Vacant(entry) => {
                entry.insert(value);
                Ok(())
            }
        }
    }
}
