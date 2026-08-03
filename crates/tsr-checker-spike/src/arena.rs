//! Styles (a) and (c): `&self` methods, types in the bump arena, memo tables
//! behind `RefCell`.
//!
//! The issue lists these as separate options — "append-only arenas with interior
//! mutability, elsa-style" and "`RefCell`-guarded memo tables separate from the
//! arena" — but for us they collapse into one, and that is a finding rather than
//! a shortcut. `tsr_core::Arena::alloc` already takes `&self` and returns a
//! reference valid for the arena's lifetime, which is exactly the property elsa
//! exists to provide. We do not need elsa because we already have the arena.
//!
//! What that buys, and it is the thing style [`crate::ids`] cannot do: a resolved
//! `&'a Type<'a>` can be held *across* a recursive call, because its lifetime is
//! the arena's and not a borrow of the checker. Reading a type's contents does not
//! go back through `self`.
//!
//! What it costs is interior mutability, which moves two invariants from compile
//! time to run time:
//!
//! - A `RefCell` borrow held across a recursive call panics. Every read below
//!   therefore ends in the statement that starts it, and that discipline is
//!   invisible to the compiler.
//! - The arena never frees, so a type computed and then discarded stays. The
//!   checker is a per-file, per-thread object, so this bounds at "everything one
//!   check computed", which is what upstream's arenas do too.

use std::cell::RefCell;

use rustc_hash::FxHashMap;
use tsr_core::Arena;

use crate::{
    Resolved,
    program::{Decl, Prim, Program, SymbolId},
};

/// A resolved type, living in the arena.
///
/// Members are `&'a Type<'a>`: once you have one it stays valid, so a caller can
/// hold it across further resolution.
#[derive(Debug)]
/// A resolved type living in the arena.
pub enum Type<'a> {
    /// A primitive type.
    Prim(Prim),
    /// A union, members in interning order.
    Union(&'a [&'a Type<'a>]),
    /// An array of the element type.
    Array(&'a Type<'a>),
    /// Produced when a definition refers to itself.
    Circular,
}

/// A structural key for interning, since `Type` contains references.
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
    memo: RefCell<Vec<Option<&'a Type<'a>>>>,
    resolving: RefCell<Vec<SymbolId>>,
    computations: std::cell::Cell<usize>,
}

impl<'a, 'p> Checker<'a, 'p> {
    #[must_use]
    /// A checker allocating types into `arena`.
    pub fn new(arena: &'a Arena, program: &'p Program) -> Self {
        Self {
            arena,
            program,
            intern: RefCell::new(FxHashMap::default()),
            memo: RefCell::new(vec![None; program.len()]),
            resolving: RefCell::new(Vec::new()),
            computations: std::cell::Cell::new(0),
        }
    }

    /// The type of `symbol`, computing it if this is the first request.
    ///
    /// Note `&self`: the recursion below re-enters through a shared reference, so
    /// there is no borrow-of-`self` to conflict with. The conflicts move into the
    /// `RefCell`s, and every borrow of one is confined to a single statement.
    pub fn type_of(&self, symbol: SymbolId) -> &'a Type<'a> {
        // The borrow ends at the semicolon. Holding it across the recursion below
        // would panic at runtime — this is the invariant the compiler is no longer
        // checking for us.
        let cached = self.memo.borrow()[symbol.index()];
        if let Some(ty) = cached {
            return ty;
        }
        if self.resolving.borrow().contains(&symbol) {
            return self.intern(Key::Circular, || Type::Circular);
        }

        self.resolving.borrow_mut().push(symbol);
        self.computations.set(self.computations.get() + 1);
        let resolved = self.compute(symbol);
        self.resolving.borrow_mut().pop();

        self.memo.borrow_mut()[symbol.index()] = Some(resolved);
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
                // `element` is a `&'a Type`, held across the `intern` call below.
                // This is the thing style `ids` cannot express without going back
                // through `self` for the contents.
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
                self.intern(key, || {
                    let slice: &'a [&'a Type<'a>] = self.arena.alloc_slice(&resolved);
                    Type::Union(slice)
                })
            }
        }
    }

    /// Look up `key`, or build and record a type for it.
    ///
    /// `build` is a closure rather than a value because constructing the type
    /// allocates, and on the hit path there is nothing to construct.
    fn intern(&self, key: Key, build: impl FnOnce() -> Type<'a>) -> &'a Type<'a> {
        if let Some(existing) = self.intern.borrow().get(&key) {
            return existing;
        }
        // `build` may allocate in the arena; no `RefCell` borrow is live here,
        // which matters because a union's construction allocates a slice.
        let allocated: &'a Type<'a> = self.arena.alloc(build());
        self.intern.borrow_mut().insert(key, allocated);
        allocated
    }
}

/// Structural view, for comparing styles.
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
/// What the benchmark times; see the note on `ids::resolve_only`.
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
