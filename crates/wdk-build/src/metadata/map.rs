use std::{borrow::Borrow, collections::HashMap, fmt, hash::Hash, ops::Index};

pub struct Map<K, V>(MapType<K, V>);
/// Map-like type that is used by [`Serializer`].
///
/// Currently this is a `HashMap`, but its subject to change (ex. to
/// `BTreeMap``).
type MapType<K, V> = HashMap<K, V>;

impl<K, V> Default for Map<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> Map<K, V> {
    pub fn new() -> Self {
        Self(MapType::new())
    }

    pub(crate) fn entry(&mut self, k: K) -> std::collections::hash_map::Entry<K, V>
    where
        K: Eq + Hash,
    {
        self.0.entry(k)
    }
}

impl<K, V> core::fmt::Debug for Map<K, V>
where
    K: core::fmt::Debug,
    V: core::fmt::Debug,
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> std::result::Result<(), fmt::Error> {
        self.0.fmt(formatter)
    }
}

impl<K, V> IntoIterator for Map<K, V> {
    type IntoIter = std::collections::hash_map::IntoIter<K, V>;
    type Item = (K, V);

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, K, V> IntoIterator for &'a Map<K, V> {
    type IntoIter = std::collections::hash_map::Iter<'a, K, V>;
    type Item = (&'a K, &'a V);

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a, K, V> IntoIterator for &'a mut Map<K, V> {
    type IntoIter = std::collections::hash_map::IterMut<'a, K, V>;
    type Item = (&'a K, &'a mut V);

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}

impl<K, Q, V> Index<&Q> for Map<K, V>
where
    K: Borrow<Q> + Eq + Hash + Ord,
    Q: Eq + Hash + Ord + ?Sized,
{
    type Output = V;

    fn index(&self, index: &Q) -> &Self::Output {
        self.0.index(index)
    }
}

impl<K, V> Map<K, V> {
    pub fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q> + Eq + Hash + Ord,
        Q: Eq + Hash + Ord + ?Sized,
    {
        self.0.get(key)
    }
}
