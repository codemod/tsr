//! Foundational data structures shared by every stage of the compiler.
//!
//! This crate is deliberately small. It holds the three things the architecture in
//! PLAN.md §3 rests on:
//!
//! - [`span`] — byte-offset source positions.
//! - [`index`] — typed newtype indices and index-keyed vectors, the replacement for
//!   Go's pervasive pointer graph.
//! - [`side_table`] — dense and sparse id-keyed tables, where everything cyclic
//!   lives.
//!
//! It also holds [`stack`], which is not a data structure but belongs here for the
//! same reason: every stage recurses over the tree, and all of them need the same
//! answer to "what happens when the input is nested deeper than the stack".
//!
//! It links no `oxc_*` crate. The designs are borrowed; the code is ours. See
//! PLAN.md §3.1 for why.

pub mod arena;
pub mod index;
pub mod options;
pub mod side_table;
pub mod span;
pub mod stack;

pub use arena::Arena;
pub use index::{Idx, IndexVec};
pub use options::{
    CompilerOptions, JsxEmit, ModuleKind, ModuleResolutionKind, OrderedMap, ResolutionMode,
    ScriptTarget, Tristate,
};
pub use side_table::PagedTable;
pub use span::{GetSpan, Span};
