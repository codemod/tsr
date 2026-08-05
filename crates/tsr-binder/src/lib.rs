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
//! - **Late *binding*.** `[Symbol.iterator]` and `[k]` do get a symbol — an
//!   anonymous `__computed` one, as upstream creates — but nothing resolves the
//!   expression to a name, so `class C { a: string; [k]: number }` with `k` of
//!   `"a"` is two symbols here and one upstream. That is the checker's work.
//! - **A signed numeric literal name** (`[-1]`). Upstream treats it as static and
//!   builds the name by concatenating the operator with the operand; every name
//!   here borrows from the source, so that needs an arena allocation.
//! - **Constructor functions.** `function C() { this.x = 1 }` should declare `x`
//!   on `C`, and `C.prototype.m = …` should declare `m` on its prototype.
//!   Upstream marks both `!!!` — unimplemented — in the Go port, so neither is
//!   ported here.
//! - **`import.meta` as a module indicator.** A file with a top-level import or
//!   export binds as a module; upstream also counts a file that mentions
//!   `import.meta`, which needs a full-tree walk under module settings the binder
//!   does not have. See [`binder::is_external_module`].
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
//! - **Names that must be built rather than sliced.** `{ 0b11: x, 3: y }` is one
//!   member upstream, which normalises a numeric name to its value; here it is
//!   two, because every [`Symbol`] name is a borrow. See
//!   [ADR-0016](../../../docs/adr/0016-file-info-not-a-file-name.md).
//!
//! Each is a `bd` issue under the binder epic.

mod binder;
mod container;
mod flow;
mod narrowing;
mod symbol;

use rustc_hash::FxHashMap;
use tsr_ast::{NodeId, NodeMap, NodeTable, SourceFile, SyntaxKind};
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
    /// TEMPORARY instrumentation for `bd tsr-el3.1`.
    max_depth: u32,
    symbols: SymbolStore<'a>,
    node_symbols: Vec<Option<SymbolId>>,
    locals: FxHashMap<NodeId, SymbolTable<'a>>,
    global_exports: SymbolTable<'a>,
    computed_names: FxHashMap<NodeId, NodeId>,
    diagnostics: Vec<Diagnostic>,
    flow: FlowStore,
    node_flow: Vec<Option<FlowId>>,
    facts: FxHashMap<NodeId, NodeFacts>,
    end_flow: FxHashMap<NodeId, FlowId>,
    return_flow: FxHashMap<NodeId, FlowId>,
    fallthrough_flow: FxHashMap<NodeId, FlowId>,
}

impl<'a> BindResult<'a> {
    /// TEMPORARY instrumentation for `bd tsr-el3.1`: peak `bind()` recursion depth.
    #[must_use]
    pub fn max_depth(&self) -> u32 {
        self.max_depth
    }

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

    /// The global names a UMD module claims with `export as namespace N`.
    ///
    /// Upstream's `SourceFile.GlobalExports`. Empty for everything else.
    #[must_use]
    pub fn global_exports(&self) -> &SymbolTable<'a> {
        &self.global_exports
    }

    /// The `ComputedPropertyName` a declaration was written with, if it was.
    ///
    /// Recorded for every computed name, late-bound (`[k]`) or not (`['a']`),
    /// because a declaration is seen as a [`NodeId`] and the name is a child the
    /// tree has no edge to reach ([ADR-0003](../../../docs/adr/0003-tree-plus-side-tables.md)).
    /// The checker's late binding reads the expression back through this.
    #[must_use]
    pub fn computed_name(&self, declaration: NodeId) -> Option<NodeId> {
        self.computed_names.get(&declaration).copied()
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

    /// Resolve `name` from `start` for a particular *meaning*, walking outward
    /// through enclosing scopes.
    ///
    /// Ported from `(*NameResolver).Resolve` (`internal/binder/nameresolver.go`),
    /// restricted to the two arms this port has the tables for: a location's
    /// `locals`, and the **members** of an enclosing class, class expression, or
    /// interface.
    ///
    /// # Why the second arm is not an optimisation
    ///
    /// A class's or an interface's type parameters are not in any `locals` table
    /// — here or upstream. `declareSymbolAndAddToSymbolTable` switches on the
    /// *container's* kind and sends a declaration inside a class or an interface
    /// to `GetMembers(container.Symbol())` (`internal/binder/binder.go:429-441`),
    /// and a type parameter is a declaration like any other. Upstream says why in
    /// as many words at `internal/checker/symbolaccessibility.go:766`: *"Type
    /// parameters are bound into `members` lists so they can merge across
    /// declarations. This is troublesome, since in all other respects, they
    /// behave like locals."* So `T` in `class C<T> { p: T }` is reachable only
    /// through `C`'s members table, and a resolver that reads `locals` alone
    /// finds nothing.
    ///
    /// `meaning` is what keeps that table from leaking: it holds the class's
    /// properties and methods too, and the lookup is filtered to
    /// `meaning & SymbolFlags::TYPE`, exactly as upstream's is. A value reference
    /// therefore does not find a type parameter, and a type reference does not
    /// find a method.
    ///
    /// # What is not ported, and is therefore not answered
    ///
    /// Every other arm of upstream's loop: module and namespace exports, enum
    /// members, `arguments`, a function expression's own name, `infer T`,
    /// decorator relocation, computed property names, and the globals lookup at
    /// the end. Each is a miss here rather than a wrong answer, because this only
    /// ever *adds* a table to consult.
    ///
    /// The `locals` lookup is **not** meaning-filtered, which upstream's is. That
    /// is the behaviour the meaning-less predecessor of this function had, and
    /// changing it is a separate question with its own regression surface —
    /// filtering would, for one, stop an `import X = Y` alias resolving for a
    /// value reference, since `SymbolFlags::ALIAS` is not in `SymbolFlags::VALUE`
    /// and nothing here follows aliases yet. Left as it was, deliberately;
    /// `bd tsr-y4u.12` owns the alias half. That is also what made replacing the
    /// meaning-less `resolve` a pure widening: it read the same `locals` in the
    /// same order, so every answer it gave, this gives, and the only new answers
    /// come from the members arm below. `resolve` is gone — all four callers
    /// (`declared.rs`, `expressions.rs`, `members.rs`, and `tsr-conformance`'s
    /// `types_producer.rs` twice) now pass the meaning upstream passes at that
    /// site. A meaning-less shim would have been a resolver that answers a
    /// question upstream never asks, kept alive by nobody re-checking it.
    #[must_use]
    pub fn resolve_name(
        &self,
        nodes: &NodeTable,
        node_map: &NodeMap<'a>,
        start: NodeId,
        name: &str,
        meaning: SymbolFlags,
    ) -> Option<SymbolId> {
        // Upstream's `lastLocation`: the node the walk came *from*. The static
        // rule below is a question about it, not about the class.
        let mut last: Option<NodeId> = None;
        let mut current = Some(start);
        while let Some(node) = current {
            // Locals first, as upstream does. **Unobservable today**: a class is
            // `IsContainer` without `HasLocals` (`container.rs:53`, matching
            // `GetContainerFlags`), so no node ever owns both tables and swapping
            // these two turns no test red. Stated rather than pinned by a test
            // that could not bite.
            if let Some(found) = self.lookup_local(node, name) {
                return Some(found);
            }
            if matches!(
                nodes.kind(node),
                SyntaxKind::ClassDeclaration
                    | SyntaxKind::ClassExpression
                    | SyntaxKind::InterfaceDeclaration
            ) && let Some(found) = self.lookup_type_member(node, name, meaning)
            {
                // `isTypeParameterSymbolDeclaredInContainer`
                // (`nameresolver.go:477`). A class and an interface of the same
                // name merge into one symbol and share one members table, so the
                // `T` of `interface C<T>` is visible in the table reached from
                // `class C`'s node. Upstream ignores a type parameter whose
                // declaration is parented elsewhere; without this,
                // `class C {} interface C<T> {}` would resolve `T` inside the
                // class body.
                if !self.is_type_parameter_declared_in(found, node, nodes) {
                    // Upstream clears `result` and falls out of the switch, so
                    // the walk continues outward rather than stopping here.
                    last = Some(node);
                    current = nodes.parent(node);
                    continue;
                }
                // TypeScript 1.0 spec (April 2014) §3.4.1: a type parameter's
                // scope covers the whole declaration **except static members**.
                // Upstream reports `Static_members_cannot_reference_class_type_parameters`
                // and returns nil — not the symbol. There are no checker
                // diagnostics yet (`bd tsr-5e7.6`), so only the `nil` is ported;
                // answering the type parameter here would be answering a question
                // upstream refuses.
                if last.is_some_and(|l| is_static_member(node_map, l)) {
                    // Upstream returns nil here, not the symbol.
                    return None;
                }
                return Some(found);
            }
            last = Some(node);
            current = nodes.parent(node);
        }
        None
    }

    /// `r.lookup(getSymbolOfDeclaration(location).Members, name, meaning & Type)`
    /// (`nameresolver.go:174`), with upstream's default `lookup`
    /// (`nameresolver.go:418`): a hit only when the symbol's flags intersect the
    /// meaning, and never when the meaning is empty.
    fn lookup_type_member(
        &self,
        container: NodeId,
        name: &str,
        meaning: SymbolFlags,
    ) -> Option<SymbolId> {
        let owner = self.symbol_of(container)?;
        let &found = self.symbols.get(owner).members.get(name)?;
        (self.symbols.get(found).flags.intersects(meaning & SymbolFlags::TYPE)).then_some(found)
    }

    /// Ported from `isTypeParameterSymbolDeclaredInContainer`
    /// (`nameresolver.go:477`).
    fn is_type_parameter_declared_in(
        &self,
        symbol: SymbolId,
        container: NodeId,
        nodes: &NodeTable,
    ) -> bool {
        self.symbols.get(symbol).declarations.iter().any(|&declaration| {
            nodes.kind(declaration) == SyntaxKind::TypeParameter
                && nodes.parent(declaration) == Some(container)
        })
    }
}

/// Ported from `ast.IsStatic` (`internal/ast/utilities.go`), for the one caller
/// above.
///
/// This is why [`BindResult::resolve_name`] takes a [`NodeMap`]: staticness is a
/// *modifier*, which lives in the typed node, and the resolver otherwise walks
/// entirely through [`NodeTable`] ids
/// ([ADR-0003](../../../docs/adr/0003-tree-plus-side-tables.md)).
///
/// `false` for an id with no node behind it, which [`NodeMap::get`] answers with
/// `None` rather than a panic.
fn is_static_member(node_map: &NodeMap<'_>, node: NodeId) -> bool {
    let Some(node) = node_map.get(node) else { return false };
    let modifiers = match node {
        tsr_ast::Node::ClassStaticBlockDeclaration(_) => return true,
        tsr_ast::Node::PropertyDeclaration(n) => n.modifiers,
        tsr_ast::Node::MethodDeclaration(n) => n.modifiers,
        tsr_ast::Node::GetAccessorDeclaration(n) => n.modifiers,
        tsr_ast::Node::SetAccessorDeclaration(n) => n.modifiers,
        tsr_ast::Node::IndexSignatureDeclaration(n) => n.modifiers,
        _ => return false,
    };
    modifiers.iter().any(|modifier| {
        matches!(modifier, tsr_ast::ModifierLike::Token(token)
            if token.kind == SyntaxKind::StaticKeyword)
    })
}

impl<'a> BindResult<'a> {
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

/// What the binder knows about a file that is not in the tree.
///
/// Upstream reads these off `b.file`, which is the source file *and* everything
/// the compiler knows about it. Our `SourceFile` node is generated from
/// `ast.json` and carries only what that schema names
/// ([ADR-0005](../../../docs/adr/0005-codegen-from-ast-json.md)), so they arrive
/// alongside it. See
/// [ADR-0016](../../../docs/adr/0016-file-info-not-a-file-name.md).
#[derive(Debug, Clone, Copy)]
pub struct FileInfo<'a> {
    /// The path, as `b.file.FileName()`. An external module's own symbol is
    /// named after it with the extension removed, and a `.d.ts` is an ambient
    /// context whose declarations are implicitly exported.
    pub name: &'a str,
    /// The source text, as `b.file.Text()`. Read only where a symbol's name is
    /// a *range* of the source that no single node holds — a JSX namespaced
    /// name, `ns:href`, is two identifiers with a colon between them.
    pub text: &'a str,
}

/// Bind a parsed file.
#[must_use]
pub fn bind<'a>(file: &'a SourceFile<'a>, nodes: &NodeTable, info: FileInfo<'a>) -> BindResult<'a> {
    binder::Binder::new(nodes).bind_source_file(file, info)
}
