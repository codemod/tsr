//! The walk: create symbols, put them in the right table, and build the
//! control-flow graph.
//!
//! Ported from `internal/binder/binder.go`. The structure follows upstream's:
//! `bind` declares, `bindContainer` saves the surrounding scope and flow state
//! and restores it, `bindChildren` dispatches on kind to whichever flow shape
//! the construct needs.
//!
//! # Why this is a struct with fields rather than threaded arguments
//!
//! The symbol half of the binder used to thread a `Scope` value through the
//! recursion, which read well: a scope is a property of a *position* in the
//! tree, so passing it down and letting it expire on the way out is exactly
//! right.
//!
//! Control flow is not like that. `currentFlow` is a cursor that moves
//! *forward* across siblings — after `if (a) return;`, the next statement's flow
//! is what the `if` left behind, not what the enclosing block started with — so
//! a by-value parameter would restore the wrong thing at every statement
//! boundary. Nine such cursors (`currentFlow`, the break/continue/return/
//! true/false/exception targets, the pre-switch flow, the active labels)
//! interact, and each has its own save-and-restore *scope*, which is not the
//! same as the recursion's. That is why upstream keeps them as fields with
//! explicit saves, and why doing anything else here would be a rewrite of the
//! algorithm rather than a port of it.
//!
//! The lexical scope moved into fields alongside them, for consistency and
//! because `bindContainer` is now one place that saves both halves.
//!
//! # What is written where
//!
//! Upstream writes its results back into the AST: `node.FlowNode`,
//! `node.Flags |= Unreachable`, `bodyData.EndFlowNode`. None of that is
//! available here — the AST is immutable and `Sync`
//! ([ADR-0012](../../../docs/adr/0012-ast-is-sync.md)) and the binder only holds
//! `&NodeTable` — so each lands in a side table on [`BindResult`] instead. The
//! dense ones (`node -> flow`) are vectors; the sparse ones (unreachable code,
//! per-function facts) are maps, because a file with no unreachable code should
//! pay nothing for the possibility.

use tsr_ast::{
    BindingName, Expression, ForInitializer, Node, NodeFlags, NodeId, NodeTable,
    ObjectLiteralElementLike, SourceFile, Statement, SyntaxKind, push_children,
};
use tsr_core::Idx as _;
use tsr_diagnostics::{Diagnostic, messages};

use crate::{
    BindResult, FileInfo, NodeFacts,
    container::{ContainerFlags, container_flags},
    flow::{FlowFlags, FlowId, FlowStore},
    narrowing::{
        contains_narrowable_reference, expression_of, is_assignment_operator, is_dotted_name,
        is_expression_of_optional_chain_root, is_left_hand_side_expression,
        is_logical_assignment_expression, is_logical_expression,
        is_logical_or_coalescing_assignment_operator, is_logical_or_coalescing_binary_operator,
        is_narrowable_reference, is_narrowing_expression, is_nullish_coalesce, is_optional_chain,
        is_optional_chain_root, is_outermost_optional_chain, is_potentially_executable_node,
        is_push_or_unshift_identifier, skip_parentheses,
    },
    symbol::{SymbolFlags, SymbolId, SymbolStore, SymbolTable},
};

/// Where a declaration's symbol belongs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Destination {
    /// The enclosing container's `locals` — ordinary lexical scope.
    Locals,
    /// The container symbol's `members` — class, interface, enum, type literal.
    Members,
    /// The container symbol's `exports` — module, namespace, or source file.
    ///
    /// Reached by a namespace or module member: explicitly, with `export`, or
    /// implicitly inside an ambient module.
    Exports,
    /// The file's `global_exports` — `export as namespace N` only.
    ///
    /// Upstream's `file.GlobalExports`: a UMD module's global name is not an
    /// export of the module, it is a *global* the module claims, so it cannot go
    /// in either table without changing what the checker resolves.
    GlobalExports,
}

/// A `label:` in scope, and where `break label` / `continue label` go.
///
/// Upstream threads these as a linked list (`ActiveLabel.next`) allocated per
/// labelled statement. A stack is the same structure without the allocations;
/// `labels_base` hides the labels of an enclosing function, which upstream does
/// by setting the list head to `nil` and restoring it.
/// One `f.x = 1`, and the scope it was written in.
///
/// Upstream's `ExpandoAssignmentInfo`. The scope is recorded rather than
/// recomputed because the deferred pass runs with the cursors wherever the walk
/// left them.
#[derive(Clone, Copy)]
struct ExpandoAssignment<'a> {
    node: Node<'a>,
    id: NodeId,
    container: NodeId,
    block: NodeId,
}

struct ActiveLabel<'a> {
    name: &'a str,
    break_target: FlowId,
    continue_target: Option<FlowId>,
    referenced: bool,
}

/// How often `bind()` checks for remaining stack.
///
/// Every 32nd level, not every level. `bind()` is the binder's hottest path and
/// checking on each one measured about 2% on parse+bind over `checker.ts`; at this
/// interval the same benchmark is back inside run-to-run noise. The safety
/// condition is that 32 levels of frames fit inside the red zone
/// [`tsr_core::stack::ensure_sufficient`] keeps in reserve: the binder's frames are
/// roughly 800 bytes in debug, so 32 of them is about 26 KiB against 100 KiB.
const STACK_CHECK_INTERVAL: u32 = 32;

// `bind()` recursion depth is measured but no longer bounded. The walk recurses
// once per tree level and the corpus contains a 4,958-deep left-leaning chain
// (`compiler/binderBinaryExpressionStress`), which overflowed a fixed stack. It is
// now wrapped in `tsr_core::stack::ensure_sufficient`, growing the stack on demand
// as rustc does — matching upstream, whose `internal/binder/binder.go:2212`
// recurses plainly because Go grows goroutine stacks itself.
//
// A depth *limit* was implemented first and abandoned: it required every recursive
// walk to opt in, and `tsr_ast::visit::walk_node` — a public visitor with
// user-written impls — has no natural place to keep the counter. See
// `docs/adr/0030-grow-the-stack-natively-wasm-traps.md`.
//
// The measurement is kept because it still sizes *wasm's* link-time stack, which
// cannot grow (`bd tsr-el3.3`). Over 16,207 corpus files: p50 7, p99.9 25, deepest
// non-chain file 285, deepest overall 4,958.

// Nine independent cursors is what the algorithm is; collapsing them into a
// state machine would hide the save-and-restore structure that makes it
// checkable against upstream line by line.
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct Binder<'a, 'n> {
    /// The arena symbol names live in — almost every name borrows from the
    /// source, and the exceptions are the canonical numeric spellings
    /// upstream's scanner computes into its token value
    /// (`scanner.go:2194`, `jsnum.FromString(…).String()`).
    arena: crate::names::Names<'a, 'n>,
    /// Current and peak `bind()` recursion depth. Measured, not bounded; see the
    /// note above this struct and `bd tsr-el3.3`.
    depth: u32,
    max_depth: u32,
    nodes: &'n NodeTable,
    symbols: SymbolStore<'a>,
    /// `node -> the symbol it declares`. Dense, because node ids are dense.
    node_symbols: Vec<Option<SymbolId>>,
    node_base: usize,
    /// `container node -> its locals`. Sparse: most nodes are not containers.
    locals: rustc_hash::FxHashMap<NodeId, SymbolTable<'a>>,
    diagnostics: Vec<Diagnostic>,

    // ---- lexical scope ----
    /// Nearest node with a `locals` table.
    container: NodeId,
    /// Nearest `IsContainer`, including object literals without locals.
    /// Deferred expando lookup uses this upstream cursor rather than the
    /// lexical `HasLocals` cursor above.
    expando_container: NodeId,
    /// Nearest block, for `let`/`const` and class declarations.
    block: NodeId,
    /// Symbol of the nearest container that owns members or exports.
    owner: Option<SymbolId>,
    /// The file's source text.
    ///
    /// Read only where a name is a *range* of the source that no single node
    /// holds; see [`FileInfo`].
    source: &'a str,
    /// Whether the file being bound is a `.d.ts`, where an unexported
    /// namespace local still prints qualified.
    in_declaration_file: bool,
    /// Whether the file being bound is a JavaScript file.
    ///
    /// Upstream reads `node.Flags & NodeFlagsJavaScriptFile`, which the parser
    /// sets; here it comes from the file name, like everything else in
    /// [ADR-0015](../../../docs/adr/0015-file-name-is-a-bind-input.md). It gates
    /// the assignment-declaration forms in `binder.go:618` — `module.exports =`,
    /// `exports.x =`, `this.x =` — which are declarations in a `.js` file and
    /// plain assignments in a `.ts` one.
    in_js_file: bool,
    /// The file node, for the two places that reach it after the walk has moved
    /// on: `module.exports =` nested inside a function, and the `module` and
    /// `exports` locals declared once the file has been walked.
    file_node: NodeId,
    /// What the file's own symbol is called if it turns out to need one.
    file_symbol_name: &'a str,
    /// The file's own symbol, once it has one.
    ///
    /// Upstream's `file.Symbol`. Distinct from [`Self::owner`], which is a
    /// *cursor*: a `module.exports =` nested inside a function must still reach
    /// the file's symbol, and by then `owner` is something else entirely.
    module_symbol: Option<SymbolId>,
    /// Whether a `CommonJS` export form has been seen (`file.CommonJSModuleIndicator`).
    ///
    /// Set by the first `module.exports =` or `exports.x =`, which is also what
    /// turns a `.js` script into a module and what makes `module` and `exports`
    /// locals of the file.
    commonjs_module: bool,
    /// Native reparsed `node.Type` presence for `CommonJS` require-alias admission.
    /// Owned by this file's binder only, filled from parsed @type attachments
    /// before binding; statement and declaration hosts retain `NodeId` identity.
    /// This immutable admission set does not cache any checker computation.
    jsdoc_type_hosts: rustc_hash::FxHashSet<NodeId>,
    /// `declaration -> the computed name it was written with`.
    ///
    /// Recorded for every declaration whose name is a `ComputedPropertyName`,
    /// late-bound or not. A consumer sees declarations as ids and the name is a
    /// *child*, which the tree has no edge to follow
    /// ([ADR-0003](../../../docs/adr/0003-tree-plus-side-tables.md)); the
    /// checker's late binding and the conformance harness both need to read the
    /// expression back.
    computed_names: rustc_hash::FxHashMap<NodeId, NodeId>,
    /// Declaration node → its name node, for anchoring redeclaration diagnostics.
    ///
    /// A `NodeId` alone cannot reach the tree — [`NodeTable`] holds kind, span,
    /// flags and parent, not the node — and a redeclaration has to be reported on
    /// declarations bound earlier, which are known only by id. Recording the name
    /// as each declaration is bound is what makes that possible.
    name_nodes: rustc_hash::FxHashMap<NodeId, NodeId>,
    /// Expando assignments (`f.x = 1`), and the scope each was written in.
    ///
    /// Bound in a second pass: the target may be declared further down the file
    /// than the assignment that extends it.
    expando_assignments: Vec<ExpandoAssignment<'a>>,
    /// `declaration -> the function, class, or empty object literal it is
    /// initialised with`, for the declarations that can carry expando properties.
    ///
    /// Upstream reads `declaration.Initializer()` in the deferred pass; the tree
    /// here has no parent-to-child edge to follow from a `NodeId`
    /// ([ADR-0003](../../../docs/adr/0003-tree-plus-side-tables.md)), so the id
    /// is recorded on the way past, where the node is in hand. Only expando-shaped
    /// initializers are recorded, so the map stays small.
    expando_initializers: rustc_hash::FxHashMap<NodeId, NodeId>,
    /// The nearest node that binds its own `this` (`b.thisContainer`).
    ///
    /// `this.x = 1` declares a property on whatever this names, so the binder
    /// needs the container as well as the scope. It is not the same cursor as
    /// [`Self::container`]: an arrow function is a container and is not a `this`
    /// container, which is the entire point of the syntax.
    this_container: NodeId,
    /// Whether the file being bound is an external module rather than a script.
    ///
    /// Upstream's `ast.IsExternalModule(b.file)`, which reads an indicator the
    /// *parser* recorded; see [`is_external_module`] for why the binder computes
    /// it instead.
    is_module: bool,
    /// Names a UMD module claims globally (`export as namespace N`).
    ///
    /// Upstream's `file.GlobalExports`. Separate from the module's exports
    /// because it is a different question: what the file is called when loaded
    /// as a script, not what it exports.
    global_exports: SymbolTable<'a>,
    /// Every name visible to the whole program, merged across its script files.
    ///
    /// Upstream's `c.globals` (`internal/checker/checker.go:930`), filled by
    /// `initializeChecker` (`:1296`). See [`Binder::merge_globals`] for why it
    /// is filled here rather than in the checker.
    /// Source-to-target redirects recorded by [`Binder::merge_symbol`].
    ///
    /// The port of `c.mergedSymbols` (`internal/checker/checker.go:666`). It
    /// lives on the binder rather than the checker because **this port merges in
    /// the binder** — `BindResult` is one program (ADR-0034), so `merge_symbol`
    /// mutates in place where upstream clones, and the redirect has to be
    /// recorded where the merge happens.
    merged: rustc_hash::FxHashMap<SymbolId, SymbolId>,
    /// Merges the excludes masks forbade; see [`BindResult::merge_conflicts`].
    merge_conflicts: Vec<(SymbolId, SymbolId)>,
    /// The declaration [`Self::declare`] is binding, when it is a default
    /// export in `declareSymbolEx`'s sense — `isDefaultExport`, or an
    /// `export default` assignment — which is what selects TS2528 over TS2300
    /// on a conflict (`binder.go:224-244`). `docs/parity/notes/decls.md` §14.
    default_export_declaration: Option<NodeId>,
    globals: SymbolTable<'a>,
    /// The synthesised `undefined` symbol, if this bind created one.
    undefined_symbol: Option<SymbolId>,

    // ---- control flow ----
    flow: FlowStore,
    /// `node -> the flow node in effect where it appears`. Dense: four bytes a
    /// node, against a hash map's per-entry overhead on what ends up being most
    /// statements and every identifier in the file.
    node_flow: Vec<Option<FlowId>>,
    current_flow: FlowId,
    current_break_target: Option<FlowId>,
    current_continue_target: Option<FlowId>,
    current_return_target: Option<FlowId>,
    current_true_target: Option<FlowId>,
    current_false_target: Option<FlowId>,
    current_exception_target: Option<FlowId>,
    pre_switch_case_flow: Option<FlowId>,
    active_labels: Vec<ActiveLabel<'a>>,
    /// Labels below this index belong to an enclosing function and are invisible.
    labels_base: usize,
    has_explicit_return: bool,
    has_flow_effects: bool,
    /// Whether declarations in the current container are implicitly exported.
    ///
    /// Upstream keeps this as `NodeFlagsExportContext`, set by
    /// `setExportContextFlag` on an ambient module with no `export`
    /// declarations. Our parser does not record ambience on nodes, so the binder
    /// tracks it the way the parser would have: set on entering a `declare`
    /// module or one named by a string literal, and inherited by nested ones.
    export_context: bool,
    /// Whether we are inside a `declare` module.
    in_ambient_module: bool,
    /// The symbols of this file's `declare global { … }` / `global { … }` blocks
    /// whose exports merge into [`Binder::globals`].
    ///
    /// Upstream's `SourceFile.ModuleAugmentations`, filtered to the global ones
    /// — the list `initializeChecker` walks before it builds the global types
    /// (`internal/checker/checker.go:1335-1343`). Per file: [`merge_globals`]
    /// drains it at the end of each `bind_source_file`, so `resuming` starts a
    /// new file with an empty one.
    ///
    /// [`merge_globals`]: Binder::merge_globals
    global_augmentations: Vec<SymbolId>,
    /// Every file's non-global external module augmentations, in file then
    /// source order: upstream's `SourceFile.ModuleAugmentations` minus the
    /// `declare global` entries, which [`Binder::global_augmentations`]
    /// handles per file. Carried across files, because the merge
    /// ([`crate::BindResult::merge_module_augmentations`]) needs every file
    /// bound and module resolution, exactly as `initializeChecker`'s last
    /// loop (`checker.go:1384-1391`) runs after everything else.
    module_augmentations: Vec<crate::ModuleAugmentation<'a>>,
    /// Carried through for [`crate::BindResult::pattern_ambient_module`];
    /// filled only by the augmentation merge.
    pattern_ambient_module_augmentations: rustc_hash::FxHashMap<&'a str, SymbolId>,
    in_assignment_pattern: bool,
    seen_this_keyword: bool,

    // ---- derived facts, upstream's `node.Flags |= …` ----
    facts: rustc_hash::FxHashMap<NodeId, NodeFacts>,
    end_flow: rustc_hash::FxHashMap<NodeId, FlowId>,
    return_flow: rustc_hash::FxHashMap<NodeId, FlowId>,
    fallthrough_flow: rustc_hash::FxHashMap<NodeId, FlowId>,

    // ---- traversal scratch ----
    /// The chain from the root to the node being bound, innermost last.
    ///
    /// Upstream reads `node.Parent`; the tree has no back-edges and
    /// [`NodeTable`] answers with an id, so the handful of predicates that need
    /// an ancestor as a *node* read it from here. Bounded by source nesting
    /// depth, so a few hundred entries at worst.
    ancestors: Vec<(NodeId, Node<'a>)>,
    /// One shared stack for the generic child walk, rather than a `Vec` per
    /// level. Each frame appends above the previous one's high-water mark and
    /// truncates back to it on the way out.
    children: Vec<Node<'a>>,
}

impl<'a, 'n> Binder<'a, 'n> {
    pub(crate) fn bind_independent(
        names: &'n crate::PreparedNames<'a>,
        nodes: &'n NodeTable,
        file: &'a SourceFile<'a>,
        info: FileInfo<'a>,
        jsdoc: &[(NodeId, &'a [&'a tsr_ast::JSDoc<'a>])],
        range: std::ops::Range<u32>,
    ) -> crate::FileBindResult<'a, 'n> {
        assert!(
            range.start <= range.end && range.end as usize <= nodes.len(),
            "invalid file node range"
        );
        assert!(
            file.node_id.is_some_and(|id| range.contains(&id.as_u32())),
            "source file is outside its node range"
        );
        let mut binder = Self::resuming_with_names(
            crate::names::Names::Prepared(names),
            nodes,
            BindResult::empty(),
            range.start as usize,
            (range.end - range.start) as usize,
        )
        .bind_file_body(file, info, jsdoc);
        assert!(binder.global_exports.is_empty(), "UMD alias declarations require ordered binding");
        let root = file.node_id.expect("registered source file");
        let node_base = binder.node_base;
        let is_module = binder.is_module;
        let commonjs_module = binder.commonjs_module;
        let global_augmentations = std::mem::take(&mut binder.global_augmentations);
        crate::FileBindResult {
            nodes,
            bindings: binder.into_result(),
            node_base,
            root,
            is_module,
            commonjs_module,
            global_augmentations,
        }
    }

    pub(crate) fn publish_file(
        arena: &'a tsr_core::Arena,
        nodes: &'n NodeTable,
        previous: BindResult<'a>,
        local: crate::FileBindResult<'a, '_>,
    ) -> BindResult<'a> {
        assert!(std::ptr::eq(nodes, local.nodes), "file bind belongs to a different node table");
        let mut binder = Self::resuming(arena, nodes, previous);
        let crate::FileBindResult {
            nodes: _,
            bindings,
            node_base,
            root,
            is_module,
            commonjs_module,
            global_augmentations,
        } = local;
        let BindResult {
            max_depth,
            symbols,
            node_symbols,
            locals,
            global_exports,
            globals,
            merged,
            merge_conflicts,
            module_augmentations,
            pattern_ambient_module_augmentations,
            undefined_symbol,
            computed_names,
            diagnostics,
            flow,
            node_flow,
            facts,
            end_flow,
            return_flow,
            fallthrough_flow,
        } = bindings;
        assert!(globals.is_empty() && global_exports.is_empty() && undefined_symbol.is_none());
        let symbol_base = binder.symbols.append(symbols);
        let symbol = |id: SymbolId| id.relocated(symbol_base);
        let flow_base = binder.flow.append(flow);
        let flow = |id| crate::flow::relocate_flow(id, flow_base);
        binder.max_depth = binder.max_depth.max(max_depth);
        for (slot, id) in binder.node_symbols[node_base..node_base + node_symbols.len()]
            .iter_mut()
            .zip(node_symbols)
        {
            *slot = id.map(symbol);
        }
        for (slot, id) in
            binder.node_flow[node_base..node_base + node_flow.len()].iter_mut().zip(node_flow)
        {
            *slot = id.map(flow);
        }
        binder.locals.extend(locals.into_iter().map(|(node, mut table)| {
            for id in table.values_mut() {
                *id = symbol(*id);
            }
            (node, table)
        }));
        binder
            .merged
            .extend(merged.into_iter().map(|(source, target)| (symbol(source), symbol(target))));
        binder.merge_conflicts.extend(
            merge_conflicts.into_iter().map(|(target, source)| (symbol(target), symbol(source))),
        );
        binder.module_augmentations.extend(module_augmentations.into_iter().map(
            |mut augmentation| {
                augmentation.symbol = symbol(augmentation.symbol);
                augmentation
            },
        ));
        binder.pattern_ambient_module_augmentations.extend(
            pattern_ambient_module_augmentations.into_iter().map(|(name, id)| (name, symbol(id))),
        );
        binder.computed_names.extend(computed_names);
        binder.diagnostics.extend(diagnostics);
        binder.facts.extend(facts);
        binder.end_flow.extend(end_flow.into_iter().map(|(node, id)| (node, flow(id))));
        binder.return_flow.extend(return_flow.into_iter().map(|(node, id)| (node, flow(id))));
        binder
            .fallthrough_flow
            .extend(fallthrough_flow.into_iter().map(|(node, id)| (node, flow(id))));
        binder.is_module = is_module;
        binder.commonjs_module = commonjs_module;
        binder.global_augmentations = global_augmentations.into_iter().map(symbol).collect();
        binder.finish_file(root)
    }
    /// A binder that **adds to** what an earlier file of the same program
    /// produced.
    ///
    /// The counterpart of `tsr_parser::parse_into`, and the second half of
    /// ADR-0034's identity widening: symbols from every file of a program live
    /// in one [`SymbolStore`], so a [`SymbolId`] means one thing across the
    /// program and can be handed to a checker that holds the program's node
    /// table.
    ///
    /// Only the *accumulating* state is carried — exactly the fields of
    /// [`BindResult`]. Every per-file cursor (`container`, `owner`,
    /// `this_container`, `commonjs_module`, the flow targets, the ancestor
    /// stack) is initialised fresh here, the same way it always was, so
    /// resuming cannot leak one file's position into the next. That split is
    /// why this takes a `BindResult` rather than a `Binder`: the result is
    /// precisely the set of fields that should survive, and letting the type
    /// system enumerate them means a field added later cannot be silently
    /// forgotten.
    pub(crate) fn resuming(
        arena: &'a tsr_core::Arena,
        nodes: &'n NodeTable,
        previous: BindResult<'a>,
    ) -> Self {
        Self::resuming_with_names(
            crate::names::Names::Arena(arena),
            nodes,
            previous,
            0,
            nodes.len(),
        )
    }

    fn resuming_with_names(
        arena: crate::names::Names<'a, 'n>,
        nodes: &'n NodeTable,
        previous: BindResult<'a>,
        node_base: usize,
        node_count: usize,
    ) -> Self {
        let BindResult {
            max_depth,
            symbols,
            node_symbols,
            locals,
            global_exports,
            globals,
            merged,
            merge_conflicts,
            module_augmentations,
            pattern_ambient_module_augmentations,
            undefined_symbol,
            computed_names,
            diagnostics,
            mut flow,
            node_flow,
            facts,
            end_flow,
            return_flow,
            fallthrough_flow,
        } = previous;

        // Measured: `checker.ts`, `dom.generated.d.ts`, `Herebyfile.mjs` and a
        // `.tsx` fixture come to 81,713 flow nodes for 419,464 AST nodes — one
        // per 5.1. Reserving for one per five costs a few percent of slack and
        // removes every growth reallocation of a megabyte-scale vector.
        //
        // `nodes.len()` is the *program's* node count once files share a table,
        // so this reserves against the whole program on the first file and
        // against nothing much afterwards. Reserving the difference rather than
        // the total is what keeps this linear: sizing a fresh dense vector per
        // file would be O(files × program nodes).
        flow.reserve((node_count / 5 + 16).saturating_sub(flow.len()));
        let unreachable = flow.unreachable();

        // `resize`, not `vec![None; …]`: the earlier files' entries must survive,
        // and only the new file's rows are added. Same reason as above.
        let mut node_symbols = node_symbols;
        let mut node_flow = node_flow;
        node_symbols.resize(node_count, None);
        node_flow.resize(node_count, None);

        Self {
            arena,
            depth: 0,
            max_depth,
            // Carried across files: a second file must not re-synthesise it.
            undefined_symbol,
            nodes,
            symbols,
            node_symbols,
            node_base,
            locals,
            diagnostics,
            container: NodeId::ZERO,
            expando_container: NodeId::ZERO,
            block: NodeId::ZERO,
            owner: None,
            source: "",
            in_declaration_file: false,
            in_js_file: false,
            file_node: NodeId::ZERO,
            file_symbol_name: "",
            module_symbol: None,
            commonjs_module: false,
            jsdoc_type_hosts: rustc_hash::FxHashSet::default(),
            this_container: NodeId::ZERO,
            computed_names,
            name_nodes: rustc_hash::FxHashMap::default(),
            expando_assignments: Vec::new(),
            expando_initializers: rustc_hash::FxHashMap::default(),
            is_module: false,
            global_exports,
            globals,
            merged,
            merge_conflicts,
            default_export_declaration: None,
            flow,
            node_flow,
            current_flow: unreachable,
            current_break_target: None,
            current_continue_target: None,
            current_return_target: None,
            current_true_target: None,
            current_false_target: None,
            current_exception_target: None,
            pre_switch_case_flow: None,
            active_labels: Vec::new(),
            labels_base: 0,
            has_explicit_return: false,
            has_flow_effects: false,
            export_context: false,
            in_ambient_module: false,
            global_augmentations: Vec::new(),
            module_augmentations,
            pattern_ambient_module_augmentations,
            in_assignment_pattern: false,
            seen_this_keyword: false,
            facts,
            end_flow,
            return_flow,
            fallthrough_flow,
            ancestors: Vec::new(),
            children: Vec::new(),
        }
    }

    pub(crate) fn bind_source_file(
        self,
        file: &'a SourceFile<'a>,
        info: FileInfo<'a>,
    ) -> BindResult<'a> {
        self.bind_source_file_with_jsdoc(file, info, &[])
    }

    pub(crate) fn bind_source_file_with_jsdoc(
        self,
        file: &'a SourceFile<'a>,
        info: FileInfo<'a>,
        jsdoc: &[(NodeId, &'a [&'a tsr_ast::JSDoc<'a>])],
    ) -> BindResult<'a> {
        self.bind_file_body(file, info, jsdoc)
            .finish_file(file.node_id.expect("registered source file"))
    }

    fn bind_file_body(
        mut self,
        file: &'a SourceFile<'a>,
        info: FileInfo<'a>,
        jsdoc: &[(NodeId, &'a [&'a tsr_ast::JSDoc<'a>])],
    ) -> Self {
        let file_name = info.name;
        self.source = info.text;
        let root = Node::SourceFile(file);
        let root_id = root.node_id().expect("the source file is registered");

        self.jsdoc_type_hosts.extend(jsdoc.iter().filter_map(|(host, docs)| {
            docs.iter()
                .any(|doc| {
                    doc.tags.iter().any(|tag| {
                        matches!(
                            tag,
                            tsr_ast::JSDocTag::JSDocTypeTag(tag) if tag.type_expression.is_some()
                        )
                    })
                })
                .then_some(*host)
        }));
        self.container = root_id;
        self.block = root_id;

        // Upstream's `bindSourceFileIfExternalModule`. A file's own symbol exists
        // only for a *module*; a script's top-level declarations are globals and
        // belong in the file's locals, with nothing to export them from.
        self.in_declaration_file = is_declaration_file(file_name);
        self.export_context = self.in_declaration_file && !file_has_export_declarations(file);
        self.in_js_file = is_javascript_file(file_name);
        self.is_module = is_external_module(file);
        self.file_node = root_id;
        self.file_symbol_name = remove_file_extension(file_name);
        if self.is_module {
            // `bindSourceFileAsExternalModule` names the symbol after the path
            // with its extension removed. Upstream wraps that in quotes, the way
            // `declare module "fs"` is spelled; we store the value rather than a
            // spelling, exactly as [`module_name`] does for an ambient module,
            // because quoting would need an owned string where every symbol name
            // here borrows from the source or the file name.
            let symbol = self.bind_source_file_as_external_module(root_id);
            self.owner = Some(symbol);
        } else if self.nodes.flags(root_id).contains(NodeFlags::JSON_FILE) {
            // bindSourceFileIfExternalModule (binder.go:758): a JSON value is
            // the module's export= property, equivalent to module.exports.
            let module = self.bind_source_file_as_external_module(root_id);
            self.owner = Some(module);
            self.declare_into(
                Destination::Exports,
                root_id,
                Some(module),
                INTERNAL_EXPORT_EQUALS,
                SymbolFlags::PROPERTY,
                root_id,
            );
            // declareSymbol sets the declaration's symbol to the property;
            // upstream restores the source file's original module symbol.
            self.node_symbols[root_id.index() - self.node_base] = Some(module);
        }

        self.bind(root);
        // Expando assignments are bound last, because `f.x = 1` may extend an
        // `f` declared further down the file (`bindDeferredExpandoAssignments`).
        self.bind_deferred_expando_assignments();

        // `module` and `exports` are locals of a CommonJS file, and only of one
        // that actually uses them — which is not known until the whole file has
        // been walked, so upstream declares them here rather than up front.
        if self.commonjs_module {
            self.declare_commonjs_variable("module", root_id);
            self.declare_commonjs_variable("exports", root_id);
        }

        // In a JavaScript file, JSDoc tags DECLARE: a `@typedef`/`@callback`
        // is a type alias, `@template` declares type parameters, and an
        // `@overload` block is another declaration of the function it
        // documents. Upstream reparses these into the tree and binds them like
        // written syntax (`parser.reparseTags`); here the parser kept them in
        // a side table, so the binder files them from it after the main walk.
        if self.in_js_file {
            self.bind_jsdoc_declarations(root_id, root, jsdoc);
        }

        self
    }

    fn finish_file(mut self, root_id: NodeId) -> BindResult<'a> {
        self.merge_globals(root_id);
        self.declare_synthesised_globals();
        self.into_result()
    }

    /// The fields that survive into the next file, or into the checker.
    fn into_result(self) -> BindResult<'a> {
        BindResult {
            max_depth: self.max_depth,
            computed_names: self.computed_names,
            global_exports: self.global_exports,
            globals: self.globals,
            merged: self.merged,
            merge_conflicts: self.merge_conflicts,
            module_augmentations: self.module_augmentations,
            pattern_ambient_module_augmentations: self.pattern_ambient_module_augmentations,
            undefined_symbol: self.undefined_symbol,
            symbols: self.symbols,
            node_symbols: self.node_symbols,
            locals: self.locals,
            diagnostics: self.diagnostics,
            flow: self.flow,
            node_flow: self.node_flow,
            facts: self.facts,
            end_flow: self.end_flow,
            return_flow: self.return_flow,
            fallthrough_flow: self.fallthrough_flow,
        }
    }

    /// Merge this file's top-level names into the program's global scope.
    ///
    /// Ported from the first loop of `initializeChecker`
    /// (`internal/checker/checker.go:1296`): for each file that is **not** an
    /// external module, every one of its `Locals` becomes a global; and a file's
    /// `GlobalExports` — the names a UMD module claims with
    /// `export as namespace N` — are merged with first-in-wins semantics.
    ///
    /// # Why this is in the binder and not the checker
    ///
    /// Upstream runs it in the checker because that is where `c.files` first
    /// exists. Here the accumulating `BindResult` already *is* the program's
    /// symbol state, and it is the only object that has every file's locals in
    /// one place; putting the merge in the checker would mean handing the
    /// checker every file's `BindResult` in order to build a table the binder
    /// could have built as it went. The rule applied is upstream's, in
    /// upstream's order — file order, which decides which declaration of a
    /// repeated name is kept. Only the location differs.
    ///
    /// # What is not ported, and what it costs
    ///
    /// **Declaration merging.** Upstream calls `mergeGlobalSymbol`
    /// (`checker.go:1386`), which merges a second declaration of a name *into*
    /// the first — two `interface Array` declarations become one symbol with the
    /// union of their members. [`Binder::merge_symbol`] now does the same.
    ///
    /// This read "the first declaration wins outright and the second is
    /// dropped" until merging landed, and the consequence it described was
    /// real: `interface Array<T>` is declared in 8 bundled lib files and
    /// `String` in 11, so `Math.trunc` — declared only in
    /// `lib.es2015.core.d.ts` — answered `error` while `Math.random` from
    /// `lib.es5.d.ts` answered correctly. Measured before building: of 4,169
    /// corpus assertion lines that are a member access on a named lib global,
    /// 1,250 name a member that exists only in a non-base declaration.
    /// The globals upstream synthesises rather than reads from a file.
    ///
    /// Ported from `Checker.initializeChecker` (`checker.go:955`), the line
    /// that creates the synthesised `undefined` symbol. Upstream creates it in
    /// the **checker** and
    /// puts it in `c.globals`; this port keeps `globals` in the binder
    /// ([`Binder::merge_globals`]), so the symbol is created here and its
    /// *type* is seeded on the checker side, which is where upstream sets it
    /// too (`valueSymbolLinks.resolvedType`, `checker.go:1345`).
    ///
    /// **`undefined` is not in any `lib.*.d.ts`.** It is the one global with no
    /// declaration anywhere, which is why merging the lib files — the mechanism
    /// that makes `Array` and `String` resolve — never produced it and every
    /// `undefined` reference answered `errorType`.
    ///
    /// Inserted **after** `merge_globals` and only when the name is absent, so a
    /// file that declares its own `undefined` keeps its declaration. Upstream
    /// has the same order: the synthesised symbols are installed before the
    /// program's files are merged, and `mergeGlobalSymbol` merges into whatever
    /// is already there rather than replacing it.
    ///
    /// `arguments` is the next line of upstream's initialiser
    /// (`checker.go:956`) and is deliberately **not** here: its type is
    /// `IArguments` from `lib.d.ts`, which needs lib loading (`bd tsr-9or.1`),
    /// and a symbol whose type cannot be answered is worse than no symbol —
    /// it converts a resolution gap into a `getTypeOfSymbol` gap without
    /// answering a single line.
    fn declare_synthesised_globals(&mut self) {
        if !self.globals.contains_key("undefined") {
            // `SymbolFlagsProperty`, upstream's flag for it, which is what sends
            // it to the variable/parameter/property arm in the checker.
            let symbol = self.symbols.create("undefined", SymbolFlags::PROPERTY);
            self.globals.insert("undefined", symbol);
            self.undefined_symbol = Some(symbol);
        }
    }

    fn merge_globals(&mut self, root_id: NodeId) {
        // `ast.IsExternalOrCommonJSModule`. A module's top-level names are its
        // exports, not globals — which is the entire distinction between a
        // script and a module, and the reason `lib.*.d.ts` are scripts.
        if !self.is_module
            && !self.commonjs_module
            && let Some(locals) = self.locals.get(&root_id)
        {
            let locals: Vec<(&'a str, SymbolId)> =
                locals.iter().map(|(name, symbol)| (*name, *symbol)).collect();
            for (name, symbol) in locals {
                // `mergeGlobalSymbol` (`checker.go:1386`): merge into the
                // existing global if there is one, otherwise take this symbol.
                match self.globals.get(name) {
                    Some(&target) => self.merge_symbol(target, symbol, 0),
                    None => {
                        self.globals.insert(name, symbol);
                    }
                }
            }
        }
        // UMD global exports are merged for *every* file, module or not, and
        // upstream states the first-in-wins rule outright ("see #9771").
        for (name, symbol) in &self.global_exports {
            self.globals.entry(name).or_insert(*symbol);
        }

        // `declare global { … }`, which is the *only* way a file that is a
        // module contributes a name to the global scope other than
        // `export as namespace`. The block's own symbol is a local of the file
        // called `global`; what becomes global is its **exports**, which
        // `mergeModuleAugmentation` unions into `c.globals` with
        // `mergeSymbolTable` (`internal/checker/checker.go:1406-1407`).
        //
        // Its *locals* are deliberately not merged. In the shapes that reach
        // here the block is always an export context, so every declaration in
        // it produces both halves of `declareModuleMember`'s pair and the
        // export half is the one carrying the real flags — merging the locals
        // as well would put the `ExportValue` marker symbols into `globals`
        // alongside them.
        //
        // **Ordering.** Upstream runs this over every file at once, after the
        // script-locals pass has run over every file
        // (`initializeChecker`'s two loops, `checker.go:1300` and `:1335`).
        // Here `merge_globals` is per file, so an augmentation in file 1 is
        // merged before file 2's script locals rather than after. That is the
        // same interleaving the script-locals and UMD passes above already
        // have, and it only decides which of two *conflicting* declarations of
        // one global name is the merge target — a case upstream leaves to
        // `mergeSymbol`, which unions rather than picks.
        for symbol in std::mem::take(&mut self.global_augmentations) {
            let exports: Vec<(&'a str, SymbolId)> =
                self.symbols.get(symbol).exports.iter().map(|(n, s)| (*n, *s)).collect();
            for (name, source) in exports {
                self.merge_into_globals(name, source);
            }
        }
    }

    /// One name from a global augmentation, into `globals`.
    ///
    /// This is `mergeSymbolTable`'s body for one entry
    /// (`internal/checker/checker.go:14109-14126`), plus the one case where
    /// upstream's data model does the work and this port's has to say it.
    ///
    /// # `globalThis` is not a name in the table, it *is* the table
    ///
    /// Upstream creates a `globalThis` symbol during `initializeChecker` and
    /// aliases its export table to the global one:
    ///
    /// ```go
    /// c.globalThisSymbol = c.newSymbolEx(ast.SymbolFlagsModule, "globalThis", ast.CheckFlagsReadonly)
    /// c.globalThisSymbol.Exports = c.globals          // checker.go:963 — the same map
    /// c.globals[c.globalThisSymbol.Name] = c.globalThisSymbol
    /// ```
    ///
    /// So when a file writes `declare global { namespace globalThis { var t: string } }`
    /// — which is the supported way to add a name to the global object —
    /// `mergeSymbolTable` finds `globalThis` already in `c.globals`, calls
    /// `mergeSymbol` on it, and the union of the namespace's exports lands in
    /// `globalThisSymbol.Exports`, **which is `c.globals` itself**. The
    /// namespace's members become globals. `mergeSymbol` even carries an
    /// explicit `if target != c.globalThisSymbol` guard (`checker.go:14192`) to
    /// keep that one symbol from being given a parent.
    ///
    /// This port has no `globalThis` symbol — §33 of
    /// `docs/architecture/checker-notes-narrow.md` mints `typeof globalThis` as
    /// a type when the *name* fails to resolve, and member access on it reads
    /// `globals` directly (`tsr_checker::members`). There is therefore no entry
    /// to merge into, and the plain `mergeSymbolTable` arm would instead insert
    /// the namespace under the name `globalThis` — which is worse than doing
    /// nothing twice over: `globalThis` starts resolving as an identifier, so
    /// §33's mint stops firing, and the names inside the namespace stay
    /// invisible because nothing reads that symbol's exports.
    ///
    /// Measured: doing exactly that lost `compiler/extendGlobalThis`, whose
    /// `.types` baseline wants `globalThis.test : string` (the augmented name,
    /// a global) and `globalThis.tests : any` (a missing one, no error).
    /// Splicing the exports in, as below, produces both.
    fn merge_into_globals(&mut self, name: &'a str, source: SymbolId) {
        if name == GLOBAL_THIS {
            let exports: Vec<(&'a str, SymbolId)> =
                self.symbols.get(source).exports.iter().map(|(n, s)| (*n, *s)).collect();
            for (name, source) in exports {
                self.merge_into_globals(name, source);
            }
            return;
        }
        match self.globals.get(name) {
            Some(&target) => self.merge_symbol(target, source, 0),
            None => {
                self.globals.insert(name, source);
            }
        }
    }

    /// Merge `source`'s declarations into `target`, in place.
    ///
    /// Ported from `Checker.mergeSymbol` (`checker.go:14146`) and
    /// `mergeSymbolTable` (`:14109`), **reduced to what a single-program port
    /// needs**.
    ///
    /// # Why almost none of upstream's machinery is here
    ///
    /// Upstream clones the target (`cloneSymbol`), records the result in a
    /// merged-symbol table, and then reads every symbol back through
    /// `getMergedSymbol` — which appears 37 times in `checker.go` alone. That
    /// indirection exists because a `*ast.Symbol` can be shared between
    /// programs, so merging must not mutate what another program sees.
    ///
    /// Here a [`BindResult`] *is* one program: [`bind_into`] resumes this binder
    /// over the previous result, so every symbol in play belongs to one arena
    /// and nothing outside it holds a view. Merging in place is therefore sound,
    /// and it removes the clone, the record, and all 37 read sites at once. This
    /// is the port's shape rather than upstream's because the constraint that
    /// produced upstream's shape does not exist here — if symbols ever become
    /// shared across programs, this is the decision that has to be revisited
    /// first.
    ///
    /// # What is deliberately not merged
    ///
    /// - **An alias on either side.** Upstream resolves it (`resolveSymbol`)
    ///   before deciding, and nothing here follows aliases yet
    ///   (`bd tsr-y4u.12`). First-in-wins is kept, so an alias merge is a gap
    ///   rather than a wrong table.
    /// - **A conflicting redeclaration.** [`SymbolFlags::excludes`] is
    ///   upstream's `getExcludedSymbolFlags` and gates the same branch
    ///   (`checker.go:14147`). Upstream reports a diagnostic and does not merge;
    ///   this does not merge, and has no diagnostics to report
    ///   (`bd tsr-5e7.6`).
    /// - **Module augmentation** (`mergeModuleAugmentation`) is not merged
    ///   *here*: it needs module resolution, so it runs once the program is
    ///   bound, through [`crate::BindResult::merge_module_augmentations`],
    ///   which calls back into this function
    ///   (`docs/parity/notes/names-modules.md` §4).
    ///
    /// # Members and exports recurse
    ///
    /// A name in both tables is merged rather than taken first-in-wins, because
    /// the two declarations of a lib interface routinely split a member's
    /// *overloads* between them — `Array.from` has one in
    /// `lib.es2015.core.d.ts` and another in `lib.es2015.iterable.d.ts`. Taking
    /// one would print a single signature where upstream prints the overload
    /// set: a wrong answer, not a missing one, which is the failure this whole
    /// change exists to remove.
    ///
    /// `depth` bounds that recursion. The tables are trees in every tree the
    /// parser produces, so the cap is a guard and not a limit; it is here
    /// because a cycle would otherwise be a hang rather than a wrong answer.
    fn merge_symbol(&mut self, target: SymbolId, source: SymbolId, depth: u32) {
        const MAX_MERGE_DEPTH: u32 = 32;
        if target == source || depth > MAX_MERGE_DEPTH {
            return;
        }
        let (source_flags, target_flags) =
            (self.symbols.get(source).flags, self.symbols.get(target).flags);
        if (source_flags | target_flags).intersects(SymbolFlags::ALIAS) {
            // Upstream does **not** stop here: `mergeSymbol` calls
            // `resolveSymbol` on a non-transient target and re-tests the
            // excludes against what the alias resolves to
            // (`checker.go:14153-14164`), so it can reach either the merge or
            // the error. Nothing here follows aliases (`bd tsr-y4u.12`), so
            // this stays a decline — and, deliberately, an *unreported* one:
            // a diagnostic issued at a position upstream may never reach is a
            // false positive, which §159 records as this build's first
            // falsifier.
            return;
        }
        // `(source.Flags|target.Flags)&ast.SymbolFlagsAssignment != 0`
        // (`checker.go:14147`) — the *second* disjunct of the merge condition,
        // and it bypasses the excludes masks entirely. A JavaScript
        // static-property assignment (`ExpandoMerge.p1 = 111`) is a declaration
        // and a value at once, so it collides with anything it merges into;
        // upstream lets it through by name. Twenty-two of §159's first
        // measurement's thirty-three new wrong lines were this one disjunct,
        // all in `typeFromPropertyAssignment32` and `33`.
        if (source_flags | target_flags).intersects(SymbolFlags::ASSIGNMENT) {
            // Fall through to the union below, which is what upstream does.
        } else if source_flags.excludes().intersects(target_flags) {
            // `checker.go:14199`'s `else` arm — `reportMergeSymbolError`. The
            // report itself is the checker's, because only the checker knows
            // which *file* each declaration is in and only its diagnostics are
            // collected across files (§159).
            self.merge_conflicts.push((target, source));
            return;
        }
        // `recordMergedSymbol(target, source)` (`internal/checker/checker.go:14372`),
        // which upstream calls from `mergeSymbol`'s union branch only
        // (`checker.go:14185`): after the union, `source` is a symbol nothing
        // should ever be answered from again, and every read of it has to be
        // redirected. A declined merge (the alias decline and the excludes
        // conflict above) records nothing, so each side keeps answering for
        // its own declarations, as upstream's `return source` /
        // `reportMergeSymbolError` arms leave them. Recorded before the
        // recursion, which upstream does after it; the map is order-blind.
        self.merged.insert(source, target);

        // Read everything needed from `source` before touching `target`: the two
        // are entries in one store, so the borrows cannot overlap.
        let declarations = self.symbols.get(source).declarations.clone();
        let value_declaration = self.symbols.get(source).value_declaration;
        let members_present = self.symbols.get(source).members.is_present();
        let exports_present = self.symbols.get(source).exports.is_present();
        let members: Vec<(&'a str, SymbolId)> =
            self.symbols.get(source).members.iter().map(|(n, s)| (*n, *s)).collect();
        let exports: Vec<(&'a str, SymbolId)> =
            self.symbols.get(source).exports.iter().map(|(n, s)| (*n, *s)).collect();

        // `binder.SetValueDeclaration(target, source.ValueDeclaration)`
        // (`binder/binder.go:2531`): a non-assignment declaration displaces an
        // assignment one (`ExpandoMerge.p8 = false` yields to a namespace's
        // `export var p8 = 6`), and a non-namespace declaration displaces a
        // namespace; otherwise the target keeps its first one.
        let replace_value_declaration = value_declaration.is_some_and(|node| {
            match self.symbols.get(target).value_declaration {
                None => true,
                Some(current) => {
                    let (current_kind, node_kind) =
                        (self.nodes.kind(current), self.nodes.kind(node));
                    (is_assignment_declaration_kind(current_kind)
                        && !is_assignment_declaration_kind(node_kind))
                        || (current_kind != node_kind
                            && matches!(
                                current_kind,
                                SyntaxKind::ModuleDeclaration | SyntaxKind::Identifier
                            ))
                }
            }
        });
        {
            let entry = self.symbols.get_mut(target);
            entry.flags |= source_flags;
            if members_present {
                entry.members.initialize();
            }
            if exports_present {
                entry.exports.initialize();
            }
            entry.declarations.extend(declarations);
            if replace_value_declaration {
                entry.value_declaration = value_declaration;
            }
        }

        for (name, member) in members {
            match self.symbols.get(target).members.get(name) {
                Some(&existing) => self.merge_symbol(existing, member, depth + 1),
                None => {
                    self.symbols.get_mut(target).members.insert(name, member);
                }
            }
        }
        for (name, export) in exports {
            match self.symbols.get(target).exports.get(name) {
                Some(&existing) => self.merge_symbol(existing, export, depth + 1),
                None => {
                    self.symbols.get_mut(target).exports.insert(name, export);
                }
            }
        }
    }

    /// Run `merges` — `(target, source)` pairs, in order — through
    /// [`Binder::merge_symbol`], and hand the result back.
    ///
    /// The executing half of [`crate::BindResult::merge_module_augmentations`]:
    /// that function decides each pair with the program's module resolution,
    /// and this applies `mergeSymbol`'s in-place union to it.
    pub(crate) fn merge_pairs(mut self, merges: &[(SymbolId, SymbolId)]) -> BindResult<'a> {
        for &(target, source) in merges {
            self.merge_symbol(target, source, 0);
        }
        self.into_result()
    }

    /// Bind `node` and everything under it.
    ///
    /// Guarded: see [`MAX_DEPTH`]. The parser caps its *own* recursion but builds
    /// deeper trees than that iteratively, so this walk cannot assume its input
    /// is shallow just because the parser survived it.
    fn bind(&mut self, node: Node<'a>) {
        self.depth += 1;
        self.max_depth = self.max_depth.max(self.depth);
        // Check every 32nd level rather than every level. `bind()` is the hottest
        // path in the binder — checking on all of them measured ~2% on parse+bind
        // over `checker.ts` — and 32 levels is at most ~26 KiB of frames in debug,
        // comfortably inside `ensure_sufficient`'s 100 KiB red zone.
        if self.depth.is_multiple_of(STACK_CHECK_INTERVAL) {
            tsr_core::stack::ensure_sufficient(|| self.bind_inner(node));
        } else {
            self.bind_inner(node);
        }
        self.depth -= 1;
    }

    fn bind_inner(&mut self, node: Node<'a>) {
        let Some(id) = node.node_id() else {
            // Unregistered: descend without treating it as a declaration.
            self.bind_each_child(node);
            return;
        };

        // Every declare path needs the expando-initializer record, and an
        // exported variable's does not pass through the one declare site that
        // used to make it (`propertyAssignmentUseParentType1`'s
        // `export const ignoreJsdoc = () => …`); the walk sees every
        // declaration exactly once, so the record lives here.
        if matches!(node, Node::VariableDeclaration(_)) {
            self.record_expando_initializer(node, id);
        }

        self.record_flow(node, id);
        // Declare first, then descend: a class's own name is visible inside its
        // body, and its members go in the symbol this call creates.
        let declared = self.declare(node, id);

        // A token has no children and introduces no scope, so upstream stops
        // here rather than paying for `GetContainerFlags` and a child walk on
        // every identifier in the file.
        let kind = self.nodes.kind(id);
        if (kind as u16) <= (SyntaxKind::LAST_TOKEN as u16) {
            return;
        }

        let flags = container_flags(node, self.nodes);
        self.ancestors.push((id, node));
        if flags.is_empty() {
            self.bind_children(node, id);
        } else {
            self.bind_container(node, id, flags, declared);
        }
        self.ancestors.pop();
    }

    fn bind_optional(&mut self, node: Option<Node<'a>>) {
        if let Some(node) = node {
            self.bind(node);
        }
    }

    fn bind_expression(&mut self, expression: Option<Expression<'a>>) {
        self.bind_optional(expression.map(Node::from));
    }

    fn bind_statement(&mut self, statement: Option<Statement<'a>>) {
        self.bind_optional(statement.map(Node::from));
    }

    /// Every child, in syntactic order (`bindEachChild`).
    fn bind_each_child(&mut self, node: Node<'a>) {
        let base = self.children.len();
        push_children(node, &mut self.children);
        let end = self.children.len();
        for index in base..end {
            let child = self.children[index];
            self.bind(child);
        }
        self.children.truncate(base);
    }

    /// Function declarations first, then everything else
    /// (`bindEachStatementFunctionsFirst`).
    ///
    /// This is hoisting made visible: a call earlier in the block than the
    /// declaration it names has to see a symbol, and the only way a
    /// single-pass binder gets that is to declare the functions before it walks
    /// the statements that use them.
    fn bind_statements_functions_first(&mut self, statements: &'a [Statement<'a>]) {
        for statement in statements {
            if matches!(statement, Statement::FunctionDeclaration(_)) {
                self.bind(Node::from(*statement));
            }
        }
        for statement in statements {
            if !matches!(statement, Statement::FunctionDeclaration(_)) {
                self.bind(Node::from(*statement));
            }
        }
    }

    // ---------------------------------------------------------------- scope --

    /// Enter a scope, a flow graph, or both, and leave it again.
    ///
    /// A direct port of `bindContainer`.
    fn bind_container(
        &mut self,
        node: Node<'a>,
        id: NodeId,
        flags: ContainerFlags,
        declared: Option<SymbolId>,
    ) {
        if let Some(symbol) = declared {
            match node {
                Node::ClassDeclaration(_) | Node::ClassExpression(_) => {
                    self.symbols.get_mut(symbol).exports.initialize();
                }
                Node::FunctionTypeNode(_) | Node::ConstructorTypeNode(_) => {
                    self.symbols.get_mut(symbol).members.initialize();
                }
                _ => {}
            }
        }
        let saved_container = self.container;
        let saved_expando_container = self.expando_container;
        let saved_block = self.block;
        let saved_owner = self.owner;
        let saved_this_container = self.this_container;

        if flags.contains(ContainerFlags::IS_CONTAINER) {
            self.expando_container = id;
            self.block = id;
            if flags.contains(ContainerFlags::HAS_LOCALS) {
                self.container = id;
                self.locals.entry(id).or_default();
            }
            // A container that owns members or exports becomes the symbol its
            // children declare into. A function-like container does not: its
            // parameters and locals are lexical, not members.
            if !flags.contains(ContainerFlags::IS_FUNCTION_LIKE)
                && let Some(symbol) = declared
            {
                self.owner = Some(symbol);
            }
        } else if flags.contains(ContainerFlags::IS_BLOCK_SCOPED_CONTAINER) {
            self.block = id;
            if flags.contains(ContainerFlags::HAS_LOCALS) {
                self.locals.entry(id).or_default();
            }
        }
        // A separate cursor from `container`, and deliberately so: an arrow
        // function is a container and is *not* a `this` container, which is what
        // makes `this` inside one belong to the enclosing method.
        if flags.contains(ContainerFlags::IS_THIS_CONTAINER) {
            self.this_container = id;
        }

        let saved_export_context = self.export_context;
        let saved_in_ambient = self.in_ambient_module;
        if let Node::ModuleDeclaration(module) = node {
            // Recorded *before* `in_ambient_module` is overwritten below: the
            // gate reads the ambience of the scope this block sits in, not the
            // one it creates.
            if let Some(symbol) = declared
                && self.is_merged_global_augmentation(module)
                && !self.global_augmentations.contains(&symbol)
            {
                self.global_augmentations.push(symbol);
            }
            if let Some(symbol) = declared
                && let Some(name) = self.external_module_augmentation_name(module)
                && !self.module_augmentations.iter().any(|recorded| recorded.symbol == symbol)
            {
                // `moduleAugmentation.Symbol.Declarations[0] != moduleNode`
                // (`checker.go:1399`): a second `declare module "x"` in the same
                // file shares the first one's symbol, which already carries
                // both, so only the first is recorded.
                self.module_augmentations.push(crate::ModuleAugmentation {
                    symbol,
                    file: self.file_node,
                    name: name.node_id.expect("a module name is registered"),
                    text: name.text,
                    nested: matches!(
                        self.ancestors.iter().rev().nth(1),
                        Some((_, Node::ModuleBlock(_)))
                    ),
                });
            }
            // An ambient module exports everything it declares — unless it uses
            // `export` explicitly somewhere, in which case only what it names.
            // **A declaration file is ambient throughout.** Upstream's parser
            // sets `NodeFlagsAmbient` on every node of a `.d.ts`
            // (`parser.go`'s `contextFlags`), so a namespace there exports what
            // it declares whether or not `export` is written —
            // `export namespace dom { namespace JSX { … } }` in `renderer.d.ts`
            // puts `JSX` in `dom`'s **exports** upstream and in its locals here.
            //
            // This parser does not set the flag (§99 and §132 substituted it
            // structurally at two other sites); `in_declaration_file` is the
            // substitute and was missing from this disjunction. §208.
            let ambient = self.in_ambient_module
                || self.in_declaration_file
                || has_declare(module.modifiers)
                || matches!(module.name, Some(tsr_ast::ModuleName::StringLiteral(_)));
            self.in_ambient_module = ambient;
            self.export_context = ambient && !has_export_declarations(module);
        }

        if flags.contains(ContainerFlags::IS_CONTROL_FLOW_CONTAINER) {
            self.bind_flow_container(node, id, flags);
        } else if flags.contains(ContainerFlags::IS_INTERFACE) {
            let saved_seen_this = self.seen_this_keyword;
            self.seen_this_keyword = false;
            self.bind_children(node, id);
            self.set_fact(id, NodeFacts::CONTAINS_THIS, self.seen_this_keyword);
            self.seen_this_keyword = saved_seen_this;
        } else {
            self.bind_children(node, id);
        }

        self.container = saved_container;
        self.expando_container = saved_expando_container;
        self.block = saved_block;
        self.owner = saved_owner;
        self.this_container = saved_this_container;
        self.export_context = saved_export_context;
        self.in_ambient_module = saved_in_ambient;
    }

    /// Whether this `global { … }` block's exports become globals.
    ///
    /// `ast.IsGlobalScopeAugmentation` (`internal/ast/utilities.go:1690`) is one
    /// line — a `ModuleDeclaration` whose keyword is `global` — and it is *not*
    /// on its own the question. Only a global augmentation that upstream's
    /// parser collected into `SourceFile.ModuleAugmentations` is ever merged
    /// (`initializeChecker`, `internal/checker/checker.go:1335`), and
    /// `collectModuleReferences` (`internal/parser/references.go:47-69`)
    /// collects two shapes and no others:
    ///
    /// - a **top-level** block in a file that is an external module —
    ///   `export {}; declare global { … }`;
    /// - a block **directly inside an ambient module declaration** that is
    ///   itself top-level in a file that is *not* an external module —
    ///   `declare module "m" { global { … } }` in a script `.d.ts`.
    ///
    /// Everything else is a *diagnostic*, not a merge: `checkModuleDeclaration`
    /// reports TS2669 ("Augmentations for the global scope can only be directly
    /// nested in external modules or ambient module declarations") for a global
    /// block anywhere else (`checker.go:5203`, `:5209`). Verified against `tsc`
    /// 5.x on the two-form fixture in `tests/bind.rs`: putting `export {}` in
    /// the file that spells the second form turns its `global` block from a
    /// merge into a TS2669 and the name it declares stops resolving.
    ///
    /// The two shapes are exactly `ast.IsModuleAugmentationExternal`
    /// (`utilities.go:1694`), which upstream's *binder* already consults for
    /// the string-named case, so this reads that predicate rather than
    /// re-deriving the collector's recursion. They differ on one input:
    /// `collectModuleReferences` also requires the enclosing context to be
    /// ambient (`inAmbientModule || declare modifier || .d.ts`), which
    /// `IsModuleAugmentationExternal` does not test. That gate is kept below,
    /// because without it `global { … }` written in a `.ts` module with no
    /// `declare` — which upstream reports as TS2669 and does not merge — would
    /// merge here.
    ///
    /// The tree has no back-edges, so parent and grandparent are read from the
    /// ancestor chain, whose last entry is the module declaration itself.
    fn is_merged_global_augmentation(&self, module: &'a tsr_ast::ModuleDeclaration<'a>) -> bool {
        // `ast.IsGlobalScopeAugmentation`. The parser gives the block a synthetic
        // `global` *identifier* for a name, so the name cannot tell a global
        // augmentation from `namespace global { … }` — the keyword can.
        if module.keyword.kind != SyntaxKind::GlobalKeyword {
            return false;
        }
        // `collectModuleReferences`' first gate.
        if !(self.in_ambient_module || has_declare(module.modifiers) || self.in_declaration_file) {
            return false;
        }
        self.is_module_augmentation_external(
            self.ancestors.iter().rev().skip(1).map(|(_, node)| *node),
        )
    }

    /// The string-literal name of a **non-global** module augmentation that
    /// upstream's parser collects into `SourceFile.ModuleAugmentations`, or
    /// `None` for every other module declaration.
    ///
    /// `collectModuleReferences` (`internal/parser/references.go:47-69`)
    /// collects a string-named module declaration in an ambient context when
    /// the file is an external module, or when it is nested in a top-level
    /// ambient module under a non-relative name. Those two placements are
    /// [`Binder::is_module_augmentation_external`]; the ambient-context gate
    /// is the same one [`Binder::is_merged_global_augmentation`] keeps.
    ///
    /// The relative-name exclusion of the nested form is left to the merge:
    /// the binder has no `tspath`, so the record carries
    /// [`crate::ModuleAugmentation::nested`] and the resolver callback
    /// (`tsr_compiler`) declines a nested relative name with the real
    /// `IsExternalModuleNameRelative`.
    fn external_module_augmentation_name(
        &self,
        module: &'a tsr_ast::ModuleDeclaration<'a>,
    ) -> Option<&'a tsr_ast::StringLiteral<'a>> {
        let Some(tsr_ast::ModuleName::StringLiteral(name)) = module.name else { return None };
        if !(self.in_ambient_module || has_declare(module.modifiers) || self.in_declaration_file) {
            return None;
        }
        self.is_module_augmentation_external(
            self.ancestors.iter().rev().skip(1).map(|(_, node)| *node),
        )
        .then_some(name)
    }

    /// `ast.IsModuleAugmentationExternal` (`utilities.go:1694`), read from a
    /// parent chain (nearest first) because the tree has no back-edges. An
    /// external augmentation is an ambient module declared at the top level of
    /// an external module, or inside a top-level ambient module of a script.
    fn is_module_augmentation_external(&self, mut parents: impl Iterator<Item = Node<'a>>) -> bool {
        match parents.next() {
            Some(Node::SourceFile(_)) => self.is_module,
            Some(Node::ModuleBlock(_)) => {
                matches!(
                    (parents.next(), parents.next()),
                    (Some(Node::ModuleDeclaration(outer)), Some(Node::SourceFile(_)))
                        if is_ambient_module(outer)
                ) && !self.is_module
            }
            _ => false,
        }
    }

    /// The control-flow half of `bindContainer`: start a fresh graph, bind, and
    /// hand the surrounding one back.
    fn bind_flow_container(&mut self, node: Node<'a>, id: NodeId, flags: ContainerFlags) {
        let saved_flow = self.current_flow;
        let saved_break = self.current_break_target;
        let saved_continue = self.current_continue_target;
        let saved_return = self.current_return_target;
        let saved_exception = self.current_exception_target;
        let saved_labels_base = self.labels_base;
        let saved_has_explicit_return = self.has_explicit_return;
        let saved_seen_this = self.seen_this_keyword;

        // A non-async, non-generator IIFE is part of the *containing* flow: the
        // code around it can see what it assigned, because it ran. That is why
        // its `return` behaves like a break to a label just past the body rather
        // than as the end of a separate graph.
        let immediately_invoked = (flags.contains(ContainerFlags::IS_FUNCTION_EXPRESSION)
            && !has_async_modifier(node)
            && !is_generator_function_expression(node)
            && self.is_immediately_invoked(id))
            || matches!(node, Node::ClassStaticBlockDeclaration(_));

        if !immediately_invoked {
            // The start node names the function only where the checker needs to
            // find its way back: a function *expression* or an object-literal
            // method can be contextually typed, and the start node is the hook.
            let start_node = flags
                .intersects(
                    ContainerFlags::IS_FUNCTION_EXPRESSION
                        | ContainerFlags::IS_OBJECT_LITERAL_OR_CLASS_EXPRESSION_METHOD_OR_ACCESSOR,
                )
                .then_some(id);
            self.current_flow = self.flow.new_node_ex(FlowFlags::START, start_node, None);
        }
        // A return graph is built for IIFEs and for constructors; the latter is
        // what strict property-initialisation checking walks.
        self.current_return_target = (immediately_invoked
            || matches!(node, Node::ConstructorDeclaration(_)))
        .then(|| self.flow.new_branch_label());
        self.current_exception_target = None;
        self.current_break_target = None;
        self.current_continue_target = None;
        self.labels_base = self.active_labels.len();
        self.has_explicit_return = false;
        self.seen_this_keyword = false;

        self.bind_children(node, id);

        if !self.flow.flags(self.current_flow).contains(FlowFlags::UNREACHABLE)
            && flags.contains(ContainerFlags::IS_FUNCTION_LIKE)
            && has_present_body(node)
        {
            self.set_fact(id, NodeFacts::HAS_IMPLICIT_RETURN, true);
            self.set_fact(id, NodeFacts::HAS_EXPLICIT_RETURN, self.has_explicit_return);
            self.end_flow.insert(id, self.current_flow);
        }
        self.set_fact(id, NodeFacts::CONTAINS_THIS, self.seen_this_keyword);

        if let Some(return_target) = self.current_return_target {
            self.flow.add_antecedent(return_target, self.current_flow);
            self.current_flow = self.flow.finish_label(return_target);
            if matches!(
                node,
                Node::ConstructorDeclaration(_) | Node::ClassStaticBlockDeclaration(_)
            ) {
                self.return_flow.insert(id, self.current_flow);
            }
        }
        if !immediately_invoked {
            self.current_flow = saved_flow;
        }
        self.current_break_target = saved_break;
        self.current_continue_target = saved_continue;
        self.current_return_target = saved_return;
        self.current_exception_target = saved_exception;
        self.active_labels.truncate(self.labels_base);
        self.labels_base = saved_labels_base;
        self.has_explicit_return = saved_has_explicit_return;
        // An arrow function and a call signature do not bind their own `this`,
        // so a `this` inside one belongs to whatever encloses it.
        self.seen_this_keyword = if flags.contains(ContainerFlags::PROPAGATES_THIS_KEYWORD) {
            saved_seen_this || self.seen_this_keyword
        } else {
            saved_seen_this
        };
    }

    // ----------------------------------------------------------------- flow --

    /// Record the flow node in effect at `node`, for the kinds that read one.
    ///
    /// The flow half of upstream's `bind` switch. The set is deliberately narrow:
    /// only a node the checker will later ask "what was known here?" about
    /// deserves an entry.
    fn record_flow(&mut self, node: Node<'a>, id: NodeId) {
        match node {
            // Identifiers and binding elements read narrowing. The closure
            // kinds record the flow AT their creation site — what the
            // checker's §13 START walk-out continues from
            // (`container.FlowNodeData().FlowNode`, upstream `flow.go:187`);
            // this runs before the container push, so it is the OUTER flow.
            Node::Identifier(_)
            | Node::MetaProperty(_)
            | Node::BindingElement(_)
            | Node::FunctionExpression(_)
            | Node::ArrowFunction(_)
            | Node::MethodDeclaration(_)
            | Node::GetAccessorDeclaration(_)
            | Node::SetAccessorDeclaration(_) => {
                self.node_flow[id.index() - self.node_base] = Some(self.current_flow);
            }
            Node::KeywordExpression(keyword) => match keyword.kind {
                SyntaxKind::ThisKeyword => {
                    self.seen_this_keyword = true;
                    self.node_flow[id.index() - self.node_base] = Some(self.current_flow);
                }
                SyntaxKind::SuperKeyword => {
                    self.node_flow[id.index() - self.node_base] = Some(self.current_flow);
                }
                _ => {}
            },
            // A qualified name only needs one inside `typeof X.Y`, where it
            // denotes a value that narrowing can have changed.
            Node::QualifiedName(_) => {
                if self.is_part_of_type_query(id) {
                    self.node_flow[id.index() - self.node_base] = Some(self.current_flow);
                }
            }
            Node::PropertyAccessExpression(_) | Node::ElementAccessExpression(_) => {
                if is_narrowable_reference(node) {
                    self.node_flow[id.index() - self.node_base] = Some(self.current_flow);
                }
            }
            Node::ThisTypeNode(_) => self.seen_this_keyword = true,
            _ => {}
        }
    }

    /// Descend into `node`'s children with the right flow shape for its kind.
    ///
    /// A direct port of `bindChildren`.
    #[allow(clippy::too_many_lines)] // One arm per upstream arm.
    fn bind_children(&mut self, node: Node<'a>, id: NodeId) {
        let saved_in_assignment_pattern = self.in_assignment_pattern;
        // Most nodes cannot appear in an assignment pattern, so the flag is
        // cleared here and set again only on the way into ones that can.
        self.in_assignment_pattern = false;

        if self.current_flow == self.flow.unreachable() {
            // Unreachable code gets no flow node — there is no "what was known
            // here", because control never arrives.
            self.node_flow[id.index() - self.node_base] = None;
            if is_potentially_executable_node(node, self.nodes) {
                self.set_fact(id, NodeFacts::UNREACHABLE, true);
            }
            self.bind_each_child(node);
            self.in_assignment_pattern = saved_in_assignment_pattern;
            return;
        }

        let kind = self.nodes.kind(id);
        if (SyntaxKind::FIRST_STATEMENT as u16) <= (kind as u16)
            && (kind as u16) <= (SyntaxKind::LAST_STATEMENT as u16)
        {
            self.node_flow[id.index() - self.node_base] = Some(self.current_flow);
        }

        match node {
            Node::WhileStatement(_) => self.bind_while_statement(node, id),
            Node::DoStatement(_) => self.bind_do_statement(node, id),
            Node::ForStatement(_) => self.bind_for_statement(node, id),
            Node::ForInOrOfStatement(_) => self.bind_for_in_or_of_statement(node, id, kind),
            Node::IfStatement(_) => self.bind_if_statement(node),
            Node::ReturnStatement(_) => self.bind_return_statement(node),
            Node::ThrowStatement(_) => self.bind_throw_statement(node),
            Node::BreakStatement(statement) => {
                let label = statement.label.map(Node::Identifier);
                let target = self.current_break_target;
                self.bind_break_or_continue_statement(label, target, LabelTarget::Break);
            }
            Node::ContinueStatement(statement) => {
                let label = statement.label.map(Node::Identifier);
                let target = self.current_continue_target;
                self.bind_break_or_continue_statement(label, target, LabelTarget::Continue);
            }
            Node::TryStatement(_) => self.bind_try_statement(node),
            Node::SwitchStatement(_) => self.bind_switch_statement(node, id),
            Node::CaseBlock(_) => self.bind_case_block(node),
            Node::CaseOrDefaultClause(_) => self.bind_case_or_default_clause(node),
            Node::ExpressionStatement(statement) => {
                self.bind_expression(statement.expression);
                self.maybe_bind_call_flow(statement.expression.map(Node::from));
            }
            Node::LabeledStatement(_) => self.bind_labeled_statement(node),
            Node::PrefixUnaryExpression(_) => self.bind_prefix_unary_expression_flow(node),
            Node::PostfixUnaryExpression(expression) => {
                self.bind_each_child(node);
                if matches!(
                    expression.operator.kind,
                    SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken
                ) {
                    self.bind_assignment_target_flow(expression.operand.map(Node::from));
                }
            }
            Node::BinaryExpression(_) => {
                if is_destructuring_assignment(node) {
                    // Carry the flag across, because a destructuring assignment
                    // *is* the pattern its parent is asking about.
                    self.in_assignment_pattern = saved_in_assignment_pattern;
                    self.bind_destructuring_assignment_flow(node);
                    return;
                }
                self.bind_binary_expression_flow(node);
            }
            Node::DeleteExpression(expression) => {
                self.bind_each_child(node);
                if matches!(expression.expression, Some(Expression::PropertyAccessExpression(_))) {
                    self.bind_assignment_target_flow(expression.expression.map(Node::from));
                }
            }
            Node::ConditionalExpression(_) => self.bind_conditional_expression_flow(node),
            Node::VariableDeclaration(_) => self.bind_variable_declaration_flow(node, id),
            Node::CallExpression(_) => self.bind_call_expression_flow(node, id),
            Node::PropertyAccessExpression(_)
            | Node::ElementAccessExpression(_)
            | Node::NonNullExpression(_) => {
                if is_optional_chain(node, self.nodes) {
                    self.bind_optional_chain_flow(node);
                } else {
                    self.bind_each_child(node);
                }
            }
            Node::SourceFile(file) => {
                self.bind_statements_functions_first(file.statements);
                self.bind(Node::Token(file.end_of_file_token));
            }
            Node::Block(block) => self.bind_statements_functions_first(block.statements),
            Node::ModuleBlock(block) => self.bind_statements_functions_first(block.statements),
            Node::BindingElement(element) => {
                // The initializer of a binding element runs *before* the pattern
                // it initialises; binding them in source order would attribute
                // an assignment to the wrong point in the flow.
                self.bind_optional(element.dot_dot_dot_token.map(Node::Token));
                self.bind_optional(element.property_name.map(Node::from));
                self.bind_initializer(element.initializer);
                self.bind_optional(element.name.map(Node::from));
            }
            Node::ParameterDeclaration(parameter) => {
                for modifier in parameter.modifiers {
                    self.bind(Node::from(*modifier));
                }
                self.bind_optional(parameter.dot_dot_dot_token.map(Node::Token));
                self.bind_optional(parameter.question_token.map(Node::Token));
                self.bind_optional(parameter.r#type.map(Node::from));
                self.bind_initializer(parameter.initializer);
                self.bind_optional(parameter.name.map(Node::from));
            }
            Node::ObjectLiteralExpression(_)
            | Node::ArrayLiteralExpression(_)
            | Node::PropertyAssignment(_)
            | Node::SpreadElement(_) => {
                self.in_assignment_pattern = saved_in_assignment_pattern;
                self.bind_each_child(node);
            }
            _ => self.bind_each_child(node),
        }
        self.in_assignment_pattern = saved_in_assignment_pattern;
    }

    // ---- flow node construction, the parts that need binder state ----

    /// A condition node, or the antecedent unchanged when the condition tells
    /// the checker nothing (`createFlowCondition`).
    fn create_flow_condition(
        &mut self,
        flags: FlowFlags,
        antecedent: FlowId,
        expression: Option<Node<'a>>,
    ) -> FlowId {
        if self.flow.flags(antecedent).contains(FlowFlags::UNREACHABLE) {
            return antecedent;
        }
        let Some(expression) = expression else {
            // A missing condition is vacuously true: `for (;;)` never exits by
            // the condition, so the false branch is unreachable.
            return if flags.contains(FlowFlags::TRUE_CONDITION) {
                antecedent
            } else {
                self.flow.unreachable()
            };
        };

        let literal = literal_boolean(expression);
        let contradicted = (literal == Some(true) && flags.contains(FlowFlags::FALSE_CONDITION))
            || (literal == Some(false) && flags.contains(FlowFlags::TRUE_CONDITION));
        if contradicted {
            // `a?.b` and `a ?? b` put a literal where a value is meant; the
            // literal is then not the condition and does not make the branch
            // dead.
            let parent = self.parent_of(expression);
            if !is_expression_of_optional_chain_root(expression, parent, self.nodes)
                && !parent.is_some_and(is_nullish_coalesce)
            {
                return self.flow.unreachable();
            }
        }
        if !is_narrowing_expression(expression, self.nodes) {
            return antecedent;
        }
        self.flow.set_referenced(antecedent);
        self.flow.new_node_ex(flags, expression.node_id(), Some(antecedent))
    }

    /// An assignment or array-mutation node (`createFlowMutation`).
    ///
    /// Also an exception source: a mutation is the only thing that can make a
    /// `catch` block see a different type than the `try` block started with, so
    /// each one is added to the enclosing exception label.
    fn create_flow_mutation(
        &mut self,
        flags: FlowFlags,
        antecedent: FlowId,
        node: NodeId,
    ) -> FlowId {
        self.flow.set_referenced(antecedent);
        self.has_flow_effects = true;
        let result = self.flow.new_node_ex(flags, Some(node), Some(antecedent));
        if let Some(target) = self.current_exception_target {
            self.flow.add_antecedent(target, result);
        }
        result
    }

    /// A potential assertion call (`createFlowCall`).
    fn create_flow_call(&mut self, antecedent: FlowId, node: NodeId) -> FlowId {
        self.flow.set_referenced(antecedent);
        self.has_flow_effects = true;
        self.flow.new_node_ex(FlowFlags::CALL, Some(node), Some(antecedent))
    }

    /// Bind `node` as a condition, routing the two outcomes to two labels.
    fn bind_condition(
        &mut self,
        node: Option<Node<'a>>,
        true_target: FlowId,
        false_target: FlowId,
    ) {
        self.with_conditional_branches(node, true_target, false_target);
        // `a && b`, `a ||= b` and an optional chain have already added their own
        // antecedents to both targets, one per operand; adding another here
        // would narrow on the whole expression as well as on its parts.
        let handled_itself = node.is_some_and(|node| {
            is_logical_assignment_expression(node)
                || is_logical_expression(node)
                || (is_optional_chain(node, self.nodes)
                    && is_outermost_optional_chain(node, self.parent_of(node), self.nodes))
        });
        if !handled_itself {
            let current = self.current_flow;
            let when_true = self.create_flow_condition(FlowFlags::TRUE_CONDITION, current, node);
            self.flow.add_antecedent(true_target, when_true);
            let when_false = self.create_flow_condition(FlowFlags::FALSE_CONDITION, current, node);
            self.flow.add_antecedent(false_target, when_false);
        }
    }

    /// Bind `node` with the true/false targets in scope (`doWithConditionalBranches`).
    fn with_conditional_branches(
        &mut self,
        node: Option<Node<'a>>,
        true_target: FlowId,
        false_target: FlowId,
    ) {
        let saved_true = self.current_true_target;
        let saved_false = self.current_false_target;
        self.current_true_target = Some(true_target);
        self.current_false_target = Some(false_target);
        self.bind_optional(node);
        self.current_true_target = saved_true;
        self.current_false_target = saved_false;
    }

    /// Bind a loop body with its own `break` and `continue` targets.
    fn bind_iterative_statement(
        &mut self,
        statement: Option<Node<'a>>,
        break_target: FlowId,
        continue_target: FlowId,
    ) {
        let saved_break = self.current_break_target;
        let saved_continue = self.current_continue_target;
        self.current_break_target = Some(break_target);
        self.current_continue_target = Some(continue_target);
        self.bind_optional(statement);
        self.current_break_target = saved_break;
        self.current_continue_target = saved_continue;
    }

    /// Point every enclosing label's `continue` at `target` (`setContinueTarget`).
    ///
    /// `outer: for (…) { continue outer; }` continues the loop, not the label,
    /// so the label's continue target is the loop's — and a stack of labels can
    /// all name the same loop.
    fn set_continue_target(&mut self, statement: NodeId, target: FlowId) -> FlowId {
        let mut node = statement;
        let mut index = self.active_labels.len();
        while index > self.labels_base
            && self
                .nodes
                .parent(node)
                .is_some_and(|parent| self.nodes.kind(parent) == SyntaxKind::LabeledStatement)
        {
            index -= 1;
            self.active_labels[index].continue_target = Some(target);
            node = self.nodes.parent(node).expect("checked in the condition");
        }
        target
    }

    // ---- statements ----

    fn bind_while_statement(&mut self, node: Node<'a>, id: NodeId) {
        let Node::WhileStatement(statement) = node else { return };
        let loop_label = self.flow.new_loop_label();
        let pre_while = self.set_continue_target(id, loop_label);
        let pre_body = self.flow.new_branch_label();
        let post_while = self.flow.new_branch_label();
        self.flow.add_antecedent(pre_while, self.current_flow);
        self.current_flow = pre_while;
        self.bind_condition(statement.expression.map(Node::from), pre_body, post_while);
        self.current_flow = self.flow.finish_label(pre_body);
        self.bind_iterative_statement(Some(Node::from(statement.statement)), post_while, pre_while);
        self.flow.add_antecedent(pre_while, self.current_flow);
        self.current_flow = self.flow.finish_label(post_while);
    }

    fn bind_do_statement(&mut self, node: Node<'a>, id: NodeId) {
        let Node::DoStatement(statement) = node else { return };
        let pre_do = self.flow.new_loop_label();
        let branch = self.flow.new_branch_label();
        let pre_condition = self.set_continue_target(id, branch);
        let post_do = self.flow.new_branch_label();
        self.flow.add_antecedent(pre_do, self.current_flow);
        self.current_flow = pre_do;
        self.bind_iterative_statement(
            Some(Node::from(statement.statement)),
            post_do,
            pre_condition,
        );
        self.flow.add_antecedent(pre_condition, self.current_flow);
        self.current_flow = self.flow.finish_label(pre_condition);
        self.bind_condition(statement.expression.map(Node::from), pre_do, post_do);
        self.current_flow = self.flow.finish_label(post_do);
    }

    fn bind_for_statement(&mut self, node: Node<'a>, id: NodeId) {
        let Node::ForStatement(statement) = node else { return };
        let loop_label = self.flow.new_loop_label();
        let pre_loop = self.set_continue_target(id, loop_label);
        let pre_body = self.flow.new_branch_label();
        let pre_incrementor = self.flow.new_branch_label();
        let post_loop = self.flow.new_branch_label();
        self.bind_optional(statement.initializer.map(Node::from));
        self.flow.add_antecedent(pre_loop, self.current_flow);
        self.current_flow = pre_loop;
        self.bind_condition(statement.condition.map(Node::from), pre_body, post_loop);
        self.current_flow = self.flow.finish_label(pre_body);
        self.bind_iterative_statement(
            Some(Node::from(statement.statement)),
            post_loop,
            pre_incrementor,
        );
        self.flow.add_antecedent(pre_incrementor, self.current_flow);
        self.current_flow = self.flow.finish_label(pre_incrementor);
        self.bind_expression(statement.incrementor);
        self.flow.add_antecedent(pre_loop, self.current_flow);
        self.current_flow = self.flow.finish_label(post_loop);
    }

    fn bind_for_in_or_of_statement(&mut self, node: Node<'a>, id: NodeId, kind: SyntaxKind) {
        let Node::ForInOrOfStatement(statement) = node else { return };
        let loop_label = self.flow.new_loop_label();
        let pre_loop = self.set_continue_target(id, loop_label);
        let post_loop = self.flow.new_branch_label();
        self.bind_expression(statement.expression);
        self.flow.add_antecedent(pre_loop, self.current_flow);
        self.current_flow = pre_loop;
        if kind == SyntaxKind::ForOfStatement {
            self.bind_optional(statement.await_modifier.map(Node::Token));
        }
        // The loop body may run zero times, so the post-loop label is reachable
        // from before the initializer as well as from the end of the body.
        self.flow.add_antecedent(post_loop, self.current_flow);
        self.bind_optional(statement.initializer.map(Node::from));
        if !matches!(statement.initializer, Some(ForInitializer::VariableDeclarationList(_))) {
            self.bind_assignment_target_flow(statement.initializer.map(Node::from));
        }
        self.bind_iterative_statement(statement.statement.map(Node::from), post_loop, pre_loop);
        self.flow.add_antecedent(pre_loop, self.current_flow);
        self.current_flow = self.flow.finish_label(post_loop);
    }

    fn bind_if_statement(&mut self, node: Node<'a>) {
        let Node::IfStatement(statement) = node else { return };
        let then_label = self.flow.new_branch_label();
        let else_label = self.flow.new_branch_label();
        let post_if = self.flow.new_branch_label();
        self.bind_condition(statement.expression.map(Node::from), then_label, else_label);
        self.current_flow = self.flow.finish_label(then_label);
        self.bind_statement(statement.then_statement);
        self.flow.add_antecedent(post_if, self.current_flow);
        self.current_flow = self.flow.finish_label(else_label);
        self.bind_statement(statement.else_statement);
        self.flow.add_antecedent(post_if, self.current_flow);
        self.current_flow = self.flow.finish_label(post_if);
    }

    fn bind_return_statement(&mut self, node: Node<'a>) {
        let Node::ReturnStatement(statement) = node else { return };
        self.bind_expression(statement.expression);
        if let Some(target) = self.current_return_target {
            self.flow.add_antecedent(target, self.current_flow);
        }
        self.current_flow = self.flow.unreachable();
        self.has_explicit_return = true;
        self.has_flow_effects = true;
    }

    fn bind_throw_statement(&mut self, node: Node<'a>) {
        let Node::ThrowStatement(statement) = node else { return };
        self.bind_expression(statement.expression);
        self.current_flow = self.flow.unreachable();
        self.has_flow_effects = true;
    }

    fn bind_break_or_continue_statement(
        &mut self,
        label: Option<Node<'a>>,
        current_target: Option<FlowId>,
        which: LabelTarget,
    ) {
        self.bind_optional(label);
        let target = match label {
            Some(Node::Identifier(name)) => match self.find_active_label(name.text) {
                Some(index) => {
                    self.active_labels[index].referenced = true;
                    match which {
                        LabelTarget::Break => Some(self.active_labels[index].break_target),
                        LabelTarget::Continue => self.active_labels[index].continue_target,
                    }
                }
                // `break nowhere;` is a parse-time error elsewhere; here it
                // simply has no target, and control carries on.
                None => return,
            },
            _ => current_target,
        };
        if let Some(target) = target {
            self.flow.add_antecedent(target, self.current_flow);
            self.current_flow = self.flow.unreachable();
            self.has_flow_effects = true;
        }
    }

    fn find_active_label(&self, name: &str) -> Option<usize> {
        self.active_labels
            .iter()
            .enumerate()
            .skip(self.labels_base)
            .rev()
            .find_map(|(index, label)| (label.name == name).then_some(index))
    }

    /// `try`/`catch`/`finally`, the one construct whose graph is not a tree.
    ///
    /// A direct port of `bindTryStatement`, including the `ReduceLabel` trick.
    /// Any code in the `try` block may throw, but only a *mutation* can change
    /// what the `catch` block sees, so mutations — not statements — are what
    /// [`Self::create_flow_mutation`] threads onto the exception label.
    fn bind_try_statement(&mut self, node: Node<'a>) {
        let Node::TryStatement(statement) = node else { return };
        let saved_return_target = self.current_return_target;
        let saved_exception_target = self.current_exception_target;
        let normal_exit = self.flow.new_branch_label();
        let return_label = self.flow.new_branch_label();
        let mut exception_label = self.flow.new_branch_label();

        if statement.finally_block.is_some() {
            self.current_return_target = Some(return_label);
        }
        self.flow.add_antecedent(exception_label, self.current_flow);
        self.current_exception_target = Some(exception_label);
        self.bind_optional(statement.try_block.map(Node::Block));
        self.flow.add_antecedent(normal_exit, self.current_flow);

        if let Some(catch_clause) = statement.catch_clause {
            self.current_flow = self.flow.finish_label(exception_label);
            // In a try-catch-finally the catch block is itself a try block: an
            // exception in it still has to reach the finally.
            exception_label = self.flow.new_branch_label();
            self.flow.add_antecedent(exception_label, self.current_flow);
            self.current_exception_target = Some(exception_label);
            self.bind(Node::CatchClause(catch_clause));
            self.flow.add_antecedent(normal_exit, self.current_flow);
        }

        self.current_return_target = saved_return_target;
        self.current_exception_target = saved_exception_target;

        let Some(finally_block) = statement.finally_block else {
            self.current_flow = self.flow.finish_label(normal_exit);
            return;
        };

        // Control reaches the finally block five ways: normal completion of the
        // try or of the catch, a `return` from either, or an exception from
        // either. Analysis that starts *past* the finally must only see the
        // first two, and analysis of an IIFE's returns only the third — so the
        // pre-finally label is given a reduced antecedent set at each of those
        // three exits rather than one set that is right for none of them.
        let finally_label = self.flow.new_branch_label();
        let normal_list = self.flow.antecedent_list(normal_exit);
        let exception_list = self.flow.antecedent_list(exception_label);
        let return_list = self.flow.antecedent_list(return_label);
        let combined = self.flow.combine_lists(exception_list, return_list);
        let combined = self.flow.combine_lists(normal_list, combined);
        self.flow.set_antecedent_list(finally_label, combined);
        self.current_flow = finally_label;
        self.bind(Node::Block(finally_block));

        if self.flow.flags(self.current_flow).contains(FlowFlags::UNREACHABLE) {
            // `try { … } finally { throw x; }` never completes.
            self.current_flow = self.flow.unreachable();
            return;
        }
        if let Some(return_target) = self.current_return_target
            && self.flow.has_antecedents(return_label)
        {
            let current = self.current_flow;
            let reduced = self.flow.new_reduce_label(finally_label, return_list, current);
            self.flow.add_antecedent(return_target, reduced);
        }
        if let Some(exception_target) = self.current_exception_target
            && self.flow.has_antecedents(exception_label)
        {
            let current = self.current_flow;
            let reduced = self.flow.new_reduce_label(finally_label, exception_list, current);
            self.flow.add_antecedent(exception_target, reduced);
        }
        self.current_flow = if self.flow.has_antecedents(normal_exit) {
            let current = self.current_flow;
            self.flow.new_reduce_label(finally_label, normal_list, current)
        } else {
            // `try { return 1; } finally { … }`: the finally completes, but
            // nothing follows the statement.
            self.flow.unreachable()
        };
    }

    fn bind_switch_statement(&mut self, node: Node<'a>, id: NodeId) {
        let Node::SwitchStatement(statement) = node else { return };
        let post_switch = self.flow.new_branch_label();
        self.bind_expression(statement.expression);
        let saved_break = self.current_break_target;
        let saved_pre_switch = self.pre_switch_case_flow;
        self.current_break_target = Some(post_switch);
        self.pre_switch_case_flow = Some(self.current_flow);
        self.bind_optional(statement.case_block.map(Node::CaseBlock));
        self.flow.add_antecedent(post_switch, self.current_flow);

        let has_default = statement.case_block.is_some_and(|block| {
            block.clauses.iter().any(|clause| clause.kind.kind == SyntaxKind::DefaultKeyword)
        });
        if !has_default {
            // Without a `default`, falling past every clause is a way out, and
            // the empty clause range is what tells the checker the subject
            // matched none of them.
            let pre_switch = self.pre_switch_case_flow.expect("set above");
            let clause = self.flow.new_switch_clause(pre_switch, id, 0, 0);
            self.flow.add_antecedent(post_switch, clause);
        }
        self.current_break_target = saved_break;
        self.pre_switch_case_flow = saved_pre_switch;
        self.current_flow = self.flow.finish_label(post_switch);
    }

    fn bind_case_block(&mut self, node: Node<'a>) {
        let Node::CaseBlock(block) = node else { return };
        let switch_statement = self.parent_of(node);
        let Some(switch_id) = switch_statement.and_then(|s| s.node_id()) else { return };
        let subject = switch_statement.and_then(expression_of).map(Node::from);
        // `switch (true)` is the idiom for a chain of predicates, so it narrows
        // even though `true` is not a reference.
        let narrowing = subject.is_some_and(|subject| {
            literal_boolean(subject) == Some(true) || is_narrowing_expression(subject, self.nodes)
        });

        let clauses = block.clauses;
        let mut fallthrough = self.flow.unreachable();
        let mut index = 0;
        while index < clauses.len() {
            let clause_start = index;
            // Empty clauses stack up: `case 1: case 2: body` is one range.
            while clauses[index].statements.is_empty() && index + 1 < clauses.len() {
                if fallthrough == self.flow.unreachable() {
                    self.current_flow = self.pre_switch_case_flow.expect("inside a switch");
                }
                self.bind(Node::CaseOrDefaultClause(clauses[index]));
                index += 1;
            }
            let pre_case = self.flow.new_branch_label();
            let pre_switch = self.pre_switch_case_flow.expect("inside a switch");
            let pre_case_flow = if narrowing {
                self.flow.new_switch_clause(pre_switch, switch_id, clause_start, index + 1)
            } else {
                pre_switch
            };
            self.flow.add_antecedent(pre_case, pre_case_flow);
            self.flow.add_antecedent(pre_case, fallthrough);
            self.current_flow = self.flow.finish_label(pre_case);
            let clause = clauses[index];
            self.bind(Node::CaseOrDefaultClause(clause));
            fallthrough = self.current_flow;
            if !self.flow.flags(self.current_flow).contains(FlowFlags::UNREACHABLE)
                && index != clauses.len() - 1
                && let Some(clause_id) = clause.node_id
            {
                self.fallthrough_flow.insert(clause_id, self.current_flow);
            }
            index += 1;
        }
    }

    fn bind_case_or_default_clause(&mut self, node: Node<'a>) {
        let Node::CaseOrDefaultClause(clause) = node else { return };
        if clause.expression.is_some() {
            // A `case` expression is evaluated where the `switch` was, not after
            // the clauses above it: `case f():` does not see `case g():`'s
            // narrowing.
            let saved = self.current_flow;
            self.current_flow = self.pre_switch_case_flow.expect("inside a switch");
            self.bind_expression(clause.expression);
            self.current_flow = saved;
        }
        for statement in clause.statements {
            self.bind(Node::from(*statement));
        }
    }

    /// TS1344 — `A label is not allowed here.`
    ///
    /// **TS1344 was reported here as well as in the checker.** Upstream has
    /// `checkStrictModeLabeledStatement` (`binder.go:1433`) *and*
    /// `checkGrammarLabeledStatement`; this port ported both and emitted every
    /// line twice — 105 against a wanted 60. The checker's copy is kept because
    /// its statement-kind list is the longer of the two. See
    /// `docs/architecture/checker-notes-diag2.md` §991.
    fn bind_labeled_statement(&mut self, node: Node<'a>) {
        let Node::LabeledStatement(statement) = node else { return };
        let post_statement = self.flow.new_branch_label();
        let Some(label) = statement.label else {
            self.bind_statement(statement.statement);
            return;
        };
        self.active_labels.push(ActiveLabel {
            name: label.text,
            break_target: post_statement,
            continue_target: None,
            referenced: false,
        });
        self.bind(Node::Identifier(label));
        self.bind_statement(statement.statement);
        let active = self.active_labels.pop().expect("pushed just above");
        if !active.referenced
            && let Some(label_id) = label.node_id
        {
            // Recorded, not reported: whether an unused label is an error
            // depends on compiler options the binder does not have.
            self.set_fact(label_id, NodeFacts::UNUSED_LABEL, true);
        }
        self.flow.add_antecedent(post_statement, self.current_flow);
        self.current_flow = self.flow.finish_label(post_statement);
    }

    // ---- expressions ----

    /// Record the assignment that `node` performs, however it is spelled.
    ///
    /// A direct port of `bindAssignmentTargetFlow`.
    fn bind_assignment_target_flow(&mut self, node: Option<Node<'a>>) {
        let Some(node) = node else { return };
        match node {
            Node::ArrayLiteralExpression(array) => {
                for element in array.elements {
                    if let Expression::SpreadElement(spread) = element {
                        self.bind_assignment_target_flow(spread.expression.map(Node::from));
                    } else {
                        self.bind_destructuring_target_flow(Node::from(*element));
                    }
                }
            }
            Node::ObjectLiteralExpression(object) => {
                for property in object.properties {
                    match property {
                        ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                            self.bind_destructuring_target_flow_optional(
                                assignment.initializer.map(Node::from),
                            );
                        }
                        ObjectLiteralElementLike::ShorthandPropertyAssignment(assignment) => {
                            self.bind_assignment_target_flow(Some(Node::from(assignment.name)));
                        }
                        ObjectLiteralElementLike::SpreadAssignment(spread) => {
                            self.bind_assignment_target_flow(spread.expression.map(Node::from));
                        }
                        _ => {}
                    }
                }
            }
            _ => {
                if is_narrowable_reference(node)
                    && let Some(id) = node.node_id()
                {
                    let current = self.current_flow;
                    self.current_flow =
                        self.create_flow_mutation(FlowFlags::ASSIGNMENT, current, id);
                }
            }
        }
    }

    fn bind_destructuring_target_flow_optional(&mut self, node: Option<Node<'a>>) {
        if let Some(node) = node {
            self.bind_destructuring_target_flow(node);
        }
    }

    /// `{ a = 1 } = x` assigns to `a`, not to `a = 1`.
    fn bind_destructuring_target_flow(&mut self, node: Node<'a>) {
        if let Node::BinaryExpression(binary) = node
            && binary.operator_token.map(|token| token.kind) == Some(SyntaxKind::EqualsToken)
        {
            self.bind_assignment_target_flow(binary.left.map(Node::from));
        } else {
            self.bind_assignment_target_flow(Some(node));
        }
    }

    fn bind_prefix_unary_expression_flow(&mut self, node: Node<'a>) {
        let Node::PrefixUnaryExpression(expression) = node else { return };
        if expression.operator.kind == SyntaxKind::ExclamationToken {
            // `!x` inverts which branch is which, so the operand is bound with
            // the targets swapped and nothing else has to know about negation.
            let saved_true = self.current_true_target;
            std::mem::swap(&mut self.current_true_target, &mut self.current_false_target);
            self.bind_each_child(node);
            self.current_false_target = self.current_true_target;
            self.current_true_target = saved_true;
        } else {
            self.bind_each_child(node);
            if matches!(
                expression.operator.kind,
                SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken
            ) {
                self.bind_assignment_target_flow(expression.operand.map(Node::from));
            }
        }
    }

    fn bind_destructuring_assignment_flow(&mut self, node: Node<'a>) {
        let Node::BinaryExpression(binary) = node else { return };
        if self.in_assignment_pattern {
            self.in_assignment_pattern = false;
            self.bind_optional(binary.operator_token.map(Node::Token));
            self.bind_expression(binary.right);
            self.in_assignment_pattern = true;
            self.bind_expression(binary.left);
            self.bind_optional(binary.r#type.map(Node::from));
        } else {
            self.in_assignment_pattern = true;
            self.bind_expression(binary.left);
            self.bind_optional(binary.r#type.map(Node::from));
            self.in_assignment_pattern = false;
            self.bind_optional(binary.operator_token.map(Node::Token));
            self.bind_expression(binary.right);
        }
        self.bind_assignment_target_flow(binary.left.map(Node::from));
    }

    fn bind_binary_expression_flow(&mut self, node: Node<'a>) {
        let Node::BinaryExpression(binary) = node else { return };
        let operator = binary.operator_token.map(|token| token.kind);
        let Some(operator) = operator else {
            self.bind_each_child(node);
            return;
        };

        if is_logical_or_coalescing_binary_operator(operator)
            || is_logical_or_coalescing_assignment_operator(operator)
        {
            if self.is_top_level_logical_expression(node) {
                let post_expression = self.flow.new_branch_label();
                let saved_flow = self.current_flow;
                let saved_has_flow_effects = self.has_flow_effects;
                self.has_flow_effects = false;
                self.bind_logical_like_expression(node, post_expression, post_expression);
                // Nothing in either operand could change a type, so the merge
                // label would be a junction with nothing to merge; keeping the
                // flow the expression started with keeps the graph smaller and
                // the checker's walk shorter.
                self.current_flow = if self.has_flow_effects {
                    self.flow.finish_label(post_expression)
                } else {
                    saved_flow
                };
                self.has_flow_effects = self.has_flow_effects || saved_has_flow_effects;
            } else {
                let true_target = self.current_true_target;
                let false_target = self.current_false_target;
                match (true_target, false_target) {
                    (Some(true_target), Some(false_target)) => {
                        self.bind_logical_like_expression(node, true_target, false_target);
                    }
                    // A logical operator nested inside another one always has
                    // targets; if it somehow does not, binding the children is
                    // the answer that loses narrowing rather than correctness.
                    _ => self.bind_each_child(node),
                }
            }
            return;
        }

        self.bind_expression(binary.left);
        self.bind_optional(binary.r#type.map(Node::from));
        if operator == SyntaxKind::CommaToken {
            self.maybe_bind_call_flow(binary.left.map(Node::from));
        }
        self.bind_optional(binary.operator_token.map(Node::Token));
        self.bind_expression(binary.right);
        if operator == SyntaxKind::CommaToken {
            self.maybe_bind_call_flow(binary.right.map(Node::from));
        }
        if is_assignment_operator(operator) && !self.is_assignment_target(node) {
            self.bind_assignment_target_flow(binary.left.map(Node::from));
            // `a[i] = x` may change the element type of `a`, which the checker
            // tracks as a mutation of `a` rather than of `a[i]`.
            if operator == SyntaxKind::EqualsToken
                && let Some(Expression::ElementAccessExpression(access)) = binary.left
                && access.expression.is_some_and(|inner| {
                    contains_narrowable_reference(Node::from(inner), self.nodes)
                })
                && let Some(id) = node.node_id()
            {
                let current = self.current_flow;
                self.current_flow =
                    self.create_flow_mutation(FlowFlags::ARRAY_MUTATION, current, id);
            }
        }
    }

    fn bind_logical_like_expression(
        &mut self,
        node: Node<'a>,
        true_target: FlowId,
        false_target: FlowId,
    ) {
        let Node::BinaryExpression(binary) = node else { return };
        let operator = binary.operator_token.map(|token| token.kind);
        let pre_right = self.flow.new_branch_label();
        // `a && b` evaluates `b` only when `a` was true; `a || b` only when it
        // was false. That asymmetry is the whole of short-circuit narrowing.
        if matches!(
            operator,
            Some(SyntaxKind::AmpersandAmpersandToken | SyntaxKind::AmpersandAmpersandEqualsToken)
        ) {
            self.bind_condition(binary.left.map(Node::from), pre_right, false_target);
        } else {
            self.bind_condition(binary.left.map(Node::from), true_target, pre_right);
        }
        self.current_flow = self.flow.finish_label(pre_right);
        self.bind_optional(binary.operator_token.map(Node::Token));

        if operator.is_some_and(is_logical_or_coalescing_assignment_operator) {
            self.with_conditional_branches(binary.right.map(Node::from), true_target, false_target);
            self.bind_assignment_target_flow(binary.left.map(Node::from));
            let current = self.current_flow;
            let when_true =
                self.create_flow_condition(FlowFlags::TRUE_CONDITION, current, Some(node));
            self.flow.add_antecedent(true_target, when_true);
            let when_false =
                self.create_flow_condition(FlowFlags::FALSE_CONDITION, current, Some(node));
            self.flow.add_antecedent(false_target, when_false);
        } else {
            self.bind_condition(binary.right.map(Node::from), true_target, false_target);
        }
    }

    fn bind_conditional_expression_flow(&mut self, node: Node<'a>) {
        let Node::ConditionalExpression(expression) = node else { return };
        let true_label = self.flow.new_branch_label();
        let false_label = self.flow.new_branch_label();
        let post_expression = self.flow.new_branch_label();
        let saved_flow = self.current_flow;
        let saved_has_flow_effects = self.has_flow_effects;
        self.has_flow_effects = false;
        self.bind_condition(expression.condition.map(Node::from), true_label, false_label);
        self.current_flow = self.flow.finish_label(true_label);
        self.bind_optional(expression.question_token.map(Node::Token));
        self.bind_expression(expression.when_true);
        self.flow.add_antecedent(post_expression, self.current_flow);
        self.current_flow = self.flow.finish_label(false_label);
        self.bind_optional(expression.colon_token.map(Node::Token));
        self.bind_expression(expression.when_false);
        self.flow.add_antecedent(post_expression, self.current_flow);
        self.current_flow = if self.has_flow_effects {
            self.flow.finish_label(post_expression)
        } else {
            saved_flow
        };
        self.has_flow_effects = self.has_flow_effects || saved_has_flow_effects;
    }

    fn bind_variable_declaration_flow(&mut self, node: Node<'a>, id: NodeId) {
        let Node::VariableDeclaration(declaration) = node else { return };
        self.bind_each_child(node);
        // `for (const x of xs)` assigns to `x` on every iteration even though
        // the declaration has no initializer.
        let in_for_in_or_of =
            self.nodes.parent(id).and_then(|list| self.nodes.parent(list)).is_some_and(
                |statement| {
                    matches!(
                        self.nodes.kind(statement),
                        SyntaxKind::ForInStatement | SyntaxKind::ForOfStatement
                    )
                },
            );
        if declaration.initializer.is_some() || in_for_in_or_of {
            self.bind_initialized_variable_flow(declaration.name, id);
        }
    }

    /// One assignment node per name a declaration introduces.
    ///
    /// `const { a, b } = x` assigns to two references, not to the pattern.
    fn bind_initialized_variable_flow(&mut self, name: Option<BindingName<'a>>, id: NodeId) {
        if let Some(BindingName::BindingPattern(pattern)) = name {
            for element in pattern.elements {
                let Some(element_id) = element.node_id else { continue };
                self.bind_initialized_variable_flow(element.name, element_id);
            }
        } else {
            let current = self.current_flow;
            self.current_flow = self.create_flow_mutation(FlowFlags::ASSIGNMENT, current, id);
        }
    }

    fn bind_call_expression_flow(&mut self, node: Node<'a>, id: NodeId) {
        let Node::CallExpression(call) = node else { return };
        if is_optional_chain(node, self.nodes) {
            self.bind_optional_chain_flow(node);
        } else {
            let callee = call.expression.map(|expression| skip_parentheses(Node::from(expression)));
            if matches!(callee, Some(Node::FunctionExpression(_) | Node::ArrowFunction(_))) {
                // An IIFE's arguments are evaluated before the function runs, so
                // they are bound first and the function body picks up the flow
                // they left.
                for argument in call.type_arguments {
                    self.bind(Node::from(*argument));
                }
                for argument in call.arguments {
                    self.bind(Node::from(*argument));
                }
                self.bind_expression(call.expression);
            } else {
                self.bind_each_child(node);
                if matches!(callee, Some(Node::KeywordExpression(k)) if k.kind == SyntaxKind::SuperKeyword)
                {
                    let current = self.current_flow;
                    self.current_flow = self.create_flow_call(current, id);
                }
            }
        }
        // `a.push(x)` may change what `a` holds.
        if let Some(Expression::PropertyAccessExpression(access)) = call.expression
            && let Some(tsr_ast::MemberName::Identifier(name)) = access.name
            && is_push_or_unshift_identifier(name.text)
            && access
                .expression
                .is_some_and(|inner| contains_narrowable_reference(Node::from(inner), self.nodes))
        {
            let current = self.current_flow;
            self.current_flow = self.create_flow_mutation(FlowFlags::ARRAY_MUTATION, current, id);
        }
    }

    /// A statement-level call with a dotted name is a candidate assertion.
    ///
    /// `assert(x)` narrows `x` for everything after it, and the binder cannot
    /// tell an assertion from an ordinary call — that needs the signature — so
    /// it records every call that *could* be one and lets the checker decide.
    fn maybe_bind_call_flow(&mut self, node: Option<Node<'a>>) {
        let Some(Node::CallExpression(call)) = node else { return };
        let Some(callee) = call.expression.map(Node::from) else { return };
        if matches!(callee, Node::KeywordExpression(k) if k.kind == SyntaxKind::SuperKeyword) {
            return;
        }
        if !is_dotted_name(callee) {
            return;
        }
        let Some(id) = node.and_then(|node| node.node_id()) else { return };
        let current = self.current_flow;
        self.current_flow = self.create_flow_call(current, id);
    }

    /// A parameter or binding-element initializer does not always run.
    ///
    /// `function f(a = g()) {}` may or may not call `g`, so the flow after the
    /// parameter list is the merge of "the default ran" and "it did not". See
    /// upstream's `bindInitializer` and TypeScript#49759.
    fn bind_initializer(&mut self, initializer: Option<Expression<'a>>) {
        let Some(initializer) = initializer else { return };
        let entry_flow = self.current_flow;
        self.bind(Node::from(initializer));
        if entry_flow == self.flow.unreachable() || entry_flow == self.current_flow {
            return;
        }
        let exit = self.flow.new_branch_label();
        self.flow.add_antecedent(exit, entry_flow);
        self.flow.add_antecedent(exit, self.current_flow);
        self.current_flow = self.flow.finish_label(exit);
    }

    // ---- optional chains ----
    //
    // `a?.b` behaves like `a && a.b`, so its graph is a logical expression's.
    // Every function below is a direct port; they became reachable at §748,
    // when the parser began setting `NodeFlags::OPTIONAL_CHAIN` (see
    // `narrowing::is_optional_chain`).

    fn bind_optional_chain_flow(&mut self, node: Node<'a>) {
        if self.is_top_level_logical_expression(node) {
            let post_expression = self.flow.new_branch_label();
            let saved_flow = self.current_flow;
            let saved_has_flow_effects = self.has_flow_effects;
            self.bind_optional_chain(node, post_expression, post_expression);
            self.current_flow = if self.has_flow_effects {
                self.flow.finish_label(post_expression)
            } else {
                saved_flow
            };
            self.has_flow_effects = self.has_flow_effects || saved_has_flow_effects;
        } else {
            match (self.current_true_target, self.current_false_target) {
                (Some(true_target), Some(false_target)) => {
                    self.bind_optional_chain(node, true_target, false_target);
                }
                _ => self.bind_each_child(node),
            }
        }
    }

    fn bind_optional_chain(&mut self, node: Node<'a>, true_target: FlowId, false_target: FlowId) {
        let pre_chain =
            is_optional_chain_root(node, self.nodes).then(|| self.flow.new_branch_label());
        let expression = expression_of(node).map(Node::from);
        self.bind_optional_expression(expression, pre_chain.unwrap_or(true_target), false_target);
        if let Some(pre_chain) = pre_chain {
            self.current_flow = self.flow.finish_label(pre_chain);
        }
        let saved_true = self.current_true_target;
        let saved_false = self.current_false_target;
        self.current_true_target = Some(true_target);
        self.current_false_target = Some(false_target);
        self.bind_optional_chain_rest(node);
        self.current_true_target = saved_true;
        self.current_false_target = saved_false;

        if is_outermost_optional_chain(node, self.parent_of(node), self.nodes) {
            let current = self.current_flow;
            let when_true =
                self.create_flow_condition(FlowFlags::TRUE_CONDITION, current, Some(node));
            self.flow.add_antecedent(true_target, when_true);
            let when_false =
                self.create_flow_condition(FlowFlags::FALSE_CONDITION, current, Some(node));
            self.flow.add_antecedent(false_target, when_false);
        }
    }

    fn bind_optional_expression(
        &mut self,
        node: Option<Node<'a>>,
        true_target: FlowId,
        false_target: FlowId,
    ) {
        self.with_conditional_branches(node, true_target, false_target);
        let Some(node) = node else { return };
        if !is_optional_chain(node, self.nodes)
            || is_outermost_optional_chain(node, self.parent_of(node), self.nodes)
        {
            let current = self.current_flow;
            let when_true =
                self.create_flow_condition(FlowFlags::TRUE_CONDITION, current, Some(node));
            self.flow.add_antecedent(true_target, when_true);
            let when_false =
                self.create_flow_condition(FlowFlags::FALSE_CONDITION, current, Some(node));
            self.flow.add_antecedent(false_target, when_false);
        }
    }

    /// Everything in a chain link but the expression it chains from.
    fn bind_optional_chain_rest(&mut self, node: Node<'a>) {
        match node {
            Node::PropertyAccessExpression(access) => {
                self.bind_optional(access.question_dot_token.map(Node::Token));
                self.bind_optional(access.name.map(Node::from));
            }
            Node::ElementAccessExpression(access) => {
                self.bind_optional(access.question_dot_token.map(Node::Token));
                self.bind_expression(access.argument_expression);
            }
            Node::CallExpression(call) => {
                self.bind_optional(call.question_dot_token.map(Node::Token));
                for argument in call.type_arguments {
                    self.bind(Node::from(*argument));
                }
                for argument in call.arguments {
                    self.bind(Node::from(*argument));
                }
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------ ancestry --

    /// The parent of `node`, as a node rather than an id.
    ///
    /// Valid only for the node currently being bound or one of its direct
    /// children, which is every caller: the predicates that need a parent are
    /// asked about the current node or about a field of it.
    fn parent_of(&self, node: Node<'a>) -> Option<Node<'a>> {
        let id = node.node_id()?;
        let (top_id, top) = *self.ancestors.last()?;
        if top_id == id {
            return self.ancestors.len().checked_sub(2).map(|index| self.ancestors[index].1);
        }
        debug_assert_eq!(
            self.nodes.parent(id),
            Some(top_id),
            "parent_of is only valid for the current node or a direct child of it"
        );
        Some(top)
    }

    /// Whether `node` is a logical expression nobody is going to narrow *for*.
    ///
    /// Upstream: `isTopLevelLogicalExpression`. A `&&` used as an `if`
    /// condition, or as the left of another `&&`, is bound by whoever is asking
    /// for the branch; one used as a value has to build its own merge point.
    fn is_top_level_logical_expression(&self, node: Node<'a>) -> bool {
        let Some(id) = node.node_id() else { return true };
        // Position in the ancestor chain, so walking up is an index step.
        let Some(mut index) = self.parent_index(id) else { return true };
        let mut child = id;
        loop {
            let (parent_id, parent) = self.ancestors[index];
            let transparent = match parent {
                Node::ParenthesizedExpression(_) => true,
                Node::PrefixUnaryExpression(unary) => {
                    unary.operator.kind == SyntaxKind::ExclamationToken
                }
                _ => false,
            };
            if !transparent {
                let chained_from = is_optional_chain(parent, self.nodes)
                    && expression_of(parent).and_then(|e| Node::from(e).node_id()) == Some(child);
                return !is_statement_condition(child, parent)
                    && !is_logical_expression(parent)
                    && !chained_from;
            }
            child = parent_id;
            match index.checked_sub(1) {
                Some(next) => index = next,
                None => return true,
            }
        }
    }

    /// Whether `node` is itself being assigned to (`ast.IsAssignmentTarget`).
    ///
    /// `[a] = b` makes the array literal an assignment target, so the `= b` is
    /// bound by the destructuring path and not again as an ordinary assignment.
    fn is_assignment_target(&self, node: Node<'a>) -> bool {
        let Some(id) = node.node_id() else { return false };
        let Some(mut index) = self.parent_index(id) else { return false };
        let mut child = id;
        loop {
            let (parent_id, parent) = self.ancestors[index];
            // `Step::Up(n)` means upstream's `node = parent` (1) or
            // `node = parent.Parent` (2): a property in a destructuring pattern
            // is not the target, the *object literal* around it is.
            let steps = match parent {
                Node::BinaryExpression(binary) => {
                    return binary
                        .operator_token
                        .is_some_and(|token| is_assignment_operator(token.kind))
                        && binary.left.and_then(|left| Node::from(left).node_id()) == Some(child);
                }
                Node::PrefixUnaryExpression(unary) => {
                    return matches!(
                        unary.operator.kind,
                        SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken
                    );
                }
                Node::PostfixUnaryExpression(unary) => {
                    return matches!(
                        unary.operator.kind,
                        SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken
                    );
                }
                Node::ForInOrOfStatement(statement) => {
                    return statement
                        .initializer
                        .and_then(|initializer| Node::from(initializer).node_id())
                        == Some(child);
                }
                Node::ParenthesizedExpression(_)
                | Node::ArrayLiteralExpression(_)
                | Node::SpreadElement(_)
                | Node::NonNullExpression(_) => 1,
                Node::SpreadAssignment(_) => 2,
                Node::ShorthandPropertyAssignment(assignment) => {
                    if Node::from(assignment.name).node_id() != Some(child) {
                        return false;
                    }
                    2
                }
                Node::PropertyAssignment(assignment) => {
                    if Node::from(assignment.name).node_id() == Some(child) {
                        return false;
                    }
                    2
                }
                _ => return false,
            };
            let Some(next) = index.checked_sub(steps) else { return false };
            child = if steps == 1 { parent_id } else { self.ancestors[index - 1].0 };
            index = next;
        }
    }

    /// Where `id`'s parent sits in the ancestor chain.
    ///
    /// `id` is the node being bound (the top of the chain) or a direct child of
    /// it, so the answer is one of the last two entries.
    fn parent_index(&self, id: NodeId) -> Option<usize> {
        match self.ancestors.last() {
            Some((top_id, _)) if *top_id == id => self.ancestors.len().checked_sub(2),
            Some(_) => self.ancestors.len().checked_sub(1),
            None => None,
        }
    }

    /// Whether `id` sits inside a `typeof X` type (`ast.IsPartOfTypeQuery`).
    ///
    /// Kinds only, so it reads the side table and never needs a node.
    fn is_part_of_type_query(&self, id: NodeId) -> bool {
        let mut current = id;
        loop {
            let kind = self.nodes.kind(current);
            if !matches!(kind, SyntaxKind::QualifiedName | SyntaxKind::Identifier) {
                return kind == SyntaxKind::TypeQuery;
            }
            match self.nodes.parent(current) {
                Some(parent) => current = parent,
                None => return false,
            }
        }
    }

    /// Whether `id` is a function expression that is called where it is written.
    fn is_immediately_invoked(&self, id: NodeId) -> bool {
        let mut previous = id;
        let Some(mut index) = self.ancestors.len().checked_sub(2) else { return false };
        loop {
            let (ancestor_id, ancestor) = self.ancestors[index];
            match ancestor {
                Node::ParenthesizedExpression(_) => previous = ancestor_id,
                Node::CallExpression(call) => {
                    return call.expression.and_then(|expression| Node::from(expression).node_id())
                        == Some(previous);
                }
                _ => return false,
            }
            match index.checked_sub(1) {
                Some(next) => index = next,
                None => return false,
            }
        }
    }

    // ---------------------------------------------------------------- facts --

    fn set_fact(&mut self, id: NodeId, fact: NodeFacts, value: bool) {
        if value {
            *self.facts.entry(id).or_default() |= fact;
        }
    }

    // -------------------------------------------------------------- symbols --

    /// What kind of symbol `node` declares, and where it belongs.
    ///
    /// Wraps the kind-only [`classify`] with the cases that need more than the
    /// node. A variable's scope depends on whether it was written `var`, `let`,
    /// or `const`, and all three produce the same node kind; upstream keeps that
    /// in node flags and so do we, so it is reached through the side table rather
    /// than the tree.
    fn classify(&self, node: Node<'a>, id: NodeId) -> Option<(SymbolFlags, Destination)> {
        // A binding element declares whatever its root declaration would have:
        // `const { a } = x` is a `const` declaration of `a`, and `function f({ a })`
        // makes `a` a parameter.
        if matches!(node, Node::VariableDeclaration(_) | Node::BindingElement(_)) {
            return Some((
                if self.is_require_alias_variable(node, id) {
                    SymbolFlags::ALIAS
                } else if self.is_block_or_catch_scoped(id) {
                    SymbolFlags::BLOCK_SCOPED_VARIABLE
                } else {
                    SymbolFlags::FUNCTION_SCOPED_VARIABLE
                },
                Destination::Locals,
            ));
        }
        // A type parameter's table comes from its **container**, not from its own
        // kind. Upstream has no per-kind answer at all: every declaration goes
        // through `declareSymbolAndAddToSymbolTable`, which switches on
        // `b.container.Kind` (`internal/binder/binder.go:429`), and for a class or
        // an interface that is `GetMembers(container.Symbol())` — a type parameter
        // is a member of the type it parameterises.
        //
        // `classify` answers per-kind, so a type parameter was reaching
        // `Destination::Locals` and `locals_owner` resolved that to the nearest
        // *locals* container. A class is `IsContainer` without `HasLocals`
        // (`container.rs:53`, matching `GetContainerFlags`), so it is not one: the
        // `T` of `class A<T>` was filed in the enclosing file, where it met the `T`
        // of `class B<T>` and merged with it. One symbol, two declarations, in
        // unrelated classes — a resolution defect, and the reason the binder
        // reported `TS2300` on 4,132 type parameters upstream says nothing about
        // (95% of the remaining over-reports; `examples/ts2300_constructs.rs`).
        //
        // Only the containers that own a symbol are listed. Anything else — a
        // function, method, signature, type alias, mapped type — is upstream's
        // `GetLocals(b.container)` branch (`binder.go:444`), which is what
        // `Destination::Locals` already means, and those were never wrong.
        if matches!(node, Node::TypeParameterDeclaration(_)) {
            let destination = match self.ancestors.last().map(|(_, parent)| *parent) {
                Some(
                    Node::ClassDeclaration(_)
                    | Node::ClassExpression(_)
                    | Node::InterfaceDeclaration(_)
                    | Node::TypeLiteralNode(_),
                ) => Destination::Members,
                // `b.container.Kind == KindEnumDeclaration` takes exports
                // (`binder.go:437`). Unreachable in valid syntax — an enum has no
                // type parameters — but the parser will hand us one from
                // `enum E<T> {}`, and guessing `Locals` there would put it in the
                // enclosing scope.
                Some(Node::EnumDeclaration(_)) => Destination::Exports,
                _ => Destination::Locals,
            };
            return Some((SymbolFlags::TYPE_PARAMETER, destination));
        }
        // `bindModuleDeclaration`'s ambient arm (`binder.go:773-781`): an
        // ambient module — `declare module "x"`, or `declare global` — that is
        // not an external augmentation is declared `ValueModule`
        // unconditionally, whatever its instance state. Only augmentations and
        // ordinary namespaces go through `declareModuleSymbol`'s
        // instance-state choice, which the kind-only [`classify`] makes.
        // A type-only `declare module "x" { export type T = … }` in a script
        // was a `NamespaceModule` here, so `tryFindAmbientModule`'s
        // `ValueModule` meaning filter (`checker.go:15533`) missed it and every
        // import of it reported TS2307 (`docs/parity/notes/names-modules.md` §1).
        // The node is not yet on `ancestors` when it is declared, so the chain
        // starts at its parent.
        if let Node::ModuleDeclaration(module) = node
            && is_ambient_module(module)
            && !self
                .is_module_augmentation_external(self.ancestors.iter().rev().map(|(_, node)| *node))
        {
            return Some((SymbolFlags::VALUE_MODULE, Destination::Locals));
        }

        let (flags, destination) = classify(node, &self.ancestors)?;

        // A `static` class member belongs to the class's **exports**, an instance
        // member to its **members**. Upstream's `declareClassMember` splits on
        // `ast.IsStatic` (`internal/binder/binder.go:414-419`); `classify` had one
        // answer for both, so `static get x()` and `get x()` shared a symbol —
        // `static m()`/`m()` and `static p`/`p` too. Only the accessors reported
        // anything (`GetAccessorExcludes` still collides with itself), which is why
        // this looked like an `excludes()` problem: 34 of the over-reports left
        // after ADR-0023 are that pair. The method and property cases were silent
        // resolution defects, and are the reason this is worth more than the 34.
        //
        // Guarded on the container being a class, because that is the only branch
        // of upstream's switch that consults `IsStatic` (`binder.go:435`). `static`
        // is not valid on an interface or type-literal member, but the parser
        // recovers from it, and honouring it there would move the member into a
        // table upstream never puts it in.
        if destination == Destination::Members
            && matches!(
                self.ancestors.last().map(|(_, parent)| *parent),
                Some(Node::ClassDeclaration(_) | Node::ClassExpression(_))
            )
            && is_static(node)
        {
            return Some((flags, Destination::Exports));
        }

        // An enum's members belong to the enum's **exports**, not its members.
        // Upstream's switch has a whole branch for it
        // (`internal/binder/binder.go:436-437`):
        //
        // ```go
        // case ast.KindEnumDeclaration:
        //     return b.declareSymbol(ast.GetExports(b.container.Symbol()), ...)
        // ```
        //
        // `classify` answers per-kind and had `Destination::Members` for
        // `Node::EnumMember`, so `E.A` was unreachable from `typeof E`: the
        // checker's `getPropertyOfType` reads `exports` for an anonymous type
        // (`checker.go:20672`), correctly, and the member was not in it. That
        // was ~1,560 assertion lines answering `error`, and the checker side
        // was already complete — `get_type_of_enum_member` answers `E.A` when
        // handed the symbol.
        //
        // **Same container-not-kind reasoning as the two branches above**, and
        // the third instance of it: the type-parameter case already routes an
        // enum container to `Exports` at the top of this function for exactly
        // upstream's reason. This is that rule applied to the members the enum
        // actually has.
        //
        // Gated on `Members` although upstream's branch is unconditional on the
        // container. `classify` puts nothing else in an enum body — an enum can
        // hold only enum members — so the two agree on every tree the parser
        // produces from valid syntax, and restricting it this way means a node
        // the parser recovered into an enum body cannot be moved into a table
        // on the strength of a guess.
        if destination == Destination::Members
            && matches!(
                self.ancestors.last().map(|(_, parent)| *parent),
                Some(Node::EnumDeclaration(_))
            )
        {
            return Some((flags, Destination::Exports));
        }
        Some((flags, destination))
    }

    /// `IsVariableDeclarationInitializedToRequire` (native utilities.go:2825).
    /// This is syntactic, unlike the checker's admission of require CALLS.
    /// Binding patterns are a separate import-specifier resolution path.
    fn is_require_alias_variable(&self, node: Node<'a>, id: NodeId) -> bool {
        let Node::VariableDeclaration(variable) = node else { return false };
        if !self.in_js_file
            || variable.r#type.is_some()
            || !matches!(variable.name, Some(tsr_ast::BindingName::Identifier(_)))
            || self.jsdoc_type_hosts.contains(&id)
        {
            return false;
        }
        let statement = self.nodes.parent(id).and_then(|list| self.nodes.parent(list));
        if statement.is_some_and(|statement| self.jsdoc_type_hosts.contains(&statement)) {
            return false;
        }
        if self.ancestors.iter().rev().any(|(ancestor, node)| {
            Some(*ancestor) == statement
                && matches!(node, Node::VariableStatement(statement)
                    if has_export(statement.modifiers))
        }) {
            return false;
        }
        matches!(variable.initializer, Some(Expression::CallExpression(call))
            if matches!(call.expression, Some(Expression::Identifier(name)) if name.text == "require")
                && call.arguments.len() == 1
                && matches!(call.arguments[0], Expression::StringLiteral(_) | Expression::NoSubstitutionTemplateLiteral(_)))
    }

    /// Whether a declaration is scoped to the nearest block rather than the
    /// nearest function.
    ///
    /// Upstream: `ast.IsBlockOrCatchScoped`. The catch half is not an
    /// afterthought — `catch (e)` binds `e` to the clause and nothing else, and
    /// reading only the immediate parent's flags (which is what this used to do)
    /// put it in the enclosing function instead.
    fn is_block_or_catch_scoped(&self, id: NodeId) -> bool {
        if self.combined_node_flags(id).intersects(NodeFlags::BLOCK_SCOPED) {
            return true;
        }
        let root = self.root_declaration(id);
        self.nodes.kind(root) == SyntaxKind::VariableDeclaration
            && self
                .nodes
                .parent(root)
                .is_some_and(|parent| self.nodes.kind(parent) == SyntaxKind::CatchClause)
    }

    /// A declaration's own flags, plus those of the `var`/`let`/`const` list and
    /// statement it belongs to.
    ///
    /// Upstream: `ast.GetCombinedNodeFlags`. `let`-ness is recorded on the
    /// declaration *list*, not on each declaration, so a declaration read in
    /// isolation looks function-scoped.
    fn combined_node_flags(&self, id: NodeId) -> NodeFlags {
        let root = self.root_declaration(id);
        let mut flags = self.nodes.flags(root);
        let mut current = root;
        if self.nodes.kind(current) == SyntaxKind::VariableDeclaration {
            let Some(parent) = self.nodes.parent(current) else { return flags };
            current = parent;
        }
        if self.nodes.kind(current) == SyntaxKind::VariableDeclarationList {
            flags |= self.nodes.flags(current);
            let Some(parent) = self.nodes.parent(current) else { return flags };
            current = parent;
        }
        if self.nodes.kind(current) == SyntaxKind::VariableStatement {
            flags |= self.nodes.flags(current);
        }
        flags
    }

    /// The declaration a binding element ultimately belongs to.
    ///
    /// Upstream: `ast.GetRootDeclaration`. Each step is two levels, because a
    /// binding element's parent is the pattern that contains it.
    fn root_declaration(&self, id: NodeId) -> NodeId {
        let mut current = id;
        while self.nodes.kind(current) == SyntaxKind::BindingElement {
            let Some(pattern) = self.nodes.parent(current) else { break };
            let Some(parent) = self.nodes.parent(pattern) else { break };
            current = parent;
        }
        current
    }

    /// Which node's `locals` a declaration with these flags belongs in.
    ///
    /// `var` and functions go to the nearest *function* scope; `let`, `const`,
    /// and classes go to the nearest block. This is the whole of `var` hoisting.
    /// Which locals table a `Destination::Locals` declaration goes into.
    ///
    /// Upstream has two entry points, and which one a declaration uses is fixed
    /// per kind: `declareSymbolAndAddToSymbolTable` files into
    /// `GetLocals(b.container)` (`binder.go:444`), and
    /// `bindBlockScopedDeclaration` into `GetLocals(b.blockScopeContainer)`
    /// (`binder.go:1249`). Six kinds take the second: block-scoped variables
    /// (`binder.go:1171`), classes (`:944`), **interfaces** (`:681`), **type
    /// aliases** (`:693`), **enums**, both `const` and regular (`:1158`, `:1160`),
    /// and **function declarations** (`:1216`).
    ///
    /// The last four were missing, so they were filed in the enclosing *function*
    /// instead of the enclosing block. That only diverges inside a nested block —
    /// `self.block` and `self.container` are the same node at the top of a
    /// function, because both are set for every `IS_CONTAINER` — which is why it
    /// took a case with two `type A`s in sibling blocks to show it
    /// (`compiler/narrowingOfQualifiedNames`, 63 of the over-reports remaining
    /// after ADR-0023).
    ///
    /// Function declarations being block-scoped is upstream's behaviour
    /// unconditionally, not a strict-mode-only rule: `bindFunctionDeclaration`
    /// calls `bindBlockScopedDeclaration` with no dialect test.
    fn locals_owner(&self, flags: SymbolFlags) -> NodeId {
        if flags.intersects(
            SymbolFlags::BLOCK_SCOPED_VARIABLE
                | SymbolFlags::CLASS
                | SymbolFlags::INTERFACE
                | SymbolFlags::TYPE_ALIAS
                | SymbolFlags::ENUM
                | SymbolFlags::FUNCTION,
        ) {
            self.block
        } else {
            self.container
        }
    }

    /// Whether `node` is `constructor(public x)` and so declares a class member.
    ///
    /// Upstream: `ast.IsParameterPropertyDeclaration`. The accessibility,
    /// `readonly`, and `override` modifiers are the ones that promote a parameter
    /// to a property; `constructor(x)` alone does not.
    fn is_parameter_property(&self, node: Node<'a>) -> bool {
        let Node::ParameterDeclaration(parameter) = node else { return false };
        if !parameter.modifiers.iter().any(|modifier| {
            matches!(modifier, tsr_ast::ModifierLike::Token(token) if matches!(
                token.kind,
                SyntaxKind::PublicKeyword
                    | SyntaxKind::PrivateKeyword
                    | SyntaxKind::ProtectedKeyword
                    | SyntaxKind::ReadonlyKeyword
                    | SyntaxKind::OverrideKeyword
            ))
        }) {
            return false;
        }
        matches!(self.ancestors.last(), Some((_, Node::ConstructorDeclaration(_))))
    }

    /// Whether declarations in the current container are exported from it.
    ///
    /// Upstream: the `hasExportModifier || container.Flags&NodeFlagsExportContext`
    /// test in `declareModuleMember`. Two ways to be exported from a namespace —
    /// say so, or be inside an ambient one that exports everything implicitly.
    fn is_exported_from_container(&self, node: Node<'a>) -> bool {
        // An export specifier is *always* an export of its container, and there is
        // no `export` modifier on it to find — the keyword belongs to the
        // `export { … }` declaration two levels up, not to the specifier that gets
        // the symbol. Upstream does not look for a modifier either: it tests the
        // node kind (`declareModuleMember`, `binder.go:377`).
        //
        // Without this, `export { a as a1 } from "m"` declared a **local** `a1`,
        // which then collided with the local of `import { a as a1 } from "m"` in the
        // same file — 46 of the 92 over-reports left after ADR-0024, counted once on
        // the export specifier and once on the import specifier because we report on
        // every declaration involved.
        //
        // This holds with or without a module specifier. `export { x }` re-exporting
        // a local `x` is the case that looks like it should be a local, and is not:
        // the export half is a separate symbol in the container's exports, which is
        // what makes `M.x` reachable, and the local it aliases already exists.
        // declareModuleMember handles aliases before ExportContext
        // (`binder.go:399-404`): only explicit export specifiers and exported
        // import-equals declarations enter exports; every other import stays
        // local, in a source file and in an ambient module alike. The ambient
        // module arm used to follow ExportContext, which filed
        // `declare module "m" { import { I } from "n"; }`'s `I` in the
        // module's exports, where the exports arm of name resolution declines
        // an ambient alias — so `I` never resolved inside its own module
        // (`docs/parity/notes/names-modules.md` §1).
        let exported = match node {
            Node::ExportSpecifier(_) => true,
            Node::ImportEqualsDeclaration(_)
                if self.nodes.kind(self.container) == SyntaxKind::SourceFile =>
            {
                self.has_export_modifier(node)
            }
            Node::ImportClause(_) | Node::ImportSpecifier(_) | Node::NamespaceImport(_)
                if self.nodes.kind(self.container) == SyntaxKind::SourceFile =>
            {
                false
            }
            _ => self.export_context || self.has_export_modifier(node),
        };
        match self.nodes.kind(self.container) {
            SyntaxKind::ModuleDeclaration => exported,
            // `declareSourceFileMember` routes through `declareModuleMember` only
            // for an external module. A script's top-level `export` — which the
            // parser accepts and the checker rejects — has nothing to export
            // from, so it stays a plain local.
            SyntaxKind::SourceFile => self.is_module && exported,
            _ => false,
        }
    }

    /// Whether `node` is the thing a `export default …` exports.
    ///
    /// Upstream's `isDefaultExport` test in `declareSymbolEx`, which is what
    /// renames the *export* half of the pair to `default` while the local half
    /// keeps the name the source wrote.
    fn is_default_export(node: Node<'a>) -> bool {
        if modifiers_of(node).is_some_and(|m| has_modifier(m, SyntaxKind::DefaultKeyword)) {
            return true;
        }
        matches!(node, Node::ExportSpecifier(specifier)
            if specifier.name.map(export_name) == Some(INTERNAL_DEFAULT))
    }

    /// Whether `node` carries `export`, including on the statement that owns it.
    ///
    /// `export const a = 1` puts the modifier on the `VariableStatement`, two
    /// levels above the declaration that gets the symbol, so a declaration read
    /// on its own never looks exported. Upstream reaches it with
    /// `GetCombinedModifierFlags`; here the ancestor chain already holds the
    /// nodes, so it is a short walk rather than a second traversal.
    fn has_export_modifier(&self, node: Node<'a>) -> bool {
        if modifiers_of(node).is_some_and(has_export) {
            return true;
        }
        for (_, ancestor) in self.ancestors.iter().rev() {
            match ancestor {
                Node::VariableStatement(statement) => return has_export(statement.modifiers),
                // Everything between a binding and its statement is transparent.
                Node::VariableDeclarationList(_)
                | Node::VariableDeclaration(_)
                | Node::BindingPattern(_)
                | Node::BindingElement(_) => {}
                _ => return false,
            }
        }
        false
    }

    /// Give the file its own symbol, as `bindSourceFileAsExternalModule`.
    ///
    /// Reached twice: up front when a top-level `import`/`export` says the file is
    /// an ES module, and again from `set_commonjs_module_indicator` when the
    /// first `module.exports =` says a JavaScript file is a `CommonJS` one. The
    /// second may happen part-way through the walk, which is why the symbol is
    /// remembered on the binder rather than in the [`Self::owner`] cursor.
    /// File `@typedef`/`@callback`/`@template`/`@overload` declarations.
    ///
    /// A fresh symbol per tag is enough for name-and-lines fidelity: a
    /// same-named written declaration merges at the *suite's* union key, and
    /// the checker's JSDoc reading has its own road. `@overload` differs — it
    /// is another declaration of the documented function's own symbol, so the
    /// tag's node joins that symbol's declaration list, exactly where
    /// upstream's reparsed signature would sit.
    fn bind_jsdoc_declarations(
        &mut self,
        root: NodeId,
        source: Node<'a>,
        jsdoc: &[(NodeId, &'a [&'a tsr_ast::JSDoc<'a>])],
    ) {
        use tsr_ast::JSDocTag;
        for (host, docs) in jsdoc {
            for doc in *docs {
                // Native reparents the gathered template nodes under a
                // JSTypeAliasDeclaration (reparser.go::reparseUnhosted's
                // typedef/callback arms, `gatherTypeParameters` with
                // typedefOrCallback). Keep the immutable tree and use its
                // comment as the lexical locals owner instead: every nested
                // annotation climbs here. The nested `@property`/`@param`
                // children are already inside the tag's reparsed body, which
                // binds like any written type literal or function type.
                let declares_alias = doc.tags.iter().any(|tag| {
                    matches!(tag, JSDocTag::JSDocTypedefTag(_) | JSDocTag::JSDocCallbackTag(_))
                });
                let template_scope = if declares_alias {
                    doc.node_id
                } else {
                    self.jsdoc_function_like_host(source, *host)
                };
                for tag in doc.tags {
                    match tag {
                        JSDocTag::JSDocTypedefTag(typedef) => {
                            let name = match typedef.name {
                                Some(tsr_ast::JSDocFullName::Identifier(identifier)) => {
                                    identifier.text
                                }
                                _ => continue,
                            };
                            let Some(id) = typedef.node_id else { continue };
                            self.declare_jsdoc_symbol(root, name, SymbolFlags::TYPE_ALIAS, id);
                            // The tag's type expression is parsed syntax — a
                            // `{{a: string}}` object carries members upstream
                            // binds like any written type literal.
                            if let Some(expression) = typedef.type_expression {
                                self.bind(expression);
                            }
                        }
                        // §269: `@import { Foo } from "./m"` declares each
                        // binding as an alias in FILE locals — the same
                        // (ALIAS, Locals) classification the written form's
                        // nodes get, filed through `declare_jsdoc_symbol`
                        // because this loop runs outside the container walk.
                        // Upstream reaches the identical state by reparsing
                        // the tag into a `JSImportDeclaration` and binding
                        // that; this port binds JSDoc directly (the
                        // `@typedef` arm above is the precedent).
                        JSDocTag::JSDocImportTag(import) => {
                            let Some(clause) = import.import_clause else { continue };
                            if let (Some(name), Some(id)) = (clause.name, clause.node_id) {
                                self.declare_jsdoc_symbol(root, name.text, SymbolFlags::ALIAS, id);
                            }
                            match clause.named_bindings {
                                Some(tsr_ast::NamedImportBindings::NamedImports(named)) => {
                                    for specifier in named.elements {
                                        if let (Some(name), Some(id)) =
                                            (specifier.name, specifier.node_id)
                                        {
                                            self.declare_jsdoc_symbol(
                                                root,
                                                name.text,
                                                SymbolFlags::ALIAS,
                                                id,
                                            );
                                        }
                                    }
                                }
                                // `@import * as ns` deliberately does NOT
                                // bind: a namespace alias makes `ns.Foo`
                                // resolvable, and resolvable-but-unnameable is
                                // the §219 qualification hazard — the first
                                // draft turned `importTag2`'s gap into a wrong
                                // line exactly that way. `bd tsr-e2u`'s fence.
                                Some(tsr_ast::NamedImportBindings::NamespaceImport(_)) | None => {}
                            }
                        }
                        JSDocTag::JSDocCallbackTag(callback) => {
                            let name = match callback.name {
                                Some(tsr_ast::JSDocFullName::Identifier(identifier)) => {
                                    identifier.text
                                }
                                _ => continue,
                            };
                            let Some(id) = callback.node_id else { continue };
                            self.declare_jsdoc_symbol(root, name, SymbolFlags::TYPE_ALIAS, id);
                            if let Some(expression) = callback.type_expression {
                                self.bind(tsr_ast::Node::from(expression));
                            }
                        }
                        JSDocTag::JSDocTemplateTag(template) => {
                            // A braced constraint is parsed syntax whose
                            // members upstream binds (`jsdocTemplateTag3`:
                            // `@template {{ a: number }} T`).
                            if let Some(constraint) = template.constraint {
                                self.bind(constraint);
                            }
                            for parameter in template.type_parameters {
                                let Some(name) = parameter.name else { continue };
                                let Some(id) = parameter.node_id else { continue };
                                let scope = template_scope.unwrap_or(root);
                                self.declare_jsdoc_symbol(
                                    scope,
                                    name.text,
                                    SymbolFlags::TYPE_PARAMETER,
                                    id,
                                );
                                // Native reparents a cloned parameter into the
                                // function host. Detached comment annotations use
                                // the same symbol, not a second declaration/cache.
                                if !declares_alias
                                    && scope != root
                                    && let Some(doc) = doc.node_id
                                    && let Some(symbol) =
                                        self.node_symbols[id.index() - self.node_base]
                                {
                                    self.locals.entry(doc).or_default().insert(name.text, symbol);
                                }
                            }
                        }
                        JSDocTag::JSDocParameterOrPropertyTag(property) => {
                            // A braced `@param`/`@property` type carries
                            // members upstream binds (`jsdocTemplateTag6`).
                            if let Some(expression) = property.type_expression {
                                self.bind(tsr_ast::Node::from(expression));
                            }
                            if self.nodes.kind(property.node_id.unwrap_or(NodeId::ZERO))
                                != SyntaxKind::JSDocPropertyTag
                            {
                                continue;
                            }
                            let name = match property.name {
                                Some(tsr_ast::EntityName::Identifier(identifier)) => {
                                    identifier.text
                                }
                                _ => continue,
                            };
                            let Some(id) = property.node_id else { continue };
                            let symbol = self.symbols.create(name, SymbolFlags::PROPERTY);
                            self.symbols.get_mut(symbol).declarations.push(id);
                            self.node_symbols[id.index() - self.node_base] = Some(symbol);
                        }
                        // `@this {{ n: number }}`, `@returns` and `@type`
                        // braced types are parsed syntax too (`thisTag1`).
                        JSDocTag::JSDocThisTag(this_tag) => {
                            if let Some(expression) = this_tag.type_expression {
                                self.bind(tsr_ast::Node::from(expression));
                            }
                            // Upstream's reparse turns `@this {T}` into a
                            // `this` parameter whose name is missing, so the
                            // `this` reference resolves to a symbol displayed
                            // `(Missing)` declared at the tag (`thisTag1`:
                            // `>this : Symbol((Missing), Decl(a.js, 0, 5))`).
                            if let Some(id) = this_tag.node_id {
                                let symbol =
                                    self.symbols.create(INTERNAL_MISSING, SymbolFlags::VARIABLE);
                                self.symbols.get_mut(symbol).declarations.push(id);
                                self.node_symbols[id.index() - self.node_base] = Some(symbol);
                            }
                        }
                        JSDocTag::JSDocReturnTag(return_tag) => {
                            if let Some(expression) = return_tag.type_expression {
                                self.bind(tsr_ast::Node::from(expression));
                            }
                        }
                        JSDocTag::JSDocTypeTag(type_tag) => {
                            if let Some(expression) = type_tag.type_expression {
                                self.bind(expression);
                            }
                        }
                        // The reparsed `SatisfiesExpression`'s type node
                        // (`reparseHosted`, `parser/reparser.go:396`).
                        JSDocTag::JSDocSatisfiesTag(satisfies) => {
                            if let Some(expression) = satisfies.type_expression {
                                self.bind(tsr_ast::Node::from(expression));
                            }
                        }
                        JSDocTag::JSDocOverloadTag(overload) => {
                            let Some(id) = overload.node_id else { continue };
                            if let Some(symbol) = self.node_symbols[host.index() - self.node_base] {
                                self.symbols.get_mut(symbol).declarations.push(id);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Ported from parser getFunctionLikeHost (`reparser.go:653`).
    /// Only the native host/initializer routes supply function-owned locals.
    fn jsdoc_function_like_host(&self, source: Node<'a>, host: NodeId) -> Option<NodeId> {
        let node = self.nodes.kind(host);
        if matches!(
            node,
            SyntaxKind::FunctionDeclaration
                | SyntaxKind::FunctionExpression
                | SyntaxKind::ArrowFunction
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::Constructor
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor
        ) {
            return Some(host);
        }
        let mut stack = vec![source];
        let mut children = Vec::new();
        let mut host_node = None;
        while let Some(node) = stack.pop() {
            if node.node_id() == Some(host) {
                host_node = Some(node);
                break;
            }
            children.clear();
            push_children(node, &mut children);
            stack.extend(children.iter().copied());
        }
        let expression = match host_node? {
            Node::VariableStatement(statement) => {
                statement.declaration_list?.declarations.first()?.initializer
            }
            Node::PropertyAssignment(property) => property.initializer,
            Node::PropertyDeclaration(property) => property.initializer,
            Node::ExportAssignment(assignment) => assignment.expression,
            Node::ReturnStatement(statement) => statement.expression,
            Node::ExpressionStatement(statement) => statement.expression,
            _ => None,
        }?;
        let mut expression = expression;
        loop {
            expression = match expression {
                Expression::BinaryExpression(binary)
                    if binary.operator_token.is_some_and(|t| t.kind == SyntaxKind::EqualsToken) =>
                {
                    binary.right?
                }
                Expression::SatisfiesExpression(satisfies) => satisfies.expression?,
                _ => break,
            };
        }
        match expression {
            Expression::FunctionExpression(function) => function.node_id,
            Expression::ArrowFunction(function) => function.node_id,
            _ => None,
        }
    }

    /// One JSDoc-declared symbol, merged with an earlier tag of the same name.
    fn declare_jsdoc_symbol(
        &mut self,
        root: NodeId,
        name: &'a str,
        flags: SymbolFlags,
        declaration: NodeId,
    ) {
        let locals = self.locals.entry(root).or_default();
        if let Some(existing) = locals.get(name).copied() {
            let entry = self.symbols.get_mut(existing);
            if entry.flags.intersects(SymbolFlags::TYPE_ALIAS | SymbolFlags::TYPE_PARAMETER) {
                entry.declarations.push(declaration);
                self.node_symbols[declaration.index() - self.node_base] = Some(existing);
                return;
            }
        }
        let symbol = self.symbols.create(name, flags);
        self.symbols.get_mut(symbol).declarations.push(declaration);
        self.node_symbols[declaration.index() - self.node_base] = Some(symbol);
        self.locals.entry(root).or_default().entry(name).or_insert(symbol);
    }

    fn bind_source_file_as_external_module(&mut self, file: NodeId) -> SymbolId {
        // `bindSourceFileAsExternalModule` names the symbol after the path with
        // its extension removed. Upstream wraps that in quotes, the way
        // `declare module "fs"` is spelled; we store the value rather than a
        // spelling, exactly as [`module_name`] does for an ambient module,
        // because quoting would need an owned string where every symbol name
        // here borrows from the source or the file name.
        let symbol = self.symbols.create(self.file_symbol_name, SymbolFlags::VALUE_MODULE);
        let entry = self.symbols.get_mut(symbol);
        entry.declarations.push(file);
        entry.value_declaration = Some(file);
        self.node_symbols[file.index() - self.node_base] = Some(symbol);
        self.module_symbol = Some(symbol);
        symbol
    }

    /// Record that a `CommonJS` export form was seen, and whether it counts.
    ///
    /// Upstream's `setCommonJSModuleIndicator`. An **ES** module ignores
    /// `module.exports` entirely — it is an ordinary assignment to a global
    /// there — so the first test is that the file is not already one.
    fn set_commonjs_module_indicator(&mut self) -> bool {
        if self.is_module {
            return false;
        }
        if !self.commonjs_module {
            self.commonjs_module = true;
            let file = self.file_node;
            self.bind_source_file_as_external_module(file);
        }
        true
    }

    /// `module` and `exports` as locals of a `CommonJS` file
    /// (`declareCommonJSVariable`).
    ///
    /// They are not declared anywhere in the source, which is exactly why the
    /// binder has to: `module.exports = …` refers to them and every reference
    /// would otherwise resolve to nothing. `module` additionally carries an
    /// `exports` member, so `module.exports` resolves through it.
    fn declare_commonjs_variable(&mut self, name: &'a str, file: NodeId) {
        if self.locals.get(&file).is_some_and(|table| table.contains_key(name)) {
            return;
        }
        let symbol = self
            .symbols
            .create(name, SymbolFlags::FUNCTION_SCOPED_VARIABLE | SymbolFlags::MODULE_EXPORTS);
        let entry = self.symbols.get_mut(symbol);
        entry.declarations.push(file);
        entry.value_declaration = Some(file);
        if name == "module" {
            let property =
                self.symbols.create("exports", SymbolFlags::MODULE_EXPORTS | SymbolFlags::PROPERTY);
            let property_entry = self.symbols.get_mut(property);
            property_entry.declarations.push(file);
            property_entry.value_declaration = Some(file);
            property_entry.parent = Some(symbol);
            self.symbols.get_mut(symbol).members.insert("exports", property);
        }
        self.locals.entry(file).or_default().insert(name, symbol);
    }

    /// The declaration an assignment expression makes, if it makes one.
    ///
    /// Upstream's `ast.GetAssignmentDeclarationKind`. In a JavaScript file an
    /// assignment to a property is how declarations are written at all — there is
    /// no `export` and no class field syntax to use instead — so the binder has
    /// to read `module.exports = f`, `exports.x = 1`, and `this.x = 1` as
    /// declarations rather than as writes.
    ///
    /// The last form, the **expando** `f.x = 1`, is deliberately *not* gated on
    /// the file being JavaScript: TypeScript reads a property assigned to a
    /// function as a declaration too, which is what makes
    /// `function f() {}; f.cache = new Map()` type-check.
    fn assignment_declaration_kind(&self, node: Node<'a>) -> Option<JsDeclaration> {
        match node {
            Node::BinaryExpression(binary) => {
                if binary.operator_token.map(|token| token.kind) != Some(SyntaxKind::EqualsToken) {
                    return None;
                }
                let left = binary.left?;
                let target = access_target(left)?;
                if self.in_js_file {
                    if is_module_exports_access(left) {
                        // `module.exports = exports` is a self-assignment.
                        return (!is_exports_identifier(binary.right?))
                            .then_some(JsDeclaration::ModuleExports);
                    }
                    if (is_module_exports_access(target) || is_exports_identifier(target))
                        && access_name(self.arena, left).is_some()
                    {
                        return Some(JsDeclaration::ExportsProperty);
                    }
                    if matches!(target, Expression::KeywordExpression(keyword)
                        if keyword.kind == SyntaxKind::ThisKeyword)
                    {
                        return Some(JsDeclaration::ThisProperty);
                    }
                }
                // `a.b.c = 1` declares only when everything left of the last dot
                // is a *name* — an entity name expression. `f().x = 1` assigns to
                // whatever `f()` returned and declares nothing.
                let names_something = match left {
                    Expression::PropertyAccessExpression(access) => {
                        matches!(access.name, Some(tsr_ast::MemberName::Identifier(_)))
                            && is_entity_name_expression(target, self.in_js_file)
                    }
                    Expression::ElementAccessExpression(_) => {
                        is_entity_name_expression(target, self.in_js_file)
                    }
                    _ => false,
                };
                names_something.then_some(JsDeclaration::Property)
            }
            // `Object.defineProperty(f, "x", { … })` is the same declaration
            // written as a call, and only means it in a JavaScript file.
            Node::CallExpression(_) if self.in_js_file => {
                let target = bindable_object_define_property_target(node)?;
                Some(if is_exports_identifier(target) || is_module_exports_access(target) {
                    JsDeclaration::ObjectDefinePropertyExports
                } else {
                    JsDeclaration::ObjectDefinePropertyValue
                })
            }
            _ => None,
        }
    }

    /// Bind one of the JavaScript assignment declaration forms.
    ///
    /// Returns the symbol it declared, which becomes the assignment's own symbol
    /// so that anything nested under it — the members of an assigned object
    /// literal, say — has an owner.
    fn bind_assignment_declaration(
        &mut self,
        kind: JsDeclaration,
        node: Node<'a>,
        id: NodeId,
    ) -> Option<SymbolId> {
        match kind {
            // `module.exports = x` makes the file a module whose whole value is
            // `x`, which is what `export = x` means in TypeScript — so it is
            // filed under the same name.
            JsDeclaration::ModuleExports => {
                if !self.set_commonjs_module_indicator() {
                    return None;
                }
                let Node::BinaryExpression(binary) = node else { return None };
                let flags = if expression_is_alias(binary.right) {
                    SymbolFlags::ALIAS
                } else {
                    SymbolFlags::PROPERTY
                };
                let module = self.module_symbol?;
                let file = self.file_node;
                // bindModuleExportsAssignment passes no exclusions, even
                // when successive assignments mix property and alias meanings.
                let symbol = self.declare_into_with_excludes(
                    Destination::Exports,
                    file,
                    Some(module),
                    INTERNAL_EXPORT_EQUALS,
                    flags,
                    SymbolFlags::empty(),
                    id,
                );
                self.symbols.get_mut(symbol).value_declaration = Some(id);
                Some(symbol)
            }
            // `exports.x = 1` and `module.exports.x = 1` are named exports.
            JsDeclaration::ExportsProperty => {
                if !self.set_commonjs_module_indicator() {
                    return None;
                }
                let Node::BinaryExpression(binary) = node else { return None };
                let name = access_name(self.arena, binary.left?)?;
                let flags = if expression_is_alias(binary.right) {
                    SymbolFlags::ALIAS
                } else {
                    SymbolFlags::FUNCTION_SCOPED_VARIABLE
                };
                let module = self.module_symbol?;
                let file = self.file_node;
                // bindExportsOrObjectDefineProperty uses variable exclusions
                // for BOTH meanings; deriving AliasExcludes rejects ordinary
                // export initialization followed by an alias/undefined write.
                Some(self.declare_into_with_excludes(
                    Destination::Exports,
                    file,
                    Some(module),
                    name,
                    flags,
                    SymbolFlags::FUNCTION_SCOPED_VARIABLE.excludes(),
                    id,
                ))
            }
            JsDeclaration::ThisProperty => self.bind_this_property_assignment(node, id),

            // `f.x = 1` and `Object.defineProperty(f, "x", …)` name a target the
            // binder has not necessarily reached yet — `f` may be a function
            // declared further down the file — so upstream records them and
            // binds them all once the walk is over (`bindExpandoPropertyAssignment`).
            JsDeclaration::Property | JsDeclaration::ObjectDefinePropertyValue => {
                self.expando_assignments.push(ExpandoAssignment {
                    node,
                    id,
                    container: self.expando_container,
                    block: self.block,
                });
                None
            }
            // The `exports` form is not deferred: its target is the file, which
            // is already known.
            JsDeclaration::ObjectDefinePropertyExports => {
                if !self.set_commonjs_module_indicator() {
                    return None;
                }
                let Node::CallExpression(call) = node else { return None };
                let name = string_or_numeric_text(self.arena, Node::from(call.arguments[1]))?;
                let module = self.module_symbol?;
                let file = self.file_node;
                Some(self.declare_into(
                    Destination::Exports,
                    file,
                    Some(module),
                    name,
                    SymbolFlags::PROPERTY | SymbolFlags::ASSIGNMENT,
                    id,
                ))
            }
        }
    }

    /// Remember where a declaration's expando-carrying initializer is.
    ///
    /// `const f = function () {}` puts the properties of `f.x = 1` on the
    /// *function expression's* symbol, not on the variable's, so the deferred
    /// pass needs to reach the initializer from the declaration. It cannot walk
    /// down the tree to find it, so the id is recorded here — the one place that
    /// has both the node and its id.
    fn record_expando_initializer(&mut self, node: Node<'a>, id: NodeId) {
        let (initializer, has_type) = match node {
            Node::VariableDeclaration(declaration) => {
                // Upstream: a `const`, or any `var`/`let` in a JavaScript file.
                if !(self.in_js_file || self.combined_node_flags(id).contains(NodeFlags::CONST)) {
                    return;
                }
                (declaration.initializer, declaration.r#type.is_some())
            }
            _ => return,
        };
        let Some(initializer) = initializer else { return };
        if !self.is_expando_initializer(initializer, has_type) {
            return;
        }
        if let Some(initializer_id) = Node::from(initializer).node_id() {
            self.expando_initializers.insert(id, initializer_id);
        }
    }

    /// Whether an initializer is the kind of thing properties can be assigned to.
    ///
    /// Upstream's `IsExpandoInitializer`. A function is one in any dialect; a
    /// class expression and an *empty, unannotated* object literal are one only
    /// in a JavaScript file, where `const o = {}; o.x = 1` is how an object type
    /// is built up.
    fn is_expando_initializer(&self, initializer: Expression<'a>, has_type: bool) -> bool {
        match initializer {
            Expression::FunctionExpression(_) | Expression::ArrowFunction(_) => true,
            Expression::ClassExpression(_) => self.in_js_file,
            Expression::ObjectLiteralExpression(literal) => {
                self.in_js_file && literal.properties.is_empty() && !has_type
            }
            _ => false,
        }
    }

    /// Bind every expando assignment now that the whole file has been walked.
    ///
    /// Upstream's `bindDeferredExpandoAssignments`. The scope each was written in
    /// is restored from what was recorded, because the lookup below reads it.
    fn bind_deferred_expando_assignments(&mut self) {
        for index in 0..self.expando_assignments.len() {
            let ExpandoAssignment { node, id, container, block } = self.expando_assignments[index];
            self.container = container;
            self.block = block;
            self.bind_deferred_expando_assignment(node, id);
        }
    }

    /// `f.x = 1` declares `x` on whatever `f` turned out to be.
    ///
    /// Returns nothing; the `Option` is only so that the several ways of finding
    /// out there is nothing to declare can be written as `?`.
    fn bind_deferred_expando_assignment(&mut self, node: Node<'a>, id: NodeId) -> Option<()> {
        let target = match node {
            Node::BinaryExpression(binary) => access_target(binary.left?)?,
            Node::CallExpression(_) => bindable_object_define_property_target(node)?,
            _ => return None,
        };
        // Two containers, not a scope chain: upstream looks in the block scope
        // and then in the function scope, and stops. A name further out is not
        // reached, which is why `f.x = 1` inside a nested block does not extend
        // an `f` declared two scopes up.
        let block = self.block;
        let container = self.container;
        let symbol =
            self.lookup_entity(target, block).or_else(|| self.lookup_entity(target, container));
        let symbol = self.initializer_symbol(symbol)?;

        let name = match node {
            Node::BinaryExpression(binary) => access_name(self.arena, binary.left?),
            Node::CallExpression(call) => {
                string_or_numeric_text(self.arena, Node::from(call.arguments[1]))
            }
            _ => None,
        };
        // A computed name is late-bound; see `lib.rs`.
        let name = name?;

        // "We declare expandos only when there are no non-expando declarations
        // for that name": a real `class F { x }` wins, and the assignment adds
        // nothing rather than merging into it.
        if let Some(existing) = self.symbols.get(symbol).exports.get(name).copied()
            && !self.symbols.get(existing).flags.contains(SymbolFlags::ASSIGNMENT)
        {
            return None;
        }
        let owner_node = self.container;
        let declared = self.declare_into(
            Destination::Exports,
            owner_node,
            Some(symbol),
            name,
            SymbolFlags::PROPERTY | SymbolFlags::ASSIGNMENT,
            id,
        );
        self.node_symbols[id.index() - self.node_base] = Some(declared);
        Some(())
    }

    /// Resolve a name, or a dotted chain of them, against one container.
    ///
    /// Upstream's `lookupEntity`/`lookupName`. Deliberately *not* a scope walk:
    /// it reads the container's own locals and its symbol's exports and stops.
    fn lookup_entity(&self, expression: Expression<'a>, container: NodeId) -> Option<SymbolId> {
        match expression {
            Expression::Identifier(identifier) => self.lookup_name(identifier.text, container),
            // `this.x.y = 1`. Upstream restores only the container and the block
            // scope before the deferred pass, not the `this` container, so this
            // branch resolves against whatever `this` was left over — which is
            // the file. Reproducing that faithfully means declaring nothing.
            Expression::KeywordExpression(keyword) if keyword.kind == SyntaxKind::ThisKeyword => {
                None
            }
            _ => {
                let target = access_target(expression)?;
                let outer = self.lookup_entity(target, container);
                let outer = self.initializer_symbol(outer)?;
                let name = access_name(self.arena, expression)?;
                self.symbols.get(outer).exports.get(name).copied()
            }
        }
    }

    /// One container's own locals, then its symbol's exports (`lookupName`).
    fn lookup_name(&self, name: &str, container: NodeId) -> Option<SymbolId> {
        if let Some(local) = self.locals.get(&container).and_then(|table| table.get(name)) {
            // Upstream returns `local.ExportSymbol` when there is one. Here the
            // two symbols share a declaration and the *export* is what the node
            // was given, so the export is recovered through the node rather than
            // stored a second time on the local.
            let local = *local;
            let exported =
                self.symbols.get(local).declarations.first().and_then(|declaration| {
                    self.node_symbols[declaration.index() - self.node_base]
                });
            return Some(exported.unwrap_or(local));
        }
        let owner = self.node_symbols[container.index() - self.node_base]?;
        self.symbols.get(owner).exports.get(name).copied()
    }

    /// The symbol an expando property should hang off, given what the name
    /// resolved to.
    ///
    /// Upstream's `getInitializerSymbol`. A function declaration is itself the
    /// answer; a `const f = function () {}` is not — the properties belong to the
    /// function expression's symbol, not to the variable's.
    fn initializer_symbol(&self, symbol: Option<SymbolId>) -> Option<SymbolId> {
        let mut symbol = symbol?;
        // An exported const's lookup lands on its export MARKER, which carries
        // no value declaration; the real symbol is behind the link
        // (`propertyAssignmentUseParentType1`'s `export const ignoreJsdoc`).
        if self.symbols.get(symbol).value_declaration.is_none()
            && let Some(real) = self.symbols.get(symbol).export_symbol
        {
            symbol = real;
        }
        let declaration = self.symbols.get(symbol).value_declaration?;
        match self.nodes.kind(declaration) {
            SyntaxKind::FunctionDeclaration => Some(symbol),
            SyntaxKind::ClassDeclaration if self.in_js_file => Some(symbol),
            // The initializer's node id was recorded on the way past, because
            // the tree has no parent-to-child edge to find it with; see
            // [`Self::expando_initializers`].
            SyntaxKind::VariableDeclaration | SyntaxKind::BinaryExpression => self
                .expando_initializers
                .get(&declaration)
                .and_then(|initializer| self.node_symbols[initializer.index() - self.node_base]),
            _ => None,
        }
    }

    /// `this.x = 1` inside a class member declares `x` on the class.
    ///
    /// Upstream's `bindThisPropertyAssignment`. The property is
    /// **replaceable by a method**: a real declaration of the same name wins
    /// outright and this one is dropped, rather than merging, because
    /// `this.m = this.m.bind(this)` in a constructor must not turn the method
    /// into two declarations.
    fn bind_this_property_assignment(&mut self, node: Node<'a>, id: NodeId) -> Option<SymbolId> {
        // Only a class member's `this` names a class. Upstream additionally
        // handles a *constructor function* — `function C() { this.x = 1 }` — and
        // marks that path `!!!`, unimplemented, so neither does this.
        if !matches!(
            self.nodes.kind(self.this_container),
            SyntaxKind::Constructor
                | SyntaxKind::PropertyDeclaration
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor
                | SyntaxKind::ClassStaticBlockDeclaration
        ) {
            return None;
        }
        let Node::BinaryExpression(binary) = node else { return None };
        let left = binary.left?;
        // A private name is not a property of the class in this sense.
        if matches!(left, Expression::PropertyAccessExpression(access)
            if matches!(access.name, Some(tsr_ast::MemberName::PrivateIdentifier(_))))
        {
            return None;
        }
        // A computed `this[k] = 1` is late-bound; see `lib.rs`.
        let name = access_name(self.arena, left)?;
        let owner = self.owner?;

        // `getThisClassAndSymbolTable` (`binder.go:1137`): a static this
        // container (a static block or static member) files the property in
        // the class's exports, beside `static x = 1`; every other member files
        // it in the instance members.
        let this_container = self.this_container;
        let is_static_container = self
            .ancestors
            .iter()
            .rev()
            .find(|(id, _)| *id == this_container)
            .is_some_and(|(_, container)| is_static(*container));
        if let Some(existing) =
            this_property_table(self.symbols.get_mut(owner), is_static_container).get(name).copied()
        {
            let flags = self.symbols.get(existing).flags;
            if !flags.contains(SymbolFlags::REPLACEABLE_BY_METHOD) {
                // A real property or method of this name already exists. Upstream
                // returns it *without* adding this assignment as a declaration.
                return Some(existing);
            }
            let entry = self.symbols.get_mut(existing);
            entry.flags |= SymbolFlags::PROPERTY | SymbolFlags::ASSIGNMENT;
            entry.declarations.push(id);
            return Some(existing);
        }

        let symbol = self.symbols.create(
            name,
            SymbolFlags::PROPERTY | SymbolFlags::ASSIGNMENT | SymbolFlags::REPLACEABLE_BY_METHOD,
        );
        let entry = self.symbols.get_mut(symbol);
        entry.declarations.push(id);
        entry.value_declaration = Some(id);
        entry.parent = Some(owner);
        this_property_table(self.symbols.get_mut(owner), is_static_container).insert(name, symbol);
        Some(symbol)
    }

    /// The declared name, threading the file text the JSX case needs.
    fn declaration_name(&self, node: Node<'a>) -> Option<&'a str> {
        declaration_name(self.arena, node, self.nodes, self.source)
    }

    /// Create a symbol for `node` if it declares one.
    fn declare(&mut self, node: Node<'a>, id: NodeId) -> Option<SymbolId> {
        self.default_export_declaration = (Self::is_default_export(node)
            || matches!(node, Node::ExportAssignment(assignment) if !assignment.is_export_equals))
        .then_some(id);
        // **Here, not at the one `declare_into` call that used to record it.**
        // `GetNameOfDeclaration` (`binder.go:245`) is what every redeclaration
        // diagnostic is positioned at, and `declare` reaches `declare_into`
        // through eight call sites — the module-member path among them. Only one
        // recorded the name, so a duplicate `class` inside a `namespace`
        // reported on the `class` keyword while the same class at file scope
        // reported on its name. `extragap.rs` reads that as **47 displaced
        // TS2300 lines**, which is what a position bug looks like from the
        // outside: the same code most-missing and most-extra in the same files.
        if let Some(name_node) = name_node_of(node) {
            self.name_nodes.insert(id, name_node);
        }
        // bindCallExpression (native 5b1047d1 binder.go:920): syntactic
        // require calls mark JS CommonJS modules even when the binding is
        // shadowed. Semantic require-call recognition belongs to the checker.
        if self.in_js_file
            && !self.commonjs_module
            && let Node::CallExpression(call) = node
            && call.arguments.len() == 1
            && matches!(call.expression, Some(Expression::Identifier(name)) if name.text == "require")
        {
            self.set_commonjs_module_indicator();
        }
        // In a JavaScript file an assignment to a property can *be* a
        // declaration. Checked before anything else, because the node kinds
        // involved — a binary expression — declare nothing otherwise.
        if let Some(kind) = self.assignment_declaration_kind(node) {
            let symbol = self.bind_assignment_declaration(kind, node, id);
            if let Some(symbol) = symbol {
                self.node_symbols[id.index() - self.node_base] = Some(symbol);
            }
            return symbol;
        }
        // A bare `exports = …` in a JavaScript file is a CommonJS indicator
        // even though it declares no member — `exports = require('./mod')`
        // makes the file a module with `exports` as a local
        // (`exportNestedNamespaces2`: `Symbol(exports, Decl(first.js, 0, 0))`).
        if self.in_js_file
            && let Node::BinaryExpression(binary) = node
            && binary.operator_token.map(|token| token.kind) == Some(SyntaxKind::EqualsToken)
            && binary.left.is_some_and(is_exports_identifier)
        {
            self.set_commonjs_module_indicator();
        }

        // An object literal, a type literal, or an unnamed class expression has
        // members but no name to file them under. Upstream gives each an
        // *anonymous* symbol (`bindAnonymousDeclaration`) that goes into no
        // table; without one, the members are added to whatever symbol happened
        // to be the enclosing owner — so `interface I { salt: number }` followed
        // by `{ salt: 2 }` put the object literal's property on `I`.
        if let Some((internal, flags)) = anonymous_declaration(node) {
            let symbol = self.symbols.create(internal, flags);
            self.symbols.get_mut(symbol).declarations.push(id);
            // bindAnonymousDeclaration -> addDeclarationToSymbol ->
            // SetValueDeclaration (5b1047d1, binder.go:1230/2513). The named
            // expression's constant self-reference uses this original owner.
            if matches!(node, Node::FunctionExpression(_)) {
                self.symbols.get_mut(symbol).value_declaration = Some(id);
            }
            self.node_symbols[id.index() - self.node_base] = Some(symbol);
            return Some(symbol);
        }

        // bindTypeParameter files an infer declaration in the conditional
        // whose extends subtree contains it, independently of b.container.
        if matches!(node, Node::TypeParameterDeclaration(_))
            && matches!(self.ancestors.last(), Some((_, Node::InferTypeNode(_))))
        {
            let name = self.declaration_name(node).unwrap_or(INTERNAL_MISSING);
            let container = self.ancestors.windows(2).rev().find_map(|pair| {
                let (id, Node::ConditionalTypeNode(conditional)) = pair[0] else {
                    return None;
                };
                (conditional.extends_type.and_then(|node| Node::from(node).node_id())
                    == Some(pair[1].0))
                .then_some(id)
            });
            let symbol = if let Some(container) = container {
                self.declare_into(
                    Destination::Locals,
                    container,
                    None,
                    name,
                    SymbolFlags::TYPE_PARAMETER,
                    id,
                )
            } else {
                let symbol = self.symbols.create(name, SymbolFlags::TYPE_PARAMETER);
                self.symbols.get_mut(symbol).declarations.push(id);
                symbol
            };
            self.node_symbols[id.index() - self.node_base] = Some(symbol);
            return Some(symbol);
        }

        let (flags, destination) = self.classify(node, id)?;

        // A *late-bound* name — `[Symbol.iterator]`, `[k]`, `[foo()]` — is
        // whatever the expression evaluates to, which needs the checker. Upstream
        // still creates a symbol: an anonymous one called `__computed`, in no
        // symbol table, parented to the container so that the checker's late
        // binding has something to attach a resolved name to
        // (`bindPropertyOrMethodOrAccessor` -> `bindAnonymousDeclaration`).
        // Without it the declaration has no symbol at all and its members and
        // flow have nowhere to go.
        if let Some(computed) = dynamic_name(self.arena, node) {
            let symbol = self.symbols.create(INTERNAL_COMPUTED, flags);
            self.symbols.get_mut(symbol).declarations.push(id);
            // Parented to the container unconditionally, as
            // `bindAnonymousDeclaration` does — an interface's or type
            // literal's late-bound member has the same need as a class's.
            self.symbols.get_mut(symbol).parent = self.owner;
            if flags.intersects(SymbolFlags::VALUE) {
                self.symbols.get_mut(symbol).value_declaration = Some(id);
            }
            self.node_symbols[id.index() - self.node_base] = Some(symbol);
            if let Some(computed_id) = computed.node_id {
                self.computed_names.insert(id, computed_id);
            }
            return Some(symbol);
        }

        let name = self.declaration_name(node);

        if destination == Destination::Locals
            && self.owner.is_some()
            && self.is_exported_from_container(node)
        {
            let container = self.container;
            // An *export* of a default declaration is always called `default`,
            // whatever the source called the declaration itself
            // (`declareSymbolEx`). This is what merges the four declarations of
            // `export default function f` / `export default interface F` into one
            // symbol, and it is why the local half below keeps the written name.
            let is_default = Self::is_default_export(node);
            let export_name = if is_default { INTERNAL_DEFAULT } else { name? };

            // An alias gets **one** symbol, not two: `export { x }` and
            // `export import y = …` name something that already has a local, so
            // a shadow local would be a second declaration of a name the source
            // wrote once.
            if flags.contains(SymbolFlags::ALIAS) {
                let exported = self.declare_into(
                    Destination::Exports,
                    container,
                    self.owner,
                    export_name,
                    flags,
                    id,
                );
                self.node_symbols[id.index() - self.node_base] = Some(exported);
                return Some(exported);
            }

            // An exported member otherwise gets **two** symbols, as upstream's
            // `declareModuleMember` explains at length: a local, and an export on
            // the container's own symbol. Both are needed — locals and exports of
            // the same name are mutually exclusive within a container, so the
            // local is what makes a duplicate a duplicate, while the export is
            // what makes the member reachable as `M.X`. Creating only the export
            // loses every unqualified reference to it, which is a 223-case
            // regression measured 2026-08-04.
            //
            // The exception is an *unnamed* default (`export default class {}`):
            // there is no name to declare a local under, and upstream says so in
            // as many words — "No local symbol for an unnamed default!".
            let local = if let Some(name) = name {
                let local_owner = self.locals_owner(flags);
                let export_value = if flags.intersects(SymbolFlags::VALUE) {
                    SymbolFlags::EXPORT_VALUE
                } else {
                    SymbolFlags::empty()
                };
                // `declareSymbol(GetLocals(container), nil, node, exportKind,
                // symbolExcludes)` (`binder.go:406`): the local carries only
                // `ExportValue`, but it is tested with the *declaration's*
                // excludes, so `class Box {}` then `export type Box;` collides
                // in the locals table. Deriving excludes from `exportKind`
                // made the local half collide with nothing.
                // `docs/parity/notes/decls.md` §15.
                Some(self.declare_into_with_excludes(
                    Destination::Locals,
                    local_owner,
                    self.owner,
                    name,
                    export_value,
                    flags.excludes(),
                    id,
                ))
            } else {
                None
            };

            let exported = self.declare_into(
                Destination::Exports,
                container,
                self.owner,
                export_name,
                flags,
                id,
            );
            // `local.ExportSymbol = b.declareSymbol(ast.GetExports(...))`
            // (`internal/binder/binder.go:407`), the line immediately after
            // upstream declares the local. The marker is the only symbol a
            // reference inside the container can reach, so without this link the
            // real symbol — and therefore the type — is reachable only by
            // guessing which table it went into. The binder is the one place
            // that does not have to guess.
            if let Some(local) = local {
                self.symbols.get_mut(local).export_symbol = Some(exported);
            }
            // The export symbol is the node's symbol, so a class's members land
            // on the thing `M.C` names rather than on the shadow local.
            self.node_symbols[id.index() - self.node_base] = Some(exported);
            return Some(exported);
        }

        // A declaration whose name the parser could not read still gets a symbol
        // — `declareSymbolEx` creates one called `__missing`, in no symbol table.
        // Without it the declaration has none at all, and anything nested inside
        // it is filed on whatever container happened to be enclosing.
        let Some(name) = name else {
            let symbol = self.symbols.create(INTERNAL_MISSING, SymbolFlags::empty());
            self.symbols.get_mut(symbol).declarations.push(id);
            self.node_symbols[id.index() - self.node_base] = Some(symbol);
            return Some(symbol);
        };
        let table_owner = match destination {
            Destination::Locals => self.locals_owner(flags),
            Destination::Members | Destination::Exports | Destination::GlobalExports => {
                self.container
            }
        };

        // **Before `declare_into`**, which is where a redeclaration diagnostic is
        // raised and therefore where this node's name span is first needed.
        // Recorded after the call, it was always absent for the declaration being
        // bound and present only for the earlier ones — so every diagnostic landed
        // on the `enum`/`class` keyword instead of the name.
        if let Some(name_node) = name_node_of(node) {
            self.name_nodes.insert(id, name_node);
        }
        // `bindParameter` / `bindBindingElement`'s `ParameterExcludes`
        // (`binder.go:1182`, `:1200`) — see `declare_into_with_excludes`.
        let excludes = if self.is_part_of_parameter_declaration(node) {
            SymbolFlags::VALUE
        } else if matches!(node, Node::ExportAssignment(_)) {
            // **`SymbolFlagsAll`** — `bindExportAssignment` (`binder.go:864`).
            // Not a shortcut upstream took but the specification: *"If there is
            // an `export default x;` alias declaration, can't `export default`
            // anything else."* Any existing symbol under that name collides,
            // whatever it is, which is what TS2528 means. Deriving excludes
            // from the flags gave `PROPERTY_EXCLUDES` or `ALIAS_EXCLUDES`,
            // neither of which intersects the other, so a second `export
            // default` merged silently and the ported TS2528 below never fired.
            // §957.
            SymbolFlags::all()
        } else {
            flags.excludes()
        };
        let symbol = self.declare_into_with_excludes(
            destination,
            table_owner,
            self.owner,
            name,
            flags,
            excludes,
            id,
        );
        self.node_symbols[id.index() - self.node_base] = Some(symbol);
        self.record_expando_initializer(node, id);
        // `{ ['a']: 1 }` declares `a` statically, but it was still *written* as a
        // computed name, and upstream prints it back the way it was written.
        if let Some(computed) = computed_property_name(node)
            && let Some(computed_id) = computed.node_id
        {
            self.computed_names.insert(id, computed_id);
        }

        // `constructor(public x: T)` declares twice: a parameter in the
        // constructor's scope and a property on the class. Upstream declares the
        // parameter first and lets the property win as the node's symbol, which
        // is what the checker wants when it asks what `x` is from outside.
        if self.is_parameter_property(node) {
            let property = self.declare_into(
                Destination::Members,
                table_owner,
                self.owner,
                name,
                SymbolFlags::PROPERTY,
                id,
            );
            self.node_symbols[id.index() - self.node_base] = Some(property);
            return Some(property);
        }
        Some(symbol)
    }

    /// `IsPartOfParameterDeclaration` (`ast/utilities.go`): this node is a
    /// parameter, or a binding element inside one — `function foo([a, a]) {}`
    /// is the second form and upstream reports it for the same reason.
    fn is_part_of_parameter_declaration(&self, node: Node<'a>) -> bool {
        if matches!(node, Node::ParameterDeclaration(_)) {
            return true;
        }
        if !matches!(node, Node::BindingElement(_)) {
            return false;
        }
        // Walk out through the pattern; the first ancestor that is neither a
        // binding element nor a pattern decides. Anything other than a
        // parameter means the pattern belongs to a variable declaration or a
        // `catch`, which keep `FunctionScopedVariableExcludes`.
        self.ancestors
            .iter()
            .rev()
            .find(|(_, ancestor)| {
                !matches!(ancestor, Node::BindingElement(_) | Node::BindingPattern(_))
            })
            .is_some_and(|(_, ancestor)| matches!(ancestor, Node::ParameterDeclaration(_)))
    }

    /// The span a redeclaration diagnostic is anchored on.
    ///
    /// The declaration's name, falling back to the declaration itself — upstream's
    /// `declarationName == nil` branch (`binder.go:245`).
    fn declaration_name_span(&self, declaration: NodeId) -> tsr_core::Span {
        self.name_nodes
            .get(&declaration)
            .map_or_else(|| self.nodes.span(declaration), |name| self.nodes.span(*name))
    }

    /// Add `name` to the appropriate table, merging with an existing symbol where
    /// TypeScript allows it and reporting a duplicate where it does not.
    fn declare_into(
        &mut self,
        destination: Destination,
        table_owner: NodeId,
        symbol_owner: Option<SymbolId>,
        name: &'a str,
        flags: SymbolFlags,
        declaration: NodeId,
    ) -> SymbolId {
        let excludes = flags.excludes();
        self.declare_into_with_excludes(
            destination,
            table_owner,
            symbol_owner,
            name,
            flags,
            excludes,
            declaration,
        )
    }

    /// `declareSymbol` with `excludes` passed rather than derived.
    ///
    /// **Excludes is not a function of includes.** Upstream's `declareSymbol`
    /// (`binder.go:202`) takes `includes` and `excludes` as two arguments, and
    /// `bindParameter` (`binder.go:1200`) passes `SymbolFlagsFunctionScopedVariable`
    /// for the first and `SymbolFlagsParameterExcludes` — plain `Value` — for the
    /// second. Deriving excludes from the flags gives a parameter
    /// `FunctionScopedVariableExcludes`, which deliberately does *not* collide
    /// with another function-scoped variable because `var x; var x;` is legal —
    /// so `function bar(a, a) {}` reported nothing. Upstream's own comment at
    /// `binder.go:1176` says this is exactly what the distinction is for.
    /// See `checker-notes-diag2.md` §93.
    #[allow(clippy::too_many_arguments)]
    fn declare_into_with_excludes(
        &mut self,
        destination: Destination,
        table_owner: NodeId,
        symbol_owner: Option<SymbolId>,
        name: &'a str,
        flags: SymbolFlags,
        excludes: SymbolFlags,
        declaration: NodeId,
    ) -> SymbolId {
        let existing =
            match destination {
                Destination::Locals => {
                    self.locals.get(&table_owner).and_then(|table| table.get(name).copied())
                }
                Destination::Members => symbol_owner
                    .and_then(|owner| self.symbols.get(owner).members.get(name).copied()),
                Destination::Exports => symbol_owner
                    .and_then(|owner| self.symbols.get(owner).exports.get(name).copied()),
                Destination::GlobalExports => self.global_exports.get(name).copied(),
            };

        let symbol = if let Some(existing) = existing {
            let existing_flags = self.symbols.get(existing).flags;
            if excludes.intersects(existing_flags)
                && existing_flags.contains(SymbolFlags::REPLACEABLE_BY_METHOD)
            {
                // `declareSymbolEx` (`binder.go:203`): a JavaScript
                // constructor-declared property loses to a prototype member of
                // the same name — the table takes a fresh symbol and the
                // `this.m = this.m.bind(this)` property is discarded without a
                // diagnostic.
                let created = self.symbols.create(name, SymbolFlags::empty());
                match destination {
                    Destination::Locals => {
                        self.locals.entry(table_owner).or_default().insert(name, created);
                    }
                    Destination::Members => {
                        if let Some(owner) = symbol_owner {
                            self.symbols.get_mut(owner).members.insert(name, created);
                        }
                    }
                    Destination::Exports => {
                        if let Some(owner) = symbol_owner {
                            self.symbols.get_mut(owner).exports.insert(name, created);
                        }
                    }
                    Destination::GlobalExports => {
                        self.global_exports.insert(name, created);
                    }
                }
                if !matches!(destination, Destination::Locals) {
                    self.symbols.get_mut(created).parent = symbol_owner;
                }
                self.symbols.get_mut(created).flags |= flags;
                created
            } else if excludes.intersects(existing_flags) {
                // Ported from `binder.declareSymbol` (`internal/binder/binder.go:202`),
                // whose message selection this originally collapsed into one
                // diagnostic at one position. Three separate defects, all of which
                // the `diagnostics` conformance suite reports as a *false positive*
                // because a diagnostic at the wrong position or with the wrong code
                // is both an unexpected one and a missing one:
                //
                // 1. An enum in the conflict is `TS2567`, not `TS2300`, and carries
                //    no name argument (`binder.go:220`). `class c4 {} enum c4 {}`
                //    was the whole of `compiler/augmentedTypesClass`'s divergence.
                // 2. A block-scoped existing declaration is `TS2451`
                //    (`binder.go:214`).
                // 3. The position is the declaration's **name**, not the
                //    declaration (`GetNameOfDeclaration`, `binder.go:245`). `class
                //    c1 {}` reports at the `c1`, not at the `class`.
                //
                // And upstream reports on *every* declaration involved
                // (`binder.go:259`), which the previous comment here claimed while
                // the code reported only the latest.
                let enum_conflict = existing_flags.intersects(SymbolFlags::ENUM)
                    || flags.intersects(SymbolFlags::ENUM);
                let mut message = if enum_conflict {
                    &messages::ENUM_DECLARATIONS_CAN_ONLY_MERGE_WITH_NAMESPACE_OR_OTHER_ENUM_DECLARATIONS
                } else if existing_flags.intersects(SymbolFlags::BLOCK_SCOPED_VARIABLE) {
                    &messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE_0
                } else {
                    &messages::DUPLICATE_IDENTIFIER_0
                };
                // Upstream tracks "does the message take the name as an argument"
                // separately from which message it is (`messageNeedsName`,
                // `binder.go:222`); it was previously derived from `enum_conflict`,
                // which worked only while the enum message was the sole nameless one.
                let mut message_needs_name = !enum_conflict;

                // A second `export default` in one module is
                // `A_module_cannot_have_multiple_default_exports` (TS2528), not a
                // duplicate identifier — upstream's `binder.go:224-244`, checked
                // after the enum case and overriding it.
                //
                // Upstream reaches this through two conditions: `isDefaultExport`,
                // and separately an `ExportAssignment` that is not `export =` (so
                // that `export default { }` after `export default class` is caught,
                // since that form carries no `default` *modifier* to test). Both are
                // properties of the *declaration*, recorded by `declare` in
                // `default_export_declaration`. This used to test
                // `name == INTERNAL_DEFAULT` on the claim that nothing else is
                // filed under that name; parser recovery spells it —
                // `import { default } from "m"` binds a local named `default`,
                // and upstream reports TS2300 there, not TS2528
                // (`es6ImportNamedImportIdentifiersParsing`,
                // `docs/parity/notes/decls.md` §14).
                //
                // The related-info chain upstream attaches (`binder.go:265-275`:
                // `Another_export_default_is_here`, `and_here`,
                // `The_first_export_default_is_here`) is **not** ported: `Diagnostic`
                // carries no related information yet, and the `diagnostics` suite
                // compares codes and positions only. Tracked in bd tsr-y4u.23.
                if self.default_export_declaration == Some(declaration)
                    && !self.symbols.get(existing).declarations.is_empty()
                {
                    message = &messages::A_MODULE_CANNOT_HAVE_MULTIPLE_DEFAULT_EXPORTS;
                    message_needs_name = false;
                }

                let report = |binder: &mut Self, at: NodeId| {
                    let span = binder.declaration_name_span(at);
                    binder.diagnostics.push(if message_needs_name {
                        Diagnostic::with_args(message, span, [name.to_string()])
                    } else {
                        Diagnostic::new(message, span)
                    });
                };
                let previous: Vec<NodeId> = self.symbols.get(existing).declarations.to_vec();
                report(self, declaration);
                for earlier in previous {
                    report(self, earlier);
                }
                // **Upstream does NOT merge, and the comment that stood here
                // saying it does was wrong for two sessions.** `binder.go:286`,
                // the last statement of the conflict branch, is
                // `symbol = b.newSymbol(SymbolFlagsNone, name)` — a fresh symbol
                // that is deliberately *not* put in the symbol table. The table
                // keeps the first declaration's symbol with its original flags;
                // the conflicting declaration gets a private one carrying only
                // itself.
                //
                // It is observable in the `.symbols` baseline, which is where it
                // was found: `compiler/varAndFunctionShareName` is `var myFn;`
                // followed by `function myFn(): any {}` and records **two**
                // symbols, one declaration each. And in `.types`, where the two
                // names then print `any` and `() => any` — different answers for
                // the same spelling, which one merged symbol cannot produce.
                // §196.
                //
                // Upstream's one exception first: a get/set accessor conflicting
                // with a non-accessor or the other kind marks the EXISTING symbol
                // a full accessor, so every later declaration conflicts too
                // (`binder.go:283-287`).
                if existing_flags.intersects(SymbolFlags::ACCESSOR)
                    && (existing_flags & SymbolFlags::ACCESSOR) != (flags & SymbolFlags::ACCESSOR)
                {
                    self.symbols.get_mut(existing).flags |= SymbolFlags::ACCESSOR;
                }
                let conflicted = self.symbols.create(name, flags);
                if !matches!(destination, Destination::Locals) {
                    self.symbols.get_mut(conflicted).parent = symbol_owner;
                }
                conflicted
            } else {
                self.symbols.get_mut(existing).flags |= flags;
                existing
            }
        } else {
            let created = self.symbols.create(name, flags);
            match destination {
                Destination::Locals => {
                    self.locals.entry(table_owner).or_default().insert(name, created);
                    // Upstream's `Symbol.Parent` is the container symbol even
                    // for an unexported local of a namespace body — it is what
                    // lets `symbolToString` print `dom.JSX` for a `namespace
                    // JSX` without `export` inside `export namespace dom`
                    // (`inlineJsxFactoryDeclarationsLocalTypes`). Only a
                    // namespace-owned locals table qualifies; a function's
                    // locals have no printable container.
                    if self.in_declaration_file
                        && let Some(owner) = self.owner
                        && self.symbols.get(owner).flags.intersects(SymbolFlags::MODULE)
                        && self.locals_owner(flags) == table_owner
                        && table_owner != self.file_node
                    {
                        self.symbols.get_mut(created).parent = Some(owner);
                    }
                }
                Destination::Members => {
                    if let Some(owner) = symbol_owner {
                        self.symbols.get_mut(owner).members.insert(name, created);
                        self.symbols.get_mut(created).parent = Some(owner);
                    }
                }
                Destination::Exports => {
                    if let Some(owner) = symbol_owner {
                        self.symbols.get_mut(owner).exports.insert(name, created);
                        self.symbols.get_mut(created).parent = Some(owner);
                    }
                }
                Destination::GlobalExports => {
                    self.global_exports.insert(name, created);
                    self.symbols.get_mut(created).parent = symbol_owner;
                }
            }
            created
        };

        let new_is_assignment = matches!(
            self.nodes.kind(declaration),
            SyntaxKind::BinaryExpression | SyntaxKind::CallExpression
        );
        // `SetValueDeclaration`'s ambient guard (`ast/utilities.go`): an
        // ambient TypeScript declaration does not displace a non-ambient one.
        let ambient_ts = !self.in_js_file && (self.in_declaration_file || self.in_ambient_module);
        let entry = self.symbols.get_mut(symbol);
        entry.declarations.push(declaration);
        if flags.intersects(SymbolFlags::VALUE) {
            match entry.value_declaration {
                None => entry.value_declaration = Some(declaration),
                // `SetValueDeclaration`: other kinds of value declarations
                // take precedence over assignment declarations. Its
                // effective-module half is not ported here.
                Some(current)
                    if !ambient_ts
                        && !new_is_assignment
                        && matches!(
                            self.nodes.kind(current),
                            SyntaxKind::BinaryExpression | SyntaxKind::CallExpression
                        ) =>
                {
                    entry.value_declaration = Some(declaration);
                }
                Some(_) => {}
            }
        }
        symbol
    }
}

/// The class table a this-property files into: exports for a static
/// this-container, members otherwise (`getThisClassAndSymbolTable`).
fn this_property_table<'s, 'a>(
    symbol: &'s mut crate::Symbol<'a>,
    exports: bool,
) -> &'s mut SymbolTable<'a> {
    if exports { symbol.exports.initialize() } else { symbol.members.initialize() }
}

/// `isAssignmentDeclaration` (`binder/binder.go:2767`), on a declaration's
/// kind.
fn is_assignment_declaration_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::BinaryExpression
            | SyntaxKind::PropertyAccessExpression
            | SyntaxKind::ElementAccessExpression
            | SyntaxKind::Identifier
            | SyntaxKind::CallExpression
    )
}

/// Whether `child` is the condition of `parent` (`isStatementCondition`).
fn is_statement_condition(child: NodeId, parent: Node<'_>) -> bool {
    let condition = match parent {
        Node::IfStatement(_) | Node::WhileStatement(_) | Node::DoStatement(_) => {
            expression_of(parent).map(Node::from)
        }
        Node::ForStatement(statement) => statement.condition.map(Node::from),
        Node::ConditionalExpression(expression) => expression.condition.map(Node::from),
        _ => return false,
    };
    condition.and_then(|node| node.node_id()) == Some(child)
}

/// Which of a label's two targets a `break` or `continue` wants.
#[derive(Clone, Copy)]
enum LabelTarget {
    Break,
    Continue,
}

/// `true` or `false` written literally, for the dead-branch check.
fn literal_boolean(node: Node<'_>) -> Option<bool> {
    match node {
        Node::KeywordExpression(keyword) => match keyword.kind {
            SyntaxKind::TrueKeyword => Some(true),
            SyntaxKind::FalseKeyword => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// Upstream: `ast.IsDestructuringAssignment`.
fn is_destructuring_assignment(node: Node<'_>) -> bool {
    let Node::BinaryExpression(binary) = node else { return false };
    binary.operator_token.map(|token| token.kind) == Some(SyntaxKind::EqualsToken)
        && binary.left.is_some_and(|left| is_left_hand_side_expression(Node::from(left)))
        && matches!(
            binary.left,
            Some(Expression::ObjectLiteralExpression(_) | Expression::ArrayLiteralExpression(_))
        )
}

/// Whether a function-like node has a body to fall off the end of.
fn has_present_body(node: Node<'_>) -> bool {
    match node {
        Node::FunctionDeclaration(n) => n.body.is_some(),
        Node::FunctionExpression(n) => n.body.is_some(),
        Node::ArrowFunction(n) => n.body.is_some(),
        Node::MethodDeclaration(n) => n.body.is_some(),
        Node::ConstructorDeclaration(n) => n.body.is_some(),
        Node::GetAccessorDeclaration(n) => n.body.is_some(),
        Node::SetAccessorDeclaration(n) => n.body.is_some(),
        Node::ClassStaticBlockDeclaration(n) => n.body.is_some(),
        _ => false,
    }
}

/// Upstream: `ast.HasSyntacticModifier(node, ModifierFlagsAsync)`.
fn has_async_modifier(node: Node<'_>) -> bool {
    let modifiers = match node {
        Node::FunctionExpression(n) => n.modifiers,
        Node::ArrowFunction(n) => n.modifiers,
        _ => return false,
    };
    modifiers.iter().any(|modifier| {
        matches!(modifier, tsr_ast::ModifierLike::Token(token) if token.kind == SyntaxKind::AsyncKeyword)
    })
}

/// The internal name and flags of a declaration that has members but no name.
///
/// Upstream's `bindAnonymousDeclaration` with the `InternalSymbolName*`
/// constants. The symbol goes into no symbol table — nothing can look it up by
/// name — but it owns the members declared inside it, which is the entire point.
fn anonymous_declaration(node: Node<'_>) -> Option<(&str, SymbolFlags)> {
    Some(match node {
        Node::ObjectLiteralExpression(_) => (INTERNAL_OBJECT, SymbolFlags::OBJECT_LITERAL),
        // One arm because `match_same_arms` is a workspace gate, and two
        // unrelated reasons for the same body:
        //
        // - a **type literal** or **mapped type** is upstream's
        //   `bindAnonymousDeclaration` with `InternalSymbolNameType`;
        // - a **function or constructor type node** is
        //   `bindFunctionOrConstructorType` (`binder.go:985`), and **a
        //   deliberate reduction of it** — see `docs/architecture/binder.md`.
        //   Upstream binds *two* symbols to that one node: a `Signature` symbol
        //   named `__call`, then a `TypeLiteral` symbol named `__type` whose
        //   members hold the first. `addDeclarationToSymbol` runs second, so
        //   `node.Symbol` — what
        //   `getTypeFromTypeLiteralOrFunctionOrConstructorTypeNode` reads — is
        //   the `__type` one, which is what this creates. Upstream's comment
        //   says the point is to make `(x) => T` indistinguishable from
        //   `{ (x): T }`, and the missing `__call` is exactly what that costs:
        //   the members table is empty, so nothing finds the call signature
        //   *through the type*. What does find it is `getSignaturesOfSymbol`,
        //   because this symbol's `declarations` is the type node itself.
        Node::TypeLiteralNode(_)
        | Node::MappedTypeNode(_)
        | Node::FunctionTypeNode(_)
        | Node::ConstructorTypeNode(_) => (INTERNAL_TYPE, SymbolFlags::TYPE_LITERAL),
        Node::JsxAttributes(_) => (INTERNAL_JSX_ATTRIBUTES, SymbolFlags::OBJECT_LITERAL),
        // A class expression is anonymous whether or not it has a name.
        // `bindClassLikeDeclaration` splits on the *kind*, not on the name
        // (`binder.go:942-951`): a `ClassDeclaration` goes through
        // `bindBlockScopedDeclaration`, a `ClassExpression` always through
        // `bindAnonymousDeclaration`, which takes the written name as the symbol's
        // name and puts the symbol in no table.
        //
        // The comment here used to claim the opposite — that a named class
        // expression declares into the enclosing block "as upstream does". It does
        // not, and the consequence was that `const C9 = class C { }` collided with
        // a `class C` in the same file. The name of a named class expression is
        // visible only *inside* it, which is the same rule as `FunctionExpression`
        // immediately below, and that one was already right.
        Node::ClassExpression(class) => {
            (class.name.map_or(INTERNAL_CLASS, |name| name.text), SymbolFlags::CLASS)
        }
        // `var obj = function f() {}` declares `f`, but *nowhere*: the name is
        // visible only inside the function, so upstream gives it a symbol in no
        // symbol table (`bindFunctionExpression` → `bindAnonymousDeclaration`).
        // That is why it reads as a bare `f` in the baselines rather than as a
        // member of anything.
        Node::FunctionExpression(function) => {
            (function.name.map_or(INTERNAL_FUNCTION, |name| name.text), SymbolFlags::FUNCTION)
        }
        Node::ArrowFunction(_) => (INTERNAL_FUNCTION, SymbolFlags::FUNCTION),
        _ => return None,
    })
}

/// Upstream's `ast.InternalSymbolNameObject` and friends.
///
/// The leading `__` is upstream's marker for a name no source can spell, and the
/// conformance harness reads it to decide whether a member is reachable by a
/// qualified name at all.
pub(crate) const INTERNAL_OBJECT: &str = "__object";
pub(crate) const INTERNAL_TYPE: &str = "__type";
pub(crate) const INTERNAL_CLASS: &str = "__class";
pub(crate) const INTERNAL_JSX_ATTRIBUTES: &str = "__jsxAttributes";
pub(crate) const INTERNAL_FUNCTION: &str = "__function";
/// The name every index signature in a container shares, so that two of them
/// merge into one symbol — which is what upstream's `__index` is for.
pub(crate) const INTERNAL_INDEX: &str = "__index";

/// The name every late-bound member shares until the checker resolves it
/// (`ast.InternalSymbolNameComputed`). Unlike the other internal names it is
/// deliberately *not* a key in any symbol table: two `[k]`s in one class are two
/// symbols, and which — if either — ends up reachable is the checker's answer.
pub(crate) const INTERNAL_COMPUTED: &str = "__computed";
/// The name a declaration gets when the parser could not read one
/// (`ast.InternalSymbolNameMissing`). Like `__computed`, it is in no symbol
/// table: two unreadable names are two declarations, not one symbol.
pub(crate) const INTERNAL_MISSING: &str = "__missing";
/// The name every `export default` in a file shares.
///
/// Upstream's `ast.InternalSymbolNameDefault`. Unlike its neighbours it has no
/// `__` prefix, because `import x from "m"` really does look `default` up by that
/// name — it is well-known rather than unspellable. Sharing it is what merges
/// `export default function f` with `export default interface F` into one symbol.
pub(crate) const INTERNAL_DEFAULT: &str = "default";
/// The name `export = x` files the module's whole value under
/// (`ast.InternalSymbolNameExportEquals`).
pub(crate) const INTERNAL_EXPORT_EQUALS: &str = "export=";
/// The global object under its own name.
///
/// Not an internal name — it is spellable, and `namespace globalThis { … }` is
/// the supported way to add a property to the global object. Upstream keeps a
/// symbol for it whose export table *is* `c.globals` (`checker.go:962-964`);
/// see [`Binder::merge_into_globals`] for why this port splices instead.
pub(crate) const GLOBAL_THIS: &str = "globalThis";
/// The name every `export * from "m"` in a module is collected under
/// (`ast.InternalSymbolNameExportStar`).
///
/// One symbol per module, carrying **one declaration per star** — the checker
/// walks those declarations to resolve each re-exported module in turn
/// (`getExportsOfModuleWorker`, `checker.go:16148`). Upstream's comment at
/// `bindExportDeclaration` says it outright: *"All export * declarations are
/// collected in an `__export` symbol."*
pub(crate) const INTERNAL_EXPORT_STAR: &str = "__export";

/// The modifier list of a declaration that can carry `export`.
pub(crate) fn modifiers_of(node: Node<'_>) -> Option<&[tsr_ast::ModifierLike<'_>]> {
    Some(match node {
        Node::FunctionDeclaration(n) => n.modifiers,
        Node::ClassDeclaration(n) => n.modifiers,
        Node::InterfaceDeclaration(n) => n.modifiers,
        Node::TypeAliasDeclaration(n) => n.modifiers,
        Node::EnumDeclaration(n) => n.modifiers,
        Node::ModuleDeclaration(n) => n.modifiers,
        Node::VariableStatement(n) => n.modifiers,
        Node::ImportEqualsDeclaration(n) => n.modifiers,
        _ => return None,
    })
}

fn has_export(modifiers: &[tsr_ast::ModifierLike<'_>]) -> bool {
    has_modifier(modifiers, SyntaxKind::ExportKeyword)
}

/// Whether a class element is `static`, and so a member of the *constructor*
/// rather than of instances.
///
/// Upstream: `ast.IsStatic` (`internal/ast/utilities.go:1048`) —
/// `IsClassElement(node) && HasStaticModifier(node) || IsClassStaticBlockDeclaration(node)`.
/// A `static` block counts without carrying the modifier, which is why it is
/// listed separately rather than falling out of the modifier check.
///
/// Only the kinds that can appear in a class body are listed. `static` is not
/// valid anywhere else, and a node that cannot be a class element is not static
/// no matter what modifiers the parser recovered on it.
fn is_static(node: Node<'_>) -> bool {
    let modifiers = match node {
        Node::ClassStaticBlockDeclaration(_) => return true,
        Node::PropertyDeclaration(n) => n.modifiers,
        Node::MethodDeclaration(n) => n.modifiers,
        Node::GetAccessorDeclaration(n) => n.modifiers,
        Node::SetAccessorDeclaration(n) => n.modifiers,
        Node::IndexSignatureDeclaration(n) => n.modifiers,
        _ => return false,
    };
    has_modifier(modifiers, SyntaxKind::StaticKeyword)
}

fn has_declare(modifiers: &[tsr_ast::ModifierLike<'_>]) -> bool {
    has_modifier(modifiers, SyntaxKind::DeclareKeyword)
}

pub(crate) fn has_modifier(modifiers: &[tsr_ast::ModifierLike<'_>], kind: SyntaxKind) -> bool {
    modifiers.iter().any(
        |modifier| matches!(modifier, tsr_ast::ModifierLike::Token(token) if token.kind == kind),
    )
}

/// Whether a module body contains an `export … ` or `export =` statement.
///
/// Upstream: `hasExportDeclarations`. An ambient module that names its exports
/// is not an export context — only one that names none exports everything.
fn has_export_declarations(module: &tsr_ast::ModuleDeclaration<'_>) -> bool {
    let Some(tsr_ast::ModuleBody::ModuleBlock(block)) = module.body else { return false };
    block.statements.iter().any(|statement| {
        matches!(statement, Statement::ExportDeclaration(_) | Statement::ExportAssignment(_))
    })
}

/// Whether a file is an external module rather than a script.
///
/// Upstream's `isFileProbablyExternalModule`, which the *parser* runs and stores
/// on the file as `ExternalModuleIndicator`. Our `SourceFile` node is generated
/// from `ast.json` and carries no such field ([ADR-0003](../../../docs/adr/0003-tree-plus-side-tables.md)
/// keeps everything derived off the tree), and a side table with one entry per
/// file would be a table for a single boolean, so the binder recomputes it. The
/// scan is over top-level statements only, so it is a few dozen comparisons on a
/// list the binder is about to walk anyway.
///
/// One indicator is **not** implemented: upstream also treats a file containing
/// `import.meta` anywhere as a module (`getImportMetaIfNecessary`), which needs a
/// full-tree walk under specific module settings the binder does not have.
#[must_use]
pub fn is_external_module(file: &SourceFile<'_>) -> bool {
    file.statements.iter().any(|statement| {
        let node = Node::from(*statement);
        match node {
            Node::ImportDeclaration(_) | Node::ExportAssignment(_) | Node::ExportDeclaration(_) => {
                true
            }
            Node::ImportEqualsDeclaration(n) => matches!(
                n.module_reference,
                Some(tsr_ast::ModuleReference::ExternalModuleReference(_))
            ),
            _ => modifiers_of(node).is_some_and(has_export),
        }
    })
}

/// What a JavaScript assignment expression declares.
///
/// Upstream's `ast.JSDeclarationKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JsDeclaration {
    /// `module.exports = x`
    ModuleExports,
    /// `exports.x = 1`, `module.exports.x = 1`
    ExportsProperty,
    /// `this.x = 1`
    ThisProperty,
    /// `f.x = 1` — an *expando* property on something declared elsewhere.
    Property,
    /// `Object.defineProperty(f, "x", { … })`
    ObjectDefinePropertyValue,
    /// `Object.defineProperty(exports, "x", { … })`
    ObjectDefinePropertyExports,
}

/// The object a `Object.defineProperty(target, "name", …)` call describes.
///
/// Upstream's `IsBindableObjectDefinePropertyCall`, returning the first argument
/// rather than a boolean because every caller wants it. The shape is exact:
/// three arguments, a literal name, and a target that is a name rather than an
/// arbitrary expression.
fn bindable_object_define_property_target(node: Node<'_>) -> Option<Expression<'_>> {
    let Node::CallExpression(call) = node else { return None };
    if call.arguments.len() != 3 {
        return None;
    }
    let Some(Expression::PropertyAccessExpression(callee)) = call.expression else { return None };
    if !matches!(callee.expression, Some(Expression::Identifier(object)) if object.text == "Object")
    {
        return None;
    }
    if access_name_source(Expression::PropertyAccessExpression(callee)) != Some("defineProperty") {
        return None;
    }
    if !matches!(
        skip_parentheses(Node::from(call.arguments[1])),
        Node::StringLiteral(_) | Node::NumericLiteral(_) | Node::NoSubstitutionTemplateLiteral(_)
    ) {
        return None;
    }
    // `excludeThisKeyword: true` upstream — `Object.defineProperty(this, …)` is
    // not one of these.
    is_entity_name_expression(call.arguments[0], false).then_some(call.arguments[0])
}

/// The computed property name a declaration was written with, if it was.
fn computed_property_name(node: Node<'_>) -> Option<&tsr_ast::ComputedPropertyName<'_>> {
    let name = match node {
        Node::PropertyDeclaration(n) => n.name,
        Node::PropertySignatureDeclaration(n) => n.name,
        Node::MethodDeclaration(n) => n.name,
        Node::MethodSignatureDeclaration(n) => n.name,
        Node::GetAccessorDeclaration(n) => n.name,
        Node::SetAccessorDeclaration(n) => n.name,
        Node::EnumMember(n) => n.name,
        Node::PropertyAssignment(n) => n.name,
        _ => return None,
    };
    match name {
        tsr_ast::PropertyName::ComputedPropertyName(computed) => Some(computed),
        _ => None,
    }
}

/// The computed name of a declaration whose name is genuinely *late-bound*.
///
/// Upstream's `ast.HasDynamicName`. `['a']` and `[2]` are computed in syntax
/// only — the expression is already the value — and declare statically; the
/// signed-numeric case (`[-1]`) is upstream's third static form and is not
/// ported, because building the name needs an owned string where every name here
/// borrows from the source.
fn dynamic_name<'a>(
    arena: crate::names::Names<'a, '_>,
    node: Node<'a>,
) -> Option<&'a tsr_ast::ComputedPropertyName<'a>> {
    let computed = computed_property_name(node)?;
    computed_name(arena, computed).is_none().then_some(computed)
}

/// The text of a string or numeric literal, which is what a statically-named
/// `Object.defineProperty` call and a bracketed access carry.
fn string_or_numeric_text<'a>(
    arena: crate::names::Names<'a, '_>,
    node: Node<'a>,
) -> Option<&'a str> {
    match skip_parentheses(node) {
        Node::StringLiteral(literal) => Some(literal.text),
        Node::NumericLiteral(literal) => Some(canonical_numeric(arena, literal.text)),
        Node::NoSubstitutionTemplateLiteral(literal) => Some(literal.text),
        _ => None,
    }
}

/// A numeric member name, spelled the way upstream's scanner canonicalises its
/// token value (`scanner.go:2194`, `jsnum.FromString(…).String()`): `0b11`
/// binds as `3`, `1.0` as `1`. The node keeps the source spelling for the
/// printer; the *name* is the value's.
fn canonical_numeric<'a>(arena: crate::names::Names<'a, '_>, text: &'a str) -> &'a str {
    let canonical = tsr_core::jsnum::canonical_numeric_text(text);
    if canonical == text { text } else { arena.alloc_str(&canonical) }
}

/// Whether an expression is a *name*: an identifier, or a dotted chain of them.
///
/// Upstream's `IsEntityNameExpressionEx`. `allow_js` additionally permits `this`
/// and a bracketed literal (`a["b"].c`), which are names only in a JavaScript
/// file.
fn is_entity_name_expression(expression: Expression<'_>, allow_js: bool) -> bool {
    match expression {
        Expression::Identifier(_) => true,
        Expression::PropertyAccessExpression(access) => {
            matches!(access.name, Some(tsr_ast::MemberName::Identifier(_)))
                && access
                    .expression
                    .is_some_and(|target| is_entity_name_expression(target, allow_js))
        }
        Expression::ElementAccessExpression(access) => {
            allow_js
                && access_name_source(expression).is_some()
                && access
                    .expression
                    .is_some_and(|target| is_entity_name_expression(target, allow_js))
        }
        Expression::KeywordExpression(keyword) => {
            allow_js && keyword.kind == SyntaxKind::ThisKeyword
        }
        _ => false,
    }
}

/// Whether a file is a JavaScript file, whose assignment forms are declarations.
///
/// Upstream reads the flag the parser set; here it is the extension, per
/// [ADR-0015](../../../docs/adr/0015-file-name-is-a-bind-input.md).
fn is_javascript_file(file_name: &str) -> bool {
    matches!(
        file_name.rsplit_once('.').map(|(_, extension)| extension),
        Some("js" | "jsx" | "mjs" | "cjs")
    )
}

/// The object an access expression reads from: `a` in `a.b` and in `a[b]`.
fn access_target(expression: Expression<'_>) -> Option<Expression<'_>> {
    match expression {
        Expression::PropertyAccessExpression(access) => access.expression,
        Expression::ElementAccessExpression(access) => access.expression,
        _ => None,
    }
}

/// The property an access expression names, when it names one statically.
///
/// Upstream's `GetElementOrPropertyAccessName`: an identifier for `a.b`, and a
/// string or numeric literal for `a["b"]`. `a[k]` names nothing the binder can
/// know, which is the late-bound case.
/// [`access_name`] without canonical numeric spelling, for the free-function
/// callers that only compare against identifier names (`defineProperty`,
/// `exports`) or test presence — a numeric spelling can never equal those.
fn access_name_source(expression: Expression<'_>) -> Option<&str> {
    match expression {
        Expression::PropertyAccessExpression(access) => match access.name? {
            tsr_ast::MemberName::Identifier(identifier) => Some(identifier.text),
            tsr_ast::MemberName::PrivateIdentifier(_) => None,
        },
        Expression::ElementAccessExpression(access) => {
            match skip_parentheses(Node::from(access.argument_expression?)) {
                Node::StringLiteral(literal) => Some(literal.text),
                Node::NumericLiteral(literal) => Some(literal.text),
                Node::NoSubstitutionTemplateLiteral(literal) => Some(literal.text),
                _ => None,
            }
        }
        _ => None,
    }
}

fn access_name<'a>(
    arena: crate::names::Names<'a, '_>,
    expression: Expression<'a>,
) -> Option<&'a str> {
    match expression {
        Expression::PropertyAccessExpression(access) => match access.name? {
            tsr_ast::MemberName::Identifier(identifier) => Some(identifier.text),
            tsr_ast::MemberName::PrivateIdentifier(_) => None,
        },
        Expression::ElementAccessExpression(access) => {
            match skip_parentheses(Node::from(access.argument_expression?)) {
                Node::StringLiteral(literal) => Some(literal.text),
                Node::NumericLiteral(literal) => Some(canonical_numeric(arena, literal.text)),
                Node::NoSubstitutionTemplateLiteral(literal) => Some(literal.text),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Whether `expression` is the identifier `exports` (`IsExportsIdentifier`).
fn is_exports_identifier(expression: Expression<'_>) -> bool {
    matches!(expression, Expression::Identifier(identifier) if identifier.text == "exports")
}

/// Whether `expression` is `module.exports` (`IsModuleExportsAccessExpression`).
fn is_module_exports_access(expression: Expression<'_>) -> bool {
    let Some(target) = access_target(expression) else { return false };
    matches!(target, Expression::Identifier(identifier) if identifier.text == "module")
        && access_name_source(expression) == Some("exports")
}

/// Whether a file is a declaration file, and so an ambient context.
///
/// Upstream reads `file.IsDeclarationFile`, which the parser sets from the same
/// suffix test. `.d.ts`, `.d.mts`, `.d.cts`, and the `.d.*.ts` form used by
/// generated libraries all count.
#[must_use]
pub fn is_declaration_file(file_name: &str) -> bool {
    let Some(stem) = file_name
        .strip_suffix(".ts")
        .or_else(|| file_name.strip_suffix(".mts"))
        .or_else(|| file_name.strip_suffix(".cts"))
    else {
        return false;
    };
    // Split rather than `ends_with(".d")` so that `d.ts` — a file whose whole
    // name is `d` — is not mistaken for a declaration file.
    matches!(stem.rsplit_once('.'), Some((_, "d")))
}

/// The file name with its extension removed, as `tspath.RemoveFileExtension`.
fn remove_file_extension(file_name: &str) -> &str {
    match file_name.rfind('.') {
        // A leading dot is the whole name (`.gitignore`), not an extension.
        Some(dot) if dot > 0 => &file_name[..dot],
        _ => file_name,
    }
}

/// Whether a file contains an `export …` or `export =` statement.
///
/// The file-level half of `hasExportDeclarations`, which decides whether a
/// declaration file is an *export context* — one where every declaration is
/// implicitly exported because none of them says `export`.
fn file_has_export_declarations(file: &SourceFile<'_>) -> bool {
    file.statements
        .iter()
        .any(|s| matches!(s, Statement::ExportDeclaration(_) | Statement::ExportAssignment(_)))
}

/// Whether `export default x` re-exports something that already has a symbol.
///
/// Upstream's `ExpressionIsAlias`: an entity name (`x`, `a.b.c`) names an
/// existing declaration, so `export default x` is an *alias* for it; anything
/// else is a fresh value and the export is a property. Upstream also treats a
/// `require(…)` call as an alias, which needs the `CommonJS` handling that is not
/// ported.
fn expression_is_alias(expression: Option<Expression<'_>>) -> bool {
    match expression {
        Some(Expression::Identifier(_) | Expression::ClassExpression(_)) => true,
        Some(Expression::PropertyAccessExpression(access)) => {
            expression_is_alias(access.expression)
        }
        _ => false,
    }
}

/// Upstream: `isGeneratorFunctionExpression`.
fn is_generator_function_expression(node: Node<'_>) -> bool {
    matches!(node, Node::FunctionExpression(function) if function.asterisk_token.is_some())
}

/// What kind of symbol `node` declares, and where it belongs.
///
/// `None` for nodes that declare nothing, which is most of them. `parents` is
/// the binder's ancestor stack (ending with `node`'s parent), which
/// `GetModuleInstanceState` climbs for an `export { x }` target.
fn classify<'a>(
    node: Node<'a>,
    parents: &[(NodeId, Node<'a>)],
) -> Option<(SymbolFlags, Destination)> {
    use Destination as D;
    use SymbolFlags as S;
    Some(match node {
        Node::FunctionDeclaration(_) => (S::FUNCTION, D::Locals),
        Node::ClassDeclaration(_) | Node::ClassExpression(_) => (S::CLASS, D::Locals),
        Node::InterfaceDeclaration(_) => (S::INTERFACE, D::Locals),
        Node::TypeAliasDeclaration(_) => (S::TYPE_ALIAS, D::Locals),
        // `bindEnumDeclaration` (`binder.go`) splits on `IsEnumConst`:
        // `ConstEnum` and `RegularEnum` are separate flags with separate
        // excludes (`symbolflags.go:64-65`, const enums merge only with const
        // enums), and consumers ask for the one they mean —
        // `checkResolvedBlockScopedVariable` (`checker.go:1908`) tests
        // `RegularEnum` because a `const enum` is inlined and has no temporal
        // dead zone. Mapping both to `REGULAR_ENUM` is the same defect §95 fixed
        // for modules, one flag over: see `checker-notes-diag2.md` §100.
        Node::EnumDeclaration(declaration) => (
            if tsr_ast::has_syntactic_modifier(declaration.modifiers, SyntaxKind::ConstKeyword) {
                S::CONST_ENUM
            } else {
                S::REGULAR_ENUM
            },
            D::Locals,
        ),
        // `bindModuleDeclaration` (`binder.go:1268`) picks the flag from
        // `GetModuleInstanceState`: a namespace that emits no JavaScript is a
        // `NamespaceModule` and collides with nothing, and one that does is a
        // `ValueModule` and occupies value space. This port mapped every module
        // to `VALUE_MODULE`, which is why §94's excludes fix over-reported on
        // ambient and augmentation declarations — `checker-notes-diag2.md` §95.
        //
        // §601: the test is **`!= NonInstantiated`**, not `== Instantiated`.
        // `ModuleInstanceState` has THREE values, and `declareModuleSymbol`
        // (`binder.go:815`) writes
        // `instantiated := state != ModuleInstanceStateNonInstantiated` — so
        // **`ConstEnumOnly` is a `ValueModule`**, which is the whole point of
        // that third state: `namespace N { const enum E { … } }` emits code
        // when `preserveConstEnums` is on and therefore occupies value space.
        // Testing `== Instantiated` put it in the namespace bucket and cost 44
        // lines in `constEnums` alone. Measured: +44, zero adverse,
        // `binder_symbols` unmoved at 100%.
        Node::ModuleDeclaration(_) => (
            if matches!(
                tsr_ast::module_instance_state(
                    node,
                    &parents.iter().map(|&(_, parent)| parent).collect::<Vec<_>>()
                ),
                tsr_ast::ModuleInstanceState::NonInstantiated
            ) {
                S::NAMESPACE_MODULE
            } else {
                S::VALUE_MODULE
            },
            D::Locals,
        ),
        Node::TypeParameterDeclaration(_) => (S::TYPE_PARAMETER, D::Locals),
        Node::ParameterDeclaration(_) => (S::FUNCTION_SCOPED_VARIABLE, D::Locals),

        // Members.
        // A JSX attribute is grouped here because it is one: a property of the
        // anonymous attributes object the element is checked against, so
        // `<X icon={…} />` declares `icon`.
        Node::PropertyDeclaration(_)
        | Node::PropertySignatureDeclaration(_)
        | Node::PropertyAssignment(_)
        | Node::ShorthandPropertyAssignment(_)
        | Node::JsxAttribute(_) => (S::PROPERTY, D::Members),
        Node::MethodDeclaration(_) | Node::MethodSignatureDeclaration(_) => (S::METHOD, D::Members),
        // An index signature has no name of its own; upstream files every one in
        // a container under the same internal name so they merge.
        Node::IndexSignatureDeclaration(_) => (S::SIGNATURE, D::Members),
        Node::GetAccessorDeclaration(_) => (S::GET_ACCESSOR, D::Members),
        Node::SetAccessorDeclaration(_) => (S::SET_ACCESSOR, D::Members),
        Node::EnumMember(_) => (S::ENUM_MEMBER, D::Members),

        // Imports and exports are aliases for symbols elsewhere. Resolving them
        // needs module resolution; the binder only records that they exist.
        Node::ImportClause(_)
        | Node::ImportSpecifier(_)
        | Node::NamespaceImport(_)
        | Node::ExportSpecifier(_)
        | Node::ImportEqualsDeclaration(_) => (S::ALIAS, D::Locals),

        // `export = x` and `export default x` name the module itself rather than
        // a declaration inside it, so they go straight into the module's exports
        // under a well-known name (`bindExportAssignment`).
        Node::ExportAssignment(assignment) => (
            if expression_is_alias(assignment.expression) { S::ALIAS } else { S::PROPERTY },
            D::Exports,
        ),
        // `export * as ns from "m"` exports one alias named `ns`.
        Node::NamespaceExport(_) => (S::ALIAS, D::Exports),
        // `export * from "m"` — no clause, so it names nothing and re-exports
        // everything. Every one in a module merges into a single `__export`
        // symbol whose declarations are the stars (`bindExportDeclaration`).
        //
        // An export declaration *with* a clause is not a declaration at all
        // here: `export { a }` files its specifiers, and `export * as ns`
        // files the `NamespaceExport` above.
        Node::ExportDeclaration(n) if n.export_clause.is_none() => (S::EXPORT_STAR, D::Exports),
        // `export as namespace N` claims the global name `N` for a UMD module.
        Node::NamespaceExportDeclaration(_) => (S::ALIAS, D::GlobalExports),

        _ => return None,
    })
}

/// The declared name, if the node has a simple one.
///
/// `None` for computed names (`[expr]`) and for destructuring patterns. Both
/// declare symbols in TypeScript — a binding pattern declares one per element —
/// and neither is handled yet; see the note in `lib.rs`.
/// The span of a declaration's *name*, for a diagnostic anchored on it.
///
/// Ported from `ast.GetNameOfDeclaration` (`internal/ast/utilities.go`) as the
/// binder uses it: upstream anchors a redeclaration diagnostic on the name and
/// falls back to the declaration itself when there is none (`binder.go:245`).
///
/// Only the declaration kinds that can collide in a symbol table are listed. A
/// kind not here falls back to the declaration's own span, which is what upstream
/// does for a nameless declaration anyway.
fn name_node_of(node: Node<'_>) -> Option<NodeId> {
    use tsr_ast::PropertyName;
    let property = |name: &PropertyName<'_>| name.node_id();
    match node {
        Node::FunctionDeclaration(n) => n.name.and_then(|i| i.node_id),
        Node::ClassDeclaration(n) => n.name.and_then(|i| i.node_id),
        Node::ClassExpression(n) => n.name.and_then(|i| i.node_id),
        Node::InterfaceDeclaration(n) => n.name.and_then(|i| i.node_id),
        Node::TypeAliasDeclaration(n) => n.name.and_then(|i| i.node_id),
        Node::EnumDeclaration(n) => n.name.and_then(|i| i.node_id),
        Node::ImportEqualsDeclaration(n) => n.name.and_then(|i| i.node_id),
        Node::ModuleDeclaration(n) => match n.name {
            Some(tsr_ast::ModuleName::Identifier(i)) => i.node_id,
            Some(tsr_ast::ModuleName::StringLiteral(l)) => l.node_id,
            None => None,
        },
        Node::VariableDeclaration(n) => n.name.as_ref().and_then(tsr_ast::BindingName::node_id),
        Node::ParameterDeclaration(n) => n.name.as_ref().and_then(tsr_ast::BindingName::node_id),
        Node::BindingElement(n) => n.name.as_ref().and_then(tsr_ast::BindingName::node_id),
        Node::PropertyDeclaration(n) => property(&n.name),
        Node::PropertySignatureDeclaration(n) => property(&n.name),
        Node::MethodDeclaration(n) => property(&n.name),
        Node::MethodSignatureDeclaration(n) => property(&n.name),
        Node::GetAccessorDeclaration(n) => property(&n.name),
        Node::SetAccessorDeclaration(n) => property(&n.name),
        Node::EnumMember(n) => property(&n.name),
        Node::PropertyAssignment(n) => property(&n.name),
        Node::ShorthandPropertyAssignment(n) => property(&n.name),
        // §234 — kinds upstream reaches through `GetNameOfDeclaration`'s
        // unconditional `return declaration.Name()` tail
        // (`ast/utilities.go:1461`). Every redeclaration diagnostic is
        // positioned here, so a kind missing from this list reports at the
        // declaration's first token instead of its name.
        Node::ImportSpecifier(n) => n.name.and_then(|i| i.node_id),
        Node::ExportSpecifier(n) => n.name.and_then(|n| n.node_id()),
        Node::ImportClause(n) => n.name.and_then(|i| i.node_id),
        Node::NamespaceImport(n) => n.name.and_then(|i| i.node_id),
        Node::NamespaceExport(n) => n.name.and_then(|n| n.node_id()),
        Node::TypeParameterDeclaration(n) => n.name.and_then(|i| i.node_id),
        // Upstream's one special case that reaches this port: the
        // **expression**, and only when it is an identifier
        // (`ast/utilities.go:1476`). `export = server` reports at `server`,
        // column 10 — not at the `export` keyword, column 1.
        Node::ExportAssignment(n) => match n.expression {
            Some(tsr_ast::Expression::Identifier(i)) => i.node_id,
            _ => None,
        },
        _ => None,
    }
}

fn declaration_name<'a>(
    arena: crate::names::Names<'a, '_>,
    node: Node<'a>,
    nodes: &NodeTable,
    source: &'a str,
) -> Option<&'a str> {
    let from_property_name = |name: tsr_ast::PropertyName<'a>| -> Option<&'a str> {
        match name {
            tsr_ast::PropertyName::Identifier(i) => Some(i.text),
            tsr_ast::PropertyName::StringLiteral(s) => Some(s.text),
            tsr_ast::PropertyName::NumericLiteral(n) => Some(canonical_numeric(arena, n.text)),
            tsr_ast::PropertyName::PrivateIdentifier(p) => Some(p.text),
            tsr_ast::PropertyName::ComputedPropertyName(computed) => computed_name(arena, computed),
            tsr_ast::PropertyName::BigIntLiteral(b) => Some(b.text),
            tsr_ast::PropertyName::NoSubstitutionTemplateLiteral(t) => Some(t.text),
        }
    };

    match node {
        Node::FunctionDeclaration(n) => n.name.map(|i| i.text),
        Node::ClassDeclaration(n) => n.name.map(|i| i.text),
        Node::ClassExpression(n) => n.name.map(|i| i.text),
        Node::InterfaceDeclaration(n) => n.name.map(|i| i.text),
        Node::TypeAliasDeclaration(n) => n.name.map(|i| i.text),
        Node::EnumDeclaration(n) => n.name.map(|i| i.text),
        Node::TypeParameterDeclaration(n) => n.name.map(|i| i.text),
        // `declare module "fs"` is the symbol `"fs"`, **with the quotes in the
        // name**, exactly as `getDeclarationName` spells it
        // (`internal/binder/binder.go:311`). See [`quoted_module_name`].
        //
        // # §234: `declare global` should be `__global` here, and doing that costs 5 cases
        //
        // Upstream returns `ast.InternalSymbolNameGlobal` (`binder.go:308-311`)
        // for a global scope augmentation rather than the identifier the parser
        // synthesises, so **`global` is not a resolvable name upstream**.
        // Confirmed by executing upstream over a probe fixture rather than
        // reading it: `TS2304: Cannot find name 'global'`, reported twice, while
        // `globalThis` resolves normally in the same file. That is why
        // `spellingSuggestionGlobal1`/`2`/`4` want `>global : any` at a
        // reference where this port answers a confident `typeof global` —
        // upstream's `any` is a rendered `errorType` for an unresolved name.
        //
        // **Making this arm return an internal name measured 0 won and 5 LOST**
        // (`doubleUnderscoreReactNamespace`,
        // `evalOrArgumentsInDeclarationFunctions`,
        // `newNamesInGlobalAugmentations1`, `globalAugmentationModuleResolution`,
        // `nodeModulesTripleSlashReferenceModeOverrideOldResolutionError`) plus
        // a scatter of −1s. The name is **load-bearing for merging**: this
        // binder merges global augmentations by matching the declaration name
        // across files, and upstream merges them by a separate path that does
        // not need the name to agree. Renaming the symbol without porting that
        // path breaks the merge.
        //
        // So the fix is not here. The reference should fail to resolve while
        // the declaration keeps its name — a filter in `resolve_name` that
        // declines a global-augmentation module symbol, not a rename. Recorded
        // rather than attempted because the merge path is the real dependency
        // and it has not been read.
        //
        // # §600 CORRECTED: §234's reference half was ALREADY FIXED, by §265
        //
        // §600 (below) recorded that the two halves must be measured together
        // because a change satisfying one would move the other. **That is
        // wrong, and the fix it worried about is thirty lines up in this file.**
        // §265 already refuses a `global` REFERENCE in `resolve_name`, and
        // `spellingSuggestionGlobal1`/`2`/`3`/`4` all PASS at the §599 baseline
        // (0 non-right of 5, 9, 7 and 5 lines). The filter is independent of the
        // declaration's type, so the declaration half is **independently
        // attemptable** — §600 argued from §234's text without re-reading the
        // code §234 asked for.
        //
        // # §600: the DECLARATION half, which §234 does not cover, and it is wrong the other way
        //
        // §234 is about a REFERENCE to `global`. At the **declaration name** the
        // port is wrong in the opposite direction: `declare global { … }` wants
        // `>global : typeof global` and this port answers `any`, because the
        // augmentation symbol carries `NAMESPACE_MODULE` alone and
        // `get_type_of_symbol`'s module arm tests `VALUE_MODULE`
        // (`checker.go:16511` — upstream's flag list, faithfully ported).
        //
        // Five single-transition cases, each one line from passing:
        // `moduleAugmentationGlobal6`/`6_1`/`7`/`7_1` and
        // `duplicatePackage_globalMerge`.
        //
        // # §603: THIS IS ALREADY REFUSED, and §600 failed to find it
        //
        // **STATUS §5 carries `§311 \`declare global\`'s name line — refused by
        // 5:42`, measured at `483de91f`, and its population is exactly the five
        // cases below.** Answering `typeof global` — *"from
        // `get_type_of_symbol` OR from the producer's declaration-name arm,
        // both measured identically"* — wins 5 lines and breaks 42:
        // error-carrying cases want `any` on the same line
        // (`duplicateIdentifierRelatedSpans5`), and files where upstream emits
        // NO line for the name shift every later assertion out of alignment
        // (`jsxElementType`, 30+ lines).
        //
        // Its reopening condition is already written and is NOT a flag change:
        // *transcribe upstream's `type_symbol_baseline.go` guard chain for WHEN
        // the global name emits and with which arm* — the §254 note records the
        // fast-path/node-builder split it turns on.
        //
        // §600 presented these five as an unrecorded population. They were
        // recorded, with a number, in the file §600 was written into. **A
        // refusal is only worth its number if the next session can find it** —
        // this one was found by reading STATUS §5 top to bottom, which is what
        // §600 should have done before writing a word.
        //
        // **Not attempted here**, and the reason below is superseded by §311's. The
        // reference half is already correct (see the correction above), so this
        // is a standalone item. What stops it is that upstream's own answer is
        // not yet understood: `declare global { interface … }` is
        // NonInstantiated, so upstream binds it `NamespaceModule` too and
        // `getTypeOfSymbol` has no arm for that — yet the baseline records
        // `typeof global` rather than the `any` a rendered `errorType` would
        // give. **The declaration-name road upstream takes has not been read**,
        // and guessing a flag here is exactly what §234 measured at 0 won / 5
        // lost. Read `getTypeOfNode`'s declaration-name arm first.
        Node::ModuleDeclaration(n) => n.name.map(|name| match name {
            tsr_ast::ModuleName::StringLiteral(literal) => quoted_module_name(arena, literal.text),
            tsr_ast::ModuleName::Identifier(identifier) => identifier.text,
        }),
        Node::ParameterDeclaration(n) => n.name.and_then(binding_name),
        Node::VariableDeclaration(n) => n.name.and_then(binding_name),
        // The elements of a pattern declare; the pattern itself does not.
        Node::BindingElement(n) => n.name.and_then(binding_name),
        Node::PropertyDeclaration(n) => from_property_name(n.name),
        Node::PropertySignatureDeclaration(n) => from_property_name(n.name),
        Node::MethodDeclaration(n) => from_property_name(n.name),
        Node::MethodSignatureDeclaration(n) => from_property_name(n.name),
        Node::GetAccessorDeclaration(n) => from_property_name(n.name),
        Node::SetAccessorDeclaration(n) => from_property_name(n.name),
        Node::EnumMember(n) => from_property_name(n.name),
        Node::PropertyAssignment(n) => from_property_name(n.name),
        Node::ShorthandPropertyAssignment(n) => from_property_name(n.name),
        Node::ImportClause(n) => n.name.map(|i| i.text),
        Node::ImportSpecifier(n) => n.name.map(|i| i.text),
        Node::NamespaceImport(n) => n.name.map(|i| i.text),
        Node::ExportSpecifier(n) => n.name.map(export_name),
        Node::ImportEqualsDeclaration(n) => n.name.map(|i| i.text),
        Node::JsxAttribute(n) => n.name.and_then(|name| jsx_attribute_name(name, nodes, source)),
        Node::IndexSignatureDeclaration(_) => Some(INTERNAL_INDEX),
        // `export = x` is the module's whole value; `export default x` is its
        // default export. Neither has a name of its own, so upstream files both
        // under a well-known one — which is also what makes `export default`
        // merge across the declarations that share it.
        Node::ExportAssignment(n) => {
            Some(if n.is_export_equals { INTERNAL_EXPORT_EQUALS } else { INTERNAL_DEFAULT })
        }
        Node::NamespaceExport(n) => n.name.map(export_name),
        Node::ExportDeclaration(n) if n.export_clause.is_none() => Some(INTERNAL_EXPORT_STAR),
        Node::NamespaceExportDeclaration(n) => n.name.map(|i| i.text),
        _ => None,
    }
}

/// The name of a JSX attribute.
///
/// A namespaced one (`xlink:href`) is named `namespace:name` upstream, which
/// needs an owned string where every name here borrows from the source. Left
/// undeclared rather than misnamed; it is rare and confined to XML-ish JSX.
fn jsx_attribute_name<'a>(
    name: tsr_ast::JsxAttributeName<'a>,
    nodes: &NodeTable,
    source: &'a str,
) -> Option<&'a str> {
    match name {
        tsr_ast::JsxAttributeName::Identifier(identifier) => Some(identifier.text),
        // `ns:href` is one name upstream (`JsxNamespacedName.Text()` joins the
        // two halves with a colon). Here it is two identifier nodes with a colon
        // between, and no node holds the whole of it — so the name is the source
        // range they span, which is a borrow rather than the concatenation
        // upstream builds.
        tsr_ast::JsxAttributeName::JsxNamespacedName(namespaced) => {
            let start = nodes.span(namespaced.namespace?.node_id?).start;
            let end = nodes.span(namespaced.name?.node_id?).end;
            source.get(start as usize..end as usize)
        }
    }
}

/// The name a computed property declares, when it is spelled with a literal.
///
/// `{ ['a']: 1 }` and `{ [2]: 1 }` name a member *statically*: upstream treats
/// them exactly as if written `a` and `2`, because the expression is already the
/// value. Anything else — `[Symbol.iterator]`, `[k]`, `[foo()]` — is
/// **late-bound**: the name is whatever the expression evaluates to, which needs
/// the checker, so the binder declares nothing and the member is invisible until
/// then.
///
/// Upstream's third static form, a signed numeric literal (`[-1]`), builds the
/// name by concatenating the operator with the operand
/// (`ast/utilities.go:3170`); the arena supplies the owned string that used to
/// keep it late-bound here.
fn computed_name<'a>(
    arena: crate::names::Names<'a, '_>,
    computed: &'a tsr_ast::ComputedPropertyName<'a>,
) -> Option<&'a str> {
    match computed.expression? {
        Expression::StringLiteral(literal) => Some(literal.text),
        Expression::NoSubstitutionTemplateLiteral(literal) => Some(literal.text),
        Expression::NumericLiteral(literal) => Some(canonical_numeric(arena, literal.text)),
        Expression::PrefixUnaryExpression(unary)
            if matches!(unary.operator.kind, SyntaxKind::MinusToken | SyntaxKind::PlusToken) =>
        {
            let Some(Expression::NumericLiteral(literal)) = unary.operand else { return None };
            // The generated-Go oracle keeps BOTH signs: `[+1]` binds as
            // `"+1"` and merges with the string member of that spelling
            // (`duplicateObjectLiteralProperty_computedName1`:
            // `Symbol("+1", Decl(15,12), Decl(16,12))`). A plain reading of
            // `GetPropertyNameForPropertyNameNode` suggested plus is dropped;
            // per ADR-0006 the baseline wins.
            let operand = canonical_numeric(arena, literal.text);
            let sign = if unary.operator.kind == SyntaxKind::MinusToken { "-" } else { "+" };
            Some(arena.alloc_str(&format!("{sign}{operand}")))
        }
        _ => None,
    }
}

/// The symbol name of `declare module "x"` — **`"x"`, quotes included**.
///
/// Upstream's `getDeclarationName` (`internal/binder/binder.go:311`):
///
/// ```go
/// if ast.IsAmbientModule(node) {
///     if ast.IsGlobalScopeAugmentation(node) { return ast.InternalSymbolNameGlobal }
///     return "\"" + moduleName + "\""
/// }
/// ```
///
/// The quotes are not decoration. `ast.IsAmbientModuleSymbolName` is literally
/// `strings.HasPrefix(s, "\"") && strings.HasSuffix(s, "\"")`
/// (`ast/utilities.go:1656`), and `initializeChecker` uses it to keep ambient
/// module symbols out of the plain global merge. They are what makes
/// `"process"` and `process` two different keys in one table.
///
/// # This port stored the text unquoted, and it was a real defect
///
/// The name was recovered from the declaration's *shape* instead
/// (`tsr_checker::Checker::ambient_module`), on the reasoning that quoting
/// would need an owned string where every symbol name borrows from the source.
/// That reasoning expired: the binder takes an [`Arena`](tsr_core::Arena) and
/// already allocates names through it for canonical numerics and signed
/// computed property names.
///
/// The defect it left is the normal shape of `@types/node`, and `tsc` is silent
/// on all of it:
///
/// ```ts
/// // crypto.d.ts — a script, so its top-level locals become globals
/// declare module "crypto" {          // symbol `crypto` (unquoted) -> globals
///     global {
///         var crypto: …              // symbol `crypto`             -> globals
///     }
/// }
/// ```
///
/// Both reached one key, `merge_symbol` hit the excludes mask, and the checker
/// reported `TS2300: Duplicate identifier 'crypto'` on both declarations —
/// or `TS2649` where the module symbol is a `NAMESPACE_MODULE`, as
/// `declare module "console"` is. **Measured before the fix: 79 spurious TS2300
/// and 13 spurious TS2649 across a 22-package monorepo, and zero conformance
/// cases**, because no corpus case exhibits the collision.
///
/// It became reachable, not new, when `declare global` blocks started merging
/// (`docs/architecture/checker-notes-diag2.md` §173): before that the `global`
/// block's contents went nowhere and never met the module symbol.
///
/// # A global augmentation is deliberately NOT renamed
///
/// The same upstream branch renames `declare global` to
/// `InternalSymbolNameGlobal` (`__global`), and this port leaves it as `global`.
/// That is not the defect — the block's symbol is a *local* of the file it is
/// written in and nothing looks it up by name — and `.symbols` baselines print
/// it as bare `global` (`moduleAugmentationGlobal4.symbols`: `>global :
/// Symbol(global, Decl(f1.ts, 0, 0))`), because `symbolToString` strips the
/// internal prefix. Renaming would mean teaching the suite to strip it back,
/// against a suite currently at 100%, for no behaviour. Recorded so the
/// divergence is a decision rather than an omission.
fn quoted_module_name<'a>(arena: crate::names::Names<'a, '_>, text: &str) -> &'a str {
    arena.alloc_str(&format!("\"{text}\""))
}

/// `ast.IsAmbientModule` (`internal/ast/utilities.go:1652`), for a node already
/// known to be a module declaration: it is spelled with a string literal name,
/// or it is a `global { … }` block.
fn is_ambient_module(module: &tsr_ast::ModuleDeclaration<'_>) -> bool {
    matches!(module.name, Some(tsr_ast::ModuleName::StringLiteral(_)))
        || module.keyword.kind == SyntaxKind::GlobalKeyword
}

/// The name a binding introduces, or `None` for a pattern.
///
/// A pattern declares nothing itself — `const { a, b } = x` declares `a` and
/// `b`, and those are the `BindingElement`s inside it, each of which reaches
/// this function with an identifier.
fn binding_name(name: BindingName<'_>) -> Option<&str> {
    match name {
        BindingName::Identifier(i) => Some(i.text),
        BindingName::BindingPattern(_) => None,
    }
}

/// The name of an `export { … }` specifier, which may be a string.
fn export_name(name: tsr_ast::ModuleExportName<'_>) -> &str {
    match name {
        tsr_ast::ModuleExportName::Identifier(i) => i.text,
        tsr_ast::ModuleExportName::StringLiteral(s) => s.text,
    }
}
