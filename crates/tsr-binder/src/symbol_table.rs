//! [`SymbolTable`]: names to symbols within one scope, iterated in insertion
//! order.
//!
//! Upstream's table is `ast.SymbolTable`, a Go `map[string]*Symbol`
//! (`internal/ast/symbol.go:45`), whose iteration order is randomized; tsgo's
//! output-ordering consumers sort what they read (`sortSymbols`,
//! `internal/checker/utilities.go:362`, by first declaration). The order here
//! is a function of the insert sequence alone, never of capacity or of a hash
//! function, so a table's capacity can be chosen freely. Why this
//! representation and not a pre-sized hash map: [ADR-0049].
//!
//! [ADR-0049]: ../../../docs/adr/0049-symbol-table-insertion-order.md

use std::{borrow::Borrow, fmt, hash::Hash};

use crate::SymbolId;

/// Up to this many entries a table has no hash index: a lookup compares
/// names linearly. Most member tables never leave this range
/// (`docs/parity/notes/r5-symtab.md` §3).
const LINEAR_MAX: usize = 8;

/// Names to symbols, within one scope, in insertion order.
///
/// Ported from `ast.SymbolTable` (`internal/ast/symbol.go:45`). An `insert`
/// of an existing name replaces its symbol in place; `remove` keeps the
/// relative order of the rest. Iteration order is the order in which names
/// were first inserted, and nothing else — see [ADR-0049].
///
/// [ADR-0049]: ../../../docs/adr/0049-symbol-table-insertion-order.md
#[derive(Clone, Default)]
pub struct SymbolTable<'a> {
    entries: Vec<(&'a str, SymbolId)>,
    /// Open-addressing (linear probing) index over `entries`, present only
    /// above [`LINEAR_MAX`] entries: each slot is `0` (empty) or the entry's
    /// position plus one in the low [`POSITION_BITS`] bits and an 8-bit tag of
    /// its name's hash above them. The slot count is a power of two at least
    /// twice `entries.len()`. Boxed so that a table is two words plus a
    /// thin pointer — the size of the `FxHashMap` it replaced — since every
    /// symbol carries two of them and few tables ever need an index.
    #[allow(clippy::box_collection, reason = "a thin pointer keeps the table at 32 bytes")]
    index: Option<Box<Vec<u32>>>,
}

/// Bits of an index slot that hold the entry position plus one.
const POSITION_BITS: u32 = 24;
const POSITION_MASK: u32 = (1 << POSITION_BITS) - 1;

/// A name's hash: `FxHash`'s well-mixed high 32 bits.
fn hash_of<Q: ?Sized + Hash>(name: &Q) -> u32 {
    use std::hash::{BuildHasher, BuildHasherDefault};
    let h = BuildHasherDefault::<rustc_hash::FxHasher>::default().hash_one(name);
    (h >> 32) as u32
}

/// The slot tag of a hash: its top 8 bits, in the slot's top 8 bits.
fn tag_of(hash: u32) -> u32 {
    hash & !POSITION_MASK
}

impl<'a> SymbolTable<'a> {
    /// An empty table that allocates nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty table with room for `capacity` names before it reallocates.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        let mut table = Self { entries: Vec::with_capacity(capacity), index: None };
        if capacity > LINEAR_MAX {
            table.index = Some(Box::new(vec![0; slots_for(capacity)]));
        }
        table
    }

    /// Room for `additional` more names before the next reallocation.
    pub fn reserve(&mut self, additional: usize) {
        self.entries.reserve(additional);
        let wanted = self.entries.len() + additional;
        if wanted > LINEAR_MAX && self.slot_count() < slots_for(wanted) {
            self.rebuild_index(slots_for(wanted));
        }
    }

    /// The number of names.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table has no names.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Names the table holds before it reallocates.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.entries.capacity()
    }

    /// Heap bytes this table retains.
    #[must_use]
    pub fn heap_bytes(&self) -> usize {
        self.entries.capacity() * size_of::<(&str, SymbolId)>()
            + self
                .index
                .as_ref()
                .map_or(0, |index| size_of::<Vec<u32>>() + index.capacity() * size_of::<u32>())
    }

    fn position<Q>(&self, name: &Q) -> Option<usize>
    where
        &'a str: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let Some(index) = self.index.as_deref() else {
            return self
                .entries
                .iter()
                .position(|(key, _)| <&str as Borrow<Q>>::borrow(key) == name);
        };
        let hash = hash_of(name);
        let mask = index.len() - 1;
        let mut slot = hash as usize & mask;
        loop {
            let packed = index[slot];
            if packed == 0 {
                return None;
            }
            if packed & !POSITION_MASK == tag_of(hash) {
                let position = ((packed & POSITION_MASK) - 1) as usize;
                if <&str as Borrow<Q>>::borrow(&self.entries[position].0) == name {
                    return Some(position);
                }
            }
            slot = (slot + 1) & mask;
        }
    }

    fn index_insert(index: &mut [u32], hash: u32, position: usize) {
        let mask = index.len() - 1;
        let mut slot = hash as usize & mask;
        while index[slot] != 0 {
            slot = (slot + 1) & mask;
        }
        let position = u32::try_from(position + 1)
            .ok()
            .filter(|position| *position <= POSITION_MASK)
            .expect("symbol table exceeds 2^24 - 1 names");
        index[slot] = tag_of(hash) | position;
    }

    /// Index slots, zero without an index.
    fn slot_count(&self) -> usize {
        self.index.as_ref().map_or(0, |index| index.len())
    }

    fn rebuild_index(&mut self, slots: usize) {
        let index = self.index.get_or_insert_with(Box::default);
        index.clear();
        index.resize(slots, 0);
        for (position, (key, _)) in self.entries.iter().enumerate() {
            Self::index_insert(index, hash_of(*key), position);
        }
    }

    /// The symbol bound to `name`.
    #[must_use]
    pub fn get<Q>(&self, name: &Q) -> Option<&SymbolId>
    where
        &'a str: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.position(name).map(|position| &self.entries[position].1)
    }

    /// The symbol bound to `name`, mutably.
    pub fn get_mut<Q>(&mut self, name: &Q) -> Option<&mut SymbolId>
    where
        &'a str: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.position(name).map(|position| &mut self.entries[position].1)
    }

    /// Whether `name` is bound.
    #[must_use]
    pub fn contains_key<Q>(&self, name: &Q) -> bool
    where
        &'a str: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.position(name).is_some()
    }

    /// Bind `name` to `symbol`. A name already bound keeps its position and
    /// returns its previous symbol; a new name goes last.
    pub fn insert(&mut self, name: &'a str, symbol: SymbolId) -> Option<SymbolId> {
        if let Some(position) = self.position(name) {
            return Some(std::mem::replace(&mut self.entries[position].1, symbol));
        }
        self.push_new(name, symbol);
        None
    }

    /// Append a name known to be absent.
    fn push_new(&mut self, name: &'a str, symbol: SymbolId) -> usize {
        let position = self.entries.len();
        self.entries.push((name, symbol));
        let len = position + 1;
        if let Some(index) = self.index.as_deref_mut() {
            if index.len() < 2 * len {
                self.rebuild_index(slots_for(len));
            } else {
                Self::index_insert(index, hash_of(name), position);
            }
        } else if len > LINEAR_MAX {
            self.rebuild_index(slots_for(len.max(self.entries.capacity())));
        }
        position
    }

    /// The entry for `name`, for in-place insertion or update.
    pub fn entry(&mut self, name: &'a str) -> Entry<'_, 'a> {
        match self.position(name) {
            Some(position) => Entry::Occupied(&mut self.entries[position].1),
            None => Entry::Vacant(VacantEntry { table: self, name }),
        }
    }

    /// Unbind `name`, keeping the relative order of the rest.
    pub fn remove<Q>(&mut self, name: &Q) -> Option<SymbolId>
    where
        &'a str: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let position = self.position(name)?;
        let (_, symbol) = self.entries.remove(position);
        if self.index.is_some() {
            self.rebuild_index(self.slot_count());
        }
        Some(symbol)
    }

    /// Unbind every name, keeping the allocation.
    pub fn clear(&mut self) {
        self.entries.clear();
        if let Some(index) = self.index.as_deref_mut() {
            index.fill(0);
        }
    }

    /// Keep only the bindings `keep` accepts, in their order.
    pub fn retain(&mut self, mut keep: impl FnMut(&&'a str, &mut SymbolId) -> bool) {
        let before = self.entries.len();
        self.entries.retain_mut(|(name, symbol)| keep(name, symbol));
        if self.entries.len() != before && self.index.is_some() {
            self.rebuild_index(self.slot_count());
        }
    }

    /// Bindings in insertion order.
    #[must_use]
    pub fn iter(&self) -> Iter<'_, 'a> {
        Iter(self.entries.iter())
    }

    /// Bindings in insertion order, with mutable symbols.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&&'a str, &mut SymbolId)> {
        self.entries.iter_mut().map(|(name, symbol)| (&*name, symbol))
    }

    /// Names in insertion order.
    #[must_use]
    pub fn keys(&self) -> impl DoubleEndedIterator<Item = &&'a str> + ExactSizeIterator + Clone {
        self.entries.iter().map(|(name, _)| name)
    }

    /// Symbols in insertion order.
    #[must_use]
    pub fn values(&self) -> impl DoubleEndedIterator<Item = &SymbolId> + ExactSizeIterator + Clone {
        self.entries.iter().map(|(_, symbol)| symbol)
    }

    /// Symbols in insertion order, mutably.
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut SymbolId> {
        self.entries.iter_mut().map(|(_, symbol)| symbol)
    }
}

/// Index-slot count for `len` entries: a power of two, load at most one half.
fn slots_for(len: usize) -> usize {
    (2 * len).next_power_of_two().max(2 * (LINEAR_MAX + 1)).next_power_of_two()
}

/// A [`SymbolTable::entry`] result.
pub enum Entry<'t, 'a> {
    /// The name is bound; the reference is its symbol.
    Occupied(&'t mut SymbolId),
    /// The name is not bound.
    Vacant(VacantEntry<'t, 'a>),
}

/// An unbound name of a [`SymbolTable`].
pub struct VacantEntry<'t, 'a> {
    table: &'t mut SymbolTable<'a>,
    name: &'a str,
}

impl<'t> VacantEntry<'t, '_> {
    /// Bind the name, last in order.
    #[must_use]
    pub fn insert(self, symbol: SymbolId) -> &'t mut SymbolId {
        let position = self.table.push_new(self.name, symbol);
        &mut self.table.entries[position].1
    }
}

impl<'t> Entry<'t, '_> {
    /// The bound symbol, binding `symbol` first if the name is unbound.
    #[allow(clippy::must_use_candidate, reason = "binding is the point; the reference is optional")]
    pub fn or_insert(self, symbol: SymbolId) -> &'t mut SymbolId {
        self.or_insert_with(|| symbol)
    }

    /// The bound symbol, binding `make()` first if the name is unbound.
    pub fn or_insert_with(self, make: impl FnOnce() -> SymbolId) -> &'t mut SymbolId {
        match self {
            Entry::Occupied(symbol) => symbol,
            Entry::Vacant(vacant) => vacant.insert(make()),
        }
    }
}

/// Borrowed iteration over a [`SymbolTable`], in insertion order.
#[derive(Clone, Debug)]
pub struct Iter<'s, 'a>(std::slice::Iter<'s, (&'a str, SymbolId)>);

impl<'s, 'a> Iterator for Iter<'s, 'a> {
    type Item = (&'s &'a str, &'s SymbolId);
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(name, symbol)| (name, symbol))
    }
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl DoubleEndedIterator for Iter<'_, '_> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(|(name, symbol)| (name, symbol))
    }
}

impl ExactSizeIterator for Iter<'_, '_> {}
impl std::iter::FusedIterator for Iter<'_, '_> {}

impl<'s, 'a> IntoIterator for &'s SymbolTable<'a> {
    type Item = (&'s &'a str, &'s SymbolId);
    type IntoIter = Iter<'s, 'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for SymbolTable<'a> {
    type Item = (&'a str, SymbolId);
    type IntoIter = std::vec::IntoIter<(&'a str, SymbolId)>;
    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
    }
}

impl<'a> Extend<(&'a str, SymbolId)> for SymbolTable<'a> {
    fn extend<I: IntoIterator<Item = (&'a str, SymbolId)>>(&mut self, bindings: I) {
        for (name, symbol) in bindings {
            self.insert(name, symbol);
        }
    }
}

impl<'a> FromIterator<(&'a str, SymbolId)> for SymbolTable<'a> {
    fn from_iter<I: IntoIterator<Item = (&'a str, SymbolId)>>(bindings: I) -> Self {
        let mut table = Self::new();
        table.extend(bindings);
        table
    }
}

impl<'a, Q> std::ops::Index<&Q> for SymbolTable<'a>
where
    &'a str: Borrow<Q>,
    Q: ?Sized + Hash + Eq,
{
    type Output = SymbolId;
    fn index(&self, name: &Q) -> &SymbolId {
        self.get(name).expect("no entry found for key")
    }
}

/// Order-insensitive, as the map it replaced compared.
impl PartialEq for SymbolTable<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len()
            && self.iter().all(|(name, symbol)| other.get(*name) == Some(symbol))
    }
}
impl Eq for SymbolTable<'_> {}

impl fmt::Debug for SymbolTable<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: usize) -> SymbolId {
        SymbolId(u32::try_from(n).unwrap())
    }

    /// Iteration is insertion order at every size, across the switch from
    /// linear lookup to the hash index, through replacement and removal.
    #[test]
    fn insertion_order_survives_growth_replacement_and_removal() {
        let names: Vec<String> = (0..200).map(|n| format!("n{}", (n * 37) % 200)).collect();
        for presize in [0, 3, 9, 200] {
            let mut table = SymbolTable::with_capacity(presize);
            for (n, name) in names.iter().enumerate() {
                assert_eq!(table.insert(name, id(n)), None);
                assert_eq!(table.get(name.as_str()), Some(&id(n)));
            }
            assert_eq!(table.keys().map(ToString::to_string).collect::<Vec<_>>(), names);
            assert_eq!(table.insert(&names[5], id(999)), Some(id(5)));
            assert_eq!(*table.keys().nth(5).unwrap(), names[5]);
            assert_eq!(table.remove(names[0].as_str()), Some(id(0)));
            assert_eq!(table.remove("absent"), None);
            assert_eq!(
                table.keys().copied().collect::<Vec<_>>(),
                names[1..].iter().map(String::as_str).collect::<Vec<_>>()
            );
            for (n, name) in names.iter().enumerate().skip(1) {
                let expected = if n == 5 { id(999) } else { id(n) };
                assert_eq!(table.get(name.as_str()), Some(&expected), "{name}");
            }
            assert!(!table.contains_key(names[0].as_str()));
        }
    }

    #[test]
    fn a_table_is_the_size_of_the_map_it_replaced() {
        assert_eq!(
            size_of::<SymbolTable<'_>>(),
            size_of::<rustc_hash::FxHashMap<&str, SymbolId>>()
        );
    }
}
