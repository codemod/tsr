//! The binder: symbols and scopes.
//!
//! Walks a parsed file, creates a [`Symbol`] for every declaration, and puts each
//! one in the table its scope demands — which is where `var` hoisting, block
//! scoping, and declaration merging actually happen.
//!
//! Ported from `internal/binder/binder.go` at the pinned commit.
//!
//! # Shape
//!
//! Three decisions taken earlier determine most of this:
//!
//! - **Symbols are handles.** [`SymbolId`] into a contiguous store, not pointers
//!   ([ADR-0013](../../../docs/adr/0013-checker-memoisation.md)).
//! - **A node's symbol is in a side table**, not on the node. Upstream puts it on
//!   the node; we cannot, because a mutable field on a shared node is what
//!   [ADR-0012](../../../docs/adr/0012-ast-is-sync.md) forbids — and the checker
//!   will read this from several threads at once.
//! - **The result is immutable once built**, for the same reason.
//!
//! # What is not built yet
//!
//! Named here rather than left to be discovered:
//!
//! - **The control-flow graph.** [`ContainerFlags::IS_CONTROL_FLOW_CONTAINER`] is
//!   computed faithfully and nothing consumes it. Flow nodes are what narrowing
//!   needs and are roughly half of upstream's binder; symbols come first so the
//!   checker has something to resolve against.
//! - **Destructuring patterns.** `const { a, b } = x` declares two symbols;
//!   [`declaration_name`](binder) returns `None` for a binding pattern, so it
//!   currently declares none.
//! - **Computed property names.** `{ [k]: 1 }` declares a symbol with a
//!   late-bound name; skipped for the same reason.
//! - **Module vs script.** Every file binds as a script, so top-level
//!   declarations are locals rather than exports of a module symbol.
//!   Distinguishing them needs module resolution.
//! - **`export` handling.** An `export` modifier does not yet route a declaration
//!   into an exports table.
//!
//! Each is a `bd` issue under the binder epic.

mod binder;
mod container;
mod symbol;

use rustc_hash::FxHashMap;
use tsr_ast::{NodeId, NodeTable, SourceFile};
use tsr_core::Idx as _;
use tsr_diagnostics::Diagnostic;

pub use container::{ContainerFlags, container_flags};
pub use symbol::{Symbol, SymbolFlags, SymbolId, SymbolStore, SymbolTable};

/// What the binder produced.
///
/// Read-only by construction: there is no method taking `&mut self`, so once this
/// is returned it can be shared across threads. That is the property the parallel
/// checker needs, and the reason nothing here hangs off the AST.
#[derive(Debug)]
pub struct BindResult<'a> {
    symbols: SymbolStore<'a>,
    node_symbols: Vec<Option<SymbolId>>,
    locals: FxHashMap<NodeId, SymbolTable<'a>>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> BindResult<'a> {
    /// Every symbol created.
    #[must_use]
    pub fn symbols(&self) -> &SymbolStore<'a> {
        &self.symbols
    }

    /// The symbol `node` declares, if it declares one.
    #[must_use]
    pub fn symbol_of(&self, node: NodeId) -> Option<SymbolId> {
        self.node_symbols.get(node.index()).copied().flatten()
    }

    /// The scope table owned by `container`, if it owns one.
    #[must_use]
    pub fn locals(&self, container: NodeId) -> Option<&SymbolTable<'a>> {
        self.locals.get(&container)
    }

    /// Duplicate-identifier and related errors, in discovery order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Look a name up in `container`'s own scope, without walking outward.
    #[must_use]
    pub fn lookup_local(&self, container: NodeId, name: &str) -> Option<SymbolId> {
        self.locals.get(&container)?.get(name).copied()
    }

    /// Resolve `name` from `start`, walking outward through enclosing scopes.
    ///
    /// The lexical half of name resolution. It does not consult members, exports,
    /// or globals, and it does not implement the checker's meaning-based filtering
    /// (a type reference must not resolve to a value), so it is a foundation
    /// rather than the finished article — see `nameresolver.go` upstream.
    #[must_use]
    pub fn resolve(&self, nodes: &NodeTable, start: NodeId, name: &str) -> Option<SymbolId> {
        let mut current = Some(start);
        while let Some(node) = current {
            if let Some(found) = self.lookup_local(node, name) {
                return Some(found);
            }
            current = nodes.parent(node);
        }
        None
    }
}

/// Bind a parsed file.
#[must_use]
pub fn bind<'a>(file: &'a SourceFile<'a>, nodes: &NodeTable) -> BindResult<'a> {
    binder::Binder::new(nodes).bind_source_file(file)
}
