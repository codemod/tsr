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
//! It links no `oxc_*` crate. The designs are borrowed; the code is ours. See
//! PLAN.md §3.1 for why.

pub mod arena;
pub mod index;
pub mod options;
pub mod side_table;
pub mod span;

pub use arena::Arena;
pub use index::{Idx, IndexVec};
pub use options::{
    CompilerOptions, JsxEmit, ModuleKind, ModuleResolutionKind, OrderedMap, ResolutionMode,
    ScriptTarget, Tristate,
};
pub use side_table::PagedTable;
pub use span::{GetSpan, Span};
