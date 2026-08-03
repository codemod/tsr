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
    BindResult, NodeFacts,
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
    /// The container symbol's `exports` — module or namespace.
    ///
    /// Not yet constructed: routing an `export`ed declaration here needs module
    /// versus script detection, which needs module resolution. Kept because the
    /// table it targets already exists on `Symbol` and the destination is where
    /// that decision will land.
    #[expect(dead_code, reason = "awaiting export handling; see lib.rs")]
    Exports,
}

/// A `label:` in scope, and where `break label` / `continue label` go.
///
/// Upstream threads these as a linked list (`ActiveLabel.next`) allocated per
/// labelled statement. A stack is the same structure without the allocations;
/// `labels_base` hides the labels of an enclosing function, which upstream does
/// by setting the list head to `nil` and restoring it.
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

    pub(crate) fn bind_source_file(mut self, file: &'a SourceFile<'a>) -> BindResult<'a> {
        let root = Node::SourceFile(file);
        let root_id = root.node_id().expect("the source file is registered");

        // A file's own symbol exists only for a module; a script's declarations
        // are globals. Distinguishing the two needs module resolution, which does
        // not exist, so this binds every file as a script and records the gap.
        self.container = root_id;
        self.block = root_id;
        self.bind(root);

        BindResult {
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
    /// Wraps the kind-only [`classify`] with the one case that needs more than
    /// the node: a variable's scope depends on whether it was written `var`,
    /// `let`, or `const`, and all three produce the same node kind. Upstream keeps
    /// that in node flags and so do we, so it is reached through the side table
    /// rather than the tree.
    fn classify(&self, node: Node<'a>, id: NodeId) -> Option<(SymbolFlags, Destination)> {
        if matches!(node, Node::VariableDeclaration(_)) {
            let list_flags =
                self.nodes.parent(id).map_or_else(NodeFlags::empty, |list| self.nodes.flags(list));
            let block_scoped =
                list_flags.intersects(NodeFlags::LET | NodeFlags::CONST | NodeFlags::USING);
            return Some((
                if block_scoped {
                    SymbolFlags::BLOCK_SCOPED_VARIABLE
                } else {
                    SymbolFlags::FUNCTION_SCOPED_VARIABLE
                },
                Destination::Locals,
            ));
        }
        classify(node)
    }

    /// Create a symbol for `node` if it declares one.
    fn declare(&mut self, node: Node<'a>, id: NodeId) -> Option<SymbolId> {
        let (flags, destination) = self.classify(node, id)?;
        let name = declaration_name(node)?;

        let table_owner = match destination {
            // `var` and functions go to the nearest *function* scope; `let`,
            // `const`, and classes go to the nearest block. This is the whole of
            // `var` hoisting.
            Destination::Locals => {
                if flags.intersects(SymbolFlags::BLOCK_SCOPED_VARIABLE | SymbolFlags::CLASS) {
                    self.block
                } else {
                    self.container
                }
            }
            Destination::Members | Destination::Exports => self.container,
        };

        let symbol = self.declare_into(destination, table_owner, name, flags, id);
        self.node_symbols[id.index()] = Some(symbol);
        Some(symbol)
    }

    /// Add `name` to the appropriate table, merging with an existing symbol where
    /// TypeScript allows it and reporting a duplicate where it does not.
    fn declare_into(
        &mut self,
        destination: Destination,
        table_owner: NodeId,
        name: &'a str,
        flags: SymbolFlags,
        declaration: NodeId,
    ) -> SymbolId {
        let existing = match destination {
            Destination::Locals => {
                self.locals.get(&table_owner).and_then(|table| table.get(name).copied())
            }
            Destination::Members => {
                self.owner.and_then(|owner| self.symbols.get(owner).members.get(name).copied())
            }
            Destination::Exports => {
                self.owner.and_then(|owner| self.symbols.get(owner).exports.get(name).copied())
            }
        };

        let symbol = if let Some(existing) = existing {
            let existing_flags = self.symbols.get(existing).flags;
            if flags.excludes().intersects(existing_flags) {
                // A genuine redeclaration. Upstream reports on every
                // declaration involved, not only the second.
                self.diagnostics.push(Diagnostic::with_args(
                    &messages::DUPLICATE_IDENTIFIER_0,
                    self.nodes.span(declaration),
                    [name.to_string()],
                ));
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
                    if let Some(owner) = self.owner {
                        self.symbols.get_mut(owner).members.insert(name, created);
                        self.symbols.get_mut(created).parent = Some(owner);
                    }
                }
                Destination::Exports => {
                    if let Some(owner) = self.owner {
                        self.symbols.get_mut(owner).exports.insert(name, created);
                        self.symbols.get_mut(created).parent = Some(owner);
                    }
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
        Node::PropertyDeclaration(_)
        | Node::PropertySignatureDeclaration(_)
        | Node::PropertyAssignment(_)
        | Node::ShorthandPropertyAssignment(_) => (S::PROPERTY, D::Members),
        Node::MethodDeclaration(_) | Node::MethodSignatureDeclaration(_) => (S::METHOD, D::Members),
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

        _ => return None,
    })
}

/// The declared name, if the node has a simple one.
///
/// `None` for computed names (`[expr]`) and for destructuring patterns. Both
/// declare symbols in TypeScript — a binding pattern declares one per element —
/// and neither is handled yet; see the note in `lib.rs`.
fn declaration_name(node: Node<'_>) -> Option<&str> {
    fn from_property_name(name: tsr_ast::PropertyName<'_>) -> Option<&str> {
        match name {
            tsr_ast::PropertyName::Identifier(i) => Some(i.text),
            tsr_ast::PropertyName::StringLiteral(s) => Some(s.text),
            tsr_ast::PropertyName::NumericLiteral(n) => Some(n.text),
            tsr_ast::PropertyName::PrivateIdentifier(p) => Some(p.text),
            tsr_ast::PropertyName::ComputedPropertyName(_) => None,
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
        _ => None,
    }
}

fn module_name(name: tsr_ast::ModuleName<'_>) -> &str {
    match name {
        tsr_ast::ModuleName::Identifier(i) => i.text,
        tsr_ast::ModuleName::StringLiteral(s) => s.text,
    }
}

fn binding_name(name: BindingName<'_>) -> Option<&str> {
    match name {
        BindingName::Identifier(i) => Some(i.text),
        // Destructuring declares one symbol per element; not handled yet.
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
