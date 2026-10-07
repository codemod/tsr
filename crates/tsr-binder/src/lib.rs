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
//! - **Optional chains.** The flow shapes are ported in full and LIVE since
//!   §748 (`checker-notes-callres.md`), when the parser began setting
//!   [`tsr_ast::NodeFlags::OPTIONAL_CHAIN`] as upstream's
//!   `tryReparseOptionalChain` does. Before that `a?.b` got the graph of
//!   `a.b`.
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
mod names;
mod narrowing;
#[cfg(test)]
mod parallel_tests;
mod symbol;

use rustc_hash::FxHashMap;
use tsr_ast::{NodeId, NodeMap, NodeTable, SourceFile, SyntaxKind};
use tsr_core::Idx as _;
use tsr_diagnostics::Diagnostic;

pub use binder::{is_declaration_file, is_external_module};
pub use container::{ContainerFlags, container_flags};
pub use flow::{Antecedents, FlowFlags, FlowId, FlowStore, ReduceLabel, SwitchClause};
pub use names::PreparedNames;
pub use symbol::{
    Symbol, SymbolFlags, SymbolId, SymbolStore, SymbolStoreIdentity, SymbolTable, SymbolTableField,
};

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

/// One `declare module "x" { … }` that augments another module: an entry of
/// upstream's `SourceFile.ModuleAugmentations` that is not `declare global`.
#[derive(Debug, Clone, Copy)]
pub struct ModuleAugmentation<'a> {
    /// The augmentation's symbol — every `declare module "x"` of one file
    /// shares it.
    pub symbol: SymbolId,
    /// The source file the augmentation is written in.
    pub file: NodeId,
    /// The string literal naming the augmented module.
    pub name: NodeId,
    /// Its text.
    pub text: &'a str,
    /// Nested in a top-level ambient module of a script rather than written at
    /// the top level of a module. Upstream's parser collects that form only
    /// under a non-relative name (`references.go:61`).
    pub nested: bool,
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
    /// Non-global module augmentations not yet merged; see
    /// [`BindResult::merge_module_augmentations`].
    module_augmentations: Vec<ModuleAugmentation<'a>>,
    /// `patternAmbientModuleAugmentations` (`checker.go:766`): an
    /// augmentation of a name only a pattern ambient module matches, keyed by
    /// that name; see [`BindResult::pattern_ambient_module`].
    pattern_ambient_module_augmentations: FxHashMap<&'a str, SymbolId>,
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

/// An unpublished file-local bind. Its IDs cannot be read by a checker.
/// AST identities already belong to the program; symbols and flow remain private.
pub struct FileBindResult<'a, 'n> {
    nodes: &'n NodeTable,
    bindings: BindResult<'a>,
    node_base: usize,
    root: NodeId,
    is_module: bool,
    commonjs_module: bool,
    global_augmentations: Vec<SymbolId>,
}

/// Bind one published AST using only private, file-sized mutable tables.
/// UMD export declarations retain the serial path because the accumulating
/// binder reuses previously declared UMD aliases during declaration itself.
#[must_use]
pub fn bind_file<'a, 'n>(
    names: &'n PreparedNames<'a>,
    nodes: &'n NodeTable,
    file: &'a SourceFile<'a>,
    info: FileInfo<'a>,
    jsdoc: &[(NodeId, &'a [&'a tsr_ast::JSDoc<'a>])],
    node_range: std::ops::Range<u32>,
) -> FileBindResult<'a, 'n> {
    binder::Binder::bind_independent(names, nodes, file, info, jsdoc, node_range)
}

impl<'a> BindResult<'a> {
    /// Publish one completed file in program order, including the ordered
    /// global merge and first-file synthetic undefined boundary.
    #[must_use]
    pub fn publish_file(
        self,
        arena: &'a tsr_core::Arena,
        nodes: &NodeTable,
        local: FileBindResult<'a, '_>,
    ) -> Self {
        binder::Binder::publish_file(arena, nodes, self, local)
    }
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
            module_augmentations: Vec::new(),
            pattern_ambient_module_augmentations: FxHashMap::default(),
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
        let mut short_key = [0; 64];
        let long_key;
        let key = if specifier.len() <= short_key.len() - 2 {
            let end = specifier.len() + 1;
            short_key[0] = b'"';
            short_key[1..end].copy_from_slice(specifier.as_bytes());
            short_key[end] = b'"';
            std::str::from_utf8(&short_key[..=end])
                .expect("quoting a UTF-8 specifier preserves UTF-8")
        } else {
            long_key = format!("\"{specifier}\"");
            long_key.as_str()
        };
        self.globals.get(key).copied().map(|found| self.merged_symbol(found))
    }

    /// The pattern-ambient arm of `resolveExternalModule`
    /// (`checker.go:15364-15372`), consulted only once a specifier resolved
    /// to neither an ambient module nor a file: the best-matching
    /// `declare module "prefix*suffix"` (`core.FindBestPatternMatch`), or the
    /// augmentation recorded for exactly this name when one merged into it.
    ///
    /// Upstream's `patternAmbientModules` is a list in file order; the globals
    /// here are a hash table, so the list is rebuilt per call from the
    /// quoted, single-`*` `ValueModule` globals and ordered by first
    /// declaration. It runs only for a specifier nothing else resolved.
    #[must_use]
    pub fn pattern_ambient_module(&self, specifier: &str) -> Option<SymbolId> {
        let mut best: Option<(usize, NodeId, SymbolId)> = None;
        for (&name, &symbol) in &self.globals {
            let Some(pattern) = name.strip_prefix('"').and_then(|name| name.strip_suffix('"'))
            else {
                continue;
            };
            // `core.TryParsePattern`: exactly one `*` (none is not a pattern
            // module; two are TS5061 and not collected).
            let Some(star) = pattern.find('*') else { continue };
            let (prefix, suffix) = (&pattern[..star], &pattern[star + 1..]);
            if suffix.contains('*') {
                continue;
            }
            let entry = self.symbols.get(symbol);
            if !entry.flags.intersects(SymbolFlags::VALUE_MODULE) {
                continue;
            }
            // `Pattern.Matches`.
            if specifier.len() < pattern.len() - 1
                || !specifier.starts_with(prefix)
                || !specifier.ends_with(suffix)
            {
                continue;
            }
            let Some(&first) = entry.declarations.first() else { continue };
            // `StarIndex > longestMatchPrefixLength`: a longer prefix wins, and
            // among equal prefixes the first in file order.
            if best.is_none_or(|(length, declaration, _)| {
                star > length || (star == length && first < declaration)
            }) {
                best = Some((star, first, symbol));
            }
        }
        let (_, _, pattern) = best?;
        if let Some(&augmentation) = self.pattern_ambient_module_augmentations.get(specifier) {
            return Some(self.merged_symbol(augmentation));
        }
        Some(self.merged_symbol(pattern))
    }

    /// Whether `symbol` is a pattern ambient module (`declare module
    /// "*.foo"`): the `core.Some(c.patternAmbientModules, …)` test of
    /// `mergeModuleAugmentation` (`checker.go:1427`).
    fn is_pattern_ambient_module(&self, symbol: SymbolId) -> bool {
        let name = self.symbols.get(symbol).name;
        name.strip_prefix('"').and_then(|name| name.strip_suffix('"')).is_some_and(|pattern| {
            pattern.matches('*').count() == 1 && self.globals.get(name) == Some(&symbol)
        })
    }

    /// Merge every recorded non-global module augmentation into the module it
    /// augments: the last loop of `initializeChecker`
    /// (`internal/checker/checker.go:1384-1391`) over `mergeModuleAugmentation`
    /// (`:1405-1448`).
    ///
    /// Run once every file is bound, because an augmentation may name a module
    /// bound after it. Module resolution is the program's, so `resolve`
    /// answers `resolveExternalModuleNameWorker` for an augmentation's name or
    /// an `export *` specifier: `(this result, importing file, specifier,
    /// usage node, nested augmentation)` to the module symbol, or `None`.
    ///
    /// Per augmentation, as upstream:
    ///
    /// 1. resolve the name; a miss merges nothing (TS2664 is the checker's,
    ///    `check_module_augmentation_name`);
    /// 2. follow `export =` (`resolveExternalModuleSymbol`, aliases
    ///    resolved);
    /// 3. a target without `Namespace` meaning merges nothing (upstream's
    ///    TS2649, not reported here);
    /// 4. when the target re-exports with `export *`, an augmentation export
    ///    naming a re-exported symbol merges into that symbol too;
    /// 5. `mergeSymbol(target, augmentation)`.
    ///
    /// A pattern ambient module target merges the other way round,
    /// unidirectionally: the pattern's exports join the augmentation, the
    /// pattern itself is left untouched, and the augmentation is recorded
    /// under its name for [`BindResult::pattern_ambient_module`]
    /// (`checker.go:1422-1432`; `docs/parity/notes/names-modules.md` §6).
    ///
    /// Alias forms requiring checker-owned types/module images remain unsupported;
    /// see `docs/parity/notes/module-augmentation.md`. External import-equals
    /// indirections reuse this operation's Program resolution callback.
    #[must_use]
    pub fn merge_module_augmentations(
        mut self,
        arena: &'a tsr_core::Arena,
        nodes: &NodeTable,
        node_map: &NodeMap<'a>,
        mut resolve: impl FnMut(&Self, NodeId, &str, NodeId, bool) -> Option<SymbolId>,
    ) -> Self {
        let augmentations = std::mem::take(&mut self.module_augmentations);
        for augmentation in augmentations {
            if let Some(pattern) =
                self.pattern_augmentation_target(nodes, node_map, augmentation, &mut resolve)
            {
                // `mergeSymbol(moduleAugmentation.Symbol, mainModule,
                // unidirectional = true)`: no redirect is recorded at any
                // level, so the pattern and its exports keep answering for
                // themselves. `merge_symbol` records as it goes; every
                // redirect it added is taken back.
                let before = self.merged.clone();
                self = binder::Binder::resuming(arena, nodes, self)
                    .merge_pairs(&[(augmentation.symbol, pattern)]);
                self.merged = before;
                self.pattern_ambient_module_augmentations
                    .insert(augmentation.text, augmentation.symbol);
                continue;
            }
            let merges =
                self.module_augmentation_merges(nodes, node_map, augmentation, &mut resolve);
            if !merges.is_empty() {
                self = binder::Binder::resuming(arena, nodes, self).merge_pairs(&merges);
            }
        }
        self
    }

    /// The pattern ambient module an augmentation's name resolves to, when it
    /// resolves to one (and not to an augmentation already recorded for the
    /// name, which merges as an ordinary module).
    fn pattern_augmentation_target(
        &self,
        nodes: &NodeTable,
        node_map: &NodeMap<'a>,
        augmentation: ModuleAugmentation<'a>,
        resolve: &mut impl FnMut(&Self, NodeId, &str, NodeId, bool) -> Option<SymbolId>,
    ) -> Option<SymbolId> {
        let main = resolve(
            self,
            augmentation.file,
            augmentation.text,
            augmentation.name,
            augmentation.nested,
        )?;
        let main = self.resolve_external_module_symbol(
            nodes,
            node_map,
            self.merged_symbol(main),
            resolve,
        )?;
        (self.symbols.get(main).flags.intersects(SymbolFlags::NAMESPACE)
            && self.is_pattern_ambient_module(main))
        .then_some(main)
    }

    /// The `(target, source)` merges one augmentation makes, in upstream's
    /// order; empty when it merges nothing.
    fn module_augmentation_merges(
        &self,
        nodes: &NodeTable,
        node_map: &NodeMap<'a>,
        augmentation: ModuleAugmentation<'a>,
        resolve: &mut impl FnMut(&Self, NodeId, &str, NodeId, bool) -> Option<SymbolId>,
    ) -> Vec<(SymbolId, SymbolId)> {
        let Some(main) = resolve(
            self,
            augmentation.file,
            augmentation.text,
            augmentation.name,
            augmentation.nested,
        ) else {
            return Vec::new();
        };
        let Some(main) =
            self.resolve_external_module_symbol(nodes, node_map, self.merged_symbol(main), resolve)
        else {
            return Vec::new();
        };
        let target = self.symbols.get(main);
        // `mainModule.Flags&ast.SymbolFlagsNamespace != 0`.
        if !target.flags.intersects(SymbolFlags::NAMESPACE) || self.is_pattern_ambient_module(main)
        {
            return Vec::new();
        }
        let source = self.symbols.get(augmentation.symbol);
        let mut merges = Vec::new();
        // `checker.go:1433-1441`: an augmentation export that names something
        // the target re-exports through `export *` (and does not export
        // itself) merges into the re-exported symbol as well.
        if target.exports.get(binder::INTERNAL_EXPORT_STAR).is_some() && !source.exports.is_empty()
        {
            for (name, value) in &source.exports {
                if target.exports.get(name).is_none()
                    && let Some(found) = self.export_star_member(
                        nodes,
                        node_map,
                        main,
                        name,
                        resolve,
                        &mut Vec::new(),
                    )
                {
                    merges.push((found, *value));
                }
            }
        }
        merges.push((main, augmentation.symbol));
        merges
    }

    /// Ported from `Checker.resolveExternalModuleSymbol`, `resolveAlias` and
    /// `getTargetOfImportEqualsDeclaration` (`internal/checker/checker.go`).
    /// Follow `export =` and external import-equals indirections using the
    /// Program's existing usage-mode resolution, not a rendered module name.
    /// Other alias forms remain unsupported here: namespace imports require
    /// `resolveESModuleSymbol`'s checker-owned module image and interop options.
    fn resolve_external_module_symbol(
        &self,
        nodes: &NodeTable,
        node_map: &NodeMap<'a>,
        module: SymbolId,
        resolve: &mut impl FnMut(&Self, NodeId, &str, NodeId, bool) -> Option<SymbolId>,
    ) -> Option<SymbolId> {
        let module = self.merged_symbol(module);
        let Some(&export_equals) =
            self.symbols.get(module).exports.get(binder::INTERNAL_EXPORT_EQUALS)
        else {
            return Some(module);
        };
        self.augmentation_alias_target(nodes, node_map, export_equals, resolve, &mut Vec::new())
    }

    /// Symbol-only slice of native `resolveAlias`/`resolveIndirectionAlias`.
    /// The active path is local to one augmentation query; a cycle is failure,
    /// never a completed target. No alias result cache is published by binding.
    fn augmentation_alias_target(
        &self,
        nodes: &NodeTable,
        node_map: &NodeMap<'a>,
        symbol: SymbolId,
        resolve: &mut impl FnMut(&Self, NodeId, &str, NodeId, bool) -> Option<SymbolId>,
        active: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        let symbol = self.merged_symbol(symbol);
        let entry = self.symbols.get(symbol);
        if !entry.flags.intersects(SymbolFlags::ALIAS)
            || entry
                .flags
                .intersects(SymbolFlags::VALUE | SymbolFlags::TYPE | SymbolFlags::NAMESPACE)
        {
            return Some(symbol);
        }
        if active.contains(&symbol) {
            return None;
        }
        active.push(symbol);
        let result = (|| {
            let declaration = *entry.declarations.first()?;
            let target = match node_map.get(declaration)? {
                tsr_ast::Node::ExportAssignment(assignment) => {
                    let tsr_ast::Expression::Identifier(name) = assignment.expression? else {
                        return None;
                    };
                    self.resolve_name(
                        nodes,
                        node_map,
                        declaration,
                        name.text,
                        SymbolFlags::VALUE | SymbolFlags::TYPE | SymbolFlags::NAMESPACE,
                    )?
                }
                tsr_ast::Node::ImportEqualsDeclaration(import) => {
                    let tsr_ast::ModuleReference::ExternalModuleReference(reference) =
                        import.module_reference?
                    else {
                        return None;
                    };
                    let tsr_ast::Expression::StringLiteral(specifier) = reference.expression?
                    else {
                        return None;
                    };
                    let file = nodes
                        .ancestors(declaration)
                        .find(|&node| nodes.kind(node) == SyntaxKind::SourceFile)?;
                    let module = self.merged_symbol(resolve(
                        self,
                        file,
                        specifier.text,
                        specifier.node_id?,
                        false,
                    )?);
                    self.symbols
                        .get(module)
                        .exports
                        .get(binder::INTERNAL_EXPORT_EQUALS)
                        .copied()
                        .unwrap_or(module)
                }
                _ => return None,
            };
            self.augmentation_alias_target(nodes, node_map, target, resolve, active)
        })();
        active.pop();
        result
    }

    /// `getExportsOfModule(module)[name]` for a name the module does not
    /// export itself: the member its `export *` declarations re-export,
    /// depth-first in declaration order (`getExportsOfModuleWorker`'s
    /// `visit`, `checker.go`). `default` is never re-exported by a star, and
    /// two stars naming different symbols make the name ambiguous, which
    /// upstream's `extendExportStars` drops.
    fn export_star_member(
        &self,
        nodes: &NodeTable,
        node_map: &NodeMap<'a>,
        module: SymbolId,
        name: &str,
        resolve: &mut impl FnMut(&Self, NodeId, &str, NodeId, bool) -> Option<SymbolId>,
        visited: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        if name == binder::INTERNAL_DEFAULT || name == binder::INTERNAL_EXPORT_EQUALS {
            return None;
        }
        if visited.contains(&module) {
            return None;
        }
        visited.push(module);
        let star = *self.symbols.get(module).exports.get(binder::INTERNAL_EXPORT_STAR)?;
        let mut found: Option<SymbolId> = None;
        for &declaration in &self.symbols.get(star).declarations {
            let Some(tsr_ast::Node::ExportDeclaration(export)) = node_map.get(declaration) else {
                continue;
            };
            let Some(specifier) = export.module_specifier.and_then(|specifier| specifier.node_id())
            else {
                continue;
            };
            let text = match node_map.get(specifier) {
                Some(tsr_ast::Node::StringLiteral(literal)) => literal.text,
                _ => continue,
            };
            let Some(file) = std::iter::once(declaration)
                .chain(nodes.ancestors(declaration))
                .find(|&node| nodes.kind(node) == SyntaxKind::SourceFile)
            else {
                continue;
            };
            let Some(nested) = resolve(self, file, text, specifier, false).and_then(|nested| {
                self.resolve_external_module_symbol(
                    nodes,
                    node_map,
                    self.merged_symbol(nested),
                    resolve,
                )
            }) else {
                continue;
            };
            let candidate = match self.symbols.get(nested).exports.get(name) {
                Some(&own) => Some(self.merged_symbol(own)),
                None => self.export_star_member(nodes, node_map, nested, name, resolve, visited),
            };
            match (found, candidate) {
                (_, None) => {}
                (None, Some(candidate)) => found = Some(candidate),
                (Some(previous), Some(candidate)) if previous == candidate => {}
                (Some(_), Some(_)) => return None,
            }
        }
        found
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
        self.resolve_name_with_alias_meaning(
            nodes,
            node_map,
            start,
            name,
            meaning,
            &mut |_, _| Some(false),
            false,
        )
    }

    /// Pinned 5b1047d nameresolver.go:99 / checker.go:2176 delegates exported
    /// alias meaning to the checker while retaining this exact ancestor walk.
    /// The callback handles only nonambient external import-equals exports:
    /// `Some(false)` continues outward; `None` declines an unsupported target
    /// without claiming an outer binding. No symbols or scope tables change.
    pub fn resolve_name_with_export_alias(
        &self,
        nodes: &NodeTable,
        node_map: &NodeMap<'a>,
        start: NodeId,
        name: &str,
        meaning: SymbolFlags,
        mut exported_alias: impl FnMut(SymbolId, SymbolFlags) -> Option<bool>,
    ) -> Option<SymbolId> {
        self.resolve_name_with_alias_meaning(
            nodes,
            node_map,
            start,
            name,
            meaning,
            &mut exported_alias,
            true,
        )
    }

    /// The shared body of [`Self::resolve_name`] and
    /// [`Self::resolve_name_with_export_alias`]. `filter_local_aliases` is
    /// set only when `alias_meaning` is the checker's real target-meaning
    /// answer, never for `resolve_name`'s constant `Some(false)`.
    #[allow(clippy::too_many_arguments)]
    fn resolve_name_with_alias_meaning(
        &self,
        nodes: &NodeTable,
        node_map: &NodeMap<'a>,
        start: NodeId,
        name: &str,
        meaning: SymbolFlags,
        alias_meaning: &mut impl FnMut(SymbolId, SymbolFlags) -> Option<bool>,
        filter_local_aliases: bool,
    ) -> Option<SymbolId> {
        let resolved = self.resolve_name_excluding_with_export_alias(
            nodes,
            node_map,
            start,
            name,
            meaning,
            None,
            alias_meaning,
            filter_local_aliases,
        )?;
        // §265. `declare global { … }` does NOT declare a binding called
        // `global` — the keyword is syntax, not a name. Upstream reports
        // `TS2304: Cannot find name 'global'` for `global.x` (checker-1's
        // probe, twice in one file, while `globalThis` resolves normally in
        // the same file), so upstream's `>global : any` is a rendered
        // errorType and every step after the resolution is already right here.
        //
        // The parser gives the block a SYNTHETIC `global` identifier for a
        // name — see `is_merged_global_augmentation` — and that name is
        // load-bearing: this binder merges augmentations across files by
        // matching it, which is why checker-1's §234 renaming it to
        // `__global` measured **0 won / 5 lost**. So the declaration keeps its
        // name and the REFERENCE is refused instead.
        //
        // Refused only when EVERY declaration of the resolved symbol is a
        // global-scope augmentation. A real `namespace global { … }` is a
        // different construct with the same name and must keep resolving; the
        // keyword is what separates them, exactly as it does in
        // `is_merged_global_augmentation`.
        if name == "global" {
            let declarations = &self.symbols().get(resolved).declarations;
            let all_global_augmentations = !declarations.is_empty()
                && declarations.iter().all(|&declaration| {
                    matches!(node_map.get(declaration), Some(tsr_ast::Node::ModuleDeclaration(module))
                        if module.keyword.kind == tsr_ast::SyntaxKind::GlobalKeyword)
                });
            if all_global_augmentations {
                return None;
            }
        }
        Some(resolved)
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
        self.resolve_name_excluding_with_export_alias(
            nodes,
            node_map,
            start,
            name,
            meaning,
            exclude,
            &mut |_, _| Some(false),
            false,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_name_excluding_with_export_alias(
        &self,
        nodes: &NodeTable,
        node_map: &NodeMap<'a>,
        start: NodeId,
        name: &str,
        meaning: SymbolFlags,
        exclude: Option<NodeId>,
        exported_alias: &mut impl FnMut(SymbolId, SymbolFlags) -> Option<bool>,
        filter_local_aliases: bool,
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
            // `!ast.IsGlobalSourceFile(location)` (`binder/nameresolver.go:50`):
            // a script file's locals are never consulted — every one of them
            // was merged into `globals` (`merge_globals`), and a name whose
            // merge was refused resolves to the global that refused it, as
            // upstream's walk reaches `c.globals` instead. A script source
            // file is the one without a file symbol
            // (`bindSourceFileIfExternalModule`).
            let global_source_file =
                nodes.kind(node) == SyntaxKind::SourceFile && self.symbol_of(node).is_none();
            if !global_source_file
                && let Some(found) = self.lookup_scoped(self.locals.get(&node), name, meaning)
                // `getSymbol`'s alias arm (`checker.go:2183`) on the locals
                // table: an alias whose own flags lack `meaning` is a hit only
                // when its target's flags carry it. Asked of the checker
                // (`filter_local_aliases` is set only by its wrapper), and only
                // a definite `Some(false)` rejects — an unsupported target
                // keeps the binder's accept. A rejected local falls through to
                // the location's exports arm and then outward, as upstream's
                // nil `result` does. `docs/parity/notes/names-modules.md` §1.
                && !(filter_local_aliases
                    && !self.symbols.get(found).flags.intersects(meaning)
                    && self.symbols.get(found).flags.intersects(SymbolFlags::ALIAS)
                    && exported_alias(found, meaning) == Some(false))
                && !self.symbol_is_declared_within(found, exclude, nodes)
                && !self.parameter_hidden_from_type_parameter_list(found, node, last, nodes)
                && !self.local_type_hidden_outside_body(found, node, last, meaning, nodes, node_map)
                // resolveNameHelper exposes conditional infer locals only in
                // the true branch; check/extends/false continue outward.
                && !matches!(node_map.get(node), Some(tsr_ast::Node::ConditionalTypeNode(conditional))
                    if conditional.true_type.and_then(|node| tsr_ast::Node::from(node).node_id()) != last)
            {
                return Some(found);
            }
            // A named function expression's own name is in scope inside it.
            // The historical refusal (§216/§217) and its prerequisite:
            //
            // The arm is four lines and upstream's is at
            // `nameresolver.go:233-244`: `bindFunctionExpression` gives the name
            // a symbol in NO symbol table, so no `locals` lookup can find it and
            // the walk compares the name being resolved against the function
            // expression's written name. Built, measured at **+1 case / −10
            // lines**, reverted.
            //
            // # The blocker, and the first diagnosis of it was WRONG
            //
            // Resolving `y` inside `function y() { return y; }` closes a type
            // cycle through the function's own inferred return. §216 recorded
            // the blocker as "the return-type cycle answers `errorType` where
            // upstream's `getReturnTypeOfSignature` answers `anyType`
            // (`checker.go:20020`)", and named a one-function falsifier: give
            // the return inference its own resolution key.
            //
            // **§217 built exactly that and it changed nothing** — same +1/−10.
            // The cycle never reaches the return key, because it is caught one
            // level out at `(symbol, Type)`:
            // `get_type_of_func_class_enum_module` (`symbols.rs:1842`) pushes
            // that key and its worker builds the signature's return type
            // EAGERLY, so the re-entry lands on the symbol before the return
            // guard can see it.
            //
            // Upstream does not have that shape. `getTypeOfFuncClassEnumModule`
            // answers an anonymous type immediately and computes each
            // signature's return **on demand**, so the symbol's resolution has
            // already completed by the time any body is checked. The real
            // blocker is therefore **eager return types inside the symbol's
            // type** — an architectural difference, not a missing key — and
            // that is a deferred-signature-return change, not a four-line arm.
            //
            // FALSIFIER, corrected: make `get_signatures_of_symbol` produce
            // signatures whose return type is computed lazily, then re-apply the
            // four lines. If `function y() { return y; }` reads `() => any`, the
            // row converts. §217's resolution key is the *second* half of that
            // and was reverted with this, being unreachable without the first.
            // tsr-6.47.1 installs the stable callable identity before any return
            // slot resolves, removing that whole-symbol failure. Restore the
            // pinned native arm after locals, with the existing exclusion rule.
            if meaning.intersects(SymbolFlags::FUNCTION)
                && let Some(tsr_ast::Node::FunctionExpression(function)) = node_map.get(node)
                && function.name.is_some_and(|written| written.text == name)
                && let Some(symbol) = self.symbol_of(node)
                && !self.symbol_is_declared_within(symbol, exclude, nodes)
            {
                return Some(symbol);
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
                return Some(self.merged_symbol(found));
            }
            // A named class expression's own name, in its body
            // (`nameresolver.go:189-195`): reached only when the members
            // lookup missed (a type parameter declared elsewhere `break`s out
            // of the case above). `bindClassLikeDeclaration` gives the name a
            // symbol in no table, so only this comparison finds it.
            // names-modules notes §8.
            if meaning.intersects(SymbolFlags::CLASS)
                && let Some(tsr_ast::Node::ClassExpression(class)) = node_map.get(node)
                && class.name.is_some_and(|written| written.text == name)
                && let Some(symbol) = self.symbol_of(node)
            {
                return Some(symbol);
            }
            // The **export-default local name** (`nameresolver.go:109`–`:119`):
            // in an external module, or an ambient module declaration that is
            // not `declare global`, `export default class Foo {}` exports
            // `default` and leaves only an `EXPORT_VALUE` marker named `Foo`
            // in `locals`, which no value/type/namespace meaning matches. So
            // upstream first reads `moduleExports["default"]` and answers it
            // when its local symbol (`GetLocalSymbolForExportDefault`,
            // `nameresolver.go:442`) carries the written name and the export
            // matches the meaning — before the `moduleExports[name]` arm below.
            //
            // `isExportDefaultSymbol` is the first declaration's syntactic
            // `default`. This binder keeps no node-to-local link, so "its local
            // symbol is named `name`" is read from the other end: the marker
            // `locals[name]` in this container (class/function markers and
            // the rest are declared into the container's own table) whose
            // `export_symbol` is that `default`.
            if (match node_map.get(node) {
                Some(tsr_ast::Node::SourceFile(_)) => true,
                Some(tsr_ast::Node::ModuleDeclaration(module)) => {
                    nodes.flags(node).contains(tsr_ast::NodeFlags::AMBIENT)
                        && module.keyword.kind != SyntaxKind::GlobalKeyword
                }
                _ => false,
            }) && let Some(module) = self.symbol_of(node)
                && let Some(&default) = self
                    .symbols
                    .get(self.merged_symbol(module))
                    .exports
                    .get(binder::INTERNAL_DEFAULT)
                && self.symbols.get(default).declarations.first().is_some_and(|&declaration| {
                    node_map.get(declaration).and_then(binder::modifiers_of).is_some_and(
                        |modifiers| binder::has_modifier(modifiers, SyntaxKind::DefaultKeyword),
                    )
                })
                && self
                    .locals
                    .get(&node)
                    .and_then(|locals| locals.get(name))
                    .is_some_and(|&local| self.symbols.get(local).export_symbol == Some(default))
                && self.symbols.get(self.merged_symbol(default)).flags.intersects(meaning)
                && !self.symbol_is_declared_within(default, exclude, nodes)
            {
                return Some(self.merged_symbol(default));
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
                && let Some(&found) = self.symbols.get(self.merged_symbol(symbol)).exports.get(name)
                && (self.symbols.get(self.merged_symbol(found)).flags != SymbolFlags::ALIAS
                    || !self.symbols.get(self.merged_symbol(found)).declarations.iter().any(|&d| {
                        matches!(
                            nodes.kind(d),
                            SyntaxKind::ExportSpecifier | SyntaxKind::NamespaceExport
                        )
                    }))
                // An `export { X }` specifier's own symbol lives in the file's
                // **exports**, not its `locals`, so the exclusion belongs on
                // this arm too — §836's first measurement found the specifier
                // here after the `locals` arm had been filtered.
                && !self.symbol_is_declared_within(found, exclude, nodes)
            {
                let found = self.merged_symbol(found);
                let entry = self.symbols.get(found);
                if entry.flags.intersects(meaning & mask) {
                    return Some(found);
                }
                if !nodes.flags(node).contains(tsr_ast::NodeFlags::AMBIENT)
                    && entry.flags.intersects(SymbolFlags::ALIAS)
                {
                    // Preserve the existing qualified import-equals admission.
                    // Only external require aliases use the checker callback.
                    for &declaration in &entry.declarations {
                        if let Some(tsr_ast::Node::ImportEqualsDeclaration(alias)) =
                            node_map.get(declaration)
                        {
                            match alias.module_reference {
                                Some(tsr_ast::ModuleReference::QualifiedName(_)) => {
                                    return Some(found);
                                }
                                Some(tsr_ast::ModuleReference::ExternalModuleReference(_))
                                    if exported_alias(found, meaning & mask)? =>
                                {
                                    return Some(found);
                                }
                                _ => {}
                            }
                        }
                    }
                }
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
                if let Some(table) = symbol.members.as_ref() {
                    push(&mut names, table);
                }
                if let Some(table) = symbol.exports.as_ref() {
                    push(&mut names, table);
                }
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

#[cfg(test)]
mod ambient_lookup_tests {
    use super::{BindResult, SymbolFlags};

    #[test]
    fn quoted_keys_preserve_utf8_and_inline_boundary_names() {
        let names = [
            String::new(),
            "process".to_owned(),
            "x".repeat(62),
            "x".repeat(63),
            "é".repeat(31),
            "é".repeat(32),
            "café/日本語/🦀".to_owned(),
            "embedded\"quote\\slash\nnewline".to_owned(),
            "./relative".to_owned(),
            "C:\\rooted".to_owned(),
            "*.widgets".to_owned(),
        ];
        let keys: Vec<_> = names.iter().map(|name| format!("\"{name}\"")).collect();
        let mut bound = BindResult::empty();
        let mut symbols = Vec::new();
        for key in &keys {
            let symbol = bound.symbols.create(key, SymbolFlags::VALUE_MODULE);
            bound.globals.insert(key, symbol);
            symbols.push(symbol);
        }
        // The unquoted global and quoted ambient module are distinct domains.
        let ordinary = bound.symbols.create("process", SymbolFlags::FUNCTION_SCOPED_VARIABLE);
        bound.globals.insert("process", ordinary);
        for (name, symbol) in names.iter().zip(symbols) {
            assert_eq!(bound.ambient_module(name), Some(symbol), "{name:?}");
            assert_eq!(bound.ambient_module(&format!("{name}/missing")), None);
        }
        assert_ne!(bound.ambient_module("process"), Some(ordinary));
        // Wildcard keys remain exact at this API; matching is the caller's policy.
        assert_eq!(bound.ambient_module("one.widgets"), None);
    }

    #[test]
    fn ambient_lookup_follows_merged_symbol_redirects() {
        let mut bound = BindResult::empty();
        let first = bound.symbols.create("\"m\"", SymbolFlags::VALUE_MODULE);
        let second = bound.symbols.create("\"m\"", SymbolFlags::VALUE_MODULE);
        let final_symbol = bound.symbols.create("\"m\"", SymbolFlags::VALUE_MODULE);
        bound.globals.insert("\"m\"", first);
        bound.merged.insert(first, second);
        bound.merged.insert(second, final_symbol);
        assert_eq!(bound.ambient_module("m"), Some(final_symbol));
        assert_eq!(bound.ambient_module("missing"), None);
    }
}
