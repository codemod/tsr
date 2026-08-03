//! The input the three styles share: symbols and their declarations.
//!
//! Deliberately tiny. The spike is about the *shape* of lazy memoised
//! computation, not about type theory, so the declaration forms are the smallest
//! set that reproduces every structural difficulty:
//!
//! | Form | What it forces |
//! |---|---|
//! | `Prim` | a base case |
//! | `AliasOf` | recursion, and the simplest circularity |
//! | `Array` | a constructed type built from one resolved operand |
//! | `Union` | several operands resolved into a `Vec`, then interned — the case where a naive borrow of the memo table is still live while the intern table is written |

/// Builds a `SymbolId` from a `usize` index.
///
/// Test-program construction only, where the counts are hundreds or thousands and
/// the truncation the lint warns about cannot occur.
#[allow(clippy::cast_possible_truncation)]
const fn sym(i: usize) -> SymbolId {
    SymbolId(i as u32)
}

/// Index of a symbol, dense from zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SymbolId(pub u32);

impl SymbolId {
    #[must_use]
    /// As a `usize`, for indexing.
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// A primitive type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Prim {
    /// `number`
    Number,
    /// `string`
    String,
    /// `boolean`
    Boolean,
}

/// What a symbol's type is written as.
#[derive(Debug, Clone)]
pub enum Decl {
    /// A primitive type.
    Prim(Prim),
    /// `type A = B`
    AliasOf(SymbolId),
    /// `type A = B[]`
    Array(SymbolId),
    /// `type A = B | C`
    Union(Vec<SymbolId>),
}

/// A set of symbol declarations, indexed by [`SymbolId`].
#[derive(Debug, Clone)]
pub struct Program {
    /// Name, for benchmark and failure output.
    pub name: &'static str,
    /// One declaration per symbol.
    pub decls: Vec<Decl>,
}

impl Program {
    #[must_use]
    /// How many symbols.
    pub fn len(&self) -> usize {
        self.decls.len()
    }

    #[must_use]
    /// Whether there are no symbols.
    pub fn is_empty(&self) -> bool {
        self.decls.is_empty()
    }

    #[must_use]
    /// The declaration of `symbol`.
    pub fn decl(&self, symbol: SymbolId) -> &Decl {
        &self.decls[symbol.index()]
    }

    /// `type A = B; type B = A;` — the minimal circularity.
    #[must_use]
    pub fn circular_pair() -> Self {
        Self {
            name: "circular_pair",
            decls: vec![Decl::AliasOf(SymbolId(1)), Decl::AliasOf(SymbolId(0))],
        }
    }

    /// A layered graph where every symbol is referenced by many later ones.
    ///
    /// Without memoisation this is exponential, which is what makes it the right
    /// shape for checking that all three styles memoise rather than merely agree.
    #[must_use]
    pub fn diamond(depth: usize) -> Self {
        let mut decls = vec![Decl::Prim(Prim::Number), Decl::Prim(Prim::String)];
        for level in 0..depth {
            let base = decls.len();
            // Each level unions the two symbols below it, so the number of paths
            // to the root doubles per level while the number of symbols does not.
            decls.push(Decl::Union(vec![sym(base - 2), sym(base - 1)]));
            decls.push(Decl::Array(sym(base - 1)));
            let _ = level;
        }
        Self { name: "diamond", decls }
    }

    /// A wide, shallow program: many independent symbols, the common real case.
    #[must_use]
    pub fn wide(count: usize) -> Self {
        let mut decls =
            vec![Decl::Prim(Prim::Number), Decl::Prim(Prim::String), Decl::Prim(Prim::Boolean)];
        for i in 3..count {
            let a = sym(i % 3);
            let b = sym((i - 1) % i.max(1));
            decls.push(match i % 4 {
                0 => Decl::AliasOf(a),
                1 => Decl::Array(a),
                2 => Decl::Union(vec![a, b]),
                _ => Decl::Union(vec![a, b, sym(i - 2)]),
            });
        }
        Self { name: "wide", decls }
    }

    /// A deep alias chain — the worst case for recursion depth.
    #[must_use]
    pub fn chain(depth: usize) -> Self {
        let mut decls = vec![Decl::Prim(Prim::Number)];
        for i in 1..depth {
            decls.push(Decl::AliasOf(sym(i - 1)));
        }
        Self { name: "chain", decls }
    }

    /// A cycle buried inside an otherwise resolvable program.
    #[must_use]
    pub fn cycle_in_the_middle(count: usize) -> Self {
        let mut program = Self::wide(count);
        program.name = "cycle_in_the_middle";
        let mid = count / 2;
        program.decls[mid] = Decl::AliasOf(sym(mid + 1));
        program.decls[mid + 1] = Decl::AliasOf(sym(mid));
        program
    }
}

/// The programs every style is checked against.
#[must_use]
pub fn fixtures() -> Vec<Program> {
    vec![
        Program::circular_pair(),
        Program::diamond(10),
        Program::wide(200),
        Program::chain(64),
        Program::cycle_in_the_middle(200),
    ]
}
