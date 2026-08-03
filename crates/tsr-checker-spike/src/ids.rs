//! Style (b): `&mut self`, ids everywhere, no long-lived `&Type`.
//!
//! The rule is that a method never holds a borrow of any field across a call that
//! needs `self` again. Everything that crosses a call boundary is a `TypeId`,
//! which is `Copy`, so there is nothing to keep borrowed.
//!
//! The Go pattern
//!
//! ```go
//! links := c.links.Get(symbol)      // borrow
//! if links.resolvedType == nil {
//!     t := c.worker(symbol)         // recurse
//!     links.resolvedType = t        // write through the earlier borrow
//! }
//! ```
//!
//! becomes read, drop, recurse, write:
//!
//! ```ignore
//! if let Some(t) = self.memo[i] { return t }   // read ends here
//! let t = self.compute(symbol);                // recursion owns `self`
//! self.memo[i] = Some(t);                      // fresh borrow
//! ```
//!
//! The cost is a second lookup on the miss path, and the discipline that a type's
//! contents can only be reached through `self`. The benefit is that this is
//! ordinary safe Rust with no interior mutability anywhere, so nothing about it
//! can be got subtly wrong at runtime.

use rustc_hash::FxHashMap;

use crate::{
    Resolved,
    program::{Decl, Prim, Program, SymbolId},
};

/// Index of a type in the checker's type list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(u32);

/// A resolved type. Members are `TypeId`s, never references.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Type {
    /// A primitive type.
    Prim(Prim),
    /// A union, members sorted and deduplicated.
    Union(Vec<TypeId>),
    /// An array of the element type.
    Array(TypeId),
    /// Produced when a definition refers to itself.
    Circular,
}

/// A checker in this style, over one program.
pub struct Checker<'p> {
    program: &'p Program,
    types: Vec<Type>,
    /// Structural deduplication, as upstream interns unions.
    intern: FxHashMap<Type, TypeId>,
    /// `symbol -> resolved type`, the memo table.
    memo: Vec<Option<TypeId>>,
    /// Symbols currently being resolved, for circularity detection. Upstream's
    /// `pushTypeResolution`/`popTypeResolution`.
    resolving: Vec<SymbolId>,
    /// How many times a symbol was actually computed, to prove memoisation.
    computations: usize,
}

impl<'p> Checker<'p> {
    #[must_use]
    /// A checker over `program`.
    pub fn new(program: &'p Program) -> Self {
        Self {
            program,
            types: Vec::new(),
            intern: FxHashMap::default(),
            memo: vec![None; program.len()],
            resolving: Vec::new(),
            computations: 0,
        }
    }

    /// The type of `symbol`, computing it if this is the first request.
    pub fn type_of(&mut self, symbol: SymbolId) -> TypeId {
        // The read ends with this statement — `TypeId` is `Copy`, so no borrow of
        // `self.memo` survives into the recursion below.
        if let Some(cached) = self.memo[symbol.index()] {
            return cached;
        }
        if self.resolving.contains(&symbol) {
            // Circular. Deliberately *not* memoised: upstream reports the error at
            // each site that participates in the cycle, and caching the error type
            // here would silence all but the first.
            return self.intern(Type::Circular);
        }

        self.resolving.push(symbol);
        self.computations += 1;
        let resolved = self.compute(symbol);
        self.resolving.pop();

        self.memo[symbol.index()] = Some(resolved);
        resolved
    }

    fn compute(&mut self, symbol: SymbolId) -> TypeId {
        match self.program.decl(symbol).clone() {
            Decl::Prim(p) => self.intern(Type::Prim(p)),
            Decl::AliasOf(target) => self.type_of(target),
            Decl::Array(element) => {
                let element = self.type_of(element);
                self.intern(Type::Array(element))
            }
            Decl::Union(members) => {
                // Each member is resolved through `&mut self`; the results are
                // `TypeId`s, so accumulating them borrows nothing.
                let mut resolved: Vec<TypeId> = members.iter().map(|m| self.type_of(*m)).collect();
                resolved.sort_unstable_by_key(|t| t.0);
                resolved.dedup();
                if resolved.len() == 1 {
                    return resolved[0];
                }
                self.intern(Type::Union(resolved))
            }
        }
    }

    fn intern(&mut self, ty: Type) -> TypeId {
        if let Some(existing) = self.intern.get(&ty) {
            return *existing;
        }
        let id = TypeId(u32::try_from(self.types.len()).expect("type count exceeds u32"));
        self.types.push(ty.clone());
        self.intern.insert(ty, id);
        id
    }

    /// Structural view of a type, for comparing styles.
    fn describe(&self, id: TypeId) -> Resolved {
        match &self.types[id.0 as usize] {
            Type::Prim(p) => Resolved::Prim(*p),
            Type::Array(inner) => Resolved::Array(Box::new(self.describe(*inner))),
            Type::Circular => Resolved::Circular,
            Type::Union(members) => {
                let mut described: Vec<Resolved> =
                    members.iter().map(|m| self.describe(*m)).collect();
                described.sort_by_key(|r| format!("{r:?}"));
                Resolved::Union(described)
            }
        }
    }
}

/// Resolve every symbol without describing the result.
///
/// What the benchmark times. `resolve_all` additionally walks every type to build
/// a structural description, which is identical in all three styles and large
/// enough to hide the differences the spike exists to measure.
#[must_use]
pub fn resolve_only(program: &Program) -> usize {
    let mut checker = Checker::new(program);
    let mut last = 0;
    for i in 0..program.len() {
        last = checker.type_of(SymbolId(u32::try_from(i).expect("symbol count exceeds u32"))).0
            as usize;
    }
    last
}

/// Resolve every symbol, in order.
#[must_use]
pub fn resolve_all(program: &Program) -> Vec<Resolved> {
    resolve_all_counting(program).0
}

/// As [`resolve_all`], also reporting how many symbols were actually computed.
#[must_use]
pub fn resolve_all_counting(program: &Program) -> (Vec<Resolved>, usize) {
    let mut checker = Checker::new(program);
    let ids: Vec<TypeId> = (0..program.len())
        .map(|i| checker.type_of(SymbolId(u32::try_from(i).expect("symbol count exceeds u32"))))
        .collect();
    let described = ids.iter().map(|id| checker.describe(*id)).collect();
    (described, checker.computations)
}
