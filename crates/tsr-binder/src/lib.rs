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

pub use binder::{is_declaration_file, is_external_module};
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
    globals: SymbolTable<'a>,
    /// Source-to-target redirects from declaration merging; see
    /// [`BindResult::merged_symbol`].
    merged: FxHashMap<SymbolId, SymbolId>,
    /// `(target, source)` for every merge the excludes masks forbade; see
    /// [`BindResult::merge_conflicts`].
    merge_conflicts: Vec<(SymbolId, SymbolId)>,
    /// The synthesised `undefined` global, if this bind created one.
    ///
    /// `None` when the program declared its own `undefined`, which must keep
    /// its declared type — seeding the slot unconditionally would clobber it.
    undefined_symbol: Option<SymbolId>,
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
    /// Nothing bound yet: the seed a program's first file is bound into.
    ///
    /// Not a `Default` impl, because "an empty bind result" is a *starting
    /// point* for accumulation rather than a sensible value to fall back to —
    /// and something that silently produces an empty symbol table when a real
    /// one was expected is precisely the shape of bug this crate exists to
    /// avoid.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            max_depth: 0,
            symbols: SymbolStore::new(),
            node_symbols: Vec::new(),
            locals: FxHashMap::default(),
            global_exports: SymbolTable::default(),
            globals: SymbolTable::default(),
            merged: FxHashMap::default(),
            merge_conflicts: Vec::new(),
            undefined_symbol: None,
            computed_names: FxHashMap::default(),
            diagnostics: Vec::new(),
            flow: FlowStore::new(),
            node_flow: Vec::new(),
            facts: FxHashMap::default(),
            end_flow: FxHashMap::default(),
            return_flow: FxHashMap::default(),
            fallthrough_flow: FxHashMap::default(),
        }
    }

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

    /// A name in the merged GLOBALS table — the §33 `globalThis` member
    /// road (`checker-notes-narrow.md`).
    #[must_use]
    pub fn global(&self, name: &str) -> Option<SymbolId> {
        self.globals.get(name).copied().map(|found| self.merged_symbol(found))
    }

    /// The `declare module "x"` a **specifier** names, if the program declares
    /// one.
    ///
    /// The lookup half of `c.getSymbol(c.globals, "\""+moduleName+"\"", …)`
    /// (`tryFindAmbientModule`, `internal/checker/checker.go:15533`), and it
    /// exists so that the quoting convention has exactly one spelling on each
    /// side. An ambient module's symbol is keyed by the **quoted** specifier —
    /// see `quoted_module_name` in `binder.rs` for why the quotes are
    /// load-bearing rather than decorative — and a caller that passes the bare
    /// specifier to [`BindResult::global`] instead gets whichever ordinary
    /// global happens to share the name.
    ///
    /// That is not hypothetical: `@types/node` declares both
    /// `declare module "process"` and a global `var process`, and the two
    /// collided in one key for as long as the specifier was stored unquoted
    /// (`docs/architecture/checker-notes-diag2.md` §202).
    ///
    /// **This is the table lookup and nothing else.** Upstream's
    /// `tryFindAmbientModule` also short-circuits a *relative* specifier and
    /// filters by `SymbolFlagsValueModule`; both stay at the call site, in
    /// `tsr_checker`, because both are that function's policy while the quoting
    /// is this table's convention. The relative test is
    /// `tsr_path::is_external_module_name_relative`, which is the real port of
    /// `tspath.IsExternalModuleNameRelative` — it also covers `.\\`, `..\\` and
    /// rooted disk paths, which a hand-rolled `starts_with("./")` misses.
    #[must_use]
    pub fn ambient_module(&self, specifier: &str) -> Option<SymbolId> {
        self.globals
            .get(format!("\"{specifier}\"").as_str())
            .copied()
            .map(|found| self.merged_symbol(found))
    }

    /// Look a name up in `container`'s own scope, without walking outward.
    #[must_use]
    pub fn lookup_local(&self, container: NodeId, name: &str) -> Option<SymbolId> {
        self.locals.get(&container)?.get(name).copied()
    }

    /// One symbol table, filtered by **meaning** — `(*NameResolver).lookup`
    /// (`nameresolver.go:418`), whose whole body is
    /// `if symbol.Flags&meaning != 0 { return symbol }`.
    ///
    /// # This filter is not an optimisation, it is what makes `export` work
    ///
    /// An exported namespace member gets **two** symbols
    /// (`declareModuleMember`, `binder.go:397`–`:409`): an *export* symbol in
    /// the container symbol's `exports` carrying the real flags, and a *local*
    /// carrying only `SymbolFlagsExportValue` — or **no flags at all** for a
    /// type-only declaration. This port reproduces that exactly; the local for
    /// `export class C {}` has flags `EXPORT_VALUE` and nothing else.
    ///
    /// So an unfiltered locals lookup finds that flagless local, stops, and the
    /// exports table is never consulted. `namespace N { export class C {} let
    /// x: C; }` answered `error` while the same namespace **without** `export`
    /// answered `C`. `bd tsr-56r`.
    ///
    /// [`Self::lookup_local`] stays unfiltered for its other callers, which ask
    /// a different question — *is this name declared in this scope* — rather
    /// than *does this name resolve here for this meaning*.
    fn lookup_scoped(
        &self,
        table: Option<&SymbolTable<'a>>,
        name: &str,
        meaning: SymbolFlags,
    ) -> Option<SymbolId> {
        if meaning.is_empty() {
            return None;
        }
        let found = self.merged_symbol(*table?.get(name)?);
        let flags = self.symbols.get(found).flags;
        if flags.intersects(meaning) {
            return Some(found);
        }
        // **An alias is accepted whatever its own flags say.** Upstream's
        // checker-side override (`Checker.getSymbol`, `checker.go:2183`) accepts
        // an `Alias` when the flags of its *target* carry the meaning:
        // `import { C } from "m"` is a symbol with `ALIAS` and nothing else, and
        // a lookup for `TYPE` has to see through it.
        //
        // Resolving the target needs the checker, which a `BindResult` does not
        // have, so the target's flags are not tested here. That is **weaker than
        // upstream and stronger than what this port did before**, which tested
        // nothing at all — so it cannot reject anything the unfiltered lookup
        // accepted. The first run without this clause lost 2,627 lines across
        // 399 cases, every one an imported name.
        flags.intersects(SymbolFlags::ALIAS).then_some(found)
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
        self.resolve_name_excluding(nodes, node_map, start, name, meaning, None)
    }

    /// `useResult`'s type arm (`binder/nameresolver.go:61`): *"local types are
    /// only in scope in the function body"*. A type found in a function's
    /// `locals` while the walk arrives from anywhere but the body is the answer
    /// **only if it is a type parameter**. §842.
    fn local_type_hidden_outside_body(
        &self,
        symbol: SymbolId,
        location: NodeId,
        last: Option<NodeId>,
        meaning: SymbolFlags,
        nodes: &NodeTable,
        node_map: &NodeMap<'a>,
    ) -> bool {
        let Some(last) = last else { return false };
        if !crate::container::is_function_like_kind(nodes.kind(location)) {
            return false;
        }
        let body = match node_map.get(location) {
            Some(tsr_ast::Node::FunctionDeclaration(f)) => f.body.and_then(|b| b.node_id()),
            Some(tsr_ast::Node::FunctionExpression(f)) => f.body.and_then(|b| b.node_id()),
            Some(tsr_ast::Node::ArrowFunction(f)) => f.body.and_then(|b| b.node_id()),
            Some(tsr_ast::Node::MethodDeclaration(m)) => m.body.and_then(|b| b.node_id()),
            Some(tsr_ast::Node::ConstructorDeclaration(c)) => c.body.and_then(|b| b.node_id()),
            Some(tsr_ast::Node::GetAccessorDeclaration(a)) => a.body.and_then(|b| b.node_id()),
            Some(tsr_ast::Node::SetAccessorDeclaration(a)) => a.body.and_then(|b| b.node_id()),
            _ => None,
        };
        if body == Some(last) {
            return false;
        }
        let flags = self.symbols.get(self.merged_symbol(symbol)).flags;
        meaning.intersects(flags & SymbolFlags::TYPE)
            && !flags.contains(SymbolFlags::TYPE_PARAMETER)
    }

    /// `useResult`'s parameter arm (`binder/nameresolver.go:72`): *"parameters
    /// are only in the scope of function body"*. When the walk arrives at a
    /// function-like node **from a type parameter**, a parameter symbol found
    /// in that node's `locals` is not the answer and the walk continues
    /// outward — which is what makes `function f<T extends typeof a>(a: T)`
    /// report on `a`.
    ///
    /// Only this arm of upstream's `useResult` block is ported; the rest —
    /// type parameters, `infer T` in conditional types,
    /// `useOuterVariableScopeInParameter` — each has its own blast radius. §838.
    fn parameter_hidden_from_type_parameter_list(
        &self,
        symbol: SymbolId,
        location: NodeId,
        last: Option<NodeId>,
        nodes: &NodeTable,
    ) -> bool {
        let Some(last) = last else { return false };
        if nodes.kind(last) != SyntaxKind::TypeParameter {
            return false;
        }
        if !crate::container::is_function_like_kind(nodes.kind(location)) {
            return false;
        }
        let declarations = &self.symbols.get(self.merged_symbol(symbol)).declarations;
        // Upstream tests `SymbolFlagsFunctionScopedVariable`, which a binding
        // element inside a parameter also carries: `f1<T extends typeof a>({a}:
        // …)` declares `a` as a `BindingElement`, not a `Parameter`. Testing
        // the node kind alone reported 2 of the fixture's 6 lines. §838.
        !declarations.is_empty()
            && declarations.iter().all(|&declaration| {
                let mut current = Some(declaration);
                while let Some(node) = current {
                    match nodes.kind(node) {
                        SyntaxKind::Parameter => return true,
                        SyntaxKind::BindingElement
                        | SyntaxKind::ObjectBindingPattern
                        | SyntaxKind::ArrayBindingPattern => {}
                        _ => return false,
                    }
                    current = nodes.parent(node);
                }
                false
            })
    }

    /// Do **all** of this symbol's declarations lie inside `exclude`? Such a
    /// symbol is the excluded declaration's own, and not an answer to a lookup
    /// that must resolve from the parent scope. §836.
    fn symbol_is_declared_within(
        &self,
        symbol: SymbolId,
        exclude: Option<NodeId>,
        nodes: &NodeTable,
    ) -> bool {
        let Some(exclude) = exclude else { return false };
        let declarations = &self.symbols.get(self.merged_symbol(symbol)).declarations;
        if declarations.is_empty() {
            return false;
        }
        declarations.iter().all(|&declaration| {
            let mut current = Some(declaration);
            while let Some(node) = current {
                if node == exclude {
                    return true;
                }
                current = nodes.parent(node);
            }
            false
        })
    }

    /// [`Binder::resolve_name`], with a declaration whose own symbol is not an
    /// answer.
    ///
    /// Upstream's `resolveEntityName` resolves an export specifier's name from
    /// the **parent** scope, so the alias the specifier itself created is not a
    /// candidate. This port's binder gives `export { X }` a symbol named `X`
    /// whose declaration list is exactly `[the specifier]`, and a plain lookup
    /// finds it and stops.
    ///
    /// The exclusion is applied at the `locals` hit rather than to the
    /// resulting declaration list: §765 tried the latter and measured `+0`,
    /// because the assumption was one symbol with two declarations and the
    /// reality is **two symbols**. A candidate whose declarations all lie
    /// within `exclude` is skipped and the walk continues outward.
    ///
    /// `docs/architecture/checker-notes-diag2.md` §836, `bd tsr-8esz`.
    #[must_use]
    pub fn resolve_name_excluding(
        &self,
        nodes: &NodeTable,
        node_map: &NodeMap<'a>,
        start: NodeId,
        name: &str,
        meaning: SymbolFlags,
        exclude: Option<NodeId>,
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
            if let Some(found) = self.lookup_scoped(self.locals.get(&node), name, meaning)
                && !self.symbol_is_declared_within(found, exclude, nodes)
                && !self.parameter_hidden_from_type_parameter_list(found, node, last, nodes)
                && !self.local_type_hidden_outside_body(found, node, last, meaning, nodes, node_map)
            {
                return Some(found);
            }
            // **A named function expression's own name is in scope inside it,
            // and this walk does not answer it — REFUSED at +1 case / -10
            // lines, §216.**
            //
            // The arm itself is four lines and upstream's is at
            // `nameresolver.go:233-244`: `bindFunctionExpression` gives the name
            // a symbol in NO symbol table, so no `locals` lookup can find it and
            // the walk compares the name being resolved against the function
            // expression's written name. Built, measured, reverted.
            //
            // **It is blocked on the RETURN-TYPE circularity answer, not on
            // itself.** Resolving `y` inside `function y() { return y; }` makes
            // the symbol's type depend on itself. Upstream's
            // `getReturnTypeOfSignature` answers `anyType` on that cycle
            // (`checker.go:20020`) and so records `>y : () => any`; this port's
            // resolution stack has no `ResolvedReturnType` key, so the cycle is
            // caught one level out — at the symbol's `Type` — and answers
            // `errorType`. Landing the arm alone turns four cases' `any` lines
            // into `error` lines: `namedFunctionExpressionCall` 3/12 -> 0/12,
            // `templateStringWithEmbeddedFunctionExpression` 4/5 -> 2/5 and its
            // ES6 twin, `functionExpressionWithResolutionOfTypeOfSameName01`
            // 4/5 -> 1/5. Only `recursiveNamedLambdaCall` converts.
            //
            // FALSIFIER, one function: give `Resolutions` a `NodeId`-keyed
            // instance and a `PropertyName::ResolvedReturnType`, guard
            // `return_type_from_body` with it, and answer `any` — not `error` —
            // when the pop fails. If `function y() { return y; }` then reads
            // `() => any`, this arm is a four-line transcription and the whole
            // row converts.
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
                return Some(self.merged_symbol(found));
            }
            // A **namespace's exports** (`nameresolver.go:104`–`:146`).
            //
            // `namespace N { export class C {} }` puts `C` in `N`'s *symbol's*
            // `exports` table, not in the body's `locals` — so before this arm
            // existed, **exporting a declaration made it unresolvable**:
            // `namespace N { class C {} let x: C; }` answered `C` and
            // `namespace N { export class C {} export let x: C; }` answered
            // `error`. `bd tsr-56r`.
            //
            // Upstream's mask is what stops an exported `const` answering a
            // lookup for a type, and it is *not* the same mask for an enum:
            // `EnumDeclaration` gets its own arm at `:146` with
            // `SymbolFlagsEnumMember`, because an enum's members are values of
            // the enum rather than module members.
            //
            // **After locals, not before.** Upstream reaches this in the same
            // `switch` that locals are tested before, so a non-exported local
            // shadows an export of the same name — which is also why removing
            // the `lookup_local` call above cannot be compensated for here.
            let exported = match nodes.kind(node) {
                // `case KindSourceFile:` **falls through** to
                // `KindModuleDeclaration` upstream (`nameresolver.go:100`–`:104`)
                // when the file is an external or CommonJS module — a module's
                // own top-level `export`s are in scope inside it, by the same
                // two-symbol construction.
                //
                // Included unconditionally rather than gated on "is a module":
                // for a *script* file nothing is ever routed to exports
                // (`is_export_context` requires `self.is_module`), so the table
                // is empty and the arm cannot fire. Omitting it cost 1,621 lines
                // across 219 cases on the first measured run.
                SyntaxKind::SourceFile | SyntaxKind::ModuleDeclaration => {
                    Some(SymbolFlags::MODULE_MEMBER)
                }
                SyntaxKind::EnumDeclaration => Some(SymbolFlags::ENUM_MEMBER),
                _ => None,
            };
            if let Some(mask) = exported
                && let Some(symbol) = self.symbol_of(node)
                && let Some(&found) = self.symbols.get(symbol).exports.get(name)
                && self.symbols.get(found).flags.intersects(meaning & mask)
                // An `export { X }` specifier's own symbol lives in the file's
                // **exports**, not its `locals`, so the exclusion belongs on
                // this arm too — §836's first measurement found the specifier
                // here after the `locals` arm had been filtered.
                && !self.symbol_is_declared_within(found, exclude, nodes)
            {
                return Some(self.merged_symbol(found));
            }

            last = Some(node);
            current = nodes.parent(node);
        }
        // The walk has run off the top of the file. Upstream's
        // `resolveNameHelper` ends at `c.globals` (`nameresolver.go`), which is
        // every script file's top-level names merged together — including the
        // bundled `lib.*.d.ts`, which is how `Array` and `Promise` resolve at
        // all.
        //
        // Reached only after every enclosing scope has been tried, so a local
        // always shadows a global, which is the order upstream has and the
        // reason this is here rather than earlier.
        //
        // Empty unless several files were bound into one result
        // ([`bind_into`]), so a file bound alone behaves exactly as before.
        //
        // **Meaning-filtered, through the same helper every other arm uses.**
        // This lookup ignored `meaning` entirely until §166, which made every
        // global name answer *every* query: `namespace A {}` resolved under
        // `SymbolFlags::TYPE` even though its symbol carries only
        // `NAMESPACE_MODULE`, and since `lib.*.d.ts` is bound into `globals`,
        // so did every name in it. That is why `var a: A` reported nothing —
        // the checker's meaning ladder saw a `TYPE` hit and fell silent, which
        // is the correct response to a hit and the wrong answer here.
        // `checker-notes-diag2.md` §202.
        self.lookup_scoped(Some(&self.globals), name, meaning)
    }

    /// Every name a lookup from `start` could reach, in no particular order.
    ///
    /// The same scope chain [`BindResult::resolve_name`] walks — enclosing
    /// `locals`, a class or interface's `members`, a module's or enum's
    /// `exports`, and finally `globals` — collected instead of searched.
    ///
    /// # Why this exists, and what it is not
    ///
    /// Upstream's `getSpellingSuggestion` (`checker.go`) is driven by
    /// `forEachSymbol`, which walks the same chain. The checker's TS2304 rule
    /// needs the *set* rather than a hit, because upstream reports a different
    /// code (TS2552) when a near-miss exists and this port must stay silent
    /// there rather than report the wrong one — see
    /// `Checker::has_spelling_suggestion`.
    ///
    /// **It is not meaning-filtered**, deliberately: a suggestion of the wrong
    /// meaning still makes upstream pick TS2552 over TS2304, so filtering would
    /// narrow the set in the direction that manufactures wrong codes.
    #[must_use]
    pub fn names_in_scope(
        &self,
        nodes: &NodeTable,
        node_map: &NodeMap<'a>,
        start: NodeId,
    ) -> Vec<&'a str> {
        self.names_in_scope_with_meaning(nodes, node_map, start, SymbolFlags::all())
    }

    /// [`Binder::names_in_scope`], restricted to the names whose symbol carries
    /// one of `meaning`'s flags.
    ///
    /// Upstream's `getSuggestedSymbolForNonexistentSymbol` searches
    /// `symbolsInScope(location, meaning)` and **always** passes a meaning
    /// (`checker.go`). Ignoring it is harmless where nearly every name in scope
    /// is a value and wrong in a type position, where it offers a nearby
    /// *variable* for a missing *type*: `parserRealSource13` was 105 wrong
    /// TS2552 lines for one missing `AST`
    /// (`docs/architecture/checker-notes-diag2.md` §202, §57).
    #[must_use]
    pub fn names_in_scope_with_meaning(
        &self,
        nodes: &NodeTable,
        node_map: &NodeMap<'a>,
        start: NodeId,
        meaning: SymbolFlags,
    ) -> Vec<&'a str> {
        let _ = node_map;
        let mut names: Vec<&'a str> = Vec::new();
        let push = |names: &mut Vec<&'a str>, table: &SymbolTable<'a>| {
            for (name, symbol) in table {
                if self.symbols.get(*symbol).flags.intersects(meaning) {
                    names.push(name);
                }
            }
        };
        let mut current = Some(start);
        while let Some(node) = current {
            if let Some(table) = self.locals.get(&node) {
                push(&mut names, table);
            }
            if let Some(symbol) = self.symbol_of(node) {
                let symbol = self.symbols.get(symbol);
                push(&mut names, &symbol.members);
                push(&mut names, &symbol.exports);
            }
            current = nodes.parent(node);
        }
        push(&mut names, &self.globals);
        names
    }

    /// Every declaration merge the excludes masks forbade, as `(target, source)`.
    ///
    /// Upstream calls `reportMergeSymbolError` (`checker.go:14201`) inline at
    /// the point `mergeSymbol` refuses the union. This port cannot: the merge
    /// happens in the binder and the *only* binder diagnostics anything
    /// collects are the ones from `diagnostics_suite`'s per-unit bind, which
    /// binds each file alone and so never sees a cross-file collision at all.
    /// Recording the pair and letting `Checker::report_merge_conflicts` issue
    /// the diagnostics restores upstream's own layering — `mergeSymbol` is a
    /// checker function there — and gets the per-declaration **file** right,
    /// which is the whole difficulty: the two declarations are in different
    /// files by construction.
    ///
    /// Order is the order the merges were attempted, which is file order.
    /// See `docs/architecture/checker-notes-diag2.md` §202.
    #[must_use]
    pub fn merge_conflicts(&self) -> &[(SymbolId, SymbolId)] {
        &self.merge_conflicts
    }

    /// The synthesised `undefined` symbol, if this bind created one.
    ///
    /// `None` when the program declared its own, so the checker seeds a type
    /// only for the symbol it is entitled to.
    #[must_use]
    pub fn undefined_symbol(&self) -> Option<SymbolId> {
        self.undefined_symbol
    }

    /// Every name visible to the whole program (`c.globals`).
    ///
    /// Empty for a file bound on its own: a program's globals are the union of
    /// its *script* files' top-level names, and one file is only a program if
    /// something says so. See [`bind_into`].
    #[must_use]
    pub fn globals(&self) -> &SymbolTable<'a> {
        &self.globals
    }

    /// The symbol a merged-away symbol redirects to.
    ///
    /// Ported from `Checker.getMergedSymbol` (`internal/checker/checker.go:14355`),
    /// backed by `c.mergedSymbols` (`:666`) and recorded by `recordMergedSymbol`
    /// (`:14372`).
    ///
    /// # Why this is not optional bookkeeping
    ///
    /// `Binder::merge_symbol` unions a **source** symbol into a **target** and
    /// leaves the source in place, still reachable: it is still in its own
    /// file's `locals`, and it still carries only its own file's members. So
    /// `interface I { a }` in one script file and `interface I { b }` in another
    /// produce a complete merged symbol *and* a stale partial one, and which of
    /// the two a lookup reaches depends on **which file the reference is in**.
    /// A reference in the target's file sees both members; one in the source's
    /// file sees only its own.
    ///
    /// That is not a lookup bug — `get_property_of_type` answers correctly for
    /// the symbol it was handed. It is a missing redirect, and the information
    /// needed to build it exists only where the merge happened.
    ///
    /// **Idempotent and total**: a symbol that was never merged is its own
    /// answer, so callers can apply this unconditionally. Chains are followed,
    /// because `merge_symbol` recurses into members and exports and a member can
    /// be merged into a symbol that is itself later merged.
    #[must_use]
    pub fn merged_symbol(&self, symbol: SymbolId) -> SymbolId {
        let mut current = symbol;
        // Bounded rather than trusting acyclicity: `merge_symbol` records
        // `source -> target` and never `target -> source`, so a cycle needs a
        // bug, and a hang is a worse way to find out than a wrong answer.
        for _ in 0..32 {
            match self.merged.get(&current) {
                Some(&next) if next != current => current = next,
                _ => return current,
            }
        }
        current
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
pub fn bind<'a>(
    arena: &'a tsr_core::Arena,
    file: &'a SourceFile<'a>,
    nodes: &NodeTable,
    info: FileInfo<'a>,
) -> BindResult<'a> {
    bind_into(BindResult::empty(), arena, file, nodes, info)
}

/// Bind a file **into what the program's earlier files produced**.
///
/// The binder half of ADR-0034's identity widening, and the counterpart of
/// `tsr_parser::parse_into`: every file of a program binds into one
/// [`SymbolStore`], so a [`SymbolId`] names one symbol across the program rather
/// than one per file. Together with a shared node table that is what makes a
/// symbol from another file safe to hand to the checker at all — its
/// `value_declaration` then indexes the same node table the checker is reading.
///
/// `nodes` must be the shared table the file was parsed into, and the files must
/// be bound in the order they were parsed. Neither is checked: the first is a
/// type-level truth only for the arena, and the second is the caller's, because
/// the binder cannot tell which range of ids belongs to the file it was handed.
///
/// [`bind`] is this with an [`BindResult::empty`] seed, and remains the entry
/// point for anything binding one file alone.
#[must_use]
pub fn bind_into<'a>(
    previous: BindResult<'a>,
    arena: &'a tsr_core::Arena,
    file: &'a SourceFile<'a>,
    nodes: &NodeTable,
    info: FileInfo<'a>,
) -> BindResult<'a> {
    binder::Binder::resuming(arena, nodes, previous).bind_source_file(file, info)
}

/// [`bind_into`], with the file's JSDoc side table along.
///
/// In a JavaScript file, `@typedef`/`@callback`/`@template` tags declare and
/// an `@overload` block is another declaration of the function it documents —
/// upstream reparses them into the tree (`parser.reparseTags`) and binds them
/// as written syntax. This parser keeps JSDoc in a side table, so the binder
/// takes the entries and files the declarations from them.
#[must_use]
pub fn bind_into_with_jsdoc<'a>(
    previous: BindResult<'a>,
    arena: &'a tsr_core::Arena,
    file: &'a SourceFile<'a>,
    nodes: &NodeTable,
    info: FileInfo<'a>,
    jsdoc: &[(NodeId, &'a [&'a tsr_ast::JSDoc<'a>])],
) -> BindResult<'a> {
    binder::Binder::resuming(arena, nodes, previous).bind_source_file_with_jsdoc(file, info, jsdoc)
}
