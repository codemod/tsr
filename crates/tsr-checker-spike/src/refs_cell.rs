//! Style (c), refined: `&self` methods, arena types, and a memo table of `Cell`s.
//!
//! Identical to [`crate::refs_refcell`] except for what guards the memo table. That table
//! holds `Option<&'a Type<'a>>`, which is `Copy`, so it does not need `RefCell` —
//! a `Cell` gives get-and-set with no borrow flag, no runtime check, and
//! crucially **no way to hold a borrow across a recursive call by mistake**.
//!
//! That last point is the reason this variant exists. The `RefCell` version is
//! correct only because every read is confined to one statement, and nothing
//! enforces that: a later edit that writes
//!
//! ```ignore
//! let memo = self.memo.borrow();
//! if memo[i].is_none() { let t = self.compute(symbol); /* panic */ }
//! ```
//!
//! compiles cleanly and panics at run time, on some inputs, in a subsystem that
//! recurses through dozens of functions. Making the common table a `Cell` removes
//! that failure mode for the hot path.
//!
//! The intern map still needs `RefCell` — a `HashMap` is not `Copy` — so the
//! hazard is reduced rather than eliminated, and that is the honest description.

use std::cell::{Cell, RefCell};

use rustc_hash::FxHashMap;
use tsr_core::Arena;

use crate::{
    Resolved,
    program::{Decl, Prim, Program, SymbolId},
    refs_refcell::Type,
};

#[derive(PartialEq, Eq, Hash)]
enum Key {
    /// A primitive type.
    Prim(Prim),
    Union(Vec<usize>),
    Array(usize),
    /// Produced when a definition refers to itself.
    Circular,
}

fn address(ty: &Type<'_>) -> usize {
    std::ptr::from_ref(ty) as usize
}

/// A checker in this style, over one program.
pub struct Checker<'a, 'p> {
    arena: &'a Arena,
    program: &'p Program,
    intern: RefCell<FxHashMap<Key, &'a Type<'a>>>,
    /// `Cell`, not `RefCell`: the value is `Copy`, so this is a plain load/store
    /// and there is no borrow to hold across the recursion.
    memo: Vec<Cell<Option<&'a Type<'a>>>>,
    /// A depth-keyed stack still needs `RefCell`; it is pushed and popped around
    /// the recursion rather than read during it, so the window is tight.
    resolving: RefCell<Vec<SymbolId>>,
    computations: Cell<usize>,
}

impl<'a, 'p> Checker<'a, 'p> {
    #[must_use]
    /// A checker allocating types into `arena`.
    pub fn new(arena: &'a Arena, program: &'p Program) -> Self {
        Self {
            arena,
            program,
            intern: RefCell::new(FxHashMap::default()),
            memo: (0..program.len()).map(|_| Cell::new(None)).collect(),
            resolving: RefCell::new(Vec::new()),
            computations: Cell::new(0),
        }
    }

    /// The type of `symbol`, computing it if this is the first request.
    pub fn type_of(&self, symbol: SymbolId) -> &'a Type<'a> {
        if let Some(ty) = self.memo[symbol.index()].get() {
            return ty;
        }
        if self.resolving.borrow().contains(&symbol) {
            return self.intern(Key::Circular, || Type::Circular);
        }

        self.resolving.borrow_mut().push(symbol);
        self.computations.set(self.computations.get() + 1);
        let resolved = self.compute(symbol);
        self.resolving.borrow_mut().pop();

        self.memo[symbol.index()].set(Some(resolved));
        resolved
    }

    fn compute(&self, symbol: SymbolId) -> &'a Type<'a> {
        match self.program.decl(symbol) {
            Decl::Prim(p) => {
                let p = *p;
                self.intern(Key::Prim(p), || Type::Prim(p))
            }
            Decl::AliasOf(target) => self.type_of(*target),
            Decl::Array(element) => {
                let element = self.type_of(*element);
                self.intern(Key::Array(address(element)), || Type::Array(element))
            }
            Decl::Union(members) => {
                let mut resolved: Vec<&'a Type<'a>> =
                    members.iter().map(|m| self.type_of(*m)).collect();
                resolved.sort_unstable_by_key(|t| address(t));
                resolved.dedup_by_key(|t| address(t));
                if resolved.len() == 1 {
                    return resolved[0];
                }
                let key = Key::Union(resolved.iter().map(|t| address(t)).collect());
                self.intern(key, || Type::Union(self.arena.alloc_slice(&resolved)))
            }
        }
    }

    fn intern(&self, key: Key, build: impl FnOnce() -> Type<'a>) -> &'a Type<'a> {
        if let Some(existing) = self.intern.borrow().get(&key) {
            return existing;
        }
        let allocated: &'a Type<'a> = self.arena.alloc(build());
        self.intern.borrow_mut().insert(key, allocated);
        allocated
    }
}

fn describe(ty: &Type<'_>) -> Resolved {
    match ty {
        Type::Prim(p) => Resolved::Prim(*p),
        Type::Array(inner) => Resolved::Array(Box::new(describe(inner))),
        Type::Circular => Resolved::Circular,
        Type::Union(members) => {
            let mut described: Vec<Resolved> = members.iter().map(|m| describe(m)).collect();
            described.sort_by_key(|r| format!("{r:?}"));
            Resolved::Union(described)
        }
    }
}

/// Resolve every symbol without describing the result.
///
/// What the benchmark times; see the note on `handles::resolve_only`.
#[must_use]
pub fn resolve_only(program: &Program) -> usize {
    let arena = Arena::new();
    let checker = Checker::new(&arena, program);
    let mut last = 0;
    for i in 0..program.len() {
        last =
            address(checker.type_of(SymbolId(u32::try_from(i).expect("symbol count exceeds u32"))));
    }
    last
}

/// Resolve every symbol, in order.
#[must_use]
pub fn resolve_all(program: &Program) -> Vec<Resolved> {
    let arena = Arena::new();
    let checker = Checker::new(&arena, program);
    (0..program.len())
        .map(|i| {
            describe(checker.type_of(SymbolId(u32::try_from(i).expect("symbol count exceeds u32"))))
        })
        .collect()
}
