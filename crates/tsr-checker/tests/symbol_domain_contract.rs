//! Executable ownership layout for tsr-1yb.4.1.6.1, not a production symbol cache.
//! Uses actual binder handles; native clone/merge behavior is characterized by
//! docs/architecture/checker-symbol-storage-contract-controls.json.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tsr_binder::{SymbolFlags, SymbolId, SymbolStore};

#[derive(Clone, Debug)]
struct Domain(Arc<u8>);

impl Domain {
    fn new() -> Self {
        // Nonzero payload: identity is the live allocation, not its value.
        Self(Arc::new(1))
    }
}

impl PartialEq for Domain {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Domain {}

impl Hash for Domain {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).hash(state);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum SymbolRef {
    Bound { domain: Domain, id: SymbolId },
    Private { domain: Domain, index: u32 },
}

struct ProgramView<'a> {
    domain: Domain,
    symbols: &'a SymbolStore<'a>,
}

impl<'a> ProgramView<'a> {
    fn new(symbols: &'a SymbolStore<'a>) -> Self {
        Self { domain: Domain::new(), symbols }
    }

    fn bound(&self, id: SymbolId) -> SymbolRef {
        let _ = self.symbols.get(id);
        SymbolRef::Bound { domain: self.domain.clone(), id }
    }
}

#[derive(Debug)]
enum PrivateRecord {
    Unknown,
    Unresolved { parent: Option<SymbolRef> },
    Clone { origin: SymbolRef },
}

struct CheckerSymbols<'p, 'a> {
    program: &'p ProgramView<'a>,
    domain: Domain,
    records: Vec<PrivateRecord>,
    unresolved: HashMap<String, SymbolRef>,
}

impl<'p, 'a> CheckerSymbols<'p, 'a> {
    fn new(program: &'p ProgramView<'a>) -> Self {
        Self {
            program,
            domain: Domain::new(),
            records: vec![PrivateRecord::Unknown],
            unresolved: HashMap::new(),
        }
    }

    fn private(&self, index: u32) -> SymbolRef {
        assert!((index as usize) < self.records.len());
        SymbolRef::Private { domain: self.domain.clone(), index }
    }

    fn accepts(&self, symbol: &SymbolRef) -> bool {
        match symbol {
            SymbolRef::Bound { domain, id } => {
                *domain == self.program.domain && id.index() < self.program.symbols.len()
            }
            SymbolRef::Private { domain, index } => {
                *domain == self.domain && (*index as usize) < self.records.len()
            }
        }
    }

    fn record(&self, symbol: &SymbolRef) -> Option<&PrivateRecord> {
        if !self.accepts(symbol) {
            return None;
        }
        match symbol {
            SymbolRef::Private { index, .. } => self.records.get(*index as usize),
            SymbolRef::Bound { .. } => None,
        }
    }

    fn add(&mut self, record: PrivateRecord) -> SymbolRef {
        let index = u32::try_from(self.records.len()).expect("private symbol index fits");
        self.records.push(record);
        self.private(index)
    }

    fn unresolved(&mut self, path: &str) -> SymbolRef {
        if let Some(symbol) = self.unresolved.get(path) {
            return symbol.clone();
        }
        let parent = path.rsplit_once('.').map(|(parent, _)| self.unresolved(parent));
        let symbol = self.add(PrivateRecord::Unresolved { parent });
        self.unresolved.insert(path.to_owned(), symbol.clone());
        symbol
    }

    fn clone_symbol(&mut self, origin: SymbolRef) -> SymbolRef {
        assert!(self.accepts(&origin), "foreign symbol domain");
        self.add(PrivateRecord::Clone { origin })
    }
}

#[test]
fn bound_symbols_share_program_identity_and_reject_another_program() {
    let mut symbols = SymbolStore::new();
    let id = symbols.create("Shared", SymbolFlags::INTERFACE);
    let program = ProgramView::new(&symbols);
    let first = CheckerSymbols::new(&program);
    let second = CheckerSymbols::new(&program);
    let handle = program.bound(id);
    assert!(first.accepts(&handle) && second.accepts(&handle));
    assert_eq!(handle, program.bound(id));

    let mut other_symbols = SymbolStore::new();
    let other_id = other_symbols.create("Shared", SymbolFlags::INTERFACE);
    assert_eq!(id.index(), other_id.index());
    let other_program = ProgramView::new(&other_symbols);
    let foreign = other_program.bound(other_id);
    assert_ne!(handle, foreign);
    assert!(!first.accepts(&foreign));
}

#[test]
fn private_unknown_slots_are_distinct_from_uncomputed_and_foreign_slots() {
    let symbols = SymbolStore::new();
    let program = ProgramView::new(&symbols);
    let first = CheckerSymbols::new(&program);
    let second = CheckerSymbols::new(&program);
    let unknown = first.private(0);
    let uncomputed: Option<SymbolRef> = None;
    assert_ne!(Some(unknown.clone()), uncomputed);
    assert!(matches!(first.record(&unknown), Some(PrivateRecord::Unknown)));
    assert_ne!(unknown, second.private(0));
    assert!(!second.accepts(&unknown));
    assert!(second.record(&unknown).is_none());
}

#[test]
fn pointer_identity_hashing_does_not_collapse_equal_stamp_payloads() {
    let mut symbols = SymbolStore::new();
    let id = symbols.create("Shared", SymbolFlags::INTERFACE);
    let program = ProgramView::new(&symbols);
    let first = CheckerSymbols::new(&program);
    let second = CheckerSymbols::new(&program);
    let keys: HashSet<_> =
        [program.bound(id), first.private(0), second.private(0)].into_iter().collect();
    assert_eq!(keys.len(), 3);
    assert!(keys.contains(&program.bound(id)));
    assert!(keys.contains(&first.private(0)));
}

#[test]
fn a_retained_handle_keeps_its_owner_stamp_alive_after_checker_drop() {
    let symbols = SymbolStore::new();
    let program = ProgramView::new(&symbols);
    let first = CheckerSymbols::new(&program);
    let weak = Arc::downgrade(&first.domain.0);
    let retained = first.private(0);
    drop(first);
    assert!(weak.upgrade().is_some());
    let replacement = CheckerSymbols::new(&program);
    assert!(!replacement.accepts(&retained));
    drop(retained);
    assert!(weak.upgrade().is_none());
}

#[test]
fn moving_a_checker_preserves_its_handle_domain() {
    let symbols = SymbolStore::new();
    let program = ProgramView::new(&symbols);
    let original = CheckerSymbols::new(&program);
    let unknown = original.private(0);
    let moved = Box::new(original);
    assert!(moved.accepts(&unknown));
    assert!(matches!(moved.record(&unknown), Some(PrivateRecord::Unknown)));
}

#[test]
fn unresolved_symbols_intern_full_paths_in_one_checker_only() {
    let symbols = SymbolStore::new();
    let program = ProgramView::new(&symbols);
    let mut first = CheckerSymbols::new(&program);
    let mut second = CheckerSymbols::new(&program);
    let a = first.unresolved("Missing.Child");
    let repeated = first.unresolved("Missing.Child");
    let other = first.unresolved("Other.Child");
    let foreign = second.unresolved("Missing.Child");
    assert_eq!(a, repeated);
    assert_ne!(a, other);
    assert_ne!(a, foreign);
    assert!(!first.accepts(&foreign));
    let Some(PrivateRecord::Unresolved { parent: Some(parent) }) = first.record(&a) else {
        panic!("qualified unresolved symbol requires a parent");
    };
    assert_eq!(*parent, first.unresolved["Missing"]);
    assert_ne!(*parent, first.unresolved["Other"]);
}

#[test]
fn private_clones_keep_bound_origin_without_becoming_bound_handles() {
    let mut symbols = SymbolStore::new();
    let target = symbols.create("Original", SymbolFlags::INTERFACE);
    let alias = symbols.create("Renamed", SymbolFlags::ALIAS);
    let program = ProgramView::new(&symbols);
    let mut first = CheckerSymbols::new(&program);
    let mut second = CheckerSymbols::new(&program);
    let origin = program.bound(target);
    let written_alias = program.bound(alias);
    let clone = first.clone_symbol(origin.clone());
    let other_clone = second.clone_symbol(origin.clone());
    assert_ne!(clone, origin);
    assert_ne!(clone, other_clone);
    assert_ne!(written_alias, origin);
    assert!(!second.accepts(&clone));
    let Some(PrivateRecord::Clone { origin: saved }) = first.record(&clone) else {
        panic!("private clone requires origin metadata");
    };
    assert_eq!(*saved, origin);
    assert!(second.accepts(saved));
}
