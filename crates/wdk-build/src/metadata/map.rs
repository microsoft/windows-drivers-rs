use std::{
    collections::{btree_map, hash_map, BTreeMap, HashMap},
    hash::Hash,
};

/// Trait for map-like type that is returned by [`crate::to_map`] and
/// [`crate::to_map_with_prefix`].
pub trait Map<K, V>: Default {
    /// Creates a new, empty map
    fn new() -> Self {
        Self::default()
    }

    fn insert_or_else<F, E>(&mut self, key: K, value: V, f: F) -> Result<(), E>
    where
        F: FnMut(&K, &V, V) -> Result<(), E>;
}

impl<K: Eq + Hash, V> Map<K, V> for HashMap<K, V> {
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
