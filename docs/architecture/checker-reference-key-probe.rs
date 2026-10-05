//! Archive-only reference-key construction and call-site observation.
#![allow(missing_docs)]
use crate::TypeId;
use std::cell::{Cell, RefCell};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tsr_binder::SymbolId;

pub const SITES: [(&str, &str, u8); 29] = [
    ("empty_lookup", "empty", 1),
    ("empty_insert", "reused", 2),
    ("deferred_lookup", "slice_copy", 1),
    ("deferred_insert", "reused", 2),
    ("string_mapping_insert", "move", 2),
    ("identity_mapping_insert", "move", 2),
    ("ordinary_lookup", "vec_clone", 1),
    ("keyword_insert", "move", 2),
    ("conditional_insert", "move", 2),
    ("alias_body_insert", "move", 2),
    ("template_insert", "move", 2),
    ("normalized_mapping_insert", "move", 2),
    ("named_insert", "vec_clone", 2),
    ("mapped_sequence_insert", "move", 2),
    ("nonnullable_lookup", "singleton", 1),
    ("nonnullable_insert", "singleton", 2),
    ("index_original_lookup", "borrowed", 1),
    ("index_body_lookup", "borrowed", 1),
    ("member_body_lookup", "borrowed", 1),
    ("deferred_target", "tuple_clone", 0),
    ("tuple_mapped_target", "slice_copy", 0),
    ("template_mapped_target", "move", 0),
    ("variadic_target", "move", 0),
    ("indexed_target", "vec_clone", 0),
    ("union_target", "vec_clone", 0),
    ("callable_target", "vec_clone", 0),
    ("named_target", "vec_clone", 0),
    ("nonnullable_target", "singleton", 0),
    ("unclassified", "unknown", 3),
];
pub const FIELDS: [&str; 11] = [
    "constructions",
    "vector_items",
    "vector_capacity_items",
    "delegate_ns",
    "allocations",
    "requested_bytes",
    "usable_bytes",
    "lookup_operations",
    "insert_operations",
    "operation_vector_items",
    "nonempty_constructions",
];
const N: usize = 29 * 11;
static GLOBAL: [AtomicU64; N] = [const { AtomicU64::new(0) }; N];
std::thread_local! {
    static LOCAL: RefCell<[u64; N]> = const { RefCell::new([0; N]) };
    static CONSTRUCTION_SITE: Cell<Option<usize>> = const { Cell::new(None) };
    static OPERATION_SITE: Cell<Option<usize>> = const { Cell::new(None) };
}
fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("TSR_MAPPER_PROFILE").is_some())
}
fn add(site: usize, field: usize, value: u64) {
    let index = site * 11 + field;
    LOCAL.with(|cell| cell.borrow_mut()[index] += value);
    GLOBAL[index].fetch_add(value, Ordering::Relaxed);
}
fn named(values: &[u64; N]) -> Vec<(String, u64)> {
    SITES
        .iter()
        .enumerate()
        .flat_map(|(site, (name, _, _))| {
            FIELDS.iter().enumerate().map(move |(field, field_name)| {
                (format!("reference_site_{name}_{field_name}"), values[site * 11 + field])
            })
        })
        .collect()
}
#[must_use]
pub fn global_counts() -> Vec<(String, u64)> {
    named(&std::array::from_fn(|i| GLOBAL[i].load(Ordering::Relaxed)))
}
#[must_use]
pub fn local_counts() -> Vec<(String, u64)> {
    LOCAL.with(|cell| named(&cell.borrow()))
}

struct ConstructionScope(Option<usize>);
impl Drop for ConstructionScope {
    fn drop(&mut self) {
        CONSTRUCTION_SITE.with(|cell| cell.set(self.0));
    }
}
struct OperationScope(Option<usize>);
impl Drop for OperationScope {
    fn drop(&mut self) {
        OPERATION_SITE.with(|cell| cell.set(self.0));
    }
}

/// The closure performs the original empty, move, clone, `to_vec`, or `vec!` operation.
pub fn build(
    site: usize,
    allocation_tag: usize,
    original: impl FnOnce() -> (SymbolId, Vec<TypeId>),
) -> (SymbolId, Vec<TypeId>) {
    if !enabled() {
        return original();
    }
    assert!(site < 28);
    assert!(!matches!(SITES[site].1, "reused" | "borrowed"));
    let previous = CONSTRUCTION_SITE.with(|cell| cell.replace(Some(site)));
    assert!(previous.is_none(), "primitive key constructors cannot nest semantic work");
    let _site = ConstructionScope(previous);
    let _allocation = crate::mapper_key_probe::reference_allocation_scope(allocation_tag);
    let started = Instant::now();
    let result = original();
    let elapsed = u64::try_from(started.elapsed().as_nanos()).expect("observer interval fits u64");
    add(site, 0, 1);
    add(site, 1, result.1.len() as u64);
    add(site, 2, result.1.capacity() as u64);
    add(site, 3, elapsed);
    add(site, 10, u64::from(!result.1.is_empty()));
    result
}

pub fn at<R>(site: usize, original: impl FnOnce() -> R) -> R {
    if !enabled() {
        return original();
    }
    assert!(site < 19);
    let previous = OPERATION_SITE.with(|cell| cell.replace(Some(site)));
    assert!(previous.is_none(), "primitive table operations cannot nest semantic work");
    let _site = OperationScope(previous);
    original()
}

/// Called from the original table wrapper after the key already exists.
pub fn map_operation(operation: usize, vector_items: usize) {
    if !enabled() {
        return;
    }
    let site = OPERATION_SITE.with(Cell::get).unwrap_or(28);
    if site != 28 {
        assert_eq!(usize::from(SITES[site].2), operation + 1, "wrong reference operation site");
    }
    add(site, 7 + operation, 1);
    add(site, 9, vector_items as u64);
}

/// Original System allocation receipt; no heap allocation or pointer retention.
pub fn allocation(requested: u64, usable: u64) {
    if let Some(site) = CONSTRUCTION_SITE.try_with(Cell::get).ok().flatten() {
        add(site, 4, 1);
        add(site, 5, requested);
        add(site, 6, usable);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TypeStore;
    use tsr_binder::{SymbolFlags, SymbolStore};

    fn value(site: usize, field: usize) -> u64 {
        LOCAL.with(|cell| cell.borrow()[site * 11 + field])
    }

    #[test]
    fn exact_original_constructors_and_borrowed_calls_are_separate() {
        let mut symbols = SymbolStore::new();
        let symbol = symbols.create("Key", SymbolFlags::TYPE_ALIAS);
        let mut store = TypeStore::new();
        let t = store.new_intrinsic(crate::TypeFlags::NUMBER, "number");
        let empty_before = value(0, 0);
        let empty = build(0, 10, || (symbol, Vec::new()));
        assert!(empty.1.is_empty());
        assert_eq!(value(0, 0) - empty_before, 1);
        assert_eq!(value(0, 4), 0);
        let mut table = crate::mapper_hash_probe::Map::<_, _, 2>::default();
        for size in [1, 8, 128] {
            let arguments = vec![t; size];
            let clone_before = value(6, 0);
            let allocation_before = value(6, 4);
            let bytes_before = value(6, 5);
            let key = build(6, 10, || (symbol, arguments.clone()));
            assert_eq!(key, (symbol, arguments.clone()));
            assert_eq!(value(6, 0) - clone_before, 1);
            assert_eq!(value(6, 4) - allocation_before, 1);
            assert_eq!(value(6, 5) - bytes_before, (size * std::mem::size_of::<TypeId>()) as u64);
            let pointer = key.1.as_ptr();
            at(1, || table.insert(key, t));
            assert_eq!(table.keys().find(|key| key.1.len() == size).unwrap().1.as_ptr(), pointer);
            let borrowed = (symbol, arguments);
            let constructions_before = value(16, 0);
            let borrow_storage_before = value(16, 4);
            let operations_before = value(16, 7);
            assert_eq!(at(16, || table.get(&borrowed)), Some(&t));
            assert_eq!(value(16, 0), constructions_before);
            assert_eq!(value(16, 4), borrow_storage_before);
            assert_eq!(value(16, 7) - operations_before, 1);
        }
        let singleton = build(14, 10, || (symbol, vec![t]));
        assert_eq!(singleton, (symbol, vec![t]));
        let moved = vec![t; 8];
        let pointer = moved.as_ptr();
        let allocations_before = value(7, 4);
        let key = build(7, 11, || (symbol, moved));
        assert_eq!(key.1.as_ptr(), pointer);
        assert_eq!(value(7, 4), allocations_before);
        let tuple_before = value(19, 4);
        let tuple = build(19, 12, || key.clone());
        assert_eq!(tuple, key);
        assert_eq!(value(19, 4) - tuple_before, 1);
        let slice: &[TypeId] = &key.1;
        assert_eq!(build(2, 10, || (symbol, slice.to_vec())), key);
        // Existing map-operation/hash counter tests cover identity/order/rehash.
        // This detects omissions in actual constructor and allocation delegates.
    }

    #[test]
    fn all_three_borrowed_and_both_reused_sites_remain_allocation_free() {
        let mut symbols = SymbolStore::new();
        let symbol = symbols.create("Borrow", SymbolFlags::TYPE_ALIAS);
        let mut store = TypeStore::new();
        let t = store.new_intrinsic(crate::TypeFlags::STRING, "string");
        let key = (symbol, vec![t]);
        let mut table = crate::mapper_hash_probe::Map::<_, _, 2>::default();
        for site in [1, 3] {
            let before = value(site, 8);
            let existing = key.clone();
            at(site, || table.insert(existing, t));
            assert_eq!(value(site, 8) - before, 1);
            assert_eq!(value(site, 0), 0);
            assert_eq!(value(site, 4), 0);
        }
        for site in [16, 17, 18] {
            let before = value(site, 7);
            assert_eq!(at(site, || table.get(&key)), Some(&t));
            assert_eq!(value(site, 7) - before, 1);
            assert_eq!(value(site, 0), 0);
            assert_eq!(value(site, 4), 0);
        }
    }

    #[test]
    fn fresh_owners_reconcile_construction_allocations_and_operations() {
        let before: [u64; N] = std::array::from_fn(|i| GLOBAL[i].load(Ordering::Relaxed));
        let owners: Vec<_> = (0..2)
            .map(|_| {
                std::thread::spawn(|| {
                    let mut symbols = SymbolStore::new();
                    let symbol = symbols.create("Owner", SymbolFlags::TYPE_ALIAS);
                    let mut store = TypeStore::new();
                    let t = store.new_intrinsic(crate::TypeFlags::NUMBER, "number");
                    let arguments = vec![t; 128];
                    let key = build(6, 10, || (symbol, arguments.clone()));
                    let mut table = crate::mapper_hash_probe::Map::<_, _, 2>::default();
                    at(1, || table.insert(key, t));
                    let borrowed = (symbol, arguments);
                    assert_eq!(at(16, || table.get(&borrowed)), Some(&t));
                    LOCAL.with(|cell| *cell.borrow())
                })
            })
            .collect();
        let values: Vec<_> = owners.into_iter().map(|handle| handle.join().unwrap()).collect();
        for i in 0..N {
            assert_eq!(
                GLOBAL[i].load(Ordering::Relaxed) - before[i],
                values.iter().map(|owner| owner[i]).sum::<u64>()
            );
        }
        for owner in values {
            assert_eq!(owner[6 * 11], 1);
            assert_eq!(owner[6 * 11 + 4], 1);
            assert_eq!(owner[6 * 11 + 5], (128 * std::mem::size_of::<TypeId>()) as u64);
            assert_eq!(owner[11 + 8], 1);
            assert_eq!(owner[16 * 11 + 7], 1);
            assert_eq!(owner[28 * 11 + 7], 0);
        }
    }
}
