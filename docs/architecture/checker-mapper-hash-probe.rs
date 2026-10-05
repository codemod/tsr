//! Archive-only map-operation observation. Keys, hashing and equality are delegated unchanged.
#![allow(missing_docs)]
use crate::TypeId;
use rustc_hash::FxHashMap;
use std::borrow::Borrow;
use std::cell::Cell;
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tsr_binder::SymbolId;

const DOMAINS: [&str; 4] = ["object", "signature", "reference", "predicate"];
const OPERATIONS: [&str; 3] = ["lookup", "insert", "unscoped"];
const FIELDS: [&str; 12] = [
    "operations",
    "hash_invocations",
    "hash_vector_items",
    "hash_identity_fields",
    "hash_delegate_ns",
    "eq_key_comparisons",
    "eq_true",
    "eq_false",
    "eq_delegate_ns",
    "map_ns",
    "max_vector_items",
    "eq_vector_items_supplied",
];
const N: usize = 4 * 3 * 12;
static GLOBAL: [AtomicU64; N] = [const { AtomicU64::new(0) }; N];
std::thread_local! {
    static LOCAL: Cell<[u64; N]> = const { Cell::new([0; N]) };
    static OPERATION: Cell<Option<(usize, usize)>> = const { Cell::new(None) };
}
fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("TSR_MAPPER_PROFILE").is_some())
}
fn add(domain: usize, operation: usize, field: usize, value: u64) {
    let index = (domain * 3 + operation) * 12 + field;
    LOCAL.with(|cell| {
        let mut counts = cell.get();
        if field == 10 {
            counts[index] = counts[index].max(value);
        } else {
            counts[index] += value;
        }
        cell.set(counts);
    });
    if field == 10 {
        GLOBAL[index].fetch_max(value, Ordering::Relaxed);
    } else {
        GLOBAL[index].fetch_add(value, Ordering::Relaxed);
    }
}
fn operation(domain: usize) -> usize {
    OPERATION.with(|cell| match cell.get() {
        Some((active_domain, operation)) => {
            assert_eq!(active_domain, domain, "hash/equality escaped its actual table domain");
            operation
        }
        None => 2,
    })
}
fn named(values: &[u64; N]) -> Vec<(String, u64)> {
    let mut out = Vec::with_capacity(N);
    for (domain, name) in DOMAINS.iter().enumerate() {
        for (operation, op_name) in OPERATIONS.iter().enumerate() {
            for (field, field_name) in FIELDS.iter().enumerate() {
                out.push((
                    format!("table_{name}_{op_name}_{field_name}"),
                    values[(domain * 3 + operation) * 12 + field],
                ));
            }
        }
    }
    out
}
#[must_use]
pub fn global_counts() -> Vec<(String, u64)> {
    named(&std::array::from_fn(|i| GLOBAL[i].load(Ordering::Relaxed)))
}
#[must_use]
pub fn local_counts() -> Vec<(String, u64)> {
    named(&LOCAL.with(Cell::get))
}

pub trait KeyShape {
    fn vector_items(&self) -> usize;
    fn identity_fields(&self) -> usize;
}
impl KeyShape for (TypeId, Vec<(TypeId, TypeId)>) {
    fn vector_items(&self) -> usize {
        self.1.len()
    }
    fn identity_fields(&self) -> usize {
        1 + 2 * self.1.len()
    }
}
impl KeyShape for (SymbolId, Vec<TypeId>) {
    fn vector_items(&self) -> usize {
        self.1.len()
    }
    fn identity_fields(&self) -> usize {
        1 + self.1.len()
    }
}
impl KeyShape for (TypeId, Vec<TypeId>) {
    fn vector_items(&self) -> usize {
        self.1.len()
    }
    fn identity_fields(&self) -> usize {
        1 + self.1.len()
    }
}
fn hash_key<K: Hash + KeyShape, H: Hasher>(key: &K, hasher: &mut H, domain: usize) {
    if !enabled() {
        key.hash(hasher);
        return;
    }
    let op = operation(domain);
    let started = Instant::now();
    key.hash(hasher);
    let elapsed = u64::try_from(started.elapsed().as_nanos())
        .expect("observer interval fits u64 nanoseconds");
    add(domain, op, 1, 1);
    add(domain, op, 2, key.vector_items() as u64);
    add(domain, op, 3, key.identity_fields() as u64);
    add(domain, op, 4, elapsed);
    add(domain, op, 10, key.vector_items() as u64);
}
fn equal_keys<K: Eq + KeyShape>(left: &K, right: &K, domain: usize) -> bool {
    if !enabled() {
        return left.eq(right);
    }
    let op = operation(domain);
    let started = Instant::now();
    let answer = left.eq(right);
    let elapsed = u64::try_from(started.elapsed().as_nanos())
        .expect("observer interval fits u64 nanoseconds");
    add(domain, op, 5, 1);
    add(domain, op, if answer { 6 } else { 7 }, 1);
    add(domain, op, 8, elapsed);
    // Inputs available to key equality, not a scalar-comparison or traversal count.
    add(domain, op, 11, (left.vector_items() + right.vector_items()) as u64);
    answer
}
trait View<K, const D: usize> {
    fn raw(&self) -> &K;
}
#[repr(transparent)]
#[derive(Clone, Debug)]
struct Stored<K, const D: usize>(K);
impl<K, const D: usize> View<K, D> for Stored<K, D> {
    fn raw(&self) -> &K {
        &self.0
    }
}
impl<K: Hash + KeyShape, const D: usize> Hash for Stored<K, D> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_key(&self.0, state, D);
    }
}
impl<K: Eq + KeyShape, const D: usize> PartialEq for Stored<K, D> {
    fn eq(&self, other: &Self) -> bool {
        equal_keys(&self.0, &other.0, D)
    }
}
impl<K: Eq + KeyShape, const D: usize> Eq for Stored<K, D> {}
impl<'v, K: 'v, const D: usize> Borrow<dyn View<K, D> + 'v> for Stored<K, D> {
    fn borrow(&self) -> &(dyn View<K, D> + 'v) {
        self
    }
}
struct Query<'k, K>(&'k K);
impl<K, const D: usize> View<K, D> for Query<'_, K> {
    fn raw(&self) -> &K {
        self.0
    }
}
impl<K: Hash + KeyShape, const D: usize> Hash for dyn View<K, D> + '_ {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_key(self.raw(), state, D);
    }
}
impl<K: Eq + KeyShape, const D: usize> PartialEq for dyn View<K, D> + '_ {
    fn eq(&self, other: &Self) -> bool {
        equal_keys(self.raw(), other.raw(), D)
    }
}
impl<K: Eq + KeyShape, const D: usize> Eq for dyn View<K, D> + '_ {}
struct Scope {
    prior: Option<(usize, usize)>,
    started: Option<Instant>,
    domain: usize,
    op: usize,
}
impl Scope {
    fn enter(domain: usize, op: usize) -> Self {
        if !enabled() {
            return Self { prior: None, started: None, domain, op };
        }
        let prior = OPERATION.with(|cell| cell.replace(Some((domain, op))));
        assert!(prior.is_none(), "map calls cannot nest semantic work inside hash/equality");
        add(domain, op, 0, 1);
        Self { prior, started: Some(Instant::now()), domain, op }
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        if let Some(started) = self.started {
            let elapsed = u64::try_from(started.elapsed().as_nanos())
                .expect("observer interval fits u64 nanoseconds");
            OPERATION.with(|cell| cell.set(self.prior));
            add(self.domain, self.op, 9, elapsed);
        }
    }
}
#[repr(transparent)]
#[derive(Clone, Debug)]
pub struct Map<K, V, const D: usize> {
    inner: FxHashMap<Stored<K, D>, V>,
}
impl<K, V, const D: usize> Default for Map<K, V, D> {
    fn default() -> Self {
        Self { inner: FxHashMap::default() }
    }
}
impl<K: Eq + Hash + KeyShape, V, const D: usize> Map<K, V, D> {
    pub fn get(&self, key: &K) -> Option<&V> {
        let _scope = Scope::enter(D, 0);
        let query = Query(key);
        let view: &(dyn View<K, D> + '_) = &query;
        self.inner.get(view)
    }
    pub fn contains_key(&self, key: &K) -> bool {
        let _scope = Scope::enter(D, 0);
        let query = Query(key);
        let view: &(dyn View<K, D> + '_) = &query;
        self.inner.contains_key(view)
    }
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let _scope = Scope::enter(D, 1);
        self.inner.insert(Stored(key), value)
    }
}
impl<K, V, const D: usize> Map<K, V, D> {
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.inner.keys().map(|key| &key.0)
    }
}
impl<K: Eq + Hash + KeyShape, V: PartialEq, const D: usize> PartialEq for Map<K, V, D> {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}
impl<K: Eq + Hash + KeyShape, V: Eq, const D: usize> Eq for Map<K, V, D> {}

#[cfg(test)]
mod tests {
    use super::*;
    use rustc_hash::FxHasher;
    use std::collections::HashMap;
    use std::hash::BuildHasherDefault;
    impl KeyShape for (u32, Vec<u32>) {
        fn vector_items(&self) -> usize {
            self.1.len()
        }
        fn identity_fields(&self) -> usize {
            1 + self.1.len()
        }
    }
    impl KeyShape for (u32, Vec<(u32, u32)>) {
        fn vector_items(&self) -> usize {
            self.1.len()
        }
        fn identity_fields(&self) -> usize {
            1 + 2 * self.1.len()
        }
    }
    #[derive(Default, Debug, PartialEq)]
    struct Feed(Vec<(String, Vec<u8>)>);
    impl Hasher for Feed {
        fn finish(&self) -> u64 {
            0
        }
        fn write(&mut self, bytes: &[u8]) {
            self.0.push(("bytes".into(), bytes.to_vec()));
        }
        fn write_u32(&mut self, value: u32) {
            self.0.push(("u32".into(), value.to_ne_bytes().to_vec()));
        }
        fn write_usize(&mut self, value: usize) {
            self.0.push(("usize".into(), value.to_ne_bytes().to_vec()));
        }
    }
    fn feed<K: Hash + ?Sized>(key: &K) -> Feed {
        let mut h = Feed::default();
        key.hash(&mut h);
        h
    }
    fn digest<K: Hash + ?Sized>(key: &K) -> u64 {
        let mut h = FxHasher::default();
        key.hash(&mut h);
        h.finish()
    }
    #[test]
    fn original_hash_feed_and_fx_digest_are_exact_for_owned_and_borrowed_keys() {
        let maps = [
            vec![],
            vec![(7, 9)],
            vec![(7, 9), (9, 11)],
            vec![(9, 11), (7, 9)],
            (0..128).map(|i| (i, i + 1)).collect(),
        ];
        for items in maps {
            for id in [3u32, 4] {
                let raw = (id, items.clone());
                let stored = Stored::<_, 0>(raw.clone());
                let query = Query(&raw);
                let view: &dyn View<_, 0> = &query;
                assert_eq!(feed(&raw), feed(&stored));
                assert_eq!(feed(&raw), feed(view));
                assert_eq!(digest(&raw), digest(&stored));
                assert_eq!(digest(&raw), digest(view));
            }
        }
        for size in [0, 1, 8, 128] {
            let raw = (3u32, (0..size).collect::<Vec<u32>>());
            let stored = Stored::<_, 2>(raw.clone());
            let query = Query(&raw);
            let view: &dyn View<_, 2> = &query;
            assert_eq!(feed(&raw), feed(&stored));
            assert_eq!(feed(&raw), feed(view));
            assert_eq!(digest(&raw), digest(&stored));
            assert_eq!(digest(&raw), digest(view));
        }
    }
    #[test]
    fn actual_checker_type_and_symbol_ids_delegate_the_identical_hash_feed() {
        let mut types = crate::TypeStore::new();
        let id = types.new_named(crate::TypeFlags::OBJECT, "source".to_owned(), None);
        let image = types.new_named(crate::TypeFlags::OBJECT, "image".to_owned(), None);
        let raw = (id, vec![(id, image), (image, id)]);
        let stored = Stored::<_, 0>(raw.clone());
        let query = Query(&raw);
        let view: &dyn View<_, 0> = &query;
        assert_eq!(feed(&raw), feed(&stored));
        assert_eq!(feed(&raw), feed(view));
        assert_eq!(digest(&raw), digest(view));
        let mut symbols = tsr_binder::SymbolStore::new();
        let symbol = symbols.create("Alias", tsr_binder::SymbolFlags::TYPE_ALIAS);
        let raw = (symbol, vec![id, image]);
        let stored = Stored::<_, 2>(raw.clone());
        let query = Query(&raw);
        let view: &dyn View<_, 2> = &query;
        assert_eq!(feed(&raw), feed(&stored));
        assert_eq!(feed(&raw), feed(view));
        assert_eq!(digest(&raw), digest(view));
    }
    #[test]
    fn moved_key_has_same_layout_and_vector_allocation_and_lookup_answers() {
        type K = (u32, Vec<(u32, u32)>);
        assert_eq!(size_of::<Stored<K, 0>>(), size_of::<K>());
        assert_eq!(size_of::<Map<K, u32, 0>>(), size_of::<FxHashMap<K, u32>>());
        let mut map = Map::<K, u32, 0>::default();
        let mut raw_map = FxHashMap::default();
        for i in 0..64u32 {
            let key = (i, vec![(7, 9), (i, 11)]);
            let ptr = key.1.as_ptr();
            raw_map.insert(key.clone(), i);
            map.insert(key, i);
            assert_eq!(map.keys().find(|k| k.0 == i).unwrap().1.as_ptr(), ptr);
        }
        for i in 0..65u32 {
            for items in [vec![(7, 9), (i, 11)], vec![(i, 11), (7, 9)], vec![(7, 9)], vec![]] {
                let key = (i, items);
                assert_eq!(map.get(&key), raw_map.get(&key));
            }
        }
        assert_eq!(map.len(), raw_map.len());
        assert_eq!(map.capacity(), raw_map.capacity());
    }
    #[derive(Default)]
    struct Collision;
    impl Hasher for Collision {
        fn finish(&self) -> u64 {
            0
        }
        fn write(&mut self, _: &[u8]) {}
    }
    #[test]
    fn test_only_collisions_exercise_equal_unequal_and_missing_counter_guards() {
        assert!(enabled(), "run focused controls with TSR_MAPPER_PROFILE=1");
        let before = LOCAL.with(Cell::get);
        let mut map =
            HashMap::<Stored<(u32, Vec<u32>), 3>, u32, BuildHasherDefault<Collision>>::default();
        for id in 0..8u32 {
            map.insert(Stored((id, vec![1, 2, 3])), id);
        }
        for id in 0..9u32 {
            let raw = (id, vec![1, 2, 3]);
            let query = Query(&raw);
            let view: &dyn View<_, 3> = &query;
            assert_eq!(map.get(view).copied(), (id < 8).then_some(id));
        }
        let after = LOCAL.with(Cell::get);
        let offset = (3 * 3 + 2) * 12;
        let delta: [u64; 12] = std::array::from_fn(|i| after[offset + i] - before[offset + i]);
        assert!(delta[1] > 0);
        assert_eq!(delta[2], delta[1] * 3);
        assert_eq!(delta[3], delta[1] + delta[2]);
        assert!(delta[5] > 0);
        assert!(delta[6] > 0);
        assert!(delta[7] > 0);
        assert_eq!(delta[5], delta[6] + delta[7]);
        assert_eq!(delta[11], delta[5] * 6);
    }
    #[test]
    fn actual_map_operations_reconcile_global_and_fresh_owner_tls_without_domain_aliasing() {
        assert!(enabled());
        let before: [u64; N] = std::array::from_fn(|i| GLOBAL[i].load(Ordering::Relaxed));
        let owners: Vec<_> = (0..2)
            .map(|owner| {
                std::thread::spawn(move || {
                    let mut objects = Map::<(u32, Vec<u32>), u32, 0>::default();
                    let mut signatures = Map::<(u32, Vec<u32>), u32, 1>::default();
                    for i in 0..32u32 {
                        let key = (i, vec![owner; 1024]);
                        objects.insert(key.clone(), i);
                        signatures.insert(key, i + 1);
                    }
                    let key = (3, vec![owner; 1024]);
                    assert_eq!(objects.get(&key), Some(&3));
                    assert_eq!(signatures.get(&key), Some(&4));
                    LOCAL.with(Cell::get)
                })
            })
            .collect();
        let values: Vec<_> = owners.into_iter().map(|h| h.join().unwrap()).collect();
        for i in 0..N {
            let after = GLOBAL[i].load(Ordering::Relaxed);
            if i % 12 == 10 {
                assert_eq!(after, values.iter().map(|v| v[i]).fold(before[i], u64::max));
            } else {
                assert_eq!(after - before[i], values.iter().map(|v| v[i]).sum::<u64>());
            }
        }
        for owner in values {
            for domain in 0..2 {
                let lookup = domain * 3 * 12;
                let insert = lookup + 12;
                assert_eq!(owner[lookup], 1);
                assert_eq!(owner[lookup + 1], 1);
                assert_eq!(owner[lookup + 5], 1);
                assert_eq!(owner[lookup + 6], 1);
                assert_eq!(owner[lookup + 7], 0);
                assert_eq!(owner[insert], 32);
                assert!(owner[insert + 1] >= 32, "inserts include table-growth rehashing");
                assert_eq!(owner[insert + 3], owner[insert + 1] + owner[insert + 2]);
            }
            assert!(owner[2 * 3 * 12..].iter().all(|v| *v == 0));
        }
    }
}
