//! The binder: symbols, scopes, and the control-flow graph.
//!
//! Walks a parsed file, creates a [`Symbol`] for every declaration, puts each one
//! in the table its scope demands — which is where `var` hoisting, block scoping,
//! and declaration merging actually happen — and builds the flow graph the
//! checker narrows against.
//!
//! Ported from `internal/binder/binder.go` at the pinned commit.
//!
//! # Shape
//!
//! Four decisions taken earlier determine most of this:
//!
//! - **Symbols and flow nodes are handles.** [`SymbolId`] and [`FlowId`] index
//!   contiguous stores, not pointers
//!   ([ADR-0013](../../../docs/adr/0013-checker-memoisation.md)).
//! - **A node's symbol and flow node are in side tables**, not on the node.
//!   Upstream puts both on the node; we cannot, because a mutable field on a
//!   shared node is what [ADR-0012](../../../docs/adr/0012-ast-is-sync.md)
//!   forbids — and the checker will read these from several threads at once.
//! - **The result is immutable once built**, for the same reason.
//! - **The flow graph is a subset of the CFG, not the CFG.** Only branches that
//!   could change a type get a node; see [`narrowing`] for the filter and
//!   [`docs/architecture/binder.md`](../../../docs/architecture/binder.md) for
//!   why that is the right shape.
//!
//! # What is not built yet
//!
//! Named here rather than left to be discovered:
//!
//! - **Late-bound property names.** `{ ['a']: 1 }` names a symbol statically and
//!   does declare `a`, but `[Symbol.iterator]` and `[k]` name whatever the
//!   expression evaluates to. The binder declares nothing for those: a symbol
//!   under a guessed name would be unreachable by any reference. Upstream gives
//!   them an internal `__computed` name and resolves them in the checker.
//! - **Module vs script.** A *namespace* routes its exported members into its own
//!   symbol, but a source file still binds as a script, so top-level declarations
//!   are locals rather than exports of a module symbol. Telling the two apart
//!   needs module resolution.
//! - **Optional chains.** The flow shapes are ported in full, but the parser does
//!   not set [`tsr_ast::NodeFlags::OPTIONAL_CHAIN`], so `a?.b` currently gets the
//!   graph of `a.b` and loses the narrowing that the `?.` implies.
//! - **Strict-mode and contextual-identifier diagnostics.** Upstream's binder
//!   reports `with` in strict mode, `eval`/`arguments` misuse, and octal
//!   literals; none of that is ported.
//!
//! - **Alias resolution.** `import X = Y` declares `X` as an alias and stops
//!   there; what `X` refers to needs a resolver. Upstream's baselines resolve
//!   through it, which is the ceiling on `binder_symbols` for those cases.
//!
//! Each is a `bd` issue under the binder epic.

mod binder;
mod container;
mod flow;
mod narrowing;
mod symbol;

use rustc_hash::FxHashMap;
use tsr_ast::{NodeId, NodeTable, SourceFile};
use tsr_core::Idx as _;
use tsr_diagnostics::Diagnostic;

pub use container::{ContainerFlags, container_flags};
pub use flow::{Antecedents, FlowFlags, FlowId, FlowStore, ReduceLabel, SwitchClause};
pub use symbol::{Symbol, SymbolFlags, SymbolId, SymbolStore, SymbolTable};

bitflags::bitflags! {
    /// Per-node conclusions the binder reaches on its way through.
    ///
    /// Upstream writes each of these into `node.Flags` during binding. The AST
    /// here is immutable ([ADR-0012](../../../docs/adr/0012-ast-is-sync.md)) and
    /// the binder holds only `&NodeTable`, so they land in a map instead — a map
    /// rather than a dense column because a file with no unreachable code and no
    /// `this` should pay nothing for the possibility, and most files are that
    /// file.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct NodeFacts: u8 {
        /// Control never reaches this node.
        ///
        /// Set only on nodes that would actually *run* — see
        /// `narrowing::is_potentially_executable_node`. A type alias after a
        /// `return` is unreachable and uninteresting.
        const UNREACHABLE = 1 << 0;
        /// A function-like node whose body can fall off the end.
        const HAS_IMPLICIT_RETURN = 1 << 1;
        /// A function-like node containing a `return` statement.
        const HAS_EXPLICIT_RETURN = 1 << 2;
        /// A node whose own `this` is referred to somewhere inside it.
        const CONTAINS_THIS = 1 << 3;
        /// A `label:` that nothing `break`s or `continue`s to.
        const UNUSED_LABEL = 1 << 4;
    }
}

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
    flow: FlowStore,
    node_flow: Vec<Option<FlowId>>,
    facts: FxHashMap<NodeId, NodeFacts>,
    end_flow: FxHashMap<NodeId, FlowId>,
    return_flow: FxHashMap<NodeId, FlowId>,
    fallthrough_flow: FxHashMap<NodeId, FlowId>,
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

    /// Every flow node in the file.
    #[must_use]
    pub fn flow(&self) -> &FlowStore {
        &self.flow
    }

    /// The flow node in effect where `node` appears.
    ///
    /// `None` for a node the checker never asks about, and for one in
    /// unreachable code — those two cases are distinguished by
    /// [`BindResult::facts`], which carries [`NodeFacts::UNREACHABLE`] for the
    /// second.
    #[must_use]
    pub fn flow_of(&self, node: NodeId) -> Option<FlowId> {
        self.node_flow.get(node.index()).copied().flatten()
    }

    /// What the binder concluded about `node`.
    #[must_use]
    pub fn facts(&self, node: NodeId) -> NodeFacts {
        self.facts.get(&node).copied().unwrap_or_default()
    }

    /// The flow node at the end of a function-like node's body.
    ///
    /// Upstream's `BodyBase.EndFlowNode`. `None` when the end is unreachable,
    /// which is how "every path returns" is spelled.
    #[must_use]
    pub fn end_flow(&self, function: NodeId) -> Option<FlowId> {
        self.end_flow.get(&function).copied()
    }

    /// The merged flow of every `return` in a constructor or static block.
    ///
    /// Upstream's `ReturnFlowNode`; strict property-initialisation checking
    /// walks it.
    #[must_use]
    pub fn return_flow(&self, node: NodeId) -> Option<FlowId> {
        self.return_flow.get(&node).copied()
    }

    /// The flow that falls through from a `case` clause into the next one.
    ///
    /// `None` when the clause cannot fall through, which is what makes
    /// fallthrough reportable.
    #[must_use]
    pub fn fallthrough_flow(&self, clause: NodeId) -> Option<FlowId> {
        self.fallthrough_flow.get(&clause).copied()
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

    /// Bytes of heap the result holds, for the memory reporting in
    /// `docs/architecture/performance.md`.
    ///
    /// Approximate for the hash maps, whose spare capacity is not observable;
    /// exact for the dense columns, which are what dominate.
    #[must_use]
    pub fn heap_bytes(&self) -> usize {
        self.node_symbols.capacity() * size_of::<Option<SymbolId>>()
            + self.node_flow.capacity() * size_of::<Option<FlowId>>()
            + self.flow.heap_bytes()
            + self.locals.capacity() * (size_of::<NodeId>() + size_of::<SymbolTable<'a>>())
            + self.facts.capacity() * (size_of::<NodeId>() + size_of::<NodeFacts>())
    }
}

/// Bind a parsed file.
#[must_use]
pub fn bind<'a>(file: &'a SourceFile<'a>, nodes: &NodeTable) -> BindResult<'a> {
    binder::Binder::new(nodes).bind_source_file(file)
}
