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

// Nine independent cursors is what the algorithm is; collapsing them into a
// state machine would hide the save-and-restore structure that makes it
// checkable against upstream line by line.
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct Binder<'a, 'n> {
    nodes: &'n NodeTable,
    symbols: SymbolStore<'a>,
    /// `node -> the symbol it declares`. Dense, because node ids are dense.
    node_symbols: Vec<Option<SymbolId>>,
    /// `container node -> its locals`. Sparse: most nodes are not containers.
    locals: rustc_hash::FxHashMap<NodeId, SymbolTable<'a>>,
    diagnostics: Vec<Diagnostic>,

    // ---- lexical scope ----
    /// Nearest node with a `locals` table.
    container: NodeId,
    /// Nearest block, for `let`/`const` and class declarations.
    block: NodeId,
    /// Symbol of the nearest container that owns members or exports.
    owner: Option<SymbolId>,
    /// The file's source text.
    ///
    /// Read only where a name is a *range* of the source that no single node
    /// holds; see [`FileInfo`].
    source: &'a str,
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
    pub(crate) fn new(nodes: &'n NodeTable) -> Self {
        // Measured: `checker.ts`, `dom.generated.d.ts`, `Herebyfile.mjs` and a
        // `.tsx` fixture come to 81,713 flow nodes for 419,464 AST nodes — one
        // per 5.1. Reserving for one per five costs a few percent of slack and
        // removes every growth reallocation of a megabyte-scale vector.
        let flow = FlowStore::with_capacity(nodes.len() / 5 + 16);
        let unreachable = flow.unreachable();
        Self {
            nodes,
            symbols: SymbolStore::new(),
            node_symbols: vec![None; nodes.len()],
            locals: rustc_hash::FxHashMap::default(),
            diagnostics: Vec::new(),
            container: NodeId::ZERO,
            block: NodeId::ZERO,
            owner: None,
            source: "",
            in_js_file: false,
            file_node: NodeId::ZERO,
            file_symbol_name: "",
            module_symbol: None,
            commonjs_module: false,
            this_container: NodeId::ZERO,
            computed_names: rustc_hash::FxHashMap::default(),
            name_nodes: rustc_hash::FxHashMap::default(),
            expando_assignments: Vec::new(),
            expando_initializers: rustc_hash::FxHashMap::default(),
            is_module: false,
            global_exports: SymbolTable::default(),
            flow,
            node_flow: vec![None; nodes.len()],
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
            in_assignment_pattern: false,
            seen_this_keyword: false,
            facts: rustc_hash::FxHashMap::default(),
            end_flow: rustc_hash::FxHashMap::default(),
            return_flow: rustc_hash::FxHashMap::default(),
            fallthrough_flow: rustc_hash::FxHashMap::default(),
            ancestors: Vec::new(),
            children: Vec::new(),
        }
    }

    pub(crate) fn bind_source_file(
        mut self,
        file: &'a SourceFile<'a>,
        info: FileInfo<'a>,
    ) -> BindResult<'a> {
        let file_name = info.name;
        self.source = info.text;
        let root = Node::SourceFile(file);
        let root_id = root.node_id().expect("the source file is registered");

        self.container = root_id;
        self.block = root_id;

        // Upstream's `bindSourceFileIfExternalModule`. A file's own symbol exists
        // only for a *module*; a script's top-level declarations are globals and
        // belong in the file's locals, with nothing to export them from.
        self.export_context = is_declaration_file(file_name) && !file_has_export_declarations(file);
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

        BindResult {
            computed_names: self.computed_names,
            global_exports: self.global_exports,
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

    /// Bind `node` and everything under it.
    fn bind(&mut self, node: Node<'a>) {
        let Some(id) = node.node_id() else {
            // Unregistered: descend without treating it as a declaration.
            self.bind_each_child(node);
            return;
        };

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
        let saved_container = self.container;
        let saved_block = self.block;
        let saved_owner = self.owner;
        let saved_this_container = self.this_container;

        if flags.contains(ContainerFlags::IS_CONTAINER) {
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
            // An ambient module exports everything it declares — unless it uses
            // `export` explicitly somewhere, in which case only what it names.
            let ambient = self.in_ambient_module
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
        self.block = saved_block;
        self.owner = saved_owner;
        self.this_container = saved_this_container;
        self.export_context = saved_export_context;
        self.in_ambient_module = saved_in_ambient;
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
            Node::Identifier(_) | Node::MetaProperty(_) | Node::BindingElement(_) => {
                self.node_flow[id.index()] = Some(self.current_flow);
            }
            Node::KeywordExpression(keyword) => match keyword.kind {
                SyntaxKind::ThisKeyword => {
                    self.seen_this_keyword = true;
                    self.node_flow[id.index()] = Some(self.current_flow);
                }
                SyntaxKind::SuperKeyword => {
                    self.node_flow[id.index()] = Some(self.current_flow);
                }
                _ => {}
            },
            // A qualified name only needs one inside `typeof X.Y`, where it
            // denotes a value that narrowing can have changed.
            Node::QualifiedName(_) => {
                if self.is_part_of_type_query(id) {
                    self.node_flow[id.index()] = Some(self.current_flow);
                }
            }
            Node::PropertyAccessExpression(_) | Node::ElementAccessExpression(_) => {
                if is_narrowable_reference(node) {
                    self.node_flow[id.index()] = Some(self.current_flow);
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
            self.node_flow[id.index()] = None;
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
            self.node_flow[id.index()] = Some(self.current_flow);
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
    // Every function below is a direct port; they are unreachable until the
    // parser sets `NodeFlags::OPTIONAL_CHAIN` (see `narrowing::is_optional_chain`).

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
                if self.is_block_or_catch_scoped(id) {
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
        classify(node)
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
    fn locals_owner(&self, flags: SymbolFlags) -> NodeId {
        if flags.intersects(SymbolFlags::BLOCK_SCOPED_VARIABLE | SymbolFlags::CLASS) {
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
        match self.nodes.kind(self.container) {
            SyntaxKind::ModuleDeclaration => self.export_context || self.has_export_modifier(node),
            // `declareSourceFileMember` routes through `declareModuleMember` only
            // for an external module. A script's top-level `export` — which the
            // parser accepts and the checker rejects — has nothing to export
            // from, so it stays a plain local.
            SyntaxKind::SourceFile => {
                self.is_module && (self.export_context || self.has_export_modifier(node))
            }
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
        self.node_symbols[file.index()] = Some(symbol);
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
                        && access_name(left).is_some()
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
                let symbol = self.declare_into(
                    Destination::Exports,
                    file,
                    Some(module),
                    INTERNAL_EXPORT_EQUALS,
                    flags,
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
                let name = access_name(binary.left?)?;
                let flags = if expression_is_alias(binary.right) {
                    SymbolFlags::ALIAS
                } else {
                    SymbolFlags::FUNCTION_SCOPED_VARIABLE
                };
                let module = self.module_symbol?;
                let file = self.file_node;
                Some(self.declare_into(Destination::Exports, file, Some(module), name, flags, id))
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
                    container: self.container,
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
                let name = string_or_numeric_text(Node::from(call.arguments[1]))?;
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
            Node::BinaryExpression(binary) => access_name(binary.left?),
            Node::CallExpression(call) => string_or_numeric_text(Node::from(call.arguments[1])),
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
        self.node_symbols[id.index()] = Some(declared);
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
                let name = access_name(expression)?;
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
            let exported = self
                .symbols
                .get(local)
                .declarations
                .first()
                .and_then(|declaration| self.node_symbols[declaration.index()]);
            return Some(exported.unwrap_or(local));
        }
        let owner = self.node_symbols[container.index()]?;
        self.symbols.get(owner).exports.get(name).copied()
    }

    /// The symbol an expando property should hang off, given what the name
    /// resolved to.
    ///
    /// Upstream's `getInitializerSymbol`. A function declaration is itself the
    /// answer; a `const f = function () {}` is not — the properties belong to the
    /// function expression's symbol, not to the variable's.
    fn initializer_symbol(&self, symbol: Option<SymbolId>) -> Option<SymbolId> {
        let symbol = symbol?;
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
                .and_then(|initializer| self.node_symbols[initializer.index()]),
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
        let name = access_name(left)?;
        let owner = self.owner?;

        // Upstream files a *static* member in the class's exports and an
        // instance member in its members. This binder puts both in `members`
        // — `static x = 1` already goes there — so `this.x` follows suit; the
        // divergence is pre-existing and uniform.
        if let Some(existing) = self.symbols.get(owner).members.get(name).copied() {
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
        self.symbols.get_mut(owner).members.insert(name, symbol);
        Some(symbol)
    }

    /// The declared name, threading the file text the JSX case needs.
    fn declaration_name(&self, node: Node<'a>) -> Option<&'a str> {
        declaration_name(node, self.nodes, self.source)
    }

    /// Create a symbol for `node` if it declares one.
    fn declare(&mut self, node: Node<'a>, id: NodeId) -> Option<SymbolId> {
        // In a JavaScript file an assignment to a property can *be* a
        // declaration. Checked before anything else, because the node kinds
        // involved — a binary expression — declare nothing otherwise.
        if let Some(kind) = self.assignment_declaration_kind(node) {
            let symbol = self.bind_assignment_declaration(kind, node, id);
            if let Some(symbol) = symbol {
                self.node_symbols[id.index()] = Some(symbol);
            }
            return symbol;
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
            self.node_symbols[id.index()] = Some(symbol);
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
        if let Some(computed) = dynamic_name(node) {
            let symbol = self.symbols.create(INTERNAL_COMPUTED, flags);
            self.symbols.get_mut(symbol).declarations.push(id);
            if flags.intersects(SymbolFlags::ENUM_MEMBER | SymbolFlags::CLASS_MEMBER) {
                self.symbols.get_mut(symbol).parent = self.owner;
            }
            if flags.intersects(SymbolFlags::VALUE) {
                self.symbols.get_mut(symbol).value_declaration = Some(id);
            }
            self.node_symbols[id.index()] = Some(symbol);
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
                self.node_symbols[id.index()] = Some(exported);
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
            if let Some(name) = name {
                let local_owner = self.locals_owner(flags);
                let export_value = if flags.intersects(SymbolFlags::VALUE) {
                    SymbolFlags::EXPORT_VALUE
                } else {
                    SymbolFlags::empty()
                };
                self.declare_into(
                    Destination::Locals,
                    local_owner,
                    self.owner,
                    name,
                    export_value,
                    id,
                );
            }

            let exported = self.declare_into(
                Destination::Exports,
                container,
                self.owner,
                export_name,
                flags,
                id,
            );
            // The export symbol is the node's symbol, so a class's members land
            // on the thing `M.C` names rather than on the shadow local.
            self.node_symbols[id.index()] = Some(exported);
            return Some(exported);
        }

        // A declaration whose name the parser could not read still gets a symbol
        // — `declareSymbolEx` creates one called `__missing`, in no symbol table.
        // Without it the declaration has none at all, and anything nested inside
        // it is filed on whatever container happened to be enclosing.
        let Some(name) = name else {
            let symbol = self.symbols.create(INTERNAL_MISSING, SymbolFlags::empty());
            self.symbols.get_mut(symbol).declarations.push(id);
            self.node_symbols[id.index()] = Some(symbol);
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
        let symbol = self.declare_into(destination, table_owner, self.owner, name, flags, id);
        self.node_symbols[id.index()] = Some(symbol);
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
            self.node_symbols[id.index()] = Some(property);
            return Some(property);
        }
        Some(symbol)
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
            if flags.excludes().intersects(existing_flags) {
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
                let message = if enum_conflict {
                    &messages::ENUM_DECLARATIONS_CAN_ONLY_MERGE_WITH_NAMESPACE_OR_OTHER_ENUM_DECLARATIONS
                } else if existing_flags.intersects(SymbolFlags::BLOCK_SCOPED_VARIABLE) {
                    &messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE_0
                } else {
                    &messages::DUPLICATE_IDENTIFIER_0
                };

                let report = |binder: &mut Self, at: NodeId| {
                    let span = binder.declaration_name_span(at);
                    binder.diagnostics.push(if enum_conflict {
                        Diagnostic::new(message, span)
                    } else {
                        Diagnostic::with_args(message, span, [name.to_string()])
                    });
                };
                let previous: Vec<NodeId> = self.symbols.get(existing).declarations.to_vec();
                report(self, declaration);
                for earlier in previous {
                    report(self, earlier);
                }
                // Still merge, so the checker has one symbol to resolve
                // against rather than a hole. Upstream does the same.
            }
            self.symbols.get_mut(existing).flags |= flags;
            existing
        } else {
            let created = self.symbols.create(name, flags);
            match destination {
                Destination::Locals => {
                    self.locals.entry(table_owner).or_default().insert(name, created);
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

        let entry = self.symbols.get_mut(symbol);
        entry.declarations.push(declaration);
        if entry.value_declaration.is_none() && flags.intersects(SymbolFlags::VALUE) {
            entry.value_declaration = Some(declaration);
        }
        symbol
    }
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
        Node::TypeLiteralNode(_) | Node::MappedTypeNode(_) => {
            (INTERNAL_TYPE, SymbolFlags::TYPE_LITERAL)
        }
        Node::JsxAttributes(_) => (INTERNAL_JSX_ATTRIBUTES, SymbolFlags::OBJECT_LITERAL),
        // A *named* class expression declares its name into the enclosing block,
        // as upstream's `bindClassLikeDeclaration` does; only an unnamed one is
        // anonymous.
        Node::ClassExpression(class) if class.name.is_none() => {
            (INTERNAL_CLASS, SymbolFlags::CLASS)
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

/// The modifier list of a declaration that can carry `export`.
fn modifiers_of(node: Node<'_>) -> Option<&[tsr_ast::ModifierLike<'_>]> {
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

fn has_declare(modifiers: &[tsr_ast::ModifierLike<'_>]) -> bool {
    has_modifier(modifiers, SyntaxKind::DeclareKeyword)
}

fn has_modifier(modifiers: &[tsr_ast::ModifierLike<'_>], kind: SyntaxKind) -> bool {
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
fn is_external_module(file: &SourceFile<'_>) -> bool {
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
    if access_name(Expression::PropertyAccessExpression(callee)) != Some("defineProperty") {
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
fn dynamic_name(node: Node<'_>) -> Option<&tsr_ast::ComputedPropertyName<'_>> {
    let computed = computed_property_name(node)?;
    computed_name(computed).is_none().then_some(computed)
}

/// The text of a string or numeric literal, which is what a statically-named
/// `Object.defineProperty` call and a bracketed access carry.
fn string_or_numeric_text(node: Node<'_>) -> Option<&str> {
    match skip_parentheses(node) {
        Node::StringLiteral(literal) => Some(literal.text),
        Node::NumericLiteral(literal) => Some(literal.text),
        Node::NoSubstitutionTemplateLiteral(literal) => Some(literal.text),
        _ => None,
    }
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
                && access_name(expression).is_some()
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
fn access_name(expression: Expression<'_>) -> Option<&str> {
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

/// Whether `expression` is the identifier `exports` (`IsExportsIdentifier`).
fn is_exports_identifier(expression: Expression<'_>) -> bool {
    matches!(expression, Expression::Identifier(identifier) if identifier.text == "exports")
}

/// Whether `expression` is `module.exports` (`IsModuleExportsAccessExpression`).
fn is_module_exports_access(expression: Expression<'_>) -> bool {
    let Some(target) = access_target(expression) else { return false };
    matches!(target, Expression::Identifier(identifier) if identifier.text == "module")
        && access_name(expression) == Some("exports")
}

/// Whether a file is a declaration file, and so an ambient context.
///
/// Upstream reads `file.IsDeclarationFile`, which the parser sets from the same
/// suffix test. `.d.ts`, `.d.mts`, `.d.cts`, and the `.d.*.ts` form used by
/// generated libraries all count.
fn is_declaration_file(file_name: &str) -> bool {
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
        Some(Expression::Identifier(_)) => true,
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
/// `None` for nodes that declare nothing, which is most of them.
fn classify(node: Node<'_>) -> Option<(SymbolFlags, Destination)> {
    use Destination as D;
    use SymbolFlags as S;
    Some(match node {
        Node::FunctionDeclaration(_) => (S::FUNCTION, D::Locals),
        Node::ClassDeclaration(_) | Node::ClassExpression(_) => (S::CLASS, D::Locals),
        Node::InterfaceDeclaration(_) => (S::INTERFACE, D::Locals),
        Node::TypeAliasDeclaration(_) => (S::TYPE_ALIAS, D::Locals),
        Node::EnumDeclaration(_) => (S::REGULAR_ENUM, D::Locals),
        Node::ModuleDeclaration(_) => (S::VALUE_MODULE, D::Locals),
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
        _ => None,
    }
}

fn declaration_name<'a>(node: Node<'a>, nodes: &NodeTable, source: &'a str) -> Option<&'a str> {
    fn from_property_name(name: tsr_ast::PropertyName<'_>) -> Option<&str> {
        match name {
            tsr_ast::PropertyName::Identifier(i) => Some(i.text),
            tsr_ast::PropertyName::StringLiteral(s) => Some(s.text),
            tsr_ast::PropertyName::NumericLiteral(n) => Some(n.text),
            tsr_ast::PropertyName::PrivateIdentifier(p) => Some(p.text),
            tsr_ast::PropertyName::ComputedPropertyName(computed) => computed_name(computed),
            tsr_ast::PropertyName::BigIntLiteral(b) => Some(b.text),
            tsr_ast::PropertyName::NoSubstitutionTemplateLiteral(t) => Some(t.text),
        }
    }

    match node {
        Node::FunctionDeclaration(n) => n.name.map(|i| i.text),
        Node::ClassDeclaration(n) => n.name.map(|i| i.text),
        Node::ClassExpression(n) => n.name.map(|i| i.text),
        Node::InterfaceDeclaration(n) => n.name.map(|i| i.text),
        Node::TypeAliasDeclaration(n) => n.name.map(|i| i.text),
        Node::EnumDeclaration(n) => n.name.map(|i| i.text),
        Node::TypeParameterDeclaration(n) => n.name.map(|i| i.text),
        Node::ModuleDeclaration(n) => n.name.map(module_name),
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
/// Upstream also handles a signed numeric literal (`[-1]`), building the name by
/// concatenating the operator with the operand. That needs an owned string where
/// every name here is a borrow from the source, and `[-1]` as a property name is
/// vanishingly rare, so it is left late-bound instead.
fn computed_name<'a>(computed: &'a tsr_ast::ComputedPropertyName<'a>) -> Option<&'a str> {
    match computed.expression? {
        Expression::StringLiteral(literal) => Some(literal.text),
        Expression::NoSubstitutionTemplateLiteral(literal) => Some(literal.text),
        Expression::NumericLiteral(literal) => Some(literal.text),
        _ => None,
    }
}

fn module_name(name: tsr_ast::ModuleName<'_>) -> &str {
    match name {
        tsr_ast::ModuleName::Identifier(i) => i.text,
        tsr_ast::ModuleName::StringLiteral(s) => s.text,
    }
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
