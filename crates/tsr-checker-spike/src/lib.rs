//! A vertical slice of the checker's memoisation problem, in three styles.
//!
//! Spike for `bd tsr-6n3`. Not part of the compiler; it exists to settle one
//! question by working code before that question constrains ~60k lines:
//!
//! > The checker lazily computes and caches types while holding references into
//! > its own arenas. What does that look like in Rust?
//!
//! # The pattern being modelled
//!
//! Every lazily-computed value in typescript-go's checker has this shape
//! (`internal/checker/checker.go:16544`):
//!
//! ```go
//! func (c *Checker) getTypeOfVariableOrParameterOrProperty(symbol *ast.Symbol) *Type {
//!     links := c.valueSymbolLinks.Get(symbol)   // 1. mutable borrow of a side table
//!     if links.resolvedType == nil {
//!         t := c.getTypeOfVariableOrParameterOrPropertyWorker(symbol)  // 2. recurses into c
//!         links.resolvedType = t                // 3. writes through the borrow from (1)
//!         return t
//!     }
//!     return links.resolvedType
//! }
//! ```
//!
//! Steps 1–3 are what Rust rejects: a borrow of `self`'s field held across a call
//! that needs `self` again. There is no way to write it directly, and the checker
//! does this in dozens of places, so the workaround is not a local matter of
//! taste — it is the calling convention for the whole subsystem.
//!
//! Around it sit three more things that any candidate has to survive, because
//! each of them is what breaks the easy answers:
//!
//! - **Mutual recursion.** Resolving one symbol's type resolves others.
//! - **Circularity.** `type A = B; type B = A;` must produce an error type rather
//!   than recursing forever, which needs a resolution stack that is *also*
//!   mutable state on the checker, borrowed at the same time as the memo table.
//! - **Interning.** Unions are deduplicated through a hash map, so a *second*
//!   mutable table is written during the same traversal, while results from the
//!   first are live.
//!
//! # The three styles
//!
//! Named as in the issue:
//!
//! - [`ids`] — (b) id-returning `&mut self` methods that never hand out a
//!   long-lived `&Type`.
//! - [`arena`] — (a)/(c) `&self` methods, types in the bump arena, memo tables
//!   behind `RefCell`.
//! - [`cells`] — (c) refined: the same, with the memo table as `Cell<Option<..>>`
//!   so there is no runtime borrow to get wrong.
//!
//! See `docs/adr/0013-checker-memoisation.md` for the recommendation.

pub mod arena;
pub mod cells;
pub mod ids;
pub mod program;

pub use program::{Decl, Prim, Program, SymbolId};

/// The answer every style must produce, so they can be compared.
///
/// A structural summary rather than a type id: ids are allocation order, which
/// differs legitimately between styles, while the resolved shape must not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// A primitive type.
    Prim(Prim),
    /// Members in a canonical order, so interning differences do not show up as
    /// disagreements.
    Union(Vec<Resolved>),
    /// An array of the inner type.
    Array(Box<Resolved>),
    /// The result of a circular definition.
    Circular,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three styles must agree on every symbol of every shape.
    ///
    /// This is the load-bearing test of the spike: a style that is fast because it
    /// computes something subtly different is not a candidate, and with three
    /// independent implementations that is easy to do by accident.
    #[test]
    fn the_three_styles_agree() {
        for program in program::fixtures() {
            let by_ids = ids::resolve_all(&program);
            let by_arena = arena::resolve_all(&program);
            let by_cells = cells::resolve_all(&program);
            assert_eq!(by_ids, by_arena, "ids vs arena on {}", program.name);
            assert_eq!(by_ids, by_cells, "ids vs cells on {}", program.name);
        }
    }

    #[test]
    fn circular_definitions_resolve_to_an_error_rather_than_hanging() {
        let program = Program::circular_pair();
        let resolved = ids::resolve_all(&program);
        assert!(
            resolved.iter().any(|r| matches!(r, Resolved::Circular)),
            "expected a circularity, got {resolved:?}"
        );
    }

    #[test]
    fn a_symbol_is_computed_once_however_many_times_it_is_asked_for() {
        // Memoisation is the point; a style that recomputes would still pass the
        // agreement test above while being exponential on a diamond graph.
        let program = Program::diamond(12);
        let (_, computations) = ids::resolve_all_counting(&program);
        assert_eq!(computations, program.len(), "each symbol should be computed exactly once");
    }
}
