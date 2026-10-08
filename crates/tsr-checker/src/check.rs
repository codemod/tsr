//! The **check traversal** — the second road into the checker, the one that
//! reports.
//!
//! # Why this module exists, and why it is not a hook on the query road
//!
//! [ADR-0040](../../../docs/adr/0040-diagnostics-come-from-a-check-traversal-and-assignability-gets-a-reporting-twin.md)
//! decisions (1) and (2), which are the two the ADR's own falsifiers left
//! standing. Upstream has two entry points into one engine:
//!
//! | entry | purpose | here |
//! |---|---|---|
//! | `checkSourceFile` → `checkSourceElement` (`checker.go:2196`, `:2241`) | reports diagnostics | **this module** |
//! | `getTypeOfNode` (`checker.go:31927`) | answers "what type is this node" | `types_producer::type_at_location` |
//!
//! Until this file existed the port had built the query road and none of the
//! traversal road, which is the whole reason `diagnostics` read **80/5,488
//! (1.46%)** while `checker_types` went 36% → 73.65%. A diagnostic is an eager
//! **side effect of a walk**, not a return value a consumer can ask for, so no
//! amount of work on the query road can produce one.
//!
//! # What it walks today, and why so little
//!
//! Only the declarations that carry a **module specifier**, plus the module
//! bodies that can contain them. That is deliberately far short of upstream's
//! `checkSourceElement`, and the reason is `docs/conventions.md`'s rule that a
//! diagnostic emitter is the consumer that *acts on the negative*: every node
//! kind this walk learns to visit is a new opportunity to report something
//! upstream does not, and under the suite's **exact multiset equality** an
//! invented diagnostic fails a case exactly as a missing one does — except that
//! it can also break a case that passes today.
//!
//! So the traversal grows one rule at a time, each sized against
//! `crates/tsr-conformance/examples/diaggap.rs` before it is written and scored
//! against a registered bar afterwards. See
//! `docs/architecture/checker-notes-diag2.md`.
//!
//! # Diagnostics carry the file they are in
//!
//! Upstream's `ast.DiagnosticsCollection` keys by file because a `Diagnostic`
//! there holds its `*ast.SourceFile`. [`tsr_diagnostics::Diagnostic`] holds a
//! [`tsr_core::Span`] and nothing else, and under
//! [ADR-0034](../../../docs/adr/0034-a-program-needs-one-identity-space.md) one
//! `NodeTable` spans every file of a program — so a bare span is ambiguous
//! across files and the collection stores `(source file, diagnostic)` pairs.
//! Getting this wrong would put every cross-file diagnostic on the wrong line of
//! the wrong unit, which is the failure mode that looks like a checker bug for a
//! week.

use tsr_ast::{
    ClassElement, InterfaceDeclaration, ModifierLike, ModuleReference, Node, NodeId, SyntaxKind,
    TypeLiteralNode,
};
use tsr_binder::{NodeFacts, SymbolFlags};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;
use crate::flags::TypeFlags;
use crate::relater::{Relation, Ternary};
use crate::types::TypeId;

/// The written name of a class member, for the kinds an `abstract` member can
/// take. Wider than [`class_member_shape`], which omits methods because its own
/// caller asks a property-versus-accessor question. §761.
fn member_name_text(member: &tsr_ast::ClassElement<'_>) -> Option<String> {
    let name = match member {
        tsr_ast::ClassElement::PropertyDeclaration(p) => p.name,
        tsr_ast::ClassElement::MethodDeclaration(m) => m.name,
        tsr_ast::ClassElement::GetAccessorDeclaration(a) => a.name,
        tsr_ast::ClassElement::SetAccessorDeclaration(a) => a.name,
        _ => return None,
    };
    match name {
        tsr_ast::PropertyName::Identifier(identifier) => Some(identifier.text.to_string()),
        tsr_ast::PropertyName::StringLiteral(literal) => Some(literal.text.to_string()),
        _ => None,
    }
}

/// What the caller knows about a file that the tree does not say.
///
/// Both fields stand in for facts upstream's parser records and this port's does
/// not — `NodeFlagsAmbient` for the first, and for the second the simple fact
/// that upstream is comparing its *own* recovery against baselines produced by
/// it. Each is documented at its reader; they travel together because a caller
/// that knows one knows the other.
#[derive(Debug, Clone, Copy)]
pub struct FileContext {
    /// Upstream's `node.Flags & ast.NodeFlagsAmbient` for the file: is this a
    /// declaration file?
    pub ambient: bool,
    /// Did the parser report anything in this file?
    pub has_parse_errors: bool,
}

/// The walk's stack budget, matching `tsr-binder`'s `MAX_DEPTH`.
///
/// Not a correctness bound: a file deeper than this loses diagnostics from the
/// deep part, which is a missing diagnostic and therefore a *failed* case rather
/// than a wrong one. ADR-0029's budget is what makes a fixed number necessary
/// here at all — Go grows a goroutine's stack on demand and upstream needs no
/// equivalent.
const MAX_CHECK_DEPTH: u32 = 1_000;

/// Children held without allocating, before falling back to a `Vec`.
///
/// The walk cannot borrow `self.node_map` across a `&mut self` recursion, so the
/// child ids are collected first. Almost every node has few children; the ones
/// that do not are statement lists and argument lists, and paying an allocation
/// for those alone keeps the common node allocation-free.
const INLINE_CHILDREN: usize = 8;

/// How far an alias chain is followed before the answer is treated as unknown.
///
/// Upstream's `resolveAlias` recurses with a cycle guard; this is the bound. §692.
const MAX_ALIAS_HOPS: usize = 8;

impl Checker<'_, '_> {
    /// Report every diagnostic this port can produce for one source file.
    ///
    /// `checkSourceFile` (`checker.go:2196`) → `checkSourceElements` →
    /// `checkSourceElement` (`checker.go:2241`). Upstream's version also runs
    /// grammar checks and deferred nodes; neither is ported.
    ///
    /// Idempotent by construction is *not* claimed: calling this twice on one
    /// file appends its diagnostics twice, exactly as upstream's would without
    /// its `checkSourceFileWorker` memo. Callers run it once per file.
    /// `in_ambient_context` is upstream's `node.Flags & ast.NodeFlagsAmbient`,
    /// **passed in because this port's parser never sets that flag** — it is
    /// declared in `tsr_ast::NodeFlags` and written nowhere
    /// (`grep -rn AMBIENT crates/tsr-parser/src` is empty). Upstream's parser
    /// sets it as a context flag on every node of a declaration file and inside
    /// every `declare`d declaration, and the checker reads it in ~40 places.
    ///
    /// Here the caller supplies the *file-level* half — is this a `.d.ts` — and
    /// [`Checker::check_source_element`] carries the `declare`-modifier half
    /// down the walk. That is a faithful reproduction of the effect and an
    /// unfaithful reproduction of the mechanism; when the parser learns to set
    /// the flag this parameter goes away and every reader gets it for free.
    /// `bd tsr-o9tl` carries it.
    ///
    /// It was not optional: reading the unset flag instead reported TS2564 on
    /// **every property of every `declare class` in the corpus**, ~25 of the 86
    /// wrong lines that measurement produced.
    pub fn check_source_file(&mut self, file: NodeId, context: FileContext) {
        #[cfg(feature = "work-trace")]
        let _work = self.trace_file_work(file);
        // Program-scoped, not file-scoped: the binder's cross-file merge
        // conflicts are reported once, on whichever file is checked first, and
        // each diagnostic carries its own declaration's file. §159.
        self.report_merge_conflicts();
        self.file_has_parse_errors = context.has_parse_errors;
        self.file_is_ambient = context.ambient;
        self.reset_unused_state();
        self.check_top_level_declare_modifiers(file, context.ambient);
        self.check_node(file, context.ambient, 0);
        // `checkSourceFile` (`checker.go:2220`) runs the unused-identifier pass
        // *after* the file's own check, because it reads reference marks the
        // check produces. Here the marks come from the walk that just finished,
        // so the ordering constraint is the same one.
        self.check_exported_redeclarations(file);
        self.check_unused_identifiers();
    }

    /// One node: its own rules, then its children.
    ///
    /// # Why a generic child walk rather than a typed `checkSourceElement`
    ///
    /// Upstream's `checkSourceElement` (`checker.go:2241`) is a 120-arm switch
    /// that dispatches each kind to its own `checkXxx`, and each `checkXxx`
    /// recurses into exactly the children that kind checks. Reproducing that
    /// shape means writing 120 arms before the first rule beyond declarations
    /// can fire.
    ///
    /// This walks every registered child via
    /// [`tsr_ast::for_each_child_id`] and lets each rule decide, at the node it
    /// cares about, whether it applies. The two produce the same *set of visited
    /// nodes* for the rules ported so far; they differ in that upstream's order
    /// is a checking order with deferred work, and this one is document order
    /// with none. Nothing ported yet depends on either.
    ///
    /// The consequence to accept, and it is the one that matters: **a rule here
    /// sees nodes upstream's corresponding `checkXxx` would never be handed**,
    /// so every rule must carry its own position test rather than relying on the
    /// walk to have filtered for it. [`Checker::is_value_reference`] is that test
    /// for identifiers and it is written as an *allow*-list precisely because a
    /// missing arm then costs silence rather than a false positive.
    ///
    /// `depth` is bounded for the same reason `tsr-binder`'s walk is
    /// ([ADR-0029](../../../docs/adr/0029-stack-discipline-is-guards-plus-a-budget.md)):
    /// the corpus contains files written to break compilers, and
    /// `compiler/binderBinaryExpressionStress` is 4,971 operands of one
    /// left-leaning chain.
    fn check_node(&mut self, node: NodeId, ambient: bool, depth: u32) {
        if depth > MAX_CHECK_DEPTH {
            return;
        }
        let Some(typed) = self.node_map.get(node) else { return };
        self.check_construct_emit_helpers(node, typed);
        let ambient = match typed {
            Node::ImportDeclaration(declaration) => {
                self.check_import_in_namespace(
                    node,
                    declaration.module_specifier.and_then(|s| s.node_id()),
                );
                // `import "x"` with no clause is a **side-effect import**, and
                // upstream gives it its own message — `checkImportDeclaration`'s
                // `else if` branch at `checker.go:5321`, guarded by
                // `NoUncheckedSideEffectImports.IsTrueOrUnknown()`, which is
                // true when the option is unset. Same site, same resolution,
                // a different code.
                let side_effect = declaration.import_clause.is_none();
                // `checker.go:5321`'s guard: with the option explicitly off,
                // upstream does not resolve the specifier at all, so there is no
                // diagnostic of any code to report there.
                if !side_effect || self.no_unchecked_side_effect_imports {
                    self.check_module_specifier(
                        node,
                        declaration.module_specifier.and_then(|s| s.node_id()),
                        side_effect,
                    );
                }
                ambient
            }
            Node::ExportDeclaration(declaration) => {
                self.check_module_specifier(
                    node,
                    declaration.module_specifier.and_then(|s| s.node_id()),
                    false,
                );
                ambient
            }
            Node::ImportEqualsDeclaration(declaration) => {
                self.check_circular_import_alias(node);
                // **`import a = b.c` carries TS2694 too**, under a wider
                // meaning: an alias may name a value, where a type reference may
                // not. §559.
                if let Some(tsr_ast::ModuleReference::QualifiedName(name)) =
                    declaration.module_reference
                    && let Some(name) = name.node_id
                {
                    self.check_qualified_type_name_at(
                        name,
                        true,
                        SymbolFlags::VALUE
                            | SymbolFlags::TYPE
                            | SymbolFlags::NAMESPACE
                            | SymbolFlags::ALIAS,
                    );
                }
                // Only the `require("x")` spelling names a module; `import a = b.c`
                // is an entity-name alias and resolves through the scope.
                if let Some(tsr_ast::ModuleReference::ExternalModuleReference(reference)) =
                    declaration.module_reference
                {
                    self.check_import_in_namespace(
                        node,
                        reference.expression.and_then(|e| e.node_id()),
                    );
                    self.check_module_specifier(
                        node,
                        reference.expression.and_then(|e| e.node_id()),
                        false,
                    );
                }
                ambient
            }
            Node::ClassDeclaration(declaration) => {
                self.check_exports_on_merged_declarations(node);
                self.check_class_function_merge(node);
                self.check_super_call_is_first(node);
                self.check_derived_constructor_calls_super(node);
                self.check_static_side_assignability(node);
                self.check_extends_primitive(node);
                // `declare class C { x: number }` puts every member in an
                // ambient context, which is where upstream's flag would already
                // be set on the members themselves.
                let ambient =
                    ambient || has_modifier(declaration.modifiers, SyntaxKind::DeclareKeyword);
                self.check_property_initialization(declaration.members, ambient);
                self.check_heritage_conformance(node);
                self.check_members_for_override_modifier(node, ambient);
                self.check_index_constraints(node);
                self.check_duplicate_index_signatures(node);
                ambient
            }
            Node::ClassExpression(declaration) => {
                let ambient =
                    ambient || has_modifier(declaration.modifiers, SyntaxKind::DeclareKeyword);
                self.check_property_initialization(declaration.members, ambient);
                // §764's sweep: the rule's entry destructures `ClassExpression`
                // and only `ClassDeclaration` ever reached it.
                self.check_extends_primitive(node);
                // `checkClassLikeDeclaration` (`checker.go:4293`) runs for
                // class expressions as well as declarations.
                self.check_heritage_conformance(node);
                self.check_members_for_override_modifier(node, ambient);
                self.check_index_constraints(node);
                self.check_object_type_for_duplicate_declarations(node);
                // `checkClassOrInterfaceForDuplicateIndexSignatures`
                // (`checker.go:4389`), from `checkClassLikeDeclaration`.
                self.check_duplicate_index_signatures(node);
                ambient
            }
            // `declare module "m" { … }` and `declare namespace N { … }` are
            // ambient contexts, and so is an *ambient* module's body whether or
            // not the keyword is repeated inside it.
            Node::ModuleDeclaration(declaration) => {
                self.check_grammar_module_element_context(node);
                self.check_ambient_module_relative_name(node);
                self.check_module_keyword_deprecated(node);
                // `!inAmbientContext && IsStringLiteral(node.Name())`
                // (`checker.go:5151`), reported on the **name**. §807.
                if !ambient
                    && !self.file_is_ambient
                    && !has_modifier(declaration.modifiers, SyntaxKind::DeclareKeyword)
                    && let Some(tsr_ast::ModuleName::StringLiteral(name)) = declaration.name
                    && let Some(at) = name.node_id
                    && let Some(file) = self.source_file_of_for_diagnostics(at)
                {
                    let span = self.nodes.span(at);
                    self.report(
                        file,
                        Diagnostic::new(&messages::ONLY_AMBIENT_MODULES_CAN_USE_QUOTED_NAMES, span),
                    );
                }
                self.check_global_augmentation_position(node);
                self.check_ambient_module_export_modifier(node);
                self.check_nested_ambient_module(node);
                self.check_namespace_merge_position(node, ambient);
                if let Some(name) = declaration.name.and_then(|n| n.node_id()) {
                    self.check_module_augmentation_name(node, name);
                }
                // `checkModuleDeclaration` (`checker.go:5161`), reached only
                // past `checkGrammarModuleElementContext`'s bail-out
                // (`:5146`): a module declaration in an illegal context is not
                // checked further.
                if self.nodes.parent(node).is_some_and(|parent| {
                    matches!(
                        self.nodes.kind(parent),
                        SyntaxKind::SourceFile
                            | SyntaxKind::ModuleBlock
                            | SyntaxKind::ModuleDeclaration
                    )
                }) {
                    self.check_exports_on_merged_declarations(node);
                }
                ambient
                    || has_modifier(declaration.modifiers, SyntaxKind::DeclareKeyword)
                    || self.is_ambient_module_node(node)
            }
            Node::VariableStatement(statement) => {
                self.check_block_scoped_statement_container(node);
                ambient || has_modifier(statement.modifiers, SyntaxKind::DeclareKeyword)
            }
            Node::FunctionDeclaration(declaration) => {
                self.check_class_function_merge(node);
                self.check_function_or_constructor_symbol(node, ambient);
                let ambient =
                    ambient || has_modifier(declaration.modifiers, SyntaxKind::DeclareKeyword);
                self.check_implicit_any_parameters(node, ambient);
                self.check_implicit_any_return(node, ambient);
                ambient
            }
            Node::InterfaceDeclaration(_) => {
                self.check_interface_extends_entity_name(node);
                self.check_exports_on_merged_declarations(node);
                self.check_private_name_in_object_literal(node);
                // TS2320, TS2430 and the index constraints, once per symbol.
                self.check_heritage_conformance(node);
                self.check_duplicate_index_signatures(node);
                ambient
            }
            Node::EnumDeclaration(declaration) => {
                self.check_exports_on_merged_declarations(node);
                self.check_enum_first_member_initializers(node);
                for member in declaration.members {
                    if let Some(at) = member.node_id {
                        self.check_enum_member_name(at);
                        self.check_computed_enum_member_initializer(at, ambient);
                        self.check_const_enum_member_value(at);
                        self.check_enum_member_forward_references(at);
                        self.check_enum_member_auto_value(at);
                        self.check_enum_member_isolated_modules(at);
                    }
                }
                self.check_reserved_enum_name(declaration);
                ambient || has_modifier(declaration.modifiers, SyntaxKind::DeclareKeyword)
            }
            Node::BreakStatement(statement) => {
                if !self.check_grammar_statement_in_ambient_context(node, ambient) {
                    self.check_break_or_continue(
                        node,
                        true,
                        statement.label.map(|label| label.text),
                        ambient,
                    );
                }
                ambient
            }
            Node::ContinueStatement(statement) => {
                if !self.check_grammar_statement_in_ambient_context(node, ambient) {
                    self.check_break_or_continue(
                        node,
                        false,
                        statement.label.map(|label| label.text),
                        ambient,
                    );
                }
                ambient
            }
            // The remaining twelve statement kinds whose upstream `checkXxx`
            // calls `checkGrammarStatementInAmbientContext` — `checker.go`
            // lines 2382, 2384, 3788, 3803, 3948, 3954, 3960, 4095, 4157, 4173,
            // 4211, 4227, 4236 and 7331. `checkBlock` calls it only for a
            // `Block`, never a `ModuleBlock` (`checker.go:3786`), and a
            // `VariableStatement` is deliberately absent: `declare var x` is
            // legal in an ambient context and upstream never asks.
            Node::Block(_)
            | Node::IfStatement(_)
            | Node::DoStatement(_)
            | Node::WhileStatement(_)
            | Node::ForStatement(_)
            | Node::ForInOrOfStatement(_)
            | Node::WithStatement(_)
            | Node::SwitchStatement(_)
            | Node::LabeledStatement(_)
            | Node::ThrowStatement(_)
            | Node::TryStatement(_)
            | Node::ExpressionStatement(_)
            | Node::EmptyStatement(_)
            | Node::DebuggerStatement(_) => {
                self.check_grammar_statement_in_ambient_context(node, ambient);
                ambient
            }
            Node::ParameterDeclaration(parameter) => {
                self.check_rest_parameter_type(node);
                self.check_optional_binding_pattern_parameter(node);
                self.check_optional_parameter_initializer(node);
                self.check_parameter_initializer_needs_body(node);
                self.check_parameter_property_position(node, parameter.modifiers);
                self.check_this_parameter_position(node);
                self.check_annotated_initializer(node, ambient);
                self.check_subsequent_declaration_type(node);
                ambient
            }
            Node::PropertySignatureDeclaration(_) => {
                self.check_implicit_any_member(node, ambient);
                self.check_subsequent_declaration_type(node);
                ambient
            }
            Node::GetAccessorDeclaration(accessor) => {
                self.check_get_accessor_returns(accessor, ambient);
                ambient
            }
            Node::PropertyDeclaration(property) => {
                // §271 built this rule and dispatched it only from
                // `PropertySignatureDeclaration`, so no *class* property ever
                // reached it — the shape §321 opened, at the dispatch. §379.
                self.check_implicit_any_member(node, ambient);
                self.check_ambient_initializer(
                    node,
                    property.initializer,
                    property.r#type,
                    ambient,
                );
                self.check_annotated_initializer(node, ambient);
                self.check_jsdoc_annotated_initializer(node, ambient);
                self.check_subsequent_declaration_type(node);
                ambient
            }
            // `checkVariableLikeDeclaration` runs for a binding element too,
            // and TS2481 is one of its arms: `var { x } = …` declares `x` on
            // the element. §718.
            Node::BindingElement(_) => {
                self.check_outer_scoped_variable(node);
                self.check_renamed_binding_element_in_signature(node);
                self.check_binding_element_initializer(node, ambient);
                self.check_binding_element_accessibility(node, ambient);
                self.check_subsequent_declaration_type(node);
                ambient
            }
            Node::VariableDeclaration(declaration) => {
                self.check_grammar_name_in_let_or_const(node);
                self.check_implicit_any_variable(node, ambient);
                self.check_exports_on_merged_declarations(node);
                self.check_implicit_any_binding_pattern(node);
                self.check_using_is_initialized(node);
                self.check_outer_scoped_variable(node);
                self.check_ambient_initializer(
                    node,
                    declaration.initializer,
                    declaration.r#type,
                    ambient,
                );
                self.check_subsequent_declaration_type(node);
                self.check_variable_like_declaration(node, declaration, ambient);
                self.check_using_declaration_initializer(node);
                self.check_jsdoc_annotated_initializer(node, ambient);
                self.check_empty_binding_pattern_source(node, declaration, ambient);
                self.check_const_is_initialized(node, declaration, ambient);
                ambient
            }
            Node::ReturnStatement(statement) => {
                // `IsSetAccessorDeclaration(container) && exprNode != nil`
                // (`checker.go:4111`). The outer condition there is satisfied by
                // `exprNode != nil` whenever this one is, so the rule reduces to
                // a `return` with an expression inside a set accessor — no type
                // and no signature. §809.
                if statement.expression.is_some()
                    && self
                        .nodes
                        .ancestors(node)
                        .find(|&a| self.is_function_like_or_static_block(a))
                        .is_some_and(|owner| self.nodes.kind(owner) == SyntaxKind::SetAccessor)
                    && let Some(file) = self.source_file_of_for_diagnostics(node)
                {
                    let span = self.error_span(node);
                    self.report(
                        file,
                        Diagnostic::new(&messages::SETTERS_CANNOT_RETURN_A_VALUE, span),
                    );
                }
                self.check_grammar_statement_in_ambient_context(node, ambient);
                self.check_return_container(node, ambient);
                self.check_return_statement(node, ambient);
                self.check_return_statement_implicit_returns(
                    node,
                    statement.expression.is_some(),
                    ambient,
                );
                ambient
            }
            Node::BinaryExpression(binary)
                if binary.operator_token.is_some_and(|t| is_numeric_binary_operator(t.kind)) =>
            {
                // `+`/`+=` with no result type returns `anyType` before
                // `checkAssignmentOperator` (`checker.go:12446`).
                let operands_ok = self.check_operator_operands(node, ambient)
                    & self.check_arithmetic_operand_types(node, ambient);
                // **A `match` is exclusive and this arm comes first.**
                // `is_numeric_binary_operator` includes the compound arithmetic
                // assignments (`-=`, `*=`, `/=`, `%=`, …), so those never reach
                // the assignment arm below and never saw
                // `check_reference_expression` — §528's ordering defect, in the
                // dispatch rather than in a rule. §545.
                // **`if leftOk && rightOk`** (`checker.go:12400`): a failed
                // arithmetic-operand check suppresses `checkAssignmentOperator`,
                // which is TS2364's site. `this *= value` is upstream's TS2362
                // alone and was TS2362 **and** TS2364 here. §741 wired the same
                // shape for the prefix increment; this is the binary arm. §861.
                if operands_ok
                    && binary.operator_token.is_some_and(|t| t.kind.is_assignment_operator())
                {
                    self.check_assignment_operator(node, binary, ambient);
                    self.check_reference_expression(node);
                }
                if operands_ok {
                    self.check_enum_member_overshift(node);
                }
                // The assignment arm's other rule, for the compound forms this
                // arm takes from it.
                self.check_private_accessor_is_writable(node);
                ambient
            }
            Node::BinaryExpression(binary)
                if binary.operator_token.is_some_and(|t| t.kind == SyntaxKind::CommaToken) =>
            {
                self.check_comma_left(node, binary.left.and_then(|left| left.node_id()));
                ambient
            }
            // **The dispatch is the guard.** §542 widened the rule to every
            // assignment operator and measured +0, because this arm still
            // admitted `=` alone — §380's "dispatch never called", for the
            // fourth time. `=`, `+=`, `&&=`, `||=` and `??=` reach
            // `checkAssignmentOperator` here (`checker.go:12458`, `:12506`,
            // `:12515`, `:12527`, `:12531`); the arithmetic compound forms
            // reach it from the arm above, behind `leftOk && rightOk`. §543.
            Node::BinaryExpression(binary)
                if binary.operator_token.is_some_and(|t| t.kind.is_assignment_operator()) =>
            {
                self.check_assignment_operator(node, binary, ambient);
                if binary.operator_token.is_some_and(|t| t.kind == SyntaxKind::EqualsToken) {
                    self.check_destructuring_assignment_targets(node);
                }
                self.check_private_accessor_is_writable(node);
                self.check_reference_expression(node);
                ambient
            }
            Node::MethodDeclaration(_) | Node::ConstructorDeclaration(_) => {
                self.check_function_or_constructor_symbol(node, ambient);
                self.check_implicit_any_parameters(node, ambient);
                self.check_implicit_any_return(node, ambient);
                // **A member's own `declare` is an ambient context for its
                // body.** The threading widens at `VariableStatement`,
                // `FunctionDeclaration`, `EnumDeclaration` and
                // `ModuleDeclaration` and nowhere else, so `class C { declare
                // Foo() { } }` reached `check_grammar_statement_in_ambient_context`
                // with `ambient == false` and its TS1183 never fired — even
                // though that function's own comment describes this shape.
                // §135's hazard, third sighting; §946.
                ambient || self.member_has_declare_modifier(node)
            }
            // §81. No walk arm claimed this kind before, which is what §49's
            // trap says to check before pricing a rule that measures zero.
            Node::MethodSignatureDeclaration(_) => {
                self.check_function_or_constructor_symbol(node, ambient);
                self.check_implicit_any_parameters(node, ambient);
                self.check_implicit_any_return(node, ambient);
                ambient
            }
            // §333 — the rule, its candidate list, its contextual-typing guard
            // *and* this dispatch all had to widen together; §331 moved two of
            // the three and measured zero.
            Node::FunctionExpression(_)
            | Node::ArrowFunction(_)
            | Node::FunctionTypeNode(_)
            | Node::ConstructorTypeNode(_)
            | Node::CallSignatureDeclaration(_)
            | Node::ConstructSignatureDeclaration(_) => {
                self.check_implicit_any_parameters(node, ambient);
                ambient
            }
            Node::ElementAccessExpression(_) => {
                self.check_null_or_undefined_receiver(node);
                // §401: the same question as the property-access arm below, and
                // §380's lesson — a rule that declines and a rule that is never
                // called are indistinguishable from the outside.
                self.check_nonexistent_property(node, ambient);
                // §326 — `M["x"] = 1` is the same readonly question as
                // `M.x = 1`, and the rule was reached from one arm only.
                self.check_readonly_assignment_target(node, ambient);
                ambient
            }
            Node::PropertyAccessExpression(_) => {
                self.check_null_or_undefined_receiver(node);
                self.check_nonexistent_property(node, ambient);
                self.check_readonly_assignment_target(node, ambient);
                self.check_property_not_used_before_declaration(node);
                self.check_private_property_access(node, ambient);
                self.check_private_name_shadowing(node);
                self.check_private_setter_read(node, ambient);
                ambient
            }
            Node::AsExpression(_) | Node::TypeAssertion(_) => {
                self.check_assertion_overlap(node, ambient);
                self.check_const_assertion_argument(node);
                ambient
            }
            Node::MetaProperty(_) => {
                self.check_new_target_meta_property(node);
                ambient
            }
            Node::SatisfiesExpression(_) => {
                self.check_satisfies_expression(node, ambient);
                ambient
            }
            Node::BinaryExpression(_) => {
                self.check_instanceof_left_operand(node);
                self.check_instanceof_right_operand(node);
                self.check_comparison_overlap(node, ambient);
                self.check_operator_operands(node, ambient);
                self.check_in_expression(node, ambient);
                self.check_nullish_coalesce_operands(node);
                ambient
            }
            Node::ComputedPropertyName(_) => {
                self.check_computed_property_name(node, ambient);
                ambient
            }
            Node::ObjectLiteralExpression(_) => {
                self.check_spread_property_overrides(node);
                self.check_spread_of_non_object_type(node);
                self.check_duplicate_object_literal_accessors(node);
                self.check_duplicate_object_literal_names(node);
                self.check_private_name_in_object_literal(node);
                ambient
            }
            // `[...x = a] = a` — a spread element in a destructuring assignment
            // whose expression is `x = a`. Upstream reports on the **operator
            // token** (`checker.go:12688`), which this port has directly;
            // the binding-element half of the same code computes its position
            // as `Initializer.Pos() - 1` and lands one column off here, which
            // is §641's trivia-inclusive span gap and its owner. §811.
            Node::SpreadElement(spread) => {
                if let Some(inner) = spread.expression.and_then(|e| e.node_id())
                    && let Some(Node::BinaryExpression(binary)) = self.node_map.get(inner)
                    && let Some(token) = binary.operator_token
                    && token.kind == SyntaxKind::EqualsToken
                    && let Some(at) = token.node_id
                    && self
                        .nodes
                        .parent(node)
                        .is_some_and(|p| self.nodes.kind(p) == SyntaxKind::ArrayLiteralExpression)
                    && let Some(file) = self.source_file_of_for_diagnostics(node)
                {
                    self.report(
                        file,
                        Diagnostic::new(
                            &messages::A_REST_ELEMENT_CANNOT_HAVE_AN_INITIALIZER,
                            self.nodes.span(at),
                        ),
                    );
                }
                ambient
            }
            Node::DeleteExpression(_) => {
                self.check_reference_expression(node);
                self.check_delete_operand_symbol(node);
                ambient
            }
            Node::CallExpression(call) => {
                self.resolve_call_before_callback_bodies(node, call.arguments);
                self.check_call_expression_diagnostics(node);
                self.check_import_call_specifier(node);
                ambient
            }
            Node::NewExpression(new) => {
                self.resolve_call_before_callback_bodies(node, new.arguments);
                self.check_new_expression_diagnostics(node);
                self.check_implicit_any_new_expression(node);
                ambient
            }
            Node::TaggedTemplateExpression(_) => {
                self.check_tagged_template_diagnostics(node);
                ambient
            }
            Node::TypeReferenceNode(_) | Node::ExpressionWithTypeArguments(_) => {
                self.check_type_argument_arity(node);
                self.check_type_argument_constraints(node);
                ambient
            }
            Node::TypeParameterDeclaration(_) => {
                self.check_circular_type_parameter_constraint(node);
                self.check_circular_type_parameter_default(node);
                self.check_type_alias_variance_annotation(node);
                ambient
            }
            Node::TypeLiteralNode(_) => {
                self.check_index_constraints(node);
                self.check_object_type_for_duplicate_declarations(node);
                self.check_private_name_in_object_literal(node);
                // A type literal carries index signatures exactly as an
                // interface does, and §69's rule already matches the kind —
                // only the dispatch named two of the three. §760's two-guards
                // shape, in a second rule. §762.
                self.check_duplicate_index_signatures(node);
                ambient
            }
            Node::QualifiedName(_) => {
                self.check_qualified_type_name(node);
                ambient
            }
            Node::TemplateSpan(_) => {
                self.check_template_span_symbol_conversion(node);
                ambient
            }
            Node::PrefixUnaryExpression(_) | Node::PostfixUnaryExpression(_) => {
                // `checkPrefixUnaryExpression` wraps its operand in
                // `checkNonNullType` exactly as the binary arms wrap theirs.
                // §759.
                // `if ok { checkReferenceExpression(...) }` — upstream gates
                // the reference check on the arithmetic one so a non-numeric
                // operand reports TS2356 alone. §741.
                if self.check_unary_operator_operands(node, ambient) {
                    self.check_reference_expression(node);
                }
                ambient
            }
            Node::Identifier(identifier) => {
                self.check_identifier_assignment_target(node, ambient);
                self.check_readonly_identifier_assignment(node, ambient);
                self.check_parameter_self_reference(node, identifier.text);
                self.check_value_identifier(node, identifier.text);
                self.check_type_reference_name(node, identifier.text);
                self.check_await_as_binding_name(node);
                self.check_umd_global_reference(node, identifier.text);
                // `checkResolvedBlockScopedVariable` runs during name
                // resolution, before `checkIdentifier`'s flow section.
                self.check_used_before_its_declaration(node, identifier.text);
                self.check_used_before_assigned(node, identifier.text);
                self.mark_identifier_reference(node, identifier.text);
                ambient
            }
            // §982.
            Node::HeritageClause(clause) => {
                self.check_grammar_heritage_clause(node, clause);
                ambient
            }
            // §960: a type alias is the other half of `export type A = {}` /
            // `type A = {}`, and had no arm in this match at all.
            Node::TypeAliasDeclaration(_) => {
                self.check_exports_on_merged_declarations(node);
                self.check_type_alias_circularity(node);
                ambient
            }
            _ => ambient,
        };
        self.check_unreachable(node, ambient);
        self.check_module_format(node, typed, ambient);
        // `checkGrammarModifiers` runs on the declaration that carries the
        // list. Bounded to class elements and parameters — `defaultKeywordWithoutExport1`
        // is the statement-level shape and is declined, §103.
        match typed {
            Node::YieldExpression(_) => {
                self.check_yield_grammar(node);
                self.check_yield_expression_assignability(node);
            }
            Node::AwaitExpression(_) => {
                self.check_await_in_parameter_initializer(node);
                self.check_await_in_non_async_function(node);
                self.check_await_operand_awaited(node);
            }
            Node::ImportSpecifier(_) | Node::ExportSpecifier(_) => {
                self.report_missing_module_export(node);
                self.check_circular_import_alias(node);
                self.check_alias_symbol(node);
                self.check_export_specifier_is_local(node);
            }
            // `checkImportBinding` (`checker.go:5287`-`:5303`, `:5473`) and
            // `checkExportDeclaration`'s clause (`:5534`).
            Node::ImportClause(_) | Node::NamespaceImport(_) | Node::NamespaceExport(_) => {
                self.check_module_has_default_export(node);
                self.check_circular_import_alias(node);
                self.check_alias_symbol(node);
            }
            // `NodeCanBeDecorated` rejects every one of these outright.
            Node::EnumDeclaration(n) => self.check_illegal_decorator(n.modifiers),
            Node::ClassDeclaration(class_declaration) => {
                self.check_object_type_for_duplicate_declarations(node);
                self.check_merged_namespace_prototype(node);
                self.check_class_static_property_names(node, class_declaration.members);
                self.check_type_parameter_lists_identical(node);
                self.check_base_chain_is_acyclic(node);
                self.check_abstract_members_implemented(node);
            }
            Node::FunctionDeclaration(n) => self.check_illegal_decorator(n.modifiers),
            Node::InterfaceDeclaration(n) => {
                self.check_object_type_for_duplicate_declarations(node);
                self.check_illegal_decorator(n.modifiers);
                self.check_type_parameter_lists_identical(node);
            }
            Node::TypeAliasDeclaration(n) => self.check_illegal_decorator(n.modifiers),
            Node::VariableStatement(n) => self.check_illegal_decorator(n.modifiers),
            Node::ImportEqualsDeclaration(n) => {
                self.check_illegal_decorator(n.modifiers);
                self.check_alias_symbol(node);
                self.check_module_hidden_by_local(node);
            }
            Node::ModuleDeclaration(n) => self.check_illegal_decorator(n.modifiers),
            Node::ImportDeclaration(n) => {
                // `!checkGrammarModifiers(node) && node.Modifiers() != nil`
                // (`checker.go:5278`), on the declaration's first token — the
                // modifier itself. The first conjunct defers to the
                // modifier-order codes, which do not fire for a plain `export`
                // on an import, so the arm is one condition here. §813.
                if let Some(first) = n.modifiers.first()
                    && let Some(at) = tsr_ast::Node::from(*first).node_id()
                    && let Some(file) = self.source_file_of_for_diagnostics(at)
                {
                    let span = self.nodes.span(at);
                    self.report(
                        file,
                        Diagnostic::new(
                            &messages::AN_IMPORT_DECLARATION_CANNOT_HAVE_MODIFIERS,
                            span,
                        ),
                    );
                }
                self.check_illegal_decorator(n.modifiers);
            }
            Node::ExportDeclaration(n) => self.check_illegal_decorator(n.modifiers),
            // **Signatures reach the same walk**: TS1070's arm is inside it and
            // fires only for these two kinds, so without these arms it was
            // §380's dispatch-never-called for the sixth time. §599.
            Node::PropertySignatureDeclaration(n) => self.check_modifier_order(node, n.modifiers),
            Node::MethodSignatureDeclaration(n) => self.check_modifier_order(node, n.modifiers),
            Node::PropertyDeclaration(n) => self.check_modifier_order(node, n.modifiers),
            Node::MethodDeclaration(n) => self.check_modifier_order(node, n.modifiers),
            Node::GetAccessorDeclaration(n) => self.check_modifier_order(node, n.modifiers),
            Node::SetAccessorDeclaration(n) => self.check_modifier_order(node, n.modifiers),
            Node::ConstructorDeclaration(n) => {
                self.check_modifier_order(node, n.modifiers);
                self.check_constructor_type_parameters(n);
                self.check_constructor_type_annotation(n);
            }
            Node::ParameterDeclaration(n) => self.check_modifier_order(node, n.modifiers),
            Node::IndexSignatureDeclaration(n) => {
                self.check_index_signature_modifiers(node, n.modifiers);
            }
            _ => {}
        }
        // Its own call rather than an arm in either `match` above: the two
        // above claim `BinaryExpression`, `ParameterDeclaration`,
        // `FunctionDeclaration` and both unary kinds behind guards this rule
        // must not be filtered through, and §140 recorded a rule silently
        // deleted by exactly that. §156.
        self.check_reserved_declaration_name(typed);
        // §320 — `checkGrammarModifiers` runs for **every** declaration that
        // can carry modifiers; this port invoked `check_modifier_order` from
        // six dispatch arms, all of them class members or a parameter, so
        // TS1038's arm existed and never fired on a `declare` inside a
        // `declare namespace`.
        // `checkGrammarAsyncModifier` (`grammarchecks.go:659`) applies to **any**
        // node carrying an `async` modifier, including the class-member kinds
        // the order check below excludes, so it is asked separately. §377.
        if let Some(modifiers) = modifiers_of(typed) {
            self.check_grammar_async_modifier(node, modifiers);
        }
        if !matches!(
            typed,
            Node::PropertyDeclaration(_)
                | Node::MethodDeclaration(_)
                | Node::GetAccessorDeclaration(_)
                | Node::SetAccessorDeclaration(_)
                | Node::ConstructorDeclaration(_)
                | Node::ParameterDeclaration(_)
                // **The two signature kinds are dispatched by kind above** and
                // were missing from this exclusion list, so every type member
                // ran `check_modifier_order` twice and every TS1070 was emitted
                // twice. Nine cases, invisible to every set-comparing
                // instrument because both copies are *right*. §993.
                | Node::PropertySignatureDeclaration(_)
                | Node::MethodSignatureDeclaration(_)
        ) && let Some(modifiers) = modifiers_of(typed)
            && !modifiers.is_empty()
        {
            self.check_modifier_order(node, modifiers);
        }
        self.check_grammar_heritage_clauses(typed);
        self.check_override_kind(node, typed);
        if matches!(typed, Node::GetAccessorDeclaration(_) | Node::SetAccessorDeclaration(_)) {
            self.check_grammar_accessor(node, typed);
        }
        // **Every kind 's default arm names**, not
        // just  — §623, and §600's dispatch class for the
        // ninth time.
        self.check_modifier_on_nested_statement(node);
        self.check_declaration_statement_container(node, typed);
        self.check_field_named_constructor(node, typed);
        self.check_decorated_private_name(node, typed);
        self.check_dynamic_import_module_kind(node, typed);
        self.check_dynamic_import_specifier(typed);
        self.check_interface_computed_name(node, typed);
        self.check_grammar_for_generator(node, typed);
        self.check_grammar_parameter_list(node);
        // §876: upstream calls these behind `!c.checkGrammarModifiers(node)`.
        if !self.modifier_chain_reported.contains(&node) {
            self.check_grammar_modifier_shapes(node, typed);
        }
        self.check_parser_lane_statement(typed);
        self.check_grammar_jsx_element(typed);
        self.check_jsx_intrinsic_element(node, typed);
        self.check_jsx_intrinsic_tag_exists(node, typed);
        self.mark_jsx_alias_referenced(node, typed);
        self.check_jsx_component_bound(node, typed);
        self.check_jsx_string_literal_tag(node, typed);
        self.check_jsx_fragment_factory(node, typed);
        self.check_strict_mode_eval_or_arguments_sites(node, typed, ambient);
        if matches!(typed, Node::DeleteExpression(_)) {
            self.check_strict_mode_delete_expression(node);
        }
        if matches!(typed, Node::WithStatement(_)) {
            self.check_with_statement_grammar(node);
        }
        self.check_contextual_identifier(node, ambient);
        self.check_type_parameter_list(type_parameters_of(typed));
        if self.nodes.kind(node) == SyntaxKind::SwitchStatement {
            self.check_switch_case_comparable(node);
        }
        // `this` is a keyword node rather than a `Node` variant, so it is asked
        // here rather than from a match arm. §392.
        if matches!(
            typed,
            Node::ConstructSignatureDeclaration(_) | Node::CallSignatureDeclaration(_)
        ) {
            self.check_implicit_any_signature_return(node, ambient);
        }
        if matches!(typed, Node::MethodDeclaration(_)) {
            self.check_abstract_method_has_no_body(node);
        }
        if !self.modifier_chain_reported.contains(&node) {
            self.check_abstract_modifier_position(node, typed);
        }
        if matches!(
            typed,
            Node::FunctionDeclaration(_)
                | Node::MethodDeclaration(_)
                | Node::FunctionExpression(_)
                | Node::ArrowFunction(_)
                | Node::GetAccessorDeclaration(_)
        ) {
            self.check_all_code_paths_return_or_throw(node);
            self.check_async_function_return_type(node);
        }
        if matches!(
            typed,
            Node::VariableDeclaration(_)
                | Node::FunctionDeclaration(_)
                | Node::ClassDeclaration(_)
                | Node::ModuleDeclaration(_)
        ) {
            self.check_builtin_global_redeclaration(node);
        }
        if matches!(typed, Node::InterfaceDeclaration(_) | Node::ClassDeclaration(_)) {
            self.check_recursive_base_type(node, typed);
        }
        if matches!(typed, Node::LabeledStatement(_)) {
            self.check_duplicate_label(node, ambient);
            self.check_label_is_allowed(node);
        }
        if matches!(typed, Node::ForInOrOfStatement(_)) {
            self.check_for_in_variable_type(node);
            self.check_for_in_right_operand(node);
            self.check_for_in_reference_expression(node);
            self.check_for_in_or_of_declarations(node);
            self.check_for_await_context(node);
            self.check_for_of_iteration(node);
            self.check_for_of_reference_assignment(node, ambient);
            self.check_for_of_reference_target(node);
        }
        if matches!(typed, Node::ArrowFunction(_)) {
            self.check_arrow_expression_body(node, ambient);
        }
        if matches!(typed, Node::ImportTypeNode(_)) {
            self.check_import_type_argument(node);
        }
        if matches!(
            typed,
            Node::BindingPattern(_)
                | Node::ArrayLiteralExpression(_)
                | Node::ObjectLiteralExpression(_)
        ) {
            self.check_rest_element_is_last(node, typed);
        }
        match typed {
            Node::BindingPattern(_) => self.check_array_binding_pattern_iteration(node),
            Node::BindingElement(_) => {
                self.check_binding_element_tuple_bounds(node);
                self.check_binding_element_index_access(node);
                self.check_object_rest_of_non_object_type(node);
            }
            Node::ObjectLiteralExpression(_) => self.check_object_assignment_index_access(node),
            Node::ElementAccessExpression(_) => {
                self.check_element_access_tuple_bounds(node);
                self.check_element_access_index_type(node);
            }
            Node::IndexedAccessTypeNode(_) => {
                self.check_indexed_access_type_tuple_bounds(node);
                self.check_indexed_access_type_index_type(node);
            }
            Node::ArrayLiteralExpression(_) => {
                self.check_array_destructuring_assignment_iteration(node);
                self.check_array_assignment_tuple_bounds(node);
            }
            Node::SpreadElement(_) => self.check_spread_element_iteration(node),
            Node::JsxSpreadAttribute(_) => self.check_jsx_spread_of_non_object_type(node),
            Node::JsxAttributes(_) => {
                self.check_jsx_spread_property_overrides(node);
                self.check_jsx_children_specified_twice(node);
            }
            Node::JsxExpression(_) => self.check_jsx_expression(node),
            Node::YieldExpression(_) => {
                self.check_yield_star_iteration(node);
                self.check_implicit_any_yield_expression(node);
            }
            _ => {}
        }
        if matches!(typed, Node::ExportDeclaration(_)) {
            self.check_export_declaration_in_namespace(node, typed);
        }
        if matches!(typed, Node::ImportDeclaration(_)) {
            self.check_deferred_import_clause(typed);
        }
        if matches!(typed, Node::IndexSignatureDeclaration(_)) {
            self.check_index_signature_key_type(node);
        }
        if matches!(typed, Node::ExportAssignment(_)) {
            self.check_export_assignment_alone(node, ambient);
            self.check_jsdoc_annotated_initializer(node, ambient);
        }
        if self.nodes.kind(node) == SyntaxKind::NewExpression {
            self.check_new_on_abstract_class(node);
        }
        if self.nodes.kind(node) == SyntaxKind::IndexSignature {
            self.check_index_signature_parameter_type(node);
        }
        if self.nodes.kind(node) == SyntaxKind::SuperKeyword {
            self.check_super_expression_diagnostics(node);
        }
        if self.nodes.kind(node) == SyntaxKind::ThisKeyword {
            self.check_this_expression_diagnostics(node);
        }
        if self.nodes.kind(node) == SyntaxKind::Identifier {
            self.check_this_in_type_query_diagnostics(node);
        }
        if self.nodes.kind(node) == SyntaxKind::ThisType {
            self.check_this_type_node(node);
        }
        self.check_truthiness_sites(node, ambient);
        self.note_member_name_at(node);
        self.register_for_unused_check(node);
        self.check_jsdoc_link_references(node);
        self.check_unmatched_jsdoc_parameters(node);
        self.check_jsdoc_satisfies_tags(node, ambient);
        let mut children = [const { None }; INLINE_CHILDREN];
        let mut count = 0usize;
        let mut overflow: Vec<NodeId> = Vec::new();
        tsr_ast::for_each_child_id(typed, |child| {
            if count < INLINE_CHILDREN {
                children[count] = Some(child);
            } else {
                overflow.push(child);
            }
            count += 1;
        });
        for child in children.into_iter().flatten() {
            self.check_node(child, ambient, depth + 1);
        }
        for child in overflow {
            self.check_node(child, ambient, depth + 1);
        }
    }

    /// `checkExternalImportOrExportDeclaration` (`checker.go:5332`) — the gate
    /// that runs **before** any resolution and can stop it.
    ///
    /// Only the arm that matters for a specifier that will not resolve is
    /// ported: an import or export declaration whose parent is neither a
    /// `SourceFile` nor a module block belonging to an **ambient** module is a
    /// grammar error (TS1147 `Import_declarations_in_a_namespace_cannot_reference_a_module`,
    /// or TS1148 for the export spelling) and upstream `return`s without
    /// resolving anything.
    ///
    /// This port emits neither 1147 nor 1148 — they are grammar checks with
    /// their own sizing — so the arm is a **refusal**: silence where upstream
    /// says something else. Reporting TS2307 inside a namespace was 14 of the 60
    /// wrong lines the first counterfactual measured, all in
    /// `compiler/privacyImportParseErrors` and its sibling.
    pub(crate) fn external_import_is_positioned_for_resolution(&self, declaration: NodeId) -> bool {
        let Some(parent) = self.nodes.parent(declaration) else { return false };
        if self.nodes.kind(parent) == SyntaxKind::SourceFile {
            return true;
        }
        // `inAmbientExternalModule` (`checker.go:5343`): a module *block* whose
        // own parent is an ambient module declaration.
        self.nodes.kind(parent) == SyntaxKind::ModuleBlock
            && self.nodes.parent(parent).is_some_and(|owner| self.is_ambient_module_node(owner))
    }

    /// `ast.IsAmbientModule` (`ast/utilities.go:1652`): a module declaration
    /// named by a **string literal**, or a `declare global` augmentation
    /// (`IsGlobalScopeAugmentation`, `:1690`).
    fn is_ambient_module_node(&self, node: NodeId) -> bool {
        let Some(Node::ModuleDeclaration(declaration)) = self.node_map.get(node) else {
            return false;
        };
        matches!(declaration.name, Some(tsr_ast::ModuleName::StringLiteral(_)))
            || declaration.keyword.kind == SyntaxKind::GlobalKeyword
    }

    /// TS2686 — `'{0}' refers to a UMD global, but the current file is a
    /// module. Consider adding an import instead.`
    ///
    /// `checkIdentifier`'s UMD arm. A UMD global is *meant* to resolve in a
    /// script file — the binder files `export as namespace N` into the file's
    /// `global_exports` (`binder.rs:4620`, upstream `binder.go:820`) precisely
    /// so it can. What makes it an error is resolving it from a **module**,
    /// which is the only thing this rule adds to machinery already in place.
    /// §361.
    fn check_umd_global_reference(&mut self, node: NodeId, text: &str) {
        if self.file_has_parse_errors || !self.is_value_reference(node) {
            return;
        }
        // Upstream's guard is `meaning&SymbolFlagsValue == SymbolFlagsValue`
        // (`checker.go:1841`), and a heritage entry that emits nothing is
        // resolved at **type** meaning — so this rule never sees one.
        //
        // It is spelled here rather than in [`Checker::is_value_reference`]
        // because that predicate is shared with TS2304, which upstream *does*
        // report in an interface's `extends` (through `resolveEntityName`'s
        // failure rather than through a value lookup). Declining there instead
        // lost `compiler/protoAssignment`. One rule's meaning is not another's.
        //
        // `interface LogoProps extends React.SVGProps<SVGSVGElement>` is the
        // shape: `React` is the `expression` of a `PropertyAccessExpression`,
        // so it is a reference by every slot test, and only the heritage walk
        // tells it apart from a real value use.
        if self.identifier_in_non_emitting_heritage_clause(node) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let Some(Node::SourceFile(source)) = self.node_map.get(file) else { return };
        if !tsr_binder::is_external_module(source) {
            return;
        }
        let Some(symbol) = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            node,
            text,
            SymbolFlags::VALUE | SymbolFlags::EXPORT_VALUE,
        ) else {
            return;
        };
        let symbol = self.binder.merged_symbol(symbol);
        // The declaration kind, not `ALIAS`: a plain `import * as Bar` is an
        // alias too and must stay silent.
        //
        // **`Every`, not `any`** — `core.Every` at `checker.go:1843`:
        //
        // ```go
        // if len(merged.Declarations) != 0 && core.Every(merged.Declarations, func(d *ast.Node) bool {
        //     return ast.IsNamespaceExportDeclaration(d) || ast.IsSourceFile(d) && d.AsSourceFile().GlobalExports != nil
        // })
        // ```
        //
        // The difference is the whole rule on a real `@types` package. Every UMD
        // declaration file is written
        //
        // ```ts
        // declare namespace React { … }
        // export = React;
        // export as namespace React;
        // ```
        //
        // and those merge into **one** symbol whose declarations are a
        // `ModuleDeclaration` *and* a `NamespaceExportDeclaration`. Under `any`
        // the name is a UMD-only global and every reference to it inside its own
        // declaration file is an error; under `Every` it is a namespace that
        // *also* has a UMD name, and referring to it is ordinary. `tsc` reports
        // nothing on `export = React`; this reported TS2686.
        //
        // **The second disjunct is not ported.** It admits a `SourceFile`
        // declaration whose `GlobalExports` is non-empty — the module symbol of
        // a UMD file — and this binder merges `global_exports` into one
        // program-wide table, so "does *this file* have global exports" is not a
        // question it can answer. Omitting it makes the `Every` stricter than
        // upstream's, so the rule reports **less** where it should report: a
        // missing diagnostic rather than a wrong one, and the direction this
        // rule needs while its false positives are what hurt.
        let declarations = &self.binder.symbols().get(symbol).declarations;
        if declarations.is_empty()
            || !declarations.iter().all(|&declaration| {
                self.nodes.kind(declaration) == SyntaxKind::NamespaceExportDeclaration
            })
        {
            return;
        }
        let span = self.error_span(node);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::_0_REFERS_TO_A_UMD_GLOBAL_BUT_THE_CURRENT_FILE_IS_A_MODULE_CONSIDER_ADDING_AN_IMPORT_INSTEAD,
                span,
                [text.to_string()],
            ),
        );
    }

    /// TS2313 — `Type parameter '{0}' has a circular constraint.`
    ///
    /// `checkTypeParameter` (`checker.go`) resolves the parameter's base
    /// constraint "to reveal circularity errors"; the report itself belongs to
    /// [`Checker::base_constraint_of_type`]'s failed pop, which marks every
    /// participant of `U extends T, T extends U` and none outside the cycle.
    /// `T extends Array<T>` is legal because an object type ends the walk.
    fn check_circular_type_parameter_constraint(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        if !self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_PARAMETER) {
            return;
        }
        let parameter = self.get_declared_type_of_symbol(symbol);
        self.base_constraint_of_type(parameter);
    }

    /// TS2456 — `Type alias '{0}' circularly references itself.`
    ///
    /// `checkTypeAliasDeclaration`'s `checkSourceElement(node.Type())`
    /// resolves the body's references, which is what reaches
    /// `getDeclaredTypeOfTypeAlias` for an alias nothing else mentions
    /// (`type T0 = T0`). The report is that getter's failed pop, for generic
    /// and non-generic aliases alike (`type T1<in in> = T1`).
    fn check_type_alias_circularity(&mut self, node: NodeId) {
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        if !self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_ALIAS) {
            return;
        }
        self.get_declared_type_of_symbol(symbol);
    }

    /// TS1042 — `'{0}' modifier cannot be used here.`
    ///
    /// `checkGrammarAsyncModifier` (`grammarchecks.go:659`). `async` is legal
    /// on exactly four node kinds; everywhere else the modifier itself is the
    /// error node — `async class C {}` reports at column 1, not at `C`. §377.
    fn check_grammar_async_modifier(
        &mut self,
        node: NodeId,
        modifiers: &[tsr_ast::ModifierLike<'_>],
    ) {
        if self.file_has_parse_errors {
            return;
        }
        if matches!(
            self.nodes.kind(node),
            SyntaxKind::MethodDeclaration
                | SyntaxKind::FunctionDeclaration
                | SyntaxKind::FunctionExpression
                | SyntaxKind::ArrowFunction
                // **A constructor is TS1089's**, and upstream's chain returns
                // there (`grammarchecks.go:547`) so this arm is never reached
                // for one. This rule lives outside that chain and must restate
                // its exclusions; §823 added the arm and this line is what
                // stops the two from both reporting at the same column. §857.
                | SyntaxKind::Constructor
        ) {
            return;
        }
        for modifier in modifiers {
            let tsr_ast::ModifierLike::Token(token) = modifier else { continue };
            if token.kind != SyntaxKind::AsyncKeyword {
                continue;
            }
            let Some(file) = token.node_id.and_then(|id| self.source_file_of_for_diagnostics(id))
            else {
                continue;
            };
            let Some(id) = token.node_id else { continue };
            let span = self.nodes.span(id);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::_0_MODIFIER_CANNOT_BE_USED_HERE,
                    span,
                    ["async".to_string()],
                ),
            );
        }
    }

    /// TS18016 — `Private identifiers are not allowed outside class bodies.`
    ///
    /// `checkGrammarObjectLiteralExpression`'s private-name arm
    /// (`grammarchecks.go:1063`). An object literal is not a class body, so a
    /// `#name` property is a grammar error wherever it appears — the error node
    /// is the name. §388.
    fn check_private_name_in_object_literal(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        // A **type literal** and an **interface body** are no more class bodies
        // than an object literal is, and upstream reports the same code for a
        // `#name` member of either. §390.
        if let Some(members) = match self.node_map.get(node) {
            Some(Node::TypeLiteralNode(literal)) => Some(literal.members),
            Some(Node::InterfaceDeclaration(declaration)) => Some(declaration.members),
            _ => None,
        } {
            for member in members {
                let name = match member {
                    tsr_ast::TypeElement::PropertySignatureDeclaration(property) => property.name,
                    tsr_ast::TypeElement::MethodSignatureDeclaration(method) => method.name,
                    tsr_ast::TypeElement::GetAccessorDeclaration(accessor) => accessor.name,
                    tsr_ast::TypeElement::SetAccessorDeclaration(accessor) => accessor.name,
                    _ => continue,
                };
                let tsr_ast::PropertyName::PrivateIdentifier(private) = name else { continue };
                let Some(id) = private.node_id else { continue };
                let Some(file) = self.source_file_of_for_diagnostics(id) else { continue };
                let span = self.nodes.span(id);
                self.report(
                    file,
                    Diagnostic::new(
                        &messages::PRIVATE_IDENTIFIERS_ARE_NOT_ALLOWED_OUTSIDE_CLASS_BODIES,
                        span,
                    ),
                );
            }
            return;
        }
        let Some(Node::ObjectLiteralExpression(literal)) = self.node_map.get(node) else { return };
        for property in literal.properties {
            let name = match property {
                tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                    assignment.name
                }
                tsr_ast::ObjectLiteralElementLike::MethodDeclaration(method) => method.name,
                tsr_ast::ObjectLiteralElementLike::GetAccessorDeclaration(accessor) => {
                    accessor.name
                }
                tsr_ast::ObjectLiteralElementLike::SetAccessorDeclaration(accessor) => {
                    accessor.name
                }
                _ => continue,
            };
            let tsr_ast::PropertyName::PrivateIdentifier(private) = name else { continue };
            let Some(id) = private.node_id else { continue };
            let Some(file) = self.source_file_of_for_diagnostics(id) else { continue };
            let span = self.nodes.span(id);
            self.report(
                file,
                Diagnostic::new(
                    &messages::PRIVATE_IDENTIFIERS_ARE_NOT_ALLOWED_OUTSIDE_CLASS_BODIES,
                    span,
                ),
            );
        }
    }

    /// TS2669 — `Augmentations for the global scope can only be directly nested
    /// in external modules or ambient module declarations.`
    ///
    /// `checkModuleDeclaration` (`checker.go:5203`, `:5209`). The two legal
    /// shapes are `binder.rs:1101`'s `is_merged_global_augmentation`, whose doc
    /// records a `tsc` 5.x verification of exactly this predicate; the binder's
    /// consumer was the *merge*, and the diagnostic that fires otherwise was
    /// never emitted. §418.
    fn check_global_augmentation_position(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(Node::ModuleDeclaration(module)) = self.node_map.get(node) else { return };
        // `ast.IsGlobalScopeAugmentation` — the parser gives the block a
        // synthetic `global` identifier, so only the keyword distinguishes it
        // from `namespace global { … }`.
        if module.keyword.kind != SyntaxKind::GlobalKeyword {
            return;
        }
        let legal = match self.nodes.parent(node).and_then(|p| self.node_map.get(p)) {
            Some(Node::SourceFile(source)) => tsr_binder::is_external_module(source),
            Some(Node::ModuleBlock(_)) => {
                let block = self.nodes.parent(node);
                let outer = block.and_then(|b| self.nodes.parent(b));
                let file = outer.and_then(|o| self.nodes.parent(o));
                matches!(
                    (outer.and_then(|o| self.node_map.get(o)), file.and_then(|f| self.node_map.get(f))),
                    (Some(Node::ModuleDeclaration(_)), Some(Node::SourceFile(source)))
                        if !tsr_binder::is_external_module(source)
                )
            }
            _ => false,
        };
        if legal {
            return;
        }
        let Some(name) = module.name.and_then(|name| name.node_id()) else { return };
        let Some(file) = self.source_file_of_for_diagnostics(name) else { return };
        let span = self.error_span(name);
        self.report(
            file,
            Diagnostic::new(
                &messages::AUGMENTATIONS_FOR_THE_GLOBAL_SCOPE_CAN_ONLY_BE_DIRECTLY_NESTED_IN_EXTERNAL_MODULES_OR_AMBIENT_MODULE_DECLARATIONS,
                span,
            ),
        );
    }

    /// TS2309 — `An export assignment cannot be used in a module with other
    /// exported elements.`
    ///
    /// `checkExportsOnMergedDeclarations`' export-equals arm
    /// (`checker.go:5703`), **branch (a) only**: the module exports a *value*
    /// member besides the `export =`. Branch (b), `hasShadowedNamespace`, needs
    /// the exported entity's own members and is the type-side read §424 named
    /// as this toolkit's boundary.
    ///
    /// `isTopLevelInExternalModuleAugmentation` is the one exempt shape —
    /// `declare module "x" { export = Y }`. §432.
    /// `isContainedByNamespace` (`checker.go:5575`): the container is the
    /// parent, or the parent's parent when the parent is not a source file —
    /// **one or two hops, not an ancestor walk** — and it must be a
    /// `ModuleDeclaration` that is **not** an ambient module. Written as an
    /// ancestor walk first, which cost 27 cases: `declare module "x" { export =
    /// Y }` is §432's exempt shape and every one of them reported. §826.
    fn is_contained_by_namespace(&self, node: NodeId) -> bool {
        let Some(parent) = self.nodes.parent(node) else { return false };
        let container = if self.nodes.kind(parent) == SyntaxKind::SourceFile {
            parent
        } else {
            let Some(grandparent) = self.nodes.parent(parent) else { return false };
            grandparent
        };
        self.nodes.kind(container) == SyntaxKind::ModuleDeclaration
            && !self.is_ambient_module_declaration(container)
    }

    fn check_export_assignment_alone(&mut self, node: NodeId, ambient: bool) {
        // `checkExternalModuleExports` resolves the module's `export=` symbol,
        // which is where an `export = self` cycle reports. Not grammar, so
        // ahead of the parse-error bail below.
        self.check_circular_import_alias(node);
        let Some(Node::ExportAssignment(assignment)) = self.node_map.get(node) else { return };
        // `checkExportAssignment` (checker.go:5588-5605, pinned 5b1047d):
        // resolve the expression before rejecting its context. Reuse the
        // existing Checker-owned node-type completion, never a second cache.
        if let Some(expression) = assignment.expression {
            self.check_expression(expression);
        }
        let Some(parent) = self.nodes.parent(node) else { return };
        if !matches!(
            self.nodes.kind(parent),
            SyntaxKind::SourceFile | SyntaxKind::ModuleBlock | SyntaxKind::ModuleDeclaration
        ) {
            // `checkGrammarModuleElementContext` (grammarchecks.go:206) always
            // rejects this context, even when parse diagnostics suppress the
            // first-token grammar diagnostic.
            if !self.file_has_parse_errors
                && let Some(file) = self.source_file_of_for_diagnostics(node)
                && let Some(text) =
                    self.module_host.and_then(|host| host.source_text(file, self.nodes))
                && let Some(rest) = text.get(self.nodes.span(node).start as usize..)
            {
                // `grammarErrorOnFirstToken` (grammarchecks.go:19) /
                // `GetRangeOfTokenAtPosition` (scanner.go:2521). Source bytes
                // belong to this Program's node table; scan only the first
                // token on this error path. Legacy hosts cannot supply a range.
                let token = tsr_scanner::Scanner::new(rest).scan().span;
                let start = self.nodes.span(node).start;
                let span = tsr_core::Span::new(start + token.start, start + token.end);
                let message = if assignment.is_export_equals {
                    &messages::AN_EXPORT_ASSIGNMENT_MUST_BE_AT_THE_TOP_LEVEL_OF_A_FILE_OR_MODULE_DECLARATION
                } else {
                    &messages::A_DEFAULT_EXPORT_MUST_BE_AT_THE_TOP_LEVEL_OF_A_FILE_OR_MODULE_DECLARATION
                };
                self.report(file, Diagnostic::new(message, span));
            }
            return;
        }
        if self.is_contained_by_namespace(node) {
            if let Some(file) = self.source_file_of_for_diagnostics(node) {
                let message = if assignment.is_export_equals {
                    &messages::AN_EXPORT_ASSIGNMENT_CANNOT_BE_USED_IN_A_NAMESPACE
                } else {
                    &messages::A_DEFAULT_EXPORT_CAN_ONLY_BE_USED_IN_AN_ECMASCRIPT_STYLE_MODULE
                };
                let span = self.error_span(node);
                self.report(file, Diagnostic::new(message, span));
            }
            return;
        }
        if self.file_has_parse_errors {
            return;
        }
        // TS1120 — `An export assignment cannot have modifiers.`
        // `checkExportAssignment` (`checker.go:5607`), on the **first token**:
        // `declare export = x` errors at `declare`, column 1. §481 measured this
        // at `+0` because the parser handed the node an empty modifier slice;
        // §511 threads the caller's modifiers through, so the list is real now.
        // Upstream tests `IsExportAssignment`, covering both spellings. §512.
        if let Some(tsr_ast::ModifierLike::Token(first)) = assignment.modifiers.first()
            && let Some(id) = first.node_id
            && let Some(file) = self.source_file_of_for_diagnostics(id)
        {
            let span = self.nodes.span(id);
            self.report(
                file,
                Diagnostic::new(&messages::AN_EXPORT_ASSIGNMENT_CANNOT_HAVE_MODIFIERS, span),
            );
        }
        // Ported from typescript-go's `checkExportAssignment`
        // (`internal/checker/checker.go:5666`, pinned `5b1047d`): ambient
        // assignment expressions must be entity-name expressions. The caller
        // carries the parser's Ambient context through the existing walk.
        if ambient
            && self.nodes.parent(node).is_some_and(|parent| {
                matches!(
                    self.nodes.kind(parent),
                    SyntaxKind::SourceFile
                        | SyntaxKind::ModuleBlock
                        | SyntaxKind::ModuleDeclaration
                )
            })
            && !self.is_contained_by_namespace(node)
            && let Some(expression) = assignment.expression.and_then(|e| e.node_id())
            && !self.is_entity_name_expression(expression)
            && let Some(file) = self.source_file_of_for_diagnostics(expression)
        {
            let span = self.error_span(expression);
            self.report(
                file,
                Diagnostic::new(
                    &messages::THE_EXPRESSION_OF_AN_EXPORT_ASSIGNMENT_MUST_BE_AN_IDENTIFIER_OR_QUALIFIED_NAME_IN_AN_AMBIENT_CONTEXT,
                    span,
                ),
            );
        }
        if !assignment.is_export_equals {
            return;
        }
        let Some(parent) = self.nodes.parent(node) else { return };
        let Some(Node::SourceFile(source)) = self.node_map.get(parent) else { return };
        // TS1203 / TS1218: `checkExportAssignment`'s module-format tail
        // (`checker.go:5669`), which reads the file's implied format for emit.
        // §478, §783; `docs/parity/notes/r5-modfmt.md` §2.
        if self.check_export_equals_module_format(node, parent, ambient) {
            return;
        }
        let exports_a_value = source.statements.iter().any(|statement| {
            let Some(id) = statement.node_id() else { return false };
            if id == node {
                return false;
            }
            let Some(typed) = self.node_map.get(id) else { return false };
            let exported = modifiers_of(typed)
                .is_some_and(|modifiers| has_modifier(modifiers, SyntaxKind::ExportKeyword));
            match typed {
                Node::ClassDeclaration(_)
                | Node::FunctionDeclaration(_)
                | Node::VariableStatement(_)
                | Node::EnumDeclaration(_) => exported,
                // `export { a }` names whatever it lists; a clause-less
                // `export *` re-exports and is not an own member.
                Node::ExportDeclaration(declaration) => declaration.export_clause.is_some(),
                _ => false,
            }
        });
        if !exports_a_value {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.nodes.span(node);
        self.report(
            file,
            Diagnostic::new(
                &messages::AN_EXPORT_ASSIGNMENT_CANNOT_BE_USED_IN_A_MODULE_WITH_OTHER_EXPORTED_ELEMENTS,
                span,
            ),
        );
    }

    /// TS2507 — `Type '{0}' is not a constructor function type.`
    ///
    /// `getBaseConstructorTypeOfClass` (`checker.go:16984`): the class's first
    /// `extends` expression is checked, and a type that is not `any`, not the
    /// null widening type and not `isConstructorType` is reported at the
    /// expression.
    ///
    /// `isConstructorType` is "has a construct signature" or, for a type
    /// variable, a mixin-constructor constraint. The type-variable arm is not
    /// ported and declines; a type whose construct signatures this port cannot
    /// enumerate (`None`) declines too. An intrinsic primitive has no construct
    /// signature under any structural reading (§436).
    fn check_extends_primitive(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let clauses = match self.node_map.get(node) {
            Some(Node::ClassDeclaration(class)) => class.heritage_clauses,
            Some(Node::ClassExpression(class)) => class.heritage_clauses,
            _ => return,
        };
        // `getBaseTypeNodeOfClass`: the first `extends` entry.
        let Some(base) = clauses
            .iter()
            .filter(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)
            .flat_map(|clause| clause.types.iter())
            .next()
        else {
            return;
        };
        let Some(expression) = base.expression else { return };
        let Some(id) = expression.node_id() else { return };
        let base_type = self.check_expression(expression);
        if self.is_error(base_type) || base_type == self.intrinsics.unresolved {
            return;
        }
        let flags = self.type_of(base_type).flags;
        if flags.intersects(TypeFlags::ANY | TypeFlags::NULL) {
            return;
        }
        let widened = self.get_base_type_of_literal_type(base_type);
        if !self.is_decidable_primitive(widened) {
            // An alias reference is read through its body, as native's alias
            // is its structural type.
            let body = self.binding_type_alias_body(base_type);
            let flags = self.type_of(body).flags;
            if body != base_type
                || flags.intersects(
                    TypeFlags::INSTANTIABLE | TypeFlags::UNION | TypeFlags::INTERSECTION,
                )
                || !flags.intersects(TypeFlags::OBJECT)
            {
                return;
            }
            match self.signatures_of_type_kind(body, crate::signatures::SignatureKind::Construct) {
                Some(signatures) if signatures.is_empty() => {}
                _ => return,
            }
        }
        let Some(file) = self.source_file_of_for_diagnostics(id) else { return };
        let span = self.error_span(id);
        let printed = self.type_to_string(base_type);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::TYPE_0_IS_NOT_A_CONSTRUCTOR_FUNCTION_TYPE,
                span,
                [printed],
            ),
        );
    }

    /// `checkReturnStatement`'s `noImplicitReturns` arm (`checker.go:4123`):
    /// outside `strictNullChecks`, a bare `return;` in a function other than a
    /// constructor whose return type (`getReturnTypeOfSignature`, annotated or
    /// inferred) is not `never` and does not unwrap to `undefined`, `void` or
    /// `any` (`isUnwrappedReturnTypeUndefinedVoidOrAny`, `checker.go:3780`) is
    /// TS7030 at the statement. A static block or a missing container is
    /// reported by `check_return_container` and ends upstream's check; a
    /// return type this port cannot compute ends this one.
    fn check_return_statement_implicit_returns(
        &mut self,
        node: NodeId,
        has_expression: bool,
        ambient: bool,
    ) {
        if !self.no_implicit_returns
            || self.strict_null_checks
            || has_expression
            || ambient
            || self.file_has_parse_errors
        {
            return;
        }
        let Some(container) =
            self.nodes.ancestors(node).find(|&a| self.is_function_like_or_static_block(a))
        else {
            return;
        };
        let (generator, modifiers) = match self.node_map.get(container) {
            Some(Node::FunctionDeclaration(f)) => (f.asterisk_token.is_some(), f.modifiers),
            Some(Node::FunctionExpression(f)) => (f.asterisk_token.is_some(), f.modifiers),
            Some(Node::MethodDeclaration(m)) => (m.asterisk_token.is_some(), m.modifiers),
            Some(Node::ArrowFunction(f)) => (false, f.modifiers),
            Some(Node::GetAccessorDeclaration(g)) => (false, g.modifiers),
            Some(Node::SetAccessorDeclaration(s)) => (false, s.modifiers),
            _ => return,
        };
        let Some(signature) = self.get_signature_from_declaration(container) else { return };
        let Some(return_type) = self.get_return_type_of_signature(&signature) else { return };
        if self.type_of(return_type).flags.contains(TypeFlags::NEVER) {
            return;
        }
        let is_async = has_modifier(modifiers, SyntaxKind::AsyncKeyword);
        let unwrapped = self.unwrap_return_type_for_code_paths(return_type, generator, is_async);
        if self.is_unwrapped_return_type_undefined_void_or_any(unwrapped) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(file, Diagnostic::new(&messages::NOT_ALL_CODE_PATHS_RETURN_A_VALUE, span));
    }

    /// TS2397 — `Declaration name conflicts with built-in global identifier '{0}'.`
    ///
    /// Two upstream guards, both syntactic and both in the global-table setup
    /// rather than in any `check*` function: `addUndefinedToGlobalsOrErrorOnRedeclaration`
    /// (`checker.go:1452`) and the `globalThis` loop (`:1302`). Only a **script**
    /// contributes to `c.globals`, so both reduce to one condition — a
    /// top-level, non-type declaration in a non-module file named `undefined`
    /// or `globalThis`. §442.
    fn check_builtin_global_redeclaration(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let name = match self.node_map.get(node) {
            Some(Node::VariableDeclaration(declaration)) => {
                declaration.name.and_then(|name| name.node_id())
            }
            Some(Node::FunctionDeclaration(f)) => f.name.and_then(|n| n.node_id),
            Some(Node::ClassDeclaration(c)) => c.name.and_then(|n| n.node_id),
            Some(Node::ModuleDeclaration(m)) => m.name.and_then(|n| n.node_id()),
            _ => return,
        };
        let Some(name) = name else { return };
        let Some(text) = self.identifier_text(name) else { return };
        if text != "undefined" && text != "globalThis" {
            return;
        }
        let text = text.to_string();
        // `c.globals` is fed by scripts only.
        let Some(file) = self.source_file_of_for_diagnostics(name) else { return };
        let Some(Node::SourceFile(source)) = self.node_map.get(file) else { return };
        if tsr_binder::is_external_module(source) {
            return;
        }
        // Top-level: the declaration's statement is a child of the file. A
        // `VariableDeclaration` sits under a list and a statement.
        let mut container = self.nodes.parent(node);
        while let Some(id) = container {
            match self.nodes.kind(id) {
                SyntaxKind::VariableDeclarationList | SyntaxKind::VariableStatement => {
                    container = self.nodes.parent(id);
                }
                SyntaxKind::SourceFile => break,
                _ => return,
            }
        }
        // **`if !ast.IsTypeDeclaration(declaration)`** (`checker.go:1457`).
        // The excluded kinds each have their own message for this name — a
        // class is TS2414, an interface TS2427 — and without the exclusion this
        // rule speaks over both and stays silent on the namespace, which is the
        // one declaration it is for. `IsTypeDeclaration` is a closed list at
        // `ast/utilities.go:3585`. §887.
        if matches!(
            self.nodes.kind(node),
            SyntaxKind::TypeParameter
                | SyntaxKind::ClassDeclaration
                | SyntaxKind::InterfaceDeclaration
                | SyntaxKind::TypeAliasDeclaration
                | SyntaxKind::EnumDeclaration
        ) {
            return;
        }
        let span = self.error_span(name);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::DECLARATION_NAME_CONFLICTS_WITH_BUILT_IN_GLOBAL_IDENTIFIER_0,
                span,
                [text],
            ),
        );
    }

    /// TS2417 — `Class static side '{0}' incorrectly extends base class static
    /// side '{1}'.`
    ///
    /// `checkClassLikeDeclaration`'s static-side arm (`checker.go:4337`):
    /// `checkTypeAssignableTo(staticType, getTypeWithoutSignatures(staticBaseType))`
    /// at the class name, reached only when the instance side is assignable
    /// (`checker.go:4333`). The structural relation of two `typeof` class
    /// types is `propertiesRelatedTo` (`relater.go`) over the base's static
    /// properties: each one the derived class also declares must agree on
    /// private/protected visibility and have an assignable type. Declines a
    /// generic class or base, a base written with type arguments or not as an
    /// identifier, a base merged across declarations, and any member pair the
    /// three-valued relation cannot definitely reject.
    fn check_static_side_assignability(&mut self, node: NodeId) {
        let Some(Node::ClassDeclaration(class)) = self.node_map.get(node) else { return };
        if !class.type_parameters.is_empty() {
            return;
        }
        let Some(name_id) = class.name.and_then(|name| name.node_id) else { return };
        let Some(extends) = class
            .heritage_clauses
            .iter()
            .find(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)
        else {
            return;
        };
        let [base] = extends.types else { return };
        if !base.type_arguments.is_empty() {
            return;
        }
        let Some(expression) = base.expression.and_then(|e| e.node_id()) else { return };
        let Some(text) = self.identifier_text(expression).map(str::to_string) else { return };
        let Some(base_symbol) = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            expression,
            &text,
            SymbolFlags::TYPE,
        ) else {
            return;
        };
        let base_symbol = self.binder.merged_symbol(base_symbol);
        let declarations = self.binder.symbols().get(base_symbol).declarations.clone();
        let [declaration] = declarations.as_slice() else { return };
        let Some(Node::ClassDeclaration(base_class)) = self.node_map.get(*declaration) else {
            return;
        };
        if !base_class.type_parameters.is_empty() {
            return;
        }
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        let symbol = self.binder.merged_symbol(symbol);
        // Only when the instance side is assignable; a failing instance side
        // is TS2415's report instead.
        let instance = self.get_declared_type_of_class_or_interface(symbol);
        let base_instance = self.get_declared_type_of_class_or_interface(base_symbol);
        if self.relate_ternary(instance, base_instance, crate::relater::Relation::Assignable)
            != crate::relater::Ternary::Related
        {
            return;
        }
        let base_statics: Vec<(String, tsr_binder::SymbolId)> = self
            .binder
            .symbols()
            .get(base_symbol)
            .exports
            .iter()
            .map(|(name, &member)| (name.to_string(), member))
            .collect();
        let mut failed = false;
        for (name, base_member) in base_statics {
            // A private name's symbol key is per declaring class
            // (`binder.GetSymbolNameForPrivateIdentifier`), so a derived `#x`
            // never matches the base's.
            if name == "prototype" || name.starts_with('#') {
                continue;
            }
            let Some(&member) = self.binder.symbols().get(symbol).exports.get(name.as_str()) else {
                continue;
            };
            let member = self.binder.merged_symbol(member);
            let base_member = self.binder.merged_symbol(base_member);
            if member == base_member {
                continue;
            }
            let visibility = |checker: &Self, member: tsr_binder::SymbolId| -> (bool, bool) {
                let entry = checker.binder.symbols().get(member);
                let modifiers = entry
                    .value_declaration
                    .or_else(|| entry.declarations.first().copied())
                    .and_then(|declaration| checker.node_map.get(declaration))
                    .and_then(modifiers_of)
                    .unwrap_or(&[]);
                (
                    has_modifier(modifiers, SyntaxKind::PrivateKeyword),
                    has_modifier(modifiers, SyntaxKind::ProtectedKeyword),
                )
            };
            let (private, protected) = visibility(self, member);
            let (base_private, base_protected) = visibility(self, base_member);
            // propertiesRelatedTo's visibility arms: private on either side
            // needs one declaration; a protected source needs a protected
            // target.
            if private || base_private || (protected && !base_protected) {
                failed = true;
                break;
            }
            let source = self.get_type_of_symbol(member);
            let target = self.get_type_of_symbol(base_member);
            if self.relate_ternary(source, target, crate::relater::Relation::Assignable)
                == crate::relater::Ternary::NotRelated
                && self.pair_is_reportable(source, target)
            {
                failed = true;
                break;
            }
        }
        if !failed {
            return;
        }
        let static_type = self.get_type_of_symbol(symbol);
        let base_static_type = self.get_type_of_symbol(base_symbol);
        let printed = [self.type_to_string(static_type), self.type_to_string(base_static_type)];
        let Some(file) = self.source_file_of_for_diagnostics(name_id) else { return };
        let span = self.error_span(name_id);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::CLASS_STATIC_SIDE_0_INCORRECTLY_EXTENDS_BASE_CLASS_STATIC_SIDE_1,
                span,
                printed,
            ),
        );
    }

    /// TS2358 — `The left-hand side of an 'instanceof' expression must be of
    /// type 'any', an object type or a type parameter.`
    ///
    /// `checkInstanceOfExpression` (`checker.go:13056`):
    /// `!IsTypeAny(leftType) && allTypesAssignableToKind(leftType, TypeFlagsPrimitive)`.
    /// §466 of `checker-notes-diag2.md` reported the four widened intrinsic
    /// primitives only; the flag arm of `isTypeAssignableToKind` is exact for
    /// every primitive-flagged type (`void`, `null`, `undefined`, `symbol`,
    /// literals, enums, and unions of them). Its assignability arm (a type
    /// parameter constrained to a primitive, a branded intersection, `never`)
    /// declines. `docs/parity/notes/misc-checks.md` §11.
    fn check_instanceof_left_operand(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(node) else { return };
        if binary.operator_token.is_none_or(|t| t.kind != SyntaxKind::InstanceOfKeyword) {
            return;
        }
        let Some(left) = binary.left else { return };
        let Some(id) = left.node_id() else { return };
        let left_type = self.check_expression(left);
        if !self.all_types_primitive_by_flags(left_type) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(id) else { return };
        let span = self.error_span(id);
        self.report(
            file,
            Diagnostic::new(
                &messages::THE_LEFT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_BE_OF_TYPE_ANY_AN_OBJECT_TYPE_OR_A_TYPE_PARAMETER,
                span,
            ),
        );
    }

    /// `allTypesAssignableToKind(t, TypeFlagsPrimitive)` (`checker.go:27628`)
    /// through its flag arm only: every union constituent carries a
    /// primitive flag. `false` where only the assignability arm could answer.
    fn all_types_primitive_by_flags(&self, ty: crate::types::TypeId) -> bool {
        match &self.store.get(ty).data {
            crate::types::TypeData::Union { types, .. } => {
                types.iter().all(|&member| self.all_types_primitive_by_flags(member))
            }
            _ => self.store.get(ty).flags.intersects(crate::flags::TypeFlags::PRIMITIVE),
        }
    }

    /// TS2359 — `The right-hand side of an 'instanceof' expression must be
    /// either of type 'any', a class, function, or other type assignable to
    /// the 'Function' interface type, or an object type with a
    /// 'Symbol.hasInstance' method.`
    ///
    /// `resolveInstanceofExpression` (`checker.go:8800`): a non-`any` right
    /// operand with no `[Symbol.hasInstance]` method
    /// (`getSymbolHasInstanceMethodOfObjectType`, `flow.go:2093`, which only
    /// looks when every constituent is non-primitive) must have call or
    /// construct signatures or be a subtype of `Function`.
    ///
    /// Ported for the two domains where every arm is decidable here:
    /// - **A primitive constituent** (`number`, `''`, `symbol`, `null` under
    ///   `strictNullChecks`, …). It skips the hasInstance lookup, leaves a
    ///   union with no union signatures, and is not a subtype of `Function`,
    ///   so the whole operand fails. `null`/`undefined` without
    ///   `strictNullChecks` are subtypes of everything and do not count.
    /// - **A certified plain object** ([`Checker::object_lacks_instanceof_target`]):
    ///   `{}`, `Object`, a class instance with no heritage and no computed
    ///   member.
    ///
    /// Everything else declines. `docs/parity/notes/misc-checks.md` §3.
    fn check_instanceof_right_operand(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(node) else { return };
        if binary.operator_token.is_none_or(|t| t.kind != SyntaxKind::InstanceOfKeyword) {
            return;
        }
        let Some(right) = binary.right else { return };
        let Some(id) = right.node_id() else { return };
        let right_type = self.check_expression(right);
        if self.is_error(right_type) {
            return;
        }
        let constituents = match &self.type_of(right_type).data {
            crate::types::TypeData::Union { types, .. } => types.clone(),
            _ => vec![right_type],
        };
        let fails = constituents.iter().any(|&t| self.primitive_outside_function(t))
            || (constituents.len() == 1 && self.object_lacks_instanceof_target(right_type));
        if !fails {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(id) else { return };
        let span = self.error_span(id);
        self.report(
            file,
            Diagnostic::new(
                &messages::THE_RIGHT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_BE_EITHER_OF_TYPE_ANY_A_CLASS_FUNCTION_OR_OTHER_TYPE_ASSIGNABLE_TO_THE_FUNCTION_INTERFACE_TYPE_OR_AN_OBJECT_TYPE_WITH_A_SYMBOL_HASINSTANCE_METHOD,
                span,
            ),
        );
    }

    /// A primitive type that is not a subtype of `Function`: every primitive
    /// but `never`, and `null`/`undefined` only under `strictNullChecks`.
    fn primitive_outside_function(&self, ty: crate::types::TypeId) -> bool {
        use crate::flags::TypeFlags;
        let flags = self.type_of(ty).flags;
        if !flags.intersects(TypeFlags::PRIMITIVE)
            || flags.intersects(TypeFlags::ANY_OR_UNKNOWN | TypeFlags::NEVER)
        {
            return false;
        }
        self.strict_null_checks || !flags.intersects(TypeFlags::NULLABLE)
    }

    /// An object type whose every `resolveInstanceofExpression` arm is
    /// certified negative: its declarations (a type literal, interface or
    /// class) have no heritage and no computed member (so no
    /// `[Symbol.hasInstance]`, which `Object` does not supply either), it has
    /// no call or construct signature, and no member is named `apply`, which
    /// `Function` requires, so it is not a subtype. (A plain declaration's
    /// properties are its own members plus `Object`'s, which has no `apply`.)
    fn object_lacks_instanceof_target(&mut self, ty: crate::types::TypeId) -> bool {
        use crate::signatures::SignatureKind;
        use crate::types::TypeData;
        if !self.type_of(ty).flags.intersects(crate::flags::TypeFlags::OBJECT) {
            return false;
        }
        let owner = match self.type_of(ty).data {
            TypeData::Named { members: Some(owner), .. } => owner,
            TypeData::Anonymous { symbol, .. } => symbol,
            _ => return false,
        };
        let declarations = self.binder.symbols().get(owner).declarations.clone();
        let mut shaping = 0usize;
        for declaration in declarations {
            // A merged value (`declare var Object: ObjectConstructor`) or
            // namespace contributes nothing to the declared object type.
            if matches!(
                self.nodes.kind(declaration),
                SyntaxKind::VariableDeclaration
                    | SyntaxKind::FunctionDeclaration
                    | SyntaxKind::ModuleDeclaration
            ) {
                continue;
            }
            let plain = match self.node_map.get(declaration) {
                Some(Node::TypeLiteralNode(_)) => true,
                Some(Node::InterfaceDeclaration(interface)) => {
                    interface.heritage_clauses.is_empty()
                }
                Some(Node::ClassDeclaration(class)) => class.heritage_clauses.is_empty(),
                // A spread's members are not the literal's own declarations.
                Some(Node::ObjectLiteralExpression(literal)) => !literal
                    .properties
                    .iter()
                    .any(|p| matches!(p, tsr_ast::ObjectLiteralElementLike::SpreadAssignment(_))),
                _ => false,
            };
            if !plain {
                return false;
            }
            shaping += 1;
            let mut members = Vec::new();
            let Some(typed) = self.node_map.get(declaration) else { return false };
            tsr_ast::for_each_child_id(typed, |child| members.push(child));
            // No computed name (so no `[Symbol.hasInstance]`), and no `apply`,
            // which `Function` requires of a subtype.
            if members.into_iter().any(|member| {
                self.declaration_name_of(member).is_some_and(|name| {
                    self.nodes.kind(name) == SyntaxKind::ComputedPropertyName
                        || self.identifier_text(name).is_none_or(|text| text == "apply")
                })
            }) {
                return false;
            }
        }
        if shaping == 0 {
            return false;
        }
        if self.signatures_of_type_kind(ty, SignatureKind::Call).is_none_or(|s| !s.is_empty())
            || self
                .signatures_of_type_kind(ty, SignatureKind::Construct)
                .is_none_or(|s| !s.is_empty())
        {
            return false;
        }
        true
    }

    /// TS6807 — `This operation can be simplified. This shift is identical to
    /// `{0} {1} {2}`.`
    ///
    /// `checkBinaryLikeExpressionWorker`'s shift arm (`checker.go:12402`):
    /// when both operands pass `checkArithmeticOperandType` (the caller's
    /// `operands_ok`, which is `leftOk && rightOk`), a shift count
    /// that evaluates to a number of magnitude 32 or more is reported with
    /// `errorOrSuggestion`, an **error** only when the shift (through
    /// parentheses) is an enum member's initializer. A suggestion is not a
    /// baseline diagnostic, so only the enum arm is ported.
    ///
    /// The count is `c.evaluate(right)`; this uses the symbol-free slice
    /// ([`crate::expressions::evaluate_constant_expression`]), so a count
    /// that names a constant or another member declines (a gap).
    /// `docs/parity/notes/misc-checks.md` §4.
    fn check_enum_member_overshift(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(node) else { return };
        let Some(operator) = binary.operator_token.map(|t| t.kind) else { return };
        let text = match operator {
            SyntaxKind::LessThanLessThanToken => "<<",
            SyntaxKind::GreaterThanGreaterThanToken => ">>",
            SyntaxKind::GreaterThanGreaterThanGreaterThanToken => ">>>",
            _ => return,
        };
        // ast.IsEnumMember(ast.WalkUpParenthesizedExpressions(right.Parent.Parent))
        let mut owner = self.nodes.parent(node);
        while let Some(parent) = owner
            && self.nodes.kind(parent) == SyntaxKind::ParenthesizedExpression
        {
            owner = self.nodes.parent(parent);
        }
        if owner.is_none_or(|owner| self.nodes.kind(owner) != SyntaxKind::EnumMember) {
            return;
        }
        let (Some(left), Some(right)) = (binary.left, binary.right) else { return };
        let Some(crate::expressions::EvaluatedValue::Number(count)) =
            crate::expressions::evaluate_constant_expression(&right)
        else {
            return;
        };
        if count.abs() < 32.0 {
            return;
        }
        // scanner.GetTextOfNode(left): the source text. The checker holds no
        // source bytes, so the two token forms whose text the AST keeps are
        // spelled and anything else declines.
        let left_text = match left {
            tsr_ast::Expression::NumericLiteral(literal) => literal.text.to_string(),
            tsr_ast::Expression::Identifier(identifier) => identifier.text.to_string(),
            _ => return,
        };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::THIS_OPERATION_CAN_BE_SIMPLIFIED_THIS_SHIFT_IS_IDENTICAL_TO_0_1_2,
                span,
                [left_text, text.to_string(), tsr_core::jsnum::format_number(count % 32.0)],
            ),
        );
    }

    /// TS17013 — `Meta-property '{0}' is only allowed in the body of a
    /// function declaration, function expression, or constructor.`
    ///
    /// `checkNewTargetMetaProperty` (`checker.go:10768`): `new.target` whose
    /// `GetNewTargetContainer` (`ast/utilities.go:2160`) is not a
    /// constructor, function declaration or function expression. The
    /// container walk is `GetThisContainer(node, false, false)`
    /// (`ast/utilities.go:1790`), transcribed in
    /// [`Checker::new_target_this_container`]. Entirely syntactic.
    fn check_new_target_meta_property(&mut self, node: NodeId) {
        let Some(Node::MetaProperty(meta)) = self.node_map.get(node) else { return };
        if meta.keyword_token.kind != SyntaxKind::NewKeyword {
            return;
        }
        let container = self.new_target_this_container(node);
        if container.is_some_and(|container| {
            matches!(
                self.nodes.kind(container),
                SyntaxKind::Constructor
                    | SyntaxKind::FunctionDeclaration
                    | SyntaxKind::FunctionExpression
            )
        }) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::META_PROPERTY_0_IS_ONLY_ALLOWED_IN_THE_BODY_OF_A_FUNCTION_DECLARATION_FUNCTION_EXPRESSION_OR_CONSTRUCTOR,
                span,
                ["new.target".to_string()],
            ),
        );
    }

    /// `GetThisContainer(node, includeArrowFunctions=false,
    /// includeClassComputedPropertyName=false)` (`ast/utilities.go:1790`).
    /// `None` only where upstream would panic (a detached node).
    fn new_target_this_container(&self, node: NodeId) -> Option<NodeId> {
        let mut current = node;
        loop {
            current = self.nodes.parent(current)?;
            match self.nodes.kind(current) {
                SyntaxKind::ComputedPropertyName => {
                    current = self.nodes.parent(current)?;
                    current = self.nodes.parent(current)?;
                }
                SyntaxKind::Decorator => {
                    let parent = self.nodes.parent(current)?;
                    if self.nodes.kind(parent) == SyntaxKind::Parameter
                        && self.nodes.parent(parent).is_some_and(|p| self.is_class_element(p))
                    {
                        current = self.nodes.parent(parent)?;
                    } else if self.is_class_element(parent) {
                        current = parent;
                    }
                }
                SyntaxKind::FunctionDeclaration
                | SyntaxKind::FunctionExpression
                | SyntaxKind::ModuleDeclaration
                | SyntaxKind::ClassStaticBlockDeclaration
                | SyntaxKind::PropertyDeclaration
                | SyntaxKind::PropertySignature
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::MethodSignature
                | SyntaxKind::Constructor
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor
                | SyntaxKind::CallSignature
                | SyntaxKind::ConstructSignature
                | SyntaxKind::IndexSignature
                | SyntaxKind::EnumDeclaration
                | SyntaxKind::SourceFile => return Some(current),
                _ => {}
            }
        }
    }

    /// Ported from typescript-go's `checkTypeParameter`
    /// (`internal/checker/checker.go:2603`), the TS2716 default-state check.
    /// Check the written default first: its nested parameter resolutions decide
    /// which participant native marks circular, including mutual defaults.
    fn check_circular_type_parameter_default(&mut self, node: NodeId) {
        let Some(Node::TypeParameterDeclaration(parameter)) = self.node_map.get(node) else {
            return;
        };
        let Some(default) = parameter.default_type else { return };
        let (Some(at), Some(symbol)) = (default.node_id(), self.binder.symbol_of(node)) else {
            return;
        };
        self.get_type_from_type_node(default);
        let ty = self.get_declared_type_of_symbol(symbol);
        if self.get_resolved_type_parameter_default(ty)
            != crate::declared::TypeParameterDefaultState::Circular
        {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        if !self.circularity_reported.insert(at) {
            return;
        }
        self.report(
            file,
            Diagnostic::with_args(
                &messages::TYPE_PARAMETER_0_HAS_A_CIRCULAR_DEFAULT,
                self.error_span(at),
                [self.binder.symbols().get(symbol).name.to_string()],
            ),
        );
    }

    /// TS2637 — `Variance annotations are only supported in type aliases for
    /// object, function, constructor, and mapped types.`
    ///
    /// `checkTypeParameterDeferred` (`checker.go:2627`): an `in`/`out`
    /// parameter of a type alias whose declared type has neither
    /// `ObjectFlagsAnonymous` nor `ObjectFlagsMapped`.
    ///
    /// This port keeps a generic alias's declared type as an unresolved
    /// reference, so the declared type's shape is read from the alias body
    /// instead ([`Checker::alias_body_kind`]): a type literal, function,
    /// constructor or mapped type is anonymous/mapped; a primitive, literal,
    /// tuple, array, union of non-objects, type parameter, or interface/class
    /// reference is not; a reference to another alias is followed with its
    /// type arguments substituted. Anything else (conditional, indexed
    /// access, `typeof`, intersections) declines.
    /// `docs/parity/notes/misc-checks.md` §6.
    fn check_type_alias_variance_annotation(&mut self, node: NodeId) {
        let Some(Node::TypeParameterDeclaration(parameter)) = self.node_map.get(node) else {
            return;
        };
        if !parameter.modifiers.iter().any(|modifier| {
            matches!(modifier, tsr_ast::ModifierLike::Token(token)
                if matches!(token.kind, SyntaxKind::InKeyword | SyntaxKind::OutKeyword))
        }) {
            return;
        }
        let Some(alias) = self.nodes.parent(node) else { return };
        let Some(Node::TypeAliasDeclaration(declaration)) = self.node_map.get(alias) else {
            return;
        };
        if self.in_js_file(node) {
            return;
        }
        let Some(body) = declaration.r#type.and_then(|t| t.node_id()) else { return };
        let mut frames = vec![AliasFrame { bindings: Vec::new(), parent: None }];
        if self.alias_body_kind(body, 0, &mut frames, 0) != Some(AliasBodyKind::NotAnonymous) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(
            file,
            Diagnostic::new(
                &messages::VARIANCE_ANNOTATIONS_ARE_ONLY_SUPPORTED_IN_TYPE_ALIASES_FOR_OBJECT_FUNCTION_CONSTRUCTOR_AND_MAPPED_TYPES,
                span,
            ),
        );
    }

    /// Whether the type a type node denotes is anonymous/mapped, read from
    /// syntax. `frame` holds the type-argument substitutions in force (an
    /// alias's type parameter declaration to the argument node written at
    /// the reference, evaluated in the referencing frame). `None` where the
    /// syntax does not decide it.
    fn alias_body_kind(
        &mut self,
        node: NodeId,
        frame: usize,
        frames: &mut Vec<AliasFrame>,
        depth: u32,
    ) -> Option<AliasBodyKind> {
        if depth > 16 {
            return None;
        }
        match self.nodes.kind(node) {
            SyntaxKind::ParenthesizedType => {
                let Some(Node::ParenthesizedTypeNode(inner)) = self.node_map.get(node) else {
                    return None;
                };
                let inner = inner.r#type?.node_id()?;
                self.alias_body_kind(inner, frame, frames, depth + 1)
            }
            SyntaxKind::TypeLiteral
            | SyntaxKind::FunctionType
            | SyntaxKind::ConstructorType
            | SyntaxKind::MappedType => Some(AliasBodyKind::Anonymous),
            SyntaxKind::AnyKeyword
            | SyntaxKind::UnknownKeyword
            | SyntaxKind::NumberKeyword
            | SyntaxKind::StringKeyword
            | SyntaxKind::BooleanKeyword
            | SyntaxKind::BigIntKeyword
            | SyntaxKind::SymbolKeyword
            | SyntaxKind::ObjectKeyword
            | SyntaxKind::VoidKeyword
            | SyntaxKind::UndefinedKeyword
            | SyntaxKind::NeverKeyword
            | SyntaxKind::LiteralType
            | SyntaxKind::TemplateLiteralType
            | SyntaxKind::TupleType
            | SyntaxKind::ArrayType => Some(AliasBodyKind::NotAnonymous),
            SyntaxKind::UnionType => {
                // A union stays a union unless reduction collapses it; only a
                // union of constituents that are all non-objects (and not
                // `never`, which reduction removes) is certain.
                let Some(Node::UnionTypeNode(union)) = self.node_map.get(node) else {
                    return None;
                };
                let all_primitive = union.types.iter().all(|member| {
                    member.node_id().is_some_and(|id| {
                        matches!(
                            self.nodes.kind(id),
                            SyntaxKind::NumberKeyword
                                | SyntaxKind::StringKeyword
                                | SyntaxKind::BooleanKeyword
                                | SyntaxKind::BigIntKeyword
                                | SyntaxKind::SymbolKeyword
                                | SyntaxKind::UndefinedKeyword
                                | SyntaxKind::LiteralType
                        )
                    })
                });
                all_primitive.then_some(AliasBodyKind::NotAnonymous)
            }
            SyntaxKind::TypeReference => {
                let Some(Node::TypeReferenceNode(reference)) = self.node_map.get(node) else {
                    return None;
                };
                let name = reference.type_name?;
                let name_id = name.node_id()?;
                if self.nodes.kind(name_id) != SyntaxKind::Identifier {
                    return None;
                }
                let text = self.identifier_text(name_id)?.to_string();
                let symbol = self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    name_id,
                    &text,
                    SymbolFlags::TYPE,
                )?;
                let symbol = self.binder.merged_symbol(symbol);
                let entry = self.binder.symbols().get(symbol);
                let flags = entry.flags;
                let declarations = entry.declarations.clone();
                if flags.intersects(SymbolFlags::ALIAS) {
                    return None;
                }
                if flags.intersects(SymbolFlags::TYPE_PARAMETER) {
                    let &[parameter] = declarations.as_slice() else { return None };
                    // A substituted parameter takes its argument's kind; an
                    // unsubstituted one is a type parameter type.
                    let mut current = Some(frame);
                    while let Some(index) = current {
                        if let Some(&(_, argument, argument_frame)) =
                            frames[index].bindings.iter().find(|(p, _, _)| *p == parameter)
                        {
                            return self.alias_body_kind(
                                argument,
                                argument_frame,
                                frames,
                                depth + 1,
                            );
                        }
                        current = frames[index].parent;
                    }
                    return Some(AliasBodyKind::NotAnonymous);
                }
                if flags.intersects(SymbolFlags::TYPE_ALIAS) {
                    let &[target] = declarations.as_slice() else { return None };
                    let Some(Node::TypeAliasDeclaration(target_alias)) = self.node_map.get(target)
                    else {
                        return None;
                    };
                    if target_alias.type_parameters.len() != reference.type_arguments.len() {
                        return None;
                    }
                    let mut bindings = Vec::with_capacity(reference.type_arguments.len());
                    for (parameter, argument) in
                        target_alias.type_parameters.iter().zip(reference.type_arguments)
                    {
                        bindings.push((parameter.node_id?, argument.node_id()?, frame));
                    }
                    let body = target_alias.r#type?.node_id()?;
                    // The alias body sees only its own parameters.
                    frames.push(AliasFrame { bindings, parent: None });
                    let inner = frames.len() - 1;
                    return self.alias_body_kind(body, inner, frames, depth + 1);
                }
                if flags.intersects(SymbolFlags::INTERFACE | SymbolFlags::CLASS | SymbolFlags::ENUM)
                {
                    return Some(AliasBodyKind::NotAnonymous);
                }
                None
            }
            _ => None,
        }
    }

    /// TS2376 — `A 'super' call must be the first statement in the constructor
    /// …when a derived class contains initialized properties, parameter
    /// properties, or private identifiers.`
    ///
    /// `checkConstructorDeclaration`'s else-branch (`checker.go:2868`): scan the
    /// constructor's statements for the first `super()` expression statement,
    /// stopping early at one that references `super` or `this`. Entirely
    /// syntactic, gate included. §470.
    fn check_super_call_is_first(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(Node::ClassDeclaration(class)) = self.node_map.get(node) else { return };
        let Some(extends) = class
            .heritage_clauses
            .iter()
            .find(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)
        else {
            return;
        };
        if extends.types.iter().any(|base| {
            base.expression
                .and_then(|e| e.node_id())
                .is_some_and(|id| self.nodes.kind(id) == SyntaxKind::NullKeyword)
        }) {
            return;
        }
        // "initialized properties, parameter properties, or private identifiers"
        let mut has_state = false;
        let mut constructor = None;
        for member in class.members {
            match member {
                tsr_ast::ClassElement::PropertyDeclaration(property) => {
                    if property.initializer.is_some()
                        || matches!(property.name, tsr_ast::PropertyName::PrivateIdentifier(_))
                    {
                        has_state = true;
                    }
                }
                tsr_ast::ClassElement::ConstructorDeclaration(each) => {
                    if each.parameters.iter().any(|parameter| {
                        parameter
                            .modifiers
                            .iter()
                            .any(|modifier| matches!(modifier, tsr_ast::ModifierLike::Token(_)))
                    }) {
                        has_state = true;
                    }
                    if each.body.is_some() {
                        constructor = Some(each);
                    }
                }
                _ => {}
            }
        }
        let (Some(constructor), true) = (constructor, has_state) else { return };
        let Some(body) = constructor.body.and_then(|body| body.node_id()) else { return };
        let Some(Node::Block(block)) = self.node_map.get(body) else { return };
        // **The whole arm is gated on a super call existing.**
        // `checkConstructorDeclaration` (`checker.go:2847`) computes
        // `findFirstSuperCall(body)` and enters the root-level check only when
        // it is non-nil; with no `super()` anywhere the constructor is
        // TS2377's and this code has nothing to be first. §470 ported the test
        // and not the gate. §785.
        if !self.subtree_has_super_call(body) {
            return;
        }
        let mut found = false;
        for statement in block.statements {
            let Some(id) = statement.node_id() else { continue };
            if self.statement_is_a_super_call(id) {
                found = true;
                break;
            }
            if self.subtree_references_super_or_this(id) {
                break;
            }
        }
        if found {
            return;
        }
        let Some(id) = constructor.node_id else { return };
        let Some(file) = self.source_file_of_for_diagnostics(id) else { return };
        let span = self.error_span(id);
        self.report(
            file,
            Diagnostic::new(
                &messages::A_SUPER_CALL_MUST_BE_THE_FIRST_STATEMENT_IN_THE_CONSTRUCTOR_TO_REFER_TO_SUPER_OR_THIS_WHEN_A_DERIVED_CLASS_CONTAINS_INITIALIZED_PROPERTIES_PARAMETER_PROPERTIES_OR_PRIVATE_IDENTIFIERS,
                span,
            ),
        );
    }

    /// TS2377 — `Constructors for derived classes must contain a 'super' call.`
    ///
    /// `checkConstructorDeclaration`'s final `else if` (`checker.go:2884`),
    /// reached when `findFirstSuperCall` returned nothing. §470 built the
    /// sibling branch — a super call present but not first; this is the one
    /// where there is none. An arrow function is **not** a boundary: an arrow
    /// keeps `super`, so a call inside one still counts. §485.
    fn check_derived_constructor_calls_super(&mut self, node: NodeId) {
        // Pinned `checkConstructorDeclaration` reports this semantic error
        // even with parse diagnostics; its early decline is a missing body.
        let Some(Node::ClassDeclaration(class)) = self.node_map.get(node) else { return };
        if !class
            .heritage_clauses
            .iter()
            .any(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)
        {
            return;
        }
        // `classDeclarationExtendsNull`: this parser spells the `null` of
        // `extends null` as an identifier, which a `NullKeyword` test missed
        // (`superCallBeforeThisAccessing5`, `classExtendsNull`).
        let extends_null = self.class_declaration_extends_null(node);
        for member in class.members {
            let tsr_ast::ClassElement::ConstructorDeclaration(constructor) = member else {
                continue;
            };
            let Some(body) = constructor.body.and_then(|body| body.node_id()) else { continue };
            // `ast.NodeIsMissing(node.Body())` includes an empty recovery Block.
            if self.nodes.span(body).is_empty() {
                continue;
            }
            if extends_null {
                // TS17005 — `A constructor cannot contain a 'super' call when
                // its class extends null.`, at `findFirstSuperCall`'s result
                // (`checker.go:2848`).
                if let Some(call) = self.first_super_call(body) {
                    let Some(file) = self.source_file_of_for_diagnostics(call) else { continue };
                    let span = self.error_span(call);
                    self.report(
                        file,
                        Diagnostic::new(
                            &messages::A_CONSTRUCTOR_CANNOT_CONTAIN_A_SUPER_CALL_WHEN_ITS_CLASS_EXTENDS_NULL,
                            span,
                        ),
                    );
                }
                continue;
            }
            if self.subtree_has_super_call(body) {
                continue;
            }
            let Some(id) = constructor.node_id else { continue };
            let Some(file) = self.source_file_of_for_diagnostics(id) else { continue };
            let span = self.error_span(id);
            self.report(
                file,
                Diagnostic::new(
                    &messages::CONSTRUCTORS_FOR_DERIVED_CLASSES_MUST_CONTAIN_A_SUPER_CALL,
                    span,
                ),
            );
        }
    }

    /// `findFirstSuperCall` — any call whose callee is `super`, anywhere in the
    /// subtree. Arrows are not a boundary. §485.
    fn subtree_has_super_call(&self, node: NodeId) -> bool {
        if matches!(self.node_map.get(node), Some(Node::CallExpression(call))
            if call.expression.and_then(|e| e.node_id())
                .is_some_and(|callee| self.nodes.kind(callee) == SyntaxKind::SuperKeyword))
        {
            return true;
        }
        // `case ast.IsFunctionLike(node): return false` (`checker.go:2897`).
        // **The boundary is function-like, not class-like** — a nested class's
        // `super()` is necessarily inside that class's own constructor, which
        // is function-like, so one test covers both. The same line is what
        // excludes an arrow function's `super()`, a judgement upstream makes
        // here rather than at the call site. §700.
        if self.is_function_like_or_static_block(node) {
            return false;
        }
        let mut children = Vec::new();
        if let Some(typed) = self.node_map.get(node) {
            tsr_ast::for_each_child_id(typed, |child| children.push(child));
        }
        children.into_iter().any(|child| self.subtree_has_super_call(child))
    }

    /// `findFirstSuperCall` (`checker.go:2889`): the first super call in
    /// child order, not entering function-like nodes — the node form of
    /// [`Checker::subtree_has_super_call`].
    fn first_super_call(&self, node: NodeId) -> Option<NodeId> {
        if matches!(self.node_map.get(node), Some(Node::CallExpression(call))
            if call.expression.and_then(|e| e.node_id())
                .is_some_and(|callee| self.nodes.kind(callee) == SyntaxKind::SuperKeyword))
        {
            return Some(node);
        }
        if self.is_function_like_or_static_block(node) {
            return None;
        }
        let mut children = Vec::new();
        if let Some(typed) = self.node_map.get(node) {
            tsr_ast::for_each_child_id(typed, |child| children.push(child));
        }
        children.into_iter().find_map(|child| self.first_super_call(child))
    }

    /// `IsExpressionStatement(s) && isSuperCall(SkipOuterExpressions(…))`. §470.
    fn statement_is_a_super_call(&self, node: NodeId) -> bool {
        let Some(Node::ExpressionStatement(statement)) = self.node_map.get(node) else {
            return false;
        };
        let Some(mut id) = statement.expression.and_then(|e| e.node_id()) else { return false };
        while let Some(Node::ParenthesizedExpression(inner)) = self.node_map.get(id) {
            let Some(next) = inner.expression.and_then(|e| e.node_id()) else { return false };
            id = next;
        }
        matches!(self.node_map.get(id), Some(Node::CallExpression(call))
            if call.expression.and_then(|e| e.node_id())
                .is_some_and(|callee| self.nodes.kind(callee) == SyntaxKind::SuperKeyword))
    }

    /// `nodeImmediatelyReferencesSuperOrThis` — a subtree scan that stops at
    /// function boundaries, the shape `subtree_has_return_or_throw` uses. §470.
    fn subtree_references_super_or_this(&self, node: NodeId) -> bool {
        if matches!(self.nodes.kind(node), SyntaxKind::SuperKeyword | SyntaxKind::ThisKeyword) {
            return true;
        }
        let mut children = Vec::new();
        if let Some(typed) = self.node_map.get(node) {
            tsr_ast::for_each_child_id(typed, |child| children.push(child));
        }
        children.into_iter().any(|child| {
            !self.is_function_like_or_static_block(child)
                && self.subtree_references_super_or_this(child)
        })
    }

    /// TS1268 — `An index signature parameter type must be 'string', 'number',
    /// 'symbol', or a template literal type.`
    ///
    /// `checkGrammarIndexSignature`'s fourth guard (`grammarchecks.go:831`),
    /// bounded to a **written keyword** annotation: the literal/generic guard
    /// (TS1337) runs before it, so a keyword reaches here only by being neither.
    /// Anything else — a reference, a template literal, a union — declines,
    /// because `isValidIndexKeyType` walks the type and this port would guess.
    ///
    /// §292 built seven of this function's guards for `+0`; this row's
    /// `occupied` is `0/4` where those were taken, which is the difference.
    /// §472.
    fn check_index_signature_key_type(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.index_signature_parameter_shape_error(node).is_some()
        {
            return;
        }
        let Some(Node::IndexSignatureDeclaration(signature)) = self.node_map.get(node) else {
            return;
        };
        let [parameter] = signature.parameters else { return };
        let Some(annotation) = parameter.r#type.and_then(|t| t.node_id()) else { return };
        let Some(Node::KeywordTypeNode(keyword)) = self.node_map.get(annotation) else { return };
        if matches!(
            keyword.kind,
            SyntaxKind::StringKeyword | SyntaxKind::NumberKeyword | SyntaxKind::SymbolKeyword
        ) {
            // TS1021 — the **fifth** guard of `checkGrammarIndexSignature`,
            // immediately after this one: the signature itself must carry a
            // return annotation. §230's rule — the remaining branch of a
            // half-ported function. §496.
            if signature.r#type.is_none()
                && let Some(file) = self.source_file_of_for_diagnostics(node)
            {
                let span = self.error_span(node);
                self.report(
                    file,
                    Diagnostic::new(
                        &messages::AN_INDEX_SIGNATURE_MUST_HAVE_A_TYPE_ANNOTATION,
                        span,
                    ),
                );
            }
            return;
        }
        let Some(name) = parameter.name.and_then(|name| name.node_id()) else { return };
        let Some(file) = self.source_file_of_for_diagnostics(name) else { return };
        let span = self.error_span(name);
        self.report(
            file,
            Diagnostic::new(
                &messages::AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LITERAL_TYPE,
                span,
            ),
        );
    }

    /// TS1184 — `Modifiers cannot appear here.`
    ///
    /// `findFirstIllegalModifier`'s **default** arm (`grammarchecks.go:614`): a
    /// modifier is legal on a statement only at the top level of a file or a
    /// module block. Bounded to a `VariableStatement`, which has no permitted
    /// modifier at all — the arm's other sub-cases keep one each (`async` on a
    /// function, `abstract` on a class) and need `findFirstModifierExcept`.
    /// §474.
    fn check_modifier_on_nested_statement(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        // `findFirstIllegalModifier`'s default arm (`grammarchecks.go:619`):
        // each kind keeps at most one modifier at a nested position. §474 built
        // the `VariableStatement` case and named the rest; §623 built them.
        let except = match self.node_map.get(node) {
            Some(Node::FunctionDeclaration(_)) => Some(SyntaxKind::AsyncKeyword),
            Some(Node::ClassDeclaration(_)) => Some(SyntaxKind::AbstractKeyword),
            Some(Node::EnumDeclaration(_)) => Some(SyntaxKind::ConstKeyword),
            // Every modifier is illegal on these at a nested position.
            Some(
                Node::VariableStatement(_)
                | Node::ClassExpression(_)
                | Node::InterfaceDeclaration(_)
                | Node::TypeAliasDeclaration(_),
            ) => None,
            _ => return,
        };
        let legal = self.nodes.parent(node).is_some_and(|parent| {
            matches!(self.nodes.kind(parent), SyntaxKind::ModuleBlock | SyntaxKind::SourceFile)
        });
        if legal {
            return;
        }
        let Some(modifiers) = self.node_map.get(node).and_then(modifiers_of) else { return };
        let Some(first) = modifiers.iter().find_map(|modifier| match modifier {
            tsr_ast::ModifierLike::Token(token) if Some(token.kind) != except => Some(token),
            _ => None,
        }) else {
            return;
        };
        let Some(id) = first.node_id else { return };
        let Some(file) = self.source_file_of_for_diagnostics(id) else { return };
        let span = self.nodes.span(id);
        self.report(file, Diagnostic::new(&messages::MODIFIERS_CANNOT_APPEAR_HERE, span));
    }

    /// TS1114 — `Duplicate label '{0}'.`
    ///
    /// `checkLabeledStatement` (`checker.go:4209`): walk up from the labeled
    /// statement, stopping at a function boundary, and report if an ancestor
    /// carries the same label text. Purely syntactic. §476.
    fn check_duplicate_label(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors {
            return;
        }
        let Some(Node::LabeledStatement(statement)) = self.node_map.get(node) else { return };
        let Some(label) = statement.label.and_then(|label| label.node_id) else { return };
        let Some(text) = self.identifier_text(label).map(str::to_string) else { return };
        for ancestor in self.nodes.ancestors(node) {
            if self.is_function_like_or_static_block(ancestor) {
                return;
            }
            let Some(Node::LabeledStatement(outer)) = self.node_map.get(ancestor) else { continue };
            let same = outer
                .label
                .and_then(|label| label.node_id)
                .and_then(|id| self.identifier_text(id))
                .is_some_and(|outer_text| outer_text == text);
            if !same {
                continue;
            }
            let Some(file) = self.source_file_of_for_diagnostics(label) else { return };
            let span = self.error_span(label);
            self.report(file, Diagnostic::with_args(&messages::DUPLICATE_LABEL_0, span, [text]));
            return;
        }
    }

    /// TS1192 — `Module '{0}' has no default export.`
    ///
    /// `reportNonDefaultExport`'s **second** arm (`checker.go:14595`). The
    /// first — the module exports something under the binding's own name — is a
    /// suggestion form with its own code and is excluded here, which is what
    /// keeps this from firing where upstream reports the other message.
    ///
    /// §186's rule holds: an **empty** exports table cannot be asked whether a
    /// member is missing, because the answer would be "all of them". §487.
    fn check_module_has_default_export(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(Node::ImportClause(clause)) = self.node_map.get(node) else { return };
        let Some(name) = clause.name.and_then(|name| name.node_id) else { return };
        let Some(text) = self.identifier_text(name).map(str::to_string) else { return };
        let Some(declaration) = self.nodes.parent(node) else { return };
        let Some(Node::ImportDeclaration(import)) = self.node_map.get(declaration) else { return };
        let Some(specifier) = import.module_specifier.and_then(|s| s.node_id()) else { return };
        let Some(symbol) = self.resolve_external_module_name(declaration, specifier) else {
            return;
        };
        let symbol = self.binder.merged_symbol(symbol);
        let exports = &self.binder.symbols().get(symbol).exports;
        if exports.is_empty() || exports.contains_key(text.as_str()) {
            return;
        }
        // `exportDefaultSymbol = resolveExportByName(moduleSymbol, "default", …)`
        // (`checker.go:14551`): through an `export =` value's properties, as
        // `canHaveSyntheticDefault` below reads it.
        if self.resolve_export_by_name(symbol, "default").is_some() {
            return;
        }
        // `exportDefaultSymbol == nil && !hasSyntheticDefault && !hasDefaultOnly`
        // (`checker.go:14566`) — the report is the **third** conjunct, and only
        // the first was ported. `canHaveSyntheticDefault` is asked with the
        // specifier as its usage, so the file formats decide under
        // `node16`..`nodenext` (`r5-modfmt.md` §2.5).
        if self.can_have_synthetic_default_for_usage(symbol, specifier) {
            return;
        }
        let printed = self.binder.symbols().get(symbol).name.to_string();
        let Some(file) = self.source_file_of_for_diagnostics(name) else { return };
        let span = self.error_span(name);
        self.report(
            file,
            Diagnostic::with_args(&messages::MODULE_0_HAS_NO_DEFAULT_EXPORT, span, [printed]),
        );
    }

    /// `canHaveSyntheticDefault` (`checker.go:14818`), the two arms this port
    /// can ask.
    ///
    /// A module with no `default` export is still importable as one when the
    /// importer may synthesise it, and **`esModuleInterop` makes that the
    /// normal case**. Every `@types` package is written
    ///
    /// ```ts
    /// declare namespace React { … }
    /// export = React;
    /// ```
    ///
    /// and `import React from "react"` is how every consumer writes it, so
    /// omitting this conjunct reported TS1192 on essentially every default
    /// import in a React codebase — 12 in one package.
    ///
    /// # The two arms
    ///
    /// - **A declaration file, or an ambient module with no file at all**
    ///   (`:14850`): a synthetic default is available unless the module
    ///   declares a syntactic `default` or an `__esModule` marker. Upstream's
    ///   comment is explicit that this is the *permissive* branch — there is no
    ///   marker at hand saying whether the accompanying JavaScript is ESM.
    /// - **Any other file** (`:14869`): TypeScript sources are emitted with an
    ///   `__esModule` marker, so a synthetic default exists only through
    ///   `export =` — `hasExportAssignmentSymbol`.
    ///
    /// # What is not ported, and which way it fails
    ///
    /// The `node16`/`nodenext` block (`:14823-14848`) compares the *usage*
    /// module format against the target's implied format, and needs
    /// `GetImpliedNodeFormatForEmit` per file. Its two early returns are one
    /// `true` and one `false`, so skipping it can fail in either direction —
    /// but only under those module kinds, which this port does not resolve
    /// per-file at all.
    ///
    /// The JavaScript arm (`:14874`) needs `ExternalModuleIndicator`; omitting
    /// it sends a `.js` module down the `export =` test, which is stricter, so
    /// the rule reports **more** there. `isOnlyImportableAsDefault` — the
    /// fourth conjunct at the call site — is not ported either, and is also a
    /// suppressor, so the same direction.
    ///
    /// Each of those is a **false positive** rather than a missed diagnostic,
    /// which is the direction this rule is already too loud in; they are
    /// recorded here so the next reading of its wrong column starts with them.
    pub(crate) fn can_have_synthetic_default(&self, module: tsr_binder::SymbolId) -> bool {
        let file = self
            .binder
            .symbols()
            .get(module)
            .declarations
            .iter()
            .copied()
            .find(|&declaration| self.nodes.kind(declaration) == SyntaxKind::SourceFile);
        // `file == nil || file.AsSourceFile().IsDeclarationFile`. `None` is an
        // ambient module — `declare module "x"` has no source file of its own —
        // which upstream folds into the same arm.
        let declaration_file = match file {
            None => true,
            Some(file) => self.module_host.is_some_and(|host| host.is_declaration_file(file)),
        };
        let exports = &self.binder.symbols().get(module).exports;
        if declaration_file {
            return !exports.contains_key("default") && !exports.contains_key("__esModule");
        }
        if let Some(file) = file
            && self.in_js_file(file)
            && let Some(Node::SourceFile(source)) = self.node_map.get(file)
        {
            return !tsr_binder::is_external_module(source) && !exports.contains_key("__esModule");
        }
        // `hasExportAssignmentSymbol(moduleSymbol)`.
        exports.contains_key("export=")
    }

    /// TS1141 — `String literal expected.`
    ///
    /// `getTypeFromImportTypeNode` (`checker.go:24578`): an `import(...)` type
    /// whose argument is not a literal type wrapping a string literal. A shape
    /// test on two nodes, with no other conjunct, reported at the argument.
    /// §489.
    fn check_import_type_argument(&mut self, node: NodeId) {
        // A `c.error`, not a grammar report: it stands in a file with parse
        // errors (`typeofImportDefer`).
        let Some(Node::ImportTypeNode(import)) = self.node_map.get(node) else { return };
        let Some(argument) = import.argument.and_then(|a| a.node_id()) else { return };
        let is_string_literal_type = matches!(
            self.node_map.get(argument),
            Some(Node::LiteralTypeNode(literal))
                if literal.literal.and_then(|l| l.node_id())
                    .is_some_and(|id| self.nodes.kind(id) == SyntaxKind::StringLiteral)
        );
        if is_string_literal_type {
            // **A module specifier written in a type position.**
            // `getTypeFromImportTypeNode` (`checker.go:24578`) resolves it
            // after the shape test, through the same
            // `resolveExternalModuleName` an import declaration's specifier
            // goes through, and reports the same code. §753.
            let literal = match self.node_map.get(argument) {
                Some(Node::LiteralTypeNode(node)) => node.literal.and_then(|l| l.node_id()),
                _ => None,
            };
            // **Not through `check_module_specifier`.** That entry point
            // carries `checkExternalImportOrExportDeclaration`'s position test
            // — a *declaration* must sit at file or ambient-module-block level
            // — and an `import(...)` type is nested in an annotation, so the
            // test declines it. `getTypeFromImportTypeNode` applies no such
            // test. The TS7016 branch is not ported here; an untyped JavaScript
            // target of an import type stays silent rather than getting the
            // wrong code. §753.
            if let Some(literal) = literal {
                self.check_esm_import_from_commonjs(node, literal);
            }
            if let Some(literal) = literal
                && self.module_specifier_unfindable_for_diagnostics(literal)
                && let Some(Node::StringLiteral(text)) = self.node_map.get(literal)
                && let Some(file) = self.source_file_of_for_diagnostics(literal)
            {
                let span = self.error_span(literal);
                self.report_module_not_found(
                    file,
                    literal,
                    span,
                    text.text,
                    &messages::CANNOT_FIND_MODULE_0_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS,
                );
            }
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(argument) else { return };
        let span = self.error_span(argument);
        self.report(file, Diagnostic::new(&messages::STRING_LITERAL_EXPECTED, span));
    }

    /// The `for…in` / `for…of` declaration-list grammar
    /// (`grammarchecks.go:1271`–`:1299`): three sequential shape tests, each
    /// returning on its first hit, each with a `for…in` and a `for…of` message.
    /// Shipped together per §230 — one branch of a multi-branch guard gives a
    /// case the wrong code at the right position. §491.
    fn check_for_in_or_of_declarations(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(Node::ForInOrOfStatement(statement)) = self.node_map.get(node) else { return };
        let for_in = self.nodes.kind(node) == SyntaxKind::ForInStatement;
        let Some(initializer) = statement.initializer else { return };
        let Some(list) = initializer.node_id() else { return };
        // **TS2491, `for…in` only.** `checkForInStatement` (`checker.go:3996`,
        // `:4008`): one message, two arms — a declaration list whose first name
        // is a binding pattern, or an initialiser that is an array/object
        // literal. A pattern is legal in `for…of`, which is why upstream keeps
        // this guard out of §491's shared grammar block. §493.
        if for_in {
            let pattern_name = match self.node_map.get(list) {
                Some(Node::VariableDeclarationList(declarations)) => declarations
                    .declarations
                    .first()
                    .and_then(|first| first.name.as_ref())
                    .filter(|name| matches!(name, tsr_ast::BindingName::BindingPattern(_)))
                    .and_then(tsr_ast::BindingName::node_id),
                _ => matches!(
                    self.nodes.kind(list),
                    SyntaxKind::ArrayLiteralExpression | SyntaxKind::ObjectLiteralExpression
                )
                .then_some(list),
            };
            if let Some(id) = pattern_name
                && let Some(file) = self.source_file_of_for_diagnostics(id)
            {
                let span = self.error_span(id);
                self.report(
                    file,
                    Diagnostic::new(
                        &messages::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_BE_A_DESTRUCTURING_PATTERN,
                        span,
                    ),
                );
                return;
            }
        }
        let Some(Node::VariableDeclarationList(declarations)) = self.node_map.get(list) else {
            return;
        };
        let [first, rest @ ..] = declarations.declarations else { return };
        if let Some(second) = rest.first()
            && let Some(id) = second.node_id
            && let Some(file) = self.source_file_of_for_diagnostics(id)
        {
            let span = self.error_span(id);
            let message = if for_in {
                &messages::ONLY_A_SINGLE_VARIABLE_DECLARATION_IS_ALLOWED_IN_A_FOR_IN_STATEMENT
            } else {
                &messages::ONLY_A_SINGLE_VARIABLE_DECLARATION_IS_ALLOWED_IN_A_FOR_OF_STATEMENT
            };
            self.report(file, Diagnostic::new(message, span));
            return;
        }
        if first.initializer.is_some()
            && let Some(name) = first.name.as_ref().and_then(tsr_ast::BindingName::node_id)
            && let Some(file) = self.source_file_of_for_diagnostics(name)
        {
            let span = self.error_span(name);
            let message = if for_in {
                &messages::THE_VARIABLE_DECLARATION_OF_A_FOR_IN_STATEMENT_CANNOT_HAVE_AN_INITIALIZER
            } else {
                &messages::THE_VARIABLE_DECLARATION_OF_A_FOR_OF_STATEMENT_CANNOT_HAVE_AN_INITIALIZER
            };
            self.report(file, Diagnostic::new(message, span));
            return;
        }
        if first.r#type.is_some()
            && let Some(id) = first.node_id
            && let Some(file) = self.source_file_of_for_diagnostics(id)
        {
            let span = self.error_span(id);
            let message = if for_in {
                &messages::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_USE_A_TYPE_ANNOTATION
            } else {
                &messages::THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_CANNOT_USE_A_TYPE_ANNOTATION
            };
            self.report(file, Diagnostic::new(message, span));
        }
    }

    /// TS1245 — `Method '{0}' cannot have an implementation because it is
    /// marked abstract.`
    ///
    /// `checkMethodDeclaration` (`checker.go:2808`): three conjuncts, all
    /// shape — the `abstract` modifier, the node kind, and a present body.
    /// §503.
    fn check_abstract_method_has_no_body(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(Node::MethodDeclaration(method)) = self.node_map.get(node) else { return };
        if method.body.is_none() || !has_modifier(method.modifiers, SyntaxKind::AbstractKeyword) {
            return;
        }
        let text = declaration_name_to_string(method.name).unwrap_or_default();
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::METHOD_0_CANNOT_HAVE_AN_IMPLEMENTATION_BECAUSE_IT_IS_MARKED_ABSTRACT,
                span,
                [text],
            ),
        );
    }

    /// TS1242 — `'abstract' modifier can only appear on a class, method, or
    /// property declaration.`
    ///
    /// `checkGrammarModifiers`' `abstract` arm (`grammarchecks.go:471`), the
    /// **outer** of two nested kind tests: six permitted kinds, everything else
    /// an error at the modifier. The inner test — an abstract member outside an
    /// abstract class — carries a different code and is **not ported here**
    /// (§501's rule: name the arm you did not take). §505.
    fn check_abstract_modifier_position(&mut self, node: NodeId, typed: Node<'_>) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(modifiers) = modifiers_of(typed) else { return };
        let Some(tsr_ast::ModifierLike::Token(token)) = modifiers
            .iter()
            .find(|m| matches!(m, tsr_ast::ModifierLike::Token(t) if t.kind == SyntaxKind::AbstractKeyword))
        else {
            return;
        };
        if matches!(
            self.nodes.kind(node),
            SyntaxKind::ClassDeclaration
                | SyntaxKind::ConstructorType
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::PropertyDeclaration
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor
        ) {
            return;
        }
        let Some(id) = token.node_id else { return };
        let Some(file) = self.source_file_of_for_diagnostics(id) else { return };
        let span = self.nodes.span(id);
        self.report(
            file,
            Diagnostic::new(
                &messages::ABSTRACT_MODIFIER_CAN_ONLY_APPEAR_ON_A_CLASS_METHOD_OR_PROPERTY_DECLARATION,
                span,
            ),
        );
    }

    /// TS1155 — `'{0}' declarations must be initialized.`
    ///
    /// `checkGrammarVariableDeclaration` (`grammarchecks.go:1569`), **`using`
    /// and `await using` only**. Upstream's switch has a third arm for `const`;
    /// §497 measured the three together at `−5` and this port declines that arm
    /// with a measured reason — a `const` with no initialiser is legal in an
    /// ambient context and after a grammar error, and §257 recorded 227 wrong
    /// lines from one case when a related rule mishandled exactly that. §509.
    fn check_using_is_initialized(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(Node::VariableDeclaration(declaration)) = self.node_map.get(node) else { return };
        let Some(list) = self.nodes.parent(node) else { return };
        // `checkGrammarVariableDeclaration` runs from `checkVariableDeclaration`,
        // which only a declaration list reaches: a catch clause's declaration
        // goes through `checkCatchClause` (`checker.go:4247`), which calls
        // `checkVariableLikeDeclaration` alone, so `catch ({ a })` has no
        // TS1182.
        if self.nodes.kind(list) != SyntaxKind::VariableDeclarationList {
            return;
        }
        let Some(statement) = self.nodes.parent(list) else { return };
        // **Guard four** (`grammarchecks.go:1573`), which sits between §517's
        // and §509's and is not `using`-specific — so it has to be tested
        // before the `USING` gate below. §518's rule, applied on the third
        // insertion into this one function. §632.
        let for_in_or_of = matches!(
            self.nodes.kind(statement),
            SyntaxKind::ForInStatement | SyntaxKind::ForOfStatement
        );
        if !self.nodes.flags(list).contains(tsr_ast::NodeFlags::USING)
            && !for_in_or_of
            && declaration.initializer.is_none()
            && matches!(declaration.name, Some(tsr_ast::BindingName::BindingPattern(_)))
            && !self.declaration_is_in_an_ambient_context(node)
        {
            if let Some(file) = self.source_file_of_for_diagnostics(node) {
                let span = self.error_span(node);
                self.report(
                    file,
                    Diagnostic::new(
                        &messages::A_DESTRUCTURING_DECLARATION_MUST_HAVE_AN_INITIALIZER,
                        span,
                    ),
                );
            }
            return;
        }
        if !self.nodes.flags(list).contains(tsr_ast::NodeFlags::USING) {
            return;
        }
        let awaited = self.nodes.flags(list).contains(tsr_ast::NodeFlags::CONSTANT)
            || matches!(self.node_map.get(statement), Some(Node::VariableStatement(s))
            if has_modifier(s.modifiers, SyntaxKind::AwaitKeyword))
            || matches!(self.node_map.get(statement), Some(Node::ForInOrOfStatement(f))
                if f.await_modifier.is_some());
        // **TS1492 is the FIRST guard of `checkGrammarVariableDeclaration`
        // (`grammarchecks.go:1559`) and returns before the initialiser one**, so
        // a `using [a]` with no initialiser reports the pattern message, not
        // TS1155. §103's order, inside one rule. §517.
        if matches!(declaration.name, Some(tsr_ast::BindingName::BindingPattern(_))) {
            let keyword = if awaited { "await using" } else { "using" };
            if let Some(file) = self.source_file_of_for_diagnostics(node) {
                let span = self.error_span(node);
                self.report(
                    file,
                    Diagnostic::with_args(
                        &messages::_0_DECLARATIONS_MAY_NOT_HAVE_BINDING_PATTERNS,
                        span,
                        [keyword.to_string()],
                    ),
                );
            }
            return;
        }
        if declaration.initializer.is_some() || declaration.exclamation_token.is_some() {
            return;
        }
        if matches!(
            self.nodes.kind(statement),
            SyntaxKind::ForInStatement | SyntaxKind::ForOfStatement
        ) {
            return;
        }
        if self.declaration_is_in_an_ambient_context(node) {
            return;
        }
        let keyword = if awaited { "await using" } else { "using" };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::_0_DECLARATIONS_MUST_BE_INITIALIZED,
                span,
                [keyword.to_string()],
            ),
        );
    }

    /// TS1359 — `Identifier expected. '{0}' is a reserved word that cannot be
    /// used here.`
    ///
    /// `binder.go:1314`'s `await` arm, whose test is `node.Flags &
    /// NodeFlagsAwaitContext`. That flag is unset in this parser — and it
    /// records a **context**, which is a property of the ancestor chain, so the
    /// walk answers it: is there an enclosing function-like carrying `async`?
    /// The same substitution §508 made for `NodeFlagsAmbient`. A function's own
    /// `async` counts for its own name (`asyncFunctionDeclaration12`).
    ///
    /// The top-level-module arm above it is a different code and is not ported
    /// here (§501). §513.
    fn check_await_as_binding_name(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        // `await` needs an enclosing `async`; `yield` needs an enclosing
        // generator. Two arms of one `else if` chain on one node, so they share
        // a rule (§497) — and §494's differential applies within it: the arms
        // are independent and each must keep firing when the other is added.
        // §515.
        let keyword = match self.identifier_text(node) {
            Some("await") => "await",
            // **`yield` is not the symmetric case.** §515 added it as the
            // sibling arm — an enclosing generator instead of an enclosing
            // `async` — and measured −4. Upstream's `NodeFlagsYieldContext` is
            // set by the *parser* on the tokens it scans in a yield context,
            // which is narrower than "somewhere inside a generator": a nested
            // non-generator function inside a generator is not in yield context,
            // and the ancestor walk cannot see that boundary the way it can see
            // `async`, because `async` propagates to nested arrows and `yield`
            // does not. §516.
            _ => return,
        };
        // A binding-name position, not an expression.
        let Some(parent) = self.nodes.parent(node) else { return };
        let named = match self.node_map.get(parent) {
            Some(Node::ParameterDeclaration(p)) => {
                p.name.as_ref().and_then(tsr_ast::BindingName::node_id) == Some(node)
            }
            Some(Node::VariableDeclaration(v)) => {
                v.name.as_ref().and_then(tsr_ast::BindingName::node_id) == Some(node)
            }
            Some(Node::FunctionExpression(f)) => f.name.and_then(|n| n.node_id) == Some(node),
            Some(Node::FunctionDeclaration(f)) => f.name.and_then(|n| n.node_id) == Some(node),
            _ => false,
        };
        if !named {
            return;
        }
        // A function *declaration's* name is parsed before the await context
        // its `async` opens (`parseFunctionDeclaration`, `parser.go:1715`), so
        // `async function await() {}` is legal; an expression's name is parsed
        // inside it (`asyncFunctionDeclaration11` vs `…12`).
        let own_declaration =
            matches!(self.node_map.get(parent), Some(Node::FunctionDeclaration(_)));
        let in_context = self.nodes.ancestors(node).any(|ancestor| {
            if own_declaration && ancestor == parent {
                return false;
            }
            let Some(typed) = self.node_map.get(ancestor) else { return false };
            modifiers_of(typed)
                .is_some_and(|modifiers| has_modifier(modifiers, SyntaxKind::AsyncKeyword))
        });
        if !in_context {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::IDENTIFIER_EXPECTED_0_IS_A_RESERVED_WORD_THAT_CANNOT_BE_USED_HERE,
                span,
                [keyword.to_string()],
            ),
        );
    }

    /// TS1156 — `'{0}' declarations can only be declared inside a block.`
    ///
    /// `checkGrammarForDisallowedBlockScopedVariableStatement` with
    /// `containerAllowsBlockScopedVariable` (`grammarchecks.go:1790`, `:1814`).
    /// Entirely syntactic: a `let`/`const`/`using` statement in the *body*
    /// position of one of seven statement kinds, seen through any number of
    /// labels.
    ///
    /// A `for` initialiser is a declaration *list*, not a `VariableStatement`,
    /// so it never reaches here — the seven kinds are about body position. §395.
    fn check_block_scoped_statement_container(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(Node::VariableStatement(statement)) = self.node_map.get(node) else { return };
        let Some(list) = statement.declaration_list.and_then(|l| l.node_id) else { return };
        let flags = self.nodes.flags(list);
        let keyword = if flags.contains(tsr_ast::NodeFlags::USING) {
            if flags.contains(tsr_ast::NodeFlags::CONSTANT)
                || has_modifier(statement.modifiers, SyntaxKind::AwaitKeyword)
            {
                "await using"
            } else {
                "using"
            }
        } else if flags.contains(tsr_ast::NodeFlags::CONST) {
            "const"
        } else if flags.contains(tsr_ast::NodeFlags::LET) {
            "let"
        } else {
            return;
        };
        if !self.container_allows_block_scoped(node) {
            self.report_block_scoped_container(node, keyword);
        }
    }

    /// TS1156's other two sites — a **type alias** (`checker.go:6881`) and an
    /// **interface** (`checker.go:4996`), which share
    /// `containerAllowsBlockScopedVariable` with §395's variable form and could
    /// not reach it while the predicate lived inside that rule. §624's rule,
    /// third instance. §660.
    fn check_declaration_statement_container(&mut self, node: NodeId, typed: Node<'_>) {
        if self.file_has_parse_errors {
            return;
        }
        let keyword = match typed {
            Node::TypeAliasDeclaration(_) => "type",
            Node::InterfaceDeclaration(_) => "interface",
            _ => return,
        };
        if !self.container_allows_block_scoped(node) {
            self.report_block_scoped_container(node, keyword);
        }
    }

    /// `containerAllowsBlockScopedVariable` (`grammarchecks.go:1814`), which
    /// recurses through a `LabeledStatement`. §660.
    fn container_allows_block_scoped(&self, node: NodeId) -> bool {
        let mut parent = self.nodes.parent(node);
        while let Some(container) = parent {
            match self.nodes.kind(container) {
                SyntaxKind::LabeledStatement => parent = self.nodes.parent(container),
                SyntaxKind::IfStatement
                | SyntaxKind::DoStatement
                | SyntaxKind::WhileStatement
                | SyntaxKind::WithStatement
                | SyntaxKind::ForStatement
                | SyntaxKind::ForInStatement
                | SyntaxKind::ForOfStatement => return false,
                _ => return true,
            }
        }
        true
    }

    /// The shared report for TS1156's three sites. §660.
    fn report_block_scoped_container(&mut self, node: NodeId, keyword: &str) {
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        // `getErrorRangeForNode` narrows a **named** declaration to its name, so
        // `if (true) type s = string` reports on the `s`. A `VariableStatement`
        // has no name and `error_span` leaves it alone, which is why §395's arm
        // is unaffected. §660.
        let span = self.error_span(node);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::_0_DECLARATIONS_CAN_ONLY_BE_DECLARED_INSIDE_A_BLOCK,
                span,
                [keyword.to_string()],
            ),
        );
    }

    /// TS18050 — `The value '{0}' cannot be used here.`
    ///
    /// The two syntactic arms at the head of
    /// `reportObjectPossiblyNullOrUndefinedError` (`checker.go:7455`): a literal
    /// `null` receiver, and the global `undefined` as a receiver. Everything
    /// after them is `TypeFacts` and stays unported.
    ///
    /// Neither arm is gated on `strictNullChecks` — `nullKeyword.ts` sets no
    /// directive and upstream reports anyway, because `null.foo` is wrong under
    /// every flag. §397.
    fn check_null_or_undefined_receiver(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let (receiver, is_chain_root) = match self.node_map.get(node) {
            Some(Node::PropertyAccessExpression(access)) => {
                (access.expression, access.question_dot_token.is_some())
            }
            Some(Node::ElementAccessExpression(access)) => {
                (access.expression, access.question_dot_token.is_some())
            }
            _ => return,
        };
        let receiver_expression = receiver;
        let Some(receiver) = receiver.and_then(|e| e.node_id()) else { return };
        // **`a?.b` asks the question of a receiver that has already been
        // stripped.** `checkPropertyAccessExpression` (`checker.go:11249`)
        // routes a chain link through `checkPropertyAccessChain`, which hands
        // `checkNonNullType` the result of `getOptionalExpressionType`
        // (`checker.go:29064`): `getNonNullableType` at a chain **root**, and
        // `removeOptionalTypeMarker` at an inner link. Every arm below reads its
        // facts from *that* type, which is the one `checkNonNullType` is given.
        //
        // **This replaced an early `return` at the chain root.** The return
        // reached the same answers — a stripped receiver has no nullable facts,
        // so no arm could fire — but it reached them by not asking, which made
        // it a rule about `?.` rather than a rule about the type. The
        // difference is not visible today and would be the moment TS18046
        // (`checkNonNullType`'s `unknown` arm, `checker.go:7409`) is ported:
        // `u?.x` with `u: unknown` *does* report upstream, and a skip keyed on
        // `?.` would have silently swallowed it. §5 of
        // `checker-notes-nnaccess.md`.
        let receiver_type =
            receiver_expression.map_or(self.intrinsics.error, |e| self.check_expression(e));
        let non_optional =
            self.get_optional_expression_type(receiver_type, Some(receiver), is_chain_root);
        // `checkNonNullType` reaches `reportObjectPossiblyNullOrUndefinedError`
        // only when the facts say nullish (`checker.go:7425`), which is why
        // `null?.x` is silent while `null.x` is not — the chain root strips the
        // `null` to `never` and there is nothing left to report on.
        //
        // **The test guards the two syntactic arms only.** The type-based arms
        // below hand `non_optional` to the reporter, which runs this same test
        // itself — and runs it *after* §910's `extends null` stand-in has had a
        // chance to set the fact that `check_expression(super)` cannot yet
        // supply. Testing here as well would be a second, earlier gate that
        // §910's arm never gets past: it costs `compiler/classExtendsNull3`,
        // measured, which is how this was found.
        let syntactically_nullish = {
            let (null, undefined) = self.nullish_facts(non_optional);
            null || undefined
        };
        let text = match self.nodes.kind(receiver) {
            SyntaxKind::NullKeyword if syntactically_nullish => "null",
            SyntaxKind::Identifier => {
                let Some(name) = self.identifier_text(receiver) else { return };
                // **An identifier that is not `undefined` still has a type**,
                // and it is by far the commonest nullable receiver. This arm
                // returned here, so §849's fall-through never saw an
                // identifier — a probe on `f11.toFixed()` printed nothing at
                // all, which is what said the site was still unreached rather
                // than the facts being wrong. §850.
                if name != "undefined" {
                    if let Some(expression) = receiver_expression {
                        self.report_nullable_operand_of_type(expression, non_optional);
                    }
                    return;
                }
                // A shadowed `undefined` is legal; the arm is about the global.
                if self
                    .binder
                    .resolve_name(
                        self.nodes,
                        self.node_map,
                        receiver,
                        "undefined",
                        SymbolFlags::VALUE,
                    )
                    .is_some_and(|symbol| {
                        !self
                            .binder
                            .symbols()
                            .get(self.binder.merged_symbol(symbol))
                            .declarations
                            .is_empty()
                    })
                {
                    return;
                }
                // The global `undefined` as a chain root is stripped to
                // `never` like the `null` keyword above, so the same test
                // applies before this arm's TS18050.
                if !syntactically_nullish {
                    return;
                }
                "undefined"
            }
            // **Not a literal spelling — the type-based arms.** Upstream's
            // `checkNonNullExpression` reaches
            // `reportObjectPossiblyNullOrUndefinedError`, whose first branch is
            // the spelling test above and whose other five are keyed on the
            // facts. This port had the reporter and wired it to two arithmetic
            // sites only, which is why three of its six arms never fired
            // (§846); a nullable *receiver* is the site that actually occurs.
            // §849.
            _ => {
                let Some(expression) = receiver_expression else { return };
                self.report_nullable_operand_of_type(expression, non_optional);
                return;
            }
        };
        let Some(file) = self.source_file_of_for_diagnostics(receiver) else { return };
        let span = self.nodes.span(receiver);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::THE_VALUE_0_CANNOT_BE_USED_HERE,
                span,
                [text.to_string()],
            ),
        );
    }

    /// TS1113 — `A 'default' clause cannot appear more than once in a 'switch'
    /// statement.`
    ///
    /// `hasDuplicateDefaultClause` is a **latch** (`checker.go:4180`): only the
    /// *second* `default:` reports, however many follow. §762's duplicate-index
    /// rule reports every one instead — the two are read off upstream rather
    /// than assumed either way.
    ///
    /// `docs/architecture/checker-notes-diag2.md` §815.
    fn check_duplicate_default_clause(&mut self, statement: &tsr_ast::SwitchStatement<'_>) {
        let Some(case_block) = statement.case_block else { return };
        let mut seen_default = false;
        for clause in case_block.clauses {
            let Some(id) = tsr_ast::Node::from(*clause).node_id() else { continue };
            if self.nodes.kind(id) != SyntaxKind::DefaultClause {
                continue;
            }
            if seen_default {
                if let Some(file) = self.source_file_of_for_diagnostics(id) {
                    let span = self.error_span(id);
                    self.report(
                        file,
                        Diagnostic::new(
                            &messages::A_DEFAULT_CLAUSE_CANNOT_APPEAR_MORE_THAN_ONCE_IN_A_SWITCH_STATEMENT,
                            span,
                        ),
                    );
                }
                return;
            }
            seen_default = true;
        }
    }

    /// `checkSwitchStatement`'s per-clause checks: TS1113 here, TS2678 in
    /// `crate::comparison_overlap`.
    fn check_switch_case_comparable(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(Node::SwitchStatement(statement)) = self.node_map.get(node) else { return };
        self.check_duplicate_default_clause(statement);
        self.check_switch_case_comparability(statement);
    }

    /// TS2307 — `Cannot find module '{0}' or its corresponding type declarations.`
    ///
    /// Upstream's site is the **fallthrough** of `resolveExternalModule`
    /// (`checker.go:15149`), reached from `resolveExternalModuleName`
    /// (`checker.go:15100`) after `getCannotResolveModuleNameErrorForSpecificModule`
    /// has declined to substitute a more specific message. The error node is the
    /// specifier literal itself, so the reported column is the **opening quote**
    /// — `badExternalModuleReference.errors.txt` records `(1,21)` for
    /// `import a1 = require("garbage")`, and 21 is the `"`.
    ///
    /// # This emits on a strict subset of upstream's condition, on purpose
    ///
    /// Upstream's function is 190 lines carrying **fourteen** distinct messages,
    /// and TS2307 is what is left when every one of them declines. Each of the
    /// guards below is a situation where upstream reports a *different code*, so
    /// emitting TS2307 there would be a false positive — and under the suite's
    /// exact-multiset rule a false positive costs a case exactly as a missing
    /// diagnostic does, while additionally being able to break a case that
    /// passes today. `docs/conventions.md`: the emitter is the consumer that
    /// acts on the negative, so its bound is part of the design and not an
    /// implementation shortcut.
    ///
    /// | declined here | upstream's code there |
    /// |---|---|
    /// | the specifier resolves to a file | TS2306 `File_0_is_not_a_module`, TS7016, the `node16` mode family — never 2307 |
    /// | a `declare module "x"` names it | resolved; no diagnostic |
    /// | a **pattern** ambient module matches (`declare module "foo/*"`) | resolved by `FindBestPatternMatch` (`checker.go:15364`); names-modules notes §6 |
    /// | a Node core module name (`fs`, `path`, …) under `types: ["*"]` | TS2580, but the harness does not load wildcard `@types`; any other core module is reported as TS2591 (tsr-2zk.933) |
    /// | `@types/…` | TS6137 is emitted *as well*, so the multiset would still differ |
    ///
    /// # Where a `None` from resolution is *not* a missing module
    ///
    /// [`Checker::resolve_external_module_name`] answers `None` both for "no
    /// file resolved" and for "a file resolved but it is not a module". Those
    /// are two upstream codes, so this rule cannot be written against that
    /// function's `Option` — it asks the host directly. That distinction is the
    /// single most load-bearing line in this file.
    fn check_module_specifier(
        &mut self,
        declaration: NodeId,
        specifier: Option<NodeId>,
        side_effect: bool,
    ) {
        let Some(specifier) = specifier else { return };
        if !self.external_import_is_positioned_for_resolution(declaration) {
            return;
        }
        // `checkExternalImportOrExportDeclaration` held: the declaration's
        // `checkExternalEmitHelpers` requests. names-modules notes §7.
        self.check_declaration_emit_helpers(declaration);
        if !self.module_specifier_unfindable_for_diagnostics(specifier) {
            // **The other branch of the load-bearing distinction.** A specifier
            // that resolved to a file which is not in the program is not
            // TS2307's; it is TS7016's when that file is JavaScript and
            // `noImplicitAny` is on. The rule's own table above has named this
            // arm since it was written; only the resolved path was missing. §357.
            self.check_untyped_module_import(declaration, specifier);
            self.check_esm_import_from_commonjs(declaration, specifier);
            return;
        }
        let Some(Node::StringLiteral(literal)) = self.node_map.get(specifier) else { return };
        let text = literal.text;
        let Some(importing) = self.source_file_of_for_diagnostics(specifier) else { return };
        let span = self.error_span(specifier);
        let message = if side_effect {
            &messages::CANNOT_FIND_MODULE_OR_TYPE_DECLARATIONS_FOR_SIDE_EFFECT_IMPORT_OF_0
        } else {
            &messages::CANNOT_FIND_MODULE_0_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
        };
        self.report_module_not_found(importing, specifier, span, text, message);
    }

    /// TS1471 / TS1479 / TS1541 / TS1542 — `resolveExternalModule`'s
    /// `Node16`/`Node18` arm (`checker.go:15325`): a synchronous import in a
    /// CommonJS-format file whose target is an ES module.
    ///
    /// Upstream chains `createModeMismatchDetails` under TS1479 for a
    /// `.ts`/`.js`/`.tsx`/`.jsx` importer; this port's `Diagnostic` carries no
    /// message chain, so only the head message is reported.
    fn check_esm_import_from_commonjs(&mut self, location: NodeId, specifier: NodeId) {
        if !matches!(self.module_kind, tsr_core::ModuleKind::Node16 | tsr_core::ModuleKind::Node18)
        {
            return;
        }
        let Some(host) = self.module_host else { return };
        let text = match self.node_map.get(specifier) {
            Some(Node::StringLiteral(literal)) => literal.text,
            Some(Node::NoSubstitutionTemplateLiteral(literal)) => literal.text,
            _ => return,
        };
        // `tryFindAmbientModule` answers before the program is asked.
        if self.ambient_module_for_diagnostics(text).is_some() {
            return;
        }
        let Some(importing) = self.source_file_of_for_diagnostics(location) else { return };
        let mode = self.module_resolution_mode(host, importing, location);
        let Some(target) = host.resolved_module_in_mode(importing, text, mode) else { return };
        // `sourceFile.Symbol != nil`.
        if self.binder.symbol_of(target).is_none() {
            return;
        }
        let ancestors = || std::iter::once(location).chain(self.nodes.ancestors(location));
        let in_import_equals =
            ancestors().any(|node| self.nodes.kind(node) == SyntaxKind::ImportEqualsDeclaration);
        let in_import_call = ancestors().any(|node| {
            matches!(self.node_map.get(node), Some(Node::CallExpression(call))
                if matches!(call.expression, Some(tsr_ast::Expression::KeywordExpression(keyword))
                    if keyword.kind == SyntaxKind::ImportKeyword))
        });
        let is_sync_import = host.default_resolution_mode_for_file(importing)
            == tsr_core::ModuleKind::CommonJS
            && !in_import_call
            || in_import_equals;
        if !is_sync_import
            || host.default_resolution_mode_for_file(target) != tsr_core::ModuleKind::ESNext
        {
            return;
        }
        // `FindAncestor(location, IsResolutionModeOverrideHost)` and
        // `HasResolutionModeOverride` on it.
        let override_host = ancestors().find(|&node| {
            matches!(
                self.nodes.kind(node),
                SyntaxKind::ImportType
                    | SyntaxKind::ExportDeclaration
                    | SyntaxKind::ImportDeclaration
                    | SyntaxKind::JSDocImportTag
            )
        });
        let override_host = override_host.and_then(|node| self.node_map.get(node));
        let attributes = match override_host {
            Some(Node::ImportTypeNode(import)) => import.attributes,
            Some(Node::ImportDeclaration(import)) => import.attributes,
            Some(Node::ExportDeclaration(export)) => export.attributes,
            Some(Node::JSDocImportTag(import)) => import.attributes,
            _ => None,
        };
        if has_resolution_mode_override(attributes) {
            return;
        }
        let message = if in_import_equals {
            &messages::MODULE_0_CANNOT_BE_IMPORTED_USING_THIS_CONSTRUCT_THE_SPECIFIER_ONLY_RESOLVES_TO_AN_ES_MODULE_WHICH_CANNOT_BE_IMPORTED_WITH_REQUIRE_USE_AN_ECMASCRIPT_IMPORT_INSTEAD
        } else {
            match override_host {
                Some(Node::ImportDeclaration(import))
                    if import.import_clause.is_some_and(|clause| {
                        clause.phase_modifier.is_some_and(|token| token.kind == SyntaxKind::TypeKeyword)
                    }) =>
                {
                    &messages::TYPE_ONLY_IMPORT_OF_AN_ECMASCRIPT_MODULE_FROM_A_COMMONJS_MODULE_MUST_HAVE_A_RESOLUTION_MODE_ATTRIBUTE
                }
                Some(Node::ImportTypeNode(_)) => {
                    &messages::TYPE_IMPORT_OF_AN_ECMASCRIPT_MODULE_FROM_A_COMMONJS_MODULE_MUST_HAVE_A_RESOLUTION_MODE_ATTRIBUTE
                }
                _ => {
                    &messages::THE_CURRENT_FILE_IS_A_COMMONJS_MODULE_WHOSE_IMPORTS_WILL_PRODUCE_REQUIRE_CALLS_HOWEVER_THE_REFERENCED_FILE_IS_AN_ECMASCRIPT_MODULE_AND_CANNOT_BE_IMPORTED_WITH_REQUIRE_CONSIDER_WRITING_A_DYNAMIC_IMPORT_0_CALL_INSTEAD
                }
            }
        };
        let Some(file) = self.source_file_of_for_diagnostics(specifier) else { return };
        let span = self.error_span(specifier);
        self.report(file, Diagnostic::with_args(message, span, [text.to_string()]));
    }

    /// The tail of `resolveExternalModule` (`checker.go:15420`-`:15441`) for a
    /// specifier no file and no ambient module answered: an ESM-mode
    /// extensionless relative import under `node16`/`nodenext` is TS2835 with
    /// `getSuggestedImportExtension`'s answer or TS2834 without one; anything
    /// else is the caller's `moduleNotFoundError`.
    fn report_module_not_found(
        &mut self,
        importing: NodeId,
        specifier: NodeId,
        span: tsr_core::Span,
        text: &str,
        module_not_found: &'static tsr_diagnostics::Message,
    ) {
        if let Some(host) = self.module_host {
            let mode = self.module_resolution_mode(host, importing, specifier);
            if mode == tsr_core::ResolutionMode::ESNext
                && let Some(extensionless) =
                    host.extensionless_relative_import(importing, text, mode)
            {
                let diagnostic = match extensionless {
                    crate::resolution::ExtensionlessImport::Suggested(extension) => Diagnostic::with_args(
                        &messages::RELATIVE_IMPORT_PATHS_NEED_EXPLICIT_FILE_EXTENSIONS_IN_ECMASCRIPT_IMPORTS_WHEN_MODULERESOLUTION_IS_NODE16_OR_NODENEXT_DID_YOU_MEAN_0,
                        span,
                        [format!("{text}{extension}")],
                    ),
                    crate::resolution::ExtensionlessImport::Unsuggested => Diagnostic::new(
                        &messages::RELATIVE_IMPORT_PATHS_NEED_EXPLICIT_FILE_EXTENSIONS_IN_ECMASCRIPT_IMPORTS_WHEN_MODULERESOLUTION_IS_NODE16_OR_NODENEXT_CONSIDER_ADDING_AN_EXTENSION_TO_THE_IMPORT_PATH,
                        span,
                    ),
                };
                self.report(importing, diagnostic);
                return;
            }
        }
        let module_not_found = if module_not_found.code()
            == messages::CANNOT_FIND_MODULE_0_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS.code()
        {
            self.cannot_resolve_module_name_error_for_specific_module(specifier)
                .unwrap_or(module_not_found)
        } else {
            module_not_found
        };
        self.report(importing, Diagnostic::with_args(module_not_found, span, [text.to_string()]));
    }

    /// `getCannotResolveModuleNameErrorForSpecificModule` (`checker.go:15110`):
    /// the message `resolveExternalModuleName` (`checker.go:15101`) reports in
    /// place of TS2307 for an unresolved Node core module. Only that route
    /// substitutes; a side-effect import (`checkImportDeclaration`,
    /// `checker.go:5325`) keeps its own message, which is why
    /// [`Checker::report_module_not_found`] asks only when handed the TS2307
    /// default.
    ///
    /// Upstream's TS2580 arm (`types` contains `"*"`) is never reached: under
    /// a wildcard [`Checker::module_specifier_unfindable_for_diagnostics`]
    /// still declines core modules (see there), so this answers TS2591.
    fn cannot_resolve_module_name_error_for_specific_module(
        &self,
        specifier: NodeId,
    ) -> Option<&'static tsr_diagnostics::Message> {
        let Some(Node::StringLiteral(literal)) = self.node_map.get(specifier) else {
            return None;
        };
        is_node_core_module(literal.text).then_some(
            &messages::CANNOT_FIND_NAME_0_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE_DEV_TYPES_SLASHNODE_AND_THEN_ADD_NODE_TO_THE_TYPES_FIELD_IN_YOUR_TSCONFIG,
        )
    }

    /// TS7016 — `Could not find a declaration file for module '{0}'. '{1}'
    /// implicitly has an 'any' type.`
    ///
    /// The resolved-but-untyped arm of `resolveExternalModule`
    /// (`checker.go:15149`), reported at the specifier literal exactly as
    /// TS2307 is. Confined to a resolved **JavaScript** file: `.tsx` without
    /// `--jsx` is TS6142 and a resolved `.ts` that is not a module is TS2306,
    /// both different codes at the same position. §357.
    fn check_untyped_module_import(&mut self, declaration: NodeId, specifier: NodeId) {
        if !self.external_import_is_positioned_for_resolution(declaration) {
            return;
        }
        let Some(Node::StringLiteral(literal)) = self.node_map.get(specifier) else { return };
        let text = literal.text;
        // A module this port could resolve to a symbol is typed; TS7016 is for
        // the resolution that produced a file and no module symbol.
        if self.resolve_external_module_name(declaration, specifier).is_some() {
            return;
        }
        let Some(importing) = self.source_file_of_for_diagnostics(specifier) else { return };
        let Some(host) = self.module_host else { return };
        let mode = self.module_resolution_mode(host, importing, declaration);
        let Some(path) = host.resolved_module_path_in_mode(importing, text, mode) else { return };
        let lowered = path.to_ascii_lowercase();
        let extension = lowered.rsplit('.').next().unwrap_or_default().to_string();
        let span = self.error_span(specifier);
        // The three members of the branch, split by the resolved extension —
        // the one thing that distinguishes them, and available only because
        // §357 added `resolved_module_path`. §359.
        // **`.jsx` asks `needJsx` before `needAllowJs`** (`module/util.go:161`),
        // and falls through to it when `--jsx` *is* set. `.tsx` has only
        // `needJsx`. §359 grouped `jsx` with the extension family it looks like
        // rather than the check sequence it belongs to, and the defect was
        // invisible without `--noImplicitAny` because the JavaScript arm returns
        // early. §527.
        let jsx_unset = self.jsx_emit == tsr_core::JsxEmit::None;
        let diagnostic = if extension == "jsx" && jsx_unset {
            Diagnostic::with_args(
                &messages::MODULE_0_WAS_RESOLVED_TO_1_BUT_JSX_IS_NOT_SET,
                span,
                [text.to_string(), path],
            )
        } else if ["js", "jsx", "cjs", "mjs"].contains(&extension.as_str()) {
            // errorOnImplicitAnyModule (checker.go:15486) ignores side-effect
            // imports: they do not consume a value needing a declaration.
            if matches!(self.node_map.get(declaration), Some(Node::ImportDeclaration(import)) if import.import_clause.is_none())
            {
                return;
            }
            if !self.no_implicit_any {
                return;
            }
            Diagnostic::with_args(
                &messages::COULD_NOT_FIND_A_DECLARATION_FILE_FOR_MODULE_0_1_IMPLICITLY_HAS_AN_ANY_TYPE,
                span,
                [text.to_string(), path],
            )
        } else if extension == "tsx" && jsx_unset {
            Diagnostic::with_args(
                &messages::MODULE_0_WAS_RESOLVED_TO_1_BUT_JSX_IS_NOT_SET,
                span,
                [text.to_string(), path],
            )
        } else {
            // A file that resolved, is in the program, and exports nothing —
            // `requireOfAnEmptyFile1`. Upstream's argument is the resolved file
            // name, not the specifier.
            Diagnostic::with_args(&messages::FILE_0_IS_NOT_A_MODULE, span, [path])
        };
        self.report(importing, diagnostic);
    }

    /// The boolean core of the TS2307 emitter, shared with
    /// `get_type_of_alias` (`checker-notes-callres.md` §31) so the
    /// diagnostic and the alias's `any` read ONE calibrated answer: TRUE
    /// only when every decline-gate passes and the host, consulted, found
    /// nothing. Position is the caller's test.
    pub(crate) fn module_specifier_unfindable(&mut self, specifier: NodeId) -> bool {
        self.module_specifier_unfindable_worker(specifier, false)
    }

    /// [`Checker::module_specifier_unfindable`] for the three TS2307
    /// reporters, which also admit an unresolved Node core module: upstream
    /// reports it with TS2591 instead of TS2307
    /// (`getCannotResolveModuleNameErrorForSpecificModule`,
    /// `checker.go:15110`), substituted in
    /// [`Checker::report_module_not_found`]. tsr-2zk.933.
    ///
    /// Two declines remain, each for a measured reason:
    ///
    /// - The type callers (`get_type_of_alias`, the type-reference arm in
    ///   `declared.rs`) keep reading the core-module decline. Admitting it
    ///   there makes `const fs = require("fs")` in a JS file answer their
    ///   calibrated `any` where native prints `error`
    ///   (`compiler/localRequireFunction`, 4 type lines).
    /// - Under `types: ["*"]` upstream loads every `@types` package, and the
    ///   conformance harness does not, so `declare module "url"` in
    ///   `@types/node` goes unseen and TS2580 would be a false report
    ///   (`compiler/referenceTypesPreferedToPathIfPossible`). The CLI loads
    ///   them (`tsr-compiler` `add_automatic_type_directive_task`) and
    ///   agrees with native there.
    fn module_specifier_unfindable_for_diagnostics(&mut self, specifier: NodeId) -> bool {
        let admit_node_core = !self.uses_wildcard_types;
        self.module_specifier_unfindable_worker(specifier, admit_node_core)
    }

    fn module_specifier_unfindable_worker(
        &mut self,
        specifier: NodeId,
        admit_node_core: bool,
    ) -> bool {
        let Some(Node::StringLiteral(literal)) = self.node_map.get(specifier) else {
            return false;
        };
        let text = literal.text;
        // `tryFindAmbientModule` (`checker.go:15154`), consulted before the host
        // exactly as `resolveExternalModule` does.
        if self.ambient_module_for_diagnostics(text).is_some() {
            return false;
        }
        // `resolveExternalModule`'s pattern arm (`checker.go:15364`): a
        // matching `declare module "prefix*suffix"` resolves the specifier.
        // Upstream asks it only after the program's resolution misses; a
        // resolution hit returns `false` below either way.
        if self.has_pattern_ambient_module() && self.binder.pattern_ambient_module(text).is_some() {
            return false;
        }
        // A Node core module (`fs`, `node:path`, …) is resolved like any other
        // specifier upstream; only the *message* differs. Admitted for the
        // reporters only — see `module_specifier_unfindable_for_diagnostics`.
        if !admit_node_core && is_node_core_module(text) {
            return false;
        }
        if text.starts_with("@types/") {
            return false;
        }
        let Some(importing) = self.source_file_of_for_diagnostics(specifier) else { return false };
        // No host means no resolution was ever attempted, and "we did not look"
        // must not read as "it is not there".
        let Some(host) = self.module_host else { return false };
        // `resolvedModule.IsResolved()` (`checker.go:15208`). A resolution that
        // named a file the program does not hold is upstream's TS7016 / TS6142 /
        // TS2306 territory — a *different code at the same position* — so it
        // must not read as "cannot find". Distinguishing that from "found
        // nothing" is why [`crate::resolution::ModuleHost`] grew a second
        // method; it was 15 of the 60 wrong lines the first counterfactual
        // measured.
        let mode = self.module_resolution_mode(host, importing, specifier);
        !host.module_resolution_found_in_mode(importing, text, mode)
    }

    /// TS2564 — `Property '{0}' has no initializer and is not definitely
    /// assigned in the constructor.`
    ///
    /// `checkPropertyInitialization` (`checker.go:4933`), called from
    /// `checkClassLikeDeclaration`'s last line (`checker.go:4390`). The error
    /// node is the **member's name**, so the reported column is the property
    /// name rather than the declaration or its type.
    ///
    /// # The bound: no constructor with a body, or nothing is said
    ///
    /// Upstream's condition is `constructor == nil ||
    /// !isPropertyInitializedInConstructor(...)` (`checker.go:4947`). The second
    /// disjunct synthesises a `this.x` property access, hangs it off the
    /// constructor's `ReturnFlowNode` and asks `getFlowTypeOfReference` whether
    /// `undefined` survives (`checker.go:4960` shows the sibling doing it for
    /// static blocks). **This port cannot synthesise that node**: the tree is
    /// arena-allocated and immutable after parsing
    /// ([ADR-0012](../../../docs/adr/0012-ast-is-sync.md)), and a flow query
    /// needs a *registered* node with a parent and a flow node.
    ///
    /// So only the first disjunct is ported, and a class that **has** a
    /// constructor with a body is declined outright. That is silence, never a
    /// wrong answer: upstream reports there only when the constructor fails to
    /// assign, and this port cannot tell those apart. The cost is measured
    /// rather than assumed — see `docs/architecture/checker-notes-diag2.md` §6.
    ///
    /// # `strictPropertyInitialization` is `strictNullChecks` here
    ///
    /// Upstream reads two separate `GetStrictOptionValue` results
    /// (`checker.go:919`, `:922`). This port models one strictness flag
    /// ([`Checker::strict_null_checks`], set from the case's directives), and
    /// both options default to `strict`, so the single flag is the faithful
    /// reading for every case that does not set them apart. A case writing
    /// `strictPropertyInitialization: false` under `strict: true` would be
    /// over-reported; the harness reads the directive and turns the rule off,
    /// which is where that divergence is repaired.
    fn check_property_initialization(&mut self, members: &[ClassElement<'_>], ambient: bool) {
        if !self.strict_null_checks || !self.strict_property_initialization || ambient {
            return;
        }
        // `ast.FindConstructorDeclaration` (`ast/utilities.go:2498`) — a
        // constructor **with a body**; an overload signature does not count.
        let constructor_body = members.iter().find_map(|member| match member {
            ClassElement::ConstructorDeclaration(ctor) => ctor.body.and_then(|body| body.node_id()),
            _ => None,
        });
        for member in members {
            let ClassElement::PropertyDeclaration(property) = member else { continue };
            if has_modifier(property.modifiers, SyntaxKind::DeclareKeyword)
                || has_modifier(property.modifiers, SyntaxKind::StaticKeyword)
                || has_modifier(property.modifiers, SyntaxKind::AbstractKeyword)
            {
                continue;
            }
            // `isPropertyWithoutInitializer` (`checker.go:4956`): no `!`
            // postfix, no initialiser. A `?` postfix is *not* excluded here —
            // upstream lets it through and the `containsUndefinedType` test
            // below is what stops it, because an optional property's type
            // carries `undefined` under `strictNullChecks`.
            if property.initializer.is_some()
                || property
                    .postfix_token
                    .is_some_and(|token| token.kind == SyntaxKind::ExclamationToken)
            {
                continue;
            }
            // `IsIdentifier || IsPrivateIdentifier || IsComputedPropertyName`
            // (`checker.go:4944`). A string- or number-named property is
            // skipped by upstream too.
            // The three accepted kinds, not one: a string- or number-named
            // property is what the `None` arm is for. §43 records the two that
            // were being swept up with it.
            let Some(name) = declaration_name_to_string(property.name) else { continue };
            let Some(id) = property.node_id else { continue };
            let Some(symbol) = self.binder.symbol_of(id) else { continue };
            let declared = self.get_type_of_symbol(symbol);
            // `t.flags&TypeFlagsAnyOrUnknown` (`checker.go:4946`) — and
            // `errorType` carries `TypeFlagsAny` upstream, so the error arm is
            // that disjunct rather than an extra one. `Checker::is_error` and
            // not `== intrinsics.error`: an unresolved type REFERENCE mints a
            // `Named` carrying the written text and answers `is_error`
            // (`Checker::unresolved_types`), and `class C { [e]: Type }` with
            // neither name declared is exactly that shape — §43's first wrong
            // line.
            // An error-typed property is skipped as upstream skips
            // `errorType` — but only when the annotation is itself the
            // reference that failed. This port also answers `error` for a
            // *gap* propagated out of a nested node (`X3[]`, `I<X4>` with `X3`
            // and `X4` missing their type arguments), where upstream's type is
            // `Array<errorType>` / `I<errorType>` and TS2564 is reported
            // (`missingTypeArguments1`). `docs/parity/notes/decls.md` §17,
            // superseding §6's decline and §324's computed-name bound.
            if (self.is_error(declared)
                && self.annotation_is_failed_reference(property.r#type.and_then(|t| t.node_id())))
                || declared == self.intrinsics.any
                || declared == self.intrinsics.unknown
                || self.contains_undefined_type(declared)
            {
                continue;
            }
            // `!isPropertyInitializedInConstructor(...)` (`checker.go:4947`),
            // as much of it as is decidable without synthesising a node. See
            // `checker-notes-diag2.md` §87: a constructor body that never
            // mentions `this.<name>` cannot assign it on any path, so the flow
            // query upstream runs would answer "declared type survives" and
            // report. Any other constructor declines.
            if let Some(body) = constructor_body {
                if self.subtree_accesses_this_member(body, &name, 0) {
                    continue;
                }
            }
            let Some(name_id) = property.name.node_id() else { continue };
            let Some(file) = self.source_file_of_for_diagnostics(name_id) else { continue };
            let span = self.error_span(name_id);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::PROPERTY_0_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
                    span,
                    [name],
                ),
            );
        }
    }

    /// `containsUndefinedType` (`checker.go`): the type *is* `undefined`, or is
    /// a union with `undefined` among its constituents.
    ///
    /// **A gap counts as containing it.** This port answers `errorType` where it
    /// cannot compute, and a property whose type it cannot compute must not
    /// produce a diagnostic that depends on what that type is — which is why the
    /// caller tests `error` beside this.
    fn contains_undefined_type(&self, ty: crate::types::TypeId) -> bool {
        if ty == self.intrinsics.undefined {
            return true;
        }
        match &self.store.get(ty).data {
            crate::types::TypeData::Union { types, .. } => {
                types.contains(&self.intrinsics.undefined)
            }
            _ => false,
        }
    }

    /// TS2304 — `Cannot find name '{0}'.`
    ///
    /// `getResolvedSymbol` (`checker.go:13890`) resolves every identifier
    /// expression with `SymbolFlagsValue|SymbolFlagsExportValue` and a
    /// *nameNotFoundMessage*; failing to resolve lands in
    /// `onFailedToResolveSymbol` (`checker.go:1564`), whose **last line** is this
    /// diagnostic.
    ///
    /// # Everything above that last line is a decline, and there are ten
    ///
    /// `onFailedToResolveSymbol` runs seven `checkAndReportErrorFor…`
    /// predicates, a missing-lib suggestion and a spelling suggestion before it
    /// falls through, and **each of them reports a different code at the same
    /// position** — TS2662/2663 for a missing `this.`/`super.` prefix, TS2689
    /// for extending an interface, TS2702 for a type used as a namespace,
    /// TS2708/2709 for a namespace used as a value or type, TS2693/2749 for a
    /// type used as a value and the reverse, TS2583 for a name that needs a
    /// different `lib`, TS2552 for a spelling suggestion. On top of that
    /// `getCannotFindNameDiagnosticForName` (`checker.go:13915`) substitutes the
    /// whole message for fourteen well-known names before resolution even
    /// starts.
    ///
    /// This is the TS2307 shape again and worse: the diagnostic is the *residue*
    /// of a decision tree, so porting it means porting the tree. What is ported
    /// here is the subset whose declines are cheap and total:
    ///
    /// | declined | upstream's code |
    /// |---|---|
    /// | the fourteen names of `getCannotFindNameDiagnosticForName` | TS2580–TS2593 |
    /// | the name resolves under `TYPE` or `NAMESPACE` meaning | TS2693 / TS2709 / TS2749 / TS2702 |
    /// | any other symbol in the file is spelled within one edit | TS2552, approximated — see [`Checker::has_spelling_suggestion`] |
    /// | the identifier is not in an allow-listed value slot | not an identifier expression at all |
    ///
    /// The position test is an **allow**-list ([`Checker::is_value_reference`])
    /// rather than a deny-list, because the generic walk hands this rule every
    /// identifier in the file — declaration names, member names, labels, type
    /// references, import specifiers — and a missing deny-list arm is a false
    /// positive while a missing allow-list arm is only a missed conversion.
    fn check_value_identifier(&mut self, node: NodeId, text: &str) {
        // **This is a position test, not a parse-error test**, and the
        // distinction cost a measurement to establish (§254).
        //
        // The family this guard was written for — `jsxUnclosedParserRecovery`
        // 21 lines, `arrowFunctionsMissingTokens` 15,
        // `parserUnterminatedGeneric2` 8, and a tail of `parserSkippedTokens`
        // and conflict-marker cases — is handled by `is_value_reference`
        // declining the *positions* a recovered tree invents, **not** by
        // consulting [`crate::checker::Checker::file_has_parse_errors`].
        //
        // Adding a blanket `if self.file_has_parse_errors { return }` here was
        // measured at **−14 cases**: upstream does check identifiers in a file
        // that failed to parse, and reports TS2304 in plenty of them. The
        // residue that remains — `validRegexp`'s `i` in
        // `var x = / [a - z /]$ / i;` — is a *recovery* difference, and the
        // owner is the parser, not this guard.
        // **`arguments` is synthesised for function-like containers, and an
        // arrow function is not one.** §249 declined the name everywhere
        // because this port does not synthesise the symbol; that is right about
        // *resolution* and incomplete about *position*. Where no non-arrow
        // function encloses the reference, upstream resolves nothing and
        // reports TS2304 — `(() => arguments)()` at top level, `typeof
        // arguments` in an interface, `++arguments` in a script. §951.
        if text == "arguments"
            && self.is_value_reference(node)
            && !self.in_js_file(node)
            && !self.file_has_parse_errors
            && !self.reference_has_non_arrow_function_container(node)
            // Outside a function `resolveName` is the ordinary walk, so a
            // declared `var arguments` resolves (`emitArrowFunctionWhenUsingArguments03`).
            && self
                .resolve_name_with_export_alias(
                    node,
                    text,
                    SymbolFlags::VALUE | SymbolFlags::EXPORT_VALUE,
                )
                .is_none()
        {
            if self.check_and_report_error_for_missing_prefix(node, text) {
                return;
            }
            if let Some(file) = self.source_file_of_for_diagnostics(node) {
                let span = self.error_span(node);
                self.report(
                    file,
                    Diagnostic::with_args(&messages::CANNOT_FIND_NAME_0, span, [text.to_string()]),
                );
            }
            return;
        }
        if !self.is_value_reference(node) || is_specially_diagnosed_name(text) {
            return;
        }
        // **In a JavaScript file the CommonJS names are not unresolved.**
        // `require`, `module` and friends are globals upstream supplies through
        // machinery this port does not have, so §249's table would report on
        // every one of them: `modulePreserve4`, `maxNodeModuleJsDepthDefaults…`
        // and `jsdocReferenceGlobalTypeInCommonJs` were 27 wrong lines, all
        // `.js`/`.cjs`. Declining there is what this file did for *all* files
        // before §249, kept exactly where it was right.
        if self.in_js_file(node) && cannot_find_name_message(text).is_some() {
            return;
        }
        // **A decorator is TypeScript-only syntax in a JavaScript file**, and
        // upstream neither resolves its name nor reports on it. The decline
        // above is narrower than this one — it fires only for names carrying a
        // specific *cannot find name* message — so the general case walked
        // past it. §779 and §788 added the same guard to two other rules; this
        // is the first where one existed and was written for another purpose.
        // §790.
        if self.in_js_file(node)
            && self.nodes.ancestors(node).any(|a| self.nodes.kind(a) == SyntaxKind::Decorator)
        {
            return;
        }
        // Inside a `with` block upstream reports TS2410 — *"All symbols in a
        // 'with' block will have type 'any'"* — and resolves nothing
        // (`NodeFlagsInWithStatement`, read at `checker.go:29344` and four other
        // sites). The flag is another the parser here never sets, so the
        // question is asked of the ancestors.
        if self.is_inside_with_statement(node) {
            return;
        }
        // **A duplicate heritage clause is recovered syntax**, and upstream
        // resolves only the first of each token kind. §792 built this predicate
        // and wired it into `check_type_identifier`; an `extends` name on a
        // *class* is a **value** reference (§334), so it arrives here instead
        // and the guard was never added. `class C extends A extends B` reported
        // on `B`, which upstream does not. §832.
        if self.in_duplicate_heritage_clause(node) {
            return;
        }
        // **And the extra *types* in an `extends` clause.** A class extends
        // one class, so `typeNodes[1]` and beyond are what the parser kept in
        // order to have a position for TS1174 (`grammarchecks.go:911`) — the
        // same status §833 gave a second clause, one level down. §874.
        if self.is_extra_type_in_a_class_extends_clause(node) {
            return;
        }
        // `!ast.NodeIsMissing(node)` (`checker.go:13894`) — upstream does not
        // resolve, and therefore never reports, an identifier the parser
        // synthesised while recovering. `NodeIsMissing` is `pos == end`, and a
        // missing identifier also carries empty text.
        let span = self.error_span(node);
        if text.is_empty() || span.start == span.end {
            return;
        }
        // `class C extends null {}` — upstream's parser makes `null` a
        // `NullKeyword` expression and this one makes it an `Identifier`, so the
        // name reaches a resolver that can never find it. A parser divergence
        // worked around at the reader rather than in the parser, because `null`
        // is not a spellable binding in any scope: declining it can hide no real
        // diagnostic. `classExtendsNull`, `classExtendsNull2` and
        // `classExtendsNull3` were 5 wrong lines.
        // …and `typeof this.z` is the same shape at the other keyword. §79 put
        // the leftmost name of a type query's entity name into the allow-list,
        // and `this` is spelled as an `Identifier` by this parser there, so it
        // reached a resolver that can never find it — `initializerReferencing-
        // ConstructorLocals` and `…Parameters`, 4 wrong lines, where upstream
        // reports TS2339 on the `.z` instead. Like `null`, `this` is not a
        // spellable binding in any scope, so declining it can hide no real
        // diagnostic.
        if text == "null" || text == "this" {
            return;
        }
        // `checkAndReportErrorForUsingTypeAsValue` (`checker.go:1681`) runs
        // **before** the suggestion arm, and a primitive type *keyword* used in
        // a value position is TS2693 — `class C extends string` is
        // `conformance/classExtendingPrimitive`, 9 wrong lines, plus
        // `primitiveTypeAssignment`. These names resolve to no symbol here
        // because they are keywords rather than globals, so the existing
        // "resolves as a TYPE" decline never sees them.
        // …**unless they resolve.** Upstream reaches the cascade only when
        // `getResolvedSymbol` fails, and `declare function string()` is a
        // value named `string`
        // (`classReferencedInContextualParameterWithinItsOwnBaseExpression`).
        // r4-helpers notes §7.
        if matches!(
            text,
            "string"
                | "number"
                | "boolean"
                | "symbol"
                | "object"
                | "bigint"
                | "any"
                | "never"
                | "unknown"
                | "void"
        ) && self
            .resolve_name_with_export_alias(
                node,
                text,
                SymbolFlags::VALUE | SymbolFlags::EXPORT_VALUE,
            )
            .is_none()
        {
            // **A heritage position has its own three messages**, and they are
            // what makes those nine lines right rather than wrong: upstream's
            // `checkAndReportErrorForUsingTypeAsValue` reports TS2863 / TS2864 /
            // TS2840 for a primitive in an `extends` or `implements` clause and
            // TS2693 elsewhere. The decline above was measured against a rule
            // that reported TS2693 *everywhere*; the heritage arm is the half
            // this port has. §880.
            // **Upstream's `isPrimitiveTypeName` is six names**, and this
            // list is ten: `void`, `object`, `symbol` and `bigint` are not on
            // it. `class C4a extends void {}` is a *parse* error — TS1109,
            // `Expression expected` — and reporting TS2863 beside it cost
            // `classExtendingPrimitive2`, measured. §880.
            let upstream_six =
                matches!(text, "any" | "string" | "number" | "boolean" | "never" | "unknown");
            let in_heritage = self
                .nodes
                .ancestors(node)
                .any(|a| self.nodes.kind(a) == SyntaxKind::HeritageClause);
            if upstream_six && in_heritage {
                self.report_primitive_type_as_value_at(node, text);
                return;
            }
            // **Outside a heritage clause the six names fall through to the
            // cascade**, whose `isPrimitiveTypeName` arm is TS2693 and was
            // unreachable for as long as this decline swallowed them. §880
            // measured a rule that reported TS2693 *everywhere* — including for
            // `void`, `object`, `symbol` and `bigint`, which upstream's
            // `isPrimitiveTypeName` does not list and which stay declined here.
            // §948.
            // **No parse-error gate.** §949 added one to hide 48 extra lines in
            // files the parser recovered; upstream reports TS2693 in such
            // files (`autoLift2`, `createArray`, `parserUnterminatedGeneric2`
            // are parse-error fixtures whose baselines carry it). Re-measured
            // without it: +7 cases, 0 lost. `docs/parity/notes/r4-helpers.md` §2.
            if !upstream_six {
                return;
            }
        }
        // `OnPropertyWithInvalidInitializer` (`nameresolver.go`, reached from
        // `resolveNameHelper`): an instance property's initialiser that names a
        // **constructor parameter** is TS2301, not TS2304 — upstream's resolver
        // finds the parameter, notices the position, and substitutes. This
        // port's `resolve_name` does not put constructor parameters in a
        // property initialiser's scope at all, so the same substitution is made
        // by asking the class directly.
        if let Some(property) =
            self.property_initializer_referencing_a_constructor_parameter(node, text)
        {
            // `checkAndReportErrorForInvalidInitializer` (`checker.go:1514`)
            // tries the missing prefix first when the name resolved nowhere
            // else (`result == nil`).
            if self
                .resolve_name_with_export_alias(
                    node,
                    text,
                    SymbolFlags::VALUE | SymbolFlags::EXPORT_VALUE,
                )
                .is_none()
                && self.check_and_report_error_for_missing_prefix(node, text)
            {
                return;
            }
            let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::INITIALIZER_OF_INSTANCE_MEMBER_VARIABLE_0_CANNOT_REFERENCE_IDENTIFIER_1_DECLARED_IN_THE_CONSTRUCTOR,
                    span,
                    [property, text.to_string()],
                ),
            );
            return;
        }
        if self
            // `getResolvedSymbol` (`checker.go:13890`) resolves an identifier at
            // `SymbolFlagsValue | SymbolFlagsExportValue`. **`EXPORT_VALUE` is
            // part of the meaning, not a separate question.** An exported
            // declaration leaves a local carrying `EXPORT_VALUE` and nothing
            // else (`declareModuleMember`, `binder.go:403`, which this port
            // matches line for line), so without it that local never matches
            // and the name resolves nowhere.
            //
            // `export class A` hides this — its export entry is *also* named
            // `A`, so a later arm recovers it. `export default class A` does
            // not: the export is named `default`, and the written name exists
            // **only** as the flagless local. §251.
            .resolve_name_with_export_alias(
                node,
                text,
                SymbolFlags::VALUE | SymbolFlags::EXPORT_VALUE,
            )
            .is_some_and(|value| {
                // …unless it resolved to an alias that is type-only somewhere
                // along its chain: `resolveNameEx` (`checker.go:1860`) tests
                // `Alias && !Value`, and this port's alias symbols answer
                // `VALUE` where upstream's do not (§119), so the test belongs
                // here rather than on the meaning ladder below. §121.
                self.report_type_only_alias_used_as_value(node, value, text);
                // `getSymbol` again, on the **value** lookup: `import a = A`
                // where `A` is an uninstantiated namespace carries `ALIAS` here
                // and satisfies nothing upstream, so the cascade's value branch
                // gets to say TS2708. §689 probed this return and found the
                // cascade reached zero times.
                //
                // **The test gates the early return, not the report above it.**
                // Written as a `filter` ahead of that call it also suppressed
                // TS1361 for `import type { Base }; class C extends Base` —
                // `a_class_extends_over_a_type_only_import_still_reports` went
                // red, and it is in the suite precisely because this line has
                // two jobs. §693.
                self.alias_chain_carries(value, SymbolFlags::VALUE | SymbolFlags::EXPORT_VALUE)
            })
        {
            return;
        }
        // A **named function expression** binds its own name inside its body
        // (`bindFunctionExpression`), and this binder does not — so a recursive
        // `(function f() { … f() … })` reaches here and fails.
        // `recursiveNamedLambdaCall`, and declined rather than repaired because
        // `tsr_binder` is shared with the query road
        // (`checker-notes-diag2.md` §60).
        // …and a **class** binds its own name inside its body the same way —
        // a class expression through `bindAnonymousDeclaration`, and
        // `export default class X` through the default-export symbol. Neither
        // is where `resolve_name` looks (`checker-notes-diag2.md` §60, §71).
        if self.nodes.ancestors(node).any(|ancestor| match self.node_map.get(ancestor) {
            Some(Node::FunctionExpression(function)) => {
                function.name.is_some_and(|name| name.text == text)
            }
            Some(Node::ClassDeclaration(class)) => class.name.is_some_and(|name| name.text == text),
            Some(Node::ClassExpression(class)) => class.name.is_some_and(|name| name.text == text),
            _ => false,
        }) {
            return;
        }
        // `checkAndReportErrorForUsingTypeAsValue` / `…NamespaceAsTypeOrValue`
        // (`checker.go:1681`, `:1643`): a name that resolves under another
        // meaning gets a *different* code, so silence is the only sound answer
        // until those arms are ported.
        //
        // **`ALIAS` is on the ladder because `resolveEntityName` puts it
        // there.** Upstream resolves an entity name at
        // `meaning | SymbolFlagsAlias` (`checker.go:15772`), so `import Z = M;
        // var r8: typeof Z` finds the alias and reports nothing. This binder
        // gives an import-equals its own `ALIAS` symbol and none of the other
        // three meanings, which made `typeofAnExportedType` §79's only new
        // wrong line.
        // §169: re-measured against the post-§166 resolver.
        //
        // §169: the value-position half of `onFailedToResolveSymbol`'s cascade,
        // re-measured against the post-§166 resolver.
        //
        // **This call appeared twice**, with these two comment blocks split
        // across the pair. The second was unreachable — the first returns on
        // every path that reports — so it cost nothing and read as though two
        // different cascades were being run. §248.
        // `onFailedToResolveSymbol` (`checker.go:1564`) asks for the missing
        // `this.`/class prefix before any other arm.
        if self.check_and_report_error_for_missing_prefix(node, text) {
            return;
        }
        if self.check_and_report_error_for_extending_interface(node) {
            return;
        }
        if self.report_meaning_mismatch_in_value_position(node, text) {
            return;
        }
        if [SymbolFlags::TYPE, SymbolFlags::NAMESPACE, SymbolFlags::ALIAS].into_iter().any(
            |meaning| {
                self.binder.resolve_name(self.nodes, self.node_map, node, text, meaning).is_some()
            },
        ) {
            return;
        }
        // `onFailedToResolveSymbol` reports the **missing lib first**
        // (`checker.go:1584`): a name in `getFeatureMap` is TS2583, not TS2304,
        // and the two are a wrong code at a right position apart.
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        if let Some(lib) = suggested_lib_for(text) {
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::CANNOT_FIND_NAME_0_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CHANGING_THE_LIB_COMPILER_OPTION_TO_1_OR_LATER,
                    span,
                    [text.to_string(), lib.to_string()],
                ),
            );
            return;
        }
        // Then spelling suggestions (`checker.go:1590`) — TS2552, **emitted**
        // rather than declined. The eighth session ported
        // `getSuggestedSymbolForNonexistentSymbol`'s weighted distance in full
        // precisely because a near neighbour makes TS2304 a wrong code at a
        // right position; with the algorithm already exact, reporting the code
        // it selects costs one message and converts its own row.
        if let Some(suggestion) =
            self.spelling_suggestion_for(node, text, SymbolFlags::VALUE | SymbolFlags::EXPORT_VALUE)
        {
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::CANNOT_FIND_NAME_0_DID_YOU_MEAN_1,
                    span,
                    [text.to_string(), suggestion],
                ),
            );
            return;
        }
        // `getCannotFindNameDiagnosticForName`'s remaining rows, which are
        // upstream's `nameNotFoundMessage` and therefore the **fallback** —
        // reported only once the lib and suggestion arms above have declined.
        // §247, reachable since §249. The table's `default` arm
        // (`getCannotFindNameDiagnosticForName`, `checker.go:13944`) names a
        // shorthand property's identifier TS18004 instead of TS2304.
        let shorthand = self.nodes.parent(node).is_some_and(|parent| {
            self.nodes.kind(parent) == SyntaxKind::ShorthandPropertyAssignment
        });
        let message = cannot_find_name_message(text).unwrap_or(if shorthand {
            &messages::NO_VALUE_EXISTS_IN_SCOPE_FOR_THE_SHORTHAND_PROPERTY_0_EITHER_DECLARE_ONE_OR_PROVIDE_AN_INITIALIZER
        } else {
            &messages::CANNOT_FIND_NAME_0
        });
        self.report(file, Diagnostic::with_args(message, span, [text.to_string()]));
    }

    /// TS2304 / TS2552 / TS2583 for a name in a **type** position.
    ///
    /// `getTypeFromTypeReference` → `resolveTypeReferenceName` →
    /// `resolveEntityName`, whose failure arm is the same
    /// `onFailedToResolveSymbol` (`checker.go:1584`)
    /// [`Checker::check_value_identifier`] already ports — missing lib first,
    /// then a spelling suggestion, then `Cannot find name`.
    ///
    /// The two arms are **disjoint by construction**: this one fires only on
    /// the `type_name` slot of a `TypeReferenceNode`, and
    /// [`Checker::is_value_reference`] never looks at that slot.
    ///
    /// Bounded to a bare identifier that resolves under **no** meaning:
    /// a qualified `A.B` fails as TS2694, a name that resolves as a value is
    /// TS2749, and as a namespace TS2709 — three wrong codes at a right
    /// position, which is the failure §7 and §33 each spent a build removing.
    /// `docs/architecture/checker-notes-diag2.md` §55.
    /// TS2302 — `Static members cannot reference class type parameters.`
    ///
    /// `resolveNameEx`'s type-parameter arm (`binder/nameresolver.go:178`) and
    /// TypeScript 1.0 spec 3.4.1: a type parameter's scope covers the whole
    /// declaration **except static members**. `resolve_name` already ports the
    /// decision — it returns `None` rather than the symbol when `lastLocation`
    /// is static (`lib.rs`, the `is_static_member` arm) — and left the
    /// diagnostic unported because the binder had none at the time. It has had
    /// them since §113, but `resolve_name` takes `&self`, so the *report* stays
    /// with the checker while the *decision* stays with the resolver.
    ///
    /// `lastLocation` is the child the walk came up from — §108's edge-not-node
    /// idea, here in its original home.
    fn static_member_references_class_type_parameter(&self, node: NodeId, text: &str) -> bool {
        let mut came_from = node;
        for ancestor in self.nodes.ancestors(node) {
            let Some(typed) = self.node_map.get(ancestor) else { return false };
            if matches!(typed, Node::ClassDeclaration(_) | Node::ClassExpression(_)) {
                return class_element_is_static(self.node_map.get(came_from))
                    && type_parameters_of(typed)
                        .iter()
                        .any(|parameter| parameter.name.is_some_and(|name| name.text == text));
            }
            came_from = ancestor;
        }
        false
    }

    /// TS1361 / TS1362 — an alias declared `import type` / `export type` used
    /// as a value (`checker.go:1860`; the message splits at `:1863`).
    fn report_type_only_alias_used_as_value(
        &mut self,
        node: NodeId,
        symbol: tsr_binder::SymbolId,
        text: &str,
    ) {
        if self.is_valid_type_only_alias_use_site(node) {
            return;
        }
        let Some(exported) = self.type_only_alias_declaration(symbol) else { return };
        let message = if exported {
            &messages::_0_CANNOT_BE_USED_AS_A_VALUE_BECAUSE_IT_WAS_EXPORTED_USING_EXPORT_TYPE
        } else {
            &messages::_0_CANNOT_BE_USED_AS_A_VALUE_BECAUSE_IT_WAS_IMPORTED_USING_IMPORT_TYPE
        };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(file, Diagnostic::with_args(message, span, [text.to_string()]));
    }

    /// `IsValidTypeOnlyAliasUseSite` (`ast/utilities.go:3124`) — §121's debt,
    /// and §122 attributed it to the wrong clauses.
    ///
    /// A type-only alias is legal wherever the name is **not emitted**. The
    /// clause the residual actually wanted is
    /// `isPartOfPossiblyValidTypeOrAbstractComputedPropertyName` (`:3143`): a
    /// **computed property name** on an `abstract` member, or on a member of an
    /// interface or type literal, is erased. `conformance/computedPropertyName`
    /// is three of the eight lines and `mergeSymbolRexportFunction` the fourth.
    ///
    /// `IsPartOfTypeQuery` is ported alongside it — `typeof X` names an alias
    /// without emitting it, and this port routes type queries through
    /// `check_value_identifier` because §79 made them a value position for
    /// TS2304's purposes. §123.
    fn is_valid_type_only_alias_use_site(&self, node: NodeId) -> bool {
        if self.entity_name_root_is_a_type_query(node) {
            return true;
        }
        // `isIdentifierInNonEmittingHeritageClause` (`:3127`), the clause §123
        // did not port. `interface P extends VariantProps<T>` over an
        // `import type { VariantProps }` is the shape every `cva`-style React
        // component is written in, and it reported TS1361 on all of them.
        if self.identifier_in_non_emitting_heritage_clause(node) {
            return true;
        }
        // `export = types` and `export default types` over a type-only import
        // are **not** reported at the re-exporting file. Upstream's
        // `importEquals1` baseline puts TS1361 on `/d.ts`–`/g.ts`, the
        // *consumers*, and writes nothing at `/b.ts` where the re-export lives
        // — the fixture's own `// Error` comment refers to those, and §124 read
        // the comment instead of the baseline and drew the opposite conclusion.
        // §125.
        if self
            .nodes
            .parent(node)
            .is_some_and(|parent| self.nodes.kind(parent) == SyntaxKind::ExportAssignment)
        {
            return true;
        }
        // Walk out through the entity name, exactly as upstream's loop does.
        let mut at = node;
        while matches!(
            self.nodes.kind(at),
            SyntaxKind::Identifier | SyntaxKind::PropertyAccessExpression
        ) {
            let Some(parent) = self.nodes.parent(at) else { return false };
            at = parent;
        }
        if self.nodes.kind(at) != SyntaxKind::ComputedPropertyName {
            return false;
        }
        let Some(member) = self.nodes.parent(at) else { return false };
        if self.member_is_abstract(member) {
            return true;
        }
        // `useSite.Flags&NodeFlagsAmbient != 0` — the FIRST clause of
        // `IsValidTypeOnlyAliasUseSite` (`ast/utilities.go:3125`), which §123
        // skipped because `NodeFlags::AMBIENT` is one of this port's never-set
        // flags. §99 already built the substitute: walk the `declare`
        // modifiers. `declare class H { [onInit]: any }` and
        // `class G { declare [onInit]: any }` are both this. §132.
        if self.declaration_is_in_an_ambient_context(member)
            || self.member_has_declare_modifier(member)
        {
            return true;
        }
        self.nodes.parent(member).is_some_and(|owner| {
            matches!(
                self.nodes.kind(owner),
                SyntaxKind::InterfaceDeclaration | SyntaxKind::TypeLiteral
            )
        })
    }

    /// `isIdentifierInNonEmittingHeritageClause` (`ast/utilities.go:3132`).
    ///
    /// ```go
    /// parent := node.Parent
    /// for IsPropertyAccessExpression(parent) || IsExpressionWithTypeArguments(parent) {
    ///     parent = parent.Parent
    /// }
    /// return IsHeritageClause(parent) &&
    ///     (parent.AsHeritageClause().Token == KindImplementsKeyword || IsInterfaceDeclaration(parent.Parent))
    /// ```
    ///
    /// Two heritage positions name a type and emit nothing: a class's
    /// `implements`, and **either clause of an `interface`**. Only a *class's*
    /// `extends` is a value — it is the base constructor, and it survives to
    /// the output.
    ///
    /// The `extends` keyword alone does not separate them, which is the bug
    /// this fixes: `class C extends B` and `interface I extends B` share both
    /// the keyword and the node kind, and the discriminator is the heritage
    /// clause's **parent**.
    ///
    /// # The loop shape is load-bearing
    ///
    /// It climbs `PropertyAccessExpression` and `ExpressionWithTypeArguments`
    /// and nothing else, which is what keeps the *type arguments* out. In
    /// `interface P extends VariantProps<typeof buttonVariants>` the inner
    /// `buttonVariants` has a `TypeQueryNode` parent, the loop stops there, and
    /// `typeof x` stays the value position §79 made it. Widening this to "any
    /// ancestor is a heritage clause" would silence that.
    pub(crate) fn identifier_in_non_emitting_heritage_clause(&self, node: NodeId) -> bool {
        if self.nodes.kind(node) != SyntaxKind::Identifier {
            return false;
        }
        let Some(mut at) = self.nodes.parent(node) else { return false };
        while matches!(
            self.nodes.kind(at),
            SyntaxKind::PropertyAccessExpression | SyntaxKind::ExpressionWithTypeArguments
        ) {
            let Some(parent) = self.nodes.parent(at) else { return false };
            at = parent;
        }
        let Some(Node::HeritageClause(clause)) = self.node_map.get(at) else { return false };
        clause.token.kind == SyntaxKind::ImplementsKeyword
            || self
                .nodes
                .parent(at)
                .is_some_and(|owner| self.nodes.kind(owner) == SyntaxKind::InterfaceDeclaration)
    }

    /// Does this class member carry its **own** `declare` modifier?
    ///
    /// `class G { declare [onInit]: any }` is ambient at the member, and
    /// `declaration_is_in_an_ambient_context` reads `declare` on the
    /// *declaration kinds that contain members*, never on a member itself —
    /// §81 recorded exactly this gap for TS7010 and it is the same one here.
    /// `NodeFlags::AMBIENT` would answer both and is never set. §134.
    pub(crate) fn member_has_declare_modifier(&self, member: NodeId) -> bool {
        let modifiers = match self.node_map.get(member) {
            Some(Node::PropertyDeclaration(n)) => n.modifiers,
            Some(Node::MethodDeclaration(n)) => n.modifiers,
            Some(Node::GetAccessorDeclaration(n)) => n.modifiers,
            Some(Node::SetAccessorDeclaration(n)) => n.modifiers,
            _ => return false,
        };
        has_modifier(modifiers, SyntaxKind::DeclareKeyword)
    }

    /// `HasSyntacticModifier(node.Parent, ModifierFlagsAbstract)`.
    fn member_is_abstract(&self, member: NodeId) -> bool {
        let modifiers = match self.node_map.get(member) {
            Some(Node::PropertyDeclaration(n)) => n.modifiers,
            Some(Node::MethodDeclaration(n)) => n.modifiers,
            Some(Node::GetAccessorDeclaration(n)) => n.modifiers,
            Some(Node::SetAccessorDeclaration(n)) => n.modifiers,
            // `abstract [onInit](): void` has **no body**, so it is
            // signature-shaped; §123 enumerated only the bodied kinds and so
            // missed `abstract class F`. §132.
            Some(Node::MethodSignatureDeclaration(n)) => n.modifiers,
            Some(Node::PropertySignatureDeclaration(n)) => n.modifiers,
            _ => return false,
        };
        has_modifier(modifiers, SyntaxKind::AbstractKeyword)
    }

    /// `getTypeOnlyAliasDeclarationEx` (`checker.go:1861`), reduced to its
    /// answer: **which kind** of declaration in the alias chain carried the
    /// `type`, or `None` if none did.
    ///
    /// Upstream follows re-exports and intermediate aliases rather than reading
    /// only the symbol's own declaration — §120 measured 7 wrong lines for
    /// stopping at the first hop. The walk here is the same one, bounded, using
    /// [`Checker::resolve_alias`]. §121.
    pub(crate) fn type_only_alias_declaration(
        &mut self,
        symbol: tsr_binder::SymbolId,
    ) -> Option<bool> {
        let mut current = self.binder.merged_symbol(symbol);
        for _ in 0..16 {
            let entry = self.binder.symbols().get(current);
            if !entry.flags.intersects(SymbolFlags::ALIAS) {
                return None;
            }
            let declaration = *entry.declarations.first()?;
            if let Some(exported) = self.declaration_is_type_only(declaration) {
                return Some(exported);
            }
            if self.specifier_type_only_export_star(declaration).is_some() {
                return Some(true);
            }
            current = self.binder.merged_symbol(self.resolve_alias(current)?);
        }
        None
    }

    /// `Some(true)` for `export type`, `Some(false)` for `import type`, `None`
    /// when this declaration is not type-only.
    fn declaration_is_type_only(&self, declaration: NodeId) -> Option<bool> {
        let enclosing = |kind: SyntaxKind| {
            self.nodes.ancestors(declaration).find(|&a| self.nodes.kind(a) == kind)
        };
        match self.node_map.get(declaration)? {
            Node::ImportSpecifier(n) if n.is_type_only => Some(false),
            // **A default import's declaration *is* the clause.** `enclosing`
            // walks ancestors, which is right for an `ImportSpecifier` or a
            // `NamespaceImport` — the clause is above them — and wrong for
            // `import type ns from './ns'`, where the alias's declaration is
            // the `ImportClause` itself and the walk finds nothing. §773.
            Node::ImportSpecifier(_) | Node::NamespaceImport(_) | Node::ImportClause(_) => {
                let clause = if self.nodes.kind(declaration) == SyntaxKind::ImportClause {
                    declaration
                } else {
                    enclosing(SyntaxKind::ImportClause)?
                };
                match self.node_map.get(clause)? {
                    Node::ImportClause(clause) => clause
                        .phase_modifier
                        .is_some_and(|token| token.kind == SyntaxKind::TypeKeyword)
                        .then_some(false),
                    _ => None,
                }
            }
            // `import type A = require('./a')` carries the flag on the
            // declaration itself rather than on an enclosing clause. §770.
            Node::ImportEqualsDeclaration(n) if n.is_type_only => Some(false),
            Node::ExportSpecifier(n) if n.is_type_only => Some(true),
            // `export type * as ns from './a'` binds a `NamespaceExport`, and
            // it reaches its `type` the same way an `ExportSpecifier` does —
            // off the enclosing declaration. §738.
            Node::ExportSpecifier(_) | Node::NamespaceExport(_) => {
                match self.node_map.get(enclosing(SyntaxKind::ExportDeclaration)?)? {
                    Node::ExportDeclaration(n) if n.is_type_only => Some(true),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn check_type_reference_name(&mut self, node: NodeId, text: &str) {
        // **A type position in JavaScript is already a different diagnostic.**
        // `type a = b` in a `.js` file is TS8008 and upstream stops there — the
        // annotation is a construct the file may not contain, so nothing inside
        // it is resolved. The value-position rule has carried this decline
        // since it was written; this one never got it. §779.
        // **No `file_has_parse_errors` here.** §254's −14 measurement was made
        // on `check_value_identifier`, which has never carried the gate; this
        // path always has and nobody had measured it. `interface I { a: Foo; b }`
        // is TS2304 **and** TS1005 upstream, and this port emitted only the
        // parse error. §898.
        // **The primitive spellings in a heritage clause are TS2863/2864/2840**,
        // and §880 routed them from the *value* path only. §334 recorded the
        // split four hundred sections earlier: an `implements` name is a **type**
        // reference and an `extends` name is a value one, so `implements string`
        // never reached the routing and took the spelling-suggestion rung
        // instead — nine lines of `classImplementsPrimitive` under TS2552. §925.
        // **`implements` only.** §880 already routes the `extends` half from
        // `check_value_identifier`, and an interface's `extends` name reaches
        // BOTH paths — the first draft of this doubled TS2840 on
        // `errorLocationForInterfaceExtension` and lost a case §890 had won.
        // That is §919's failure and §920's rule — *census the dispatch, expect
        // a second rule underneath* — six sections old and unapplied. §925.
        if matches!(text, "any" | "string" | "number" | "boolean" | "never" | "unknown")
            && self.nodes.ancestors(node).any(|ancestor| {
                matches!(self.node_map.get(ancestor), Some(Node::HeritageClause(clause))
                    if clause.token.kind == SyntaxKind::ImplementsKeyword)
            })
        {
            self.report_primitive_type_as_value_at(node, text);
            return;
        }
        if is_specially_diagnosed_name(text) || self.in_js_file(node) {
            return;
        }
        // **A duplicate heritage clause is recovered syntax.** `class C
        // implements A implements B` is TS1175 and upstream does not resolve
        // the types inside the second clause — the clause is not part of the
        // class, it is what the parser kept so the position could be reported.
        // The test is per token kind: `extends A implements B` is two legal
        // clauses. §792.
        if self.in_duplicate_heritage_clause(node) {
            return;
        }
        // **And the extra *types* in an `extends` clause.** A class extends
        // one class, so `typeNodes[1]` and beyond are what the parser kept in
        // order to have a position for TS1174 (`grammarchecks.go:911`) — the
        // same status §833 gave a second clause, one level down. §874.
        if self.is_extra_type_in_a_class_extends_clause(node) {
            return;
        }
        // Upstream reads `NodeFlagsInWithStatement` in `resolveName`
        // (`checker.go:29344`), which serves **both** name paths — a type name
        // inside a `with` block declines for the same reason a value name does.
        // The value path has carried this since it was written; this one never
        // got it. §834.
        if self.is_inside_with_statement(node) {
            return;
        }
        let Some(parent) = self.nodes.parent(node) else { return };
        // **An `implements` name is a type reference in a different node
        // kind.** `class C implements I` resolves `I` at `Type` and reports
        // TS2304 when it does not resolve, but its parent is an
        // `ExpressionWithTypeArguments`, not a `TypeReferenceNode`. An
        // `extends` name is a *value* reference and belongs to
        // `check_value_identifier`, which is the distinction
        // `is_value_reference` and `is_in_extends_clause` already draw. §334.
        match self.node_map.get(parent) {
            Some(Node::TypeReferenceNode(reference)) => {
                if reference.type_name.and_then(|name| name.node_id()) != Some(node) {
                    return;
                }
            }
            Some(Node::ExpressionWithTypeArguments(with_arguments)) => {
                if with_arguments.expression.and_then(|e| e.node_id()) != Some(node) {
                    return;
                }
                // `checkClassLikeDeclaration` checks `implements` types; an
                // interface's `implements` clause is only TS1176 and
                // `checkInterfaceDeclaration` never resolves it.
                let is_implements = self.nodes.parent(parent).is_some_and(|clause| {
                    matches!(
                        self.node_map.get(clause),
                        Some(Node::HeritageClause(heritage))
                            if heritage.token.kind == SyntaxKind::ImplementsKeyword
                    ) && self.nodes.parent(clause).is_some_and(|owner| {
                        matches!(
                            self.nodes.kind(owner),
                            SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
                        )
                    })
                });
                if !is_implements {
                    return;
                }
            }
            _ => return,
        }
        let span = self.error_span(node);
        if text.is_empty() || span.start == span.end {
            return;
        }
        // A `TYPE` hit is a correct resolution and stays silent. §167: the
        // other two are TS2709 and TS2749, which the cascade now selects —
        // reachable only since §166 stopped the globals fallback from
        // answering every meaning.
        // **An alias is not a resolution; its target is.** `getSymbol`
        // (`checker.go:1023`) admits an alias under a meaning only when
        // `resolveAlias(symbol)` carries that meaning, so `import modes =
        // _modes` does not satisfy a `TYPE` lookup and the cascade below gets
        // to say TS2709.
        //
        // Three things this port must add, each measured against the wrong
        // column (28 → 5 → 0 lines, §676 / §690 / §691 / §692):
        //
        // - `resolve_alias` declines a **qualified** module reference for the
        //   printer's sake (§686), so `import I = ns.IMode` needs
        //   [`Checker::qualified_alias_target`].
        // - **A target that cannot be resolved is silence, not a negative.**
        //   `import { Unresolved } from "foo"` with an unresolved `"foo"` has
        //   no target to ask; upstream answers the module error and an error
        //   type. §25's collapse.
        // - **The chain is followed to a non-alias.** A re-export resolves one
        //   hop to another alias, and upstream's `resolveAlias` recurses. The
        //   last five wrong lines were all `import type` through a re-export.
        //
        // §692.
        if let Some(hit) = self.resolve_name_with_export_alias(node, text, SymbolFlags::TYPE) {
            // `getSymbol` tests `symbol.Flags & meaning` **first** and only
            // then falls back to the alias. `import * as B from "./b"` merged
            // with `interface B {}` carries both, and the interface half is a
            // correct `TYPE` resolution — following the alias instead reports
            // on a name that resolves. `noCrashOnImportShadowing` is the
            // fixture and it was the last two wrong lines. §692.
            if self.alias_chain_carries(hit, SymbolFlags::TYPE) {
                return;
            }
        }
        if self.report_meaning_mismatch_in_type_position(node, text) {
            return;
        }
        for meaning in [SymbolFlags::VALUE, SymbolFlags::NAMESPACE] {
            if self.binder.resolve_name(self.nodes, self.node_map, node, text, meaning).is_some() {
                return;
            }
        }
        // A name an enclosing declaration introduces as a **type parameter**
        // resolves upstream and fails here, and the difference is never
        // TS2304. `class C<T> { static m(): T }` is upstream's TS2302,
        // *"Static members cannot reference class type parameters"* — the
        // resolver finds `T` and the *position* is the error — and an
        // `infer T` name is in scope for the whole conditional type.
        // `genericClassWithStaticsUsingTypeArguments`,
        // `classTypeParametersInStatics`, `staticMethodReferencingTypeArgument1`,
        // `typeParametersInStatic*` and `conditionalTypes1` were the first
        // measurement's largest new family.
        if self.an_enclosing_declaration_has_type_parameter(node, text) {
            // …and when the enclosing declaration is a **class** and the member
            // the walk came up through is **static**, upstream does not merely
            // decline — it reports. §114 tried this *before* resolution and
            // measured +137 wrong lines, because a name that merely spells the
            // same as a type parameter while resolving to something else took
            // the branch. Asking here, on the path where resolution has
            // **already failed**, is the same question upstream asks and cannot
            // catch a name that resolved. See §116.
            if self.static_member_references_class_type_parameter(node, text) {
                let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
                self.report(
                    file,
                    Diagnostic::new(
                        &messages::STATIC_MEMBERS_CANNOT_REFERENCE_CLASS_TYPE_PARAMETERS,
                        span,
                    ),
                );
            }
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        if let Some(lib) = suggested_lib_for(text) {
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::CANNOT_FIND_NAME_0_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CHANGING_THE_LIB_COMPILER_OPTION_TO_1_OR_LATER,
                    span,
                    [text.to_string(), lib.to_string()],
                ),
            );
            return;
        }
        // The suggestion arm at `resolveTypeReferenceName`'s meaning, `Type`.
        if let Some(suggestion) = self.spelling_suggestion_for(node, text, SymbolFlags::TYPE) {
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::CANNOT_FIND_NAME_0_DID_YOU_MEAN_1,
                    span,
                    [text.to_string(), suggestion],
                ),
            );
            return;
        }
        // The same fallback as the value site — upstream reaches the table from
        // a **type** position too (`checker.go:15784`, via `GetFirstIdentifier`).
        let message = cannot_find_name_message(text).unwrap_or(&messages::CANNOT_FIND_NAME_0);
        self.report(file, Diagnostic::with_args(message, span, [text.to_string()]));
    }

    /// Does any ancestor introduce `text` as a type parameter — a declaration's
    /// `<T>` list, or an `infer T` inside a conditional type?
    ///
    /// A scoping question this port's `resolve_name` answers differently from
    /// upstream's, and the difference is always a *different code* rather than
    /// a missing one. See [`Checker::check_type_reference_name`].
    fn an_enclosing_declaration_has_type_parameter(&self, node: NodeId, text: &str) -> bool {
        for ancestor in self.nodes.ancestors(node) {
            let Some(typed) = self.node_map.get(ancestor) else { continue };
            let parameters = type_parameters_of(typed);
            if parameters
                .iter()
                .any(|parameter| parameter.name.is_some_and(|name| name.text == text))
            {
                return true;
            }
            if self.nodes.kind(ancestor) == SyntaxKind::ConditionalType
                && self.subtree_declares_infer(ancestor, text, 0)
            {
                return true;
            }
        }
        false
    }

    /// Is there an `infer <text>` anywhere under this node?
    fn subtree_declares_infer(&self, node: NodeId, text: &str, depth: u32) -> bool {
        if depth > 32 {
            return false;
        }
        let Some(typed) = self.node_map.get(node) else { return false };
        if let Node::InferTypeNode(infer) = typed
            && infer
                .type_parameter
                .is_some_and(|parameter| parameter.name.is_some_and(|name| name.text == text))
        {
            return true;
        }
        let mut children = Vec::new();
        tsr_ast::for_each_child_id(typed, |child| children.push(child));
        children.into_iter().any(|child| self.subtree_declares_infer(child, text, depth + 1))
    }

    /// Is this identifier inside an **instance** property's initialiser, naming
    /// a parameter of the enclosing class's constructor?
    ///
    /// Answers the property's name, for the message. `static` members are
    /// excluded: a static initialiser is not in the constructor's scope in
    /// either direction, and upstream's check is on `PropertyDeclaration`
    /// without the static modifier.
    fn property_initializer_referencing_a_constructor_parameter(
        &self,
        node: NodeId,
        text: &str,
    ) -> Option<String> {
        // Walk out to the property declaration, stopping at anything that
        // introduces its own `this` or its own scope boundary for this purpose.
        let mut at = self.nodes.parent(node)?;
        let property = loop {
            match self.node_map.get(at)? {
                Node::PropertyDeclaration(property) => break property,
                Node::ClassDeclaration(_) | Node::ClassExpression(_) | Node::SourceFile(_) => {
                    return None;
                }
                _ => at = self.nodes.parent(at)?,
            }
        };
        if has_modifier(property.modifiers, SyntaxKind::StaticKeyword) {
            return None;
        }
        // **A name declared inside the initializer is not the constructor's.**
        // `messageHandler = () => { var field = this.field; console.log(field) }`
        // names the lambda's own local, and upstream's resolver finds *that*
        // before it ever reaches the constructor — `propertyWithInvalidInitializer`
        // is set only when the winning declaration is the constructor's.
        // `classMemberInitializerWithLamdaScoping`, §721's one wrong line. §722.
        if self
            .binder
            .resolve_name(self.nodes, self.node_map, node, text, SymbolFlags::VALUE)
            .and_then(|symbol| {
                self.binder
                    .symbols()
                    .get(self.binder.merged_symbol(symbol))
                    .declarations
                    .first()
                    .copied()
            })
            .is_some_and(|declaration| {
                self.nodes.ancestors(declaration).any(|ancestor| ancestor == at)
            })
        {
            return None;
        }
        let name = property.name.node_id().and_then(|id| self.identifier_text(id))?.to_string();
        let class = self.nodes.parent(at)?;
        let members: &[ClassElement<'_>] = match self.node_map.get(class)? {
            Node::ClassDeclaration(declaration) => declaration.members,
            Node::ClassExpression(declaration) => declaration.members,
            _ => return None,
        };
        for member in members {
            let ClassElement::ConstructorDeclaration(constructor) = member else { continue };
            for parameter in constructor.parameters {
                let Some(tsr_ast::BindingName::Identifier(written)) = parameter.name else {
                    continue;
                };
                if written.text == text {
                    return Some(name);
                }
            }
            // **A constructor's locals shadow too**, and that is upstream's
            // *second* situation: `var y = 1; class D { b = y; constructor() {
            // var y = ""; } }` resolves `y` perfectly well, to the outer one,
            // and is still an error because the initializer is emitted inside
            // the constructor. `checkAndReportErrorForInvalidInitializer`'s
            // header says it is *"needed in two situations: 1. When result is
            // undefined … 2. When result is defined"*, and this port had the
            // first. §720.
            if let Some(body) = constructor.body.and_then(|body| body.node_id())
                && self.constructor_body_declares(body, text)
            {
                return Some(name);
            }
        }
        None
    }

    /// Does this constructor body declare `text` as a variable of its own?
    ///
    /// **Nested functions are not descended into**: a `var` inside a closure in
    /// the constructor is not a constructor local, and upstream's `result`
    /// would not be it. §720.
    fn constructor_body_declares(&self, node: NodeId, text: &str) -> bool {
        if self.is_function_like_or_static_block(node) {
            return false;
        }
        if let Some(Node::VariableDeclaration(declaration)) = self.node_map.get(node)
            && let Some(tsr_ast::BindingName::Identifier(name)) = declaration.name
            && name.text == text
        {
            return true;
        }
        let mut children = Vec::new();
        if let Some(typed) = self.node_map.get(node) {
            tsr_ast::for_each_child_id(typed, |child| children.push(child));
        }
        children.into_iter().any(|child| self.constructor_body_declares(child, text))
    }

    /// Is this identifier in a slot where upstream would call
    /// `getResolvedSymbol` on it?
    ///
    /// An allow-list over the **parent's** shape: the identifier must be the
    /// node sitting in one of the parent's expression-typed fields. Every arm
    /// names a field rather than a kind, because the discriminating question is
    /// never "what is the parent" but "which of its slots is this" — a
    /// `PropertyAccessExpression` resolves its `expression` and never its
    /// `name`, and conflating those reports `Cannot find name 'length'` on every
    /// `a.length` in the corpus.
    #[allow(
        clippy::too_many_lines,
        reason = "one arm per expression-bearing node kind; splitting it would \
                  hide the exhaustiveness that is the point of the list"
    )]
    pub(crate) fn is_value_reference(&self, node: NodeId) -> bool {
        let Some(parent) = self.nodes.parent(node) else { return false };
        let Some(typed) = self.node_map.get(parent) else { return false };
        let is = |slot: Option<NodeId>| slot == Some(node);
        let is_any = |slots: &[Option<NodeId>]| slots.contains(&Some(node));
        match typed {
            Node::ExpressionStatement(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::PropertyAccessExpression(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::ElementAccessExpression(n) => is_any(&[
                n.expression.and_then(|e| e.node_id()),
                n.argument_expression.and_then(|e| e.node_id()),
            ]),
            Node::CallExpression(n) => {
                is(n.expression.and_then(|e| e.node_id()))
                    || n.arguments.iter().any(|a| a.node_id() == Some(node))
            }
            Node::NewExpression(n) => {
                is(n.expression.and_then(|e| e.node_id()))
                    || n.arguments.iter().any(|a| a.node_id() == Some(node))
            }
            Node::BinaryExpression(n) => {
                is_any(&[n.left.and_then(|e| e.node_id()), n.right.and_then(|e| e.node_id())])
            }
            Node::PrefixUnaryExpression(n) => is(n.operand.and_then(|e| e.node_id())),
            Node::PostfixUnaryExpression(n) => is(n.operand.and_then(|e| e.node_id())),
            Node::ConditionalExpression(n) => is_any(&[
                n.condition.and_then(|e| e.node_id()),
                n.when_true.and_then(|e| e.node_id()),
                n.when_false.and_then(|e| e.node_id()),
            ]),
            Node::ParenthesizedExpression(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::ArrayLiteralExpression(n) => n.elements.iter().any(|e| e.node_id() == Some(node)),
            // The *initialiser*, never the name — `{ a: b }` resolves `b`.
            Node::PropertyAssignment(n) => is(n.initializer.and_then(|e| e.node_id())),
            // `{ a }` is both a name and a reference, which is the one place a
            // declaration name is also resolved.
            Node::ShorthandPropertyAssignment(n) => {
                is(n.name.node_id())
                    || is(n.object_assignment_initializer.and_then(|e| e.node_id()))
            }
            // **An arrow's concise body is a value position**, and it is the
            // only place in the grammar where an expression hangs directly off
            // a function — which is why a list of thirty expression parents
            // could look complete without it. `private c = () => x` was the
            // fixture. §723.
            Node::ArrowFunction(n) => is(n.body.and_then(|e| e.node_id())),
            // `<a {...x} />` holds its expression directly rather than through
            // a `JsxExpression`, so the arm above it does not cover it. §725.
            Node::JsxSpreadAttribute(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::SpreadElement(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::SpreadAssignment(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::TemplateSpan(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::TaggedTemplateExpression(n) => is(n.tag.and_then(|e| e.node_id())),
            Node::TypeAssertion(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::AsExpression(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::SatisfiesExpression(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::NonNullExpression(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::AwaitExpression(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::YieldExpression(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::TypeOfExpression(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::VoidExpression(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::DeleteExpression(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::VariableDeclaration(n) => is(n.initializer.and_then(|e| e.node_id())),
            Node::ParameterDeclaration(n) => is(n.initializer.and_then(|e| e.node_id())),
            Node::PropertyDeclaration(n) => is(n.initializer.and_then(|e| e.node_id())),
            Node::BindingElement(n) => is(n.initializer.and_then(|e| e.node_id())),
            Node::EnumMember(n) => is(n.initializer.and_then(|e| e.node_id())),
            Node::ReturnStatement(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::ThrowStatement(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::IfStatement(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::WhileStatement(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::DoStatement(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::SwitchStatement(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::CaseOrDefaultClause(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::ForStatement(n) => is_any(&[
                n.initializer.and_then(|e| e.node_id()),
                n.condition.and_then(|e| e.node_id()),
                n.incrementor.and_then(|e| e.node_id()),
            ]),
            Node::ForInOrOfStatement(n) => is_any(&[
                n.initializer.and_then(|e| e.node_id()),
                n.expression.and_then(|e| e.node_id()),
            ]),
            Node::ComputedPropertyName(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::Decorator(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::ExportAssignment(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::JsxExpression(n) => is(n.expression.and_then(|e| e.node_id())),
            // `class C extends B` resolves `B` as a value; `implements I` does
            // not, and the two share this node kind. The heritage clause's
            // keyword is what separates them.
            //
            // **An interface's `extends` stays a reference here, deliberately.**
            // It is a *type* position — `identifier_in_non_emitting_heritage_clause`
            // says so, and the rules that care consult it — but upstream still
            // reports `Cannot find name` for an unresolved one through
            // `resolveEntityName`'s failure at type meaning, and TS2304 is
            // keyed on this predicate. `compiler/protoAssignment`
            // (`interface Number extends Comparable<number>`) is the case, and
            // declining here silently lost it.
            // Pinned `checkExpressionWithTypeArguments` checks the expression
            // as a value outside heritage clauses. Type arguments are not this
            // slot; keep the existing extends/implements distinction unchanged.
            Node::ExpressionWithTypeArguments(n) => {
                is(n.expression.and_then(|e| e.node_id()))
                    && self.nodes.parent(parent).is_some_and(|owner| {
                        match self.node_map.get(owner) {
                            Some(Node::HeritageClause(heritage)) => {
                                heritage.token.kind == SyntaxKind::ExtendsKeyword
                            }
                            _ => true,
                        }
                    })
            }
            // `typeof A` — the one place in the grammar where a *type node*
            // holds a value slot. `getTypeFromTypeQueryNode` (`checker.go:22964`)
            // resolves the entity name with `SymbolFlagsValue`, so an
            // unresolvable name there is the same TS2304 an expression gets:
            // `interface I1 { a: number; b: typeof a }` is
            // `compiler/typeofProperty`, whose own comments read *"Should yield
            // error (a is not a value)"*. §79.
            Node::TypeQueryNode(n) => is(n.expr_name.and_then(|name| name.node_id())),
            // `typeof A.B` resolves `A` as a value and `B` as its member, so
            // only the **leftmost** identifier of the chain is a reference — and
            // only when the chain's root is a type query. A qualified name under
            // a plain `TypeReferenceNode` is a *namespace* miss, which upstream
            // reports as TS2503 at the same position; firing there would be a
            // wrong code, which is what this allow-list exists to prevent.
            Node::QualifiedName(n) => {
                is(n.left.and_then(|left| left.node_id()))
                    && self.entity_name_root_is_a_type_query(parent)
            }
            _ => false,
        }
    }

    /// Walk out of a `QualifiedName` chain and ask whether it hangs off a
    /// `TypeQueryNode` — see [`Checker::is_value_reference`]'s `QualifiedName`
    /// arm for why the question is asked at all.
    fn entity_name_root_is_a_type_query(&self, mut at: NodeId) -> bool {
        // A qualified name nests only to the left, so the walk is the chain's
        // length. The bound is a cycle guard, not a depth limit: `A.B.C.D…` in
        // the corpus is three deep at most.
        for _ in 0..64 {
            let Some(parent) = self.nodes.parent(at) else { return false };
            match self.nodes.kind(parent) {
                SyntaxKind::QualifiedName => at = parent,
                SyntaxKind::TypeQuery => return true,
                _ => return false,
            }
        }
        false
    }

    /// TS2449 — `Class '{0}' used before its declaration.`
    ///
    /// `checkResolvedBlockScopedVariable` (`checker.go:1888`), gated on
    /// `declaration.Flags&NodeFlagsAmbient == 0 &&
    /// !isBlockScopedNameDeclaredBeforeUse(declaration, errorLocation)`.
    ///
    /// # Bounded to an `extends` clause, and that is what makes it cheap
    ///
    /// `isBlockScopedNameDeclaredBeforeUse` (`checker.go:1922`) is eighty lines
    /// and almost all of them are about **deferral** — a use inside a function
    /// body, an instance property initialiser, an export specifier, a binding
    /// element, a decorator, a computed property name — each legal because the
    /// code does not run yet.
    ///
    /// `class A extends B` evaluates `B` at class-definition time, in the
    /// enclosing scope, immediately. **There is no function between the use and
    /// the declaration by construction**, so every deferral arm is excluded by
    /// the position rather than by a test, and what is left is the two lines
    /// that matter: the same file, and the declaration starts after the use.
    ///
    /// The other positions are a second slice with that predicate as its
    /// subject — `checker-notes-diag2.md` §83. TS2448 (block-scoped variable)
    /// and TS2450 (enum) are its siblings and wait on the same thing.
    /// TS1540 — `A 'namespace' declaration should not be declared using the
    /// 'module' keyword. Please use the 'namespace' keyword instead.`
    ///
    /// `checkModuleDeclaration` (`checker.go:5155`). Two conjuncts, both fields
    /// on the node: the name is an **identifier**, and `keyword` is `module`.
    /// A string name is an ambient module and takes neither this nor §1017's
    /// relative-path error. §1041.
    fn check_module_keyword_deprecated(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(Node::ModuleDeclaration(module)) = self.node_map.get(node) else { return };
        if module.keyword.kind != SyntaxKind::ModuleKeyword {
            return;
        }
        let Some(name) = module.name.and_then(|n| n.node_id()) else { return };
        if self.nodes.kind(name) != SyntaxKind::Identifier {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(name) else { return };
        let span = self.nodes.span(name);
        self.report(
            file,
            Diagnostic::new(
                &messages::A_NAMESPACE_DECLARATION_SHOULD_NOT_BE_DECLARED_USING_THE_MODULE_KEYWORD_PLEASE_USE_THE_NAMESPACE_KEYWORD_INSTEAD,
                span,
            ),
        );
    }

    /// TS1268 — `An index signature parameter type must be 'string', 'number',
    /// 'symbol', or a template literal type.`
    ///
    /// `checkGrammarIndexSignature` (`grammarchecks.go:832`) asks
    /// `everyType(t, isValidIndexKeyType)`. The **written** annotation answers
    /// it for the corpus's shapes, and the arms *above* it in the same function
    /// bound the slice: a literal or generic type is **TS1337** and a missing
    /// annotation is TS1148, so a type parameter must stay silent here even
    /// though it is not a valid key type. §1033.
    fn check_index_signature_parameter_type(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.index_signature_parameter_shape_error(node).is_some()
        {
            return;
        }
        let Some(Node::IndexSignatureDeclaration(signature)) = self.node_map.get(node) else {
            return;
        };
        let [parameter] = signature.parameters else { return };
        if parameter.dot_dot_dot_token.is_some()
            || parameter.question_token.is_some()
            || parameter.initializer.is_some()
            || !parameter.modifiers.is_empty()
        {
            return;
        }
        let Some(annotation) = parameter.r#type.and_then(|t| t.node_id()) else { return };
        let invalid = match self.nodes.kind(annotation) {
            SyntaxKind::AnyKeyword
            | SyntaxKind::BooleanKeyword
            | SyntaxKind::VoidKeyword
            | SyntaxKind::NeverKeyword
            | SyntaxKind::UnknownKeyword
            | SyntaxKind::ObjectKeyword
            | SyntaxKind::BigIntKeyword => true,
            // **This rule's own answer, not §985's.** That helper asks *is this
            // definitely a class or interface* and says `false` when unsure,
            // which suits TS2370. `isValidIndexKeyType` is a small allow-list,
            // so a reference here is invalid **unless** something says it might
            // be valid — an alias (`type S = string`) or a type parameter
            // (TS1337's cell). §1035.
            SyntaxKind::TypeReference => !self.type_reference_may_be_a_key(annotation),
            _ => false,
        };
        if !invalid {
            return;
        }
        let Some(name) = parameter.name.and_then(|n| n.node_id()) else { return };
        let Some(file) = self.source_file_of_for_diagnostics(name) else { return };
        let span = self.nodes.span(name);
        self.report(
            file,
            Diagnostic::new(
                &messages::AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LITERAL_TYPE,
                span,
            ),
        );
    }

    /// Might this written type reference be a valid index key? A **type alias**
    /// could name one, and a **type parameter** is TS1337's cell rather than
    /// this rule's. Everything else — an interface, a class, an unresolved
    /// name — is not. §1035.
    fn type_reference_may_be_a_key(&mut self, annotation: NodeId) -> bool {
        let Some(Node::TypeReferenceNode(reference)) = self.node_map.get(annotation) else {
            return true;
        };
        let Some(name) = reference.type_name.and_then(|n| n.node_id()) else { return true };
        let Some(text) = self.identifier_text(name).map(str::to_string) else { return true };
        let Some(symbol) =
            self.binder.resolve_name(self.nodes, self.node_map, name, &text, SymbolFlags::TYPE)
        else {
            return false;
        };
        let declarations =
            self.binder.symbols().get(self.binder.merged_symbol(symbol)).declarations.clone();
        declarations.iter().any(|&declaration| {
            matches!(
                self.nodes.kind(declaration),
                SyntaxKind::TypeAliasDeclaration | SyntaxKind::TypeParameter
            )
        })
    }

    /// TS2499 — `An interface can only extend an identifier/qualified-name with
    /// optional type arguments.`
    ///
    /// `checkInterfaceDeclaration` (`checker.go:5017`). `IsEntityNameExpression`
    /// is an identifier or a property access whose chain is entirely
    /// identifiers; everything else is the error. Written as *is an entity
    /// name* rather than as a list of what is not, because the rule reports on
    /// a **negative** — §1019's decision about defaults. §1031.
    fn check_interface_extends_entity_name(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(Node::InterfaceDeclaration(interface)) = self.node_map.get(node) else { return };
        let mut reports = Vec::new();
        for clause in interface.heritage_clauses {
            if clause.token.kind != SyntaxKind::ExtendsKeyword {
                continue;
            }
            for base in clause.types {
                let Some(expression) = base.expression.and_then(|e| e.node_id()) else { continue };
                if !self.is_entity_name_expression(expression) {
                    reports.push(expression);
                }
            }
        }
        for at in reports {
            let Some(file) = self.source_file_of_for_diagnostics(at) else { continue };
            let span = self.nodes.span(at);
            self.report(
                file,
                Diagnostic::new(
                    &messages::AN_INTERFACE_CAN_ONLY_EXTEND_AN_IDENTIFIER_SLASHQUALIFIED_NAME_WITH_OPTIONAL_TYPE_ARGUMENTS,
                    span,
                ),
            );
        }
    }

    /// TS2511 — `Cannot create an instance of an abstract class.`
    ///
    /// `checkNewExpression` (`checker.go:8615`) reads the constructed type's
    /// class declaration. When the target is an **identifier** the declaration
    /// is reachable without a type.
    ///
    /// **Abstractness does not inherit**: `class B extends A {}` with `A`
    /// abstract is concrete, and the fixture puts `new A()` and `new B()` on
    /// adjacent lines to catch a rule that walks the heritage chain. §999 walks
    /// that chain for a different rule, which is why this one must not. §1029.
    fn check_new_on_abstract_class(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::NewExpression(expression)) = self.node_map.get(node) else { return };
        let Some(callee) = expression.expression.and_then(|e| e.node_id()) else { return };
        if self.nodes.kind(callee) != SyntaxKind::Identifier {
            return;
        }
        let Some(text) = self.identifier_text(callee).map(str::to_string) else { return };
        let Some(symbol) =
            self.binder.resolve_name(self.nodes, self.node_map, callee, &text, SymbolFlags::VALUE)
        else {
            return;
        };
        let declarations =
            self.binder.symbols().get(self.binder.merged_symbol(symbol)).declarations.clone();
        let abstract_class = declarations.iter().any(|&declaration| {
            matches!(
                self.nodes.kind(declaration),
                SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
            ) && self
                .node_map
                .get(declaration)
                .and_then(modifiers_of)
                .is_some_and(|m| has_modifier(m, SyntaxKind::AbstractKeyword))
        });
        if !abstract_class {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.nodes.span(node);
        self.report(
            file,
            Diagnostic::new(&messages::CANNOT_CREATE_AN_INSTANCE_OF_AN_ABSTRACT_CLASS, span),
        );
    }

    /// TS2463 — `A binding pattern parameter cannot be optional in an
    /// implementation signature.`
    ///
    /// `checkParameter` (`checker.go:2677`). Four conjuncts, all syntactic: no
    /// initializer, a `?`, a **binding pattern** name, and an owner **with a
    /// body** — §1015's question asked the other way round. §1025.
    fn check_optional_binding_pattern_parameter(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(Node::ParameterDeclaration(parameter)) = self.node_map.get(node) else { return };
        if parameter.initializer.is_some() || parameter.question_token.is_none() {
            return;
        }
        if !matches!(parameter.name, Some(tsr_ast::BindingName::BindingPattern(_))) {
            return;
        }
        let Some(owner) = self.nodes.parent(node) else { return };
        if !self.declaration_has_body(owner) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.nodes.span(node);
        self.report(
            file,
            Diagnostic::new(
                &messages::A_BINDING_PATTERN_PARAMETER_CANNOT_BE_OPTIONAL_IN_AN_IMPLEMENTATION_SIGNATURE,
                span,
            ),
        );
    }

    /// TS2406 — `The left-hand side of a 'for...in' statement must be a
    /// variable or a property access.`
    ///
    /// `checkForInStatement` (`checker.go:4013`) hands the left-hand side to
    /// `checkReferenceExpression`. No type is needed: an identifier, a property
    /// access and an element access are references; a call, a `new`, a literal
    /// and anything else are not. The `for (var a in b)` form never reaches
    /// here — its initializer is a declaration list. §1019.
    fn check_for_in_reference_expression(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::ForInOrOfStatement(statement)) = self.node_map.get(node) else { return };
        if statement.kind.kind != SyntaxKind::ForInStatement {
            return;
        }
        let Some(initializer) = statement.initializer.and_then(|i| i.node_id()) else { return };
        if !matches!(
            self.nodes.kind(initializer),
            SyntaxKind::CallExpression
                | SyntaxKind::NewExpression
                | SyntaxKind::NumericLiteral
                | SyntaxKind::StringLiteral
                | SyntaxKind::TaggedTemplateExpression
        ) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(initializer) else { return };
        let span = self.nodes.span(initializer);
        self.report(
            file,
            Diagnostic::new(
                &messages::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_A_VARIABLE_OR_A_PROPERTY_ACCESS,
                span,
            ),
        );
    }

    /// TS2323 — `Cannot redeclare exported variable '{0}'.`
    ///
    /// `checkExportsOnMergedDeclarations`'s exports loop (`checker.go:5716`)
    /// over `getExportsOfModule(moduleSymbol)`.
    ///
    /// §1010 built this over the whole *declared* export table and measured
    /// +4 with 2 lost: upstream's `getExportsOfModule` **resolves** — export
    /// stars and `export =`. Neither changes a name the module declares
    /// itself — own exports shadow star exports and are not alias-resolved —
    /// so this walks the declared table and skips only a module with an
    /// `export =` (whose exports are the target's). §1011 restricted it to
    /// `default`; this is the declared-name generalisation.
    fn check_exported_redeclarations(&mut self, file: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(file) {
            return;
        }
        let Some(module) = self.binder.symbol_of(file) else { return };
        let module = self.binder.merged_symbol(module);
        let exports = &self.binder.symbols().get(module).exports;
        if exports.contains_key("export=") {
            return;
        }
        let mut named: Vec<(String, tsr_binder::SymbolId)> =
            exports.iter().map(|(name, &symbol)| (name.to_string(), symbol)).collect();
        named.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        for (name, exported) in named {
            if name == "__export" {
                continue;
            }
            let symbol = self.binder.merged_symbol(exported);
            let entry = self.binder.symbols().get(symbol);
            let flags = entry.flags;
            let declarations = entry.declarations.clone();
            let counted: Vec<NodeId> = declarations
                .iter()
                .copied()
                .filter(|&declaration| self.declaration_counts_for_redeclaration(declaration))
                .collect();
            // `SymbolFlagsNamespace | SymbolFlagsEnum` merge legally and are
            // skipped before the count; a type alias merged with one value is
            // legal, which upstream spells as `TypeAlias` with a count of `<= 2`.
            if flags.intersects(SymbolFlags::NAMESPACE) {
                continue;
            }
            if flags.intersects(SymbolFlags::TYPE_ALIAS) && counted.len() <= 2 {
                continue;
            }
            if counted.len() <= 1 {
                continue;
            }
            // `isNotOverload(declaration)` — every non-overload declaration,
            // interfaces included, is reported.
            for declaration in declarations {
                if !self.declaration_is_not_overload(declaration) {
                    continue;
                }
                let Some(at) = self.source_file_of_for_diagnostics(declaration) else { continue };
                // **`c.error(declaration, …)` folds to the declaration's name**
                // (§11): `export default class Foo {}` reports at `Foo`. An
                // `export default expr` has no name and keeps the statement's
                // span. §1013.
                let span = self.error_span(declaration);
                self.report(
                    at,
                    Diagnostic::with_args(
                        &messages::CANNOT_REDECLARE_EXPORTED_VARIABLE_0,
                        span,
                        [name.clone()],
                    ),
                );
            }
        }
    }

    /// `isNotOverload` (`checker.go`): a function-like declaration without a
    /// body is an overload; every other declaration is not.
    fn declaration_is_not_overload(&self, declaration: NodeId) -> bool {
        match self.node_map.get(declaration) {
            Some(Node::FunctionDeclaration(n)) => n.body.is_some(),
            Some(Node::MethodDeclaration(n)) => n.body.is_some(),
            _ => true,
        }
    }

    /// `isNotOverload(d) && !IsAccessor(d) && !IsInterfaceDeclaration(d)`. §1011.
    fn declaration_counts_for_redeclaration(&self, declaration: NodeId) -> bool {
        match self.node_map.get(declaration) {
            Some(Node::FunctionDeclaration(n)) => n.body.is_some(),
            Some(Node::MethodDeclaration(n)) => n.body.is_some(),
            Some(
                Node::GetAccessorDeclaration(_)
                | Node::SetAccessorDeclaration(_)
                | Node::InterfaceDeclaration(_),
            ) => false,
            _ => true,
        }
    }

    /// TS2405 — `The left-hand side of a 'for...in' statement must be of type
    /// 'string' or 'any'.`
    ///
    /// `checkForInStatement`'s expression arm (`checker.go:4009`): for an
    /// initializer that is an expression rather than a declaration list and
    /// not an array/object literal (TS2491's arm),
    /// `isTypeAssignableTo(getIndexTypeOrString(rightType), leftType)`.
    /// `getIndexTypeOrString` (`checker.go:4027`) is the string part of the
    /// right operand's keys, or `string` when that is `never`; this port asks
    /// with `string`, which is exact whenever the left type accepts no
    /// narrower string, so a left type that could accept only some keys
    /// (string literals, templates, mappings, type variables) declines.
    fn check_for_in_variable_type(&mut self, node: NodeId) {
        if self.in_js_file(node) {
            return;
        }
        let Some(Node::ForInOrOfStatement(statement)) = self.node_map.get(node) else { return };
        // **`ForInOrOfStatement::kind` is the node's own kind**, not the `in`
        // or `of` keyword. §1006.
        if statement.kind.kind != SyntaxKind::ForInStatement {
            return;
        }
        let Some(initializer) = statement.initializer else { return };
        let Some(at) = initializer.node_id() else { return };
        if matches!(
            self.nodes.kind(at),
            SyntaxKind::VariableDeclarationList
                | SyntaxKind::ArrayLiteralExpression
                | SyntaxKind::ObjectLiteralExpression
        ) {
            return;
        }
        let Ok(expression) = tsr_ast::Expression::try_from(Node::from(initializer)) else {
            return;
        };
        let left = self.check_expression(expression);
        if self.is_error(left) || self.accepts_some_narrower_string(left) {
            return;
        }
        let string = self.intrinsics.string;
        if self.relate_ternary(string, left, crate::relater::Relation::Assignable)
            != crate::relater::Ternary::NotRelated
        {
            // `else { checkReferenceExpression(...) }` (`checker.go:4013`):
            // only after the type test passes. Its first arm is
            // `check_for_in_reference_expression` (TS2406); this is the
            // optional-chain arm. §9 of `docs/parity/notes/misc-checks.md`.
            if self.is_optional_chain_reference(at) {
                self.report_reference_error(
                    at,
                    &messages::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_ACCESS,
                );
            }
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        self.report(
            file,
            Diagnostic::new(
                &messages::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_OF_TYPE_STRING_OR_ANY,
                span,
            ),
        );
    }

    /// Could `ty` accept a string narrower than `string` — a string literal,
    /// template literal, string mapping or type variable, alone or as a union
    /// or intersection constituent?
    fn accepts_some_narrower_string(&self, ty: crate::types::TypeId) -> bool {
        let flags = self.store.get(ty).flags;
        if flags.intersects(
            crate::flags::TypeFlags::STRING_LITERAL
                | crate::flags::TypeFlags::TEMPLATE_LITERAL
                | crate::flags::TypeFlags::STRING_MAPPING
                | crate::flags::TypeFlags::INSTANTIABLE,
        ) {
            return true;
        }
        match &self.store.get(ty).data {
            crate::types::TypeData::Union { types, .. }
            | crate::types::TypeData::Intersection { types, .. } => {
                types.iter().any(|&member| self.accepts_some_narrower_string(member))
            }
            _ => false,
        }
    }

    /// TS2407 — `The right-hand side of a 'for...in' statement must be of type
    /// 'any', an object type or a type parameter, but here has type '{0}'.`
    ///
    /// `checkForInStatement`'s tail (`checker.go:4018`): the expression's type,
    /// made non-nullable when nullable (`getNonNullableTypeIfNeeded`), must be
    /// `never`-free and `isTypeAssignableToKind(NonPrimitive |
    /// InstantiableNonPrimitive)` (`checker.go:27645`): the kind flags, else
    /// assignability to `object`. Only a definite `NotRelated` reports.
    fn check_for_in_right_operand(&mut self, node: NodeId) {
        let Some(Node::ForInOrOfStatement(statement)) = self.node_map.get(node) else { return };
        if statement.kind.kind != SyntaxKind::ForInStatement {
            return;
        }
        let Some(expression) = statement.expression else { return };
        let Some(at) = expression.node_id() else { return };
        // `getNonNullableTypeIfNeeded`: `GetNonNullableType` leaves a type
        // without null/undefined facts unchanged, so the nullable test is the
        // adjustment's own.
        let right = self.check_expression(expression);
        let right = self.get_non_nullable_type(right);
        if self.is_error(right) {
            return;
        }
        let never = right == self.intrinsics.never;
        if !never {
            if self.store.get(right).flags.intersects(
                crate::flags::TypeFlags::NON_PRIMITIVE
                    | crate::flags::TypeFlags::INSTANTIABLE_NON_PRIMITIVE,
            ) {
                return;
            }
            let object = self.intrinsics.non_primitive;
            if self.relate_ternary(right, object, crate::relater::Relation::Assignable)
                != crate::relater::Ternary::NotRelated
            {
                return;
            }
        }
        let printed = self.type_to_string(right);
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::THE_RIGHT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_MUST_BE_OF_TYPE_ANY_AN_OBJECT_TYPE_OR_A_TYPE_PARAMETER_BUT_HERE_HAS_TYPE_0,
                span,
                [printed],
            ),
        );
    }

    /// TS2436 — `Ambient module declaration cannot specify relative module
    /// name.`
    ///
    /// `checkModuleDeclaration` (`checker.go:5202`-`:5207`): the parent must be
    /// a **global source file**, and the name a string literal that
    /// `IsExternalModuleNameRelative` — `./`, `../` or their backslash spellings. A leading
    /// dot alone is not relative, which is why the predicate is written out.
    /// §1017.
    fn check_ambient_module_relative_name(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        // **`IsGlobalSourceFile` is a *script*, not merely a source file.** A
        // `declare module "./x"` at the top of an external module is an
        // *augmentation* and takes a different arm; reading the predicate as
        // "parent is a SourceFile" reported on all of them — 22 wrong lines and
        // `extraonly` 75 → 76 at §1017's first measurement. §1018.
        let Some(parent) = self.nodes.parent(node) else { return };
        if self.nodes.kind(parent) != SyntaxKind::SourceFile
            || self.source_file_is_an_external_module(parent)
        {
            return;
        }
        let Some(Node::ModuleDeclaration(module)) = self.node_map.get(node) else { return };
        let Some(name) = module.name.and_then(|n| n.node_id()) else { return };
        let Some(Node::StringLiteral(literal)) = self.node_map.get(name) else { return };
        let text = literal.text;
        let relative = ["./", "../", ".\\", "..\\"].iter().any(|prefix| text.starts_with(prefix));
        if !relative {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(name) else { return };
        let span = self.nodes.span(name);
        self.report(
            file,
            Diagnostic::new(
                &messages::AMBIENT_MODULE_DECLARATION_CANNOT_SPECIFY_RELATIVE_MODULE_NAME,
                span,
            ),
        );
    }

    /// Does this source file have any import or export, making it a module
    /// rather than a script? `IsGlobalSourceFile`'s negation. §1018.
    fn source_file_is_an_external_module(&self, file: NodeId) -> bool {
        let Some(Node::SourceFile(source)) = self.node_map.get(file) else { return false };
        source.statements.iter().any(|statement| {
            let Some(id) = statement.node_id() else { return false };
            if matches!(
                self.nodes.kind(id),
                SyntaxKind::ImportDeclaration
                    | SyntaxKind::ExportDeclaration
                    | SyntaxKind::ExportAssignment
            ) {
                return true;
            }
            // **`import x = require("…")` makes a file a module too.**
            // `conflictingDeclarationsImportFromNamespace1` opens with one and
            // then augments `"./index"`; without this the augmentation reads as
            // a top-level ambient declaration. §1018.
            if let Some(Node::ImportEqualsDeclaration(declaration)) = self.node_map.get(id)
                && matches!(
                    declaration.module_reference,
                    Some(tsr_ast::ModuleReference::ExternalModuleReference(_))
                )
            {
                return true;
            }
            self.node_map
                .get(id)
                .and_then(modifiers_of)
                .is_some_and(|modifiers| has_modifier(modifiers, SyntaxKind::ExportKeyword))
        })
    }

    /// TS1235 — `A namespace declaration is only allowed at the top level of a
    /// namespace or module.`
    ///
    /// `checkGrammarModuleElementContext` (`checker.go:5146`). A module
    /// declaration belongs to a source file or a module block; the **only**
    /// other legal parent is another module declaration, which is how
    /// `namespace A.B { }` is spelled. §990.
    ///
    /// The ambient-module variant of the message is chosen upstream when
    /// `isAmbientModule(node)`; it is not built — `diagsole` prices it at zero
    /// cases.
    fn check_grammar_module_element_context(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(parent) = self.nodes.parent(node) else { return };
        if matches!(
            self.nodes.kind(parent),
            SyntaxKind::SourceFile | SyntaxKind::ModuleBlock | SyntaxKind::ModuleDeclaration
        ) {
            return;
        }
        // `declare module "x"` in an illegal context takes a different message,
        // which is not ported.
        if matches!(
            self.node_map.get(node),
            Some(Node::ModuleDeclaration(module))
                if module.name.and_then(|n| n.node_id())
                    .is_some_and(|id| self.nodes.kind(id) == SyntaxKind::StringLiteral)
        ) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        // **`grammarErrorOnNode(node, …)`, not the declaration's name.** Most
        // rules in this file fold to the name (§11), and doing so here put every
        // line ten columns right: `label: namespace M { }` wants column 8, the
        // `namespace` keyword, not column 18. §991.
        let span = self.nodes.span(node);
        self.report(
            file,
            Diagnostic::new(
                &messages::A_NAMESPACE_DECLARATION_IS_ONLY_ALLOWED_AT_THE_TOP_LEVEL_OF_A_NAMESPACE_OR_MODULE,
                span,
            ),
        );
    }

    /// TS2480 — `'let' is not allowed to be used as a name in 'let' or 'const'
    /// declarations.`
    ///
    /// `checkGrammarNameInLetOrConstDeclarations` (`grammarchecks.go:1629`).
    /// `var let = 5` is legal, which is why this is keyed on the declaration
    /// list's flags and not on the name alone. §988.
    fn check_grammar_name_in_let_or_const(&mut self, node: NodeId) {
        if self.file_has_parse_errors || !self.declaration_is_block_scoped_variable(node) {
            return;
        }
        let Some(Node::VariableDeclaration(declaration)) = self.node_map.get(node) else { return };
        let Some(name) = declaration.name else { return };
        self.report_let_named_bindings(name);
    }

    /// Upstream's `else` branch: a binding pattern recurses into its elements.
    fn report_let_named_bindings(&mut self, name: tsr_ast::BindingName<'_>) {
        match name {
            tsr_ast::BindingName::Identifier(identifier) if identifier.text == "let" => {
                let Some(at) = identifier.node_id else { return };
                let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
                let span = self.nodes.span(at);
                self.report(
                    file,
                    Diagnostic::new(
                        &messages::LET_IS_NOT_ALLOWED_TO_BE_USED_AS_A_NAME_IN_LET_OR_CONST_DECLARATIONS,
                        span,
                    ),
                );
            }
            tsr_ast::BindingName::BindingPattern(pattern) => {
                for element in pattern.elements {
                    if let Some(inner) = element.name {
                        self.report_let_named_bindings(inner);
                    }
                }
            }
            tsr_ast::BindingName::Identifier(_) => {}
        }
    }

    /// Is this declaration a `const` or `let` variable? §964.
    fn declaration_is_block_scoped_variable(&self, declaration: NodeId) -> bool {
        if self.nodes.kind(declaration) != SyntaxKind::VariableDeclaration {
            return false;
        }
        self.nodes.ancestors(declaration).any(|ancestor| {
            self.nodes.kind(ancestor) == SyntaxKind::VariableDeclarationList
                && (self.nodes.flags(ancestor).contains(tsr_ast::NodeFlags::CONST)
                    || self.nodes.flags(ancestor).contains(tsr_ast::NodeFlags::LET))
        })
    }

    /// TS1097 — `'{0}' list cannot be empty.`
    ///
    /// `checkGrammarHeritageClause` (`grammarchecks.go:866`). The position is
    /// the **types list's**, not the keyword's — upstream's own comment asks
    /// *"why not error on the token?"* and the baselines agree with the code
    /// rather than the comment. With no `NodeList` here, the list begins where
    /// the keyword ends. §982.
    fn check_grammar_heritage_clause(
        &mut self,
        node: NodeId,
        clause: &tsr_ast::HeritageClause<'_>,
    ) {
        if self.file_has_parse_errors {
            return;
        }
        // `checkGrammarForDisallowedTrailingComma(types, Trailing_comma_not_allowed)`
        // comes first and short-circuits.
        if self.report_disallowed_trailing_comma(node, &messages::TRAILING_COMMA_NOT_ALLOWED) {
            return;
        }
        if !clause.types.is_empty() {
            return;
        }
        let keyword = match clause.token.kind {
            SyntaxKind::ExtendsKeyword => "extends",
            SyntaxKind::ImplementsKeyword => "implements",
            _ => return,
        };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let start = self.nodes.span(node).start + u32::try_from(keyword.len()).unwrap_or(0);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::_0_LIST_CANNOT_BE_EMPTY,
                tsr_core::Span::new(start, start),
                [keyword.to_string()],
            ),
        );
    }

    /// Does this class member carry `static`? §974.
    pub(crate) fn member_is_static(&self, member: NodeId) -> bool {
        let modifiers = match self.node_map.get(member) {
            Some(Node::PropertyDeclaration(n)) => n.modifiers,
            Some(Node::ClassStaticBlockDeclaration(_)) => return true,
            _ => return false,
        };
        tsr_ast::has_syntactic_modifier(modifiers, SyntaxKind::StaticKeyword)
    }

    /// `isBlockScopedNameDeclaredBeforeUse`'s two variable arms
    /// (`checker.go:1937`), for a use positioned at or after the declaration.
    ///
    /// - A **binding element** is still illegal when the use sits in the
    ///   same binding element (`let {[a]: a}`, `const { f = f }`), and
    ///   otherwise asks the question of its `VariableDeclaration`
    ///   (`let [x1] = x1`).
    /// - A **variable declaration** is illegal when the use is inside it
    ///   (`let x = x`) or in the expression of its `for-in`/`for-of`
    ///   (`for (let v of v)`): `isImmediatelyUsedInInitializerOfBlockScopedVariable`.
    fn variable_declared_before_use(&self, declaration: NodeId, usage: NodeId, depth: u32) -> bool {
        if depth > 8 || self.declaration_name_of(declaration) == Some(usage) {
            return true;
        }
        let container = self.enclosing_block_scope_container(declaration);
        match self.nodes.kind(declaration) {
            SyntaxKind::BindingElement => {
                if let Some(error_element) = self
                    .nodes
                    .ancestors(usage)
                    .find(|&a| self.nodes.kind(a) == SyntaxKind::BindingElement)
                {
                    return error_element != declaration
                        || self.nodes.span(declaration).start
                            < self.nodes.span(error_element).start;
                }
                let Some(variable) = self
                    .nodes
                    .ancestors(declaration)
                    .find(|&a| self.nodes.kind(a) == SyntaxKind::VariableDeclaration)
                else {
                    return true;
                };
                if self.nodes.span(variable).start <= self.nodes.span(usage).start {
                    self.variable_declared_before_use(variable, usage, depth + 1)
                } else {
                    !self.use_is_not_deferred(usage, variable)
                }
            }
            SyntaxKind::VariableDeclaration => {
                let Some(grandparent) =
                    self.nodes.parent(declaration).and_then(|list| self.nodes.parent(list))
                else {
                    return true;
                };
                let kind = self.nodes.kind(grandparent);
                if matches!(
                    kind,
                    SyntaxKind::VariableStatement
                        | SyntaxKind::ForStatement
                        | SyntaxKind::ForOfStatement
                ) && self.is_same_scope_descendent_of(usage, Some(declaration), container)
                {
                    return false;
                }
                if matches!(kind, SyntaxKind::ForInStatement | SyntaxKind::ForOfStatement)
                    && let Some(Node::ForInOrOfStatement(statement)) =
                        self.node_map.get(grandparent)
                {
                    let expression = statement.expression.and_then(|e| e.node_id());
                    return !self.is_same_scope_descendent_of(usage, expression, container);
                }
                true
            }
            _ => true,
        }
    }

    /// `isSameScopeDescendentOf` (`checker.go:2090`): `initial` is under
    /// `parent` with no non-IIFE function (or async generator) and not
    /// past `stop_at` in between.
    fn is_same_scope_descendent_of(
        &self,
        initial: NodeId,
        parent: Option<NodeId>,
        stop_at: Option<NodeId>,
    ) -> bool {
        let Some(parent) = parent else { return false };
        let mut n = Some(initial);
        while let Some(current) = n {
            if current == parent {
                return true;
            }
            if Some(current) == stop_at {
                return false;
            }
            if self.is_function_like_or_static_block(current)
                && self.nodes.kind(current) != SyntaxKind::ClassStaticBlockDeclaration
                && (self.immediately_invoked_call(current).is_none()
                    || self.is_async_generator(current))
            {
                return false;
            }
            n = self.nodes.parent(current);
        }
        false
    }

    fn is_async_generator(&self, function: NodeId) -> bool {
        let (modifiers, asterisk) = match self.node_map.get(function) {
            Some(Node::FunctionExpression(f)) => (f.modifiers, f.asterisk_token.is_some()),
            _ => return false,
        };
        asterisk && has_modifier(modifiers, SyntaxKind::AsyncKeyword)
    }

    fn check_used_before_its_declaration(&mut self, node: NodeId, text: &str) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(symbol) = self
            // `getResolvedSymbol`'s meaning, per §251 — `EXPORT_VALUE` included.
            .binder
            .resolve_name(
                self.nodes,
                self.node_map,
                node,
                text,
                SymbolFlags::VALUE | SymbolFlags::EXPORT_VALUE,
            )
            // **`a.C` is not a name in scope.** Upstream reaches this use
            // through `resolveEntityName`, which resolves the receiver and
            // takes the member from its exports; this rule is dispatched per
            // identifier and so must do the same for the right-hand side of a
            // property access. Everything below — the `extends` bound, the
            // same-file test, the ambient test, the `Pos()` comparison — is
            // unchanged. §698.
            .or_else(|| self.qualified_member_of_namespace(node, text))
        else {
            return;
        };
        let symbol = self.binder.merged_symbol(symbol);
        let entry = self.binder.symbols().get(symbol);
        // `checkResolvedBlockScopedVariable` (`checker.go:1888`) picks the
        // message off the symbol's flags and shares everything below.
        let is_class = entry.flags.intersects(SymbolFlags::CLASS);
        // **`export const bar = bar` resolves to a symbol carrying only
        // `EXPORT_VALUE`**, so the selection below fell through and returned.
        // Four of `exportedBlockScopedDeclarations`'s eight lines are that
        // shape. Upstream reaches the local through the export symbol; this
        // asks the declaration instead, which is the same answer for a `const`
        // or `let` and costs no resolution. §964.
        let exported_block_scoped = entry.flags.intersects(SymbolFlags::EXPORT_VALUE)
            && entry.declarations.len() == 1
            && entry
                .declarations
                .first()
                .is_some_and(|&d| self.declaration_is_block_scoped_variable(d));
        let (message, kinds): (_, &[SyntaxKind]) = if entry
            .flags
            .intersects(SymbolFlags::BLOCK_SCOPED_VARIABLE)
            || exported_block_scoped
        {
            (
                &messages::BLOCK_SCOPED_VARIABLE_0_USED_BEFORE_ITS_DECLARATION,
                &[SyntaxKind::VariableDeclaration, SyntaxKind::BindingElement],
            )
        } else if entry.flags.intersects(SymbolFlags::REGULAR_ENUM) {
            // `RegularEnum`, not `Enum` (`checker.go:1908`). A `const enum`
            // is inlined at every use site, so it has no temporal dead zone
            // and upstream says nothing about using one early —
            // `enumUsedBeforeDeclaration` reports on its `Color` and not on
            // its `ConstColor`, and `ENUM` here was §99's one wrong line.
            (&messages::ENUM_0_USED_BEFORE_ITS_DECLARATION, &[SyntaxKind::EnumDeclaration])
        } else if is_class {
            (
                &messages::CLASS_0_USED_BEFORE_ITS_DECLARATION,
                &[SyntaxKind::ClassDeclaration, SyntaxKind::ClassExpression],
            )
        } else {
            return;
        };
        // A symbol with more than one declaration is a MERGE: the arm below
        // picks one declaration by kind and compares *its* position, which for
        // a merge is arbitrary among its members. §96 measured six LOST and this
        // decline removed five of them (§97).
        if !is_class && entry.declarations.len() != 1 {
            return;
        }
        // `core.Find(result.Declarations, IsBlockOrCatchScoped || IsClassLike ||
        // IsEnumDeclaration)` — **not** `first()`. A class merged with a
        // namespace or an interface has several declarations and only the
        // class-like one carries the position upstream compares.
        let Some(declaration) = entry
            .declarations
            .iter()
            .copied()
            .find(|&declaration| kinds.contains(&self.nodes.kind(declaration)))
        else {
            return;
        };
        // `isBlockScopedNameDeclaredBeforeUse`'s class arm (`checker.go:1955`):
        // a use after the class starts is still illegal inside the class's
        // own computed property names and decorators. Upstream asks it before
        // any deferral and returns its answer outright.
        if is_class
            && matches!(
                self.nodes.kind(declaration),
                SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
            )
            && self.nodes.span(declaration).start <= self.nodes.span(node).start
        {
            if !self.is_value_reference(node)
                || self.entity_name_root_is_a_type_query(node)
                || self.source_file_of_for_diagnostics(declaration)
                    != self.source_file_of_for_diagnostics(node)
                || self.declaration_is_in_an_ambient_context(declaration)
                || !self.class_used_in_own_computed_name_or_decorator(node, declaration)
            {
                return;
            }
            let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
            let span = self.error_span(node);
            self.report(file, Diagnostic::with_args(message, span, [text.to_string()]));
            return;
        }
        // §83's class arm keeps its `extends` bound exactly as measured; the
        // arms §96-§99 added carry the deferral predicate instead.
        if !self.is_in_extends_clause(node) && !self.use_is_not_deferred(node, declaration) {
            return;
        }
        // `declarationFile != useFile` returns `true` outright upstream —
        // *"nodes are in different files and order cannot be determined"*.
        if self.source_file_of_for_diagnostics(declaration)
            != self.source_file_of_for_diagnostics(node)
        {
            return;
        }
        // `declaration.Flags&NodeFlagsAmbient == 0`, read of the **declaration**
        // and not of the use. `NodeFlags::AMBIENT` is one of the three this
        // parser never sets, so the question goes to the `declare` modifiers on
        // the declaration and on everything containing it — the gap §81 closed
        // for class members, here for a whole declaration.
        if self.declaration_is_in_an_ambient_context(declaration) {
            return;
        }
        // `declaration.Pos() <= usage.Pos()` is upstream's "declaration is
        // before usage" test, so the report is its negation.
        // **A reference inside the declaration it names is in the temporal dead
        // zone**, whatever the positions say: `const foo = foo` reads `foo`
        // before the initializer finishes. `isBlockScopedNameDeclaredBeforeUse`
        // compares positions for the ordinary case and asks this separately.
        //
        // `const g = () => g` is legal — the read happens after initialization
        // — so a function-like boundary between the two declines. §964.
        // **Variables only.** `class C extends C {}` puts the reference inside
        // the declaration too, and upstream reports TS2506 there — *referenced
        // directly or indirectly in its own base expression* — not TS2449.
        // Letting the arm see classes was §964's first measurement: −3 cases
        // and five new `extraonly` rows, every one of them a TS2449 on a class
        // extending itself. §965.
        let inside_own_initializer = matches!(
            self.nodes.kind(declaration),
            SyntaxKind::VariableDeclaration | SyntaxKind::BindingElement
        ) && self.nodes.span(declaration).start
            <= self.nodes.span(node).start
            && !self.variable_declared_before_use(declaration, node, 0);
        if !inside_own_initializer
            && self.nodes.span(declaration).start <= self.nodes.span(node).start
        {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(file, Diagnostic::with_args(message, span, [text.to_string()]));
    }

    /// The member a property access names, when its receiver resolves to a
    /// namespace — `resolveEntityName`'s second half, for a rule dispatched one
    /// identifier at a time.
    ///
    /// The receiver may be an alias (`export import a = A`), so the chain is
    /// followed with the same hops [`Checker::alias_chain_carries`] walks: a
    /// declined **qualified** module reference goes through
    /// [`Checker::qualified_alias_target`] (§686), and an unresolvable one
    /// answers `None`. §698.
    fn qualified_member_of_namespace(
        &mut self,
        node: NodeId,
        text: &str,
    ) -> Option<tsr_binder::SymbolId> {
        let parent = self.nodes.parent(node)?;
        let Some(Node::PropertyAccessExpression(access)) = self.node_map.get(parent) else {
            return None;
        };
        if access.name.and_then(|name| name.node_id()) != Some(node) {
            return None;
        }
        let receiver = access.expression.and_then(|expression| expression.node_id())?;
        // **A receiver may itself be a chain.** `Box2D.Collision.Shapes.b2Shape`
        // resolves `Box2D` in scope and then walks `Collision` and `Shapes`
        // through each namespace's exports. §698 handled a single-identifier
        // receiver, which is every shape TS2449's fixtures had; TS2506's have
        // three segments. §747.
        let mut at =
            if let Some(Node::PropertyAccessExpression(inner)) = self.node_map.get(receiver) {
                let member = inner.name.and_then(|name| name.node_id())?;
                let member_text = self.identifier_text(member).map(str::to_string)?;
                self.qualified_member_of_namespace(member, &member_text)
            } else {
                let receiver_text = self.identifier_text(receiver).map(str::to_string)?;
                self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    receiver,
                    &receiver_text,
                    SymbolFlags::NAMESPACE | SymbolFlags::ALIAS,
                )
            };
        for _ in 0..MAX_ALIAS_HOPS {
            let current = at?;
            let current = self.binder.merged_symbol(current);
            if !self.binder.symbols().get(current).flags.intersects(SymbolFlags::ALIAS) {
                at = Some(current);
                break;
            }
            at = self.resolve_alias(current).or_else(|| self.qualified_alias_target(current));
        }
        let namespace = self.binder.merged_symbol(at?);
        self.binder.symbols().get(namespace).exports.get(text).copied()
    }

    /// `isBlockScopedNameDeclaredBeforeUse` (`checker.go:1922`) for a value
    /// use: a declaration in another file answers `true`; otherwise the
    /// variable arms ([`Checker::variable_declared_before_use`]) when the
    /// declaration comes first and the deferral arms
    /// ([`Checker::use_is_not_deferred`]) when it comes after — the two halves
    /// the TS2448 report already reads.
    pub(crate) fn value_use_declared_before_use(&self, declaration: NodeId, usage: NodeId) -> bool {
        if self.source_file_of_for_diagnostics(declaration)
            != self.source_file_of_for_diagnostics(usage)
        {
            return true;
        }
        if self.nodes.span(declaration).start <= self.nodes.span(usage).start {
            return !matches!(
                self.nodes.kind(declaration),
                SyntaxKind::VariableDeclaration | SyntaxKind::BindingElement
            ) || self.variable_declared_before_use(declaration, usage, 0);
        }
        !self.use_is_not_deferred(usage, declaration)
    }

    /// `isBlockScopedNameDeclaredBeforeUse`'s deferral arms (`checker.go:1922`),
    /// which §96 measured are the *majority* of that function rather than its
    /// edge cases — 176 wrong lines when they were approximated.
    ///
    /// 1. **A use in a type context is deferred regardless of position**
    ///    (`checker.go:1932`, `isInAmbientOrTypeNode` at `:11238`) — the arm
    ///    whose absence was most of §96's 176 wrong lines.
    /// 2. **An export specifier or `export =`** makes the name available
    ///    without using it (`checker.go:1993`).
    /// 3. **`isUsedInFunctionOrInstanceProperty`** (`checker.go:2011`): a
    ///    function-like ancestor defers, but the walk **quits at the
    ///    declaration's own block-scope container**, so a use and a declaration
    ///    inside one function are still compared by position.
    fn use_is_not_deferred(&self, node: NodeId, declaration: NodeId) -> bool {
        if !self.is_value_reference(node) || self.entity_name_root_is_a_type_query(node) {
            return false;
        }
        if let Some(parent) = self.nodes.parent(node)
            && matches!(
                self.nodes.kind(parent),
                SyntaxKind::ExportSpecifier | SyntaxKind::ExportAssignment
            )
        {
            return false;
        }
        // `isInAmbientOrTypeNode(usage)` (`checker.go:1932`): a use inside an
        // ambient declaration — `declare class C { [k]: number }` before
        // `const k` — is never evaluated.
        if self.declaration_is_in_an_ambient_context(node) {
            return false;
        }
        // **Not upstream's rule; a decline for a resolver gap.** From a
        // parameter, a name declared in the function's own body resolves to
        // the *outer* scope when the target needs no scope change
        // (`useOuterVariableScopeInParameter`, `checker.go`, in
        // `resolveNameHelper`); this port's binder resolves the body's
        // declaration, so the comparison below would be about the wrong
        // symbol (`parameterInitializersForwardReferencing1_es6`'s
        // `function f7({[foo]: bar}) { let foo }`). Silence until the
        // resolver is fixed — reported to the names lane.
        if self.use_in_parameter_of_declaring_function(node, declaration) {
            return false;
        }
        let container = self.enclosing_block_scope_container(declaration);
        !self.is_used_in_function_or_instance_property(node, declaration, container)
    }

    /// Is `usage` under a parameter of the function whose body holds
    /// `declaration`?
    fn use_in_parameter_of_declaring_function(&self, usage: NodeId, declaration: NodeId) -> bool {
        let mut last = usage;
        for ancestor in self.nodes.ancestors(usage) {
            if self.is_function_like_or_static_block(ancestor) {
                return self.nodes.kind(last) == SyntaxKind::Parameter
                    && self.nodes.ancestors(declaration).any(|a| a == ancestor);
            }
            last = ancestor;
        }
        false
    }

    /// `isUsedInFunctionOrInstanceProperty` (`checker.go:2011`), for the
    /// declarations this rule asks about — block-scoped variables, classes and
    /// regular enums, none of which is a property or method declaration, so
    /// the static-initializer arm never finds and the instance-initializer arm
    /// always does.
    ///
    /// Replaces a kind list that deferred at **every** property declaration
    /// and decorator: upstream defers only a property's *initializer*, only
    /// when it is an instance one (`static p = After.x` before `class After`
    /// is TS2449, `scopeCheckStaticInitializer`), and only a decorator whose
    /// decorated member is itself deferred. A function-like ancestor defers
    /// unless it is an IIFE, which runs where it stands.
    fn is_used_in_function_or_instance_property(
        &self,
        usage: NodeId,
        declaration: NodeId,
        container: Option<NodeId>,
    ) -> bool {
        let mut current = usage;
        for _ in 0..1024 {
            if Some(current) == container {
                return false;
            }
            let kind = self.nodes.kind(current);
            if kind == SyntaxKind::ClassStaticBlockDeclaration {
                if self.nodes.span(declaration).start < self.nodes.span(usage).start {
                    return true;
                }
            } else if self.is_function_like_or_static_block(current)
                && self.immediately_invoked_call(current).is_none()
            {
                return true;
            }
            // Type positions are value-free: an interface, alias or type
            // literal never evaluates the name. `use_is_not_deferred`'s
            // caller has already asked `is_value_reference`; this keeps the
            // walk from treating a type member's signature as a function.
            if matches!(
                kind,
                SyntaxKind::InterfaceDeclaration
                    | SyntaxKind::TypeAliasDeclaration
                    | SyntaxKind::TypeLiteral
            ) {
                return true;
            }
            let Some(parent) = self.nodes.parent(current) else { return false };
            if let Some(Node::PropertyDeclaration(property)) = self.node_map.get(parent)
                && property.initializer.and_then(|i| i.node_id()) == Some(current)
                && !self.member_is_static(parent)
            {
                return true;
            }
            if let Some(Node::Decorator(decorator)) = self.node_map.get(parent)
                && decorator.expression.and_then(|e| e.node_id()) == Some(current)
                && let Some(decorated) = self.nodes.parent(parent)
            {
                let member = match self.nodes.kind(decorated) {
                    SyntaxKind::Parameter => {
                        self.nodes.parent(decorated).and_then(|f| self.nodes.parent(f))
                    }
                    SyntaxKind::MethodDeclaration => self.nodes.parent(decorated),
                    _ => None,
                };
                if let Some(member) = member {
                    return self.is_used_in_function_or_instance_property(
                        member,
                        declaration,
                        container,
                    );
                }
            }
            current = parent;
        }
        false
    }

    /// The class arm of `isBlockScopedNameDeclaredBeforeUse`
    /// (`checker.go:1955`), for a use positioned **after** the class
    /// declaration starts: still illegal inside one of the class's own
    /// computed property names, or (without `experimentalDecorators`) inside
    /// a decorator on the class, its members or their parameters — unless a
    /// non-IIFE function between the use and that decorator defers it.
    fn class_used_in_own_computed_name_or_decorator(
        &self,
        usage: NodeId,
        declaration: NodeId,
    ) -> bool {
        let grandparent = |id: NodeId| self.nodes.parent(id).and_then(|p| self.nodes.parent(p));
        let mut container = Some(usage);
        while let Some(id) = container {
            if id == declaration {
                return false;
            }
            let kind = self.nodes.kind(id);
            if kind == SyntaxKind::ComputedPropertyName && grandparent(id) == Some(declaration) {
                break;
            }
            if !self.legacy_decorators
                && kind == SyntaxKind::Decorator
                && let Some(parent) = self.nodes.parent(id)
            {
                let on_class = parent == declaration;
                let on_member = matches!(
                    self.nodes.kind(parent),
                    SyntaxKind::MethodDeclaration
                        | SyntaxKind::GetAccessor
                        | SyntaxKind::SetAccessor
                        | SyntaxKind::PropertyDeclaration
                ) && self.nodes.parent(parent) == Some(declaration);
                let on_parameter = self.nodes.kind(parent) == SyntaxKind::Parameter
                    && grandparent(parent) == Some(declaration);
                if on_class || on_member || on_parameter {
                    break;
                }
            }
            container = self.nodes.parent(id);
        }
        let Some(container) = container else { return false };
        if !self.legacy_decorators && self.nodes.kind(container) == SyntaxKind::Decorator {
            // Legal when a non-IIFE function sits between the use and the
            // decorator; upstream's `n != nil && n != container` is "found
            // one", which makes the use legal, so illegal is its negation.
            let mut n = usage;
            while n != container {
                if self.is_function_like_or_static_block(n)
                    && self.nodes.kind(n) != SyntaxKind::ClassStaticBlockDeclaration
                    && self.immediately_invoked_call(n).is_none()
                {
                    return false;
                }
                let Some(parent) = self.nodes.parent(n) else { return true };
                n = parent;
            }
            return true;
        }
        true
    }

    /// `GetEnclosingBlockScopeContainer` (`ast/utilities.go:2171`) over
    /// `IsBlockScope` (`:2177`). A `Block` is a block scope **unless** its
    /// parent is function-like — a function body is the function's own scope.
    fn enclosing_block_scope_container(&self, node: NodeId) -> Option<NodeId> {
        self.nodes.ancestors(node).find(|&ancestor| {
            let kind = self.nodes.kind(ancestor);
            if kind == SyntaxKind::Block {
                return !self.nodes.parent(ancestor).is_some_and(|parent| {
                    matches!(
                        self.nodes.kind(parent),
                        SyntaxKind::FunctionDeclaration
                            | SyntaxKind::FunctionExpression
                            | SyntaxKind::ArrowFunction
                            | SyntaxKind::MethodDeclaration
                            | SyntaxKind::GetAccessor
                            | SyntaxKind::SetAccessor
                            | SyntaxKind::Constructor
                            | SyntaxKind::ClassStaticBlockDeclaration
                    )
                });
            }
            matches!(
                kind,
                SyntaxKind::SourceFile
                    | SyntaxKind::CaseBlock
                    | SyntaxKind::CatchClause
                    | SyntaxKind::ModuleDeclaration
                    | SyntaxKind::ForStatement
                    | SyntaxKind::ForInStatement
                    | SyntaxKind::ForOfStatement
                    | SyntaxKind::Constructor
                    | SyntaxKind::MethodDeclaration
                    | SyntaxKind::GetAccessor
                    | SyntaxKind::SetAccessor
                    | SyntaxKind::FunctionDeclaration
                    | SyntaxKind::FunctionExpression
                    | SyntaxKind::ArrowFunction
                    | SyntaxKind::PropertyDeclaration
                    | SyntaxKind::ClassStaticBlockDeclaration
            )
        })
    }

    /// Is this node the expression of an `extends` heritage clause?
    ///
    /// The `implements`-versus-`extends` distinction is the same one
    /// [`Checker::is_value_reference`] draws, and for the same reason: only
    /// `extends` resolves its name as a *value*.
    fn is_in_extends_clause(&self, node: NodeId) -> bool {
        // **`class D extends a.C` puts the identifier under a property
        // access.** The heritage expression is the whole `a.C`, so the
        // qualified form is climbed to it before the clause is asked — the
        // position argument in §83's rustdoc (*"`extends` evaluates its
        // operand at class-definition time, immediately"*) is about the clause
        // and holds however the operand is spelled. §698.
        let mut node = node;
        while let Some(parent) = self.nodes.parent(node) {
            let Some(Node::PropertyAccessExpression(access)) = self.node_map.get(parent) else {
                break;
            };
            if access.name.and_then(|name| name.node_id()) != Some(node) {
                break;
            }
            node = parent;
        }
        let Some(parent) = self.nodes.parent(node) else { return false };
        let Some(Node::ExpressionWithTypeArguments(with_arguments)) = self.node_map.get(parent)
        else {
            return false;
        };
        if with_arguments.expression.and_then(|e| e.node_id()) != Some(node) {
            return false;
        }
        self.nodes.parent(parent).is_some_and(|clause| {
            matches!(
                self.node_map.get(clause),
                Some(Node::HeritageClause(heritage))
                    if heritage.token.kind == SyntaxKind::ExtendsKeyword
            )
        })
    }

    /// Is this declaration in an ambient **context** — its own `declare`, any
    /// containing one, or a declaration file?
    ///
    /// [`Checker::declaration_is_ambient`] answers only the first half, which
    /// is all its callers need. `NodeFlagsAmbient` is upstream's *transitive*
    /// answer (the parser sets it under `declare` and throughout a `.d.ts`)
    /// and this parser never sets it (see [`tsr_ast::NodeFlags::AMBIENT`],
    /// declared and written by nothing), so a rule reading the flag of a
    /// declaration rather than of a use has to walk.
    pub(crate) fn declaration_is_in_an_ambient_context(&self, declaration: NodeId) -> bool {
        std::iter::once(declaration).chain(self.nodes.ancestors(declaration)).any(|at| {
            // Every kind that can carry `declare`, not just the two §83 needed.
            // A `declare const o` puts the modifier on the enclosing
            // **VariableStatement**, so a rule handed the `VariableDeclaration`
            // sees no modifier at all — which is the whole of why
            // `controlFlowNullishCoalesce` broke under §96-§98 (`checker-notes-diag2.md`
            // §99). `NodeFlags::AMBIENT` would answer this in one read and is
            // one of the three flags this parser never sets.
            match self.node_map.get(at) {
                Some(Node::SourceFile(_)) => {
                    self.module_host.is_some_and(|host| host.is_declaration_file(at))
                }
                Some(Node::ClassDeclaration(n)) => {
                    has_modifier(n.modifiers, SyntaxKind::DeclareKeyword)
                }
                Some(Node::ModuleDeclaration(n)) => {
                    has_modifier(n.modifiers, SyntaxKind::DeclareKeyword)
                }
                Some(Node::VariableStatement(n)) => {
                    has_modifier(n.modifiers, SyntaxKind::DeclareKeyword)
                }
                Some(Node::FunctionDeclaration(n)) => {
                    has_modifier(n.modifiers, SyntaxKind::DeclareKeyword)
                }
                Some(Node::EnumDeclaration(n)) => {
                    has_modifier(n.modifiers, SyntaxKind::DeclareKeyword)
                }
                _ => false,
            }
        })
    }

    /// Does this reference hit `checkIdentifier`'s uninitialized-variable arm?
    ///
    /// `checkIdentifier` (`checker.go:11189-11192`), the arm reached when
    /// `assumeInitialized` is false and the *flow* type carries `undefined`
    /// while the declared type does not. Upstream does **two** things there from
    /// this one condition:
    ///
    /// ```go
    /// c.error(node, diagnostics.Variable_0_is_used_before_being_assigned, ...)
    /// // Return the declared type to reduce follow-on errors
    /// return t
    /// ```
    ///
    /// This port had only the diagnostic. The discarded narrowing is the whole
    /// of §839 in `docs/architecture/checker-notes-deferred.md`: it is why
    /// upstream answers `string | number` for `strOrNum` in the **else** branch
    /// of `typeof strOrNum === "string"` while answering `string` in the true
    /// branch — the else branch keeps `undefined` (`typeof undefined` is not
    /// `"string"`), so the arm fires and the declared type replaces the
    /// narrowing. The two consumers are
    /// [`Checker::check_used_before_assigned`] and the identifier type road in
    /// `crate::expressions`.
    ///
    /// # This is a bound, and the bound is `assumeInitialized`
    ///
    /// Upstream's `assumeInitialized` (`checker.go:11150`) is a nine-way
    /// disjunction, and every disjunct that is false is a diagnostic. Rather
    /// than port the ones that need machinery this port lacks —
    /// `isSymbolAssignedDefinitely` needs `markNodeAssignments`,
    /// `isPastLastAssignment` needs assignment positions — the rule **requires
    /// the shape where those disjuncts cannot matter**:
    ///
    /// - the symbol's declaration is a plain `VariableDeclaration` with a type
    ///   annotation, so `isParameter`, `isAlias`, `isSameScopedBindingElement`
    ///   and the auto-typed path are all excluded by construction;
    /// - the reference's control-flow container **is** the declaration's, so
    ///   `isOuterVariable` is false and `isNeverInitialized` — the only consumer
    ///   of `isSymbolAssignedDefinitely` — is never consulted;
    /// - the reference is not a definite assignment target, which
    ///   `checker.go:11109` returns early for.
    ///
    /// The remaining disjuncts are syntactic and are ported: a `!` on the
    /// declaration, an ambient declaration, `typeof x`, an ambient-or-type-node
    /// position, an `ExportSpecifier` parent, a `NonNullExpression` parent.
    ///
    /// What the bound gives up is every `let x: T` referenced from inside a
    /// nested function — measured rather than assumed, in
    /// `docs/architecture/checker-notes-diag2.md` §8.
    pub(crate) fn uninitialized_variable_reads_declared(
        &mut self,
        node: NodeId,
        text: &str,
    ) -> bool {
        if self.file_has_parse_errors || !self.strict_null_checks || !self.is_value_reference(node)
        {
            return false;
        }
        // §839.1's guard — decline a reference under a condition naming it
        // that used a predicate call, `instanceof` or `.constructor` — is
        // gone: each of those narrowings is ported in `crate::flow`, and
        // removing it lost no case (`docs/parity/notes/flow.md` §11).
        // `assignmentKind == AssignmentKindDefinite` returns before the flow
        // section (`checker.go:11109`), so `x = 1` never reports even though the
        // flow type at `x` carries `undefined`. `getAssignmentTargetKind`
        // climbs every destructuring spelling — `[x] = arr`, `({ x } = obj)`,
        // `[...[x]] = arr`, `({ ...x } = obj)`, a for-of head — so each of
        // those targets is a definite assignment too.
        if self.assignment_target_kind(node) == crate::expressions::AssignmentTargetKind::Definite {
            return false;
        }
        if self.is_inside_with_statement(node) || self.is_in_type_query_or_type_node(node) {
            return false;
        }
        let Some(parent) = self.nodes.parent(node) else { return false };
        if matches!(
            self.nodes.kind(parent),
            SyntaxKind::ExportSpecifier | SyntaxKind::NonNullExpression
        ) {
            return false;
        }
        let Some(symbol) =
            // `getResolvedSymbol`'s meaning, per §251 — `EXPORT_VALUE` included.
            self.binder.resolve_name(
                self.nodes,
                self.node_map,
                node,
                text,
                SymbolFlags::VALUE | SymbolFlags::EXPORT_VALUE,
            )
        else {
            return false;
        };
        // `localOrExportSymbol.ValueDeclaration`; only variables are narrowed
        // (`checker.go:11108`), and a parameter (`GetRootDeclaration` is a
        // `Parameter`) is assumed initialized.
        let symbol_data = self.binder.symbols().get(symbol);
        let Some(declaration) =
            symbol_data.value_declaration.or_else(|| symbol_data.declarations.first().copied())
        else {
            return false;
        };
        // A binding element is checked as its root `VariableDeclaration`
        // (`GetRootDeclaration`); a parameter root is `isParameter`, assumed
        // initialized. The element itself is never `isNeverInitialized`
        // (that wants a `VariableDeclaration`) and carries no `!`.
        let is_binding_element = self.nodes.kind(declaration) == SyntaxKind::BindingElement;
        let mut root = declaration;
        while self.nodes.kind(root) == SyntaxKind::BindingElement {
            let Some(pattern) = self.nodes.parent(root) else { return false };
            let Some(owner) = self.nodes.parent(pattern) else { return false };
            root = owner;
        }
        let Some(Node::VariableDeclaration(variable)) = self.node_map.get(root) else {
            return false;
        };
        // `isSameScopedBindingElement` (`checker.go:11202`): a read inside a
        // binding element of the same root declaration (`const { a, b = a }`).
        if is_binding_element
            && let Some(element) = self
                .nodes
                .ancestors(node)
                .find(|&id| self.nodes.kind(id) == SyntaxKind::BindingElement)
        {
            let mut element_root = element;
            while self.nodes.kind(element_root) == SyntaxKind::BindingElement {
                let Some(owner) =
                    self.nodes.parent(element_root).and_then(|pattern| self.nodes.parent(pattern))
                else {
                    break;
                };
                element_root = owner;
            }
            if element_root == root {
                return false;
            }
        }
        // No `!`, and an annotation or an initialiser — neither is upstream's
        // auto-typed path (`t == autoType`, a different diagnostic entirely).
        // An initialised declaration still reports when some path reaches the
        // read without passing it (a `catch` after a throwing initialiser,
        // `controlFlowDestructuringVariablesInTryCatch`).
        let Some(list) = self.nodes.parent(root) else { return false };
        // A catch-clause variable is typed `any`, `unknown` or `errorType`
        // whatever its annotation (`getTypeForVariableLikeDeclaration`,
        // `checker.go:16678`), each of which `assumeInitialized` accepts.
        if !is_binding_element && self.nodes.kind(list) == SyntaxKind::CatchClause {
            return false;
        }
        // A `for (… of/in …)` head is assigned by the loop, not auto-typed.
        let for_head = self.nodes.parent(list).filter(|&owner| {
            matches!(
                self.nodes.kind(owner),
                SyntaxKind::ForInStatement | SyntaxKind::ForOfStatement
            )
        });
        if variable.exclamation_token.is_some()
            || (variable.r#type.is_none() && variable.initializer.is_none() && for_head.is_none())
        {
            return false;
        }
        // A `const` with no initialiser only occurs in an ambient context or
        // after a grammar error (TS1155), and upstream reaches neither: the
        // ambient flag short-circuits `assumeInitialized`
        // (`checker.go:11158`). `declare const b: B` supplied **227 of the
        // first measurement's 4,781 wrong lines from one case**
        // (`compiler/genericDefaults`), which is what put both tests here.
        if variable.initializer.is_none()
            && for_head.is_none()
            && self.nodes.flags(list).intersects(tsr_ast::NodeFlags::CONST)
        {
            return false;
        }
        // Exact fast path, not a decline: an initialised block-scoped
        // declaration read *after* it in its own block is assigned on every
        // path to the read — a block's statements run in order, loops re-enter
        // past the head, exceptions leave the scope — so the walk below could
        // only answer "no `undefined`". The one way to enter a block past a
        // statement is a `switch` jumping to a later `case`, which keeps the
        // walk. Measured: walking every such read cost ~5% CPU on
        // `domain-model` (`docs/parity/notes/flow.md` §13). A block-scoped
        // `for..in`/`for..of` head is assigned before its body runs, so the
        // same holds past the iterated expression.
        let assigned_by = if variable.initializer.is_some() {
            Some(self.nodes.span(root).end)
        } else {
            for_head.and_then(|statement| match self.node_map.get(statement) {
                Some(Node::ForInOrOfStatement(head)) => head
                    .expression
                    .and_then(|expression| expression.node_id())
                    .map(|expression| self.nodes.span(expression).end),
                _ => None,
            })
        };
        if let Some(assigned_by) = assigned_by
            && self.nodes.flags(list).intersects(tsr_ast::NodeFlags::BLOCK_SCOPED)
            && self.nodes.span(node).start >= assigned_by
            && !self
                .nodes
                .parent(list)
                .and_then(|statement| self.nodes.parent(statement))
                .is_some_and(|block| {
                    matches!(
                        self.nodes.kind(block),
                        SyntaxKind::CaseClause | SyntaxKind::DefaultClause
                    )
                })
        {
            return false;
        }
        // `declaration.Flags&NodeFlagsAmbient != 0` (`checker.go:11158`): a
        // `declare` on the statement or any container — `var a` inside
        // `declare module "a"` — or a declaration file.
        if self.declaration_is_in_an_ambient_context(declaration) {
            return false;
        }
        // `isOuterVariable` (`checker.go:11128`) is a disjunct of
        // `assumeInitialized` **only when the variable is not never-initialized**:
        // `(isOuterVariable && !isNeverInitialized)` (`checker.go:11152`). A
        // `let x: T;` that no assignment anywhere targets is reported even from
        // inside a nested function.
        //
        // `isNeverInitialized` (`checker.go:11147`) is a `VariableDeclaration`,
        // not a `for-in`/`for-of` head, with no initializer and no `!` (the
        // binding-element, initializer and head tests below; `!` exited above)
        // — that
        // `isMutableLocalVariableDeclaration` accepts and
        // `isSymbolAssignedDefinitely` does not.
        //
        // The definite-assignment half is a **per-symbol** record written by
        // `mark_node_assignments`, not a scan for the name: a syntactic
        // "no `x` is written anywhere in this file" measured 4 lost cases and
        // was reverted (`checker-notes-diag2.md` §42).
        let is_outer_variable =
            self.control_flow_container(node) != self.control_flow_container(declaration);
        if is_outer_variable {
            let is_never_initialized = !is_binding_element
                && variable.initializer.is_none()
                && for_head.is_none()
                && self.is_mutable_local_variable_declaration(declaration)
                && !self.is_symbol_assigned_definitely(symbol);
            if !is_never_initialized {
                return false;
            }
        }
        // An annotation is read directly rather than through
        // `get_type_of_symbol`, because a union of *named* types declares
        // `errorType` on the printing road and this rule prints no type —
        // `checker-notes-diag2.md` §76, which is §42.1 one level up. Every
        // other annotation shape answers identically through both.
        let declared = match variable.r#type {
            Some(annotation) if !is_binding_element => {
                self.get_type_from_type_node_unprinted(annotation)
            }
            _ => self.get_type_of_symbol(symbol),
        };
        if self.is_error(declared)
            || self.type_of(declared).flags.intersects(
                crate::flags::TypeFlags::ANY
                    .union(crate::flags::TypeFlags::UNKNOWN)
                    .union(crate::flags::TypeFlags::VOID),
            )
            || self.contains_undefined_type(declared)
        {
            return false;
        }
        let initial = self.get_optional_type_unprinted(declared);
        // §839.2: the flow walk must be given the symbol `checkIdentifier`'s
        // own `getResolvedSymbol` would produce — `SymbolFlags::VALUE` — and
        // **not** the `VALUE | EXPORT_VALUE` symbol resolved above for the
        // structural half.
        //
        // Upstream has no choice to make here: `getFlowTypeOfReferenceEx`
        // (`checker.go:11174`) is passed the reference *node* and matches by
        // `isMatchingReference`, never by symbol identity. The symbol parameter
        // is this port's own, and handing it the **export** symbol of an
        // `export var` makes every match fail, so the walk runs to the top of
        // the graph and returns the initial type unnarrowed — `undefined`
        // intact, this rule firing on every reference including the ones
        // upstream narrows.
        //
        // Measured, not reasoned: a `TSR_DEBUG_839` probe over
        // `typeGuardsInModule` printed, for the two `export var`s and for them
        // only, `export_value=true flow=string | number | undefined` where the
        // two plain `var`s in the same fixture printed `flow=string` in the
        // `then` branch. Upstream's `.errors.txt` for that case reports TS2454
        // on `var3` in the **`else`** branch only, which is what says upstream
        // narrows an exported variable like any other — `isModuleExports`
        // (`checker.go:11132`) is the `module.exports` symbol of a CommonJS
        // file, not an `export var`, and gating on this port's `EXPORT_VALUE`
        // would have been a compensation for a bug rather than a port of a rule.
        let flow_symbol = self
            .binder
            .resolve_name(self.nodes, self.node_map, node, text, SymbolFlags::VALUE)
            .unwrap_or(symbol);
        let flow =
            self.get_flow_type_of_reference_ex(node, Some(flow_symbol), declared, Some(initial));
        if flow == self.intrinsics.error || !self.contains_undefined_type(flow) {
            return false;
        }
        true
    }

    /// TS2454 — `Variable '{0}' is used before being assigned.`
    ///
    /// The *reporting* half only. The condition itself lives in
    /// [`Checker::uninitialized_variable_reads_declared`], because
    /// `checkIdentifier` uses one condition for two answers and this port had
    /// split them — see `docs/architecture/checker-notes-deferred.md` §839.
    fn check_used_before_assigned(&mut self, node: NodeId, text: &str) {
        if !self.uninitialized_variable_reads_declared(node, text) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::VARIABLE_0_IS_USED_BEFORE_BEING_ASSIGNED,
                span,
                [text.to_string()],
            ),
        );
    }

    /// TS2428 — `All declarations of '{0}' must have identical type
    /// parameters.`
    ///
    /// `checkTypeParameterListsIdentical` (`checker.go:4416`): a symbol with
    /// more than one class-or-interface declaration whose type parameter lists
    /// disagree is reported **on every one of them**, which is why this fires
    /// at each declaration the walk visits rather than once at the symbol.
    ///
    /// `areTypeParametersIdentical` permits omitted augmentation metadata and
    /// trailing optional parameters. Constraint/default equality still uses
    /// the shallow written signatures below rather than native resolved-type
    /// identity; this remains a separate fidelity limit. §140, §706.
    fn check_type_parameter_lists_identical(&mut self, node: NodeId) {
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        let entry = self.binder.symbols().get(self.binder.merged_symbol(symbol));
        let declarations: Vec<NodeId> = entry
            .declarations
            .iter()
            .copied()
            .filter(|&declaration| {
                matches!(
                    self.nodes.kind(declaration),
                    SyntaxKind::InterfaceDeclaration | SyntaxKind::ClassDeclaration
                )
            })
            .collect();
        if declarations.len() <= 1 {
            return;
        }
        // `areTypeParametersIdentical` (`checker.go:4340`) compares the
        // **name, the constraint and the default** of each parameter, not the
        // name alone: `class Foo<T extends Function>` and `interface Foo<T
        // extends Different>` agree on every name.
        //
        // Upstream compares constraints as types. This port has no source-text
        // printer reachable from the checker (§684), so the comparison is
        // structural and shallow — a written type reference contributes its
        // entity name, a keyword type its kind. Narrower in one direction only:
        // two spellings of one type read as different, so the risk is a wrong
        // line rather than a missing one. §706.
        let signature = |this: &Self, node: Option<NodeId>| -> String {
            let Some(node) = node else { return String::new() };
            match this.node_map.get(node) {
                Some(Node::TypeReferenceNode(reference)) => reference
                    .type_name
                    .and_then(|name| name.node_id())
                    .and_then(|id| this.identifier_text(id))
                    .map_or_else(|| format!("{:?}", this.nodes.kind(node)), str::to_string),
                _ => format!("{:?}", this.nodes.kind(node)),
            }
        };
        let parameters = |declaration: NodeId| -> Vec<(String, String, String)> {
            self.node_map.get(declaration).map_or_else(Vec::new, |typed| {
                type_parameters_of(typed)
                    .iter()
                    .map(|parameter| {
                        let name =
                            parameter.name.map_or_else(String::new, |name| name.text.to_string());
                        let constraint =
                            signature(self, parameter.constraint.and_then(|c| c.node_id()));
                        let default =
                            signature(self, parameter.default_type.and_then(|d| d.node_id()));
                        (name, constraint, default)
                    })
                    .collect()
            })
        };
        let lists: Vec<_> =
            declarations.iter().map(|&declaration| parameters(declaration)).collect();
        let maximum = lists.iter().map(Vec::len).max().unwrap_or_default();
        // Native compares each source list to merged parameter metadata, not
        // to the first written list. Find the first supplied constraint/default
        // at each position; an omitted first one must not mask later conflicts.
        // This adds no semantic cache, mapper or cross-checker type identity.
        let mut minimum = 0;
        let mut targets = Vec::with_capacity(maximum);
        for position in 0..maximum {
            let at_position = || lists.iter().filter_map(|list| list.get(position));
            let name = &at_position().next().expect("position below maximum").0;
            let constraint = at_position()
                .find(|parameter| !parameter.1.is_empty())
                .map(|parameter| &parameter.1);
            let default = at_position()
                .find(|parameter| !parameter.2.is_empty())
                .map(|parameter| &parameter.2);
            if default.is_none() {
                minimum = position + 1;
            }
            targets.push((name, constraint, default));
        }
        let identical = lists.iter().all(|list| {
            list.len() >= minimum
                && list.iter().zip(&targets).all(
                    |(
                        (name, constraint, default),
                        (target_name, target_constraint, target_default),
                    )| {
                        name == *target_name
                            && (constraint.is_empty()
                                || target_constraint.is_none_or(|target| constraint == target))
                            && (default.is_empty()
                                || target_default.is_none_or(|target| default == target))
                    },
                )
        });
        if identical {
            return;
        }
        let Some(name_id) = self.declaration_name_of(node) else { return };
        let Some(file) = self.source_file_of_for_diagnostics(name_id) else { return };
        let span = self.error_span(name_id);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::ALL_DECLARATIONS_OF_0_MUST_HAVE_IDENTICAL_TYPE_PARAMETERS,
                span,
                [entry.name.to_string()],
            ),
        );
    }

    /// TS2300 — `Duplicate identifier '{0}'.`, for a type parameter list.
    ///
    /// `checkTypeParameters` (`checker.go:7002`), reduced to its duplicate scan.
    /// Upstream calls it from `checkSignatureDeclaration` (`checker.go:2742`),
    /// `checkClassLikeDeclaration` (`:4297`), `checkInterfaceDeclaration`
    /// (`:4998`) and `checkTypeAliasDeclaration` (`:6887`); this port reaches
    /// the same set through [`type_parameters_of`], which is every node kind
    /// that carries such a list.
    ///
    /// The test is **symbol identity**, not name equality — the binder has
    /// already merged two same-named parameters of one list into one symbol, so
    /// identity is what distinguishes a genuine duplicate from two lists that
    /// happen to spell a parameter the same way. See
    /// `checker-notes-diag2.md` §88.
    ///
    /// The report lands on the **later** declaration only (`for j := range i`).
    fn check_type_parameter_list(&mut self, parameters: &[&tsr_ast::TypeParameterDeclaration<'_>]) {
        for (index, parameter) in parameters.iter().enumerate() {
            let Some(id) = parameter.node_id else { continue };
            let Some(symbol) = self.binder.symbol_of(id) else { continue };
            let duplicate = parameters[..index].iter().any(|earlier| {
                earlier
                    .node_id
                    .and_then(|earlier| self.binder.symbol_of(earlier))
                    .is_some_and(|earlier| earlier == symbol)
            });
            if !duplicate {
                continue;
            }
            let Some(name) = parameter.name else { continue };
            let Some(name_id) = name.node_id else { continue };
            let Some(file) = self.source_file_of_for_diagnostics(name_id) else { continue };
            let span = self.error_span(name_id);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::DUPLICATE_IDENTIFIER_0,
                    span,
                    [name.text.to_string()],
                ),
            );
        }
    }

    /// TS1029 — `'{0}' modifier must precede '{1}' modifier.`
    ///
    /// `checkGrammarModifiers` (`grammarchecks.go:290`), the `must precede`
    /// arms of its accessibility and `override` cases. A left-to-right scan
    /// accumulating what has been seen; a modifier that should have come before
    /// something already seen is the error.
    ///
    /// **At most one report per node.** Every arm upstream is
    /// `return c.grammarErrorOnNode(...)`, so `private static override x` is one
    /// diagnostic and not three — see `checker-notes-diag2.md` §103.
    ///
    /// The `else if` **order is the specification**: `static public async`
    /// reports *"public must precede static"* because `static` is tested before
    /// `async`. It is ported in upstream's order for that reason.
    /// TS1092 — `Type parameters cannot appear on a constructor declaration.`
    ///
    /// `checkGrammarConstructorTypeParameters` (`grammarchecks.go:1860`). The
    /// span is `SkipTrivia(range.Pos())` to `range.End()`, so it covers the
    /// type parameters **themselves** and not the surrounding `<>`; this port's
    /// slice gives first and last directly. §823.
    fn check_constructor_type_parameters(&mut self, node: &tsr_ast::ConstructorDeclaration<'_>) {
        let (Some(first), Some(last)) = (node.type_parameters.first(), node.type_parameters.last())
        else {
            return;
        };
        let (Some(start), Some(end)) = (first.node_id, last.node_id) else { return };
        let Some(file) = self.source_file_of_for_diagnostics(start) else { return };
        let span =
            tsr_core::Span { start: self.nodes.span(start).start, end: self.nodes.span(end).end };
        self.report(
            file,
            Diagnostic::new(
                &messages::TYPE_PARAMETERS_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION,
                span,
            ),
        );
    }

    /// TS1093 — `Type annotation cannot appear on a constructor declaration.`
    ///
    /// `checkGrammarConstructorTypeAnnotation` (`grammarchecks.go:1874`),
    /// reported on the annotation. §823.
    fn check_constructor_type_annotation(&mut self, node: &tsr_ast::ConstructorDeclaration<'_>) {
        let Some(at) = node.r#type.and_then(|t| t.node_id()) else { return };
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        self.report(
            file,
            Diagnostic::new(
                &messages::TYPE_ANNOTATION_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION,
                span,
            ),
        );
    }

    /// TS1103 — `'for await' loops are only allowed within async functions and
    /// at the top levels of modules.`
    ///
    /// `checkGrammarForInOrForOfStatement` (`grammarchecks.go:1205`) gates on
    /// `Flags&NodeFlagsAwaitContext == 0`. That flag is set by upstream's parser
    /// inside async bodies and **this port never sets it** (§829's inventory),
    /// so the test is vacuously true and a literal port would report every
    /// `for await`. The stand-in is what the flag records: the containing
    /// function carries no `async` modifier.
    ///
    /// Top-level `for await` is a different message (TS1431/TS1432, on module
    /// kind and target) and is not built here, so a `for await` with no
    /// containing function is skipped.
    ///
    /// `docs/architecture/checker-notes-diag2.md` §830.
    fn check_for_await_context(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        if self.nodes.kind(node) != SyntaxKind::ForOfStatement {
            return;
        }
        let Some(Node::ForInOrOfStatement(statement)) = self.node_map.get(node) else { return };
        let Some(modifier) = statement.await_modifier else { return };
        let Some(at) = modifier.node_id else { return };
        let Some(function) = self.containing_function_for_await(node) else { return };
        if self.has_async_modifier(function) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.nodes.span(at);
        self.report(
            file,
            Diagnostic::new(
                &messages::FOR_AWAIT_LOOPS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LEVELS_OF_MODULES,
                span,
            ),
        );
    }

    /// `GetContainingFunction` for §830, including the `Constructor` that
    /// upstream's related-info arm treats specially.
    fn containing_function_for_await(&self, node: NodeId) -> Option<NodeId> {
        let mut current = self.nodes.parent(node);
        while let Some(id) = current {
            if matches!(
                self.nodes.kind(id),
                SyntaxKind::FunctionDeclaration
                    | SyntaxKind::FunctionExpression
                    | SyntaxKind::ArrowFunction
                    | SyntaxKind::MethodDeclaration
                    | SyntaxKind::GetAccessor
                    | SyntaxKind::SetAccessor
                    | SyntaxKind::Constructor
            ) {
                return Some(id);
            }
            current = self.nodes.parent(id);
        }
        None
    }

    /// TS2532 — `Object is possibly 'undefined'`, for an **empty** binding
    /// pattern whose initializer is `void`.
    ///
    /// `checkVariableLikeDeclaration`'s binding-pattern arm
    /// (`checker.go:5860`): `needCheckWidenedType` is *"no element has a
    /// name"*, and under `strictNullChecks` the initializer goes to
    /// `checkNonNullNonVoidType` with **the declaration** as the reported node.
    /// A declaration is not an entity name, so the reporter takes its
    /// `Object is possibly …` arm rather than the `'{0}' is possibly …` twin —
    /// which is the arm §846 measured as never reached.
    ///
    /// `docs/architecture/checker-notes-diag2.md` §847.
    fn check_empty_binding_pattern_source(
        &mut self,
        node: NodeId,
        declaration: &tsr_ast::VariableDeclaration<'_>,
        ambient: bool,
    ) {
        if ambient || !self.strict_null_checks || self.file_has_parse_errors {
            return;
        }
        // `IsArrayBindingPattern` takes the iterated-type path instead, so only
        // an object pattern reaches this reporter.
        let Some(tsr_ast::BindingName::BindingPattern(pattern)) = declaration.name else { return };
        // The `kind` field is a **discriminator token** and holds the brace,
        // not the node kind — `OpenBraceToken` for an object pattern. The node
        // table is what answers the question. §847.
        if pattern.node_id.is_none_or(|id| self.nodes.kind(id) != SyntaxKind::ObjectBindingPattern)
        {
            return;
        }
        if !pattern.elements.iter().all(|element| element.name.is_none()) {
            return;
        }
        // `node.Parent.Parent.Kind != KindForInStatement` — a `for…in`
        // initializer is already an error and upstream does not add this one.
        let in_for_in = self
            .nodes
            .parent(node)
            .and_then(|list| self.nodes.parent(list))
            .is_some_and(|owner| self.nodes.kind(owner) == SyntaxKind::ForInStatement);
        if in_for_in {
            return;
        }
        let Some(initializer) = declaration.initializer else { return };
        let initializer_type = self.check_expression(initializer);
        if !self.type_of(initializer_type).flags.intersects(crate::flags::TypeFlags::VOID) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(file, Diagnostic::new(&messages::OBJECT_IS_POSSIBLY_UNDEFINED, span));
    }

    /// TS2300 — `Duplicate identifier 'prototype'`, for a namespace merged with
    /// a class that exports a member of that name.
    ///
    /// `bindClassLikeDeclaration` (`binder/binder.go:962`) mints a `prototype`
    /// symbol on every class and, **before installing it**, reports on any
    /// export of that name already present. Upstream's comment is the
    /// specification: *"this class may be merging into a module. The module
    /// might have an exported variable called `prototype`. We can't allow that
    /// as that would clash with the built-in `prototype` for the class."*
    ///
    /// This port's binder merges the two symbols but never mints `prototype`,
    /// so the collision has nothing to collide with. The question is exact and
    /// needs no type. §908.
    fn check_merged_namespace_prototype(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        let symbol = self.binder.merged_symbol(symbol);
        let Some(&exported) = self.binder.symbols().get(symbol).exports.get("prototype") else {
            return;
        };
        let declarations = self.binder.symbols().get(exported).declarations.clone();
        let Some(&declaration) = declarations.first() else { return };
        // This port keeps a class's **static members** in the same `exports`
        // table. Upstream binds them after `bindClassLikeDeclaration`'s check,
        // into the class's own table, so a `static prototype` is not this
        // collision — it is TS2699 (`check_class_static_property_names`).
        if self.nodes.parent(declaration).is_some_and(|parent| {
            matches!(
                self.nodes.kind(parent),
                SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
            )
        }) {
            return;
        }
        let Some(name) = self.name_node_of(declaration) else { return };
        let Some(file) = self.source_file_of_for_diagnostics(name) else { return };
        let span = self.error_span(name);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::DUPLICATE_IDENTIFIER_0,
                span,
                ["prototype".to_string()],
            ),
        );
    }

    /// TS2699 — `Static property '{0}' conflicts with built-in property
    /// 'Function.{0}' of constructor function '{1}'.`
    ///
    /// Two upstream sites, both skipped in an ambient context
    /// (`checkClassLikeDeclaration`, `checker.go:4308`):
    /// `checkObjectTypeForDuplicateDeclarations`'s `prototype` arm
    /// (`checker.go:3184`) for any static member, and
    /// `checkClassForStaticPropertyNameConflicts` (`checker.go:4393`) for
    /// `name`, `length`, `caller` and `arguments` when class fields are not
    /// defined with `[[Define]]` semantics (`useDefineForClassFields`).
    /// The error node is the member's name. `docs/parity/notes/decls.md` §9.
    fn check_class_static_property_names(
        &mut self,
        node: NodeId,
        members: &[tsr_ast::ClassElement<'_>],
    ) {
        if self.file_has_parse_errors || self.is_in_ambient_context_for_overloads(node) {
            return;
        }
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        let class_name =
            self.binder.symbols().get(self.binder.merged_symbol(symbol)).name.to_string();
        for member in members {
            let Some(id) = Node::from(*member).node_id() else { continue };
            let is_static = self
                .node_map
                .get(id)
                .and_then(modifiers_of)
                .is_some_and(|modifiers| has_modifier(modifiers, SyntaxKind::StaticKeyword));
            if !is_static {
                continue;
            }
            let Some(name) = self.declaration_name_of(id) else { continue };
            // `getEffectivePropertyNameForPropertyNameNode`.
            let text = match self.node_map.get(name) {
                Some(Node::Identifier(identifier)) => identifier.text.to_string(),
                Some(Node::StringLiteral(literal)) => literal.text.to_string(),
                Some(Node::ComputedPropertyName(computed)) => match computed.expression {
                    Some(tsr_ast::Expression::StringLiteral(literal)) => literal.text.to_string(),
                    _ => continue,
                },
                _ => continue,
            };
            let conflicts = text == "prototype"
                || (!self.standard_class_fields
                    && matches!(text.as_str(), "name" | "length" | "caller" | "arguments"));
            if !conflicts {
                continue;
            }
            let Some(file) = self.source_file_of_for_diagnostics(name) else { continue };
            let span = self.error_span(name);
            // The binder half: `bindClassLikeDeclaration` mints a `prototype`
            // property in the class's exports, and a static method or accessor
            // of that name collides with it (`MethodExcludes` and the accessor
            // excludes include `Property`; `PropertyExcludes` is empty), so
            // `declareSymbol` reports TS2300 on it. This port's binder never
            // mints the symbol — the same gap `check_merged_namespace_prototype`
            // fills.
            if text == "prototype"
                && matches!(
                    member,
                    ClassElement::MethodDeclaration(_)
                        | ClassElement::GetAccessorDeclaration(_)
                        | ClassElement::SetAccessorDeclaration(_)
                )
            {
                self.report(
                    file,
                    Diagnostic::with_args(&messages::DUPLICATE_IDENTIFIER_0, span, [text.clone()]),
                );
            }
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::STATIC_PROPERTY_0_CONFLICTS_WITH_BUILT_IN_PROPERTY_FUNCTION_0_OF_CONSTRUCTOR_FUNCTION_1,
                    span,
                    [text, class_name.clone()],
                ),
            );
        }
    }

    /// TS2300 — `Duplicate identifier '{0}'.`, for the members of one class or
    /// interface declaration.
    ///
    /// `checkObjectTypeForDuplicateDeclarations` (`checker.go:3142`) and
    /// `reportDuplicateMemberErrors` (`:3213`), called from
    /// `checkClassLikeDeclaration` (`:4306`) and `checkInterfaceDeclaration`
    /// (`:5016`). `PropertyExcludes` does not contain `Property`, so the
    /// binder merges `x: number; x: string` into one symbol with two
    /// declarations; this walk is where upstream reports it. The test is the
    /// **binder's symbol** — its name (so `1` and `1.0` meet, the binder having
    /// canonicalised both) and its declaration count — not a re-derived name
    /// comparison. Parameter properties take part as instance properties.
    ///
    /// Not gated on parse errors: upstream reports it in a file with syntax
    /// errors too (`numericNamedPropertyDuplicates`).
    /// `docs/parity/notes/decls.md` §12.
    fn check_object_type_for_duplicate_declarations(&mut self, node: NodeId) {
        // (member name node, symbol, kind: 1 property / 2 accessor / 0 other, static)
        let entries = self.object_type_member_entries(node);
        let mut instance_names: std::collections::HashMap<tsr_binder::SymbolId, u8> =
            std::collections::HashMap::new();
        let mut static_names: std::collections::HashMap<tsr_binder::SymbolId, u8> =
            std::collections::HashMap::new();
        let mut reported: Vec<(tsr_binder::SymbolId, bool)> = Vec::new();
        for &(_, symbol, kind, is_static) in &entries {
            if kind == 0 || self.binder.symbols().get(symbol).declarations.len() <= 1 {
                continue;
            }
            let names = if is_static { &mut static_names } else { &mut instance_names };
            // Upstream keys the table by `symbol.Name`; a members table holds
            // one symbol per name, so the symbol is the same key.
            let state = names.get(&symbol).copied().unwrap_or(0);
            if state == 0 {
                names.insert(symbol, kind);
            } else if state == 1 || (state == 2 && kind != 2) {
                names.insert(symbol, 3);
                reported.push((symbol, is_static));
            }
        }
        for (symbol, is_static) in reported {
            let name = self.binder.symbols().get(symbol).name.to_string();
            for &(name_node, member_symbol, _, member_static) in &entries {
                // `checkStatic` is true for this message: a parameter property
                // is an instance member, so `isStatic == ast.IsStatic(member)`
                // holds for it exactly when the duplicate is an instance one.
                if member_symbol != symbol || member_static != is_static {
                    continue;
                }
                let Some(file) = self.source_file_of_for_diagnostics(name_node) else { continue };
                let span = self.error_span(name_node);
                self.report(
                    file,
                    Diagnostic::with_args(&messages::DUPLICATE_IDENTIFIER_0, span, [name.clone()]),
                );
            }
        }
    }

    /// The members `checkObjectTypeForDuplicateDeclarations` walks, in source
    /// order: each member's name node, its symbol, its kind for that walk
    /// (1 property, 2 accessor or auto-accessor, 0 anything else) and whether
    /// it is static. A constructor contributes its parameter properties whose
    /// name is not a binding pattern.
    fn object_type_member_entries(
        &self,
        node: NodeId,
    ) -> Vec<(NodeId, tsr_binder::SymbolId, u8, bool)> {
        let mut entries = Vec::new();
        let mut push = |member: NodeId, name: Option<NodeId>, kind: u8, is_static: bool| {
            let (Some(name), Some(symbol)) = (name, self.binder.symbol_of(member)) else {
                return;
            };
            entries.push((name, self.binder.merged_symbol(symbol), kind, is_static));
        };
        let class_members = match self.node_map.get(node) {
            Some(Node::ClassDeclaration(class)) => class.members,
            Some(Node::ClassExpression(class)) => class.members,
            Some(
                Node::InterfaceDeclaration(InterfaceDeclaration { members, .. })
                | Node::TypeLiteralNode(TypeLiteralNode { members, .. }),
            ) => {
                for member in *members {
                    let Some(id) = member.node_id() else { continue };
                    let kind = match member {
                        tsr_ast::TypeElement::PropertySignatureDeclaration(_) => 1,
                        tsr_ast::TypeElement::GetAccessorDeclaration(_)
                        | tsr_ast::TypeElement::SetAccessorDeclaration(_) => 2,
                        _ => 0,
                    };
                    push(id, self.declaration_name_of(id), kind, false);
                }
                return entries;
            }
            _ => return entries,
        };
        for member in class_members {
            let Some(id) = member.node_id() else { continue };
            if let ClassElement::ConstructorDeclaration(constructor) = member {
                for parameter in constructor.parameters {
                    let Some(parameter_id) = parameter.node_id else { continue };
                    let Some(tsr_ast::BindingName::Identifier(name)) = parameter.name else {
                        continue;
                    };
                    // `ast.IsParameterPropertyDeclaration`: the binder made
                    // the class property this parameter's symbol.
                    if self.binder.symbol_of(parameter_id).is_some_and(|symbol| {
                        self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::PROPERTY)
                    }) {
                        push(parameter_id, name.node_id, 1, false);
                    }
                }
                continue;
            }
            let modifiers = self.node_map.get(id).and_then(modifiers_of).unwrap_or(&[]);
            let is_static = has_modifier(modifiers, SyntaxKind::StaticKeyword);
            let kind = match member {
                ClassElement::PropertyDeclaration(_)
                    if has_modifier(modifiers, SyntaxKind::AccessorKeyword) =>
                {
                    2
                }
                ClassElement::PropertyDeclaration(_) => 1,
                ClassElement::GetAccessorDeclaration(_)
                | ClassElement::SetAccessorDeclaration(_) => 2,
                _ => 0,
            };
            push(id, self.declaration_name_of(id), kind, is_static);
        }
        entries
    }

    fn check_modifier_order(&mut self, node: NodeId, modifiers: &[ModifierLike<'_>]) {
        if self.file_has_parse_errors {
            return;
        }
        // `reportObviousDecoratorErrors` returns from `checkGrammarModifiers`
        // before the per-keyword switch (`grammarchecks.go:218`). §878.
        if self.decorator_error_reported.contains(&node) {
            return;
        }
        if let Some(typed) = self.node_map.get(node)
            && self.check_grammar_decorator_target(node, typed)
        {
            return;
        }
        // **A type member takes no modifier but `readonly`**
        // (`grammarchecks.go:288`), tested in the `else` branch *before* the
        // per-keyword switch — so it takes precedence over every `must precede`
        // arm below it. §599.
        let is_type_member = matches!(
            self.nodes.kind(node),
            SyntaxKind::PropertySignature | SyntaxKind::MethodSignature
        );
        let mut seen: Vec<SyntaxKind> = Vec::new();
        for modifier in modifiers {
            let ModifierLike::Token(token) = modifier else { continue };
            let kind = token.kind;
            // **`const` outside an enum or a type parameter** — the first arm
            // of upstream's per-keyword switch (`grammarchecks.go:302`). The
            // message says *class member* and the test does not: it fires for
            // any other node kind, and it reports on the **node**, not the
            // modifier. §604.
            if kind == SyntaxKind::ConstKeyword
                && !matches!(
                    self.nodes.kind(node),
                    SyntaxKind::EnumDeclaration | SyntaxKind::TypeParameter
                )
            {
                if let Some(file) = self.source_file_of_for_diagnostics(node) {
                    let span = self.error_span(node);
                    self.report(
                        file,
                        Diagnostic::with_args(
                            &messages::A_CLASS_MEMBER_CANNOT_HAVE_THE_0_KEYWORD,
                            span,
                            ["const".to_string()],
                        ),
                    );
                }
                return;
            }
            if is_type_member
                && kind != SyntaxKind::ReadonlyKeyword
                && let Some(text) = modifier_keyword_text(kind)
            {
                self.report_modifier_error(
                    token,
                    &messages::_0_MODIFIER_CANNOT_APPEAR_ON_A_TYPE_MEMBER,
                    &[text.to_string()],
                );
                return;
            }
            // TS1028, the **first** arm of the same `if`/`else if` chain
            // (`grammarchecks.go:336`): `private public x` is *"Accessibility
            // modifier already seen"*, not *"'public' must precede
            // 'private'"*. §103's rule — the `else if` order is the
            // specification — and it binds here in the direction that decides
            // which of two codes lands on one token. §178.
            if matches!(
                kind,
                SyntaxKind::PublicKeyword
                    | SyntaxKind::ProtectedKeyword
                    | SyntaxKind::PrivateKeyword
            ) && !self.file_has_parse_errors
                && seen.iter().any(|earlier| {
                    matches!(
                        earlier,
                        SyntaxKind::PublicKeyword
                            | SyntaxKind::ProtectedKeyword
                            | SyntaxKind::PrivateKeyword
                    )
                })
            {
                let Some(id) = token.node_id else { return };
                let Some(file) = self.source_file_of_for_diagnostics(id) else { return };
                let span = self.error_span(id);
                self.report(
                    file,
                    Diagnostic::new(&messages::ACCESSIBILITY_MODIFIER_ALREADY_SEEN, span),
                );
                return;
            }
            // `readonly` already seen — the same shape as TS1028's accessibility
            // arm above, for a different keyword. §662.
            if kind == SyntaxKind::ReadonlyKeyword && seen.contains(&SyntaxKind::ReadonlyKeyword) {
                self.report_modifier_error(
                    token,
                    &messages::_0_MODIFIER_ALREADY_SEEN,
                    &["readonly".to_string()],
                );
                return;
            }
            // `private abstract` — `abstract`'s own case reports TS1243 when
            // `private` has been seen, which §595 guarded against and did not
            // report. §662.
            if kind == SyntaxKind::AbstractKeyword && seen.contains(&SyntaxKind::PrivateKeyword) {
                self.report_modifier_error(
                    token,
                    &messages::_0_MODIFIER_CANNOT_BE_USED_WITH_1_MODIFIER,
                    &["private".to_string(), "abstract".to_string()],
                );
                return;
            }
            let precede: Option<&str> = match kind {
                SyntaxKind::PublicKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::PrivateKeyword => [
                    (SyntaxKind::OverrideKeyword, "override"),
                    (SyntaxKind::StaticKeyword, "static"),
                    (SyntaxKind::AccessorKeyword, "accessor"),
                    (SyntaxKind::ReadonlyKeyword, "readonly"),
                    (SyntaxKind::AsyncKeyword, "async"),
                ]
                .into_iter()
                .find(|(earlier, _)| seen.contains(earlier))
                .map(|(_, name)| name),
                SyntaxKind::OverrideKeyword => [
                    (SyntaxKind::ReadonlyKeyword, "readonly"),
                    (SyntaxKind::AccessorKeyword, "accessor"),
                    (SyntaxKind::AsyncKeyword, "async"),
                ]
                .into_iter()
                .find(|(earlier, _)| seen.contains(earlier))
                .map(|(_, name)| name),
                // `export` must precede `declare`, `abstract` and `async`
                // (`grammarchecks.go:408-412`).
                SyntaxKind::ExportKeyword => [
                    (SyntaxKind::DeclareKeyword, "declare"),
                    (SyntaxKind::AbstractKeyword, "abstract"),
                    (SyntaxKind::AsyncKeyword, "async"),
                ]
                .into_iter()
                .find(|(earlier, _)| seen.contains(earlier))
                .map(|(_, name)| name),
                // **The inverted arm** (`grammarchecks.go:437`): `default`
                // reports when `export` has *not* been seen, where every other
                // arm reports when something *has*. Same message, opposite
                // test. §593.
                SyntaxKind::DefaultKeyword if !seen.contains(&SyntaxKind::ExportKeyword) => {
                    Some("default")
                }
                // **`abstract`'s pair sits after two `cannot_be_used_with`
                // checks upstream** (`grammarchecks.go:487-497`), and this
                // table is consulted before this port's equivalents — so the
                // arm carries the guard its original position gave it for
                // free. §595.
                // `static`'s four (`grammarchecks.go:362-375`). The `override`
                // entry sits after the `static abstract` check upstream, so it
                // carries that guard here. §597.
                SyntaxKind::StaticKeyword if !seen.contains(&SyntaxKind::AbstractKeyword) => [
                    (SyntaxKind::ReadonlyKeyword, "readonly"),
                    (SyntaxKind::AsyncKeyword, "async"),
                    (SyntaxKind::AccessorKeyword, "accessor"),
                    (SyntaxKind::OverrideKeyword, "override"),
                ]
                .into_iter()
                .find(|(earlier, _)| seen.contains(earlier))
                .map(|(_, name)| name),
                SyntaxKind::AbstractKeyword
                    if !seen.contains(&SyntaxKind::PrivateKeyword)
                        && !seen.contains(&SyntaxKind::AsyncKeyword) =>
                {
                    [
                        (SyntaxKind::OverrideKeyword, "override"),
                        (SyntaxKind::AccessorKeyword, "accessor"),
                    ]
                    .into_iter()
                    .find(|(earlier, _)| seen.contains(earlier))
                    .map(|(_, name)| name)
                }
                _ => None,
            };
            // The arms below are the rest of each keyword's `else if` chain,
            // in upstream's order. Every one ends in `return`, which is why a
            // node gets at most one grammar diagnostic. §183.
            let parent_is_module_or_file = self.nodes.parent(node).is_some_and(|parent| {
                matches!(self.nodes.kind(parent), SyntaxKind::ModuleBlock | SyntaxKind::SourceFile)
            });
            let parent_is_class_like = self.nodes.parent(node).is_some_and(|parent| {
                matches!(
                    self.nodes.kind(parent),
                    SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
                )
            });
            let is_parameter = self.nodes.kind(node) == SyntaxKind::Parameter;
            let is_property = self.nodes.kind(node) == SyntaxKind::PropertyDeclaration;
            let accessibility = |seen: &[SyntaxKind]| {
                seen.iter().any(|k| {
                    matches!(
                        k,
                        SyntaxKind::PublicKeyword
                            | SyntaxKind::ProtectedKeyword
                            | SyntaxKind::PrivateKeyword
                    )
                })
            };
            let _ = accessibility;
            let text = modifier_keyword_text(kind);
            // `X_0_modifier_already_seen` — the head of the `override`,
            // `static`, `export`, `declare`, `abstract`, `accessor` and
            // `readonly` chains, and the one arm every keyword shares.
            if matches!(
                kind,
                SyntaxKind::OverrideKeyword
                    | SyntaxKind::StaticKeyword
                    | SyntaxKind::ExportKeyword
                    | SyntaxKind::DeclareKeyword
                    | SyntaxKind::AbstractKeyword
                    | SyntaxKind::AccessorKeyword
                    | SyntaxKind::ReadonlyKeyword
            ) && seen.contains(&kind)
                && let Some(text) = text
            {
                self.report_modifier_error(
                    token,
                    &messages::_0_MODIFIER_ALREADY_SEEN,
                    &[text.to_string()],
                );
                return;
            }
            // **A constructor takes none of `static`, `override` or `async`**
            // (`grammarchecks.go:547`), tested after the `flags` loop upstream
            // and therefore after every arm that could claim the same modifier.
            // Fifth arm placed into this chain (§103, §183, §599, §817). §823.
            if self.nodes.kind(node) == SyntaxKind::Constructor
                && matches!(
                    kind,
                    SyntaxKind::StaticKeyword
                        | SyntaxKind::OverrideKeyword
                        | SyntaxKind::AsyncKeyword
                )
                && let Some(text) = modifier_keyword_text(kind)
            {
                self.report_modifier_error(
                    token,
                    &messages::_0_MODIFIER_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION,
                    &[text.to_string()],
                );
                return;
            }
            // `async` in an ambient context (`grammarchecks.go:507`).
            // Upstream's `flags` accumulates left to right over the modifier
            // list, so `flags&Ambient != 0` is exactly *"a `declare` earlier in
            // this same list"* — the `seen` vector already carries it. Placed
            // after the shared `already seen` arm and before the must-precede
            // arms, which is upstream's `else if` order. §817.
            if kind == SyntaxKind::AsyncKeyword
                && !seen.contains(&SyntaxKind::AsyncKeyword)
                && (seen.contains(&SyntaxKind::DeclareKeyword)
                    || self.is_in_ambient_context_for_overloads(node))
            {
                self.report_modifier_error(
                    token,
                    &messages::_0_MODIFIER_CANNOT_BE_USED_IN_AN_AMBIENT_CONTEXT,
                    &["async".to_string()],
                );
                return;
            }
            // `X_0_modifier_cannot_appear_on_a_module_or_namespace_element` —
            // in the accessibility chain **after** the must-precede arms below
            // and in the `static` chain after them too, so it is tested here
            // only for `static`; the accessibility case falls through to
            // `precede` first and is handled after it.
            if kind == SyntaxKind::StaticKeyword
                && parent_is_module_or_file
                && let Some(text) = text
            {
                self.report_modifier_error(
                    token,
                    &messages::_0_MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
                    &[text.to_string()],
                );
                return;
            }
            // `X_0_modifier_cannot_appear_on_a_parameter` — `static`, `export`
            // and `declare`.
            if matches!(
                kind,
                SyntaxKind::StaticKeyword | SyntaxKind::ExportKeyword | SyntaxKind::DeclareKeyword
            ) && is_parameter
                && let Some(text) = text
            {
                self.report_modifier_error(
                    token,
                    &messages::_0_MODIFIER_CANNOT_APPEAR_ON_A_PARAMETER,
                    &[text.to_string()],
                );
                return;
            }
            // `X_0_modifier_cannot_appear_on_class_elements_of_this_kind` —
            // `export` on any class element, `declare` on any that is not a
            // property declaration.
            // **An index signature is TS1071's**, and upstream tests it
            // (`grammarchecks.go:291`) *before* the per-keyword switch this arm
            // belongs to, then returns. This port has that test in a separate
            // function — `check_index_signature_modifiers` — outside this
            // chain, so without the deferral both fire. Third rule this session
            // ported outside the chain that kept speaking after it (§857, §858).
            // §871.
            if self.nodes.kind(node) != SyntaxKind::IndexSignature
                && ((kind == SyntaxKind::ExportKeyword && parent_is_class_like)
                    || (kind == SyntaxKind::DeclareKeyword && parent_is_class_like && !is_property))
                && let Some(text) = text
            {
                self.report_modifier_error(
                    token,
                    &messages::_0_MODIFIER_CANNOT_APPEAR_ON_CLASS_ELEMENTS_OF_THIS_KIND,
                    &[text.to_string()],
                );
                return;
            }
            // `A_declare_modifier_cannot_be_used_in_an_already_ambient_context`
            // — the SIXTH arm of the `declare` chain (`grammarchecks.go:459`),
            // and the row §179 declined to build alone. `node.Parent.Flags &
            // NodeFlagsAmbient != 0 && node.Parent.Kind == ModuleBlock`: this
            // parser has no ambient flag, so the walk-threaded `ambient` and
            // the parent's kind stand in for it, which is §99's substitute.
            //
            // **The `using` / `await using` arms sit between the parameter arm
            // above and this one and cannot fire here** — this port has no
            // block-scope kind for them. §183 records that as the reason this
            // build is barred below its ceiling.
            // **The predicate is "an enclosing `declare`", not "the file".**
            // §445 substituted the walk-threaded `ambient` — `file_is_ambient`
            // OR an enclosing `declare` — and measured −14, because in a `.d.ts`
            // it is true of every node whether or not any ancestor carries the
            // modifier. `declaration_is_in_an_ambient_context` walks ancestors
            // for an actual `declare` and excludes the bare-file case, which is
            // upstream's `NodeFlagsAmbient` for the shape this arm tests. §508.
            if kind == SyntaxKind::DeclareKeyword
                && self
                    .nodes
                    .parent(node)
                    .is_some_and(|parent| self.declaration_is_in_an_ambient_context(parent))
                && self
                    .nodes
                    .parent(node)
                    .is_some_and(|parent| self.nodes.kind(parent) == SyntaxKind::ModuleBlock)
            {
                self.report_modifier_error(
                    token,
                    &messages::A_DECLARE_MODIFIER_CANNOT_BE_USED_IN_AN_ALREADY_AMBIENT_CONTEXT,
                    &[],
                );
                return;
            }
            if let Some(after) = precede {
                // `visibilityToString` — the keyword's own text.
                let text = match kind {
                    SyntaxKind::PublicKeyword => "public",
                    SyntaxKind::ProtectedKeyword => "protected",
                    SyntaxKind::PrivateKeyword => "private",
                    SyntaxKind::ExportKeyword | SyntaxKind::DefaultKeyword => "export",
                    SyntaxKind::AbstractKeyword => "abstract",
                    SyntaxKind::StaticKeyword => "static",
                    _ => "override",
                };
                let Some(id) = token.node_id else { return };
                let Some(file) = self.source_file_of_for_diagnostics(id) else { return };
                let span = self.error_span(id);
                self.report(
                    file,
                    Diagnostic::with_args(
                        &messages::_0_MODIFIER_MUST_PRECEDE_1_MODIFIER,
                        span,
                        [text.to_string(), after.to_string()],
                    ),
                );
                return;
            }
            // The accessibility chain's module-or-namespace arm, which sits
            // **after** its five must-precede arms (`grammarchecks.go:402`).
            if matches!(
                kind,
                SyntaxKind::PublicKeyword
                    | SyntaxKind::ProtectedKeyword
                    | SyntaxKind::PrivateKeyword
            ) && parent_is_module_or_file
                && let Some(text) = text
            {
                self.report_modifier_error(
                    token,
                    &messages::_0_MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
                    &[text.to_string()],
                );
                return;
            }
            // `private` with `abstract` is TS1243; the other two spellings take
            // the must-precede arm above (`grammarchecks.go:405-409`).
            if kind == SyntaxKind::PrivateKeyword && seen.contains(&SyntaxKind::AbstractKeyword) {
                self.report_modifier_error(
                    token,
                    &messages::_0_MODIFIER_CANNOT_BE_USED_WITH_1_MODIFIER,
                    &["private".to_string(), "abstract".to_string()],
                );
                return;
            }
            // The accessibility chain's last arm (`grammarchecks.go:355`),
            // reached only without `abstract`.
            if matches!(
                kind,
                SyntaxKind::PublicKeyword
                    | SyntaxKind::ProtectedKeyword
                    | SyntaxKind::PrivateKeyword
            ) && !seen.contains(&SyntaxKind::AbstractKeyword)
                && self.is_private_identifier_class_element_declaration(node)
            {
                self.report_modifier_error(
                    token,
                    &messages::AN_ACCESSIBILITY_MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER,
                    &[],
                );
                return;
            }
            // `declare` with `accessor`, the last arm of the `declare` chain.
            if kind == SyntaxKind::DeclareKeyword && seen.contains(&SyntaxKind::AccessorKeyword) {
                self.report_modifier_error(
                    token,
                    &messages::_0_MODIFIER_CANNOT_BE_USED_WITH_1_MODIFIER,
                    &["declare".to_string(), "accessor".to_string()],
                );
                return;
            }
            seen.push(kind);
        }
        self.check_jsdoc_reparsed_modifier_grammar(node, &mut seen);
    }

    /// `grammarErrorOnNode(modifier, …)` — every arm of
    /// `checkGrammarModifiers` reports on the modifier token itself.
    fn report_modifier_error(
        &mut self,
        token: &tsr_ast::Token<'_>,
        message: &'static tsr_diagnostics::Message,
        args: &[String],
    ) {
        // §876: the chain's callers upstream are gated on `!checkGrammarModifiers(node)`,
        // so a node that got a modifier diagnostic here must not get one from
        // the rules this port split out of the chain.
        if let Some(id) = token.node_id.and_then(|id| self.nodes.parent(id)) {
            self.modifier_chain_reported.insert(id);
        }
        let Some(id) = token.node_id else { return };
        let Some(file) = self.source_file_of_for_diagnostics(id) else { return };
        let span = self.error_span(id);
        let diagnostic = if args.is_empty() {
            Diagnostic::new(message, span)
        } else {
            Diagnostic::with_args(message, span, args.to_vec())
        };
        self.report(file, diagnostic);
    }

    /// TS1071 — `'{0}' modifier cannot appear on an index signature.`
    ///
    /// `checkGrammarModifiers` (`grammarchecks.go:292`), inside the
    /// non-decorator branch and behind `modifier.Kind != KindReadonlyKeyword`:
    /// `readonly [k: string]: T` is legal and every other modifier is not.
    /// `static` is exempt **only** on a class-like parent, which an index
    /// signature in a type literal or interface never has.
    ///
    /// At most one report, like every arm of that function. §178.
    fn check_index_signature_modifiers(&mut self, node: NodeId, modifiers: &[ModifierLike<'_>]) {
        if self.file_has_parse_errors {
            return;
        }
        let class_like = self.nodes.parent(node).is_some_and(|parent| {
            matches!(
                self.nodes.kind(parent),
                SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
            )
        });
        for modifier in modifiers {
            let ModifierLike::Token(token) = modifier else { continue };
            if token.kind == SyntaxKind::ReadonlyKeyword {
                continue;
            }
            if token.kind == SyntaxKind::StaticKeyword && class_like {
                continue;
            }
            // `scanner.TokenToString(modifier.Kind)` — the keyword's own text.
            let Some(text) = modifier_keyword_text(token.kind) else { continue };
            // `checkGrammarIndexSignature` is `checkGrammarModifiers(node) ||
            // checkGrammarIndexSignatureParameters(node)`: a modifier report
            // stops the parameter checks (`grammar.rs`).
            self.modifier_chain_reported.insert(node);
            let Some(id) = token.node_id else { return };
            let Some(file) = self.source_file_of_for_diagnostics(id) else { return };
            let span = self.error_span(id);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::_0_MODIFIER_CANNOT_APPEAR_ON_AN_INDEX_SIGNATURE,
                    span,
                    [text.to_string()],
                ),
            );
            return;
        }
    }

    /// TS1155 — `'{0}' declarations must be initialized.`
    ///
    /// `checkGrammarVariableDeclaration` (`grammarchecks.go:1582`). A `const`
    /// with no initialiser, and upstream's three guards, each of which is a
    /// different legal shape rather than a bound this port chose:
    ///
    /// - **not the variable of a `for-in`/`for-of`** — `for (const x of xs)`
    ///   has no initialiser and needs none, tested at the *list's* parent;
    /// - **not ambient** — `declare const x` is a declaration, not a definition,
    ///   and `checkAmbientInitializer` takes that path instead;
    /// - **not a binding pattern** — `const {a} = …` without an initialiser is
    ///   TS1182, a different code on the same line.
    ///
    /// The error node is the **declaration**, not its name:
    /// `grammarErrorOnNode(node.AsNode(), …)`, so this is `nodes.span` and not
    /// `error_span`, which would narrow to the name.
    ///
    /// `using` and `await using` share the arm upstream and are not ported —
    /// this parser has no `using` block-scope kind, so the two spellings cannot
    /// be told from `const` here. §178.
    fn check_const_is_initialized(
        &mut self,
        node: NodeId,
        declaration: &tsr_ast::VariableDeclaration<'_>,
        ambient: bool,
    ) {
        if ambient || self.file_has_parse_errors || declaration.initializer.is_some() {
            return;
        }
        // `IsBindingPattern(node.Name())` — a pattern with no initialiser is
        // TS1182, a different code on the same line.
        if !matches!(declaration.name, Some(tsr_ast::BindingName::Identifier(_))) {
            return;
        }
        let Some(list) = self.nodes.parent(node) else { return };
        if self.nodes.kind(list) != SyntaxKind::VariableDeclarationList {
            return;
        }
        // `blockScopeKind == ast.NodeFlagsConst` — the flag the parser sets on
        // the *list*, read the way `crate::flow` reads it.
        // `await using` is `CONST | USING` (`NodeFlagsAwaitUsing`), so the
        // comparison is on the whole block-scope kind, not one bit of it.
        if self.nodes.flags(list) & tsr_ast::NodeFlags::BLOCK_SCOPED != tsr_ast::NodeFlags::CONST {
            return;
        }
        // `node.Parent.Parent.Kind != KindForInStatement && … ForOfStatement`.
        if self.nodes.parent(list).is_some_and(|owner| {
            matches!(
                self.nodes.kind(owner),
                SyntaxKind::ForInStatement | SyntaxKind::ForOfStatement
            )
        }) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.nodes.span(node);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::_0_DECLARATIONS_MUST_BE_INITIALIZED,
                span,
                ["const".to_string()],
            ),
        );
    }

    /// TS1221 — `Generators are not allowed in an ambient context.`
    ///
    /// `checkGrammarForGenerator` (`grammarchecks.go:990`). The error node is
    /// the **asterisk**, not the name, and the first of the function's two arms
    /// wins: an ambient generator is TS1221 and a bodiless one TS1222, so the
    /// ambient test must come first or `declare function* f();` takes the wrong
    /// code. §180.
    /// TS2378 — `A 'get' accessor must return a value.`
    ///
    /// `checkAccessorDeclaration` (`checker.go:2941`) tests
    /// `HasImplicitReturn && !HasExplicitReturn`, two `NodeFlags` upstream's
    /// **parser** sets and this port's does not (`flags.rs:34`, `:36` — declared
    /// and set nowhere, the same category as `SymbolFlags::OPTIONAL`).
    ///
    /// So this is a bounded slice, sound in the reporting direction: **no
    /// `return` in the body** makes `HasExplicitReturn` certainly false, and
    /// **no `throw` either** makes `HasImplicitReturn` certainly true. A getter
    /// that returns on *some* paths has both flags upstream and is declined
    /// here — a missing line, never a wrong one. §267.
    fn check_get_accessor_returns(
        &mut self,
        accessor: &tsr_ast::GetAccessorDeclaration<'_>,
        ambient: bool,
    ) {
        if ambient || self.file_has_parse_errors {
            return;
        }
        // `ast.NodeIsPresent(node.Body())` — an overload or a `.d.ts` accessor
        // has none.
        let Some(body) = accessor.body else { return };
        let Some(body_id) = body.node_id() else { return };
        if self.subtree_has_return_or_throw(body_id) {
            return;
        }
        let Some(name) = accessor.name.node_id() else { return };
        let Some(file) = self.source_file_of_for_diagnostics(name) else { return };
        let span = self.error_span(name);
        self.report(file, Diagnostic::new(&messages::A_GET_ACCESSOR_MUST_RETURN_A_VALUE, span));
    }

    /// Does this subtree contain a `return` or a `throw`, **not** descending
    /// into a nested function-like body?
    ///
    /// The nesting rule is upstream's by construction: the parser sets the
    /// return flags on the function whose body it is walking, so a `return`
    /// inside a nested arrow belongs to the arrow.
    fn subtree_has_return_or_throw(&self, node: NodeId) -> bool {
        if matches!(self.nodes.kind(node), SyntaxKind::ReturnStatement | SyntaxKind::ThrowStatement)
        {
            return true;
        }
        let mut children = Vec::new();
        if let Some(typed) = self.node_map.get(node) {
            tsr_ast::for_each_child_id(typed, |child| children.push(child));
        }
        children.into_iter().any(|child| {
            !self.is_function_like_or_static_block(child) && self.subtree_has_return_or_throw(child)
        })
    }

    /// `checkGrammarModifiers`' parameter-property and `abstract` arms
    /// (`grammarchecks.go:560`, `:475`).
    ///
    /// Three of the seven still-missing codes that function carries, found by
    /// grouping the gap by the **upstream function** a diagnostic is reported
    /// from rather than by code — see §278, where nothing that ranks by case
    /// count puts them near each other.
    fn check_grammar_modifier_shapes(&mut self, node: NodeId, typed: Node<'_>) {
        if self.file_has_parse_errors {
            return;
        }
        // The parser lane's `checkGrammar*` ports that upstream runs behind
        // the same `!checkGrammarModifiers(node)` guard (`grammar.rs`).
        self.check_grammar_behind_modifiers(node, typed);
        match typed {
            Node::ParameterDeclaration(parameter) => {
                // `flags&ast.ModifierFlagsParameterPropertyModifier != 0` —
                // `public`/`private`/`protected`/`readonly`/`override` on a
                // parameter make it a parameter property.
                if !parameter.modifiers.iter().any(|modifier| {
                    matches!(
                        modifier,
                        tsr_ast::ModifierLike::Token(token)
                            if matches!(
                                token.kind,
                                SyntaxKind::PublicKeyword
                                    | SyntaxKind::PrivateKeyword
                                    | SyntaxKind::ProtectedKeyword
                                    | SyntaxKind::ReadonlyKeyword
                                    | SyntaxKind::OverrideKeyword
                            )
                    )
                }) {
                    return;
                }
                let message =
                    if matches!(parameter.name, Some(tsr_ast::BindingName::BindingPattern(_))) {
                        &messages::A_PARAMETER_PROPERTY_MAY_NOT_BE_DECLARED_USING_A_BINDING_PATTERN
                    } else if parameter.dot_dot_dot_token.is_some() {
                        &messages::A_PARAMETER_PROPERTY_CANNOT_BE_DECLARED_USING_A_REST_PARAMETER
                    } else {
                        return;
                    };
                let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
                // `grammarErrorOnNode(node, …)` — the whole parameter,
                // modifiers included.
                let span = self.nodes.span(node);
                self.report(file, Diagnostic::new(message, span));
            }
            // `abstract` on a member whose parent class is not `abstract`
            // (`grammarchecks.go:475`), split by whether the member is a
            // property.
            Node::MethodDeclaration(_) | Node::PropertyDeclaration(_) => {
                let modifiers = match typed {
                    Node::MethodDeclaration(n) => n.modifiers,
                    Node::PropertyDeclaration(n) => n.modifiers,
                    _ => return,
                };
                if !modifiers.iter().any(|modifier| {
                    matches!(modifier, tsr_ast::ModifierLike::Token(t) if t.kind == SyntaxKind::AbstractKeyword)
                }) {
                    return;
                }
                let Some(parent) = self.nodes.parent(node) else { return };
                if self.nodes.kind(parent) != SyntaxKind::ClassDeclaration {
                    return;
                }
                let Some(Node::ClassDeclaration(class)) = self.node_map.get(parent) else { return };
                if class.modifiers.iter().any(|modifier| {
                    matches!(modifier, tsr_ast::ModifierLike::Token(t) if t.kind == SyntaxKind::AbstractKeyword)
                }) {
                    return;
                }
                let message = if matches!(typed, Node::PropertyDeclaration(_)) {
                    &messages::ABSTRACT_PROPERTIES_CAN_ONLY_APPEAR_WITHIN_AN_ABSTRACT_CLASS
                } else {
                    &messages::ABSTRACT_METHODS_CAN_ONLY_APPEAR_WITHIN_AN_ABSTRACT_CLASS
                };
                let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
                let span = self.nodes.span(node);
                self.report(file, Diagnostic::new(message, span));
            }
            _ => {}
        }
        self.check_grammar_default_and_const_modifiers(node, typed);
        self.check_grammar_object_literal_modifiers(typed);
        self.check_grammar_object_literal_postfix_tokens(typed);
    }

    /// `checkGrammarModifiers`' `KindDefaultKeyword` and `KindConstKeyword`
    /// arms (`grammarchecks.go:423`, `:301`). §280.
    fn check_grammar_default_and_const_modifiers(&mut self, node: NodeId, typed: Node<'_>) {
        let Some(modifiers) = modifiers_of(typed) else { return };
        for modifier in modifiers {
            let tsr_ast::ModifierLike::Token(token) = modifier else { continue };
            let message = match token.kind {
                SyntaxKind::DefaultKeyword => {
                    // `container = node.Parent.Kind == SourceFile ? node.Parent
                    // : node.Parent.Parent`. A statement inside a namespace has
                    // the `ModuleBlock` as its parent and the
                    // `ModuleDeclaration` as its grandparent; at file scope the
                    // parent *is* the container.
                    let Some(parent) = self.nodes.parent(node) else { continue };
                    let container = if self.nodes.kind(parent) == SyntaxKind::SourceFile {
                        parent
                    } else {
                        let Some(grandparent) = self.nodes.parent(parent) else { continue };
                        grandparent
                    };
                    if self.nodes.kind(container) != SyntaxKind::ModuleDeclaration
                        || self.module_declaration_is_ambient(container)
                    {
                        continue;
                    }
                    &messages::A_DEFAULT_EXPORT_CAN_ONLY_BE_USED_IN_AN_ECMASCRIPT_STYLE_MODULE
                }
                SyntaxKind::ConstKeyword => {
                    if matches!(typed, Node::EnumDeclaration(_) | Node::TypeParameterDeclaration(_))
                    {
                        continue;
                    }
                    &messages::A_CLASS_MEMBER_CANNOT_HAVE_THE_0_KEYWORD
                }
                _ => continue,
            };
            let Some(token_id) = token.node_id else { continue };
            let Some(file) = self.source_file_of_for_diagnostics(token_id) else { continue };
            // `grammarErrorOnNode(modifier, …)` — the modifier, not the
            // declaration.
            let span = self.nodes.span(token_id);
            let diagnostic = if token.kind == SyntaxKind::ConstKeyword {
                Diagnostic::with_args(message, span, ["const".to_string()])
            } else {
                Diagnostic::new(message, span)
            };
            self.report(file, diagnostic);
            // `checkGrammarModifiers` returns on the first offender.
            return;
        }
    }

    /// `ast.IsAmbientModule` — a module with a string-literal name, or one
    /// carrying `declare`.
    fn module_declaration_is_ambient(&self, node: NodeId) -> bool {
        let Some(Node::ModuleDeclaration(module)) = self.node_map.get(node) else { return false };
        if matches!(module.name, Some(tsr_ast::ModuleName::StringLiteral(_))) {
            return true;
        }
        module.modifiers.iter().any(|modifier| {
            matches!(modifier, tsr_ast::ModifierLike::Token(t) if t.kind == SyntaxKind::DeclareKeyword)
        })
    }

    /// `checkTypeNameIsReserved` (`checker.go:6901`) — the eleven predefined
    /// type keywords, which "are reserved and cannot be used as names of user
    /// defined types" (TS 1.0 spec 3.6.1, cited in upstream's own comment).
    ///
    /// One helper, five callers, five codes. §276.
    fn check_type_name_is_reserved(
        &mut self,
        name: Option<NodeId>,
        text: &str,
        message: &'static tsr_diagnostics::Message,
    ) {
        if !matches!(
            text,
            "any"
                | "unknown"
                | "never"
                | "number"
                | "bigint"
                | "boolean"
                | "string"
                | "symbol"
                | "void"
                | "object"
                | "undefined"
        ) {
            return;
        }
        let Some(name) = name else { return };
        let Some(file) = self.source_file_of_for_diagnostics(name) else { return };
        let span = self.nodes.span(name);
        self.report(file, Diagnostic::with_args(message, span, [text.to_string()]));
    }

    /// The reserved-name check for each declaration kind that has one.
    fn check_reserved_declaration_name(&mut self, typed: Node<'_>) {
        let (name, message) = match typed {
            Node::ClassDeclaration(n) => (n.name, &messages::CLASS_NAME_CANNOT_BE_0),
            Node::ClassExpression(n) => (n.name, &messages::CLASS_NAME_CANNOT_BE_0),
            Node::InterfaceDeclaration(n) => (n.name, &messages::INTERFACE_NAME_CANNOT_BE_0),
            Node::TypeAliasDeclaration(n) => (n.name, &messages::TYPE_ALIAS_NAME_CANNOT_BE_0),
            Node::TypeParameterDeclaration(n) => {
                (n.name, &messages::TYPE_PARAMETER_NAME_CANNOT_BE_0)
            }
            // `checkImportEqualsDeclaration` (`checker.go:5488`) is upstream's
            // **only** site for `Import_name_cannot_be_0`, and it is not an
            // import *specifier*. See
            // [`Checker::check_reserved_import_equals_name`].
            Node::ImportEqualsDeclaration(_) => {
                self.check_reserved_import_equals_name(typed);
                return;
            }
            _ => return,
        };
        let Some(name) = name else { return };
        self.check_type_name_is_reserved(name.node_id, name.text, message);
    }

    /// TS2431 — `Enum name cannot be '{0}'.`
    ///
    /// `checkCollisionsForDeclarationName`'s enum arm (`checker.go:10459`),
    /// reached from `checkEnumDeclarationWorker` for every enum declaration.
    fn check_reserved_enum_name(&mut self, declaration: &tsr_ast::EnumDeclaration<'_>) {
        let Some(name) = declaration.name else { return };
        self.check_type_name_is_reserved(name.node_id, name.text, &messages::ENUM_NAME_CANNOT_BE_0);
    }

    /// TS2438 — `Import name cannot be '{0}'.`
    ///
    /// `checkImportEqualsDeclaration` (`checker.go:5479-5489`), and the guards
    /// are the rule:
    ///
    /// ```go
    /// moduleReference := node.AsImportEqualsDeclaration().ModuleReference
    /// if !ast.IsExternalModuleReference(moduleReference) {
    ///     target := c.resolveAlias(c.getSymbolOfDeclaration(node))
    ///     if target != c.unknownSymbol {
    ///         if c.getSymbolFlags(target)&ast.SymbolFlagsType != 0 {
    ///             c.checkTypeNameIsReserved(node.Name(), diagnostics.Import_name_cannot_be_0)
    ///         }
    ///     }
    /// }
    /// ```
    ///
    /// So it fires for `import boolean = N.SomeType` and for nothing else: an
    /// **internal** module reference (not `require("…")`) whose target has a
    /// *type* meaning.
    ///
    /// # What this replaced, and what it cost
    ///
    /// This rule was attached to `ImportClause`, `NamespaceImport` and
    /// `ImportSpecifier` — three node kinds upstream never reaches with this
    /// message. `checkTypeNameIsReserved` has six callers (`:2623`, `:4999`,
    /// `:5488`, `:6879`, `:10454`, `:10459`) and not one of them is a named
    /// import.
    ///
    /// The reserved list is the *predefined type keywords*, so the misplacement
    /// turned every `import { boolean } from "drizzle-orm/pg-core"` into an
    /// error. A schema library exporting column constructors named `boolean`,
    /// `bigint`, `number` and `object` is the ordinary case, and it was worth
    /// **31 diagnostics** on a 22-package repository against `tsc`'s zero.
    ///
    /// `docs/architecture/checker-notes-diag2.md` §660.
    fn check_reserved_import_equals_name(&mut self, typed: Node<'_>) {
        let Node::ImportEqualsDeclaration(node) = typed else { return };
        let Some(name) = node.name else { return };
        // `resolveAlias(getSymbolOfDeclaration(node))` then `getSymbolFlags`:
        // the meaning that matters is the **target's**, not the alias's.
        //
        // Resolved through [`Checker::resolve_entity_name`] rather than
        // [`Checker::resolve_alias`], because the latter declines a *qualified*
        // module reference — and `import boolean = N.SomeType` is the only
        // shape this rule fires on, so declining it would leave the rule dead.
        // That decline is about **printing**: its rustdoc records that a
        // qualified name resolves fine and prints wrong for want of symbol
        // accessibility. This rule never prints the target; it asks only
        // whether the target has a type meaning. Same argument as §221's JSX
        // namespace and §400's alias half.
        let Some(module_reference) = node.module_reference else { return };
        let reference = match module_reference {
            ModuleReference::Identifier(identifier) => tsr_ast::EntityName::Identifier(identifier),
            ModuleReference::QualifiedName(qualified) => {
                tsr_ast::EntityName::QualifiedName(qualified)
            }
            ModuleReference::ExternalModuleReference(_) => return,
        };
        let meaning = SymbolFlags::TYPE | SymbolFlags::NAMESPACE | SymbolFlags::VALUE;
        let Some(target) = self.resolve_entity_name(reference, meaning) else { return };
        let target = self.binder.merged_symbol(target);
        if !self.get_symbol_flags(target).intersects(SymbolFlags::TYPE) {
            return;
        }
        self.check_type_name_is_reserved(
            name.node_id,
            name.text,
            &messages::IMPORT_NAME_CANNOT_BE_0,
        );
    }

    /// TS1046 — `Top-level declarations in .d.ts files must start with either
    /// a 'declare' or 'export' modifier.`
    ///
    /// `checkGrammarSourceFile` (`grammarchecks.go:2043`) and the loop beneath
    /// it. Run **from the file** rather than from a dispatch arm, as upstream
    /// does, because the exemption list is about *top-level* position: a
    /// declaration inside a namespace body needs no `declare`, its container is
    /// already ambient, and a per-node arm would have to re-derive "is this a
    /// direct child of the file" at every statement. §265.
    fn check_top_level_declare_modifiers(&mut self, file: NodeId, ambient: bool) {
        if !ambient || self.file_has_parse_errors {
            return;
        }
        let Some(Node::SourceFile(source)) = self.node_map.get(file) else { return };
        for statement in source.statements {
            let Some(id) = statement.node_id() else { continue };
            // `ast.IsDeclarationNode(decl) || decl.Kind == KindVariableStatement`,
            // narrowed to the statement kinds that reach a `.d.ts` top level.
            let modifiers = match statement {
                tsr_ast::Statement::FunctionDeclaration(n) => n.modifiers,
                tsr_ast::Statement::ClassDeclaration(n) => n.modifiers,
                tsr_ast::Statement::EnumDeclaration(n) => n.modifiers,
                tsr_ast::Statement::ModuleDeclaration(n) => n.modifiers,
                tsr_ast::Statement::VariableStatement(n) => n.modifiers,
                // `interface`, `type`, `import`, `import =`, `export …`,
                // `export =` and `export as namespace` are all exempt by kind
                // (`grammarchecks.go:2025`).
                _ => continue,
            };
            if modifiers.iter().any(|modifier| {
                matches!(
                    modifier,
                    tsr_ast::ModifierLike::Token(token)
                        if matches!(
                            token.kind,
                            SyntaxKind::DeclareKeyword
                                | SyntaxKind::ExportKeyword
                                | SyntaxKind::DefaultKeyword
                        )
                )
            }) {
                continue;
            }
            let Some(file_id) = self.source_file_of_for_diagnostics(id) else { continue };
            // `grammarErrorOnFirstToken` — the node's own start, which is the
            // first token's start once trivia is skipped.
            let span = self.nodes.span(id);
            self.report(
                file_id,
                Diagnostic::new(
                    &messages::TOP_LEVEL_DECLARATIONS_IN_D_TS_FILES_MUST_START_WITH_EITHER_A_DECLARE_OR_EXPORT_MODIFIER,
                    span,
                ),
            );
            // `checkGrammarTopLevelElementsForRequiredDeclareModifier` returns
            // on the **first** offender.
            return;
        }
    }

    /// TS1039 — `Initializers are not allowed in ambient contexts.`
    /// TS1254 — `A 'const' initializer in an ambient context must be a string
    /// or numeric literal or literal enum reference.`
    ///
    /// `checkGrammarVariableLikeDeclaration`'s tail (`grammarchecks.go:1963`):
    ///
    /// ```go
    /// isInvalidInitializer := !(isInitializerStringOrNumberLiteralExpression(initializer) ||
    ///     c.isInitializerSimpleLiteralEnumReference(initializer) ||
    ///     initializer.Kind == ast.KindTrueKeyword || initializer.Kind == ast.KindFalseKeyword ||
    ///     isInitializerBigIntLiteralExpression(initializer))
    /// isConstOrReadonly := isDeclarationReadonly(node) || ast.IsVariableDeclaration(node) && c.isVarConstLike(node)
    /// if isConstOrReadonly && typeNode == nil {
    ///     if isInvalidInitializer { … A_const_initializer_in_an_ambient_context… }
    /// } else {
    ///     … Initializers_are_not_allowed_in_ambient_contexts
    /// }
    /// ```
    ///
    /// **The annotation is what decides which message.** `declare const x = 1`
    /// is legal, `declare const x: number = 1` is TS1039, and the difference is
    /// the presence of `typeNode` rather than anything about the initialiser.
    ///
    /// The valid-initializer predicate, `isInitializerSimpleLiteralEnumReference`
    /// included, is [`Checker::is_valid_ambient_const_initializer`]
    /// (`grammar.rs`). §259 declined property accesses and identifiers while
    /// the enum-reference arm was unported; that decline is gone
    /// (`docs/parity/notes/r4-unused-grammar.md` §6).
    fn check_ambient_initializer(
        &mut self,
        node: NodeId,
        initializer: Option<tsr_ast::Expression<'_>>,
        annotation: Option<tsr_ast::TypeNode<'_>>,
        ambient: bool,
    ) {
        // **A member's own `declare` is invisible to the walk-threaded flag**
        // (§81/§135), so `class C { declare foo = 1 }` reached this rule with
        // `ambient == false`. Read the modifier here, as §508 did for TS1038.
        // §650.
        let own_declare = self
            .node_map
            .get(node)
            .and_then(modifiers_of)
            .is_some_and(|modifiers| has_modifier(modifiers, SyntaxKind::DeclareKeyword));
        if (!ambient && !own_declare) || self.file_has_parse_errors {
            return;
        }
        let Some(initializer) = initializer else { return };
        let Some(initializer_id) = initializer.node_id() else { return };
        // `isVarConstLike` — the `const` flag is on the **list**, not the
        // declaration, exactly as `check_const_is_initialized` reads it.
        let is_const_like = self.nodes.kind(node) == SyntaxKind::VariableDeclaration
            && self
                .nodes
                .parent(node)
                .is_some_and(|list| self.nodes.flags(list).intersects(tsr_ast::NodeFlags::CONST));
        let is_const_or_readonly = self.declaration_is_readonly(node) || is_const_like;
        let Some(file) = self.source_file_of_for_diagnostics(initializer_id) else { return };
        let span = self.nodes.span(initializer_id);
        if is_const_or_readonly && annotation.is_none() {
            if !self.is_valid_ambient_const_initializer(initializer) {
                self.report(
                    file,
                    Diagnostic::new(
                        &messages::A_CONST_INITIALIZER_IN_AN_AMBIENT_CONTEXT_MUST_BE_A_STRING_OR_NUMERIC_LITERAL_OR_LITERAL_ENUM_REFERENCE,
                        span,
                    ),
                );
            }
            return;
        }
        self.report(
            file,
            Diagnostic::new(&messages::INITIALIZERS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS, span),
        );
    }

    /// `isDeclarationReadonly` — a `readonly` modifier on the declaration.
    pub(crate) fn declaration_is_readonly(&self, node: NodeId) -> bool {
        let modifiers = match self.node_map.get(node) {
            Some(Node::PropertyDeclaration(property)) => property.modifiers,
            _ => return false,
        };
        modifiers.iter().any(|modifier| {
            matches!(modifier, tsr_ast::ModifierLike::Token(m) if m.kind == SyntaxKind::ReadonlyKeyword)
        })
    }

    /// TS2403 / TS2717 — `Subsequent variable (property) declarations must
    /// have the same type.` — and TS2687 — `All declarations of '{0}' must
    /// have identical modifiers.`
    ///
    /// `checkVariableLikeDeclaration`'s merged-declaration arms
    /// (`checker.go:5893-5935`), for variable declarations, parameters,
    /// binding elements, property declarations and property signatures. On
    /// the symbol's primary
    /// declaration (`symbol.ValueDeclaration`): TS2687 when another
    /// variable-like declaration's flags differ (`areDeclarationFlagsIdentical`).
    /// On a secondary one: `t := getTypeOfSymbol(symbol)` against the
    /// declaration's own widened type, TS2403/TS2717 when `!isTypeIdenticalTo`
    /// ([`Self::is_type_identical_to`], `crate::identity`), then TS2687 against
    /// the primary. A type pair the identity relation cannot decide is not
    /// reported.
    ///
    /// A parameter property's symbol is the class member it declares (the
    /// binder lets the property win as the node's symbol, as upstream's
    /// `bindParameter` does), so `y: number; constructor(private y: number)`
    /// is a secondary declaration with different modifiers — TS2687.
    ///
    /// No parse-error gate: upstream checks a file with syntax errors too
    /// (`asyncArrowFunction9_es2017`, `negateOperatorInvalidOperations`).
    ///
    /// **`any` and `unknown` are trusted only where written.** In this port
    /// `any` is *"no better answer"* as often as it is the type the user wrote
    /// (§338; §865 measured −34 cases admitting it unconditionally), so a
    /// top-level `any`/`unknown` takes part only when the declaration's own
    /// annotation is that keyword.
    fn check_subsequent_declaration_type(&mut self, node: NodeId) {
        let (name, annotated, is_property) = match self.node_map.get(node) {
            Some(Node::VariableDeclaration(declaration)) => (
                declaration.name.as_ref().and_then(tsr_ast::BindingName::node_id),
                declaration.r#type.is_some(),
                false,
            ),
            Some(Node::ParameterDeclaration(declaration)) => (
                declaration.name.as_ref().and_then(tsr_ast::BindingName::node_id),
                declaration.r#type.is_some(),
                false,
            ),
            Some(Node::BindingElement(element)) => {
                (element.name.as_ref().and_then(tsr_ast::BindingName::node_id), false, false)
            }
            Some(Node::PropertyDeclaration(declaration)) => {
                (declaration.name.node_id(), declaration.r#type.is_some(), true)
            }
            Some(Node::PropertySignatureDeclaration(declaration)) => {
                (declaration.name.node_id(), declaration.r#type.is_some(), true)
            }
            _ => return,
        };
        let Some(name) = name else { return };
        // `scanner.DeclarationNameToString` for the name kinds that reach the
        // merge arms (a binding pattern returned earlier upstream). Upstream
        // reads the source text; a string literal is respelled with double
        // quotes here, which only the message argument can tell apart.
        let text = match self.node_map.get(name) {
            Some(Node::Identifier(identifier)) => identifier.text.to_string(),
            Some(Node::PrivateIdentifier(identifier)) => identifier.text.to_string(),
            Some(Node::StringLiteral(literal)) => format!("\"{}\"", literal.text),
            Some(Node::NumericLiteral(literal)) => literal.text.to_string(),
            _ => return,
        };
        let Some(own) = self.binder.symbol_of(node) else { return };
        // **A merge the excludes forbid did not happen upstream.**
        // `mergeSymbol` reports `reportMergeSymbolError` and leaves the source
        // unmerged (`checker.go:14199`), so `getSymbolOfDeclaration` answers
        // the file's own symbol — `var eval` in a script stays apart from
        // `lib.d.ts`'s `function eval`. This port's binder records the conflict
        // but still maps the source to the target (`Binder::merge_globals`), so
        // the conflict list is what restores the unmerged symbol.
        let symbol = if self.binder.merge_conflicts().iter().any(|&(_, source)| source == own) {
            own
        } else {
            self.binder.merged_symbol(own)
        };
        // `symbol.ValueDeclaration` is the primary.
        let Some(primary) = self.binder.symbols().get(symbol).value_declaration else { return };
        if primary == node {
            // `checker.go:5916`: a primary with siblings whose modifiers
            // differ reports on itself.
            let declarations = self.binder.symbols().get(symbol).declarations.clone();
            if declarations.len() > 1
                && declarations.iter().any(|&other| {
                    other != node
                        && self.is_variable_like(other)
                        && !self.declaration_flags_identical(other, node)
                })
            {
                self.report_identical_modifiers(name, &text);
            }
            return;
        }
        // `symbol.Flags&ast.SymbolFlagsAssignment == 0` (`checker.go:5929`). A
        // JavaScript assignment declaration — `exports.x = …`, `this.x = …` —
        // is not a second *declaration* of a type, and upstream excludes it
        // here rather than in the caller. `jsFileCompilationBindErrors` is the
        // one line this rule got wrong without it. §258.
        if self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::ASSIGNMENT) {
            return;
        }
        // A block-scoped redeclaration is TS2451 and gets a symbol of its own
        // upstream (`declareSymbol`'s excludes); only `var`s and parameters
        // merge into one symbol with secondary declarations.
        if [own, symbol].into_iter().any(|symbol| {
            self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::BLOCK_SCOPED_VARIABLE)
        }) {
            return;
        }
        self.check_subsequent_declaration_identity(
            node,
            symbol,
            primary,
            name,
            &text,
            annotated,
            is_property,
        );
        // `checker.go:5933`.
        if !self.declaration_flags_identical(node, primary) {
            self.report_identical_modifiers(name, &text);
        }
    }

    /// The identity half of the secondary-declaration arm: TS2403, or TS2717
    /// for a property (`errorNextVariableOrPropertyDeclarationMustHaveSameType`).
    #[allow(clippy::too_many_arguments)]
    fn check_subsequent_declaration_identity(
        &mut self,
        node: NodeId,
        symbol: tsr_binder::SymbolId,
        primary: NodeId,
        name: NodeId,
        text: &str,
        annotated: bool,
        is_property: bool,
    ) {
        let first = self.get_type_of_symbol(symbol);
        let next = self.get_widened_type_for_variable_like_declaration(node);
        if self.is_error(first) || self.is_error(next) {
            return;
        }
        // `widenTypeForVariableLikeDeclaration` (`checker.go:18246`) turns a
        // `symbol`-typed member of the global `SymbolConstructor` into that
        // member's `unique symbol`, on both sides here (typescript-go#1212).
        // This port's widening lacks the special case, so the pair declines.
        if is_property
            && [first, next].into_iter().any(|ty| {
                self.type_of(ty)
                    .flags
                    .intersects(TypeFlags::ES_SYMBOL | TypeFlags::UNIQUE_ES_SYMBOL)
            })
            && self.is_global_symbol_constructor(self.nodes.parent(node))
        {
            return;
        }
        if !self.identity_side_is_trusted(first, primary)
            || !self.identity_side_is_trusted(next, node)
        {
            return;
        }
        // **The structural arm is trusted only between written types.** Full
        // identity is exact, so a negative it gives between *inferred*
        // initializer types inherits every near-miss of the subsystems that
        // inferred them (array-literal widening, overload choice, subtype
        // reduction); between annotations it compares what the user wrote.
        // Inferred operands keep the earlier necessary condition, mutual
        // assignability. `docs/parity/notes/decls.md` §2.
        let identity = if self.declaration_type_annotation(primary).is_some() && annotated {
            self.is_type_identical_to(first, next)
        } else {
            self.is_type_identical_to_by_assignability(first, next)
        };
        if identity != Ternary::NotRelated {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(name) else { return };
        let span = self.error_span(name);
        let (Some(first_text), Some(next_text)) =
            (self.type_to_string_at(first, node), self.type_to_string_at(next, node))
        else {
            return;
        };
        let message = if is_property {
            &messages::SUBSEQUENT_PROPERTY_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_PROPERTY_0_MUST_BE_OF_TYPE_1_BUT_HERE_HAS_TYPE_2
        } else {
            &messages::SUBSEQUENT_VARIABLE_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_VARIABLE_0_MUST_BE_OF_TYPE_1_BUT_HERE_HAS_TYPE_2
        };
        self.report(
            file,
            Diagnostic::with_args(message, span, [text.to_string(), first_text, next_text]),
        );
    }

    /// `isGlobalSymbolConstructor`: the node is a declaration of the global
    /// `SymbolConstructor` interface.
    fn is_global_symbol_constructor(&self, node: Option<NodeId>) -> bool {
        let Some(node) = node else { return false };
        if self.nodes.kind(node) != SyntaxKind::InterfaceDeclaration {
            return false;
        }
        let Some(symbol) = self.binder.symbol_of(node) else { return false };
        self.global_type_symbol_with_arity("SymbolConstructor", 0).is_some_and(|global| {
            self.binder.merged_symbol(global) == self.binder.merged_symbol(symbol)
        })
    }

    /// TS2687 at a declaration's name.
    fn report_identical_modifiers(&mut self, name: NodeId, text: &str) {
        let Some(file) = self.source_file_of_for_diagnostics(name) else { return };
        let span = self.error_span(name);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::ALL_DECLARATIONS_OF_0_MUST_HAVE_IDENTICAL_MODIFIERS,
                span,
                [text.to_string()],
            ),
        );
    }

    /// `ast.IsVariableLike`.
    fn is_variable_like(&self, node: NodeId) -> bool {
        matches!(
            self.nodes.kind(node),
            SyntaxKind::BindingElement
                | SyntaxKind::EnumMember
                | SyntaxKind::Parameter
                | SyntaxKind::PropertyAssignment
                | SyntaxKind::PropertyDeclaration
                | SyntaxKind::PropertySignature
                | SyntaxKind::ShorthandPropertyAssignment
                | SyntaxKind::VariableDeclaration
        )
    }

    /// `areDeclarationFlagsIdentical` (`checker.go:6864`): optionality, then
    /// the private/protected/async/abstract/readonly/static modifiers. A
    /// parameter and a variable may differ in optionality.
    fn declaration_flags_identical(&self, left: NodeId, right: NodeId) -> bool {
        const INTERESTING: [SyntaxKind; 6] = [
            SyntaxKind::PrivateKeyword,
            SyntaxKind::ProtectedKeyword,
            SyntaxKind::AsyncKeyword,
            SyntaxKind::AbstractKeyword,
            SyntaxKind::ReadonlyKeyword,
            SyntaxKind::StaticKeyword,
        ];
        let kinds = (self.nodes.kind(left), self.nodes.kind(right));
        if matches!(
            kinds,
            (SyntaxKind::Parameter, SyntaxKind::VariableDeclaration)
                | (SyntaxKind::VariableDeclaration, SyntaxKind::Parameter)
        ) {
            return true;
        }
        if self.declaration_has_question_token(left) != self.declaration_has_question_token(right) {
            return false;
        }
        let selected = |node: NodeId| {
            let modifiers = self.node_map.get(node).and_then(modifiers_of).unwrap_or_default();
            INTERESTING.map(|kind| has_modifier(modifiers, kind))
        };
        selected(left) == selected(right)
    }

    /// `isOptionalDeclaration` for the variable-like kinds: a `?` postfix (a
    /// parameter's `?` too).
    fn declaration_has_question_token(&self, node: NodeId) -> bool {
        let question = |token: Option<&tsr_ast::Token<'_>>| {
            token.is_some_and(|token| token.kind == SyntaxKind::QuestionToken)
        };
        match self.node_map.get(node) {
            Some(Node::PropertyDeclaration(declaration)) => question(declaration.postfix_token),
            Some(Node::PropertySignatureDeclaration(declaration)) => {
                question(declaration.postfix_token)
            }
            Some(Node::ParameterDeclaration(declaration)) => question(declaration.question_token),
            _ => false,
        }
    }

    /// The written type annotation of a variable-like declaration.
    fn declaration_type_annotation(&self, declaration: NodeId) -> Option<NodeId> {
        match self.node_map.get(declaration) {
            Some(Node::VariableDeclaration(variable)) => variable.r#type?.node_id(),
            Some(Node::ParameterDeclaration(parameter)) => parameter.r#type?.node_id(),
            Some(Node::PropertyDeclaration(property)) => property.r#type?.node_id(),
            Some(Node::PropertySignatureDeclaration(property)) => property.r#type?.node_id(),
            _ => None,
        }
    }

    /// A top-level `any`/`unknown` is an identity operand only when the
    /// declaration's written annotation is that keyword.
    fn identity_side_is_trusted(&self, ty: TypeId, declaration: NodeId) -> bool {
        let flags = self.type_of(ty).flags;
        if !flags.intersects(TypeFlags::ANY_OR_UNKNOWN) {
            return true;
        }
        let annotation = match self.node_map.get(declaration) {
            // **No annotation and no initializer is a written `any` too.**
            // `getTypeForVariableLikeDeclaration` answers `anyType` for a
            // `var x;` of a variable statement (or `autoType`, which the
            // secondary-declaration arm turns back into `any` through
            // `convertAutoToAny`, `checker.go:5932`). Nothing here is a
            // fallback: the declaration has no other source of a type. A
            // `for (var x in …)` / `for (var x of …)` declaration also has
            // neither, but takes its type from the loop, so only a variable
            // statement's list qualifies. `docs/parity/notes/decls.md` §1.
            Some(Node::VariableDeclaration(variable))
                if variable.r#type.is_none()
                    && variable.initializer.is_none()
                    && flags.contains(TypeFlags::ANY) =>
            {
                return self
                    .nodes
                    .parent(declaration)
                    .and_then(|list| self.nodes.parent(list))
                    .is_some_and(|statement| {
                        self.nodes.kind(statement) == SyntaxKind::VariableStatement
                    });
            }
            Some(Node::VariableDeclaration(variable)) => variable.r#type,
            Some(Node::ParameterDeclaration(parameter)) => parameter.r#type,
            Some(Node::PropertyDeclaration(property)) => property.r#type,
            Some(Node::PropertySignatureDeclaration(property)) => property.r#type,
            _ => None,
        };
        let Some(annotation) = annotation.and_then(|a| a.node_id()) else { return false };
        let keyword = if flags.contains(TypeFlags::ANY) {
            SyntaxKind::AnyKeyword
        } else {
            SyntaxKind::UnknownKeyword
        };
        self.nodes.kind(annotation) == keyword
    }

    /// A mapped type, read through its alias body and apparent type the way
    /// `get_property_names_of_type` declines composite mapped metadata.
    pub(crate) fn is_mapped_for_identity(&mut self, ty: TypeId) -> bool {
        let body = self.binding_type_alias_body(ty);
        let apparent = self.apparent_type(body);
        [ty, body, apparent].into_iter().any(|id| {
            self.mapped_types.contains_key(&id)
                || self.mapped_identity_optionality.contains_key(&id)
        })
    }

    /// An intrinsic primitive: a singleton id, so inequality *is* non-identity.
    fn is_decidable_primitive(&self, id: crate::types::TypeId) -> bool {
        // **`any` may never join this list, and neither may the rest.** §865
        // widened it to all eleven intrinsic singletons on the argument that
        // §257 made for the four — inequality is non-identity for a singleton —
        // and measured **−33 cases**; `any` alone measured **−34**. The reason
        // is §338's: in this port `any` is *"no better answer"* as often as it
        // is the type the user wrote, so comparing it for identity compares a
        // verdict with a type. The four below are the ones this port only ever
        // produces deliberately.
        [
            self.intrinsics.string,
            self.intrinsics.number,
            self.intrinsics.bigint,
            self.intrinsics.boolean,
        ]
        .contains(&id)
    }

    /// TS1015 — `Parameter cannot have question mark and initializer.`
    ///
    /// `checkGrammarParameterList` (`grammarchecks.go:711`), the optional arm.
    /// The error node is the parameter's **name**.
    ///
    /// A **rest** parameter takes an earlier branch (`A rest parameter cannot
    /// be optional`), so the rest test is the arm's guard rather than a bound
    /// this port chose — §103's rule that the `else if` order is the
    /// specification. §180.
    /// The kind a base class or any of its own bases declares `name` as.
    ///
    /// `getPropertiesOfType(baseType)` includes inherited members; this walks
    /// the `extends` chain to the same effect, bounded because the corpus
    /// contains cyclic heritage. §708.
    fn base_member_kind(&mut self, base: NodeId, name: &str) -> Option<MemberKind> {
        let mut at = Some(base);
        for _ in 0..MAX_ALIAS_HOPS {
            let current = at?;
            let Some(Node::ClassDeclaration(class)) = self.node_map.get(current) else {
                return None;
            };
            if let Some((_, kind, _)) = class
                .members
                .iter()
                .filter_map(|member| class_member_shape(*member))
                .find(|(seen, _, _)| *seen == name)
            {
                return Some(kind);
            }
            let next = class
                .heritage_clauses
                .iter()
                .find(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)
                .and_then(|clause| clause.types.first())
                .and_then(|base| base.expression)
                .and_then(|expression| expression.node_id())
                .and_then(|id| {
                    let text = self.identifier_text(id).map(str::to_string)?;
                    self.binder.resolve_name(
                        self.nodes,
                        self.node_map,
                        id,
                        &text,
                        SymbolFlags::CLASS,
                    )
                })
                .and_then(|symbol| {
                    self.binder.symbols().get(self.binder.merged_symbol(symbol)).value_declaration
                });
            at = next;
        }
        None
    }

    /// TS2610 / TS2611 — a member overridden as the *other* kind.
    ///
    /// `checkKindsOfPropertyMemberOverrides` (`checker.go:4626`). Upstream
    /// reaches the pair through `getPropertiesOfType(baseType)`, but the
    /// condition is about **declaration kinds** — a base property overridden by
    /// a derived accessor, or the reverse — and both are in the tree once the
    /// base class's declaration is resolved. §309.
    fn check_override_kind(&mut self, node: NodeId, typed: Node<'_>) {
        if self.file_has_parse_errors {
            return;
        }
        let (clauses, members) = match typed {
            Node::ClassDeclaration(class) => (class.heritage_clauses, class.members),
            _ => return,
        };
        let Some(extends) =
            clauses.iter().find(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)
        else {
            return;
        };
        let Some(base) = extends.types.first() else { return };
        let Some(tsr_ast::Expression::Identifier(name)) = base.expression else { return };
        let Some(at) = name.node_id else { return };
        let Some(symbol) =
            self.binder.resolve_name(self.nodes, self.node_map, at, name.text, SymbolFlags::CLASS)
        else {
            return;
        };
        let symbol = self.binder.merged_symbol(symbol);
        let Some(declaration) = self.binder.symbols().get(symbol).value_declaration else { return };
        if !matches!(self.node_map.get(declaration), Some(Node::ClassDeclaration(_))) {
            return;
        }
        let _ = node;
        // **A `get`/`set` pair is one symbol upstream and two declarations
        // here.** `accessorsOverrideProperty` wants one TS2611 per name and
        // this reported one per accessor — six wrong lines, every one the
        // second half of a pair. §310.
        let mut reported: Vec<&str> = Vec::new();
        // **A parameter property is a member.** `constructor(public p: string)`
        // declares `p` on the class, and it is not in `members` — it is a
        // parameter of a constructor that is. §708.
        let mut shapes: Vec<(&str, MemberKind, NodeId)> =
            members.iter().filter_map(|member| class_member_shape(*member)).collect();
        for member in members {
            let tsr_ast::ClassElement::ConstructorDeclaration(constructor) = *member else {
                continue;
            };
            for parameter in constructor.parameters {
                let modifiers = parameter.modifiers;
                let is_parameter_property =
                    tsr_ast::has_syntactic_modifier(modifiers, SyntaxKind::PublicKeyword)
                        || tsr_ast::has_syntactic_modifier(modifiers, SyntaxKind::PrivateKeyword)
                        || tsr_ast::has_syntactic_modifier(modifiers, SyntaxKind::ProtectedKeyword)
                        || tsr_ast::has_syntactic_modifier(modifiers, SyntaxKind::ReadonlyKeyword);
                if !is_parameter_property {
                    continue;
                }
                let Some(tsr_ast::BindingName::Identifier(name)) = parameter.name else { continue };
                let Some(at) = name.node_id else { continue };
                shapes.push((name.text, MemberKind::Property, at));
            }
        }
        for (derived_name, derived_kind, derived_at) in shapes {
            // `getPropertiesOfType(baseType)` includes **inherited** members,
            // so the base chain is walked rather than the immediate base
            // alone: `class C extends B extends A` finds `A`'s accessor with
            // `B` empty. Bounded, because the corpus contains cyclic
            // heritage. §708.
            let Some(base_kind) = self.base_member_kind(declaration, derived_name) else {
                continue;
            };
            // The method arms (`checker.go:4728-4740`): a method overridden by
            // an accessor, or a property or accessor overridden by a method.
            // Their arguments are ordered base class, member, derived class.
            // A method overridden by a property is the one correct mixed case.
            let (message, method_arm) = match (base_kind, derived_kind) {
                (MemberKind::Property, MemberKind::Accessor) => (
                    &messages::_0_IS_DEFINED_AS_A_PROPERTY_IN_CLASS_1_BUT_IS_OVERRIDDEN_HERE_IN_2_AS_AN_ACCESSOR,
                    false,
                ),
                (MemberKind::Accessor, MemberKind::Property) => (
                    &messages::_0_IS_DEFINED_AS_AN_ACCESSOR_IN_CLASS_1_BUT_IS_OVERRIDDEN_HERE_IN_2_AS_AN_INSTANCE_PROPERTY,
                    false,
                ),
                (MemberKind::Method, MemberKind::Accessor) => (
                    &messages::CLASS_0_DEFINES_INSTANCE_MEMBER_FUNCTION_1_BUT_EXTENDED_CLASS_2_DEFINES_IT_AS_INSTANCE_MEMBER_ACCESSOR,
                    true,
                ),
                (MemberKind::Accessor, MemberKind::Method) => (
                    &messages::CLASS_0_DEFINES_INSTANCE_MEMBER_ACCESSOR_1_BUT_EXTENDED_CLASS_2_DEFINES_IT_AS_INSTANCE_MEMBER_FUNCTION,
                    true,
                ),
                (MemberKind::Property, MemberKind::Method) => (
                    &messages::CLASS_0_DEFINES_INSTANCE_MEMBER_PROPERTY_1_BUT_EXTENDED_CLASS_2_DEFINES_IT_AS_INSTANCE_MEMBER_FUNCTION,
                    true,
                ),
                _ => continue,
            };
            if reported.contains(&derived_name) {
                continue;
            }
            reported.push(derived_name);
            if method_arm {
                let Some(file) = self.source_file_of_for_diagnostics(derived_at) else { continue };
                let span = self.nodes.span(derived_at);
                let derived_text = match typed {
                    Node::ClassDeclaration(class) => class.name.map(|name| name.text.to_string()),
                    _ => None,
                };
                let Some(derived_text) = derived_text else { continue };
                self.report(
                    file,
                    Diagnostic::with_args(
                        message,
                        span,
                        [name.text.to_string(), derived_name.to_string(), derived_text],
                    ),
                );
                continue;
            }
            let Some(file) = self.source_file_of_for_diagnostics(derived_at) else { continue };
            let span = self.nodes.span(derived_at);
            let base_text = name.text.to_string();
            self.report(
                file,
                Diagnostic::with_args(
                    message,
                    span,
                    [derived_name.to_string(), base_text.clone(), base_text],
                ),
            );
        }
    }

    /// TS2481 — `Cannot initialize outer scoped variable '{0}' in the same
    /// scope as block scoped declaration '{1}'.`
    ///
    /// `checkVariableLikeDeclaration`'s tail (`checker.go:5995`). Upstream's own
    /// comment on `namesShareScope` is the rule: *names of block-scoped and
    /// function-scoped variables can collide only if the block-scoped one is
    /// defined in the function/module/source-file scope, because of hoisting*.
    /// A `var` whose name resolves to a `let` in a **narrower block** is this
    /// error; one where both share a hoisting scope is a duplicate identifier
    /// and a different code. §298.
    fn check_outer_scoped_variable(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        // **A binding element declares a variable too.** `var { x } = …` puts
        // an object pattern on the `VariableDeclaration` and declares `x` on
        // the element inside it; upstream reaches both through
        // `checkVariableLikeDeclaration`, which takes `node.Name()` for either.
        // §718.
        let name = match self.node_map.get(node) {
            Some(Node::VariableDeclaration(declaration)) => declaration.name,
            Some(Node::BindingElement(element)) => element.name,
            _ => return,
        };
        let Some(tsr_ast::BindingName::Identifier(name)) = name else { return };
        // **`c.error(node, …)` where upstream's `node` is the name.** For
        // `var x` the declaration and its name start at the same column and the
        // two readings are indistinguishable; `var { x: x = 0 }` separates
        // them — the element starts at the *property* name and upstream reports
        // at the bound one. §718.
        let at = name.node_id.unwrap_or(node);
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        let symbol = self.binder.merged_symbol(symbol);
        if !self
            .binder
            .symbols()
            .get(symbol)
            .flags
            .intersects(SymbolFlags::FUNCTION_SCOPED_VARIABLE)
        {
            return;
        }
        let Some(local) = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            node,
            name.text,
            SymbolFlags::VARIABLE,
        ) else {
            return;
        };
        let local = self.binder.merged_symbol(local);
        if local == symbol
            || !self
                .binder
                .symbols()
                .get(local)
                .flags
                .intersects(SymbolFlags::BLOCK_SCOPED_VARIABLE)
        {
            return;
        }
        let Some(value_declaration) = self.binder.symbols().get(local).value_declaration else {
            return;
        };
        // `FindAncestorKind(…, KindVariableDeclarationList)`, then the
        // statement's parent — the container the `let` actually lives in.
        let Some(list) = self
            .nodes
            .ancestors(value_declaration)
            .find(|&it| self.nodes.kind(it) == SyntaxKind::VariableDeclarationList)
        else {
            return;
        };
        if !self.nodes.flags(list).intersects(tsr_ast::NodeFlags::BLOCK_SCOPED) {
            return;
        }
        // **A `nil` container is a reason to report, not a reason to stop.**
        // Upstream (`checker.go:6003`) leaves `container` nil when the list's
        // parent is not a variable statement, and `namesShareScope` is
        // `container != nil && …` — so nil falls straight through to the
        // error. A `let` in a `for` initializer is exactly that shape: its
        // list hangs off the `ForStatement`. Written as an early return, this
        // read as *"a shape I cannot judge"* where upstream means *"a shape
        // that always loses"*. §716.
        let container = self
            .nodes
            .parent(list)
            .filter(|&statement| self.nodes.kind(statement) == SyntaxKind::VariableStatement)
            .and_then(|statement| self.nodes.parent(statement));
        let names_share_scope =
            container.is_some_and(|container| match self.nodes.kind(container) {
                SyntaxKind::Block => self
                    .nodes
                    .parent(container)
                    .is_some_and(|owner| self.is_function_like_or_static_block(owner)),
                SyntaxKind::ModuleBlock
                | SyntaxKind::ModuleDeclaration
                | SyntaxKind::SourceFile => true,
                _ => false,
            });
        if names_share_scope {
            return;
        }
        let text = self.binder.symbols().get(local).name.to_string();
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.nodes.span(at);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::CANNOT_INITIALIZE_OUTER_SCOPED_VARIABLE_0_IN_THE_SAME_SCOPE_AS_BLOCK_SCOPED_DECLARATION_1,
                span,
                [text.clone(), text],
            ),
        );
    }

    /// TS1108 — `A 'return' statement can only be used within a function body.`
    /// TS1107 — `A 'return' statement cannot be used inside a class static block.`
    ///
    /// `checkReturnStatement` (`checker.go:4098`), after its ambient guard:
    /// `getContainingFunctionOrClassStaticBlock` is a class static block, or it
    /// is nothing at all. Both branches ported — the static-block one has no
    /// blocked cases and is free, and shipping one branch of a two-branch `if`
    /// is §230's shape. §296.
    fn check_return_container(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors {
            return;
        }
        let container = self
            .nodes
            .ancestors(node)
            .find(|&ancestor| self.is_function_like_or_static_block(ancestor));
        let message = match container {
            Some(container)
                if self.nodes.kind(container) == SyntaxKind::ClassStaticBlockDeclaration =>
            {
                &messages::A_RETURN_STATEMENT_CANNOT_BE_USED_INSIDE_A_CLASS_STATIC_BLOCK
            }
            Some(_) => return,
            None => &messages::A_RETURN_STATEMENT_CAN_ONLY_BE_USED_WITHIN_A_FUNCTION_BODY,
        };
        // `grammarErrorOnFirstToken(node, …)` — the `return` keyword.
        self.report_grammar_at(Some(node), message);
    }

    /// `checkGrammarClassLikeDeclaration` and
    /// `checkGrammarInterfaceDeclaration`'s heritage walks
    /// (`grammarchecks.go:898`, `:955`).
    ///
    /// The class loop is ordered and the order *is* the rule: `extends` after
    /// an `extends` is TS1172, `extends` after an `implements` is TS1173, a
    /// second type inside one `extends` is TS1174 — reported on `typeNodes[1]`,
    /// not the clause — and a repeated `implements` is TS1175. The interface
    /// loop is the same walk with `implements` rejected outright (TS1176).
    ///
    /// A single `extends` naming several types is legal for an *interface* and
    /// is TS1174 only for a class, which is why the two loops are separate
    /// upstream and here. §289.
    fn check_grammar_heritage_clauses(&mut self, typed: Node<'_>) {
        if self.file_has_parse_errors {
            return;
        }
        let (clauses, is_class) = match typed {
            Node::ClassDeclaration(n) => (n.heritage_clauses, true),
            Node::ClassExpression(n) => (n.heritage_clauses, true),
            Node::InterfaceDeclaration(n) => (n.heritage_clauses, false),
            _ => return,
        };
        let mut seen_extends = false;
        let mut seen_implements = false;
        for clause in clauses {
            let at = clause.token.node_id;
            match clause.token.kind {
                SyntaxKind::ExtendsKeyword => {
                    if seen_extends {
                        self.report_grammar_at(at, &messages::EXTENDS_CLAUSE_ALREADY_SEEN);
                        return;
                    }
                    if is_class {
                        if seen_implements {
                            self.report_grammar_at(
                                at,
                                &messages::EXTENDS_CLAUSE_MUST_PRECEDE_IMPLEMENTS_CLAUSE,
                            );
                            return;
                        }
                        if let Some(second) = clause.types.get(1) {
                            self.report_grammar_at(
                                second.node_id,
                                &messages::CLASSES_CAN_ONLY_EXTEND_A_SINGLE_CLASS,
                            );
                            return;
                        }
                    }
                    seen_extends = true;
                }
                SyntaxKind::ImplementsKeyword => {
                    if !is_class {
                        self.report_grammar_at(
                            at,
                            &messages::INTERFACE_DECLARATION_CANNOT_HAVE_IMPLEMENTS_CLAUSE,
                        );
                        return;
                    }
                    if seen_implements {
                        self.report_grammar_at(at, &messages::IMPLEMENTS_CLAUSE_ALREADY_SEEN);
                        return;
                    }
                    seen_implements = true;
                }
                _ => {}
            }
        }
    }

    /// `checkGrammarParameterList` (`grammarchecks.go:691`) — the arms beside
    /// §103's TS1015.
    ///
    /// One loop with a `seenOptionalParameter` flag, returning on the **first**
    /// offender, which is why this runs once per *list* and not once per
    /// parameter. §287.
    fn check_grammar_parameter_list(&mut self, owner: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let parameters = self.parameters_of(owner);
        let count = parameters.len();
        // A JS function's `@param [x]` / `{T=}` is a reparsed `?` upstream
        // (`makeQuestionIfOptional`), which this loop reads like a written
        // one; ADR-0046.
        let reparsed = self.jsdoc_reparsed_function(owner).parameters;
        let mut seen_optional = false;
        for (index, parameter) in parameters.into_iter().enumerate() {
            let Some(Node::ParameterDeclaration(declaration)) = self.node_map.get(parameter) else {
                continue;
            };
            let name = declaration.name.as_ref().and_then(tsr_ast::BindingName::node_id);
            // The reparsed token's location is its `@param` tag's.
            let question = declaration.question_token.map(|token| token.node_id).or_else(|| {
                let slot = reparsed.get(index).filter(|slot| slot.question)?;
                Some(slot.tag?.node_id)
            });
            if let Some(rest) = declaration.dot_dot_dot_token {
                let (at, message) = if index != count - 1 {
                    (rest.node_id, &messages::A_REST_PARAMETER_MUST_BE_LAST_IN_A_PARAMETER_LIST)
                } else if let Some(question) = question {
                    (question, &messages::A_REST_PARAMETER_CANNOT_BE_OPTIONAL)
                } else if declaration.initializer.is_some() {
                    (name, &messages::A_REST_PARAMETER_CANNOT_HAVE_AN_INITIALIZER)
                } else {
                    continue;
                };
                self.report_grammar_at(at, message);
                return;
            }
            // `isOptionalDeclaration` is **`ast.HasQuestionToken` alone**
            // (`checker/utilities.go:299`) — an initialiser does *not* make a
            // parameter optional for this loop, so `f(a = 1, b: number)` is
            // legal. Reading it as "`?` or initialiser" was six wrong TS1016
            // lines, every one of them a defaulted parameter. §288.
            if question.is_some() {
                seen_optional = true;
                // TS1015 is §103's, reported once per list from its own arm.
                continue;
            }
            // `seenOptionalParameter && parameter.Initializer == nil`
            // (`grammarchecks.go:714`). **The initialiser conjunct is on this
            // arm, not on the one above**, and the two are not the same test:
            // `seenOptionalParameter` is set by a `?` alone (§288), while a
            // parameter that merely *has a default* is not "required" and so
            // never offends here.
            //
            // ```ts
            // function f(a?: string, b = false) {}   // legal — b has a default
            // function g(a?: string, b: string) {}   // TS1016
            // ```
            //
            // Missing it made every defaulted parameter after an optional one
            // an error, which is the ordinary shape of a hook signature:
            // `(items, toggle, onClick?, virtualizer?, autoFocus = false)`.
            if seen_optional && declaration.initializer.is_none() {
                self.report_grammar_at(
                    name,
                    &messages::A_REQUIRED_PARAMETER_CANNOT_FOLLOW_AN_OPTIONAL_PARAMETER,
                );
                return;
            }
        }
    }

    /// `checkGrammarAccessor`'s body arms (`grammarchecks.go:1309`, `:1315`)
    /// and parameter arms (`:1332`, `:1345`, `:1349`).
    ///
    /// The body arms sit behind `checkGrammarFunctionLikeDeclaration`'s
    /// `checkGrammarModifiers` upstream, so they wait on
    /// `modifier_chain_reported`; a report returns, as upstream's does.
    fn check_grammar_accessor(&mut self, node: NodeId, typed: Node<'_>) {
        if self.file_has_parse_errors {
            return;
        }
        if !self.modifier_chain_reported.contains(&node)
            && self.check_grammar_accessor_body(node, typed)
        {
            return;
        }
        let is_set = matches!(typed, Node::SetAccessorDeclaration(_));
        let name = match typed {
            Node::SetAccessorDeclaration(accessor) => accessor.name.node_id(),
            Node::GetAccessorDeclaration(accessor) => accessor.name.node_id(),
            _ => return,
        };
        let parameters = self.parameters_of(node);
        let wanted = usize::from(is_set);
        if parameters.len() != wanted {
            let message = if is_set {
                &messages::A_SET_ACCESSOR_MUST_HAVE_EXACTLY_ONE_PARAMETER
            } else {
                &messages::A_GET_ACCESSOR_CANNOT_HAVE_PARAMETERS
            };
            self.report_grammar_at(name, message);
            return;
        }
        if !is_set {
            return;
        }
        let Some(&parameter) = parameters.first() else { return };
        let Some(Node::ParameterDeclaration(declaration)) = self.node_map.get(parameter) else {
            return;
        };
        if let Some(rest) = declaration.dot_dot_dot_token {
            self.report_grammar_at(
                rest.node_id,
                &messages::A_SET_ACCESSOR_CANNOT_HAVE_REST_PARAMETER,
            );
        } else if let Some(question) = declaration.question_token {
            self.report_grammar_at(
                question.node_id,
                &messages::A_SET_ACCESSOR_CANNOT_HAVE_AN_OPTIONAL_PARAMETER,
            );
        }
    }

    /// `checkGrammarAccessor`'s first two arms: outside an ambient context,
    /// a type literal or an interface, a body-less accessor that is not
    /// `abstract` is "'{' expected" on its last character
    /// (`grammarErrorAtPos(accessor, accessor.End()-1, len(";"), …)`), and an
    /// `abstract` accessor with a body is TS1318. The interface/type-literal
    /// body arm (TS1183) is `check_grammar_statement_in_ambient_context`'s
    /// report here and is not repeated.
    fn check_grammar_accessor_body(&mut self, node: NodeId, typed: Node<'_>) -> bool {
        let (modifiers, has_body) = match typed {
            Node::GetAccessorDeclaration(accessor) => (accessor.modifiers, accessor.body.is_some()),
            Node::SetAccessorDeclaration(accessor) => (accessor.modifiers, accessor.body.is_some()),
            _ => return false,
        };
        let is_abstract = has_modifier(modifiers, SyntaxKind::AbstractKeyword);
        if has_body {
            if is_abstract {
                self.report_grammar(
                    node,
                    &messages::AN_ABSTRACT_ACCESSOR_CANNOT_HAVE_AN_IMPLEMENTATION,
                );
                return true;
            }
            return false;
        }
        let in_type = self.nodes.parent(node).is_some_and(|parent| {
            matches!(
                self.nodes.kind(parent),
                SyntaxKind::TypeLiteral | SyntaxKind::InterfaceDeclaration
            )
        });
        if is_abstract
            || in_type
            || self.file_is_ambient
            || self.declaration_is_in_an_ambient_context(node)
        {
            return false;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return false };
        let end = self.nodes.span(node).end;
        self.report(
            file,
            Diagnostic::with_args(
                &messages::_0_EXPECTED,
                tsr_core::Span::new(end - 1, end),
                ["{".to_string()],
            ),
        );
        true
    }

    /// `grammarErrorOnNode` at an optional node id.
    fn report_grammar_at(
        &mut self,
        node: Option<NodeId>,
        message: &'static tsr_diagnostics::Message,
    ) {
        let Some(node) = node else { return };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.nodes.span(node);
        self.report(file, Diagnostic::new(message, span));
    }

    fn check_optional_parameter_initializer(&mut self, node: NodeId) {
        if self.file_has_parse_errors || !self.parameter_has_question_and_initializer(node) {
            return;
        }
        // **Once per parameter LIST, not once per parameter.**
        // `checkGrammarParameterList` walks the list and `return`s on the first
        // offender, so `function f(y? = 1, z? = 2)` is one diagnostic upstream
        // and was two here — §103's *at most one grammar report per node*, with
        // the node being the list rather than the member.
        // `optionalArgsWithDefaultValues` is all three of the first
        // measurement's wrong lines.
        let Some(owner) = self.nodes.parent(node) else { return };
        let first = self
            .parameters_of(owner)
            .into_iter()
            .find(|&parameter| self.parameter_has_question_and_initializer(parameter));
        if first != Some(node) {
            return;
        }
        let Some(Node::ParameterDeclaration(parameter)) = self.node_map.get(node) else { return };
        let Some(name) = parameter.name.and_then(|name| name.node_id()) else { return };
        let Some(file) = self.source_file_of_for_diagnostics(name) else { return };
        let span = self.nodes.span(name);
        self.report(
            file,
            Diagnostic::new(&messages::PARAMETER_CANNOT_HAVE_QUESTION_MARK_AND_INITIALIZER, span),
        );
    }

    /// TS2524 — `'await' expressions cannot be used in a parameter initializer.`
    ///
    /// `checkGrammarAwaitOrAwaitUsing`'s last arm (`grammarchecks.go:1768`):
    ///
    /// ```go
    /// if ast.IsAwaitExpression(node) && c.isInParameterInitializerBeforeContainingFunction(node) {
    ///     // NOTE: We report this regardless as to whether there are parse diagnostics.
    ///     c.error(node, diagnostics.X_await_expressions_cannot_be_used_in_a_parameter_initializer)
    /// }
    /// ```
    ///
    /// The comment is upstream's own and is the reason this rule does not take
    /// the `file_has_parse_errors` guard every neighbouring grammar check takes.
    fn check_await_in_parameter_initializer(&mut self, node: NodeId) {
        if !self.is_in_parameter_initializer_before_containing_function(node) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.nodes.span(node);
        self.report(
            file,
            Diagnostic::new(
                &messages::AWAIT_EXPRESSIONS_CANNOT_BE_USED_IN_A_PARAMETER_INITIALIZER,
                span,
            ),
        );
    }

    /// TS1308 — `'await' expressions are only allowed within async functions
    /// and at the top levels of modules.`
    ///
    /// `checkGrammarAwaitOrAwaitUsing`'s `else` arm (`grammarchecks.go:1689`).
    /// Upstream tests `node.Flags&ast.NodeFlagsAwaitContext == 0`, a flag its
    /// **parser** sets inside an async function and which this port declares
    /// and never sets (`flags.rs`).
    ///
    /// The decidable equivalent is one modifier lookup — `AwaitContext` *is*
    /// "the nearest enclosing function is async", and that function is right
    /// here. §268/§269.
    ///
    /// # What is declined
    ///
    /// `IsInTopLevelContext`'s four messages are gated on `moduleKind` against
    /// six module kinds, `ImpliedNodeFormat` and `languageVersion >= ES2017`;
    /// the class-static-block arm has its own message. Both decline, so this
    /// reports only where the answer needs no options at all.
    fn check_await_in_non_async_function(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        // `getContainingFunctionOrClassStaticBlock`.
        // **A class field initializer is an await-context boundary.** Upstream
        // reads `NodeFlagsAwaitContext`, which the parser does not set past a
        // property declaration's initializer — it is evaluated as its own
        // function at construction — so `class { x = await f() }` inside an
        // `async` function is still an error. The container walk has to stop
        // there or it finds the enclosing async function and stays silent.
        // §613, and §515 for the same shape from the failing side.
        let Some(container) = self.nodes.ancestors(node).find(|&ancestor| {
            self.is_function_like_or_static_block(ancestor)
                || self.nodes.kind(ancestor) == SyntaxKind::PropertyDeclaration
        }) else {
            // No container: `IsInTopLevelContext`, declined above.
            return;
        };
        if self.nodes.kind(container) == SyntaxKind::PropertyDeclaration {
            let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
            let span = self.nodes.span(node);
            self.report(
                file,
                Diagnostic::new(
                    &messages::AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LEVELS_OF_MODULES,
                    span,
                ),
            );
            return;
        }
        if self.nodes.kind(container) == SyntaxKind::ClassStaticBlockDeclaration {
            return;
        }
        if self.has_async_modifier(container) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        // `GetRangeOfTokenAtPosition(sourceFile, node.Pos())` — the `await`
        // keyword, which is the node's first token.
        let span = self.nodes.span(node);
        self.report(
            file,
            Diagnostic::new(
                &messages::AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LEVELS_OF_MODULES,
                span,
            ),
        );
    }

    /// `hasAsyncModifier` — an `async` modifier on a function-like.
    fn has_async_modifier(&self, node: NodeId) -> bool {
        let modifiers = match self.node_map.get(node) {
            Some(Node::FunctionDeclaration(n)) => n.modifiers,
            Some(Node::FunctionExpression(n)) => n.modifiers,
            Some(Node::ArrowFunction(n)) => n.modifiers,
            Some(Node::MethodDeclaration(n)) => n.modifiers,
            Some(Node::GetAccessorDeclaration(n)) => n.modifiers,
            Some(Node::SetAccessorDeclaration(n)) => n.modifiers,
            Some(Node::ConstructorDeclaration(n)) => n.modifiers,
            _ => return false,
        };
        modifiers.iter().any(|modifier| {
            matches!(modifier, tsr_ast::ModifierLike::Token(token) if token.kind == SyntaxKind::AsyncKeyword)
        })
    }

    /// `Checker.isInParameterInitializerBeforeContainingFunction`
    /// (`checker.go:12235`).
    ///
    /// Walks up to the first function-like parent — which is what makes
    /// `function f(a = async () => await x)` legal — and answers whether the
    /// climb passed through a parameter's initializer. `inBindingInitializer`
    /// is upstream's sticky bit: once inside a **binding element's**
    /// initializer, reaching *any* enclosing parameter counts, because the
    /// destructuring pattern is itself part of the parameter's initialisation.
    pub(crate) fn is_in_parameter_initializer_before_containing_function(
        &self,
        node: NodeId,
    ) -> bool {
        let mut node = node;
        let mut in_binding_initializer = false;
        while let Some(parent) = self.nodes.parent(node) {
            if self.is_function_like_or_static_block(parent) {
                return false;
            }
            match self.node_map.get(parent) {
                Some(Node::ParameterDeclaration(parameter)) => {
                    if in_binding_initializer
                        || parameter.initializer.and_then(|e| e.node_id()) == Some(node)
                    {
                        return true;
                    }
                }
                Some(Node::BindingElement(element))
                    if element.initializer.and_then(|e| e.node_id()) == Some(node) =>
                {
                    in_binding_initializer = true;
                }
                _ => {}
            }
            node = parent;
        }
        false
    }

    /// The offending shape TS1015 reports on, asked of one parameter.
    ///
    /// A **rest** parameter takes an earlier branch of
    /// `checkGrammarParameterList` (`A rest parameter cannot be optional`), so
    /// excluding it here is upstream's ordering rather than a bound chosen
    /// here.
    fn parameter_has_question_and_initializer(&self, node: NodeId) -> bool {
        let Some(Node::ParameterDeclaration(parameter)) = self.node_map.get(node) else {
            return false;
        };
        parameter.dot_dot_dot_token.is_none()
            && parameter.question_token.is_some()
            && parameter.initializer.is_some()
    }

    /// TS1117 — `An object literal cannot have multiple properties with the
    /// same name.`
    ///
    /// `checkGrammarObjectLiteralExpression` (`grammarchecks.go:1139`), the
    /// property-assignment arm of its `DeclarationMeaning` table. Reported on
    /// the **second and later** names, not the first, and once per repeat —
    /// upstream does not `return` here, unlike most of that file.
    ///
    /// Bounded to plain `PropertyAssignment` and `ShorthandPropertyAssignment`
    /// pairs: upstream's other arms give a *different* code to a method/method
    /// clash (TS2300) and a get/set clash (TS1118), so a rule that treated all
    /// members alike would be a wrong code at a right position. A spread
    /// contributes names this port cannot enumerate and stops the check, as it
    /// does in `check_excess_properties`. §180.
    /// TS1118 — `An object literal cannot have multiple get/set accessors with
    /// the same name.`
    ///
    /// The accessor cell of `checkGrammarObjectLiteralExpression`'s meaning
    /// table (`grammarchecks.go:1139`-`:1145`). A `get`/`set` pair merges and is
    /// legal; a repeat of either kind, or anything after the merged pair, is the
    /// error. §966.
    fn check_duplicate_object_literal_accessors(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(Node::ObjectLiteralExpression(literal)) = self.node_map.get(node) else { return };
        // `(name, saw_get, saw_set)` — upstream's `seen` table restricted to the
        // two accessor meanings.
        let mut seen: Vec<(String, bool, bool)> = Vec::new();
        let mut repeats: Vec<NodeId> = Vec::new();
        for property in literal.properties {
            let Some(at) = property.node_id() else { continue };
            let is_get = match self.nodes.kind(at) {
                SyntaxKind::GetAccessor => true,
                SyntaxKind::SetAccessor => false,
                _ => continue,
            };
            let Some(name_id) = self.declaration_name_of(at) else { continue };
            let key = match self.identifier_text(name_id) {
                Some(text) => text.to_string(),
                None => match self.computed_name_spelling(name_id) {
                    Some(spelling) => format!("[]{spelling}"),
                    None => continue,
                },
            };
            match seen.iter_mut().find(|(existing, _, _)| *existing == key) {
                None => seen.push((key, is_get, !is_get)),
                Some((_, saw_get, saw_set)) => {
                    if (is_get && *saw_get) || (!is_get && *saw_set) {
                        repeats.push(name_id);
                    } else {
                        *saw_get = true;
                        *saw_set = true;
                    }
                }
            }
        }
        for name_id in repeats {
            let Some(file) = self.source_file_of_for_diagnostics(name_id) else { continue };
            let span = self.error_span(name_id);
            self.report(
                file,
                Diagnostic::new(
                    &messages::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_GET_SLASHSET_ACCESSORS_WITH_THE_SAME_NAME,
                    span,
                ),
            );
        }
    }

    /// `getEffectivePropertyNameForPropertyNameNode` with
    /// `GetTextOfPropertyName`: the name a property actually declares, with a
    /// **literal computed name folded to its text** and a numeric name
    /// normalised, so `1`, `"1"`, `[1]` and `[+1]` are one key and `0b11` is
    /// `3`.
    ///
    /// A **non-literal** computed name names no particular property and returns
    /// `None` — §52's bound, and falsifier 1 of §968. §968.
    fn effective_property_name_key(&self, name_id: NodeId) -> Option<String> {
        match self.node_map.get(name_id)? {
            Node::Identifier(identifier) => Some(identifier.text.to_string()),
            Node::StringLiteral(literal) => Some(literal.text.to_string()),
            Node::NumericLiteral(literal) => Some(crate::printing::normalise_number(literal.text)),
            Node::ComputedPropertyName(computed) => {
                let inner = computed.expression?.node_id()?;
                match self.node_map.get(inner)? {
                    Node::StringLiteral(literal) => Some(literal.text.to_string()),
                    Node::NumericLiteral(literal) => {
                        Some(crate::printing::normalise_number(literal.text))
                    }
                    // `[+1]` and `[-1]`: upstream evaluates the literal, and a
                    // sign on a numeric literal is still a literal name.
                    Node::PrefixUnaryExpression(unary) => {
                        let operand = unary.operand?.node_id()?;
                        let Node::NumericLiteral(literal) = self.node_map.get(operand)? else {
                            return None;
                        };
                        let value = crate::printing::normalise_number(literal.text);
                        match unary.operator.kind {
                            SyntaxKind::PlusToken => Some(value),
                            SyntaxKind::MinusToken => Some(format!("-{value}")),
                            _ => None,
                        }
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn check_duplicate_object_literal_names(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(Node::ObjectLiteralExpression(literal)) = self.node_map.get(node) else { return };
        // `inDestructuring` — an assignment pattern is a *target* and repeats
        // are legal there.
        if self.nodes.parent(node).is_some_and(|parent| {
            matches!(
                self.node_map.get(parent),
                Some(Node::BinaryExpression(binary))
                    if binary.operator_token.is_some_and(|t| t.kind == SyntaxKind::EqualsToken)
                        && binary.left.and_then(|left| left.node_id()) == Some(node)
            )
        }) {
            return;
        }
        if literal.properties.iter().any(|property| {
            !matches!(
                property,
                tsr_ast::ObjectLiteralElementLike::PropertyAssignment(_)
                    | tsr_ast::ObjectLiteralElementLike::ShorthandPropertyAssignment(_)
            )
        }) {
            return;
        }
        let mut seen: Vec<String> = Vec::new();
        let mut repeats: Vec<(NodeId, String)> = Vec::new();
        for property in literal.properties {
            let name_id = match property {
                tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                    assignment.name.node_id()
                }
                tsr_ast::ObjectLiteralElementLike::ShorthandPropertyAssignment(assignment) => {
                    assignment.name.node_id()
                }
                _ => None,
            };
            let Some(name_id) = name_id else { continue };
            // `getEffectivePropertyNameForPropertyNameNode` returns `!ok` for a
            // computed name, which upstream `continue`s past — it catches those
            // through the late-bound member table instead. This port has no
            // late binding and does have the **written expression**: two
            // computed names spelled identically name the same property when
            // the spelling is an identifier or a chain of them. The key sits in
            // its own namespace, so `{ x: 1, [x]: 2 }` does not collide —
            // `x` and the value of `x` are different properties. §757.
            let key = match self.effective_property_name_key(name_id) {
                Some(key) => key,
                None => match self.computed_name_spelling(name_id) {
                    Some(spelling) => format!("[]{spelling}"),
                    None => continue,
                },
            };
            if seen.contains(&key) {
                repeats.push((name_id, key));
            } else {
                seen.push(key);
            }
        }
        for (name_id, text) in repeats {
            let Some(file) = self.source_file_of_for_diagnostics(name_id) else { continue };
            let span = self.nodes.span(name_id);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::AN_OBJECT_LITERAL_CANNOT_HAVE_MULTIPLE_PROPERTIES_WITH_THE_SAME_NAME,
                    span,
                    [text],
                ),
            );
        }
    }

    /// TS2364 / TS2703 — an expression that must be a *reference* and is not.
    ///
    /// `checkReferenceExpression` (`checker.go:13130`) and
    /// `checkDeleteExpression` (`:10804`). Both skip a spine and then test the
    /// node's kind; they differ in **what** they skip, which is upstream's and
    /// is kept: TS2364 skips assertions and parentheses, TS2703 parentheses
    /// only. §181.
    fn check_reference_expression(&mut self, node: NodeId) {
        // A plain JavaScript file reaches these positions through a different
        // path upstream and this port's parser does not agree with it there —
        // `plainJSBinderErrors.js` is three wrong TS2703 lines and nothing
        // right. The same decline every other rule in this module carries.
        // **The `file_has_parse_errors` bail is port-local** — upstream has no
        // counterpart and checks a file with parse errors like any other. All
        // five of this rule's blocked cases are parser error-recovery fixtures,
        // and the syntax it reads (a binary `=` and its left node's kind) is
        // exactly what recovery preserves. §524 took the same guard off
        // `checkFunctionOrConstructorSymbol` for +4; §526 measured the blanket
        // removal and concluded the collection must be per-rule. §541.
        if self.in_js_file(node) {
            return;
        }
        let (target, message, optional_message, skip_assertions) = match self.node_map.get(node) {
            // **Every assignment operator, not just `=`.** Upstream's
            // `checkBinaryLikeExpression` calls `checkAssignmentOperator` for
            // any `isAssignmentOperator`, and `1 >>= 2` is four of this rule's
            // five blocked cases. The destructuring short-circuit below applies
            // to `=` alone — a compound operator's left cannot be a binding
            // pattern. §542.
            Some(Node::BinaryExpression(binary))
                if binary.operator_token.is_some_and(|t| t.kind.is_assignment_operator()) =>
            {
                let simple =
                    binary.operator_token.is_some_and(|t| t.kind == SyntaxKind::EqualsToken);
                let Some(left) = binary.left.and_then(|left| left.node_id()) else { return };
                // **`checkAssignmentOperator` is never reached for a
                // destructuring assignment.** `checkBinaryLikeExpression`
                // (`checker.go:12338`) short-circuits to
                // `checkDestructuringAssignment` when the operator is `=` and
                // the left side is an object or array literal, so `[a, b] = x`
                // and `({ a } = x)` never see `checkReferenceExpression`. The
                // test is on the **unskipped** left node, as upstream's is —
                // 184 of §181's first measurement's wrong lines were this one
                // short-circuit, and all six of its losses.
                if simple
                    && matches!(
                        self.nodes.kind(left),
                        SyntaxKind::ObjectLiteralExpression | SyntaxKind::ArrayLiteralExpression
                    )
                {
                    return;
                }
                (
                    left,
                    &messages::THE_LEFT_HAND_SIDE_OF_AN_ASSIGNMENT_EXPRESSION_MUST_BE_A_VARIABLE_OR_A_PROPERTY_ACCESS,
                    Some(&messages::THE_LEFT_HAND_SIDE_OF_AN_ASSIGNMENT_EXPRESSION_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_ACCESS),
                    true,
                )
            }
            // `checkReferenceExpression`'s **third** upstream caller
            // (`checker.go:10902`, `:10918`). The helper was complete and two
            // of its three call sites were wired. §740.
            Some(Node::PrefixUnaryExpression(unary))
                if matches!(
                    unary.operator.kind,
                    SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken
                ) =>
            {
                let Some(operand) = unary.operand.and_then(|e| e.node_id()) else { return };
                (
                    operand,
                    &messages::THE_OPERAND_OF_AN_INCREMENT_OR_DECREMENT_OPERATOR_MUST_BE_A_VARIABLE_OR_A_PROPERTY_ACCESS,
                    Some(&messages::THE_OPERAND_OF_AN_INCREMENT_OR_DECREMENT_OPERATOR_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_ACCESS),
                    true,
                )
            }
            Some(Node::PostfixUnaryExpression(unary))
                if matches!(
                    unary.operator.kind,
                    SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken
                ) =>
            {
                let Some(operand) = unary.operand.and_then(|e| e.node_id()) else { return };
                (
                    operand,
                    &messages::THE_OPERAND_OF_AN_INCREMENT_OR_DECREMENT_OPERATOR_MUST_BE_A_VARIABLE_OR_A_PROPERTY_ACCESS,
                    Some(&messages::THE_OPERAND_OF_AN_INCREMENT_OR_DECREMENT_OPERATOR_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_ACCESS),
                    true,
                )
            }
            Some(Node::DeleteExpression(delete)) => {
                let Some(operand) = delete.expression.and_then(|e| e.node_id()) else { return };
                (
                    operand,
                    &messages::THE_OPERAND_OF_A_DELETE_OPERATOR_MUST_BE_A_PROPERTY_REFERENCE,
                    None,
                    false,
                )
            }
            _ => return,
        };
        let spine = self.skip_reference_spine(target, skip_assertions);
        let kind = self.nodes.kind(spine);
        let is_reference = kind == SyntaxKind::Identifier
            || matches!(
                kind,
                SyntaxKind::PropertyAccessExpression | SyntaxKind::ElementAccessExpression
            );
        // TS2364 admits an identifier or an access; TS2703 admits an access
        // only, which is why a bare `delete a` reports and `a = 1` does not.
        let acceptable = if skip_assertions {
            is_reference
        } else {
            matches!(
                kind,
                SyntaxKind::PropertyAccessExpression | SyntaxKind::ElementAccessExpression
            )
        };
        if acceptable {
            // `node.Flags&ast.NodeFlagsOptionalChain != 0`, upstream's second
            // arm (`checker.go:13137`), reported at the unskipped `expr`.
            // `checkDeleteExpression` has no such arm: `delete a?.b` is legal.
            // §181 declined here on a syntactic `?.` walk while the parser
            // did not set the flag (`checker-notes-callres.md` §748 sets it).
            // `docs/parity/notes/misc-checks.md` §9.
            if let Some(optional_message) = optional_message
                && self.is_optional_chain_reference(target)
            {
                self.report_reference_error(target, optional_message);
            }
            return;
        }
        // **The two report at different nodes, and the difference is one
        // column.** `checkReferenceExpression` errors on `expr`, its own
        // parameter, *before* skipping (`checker.go:13134`);
        // `checkDeleteExpression` reassigns `expr = SkipParentheses(...)` and
        // errors on the result (`:10806-10808`). So `delete (a)` is reported
        // at the `a` and `(a) = 1` at the `(`. Six of §181's wrong lines were
        // this one difference, all in the `deleteOperatorWith*Type` family.
        let at = if skip_assertions { target } else { spine };
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.nodes.span(at);
        self.report(file, Diagnostic::new(message, span));
    }

    /// `SkipOuterExpressions(expr, OEKAssertions|OEKParentheses)`, or
    /// `SkipParentheses` when `assertions` is false.
    pub(crate) fn skip_reference_spine(&self, mut node: NodeId, assertions: bool) -> NodeId {
        for _ in 0..64 {
            let next = match self.node_map.get(node) {
                Some(Node::ParenthesizedExpression(inner)) => inner.expression,
                Some(Node::AsExpression(inner)) if assertions => inner.expression,
                Some(Node::TypeAssertion(inner)) if assertions => inner.expression,
                Some(Node::SatisfiesExpression(inner)) if assertions => inner.expression,
                Some(Node::NonNullExpression(inner)) if assertions => inner.expression,
                _ => return node,
            };
            let Some(next) = next.and_then(|expression| expression.node_id()) else { return node };
            node = next;
        }
        node
    }

    /// Whether the spine contains a `?.`. Written (§181) when
    /// `NodeFlags::OPTIONAL_CHAIN` was never set; the flag exists since
    /// §748 and this walk is retained unchanged until the TS2779 arm is
    /// ported over it (through parentheses this walk and the flag differ).
    pub(crate) fn spine_has_optional_chain(&self, mut node: NodeId) -> bool {
        for _ in 0..64 {
            let next = match self.node_map.get(node) {
                Some(Node::PropertyAccessExpression(access)) => {
                    if access.question_dot_token.is_some() {
                        return true;
                    }
                    access.expression
                }
                Some(Node::ElementAccessExpression(access)) => {
                    if access.question_dot_token.is_some() {
                        return true;
                    }
                    access.expression
                }
                Some(Node::ParenthesizedExpression(inner)) => inner.expression,
                _ => return false,
            };
            let Some(next) = next.and_then(|expression| expression.node_id()) else { return false };
            node = next;
        }
        false
    }

    /// TS2371 — `A parameter initializer is only allowed in a function or
    /// constructor implementation.`
    ///
    /// `checkVariableLikeDeclaration` (`checker.go:5851`): a parameter with an
    /// initializer whose containing function has **no body**. An overload
    /// signature and an ambient declaration are both that. Reported on the
    /// **parameter**. §181.
    fn check_parameter_initializer_needs_body(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(Node::ParameterDeclaration(parameter)) = self.node_map.get(node) else { return };
        if parameter.initializer.is_none() {
            return;
        }
        let Some(owner) = self.nodes.parent(node) else { return };
        // `NodeIsMissing(GetContainingFunction(node).Body())`. An arrow and a
        // function expression always have a body, so only the declaration forms
        // can reach the report.
        let missing_body = match self.node_map.get(owner) {
            Some(Node::FunctionDeclaration(function)) => function.body.is_none(),
            Some(Node::MethodDeclaration(method)) => method.body.is_none(),
            Some(Node::ConstructorDeclaration(constructor)) => constructor.body.is_none(),
            // **A signature has no body to be missing**, which is the strongest
            // case for this diagnostic rather than a kind to skip: `interface I
            // { fun(a = 3); }` and `var f: (a = 3) => number` are both TS2371.
            // The `_ => return` below reads as "unknown kind, decline" and was
            // declining the two shapes the row still wanted. §1015.
            Some(
                Node::MethodSignatureDeclaration(_)
                | Node::CallSignatureDeclaration(_)
                | Node::ConstructSignatureDeclaration(_)
                | Node::FunctionTypeNode(_)
                | Node::ConstructorTypeNode(_),
            ) => true,
            _ => return,
        };
        if !missing_body {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.nodes.span(node);
        self.report(
            file,
            Diagnostic::new(
                &messages::A_PARAMETER_INITIALIZER_IS_ONLY_ALLOWED_IN_A_FUNCTION_OR_CONSTRUCTOR_IMPLEMENTATION,
                span,
            ),
        );
    }

    /// TS2694 — `Namespace '{0}' has no exported member '{1}'.`
    ///
    /// `resolveEntityName`'s qualified-name failure arm (`checker.go:15884`),
    /// the gap `check_type_reference_name`'s own doc comment names.
    ///
    /// # `resolveAlias` is the whole rule
    ///
    /// Upstream's lookup is one line —
    /// `getSymbol(getExportsOfSymbol(resolveAlias(namespace)), text, meaning)`
    /// — and **`resolveAlias` is load-bearing**: an `import A = M.B` names a
    /// namespace whose exports live on its *target*, not on the alias symbol.
    /// §185 read `binder.symbols()` directly and measured **127 wrong lines**;
    /// §186 attributed that to alias resolution being unported and was wrong —
    /// [`Checker::resolve_alias`] has been in `symbols.rs` throughout, with
    /// arms for every import and export form including import-equals. §187.
    fn check_qualified_type_name(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        // Only as the `type_name` of a bare type reference — the same slot
        // `check_type_reference_name` claims, so the two are disjoint.
        let Some(parent) = self.nodes.parent(node) else { return };
        let Some(Node::TypeReferenceNode(reference)) = self.node_map.get(parent) else { return };
        if reference.type_name.and_then(|name| name.node_id()) != Some(node) {
            return;
        }
        self.check_qualified_type_name_at(
            node,
            true,
            SymbolFlags::TYPE | SymbolFlags::NAMESPACE | SymbolFlags::ALIAS,
        );
    }

    /// The rule proper, reachable from the entry above **and from itself**.
    ///
    /// `D.inner.Class1` parses as `QualifiedName(QualifiedName(D, inner),
    /// Class1)` and upstream reports on `inner` — the inner qualified name,
    /// whose parent is the outer one and which the entry guard therefore never
    /// admits. §529.
    /// `meaning` is the caller's — upstream's `resolveEntityName` parameter.
    /// A type reference asks for `TYPE | NAMESPACE | ALIAS`; an import-equals
    /// alias may also name a **value**. §559.
    pub(crate) fn check_qualified_type_name_at(
        &mut self,
        node: NodeId,
        outermost: bool,
        meaning: SymbolFlags,
    ) {
        let Some(Node::QualifiedName(qualified)) = self.node_map.get(node) else { return };
        let Some(left) = qualified.left.and_then(|left| left.node_id()) else { return };
        let Some(right) = qualified.right.and_then(|right| right.node_id) else { return };
        // **A deeper chain reports its innermost failure.** §187 declined
        // `A.B.C` outright; the corpus wants the middle segment, which is the
        // `right` of the inner qualified name. The outer level — `A.B` resolving
        // cleanly with `C` missing — needs `resolveEntityName` over a qualified
        // left and is **not built** (§501). §529.
        if self.nodes.kind(left) == SyntaxKind::QualifiedName {
            // The innermost failure first (§529), then this level: `A.B` may
            // resolve cleanly with `C` missing, which is `resolveEntityName`
            // over a qualified left. §556.
            self.check_qualified_type_name_at(left, false, meaning);
            let Some(namespace) = self.resolve_entity_name_to_namespace(left) else { return };
            let Some(member) = self.identifier_text(right).map(str::to_string) else { return };
            // §186's decline, narrowed: a `ModuleDeclaration` with a body is
            // one the binder walked, so an empty table means *nothing was
            // exported*. §558.
            if self.binder.symbols().get(namespace).exports.is_empty()
                && !self.namespace_body_was_bound(namespace)
            {
                return;
            }
            // `getExportsOfSymbol` includes a module's `export *` re-exports.
            if self.get_export_of_module(namespace, member.as_str()).is_some()
                || self.binder.symbols().get(namespace).exports.contains_key(member.as_str())
            {
                return;
            }
            let printed = self.printed_entity_name(left).unwrap_or_default();
            if let Some(file) = self.source_file_of_for_diagnostics(right) {
                let span = self.nodes.span(right);
                self.report(
                    file,
                    Diagnostic::with_args(
                        &messages::NAMESPACE_0_HAS_NO_EXPORTED_MEMBER_1,
                        span,
                        [printed, member],
                    ),
                );
            }
            return;
        }
        if self.nodes.kind(left) != SyntaxKind::Identifier {
            return;
        }
        let Some(namespace_name) = self.identifier_text(left).map(str::to_string) else { return };
        let Some(member) = self.identifier_text(right).map(str::to_string) else { return };
        // `resolveEntityName(left, SymbolFlagsNamespace)`. `MODULE` rather than
        // the wider `NAMESPACE` keeps a class or enum from answering — the
        // message names a *namespace*.
        let Some(namespace) = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            left,
            &namespace_name,
            SymbolFlags::MODULE | SymbolFlags::ALIAS,
        ) else {
            // `resolveEntityName` reports `Cannot_find_namespace_0` only when
            // the name resolves to **nothing at all** (`checker.go:15782`).
            // A name that resolves under another meaning — a type parameter in
            // a conditional type is the corpus's shape — reaches a different
            // arm entirely, and reporting TS2503 there was 60 wrong lines.
            // §302.
            if [SymbolFlags::TYPE, SymbolFlags::VALUE, SymbolFlags::NAMESPACE].into_iter().any(
                |meaning| {
                    self.binder
                        .resolve_name(self.nodes, self.node_map, left, &namespace_name, meaning)
                        .is_some()
                },
            ) {
                return;
            }
            if let Some(file) = self.source_file_of_for_diagnostics(left) {
                let span = self.nodes.span(left);
                self.report(
                    file,
                    Diagnostic::with_args(
                        &messages::CANNOT_FIND_NAMESPACE_0,
                        span,
                        [namespace_name],
                    ),
                );
            }
            return;
        };
        // `resolveAlias(namespace)` — upstream's own call, and the one §185
        // omitted.
        let namespace = self.binder.merged_symbol(namespace);
        let namespace = if self.binder.symbols().get(namespace).flags.intersects(SymbolFlags::ALIAS)
        {
            let Some(target) = self.resolve_alias(namespace) else { return };
            self.binder.merged_symbol(target)
        } else {
            namespace
        };
        // A namespace whose exports this port never filled cannot be asked
        // whether a member is missing — the answer would be "all of them".
        // **An empty table can be declined; a partial one cannot** (§186), and
        // that limit is unchanged by resolving the alias.
        // §186's decline, narrowed as §558 narrowed it on the deep branch: a
        // `ModuleDeclaration` with a body is one the binder walked, so an empty
        // table means *nothing was exported* rather than *nothing was
        // recorded*. `namespace N { function S() {} }` exports nothing, which
        // is exactly why `var foge: N.S` is an error. §561.
        if self.binder.symbols().get(namespace).exports.is_empty()
            && !self.namespace_body_was_bound(namespace)
        {
            return;
        }
        // `getSymbol(getExportsOfSymbol(namespace), …)`: a module's table
        // includes its `export *` re-exports.
        let exported = match self.binder.symbols().get(namespace).exports.get(member.as_str()) {
            Some(&symbol) => Some(symbol),
            None => self.get_export_of_module(namespace, member.as_str()),
        };
        let found = exported
            .is_some_and(|symbol| self.binder.symbols().get(symbol).flags.intersects(meaning));
        // **`canSuggestTypeof` (`checker.go:15869`), tested before the namespace
        // branch.** A *fundule* — `function B` merged with `namespace B` — is
        // "found" by the test above because it carries `NAMESPACE`, and upstream
        // reports TS2749 there rather than TS2709: the member exists, the whole
        // qualified name resolves as a **value**, and the position wanted a
        // type. The error node is the whole name, not the member. §426.
        if found {
            let value_only = exported.is_some_and(|symbol| {
                let flags = self.binder.symbols().get(symbol).flags;
                flags.intersects(SymbolFlags::VALUE) && !flags.intersects(SymbolFlags::TYPE)
            });
            let in_type_query = self
                .nodes
                .parent(node)
                .is_some_and(|parent| self.nodes.kind(parent) == SyntaxKind::TypeQuery);
            // **The `canSuggestTypeof` arm belongs to the whole name.** Its
            // error node is the entire type reference and its message prints
            // `A.B`; asked of an inner segment it reported TS2749 at three
            // positions upstream leaves alone — §529's falsifier 2, fired.
            // **And this arm is type-position-only too.** An import-equals
            // alias may legitimately name a value, so `canSuggestTypeof` — the
            // *"did you mean `typeof`"* suggestion — is wrong there. Same
            // meaning, one arm over. §559.
            // **The value meanings a *type* position never asks for.**
            // `meaning.intersects(VALUE)` was true for every type reference and
            // made this arm unreachable from §559 until §695: `VALUE_MODULE`
            // enters through `SymbolFlagsNamespace`, and `CLASS`, `ENUM` and
            // `ENUM_MEMBER` are in both `TYPE` and `VALUE` outright — so no
            // subtraction from `VALUE` separates the two positions. `VARIABLE`
            // and `FUNCTION` do: a type meaning carries neither, and the
            // `import a = X` position §559 was bounding carries both.
            if outermost
                && !meaning.intersects(SymbolFlags::VARIABLE | SymbolFlags::FUNCTION)
                && value_only
                && !in_type_query
                && let Some(file) = self.source_file_of_for_diagnostics(node)
            {
                let span = self.nodes.span(node);
                let printed = format!("{namespace_name}.{member}");
                self.report(
                    file,
                    Diagnostic::with_args(
                        &messages::_0_REFERS_TO_A_VALUE_BUT_IS_BEING_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF_0,
                        span,
                        [printed],
                    ),
                );
            }
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(right) else { return };
        // `c.error(right, …)` — the member, not the whole name.
        let span = self.nodes.span(right);
        // `getSuggestedSymbolForNonexistentModule` (`checker.go:15861`) is
        // tried **first** and carries TS2724, so a near-miss member makes
        // TS2694 a wrong code at a right position. §185 declined this arm and
        // named it falsifier 1; it fired, on four of seven wrong lines
        // (`moduleVisibilityTest3` and `4`, both `M.num` against `nums`).
        //
        // The distance function is `spelling_suggestion`, already ported from
        // `core.getSpellingSuggestion` — it takes a candidate list, so scoping
        // it to one symbol's exports is the whole of upstream's variant.
        let candidates: Vec<&str> =
            self.binder.symbols().get(namespace).exports.keys().copied().collect();
        if let Some(suggestion) = spelling_suggestion(&member, &candidates) {
            let suggestion = suggestion.to_string();
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::_0_HAS_NO_EXPORTED_MEMBER_NAMED_1_DID_YOU_MEAN_2,
                    span,
                    [namespace_name, member, suggestion],
                ),
            );
            return;
        }
        self.report(
            file,
            Diagnostic::with_args(
                &messages::NAMESPACE_0_HAS_NO_EXPORTED_MEMBER_1,
                span,
                [namespace_name, member],
            ),
        );
    }

    /// TS1163 — `A 'yield' expression is only allowed in a generator body.`
    ///
    /// `checkGrammarYieldExpression` (`grammarchecks.go:1777`), whose test is
    /// `node.Flags&ast.NodeFlagsYieldContext == 0`. This port declares
    /// [`tsr_ast::NodeFlags::YIELD_CONTEXT`] and never sets it, so the context
    /// is derived from the tree instead: **the nearest enclosing function-like
    /// must be a generator.** An arrow function and an accessor can never be
    /// one, and a class property initialiser or static block starts a fresh
    /// context. See `checker-notes-diag2.md` §104.
    ///
    /// `grammarErrorOnFirstToken` reports the `yield` keyword, which is the
    /// expression's own start — so the span is `nodes.span`, not `error_span`.
    fn check_yield_grammar(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        // **A bare `yield` is an IDENTIFIER outside a generator**, in
        // non-strict code — `function f(yield = yield) {}` and
        // `{ [yield]: foo }` are legal and upstream's parser builds an
        // identifier there. This parser builds a `YieldExpression`, so the rule
        // would report on a name. Requiring an operand bounds it to the
        // unambiguous form. **Owner: `tsr_parser`'s yield-context tracking** —
        // 11 wrong lines measured, `FunctionDeclaration3_es6` and
        // `FunctionDeclaration8_es6` at the head of them (§104).
        let Some(Node::YieldExpression(yielded)) = self.node_map.get(node) else { return };
        if yielded.expression.is_none() {
            return;
        }
        // The other two shapes `nextTokenIsIdentifierOrKeywordOrLiteralOnSameLine`
        // (`parser.go:4171`) rejects, both decidable from the finished tree:
        //
        // - **`yield(foo)` is a CALL.** `(` is not an identifier, keyword or
        //   literal, so upstream reads `yield` as the callee. This parser builds
        //   a yield whose operand is a parenthesized expression.
        // - **`yield * []` is a MULTIPLICATION.** `*` fails the lookahead too,
        //   so outside a generator the asterisk is the operator. Inside one it
        //   is `yield*`, which is why this is guarded by the context below
        //   rather than declined outright.
        //
        // Both are §104's wrong column and both belong to `tsr_parser` (§106);
        // these bounds keep the rule quiet until it is fixed there.
        if matches!(yielded.expression, Some(tsr_ast::Expression::ParenthesizedExpression(_))) {
            return;
        }
        if yielded.asterisk_token.is_some() {
            return;
        }
        // A **computed property name is evaluated in the ENCLOSING context**,
        // where the member it names is not — so in
        // `async function* t() { class C { [yield 1] = yield 2; } }` the name is
        // inside the generator and the initialiser is not. The walk therefore
        // passes *through* a class member it reached via a
        // `ComputedPropertyName`, and stops at it otherwise. §107 measured the
        // four wrong lines this fixes, all in `awaitAndYieldInProperty`.
        let mut came_from = node;
        let mut in_generator = false;
        for ancestor in self.nodes.ancestors(node) {
            let verdict = match self.node_map.get(ancestor) {
                Some(Node::FunctionDeclaration(n)) => Some(n.asterisk_token.is_some()),
                Some(Node::FunctionExpression(n)) => Some(n.asterisk_token.is_some()),
                // A method's computed name is outside its body too:
                // `{ [yield 0]() {} }` in a generator (`generatorTypeCheck42`).
                Some(Node::MethodDeclaration(n))
                    if self.nodes.kind(came_from) != SyntaxKind::ComputedPropertyName =>
                {
                    Some(n.asterisk_token.is_some())
                }
                // Cannot be generators; they still bound the context — unless
                // this is their computed name rather than their body.
                Some(
                    Node::ArrowFunction(_)
                    | Node::GetAccessorDeclaration(_)
                    | Node::SetAccessorDeclaration(_)
                    | Node::ConstructorDeclaration(_)
                    | Node::PropertyDeclaration(_)
                    | Node::ClassStaticBlockDeclaration(_),
                ) if self.nodes.kind(came_from) != SyntaxKind::ComputedPropertyName => Some(false),
                _ => None,
            };
            if let Some(verdict) = verdict {
                in_generator = verdict;
                break;
            }
            came_from = ancestor;
        }
        if in_generator {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.nodes.span(node);
        self.report(
            file,
            Diagnostic::new(
                &messages::A_YIELD_EXPRESSION_IS_ONLY_ALLOWED_IN_A_GENERATOR_BODY,
                span,
            ),
        );
    }

    /// TS1206 — `Decorators are not valid here.`
    ///
    /// `reportObviousDecoratorErrors` → `findFirstIllegalDecorator`
    /// (`grammarchecks.go:642`), and the `NodeCanBeDecorated` arm at `:246`.
    /// A decorator is legal on a class, a method with a body, an accessor, a
    /// property and a parameter of those; **every other declaration rejects
    /// it**, and the report lands on the decorator's first token, not on the
    /// declaration.
    ///
    /// Only the declaration kinds that can never be decorated are ported —
    /// the parameter and private-name arms need `NodeCanBeDecorated`'s
    /// grandparent tests and are left to their own row
    /// (`checker-notes-diag2.md` §112).
    fn check_illegal_decorator(&mut self, modifiers: &[ModifierLike<'_>]) {
        // `grammarErrorOnFirstToken` is silent in a file with parse
        // diagnostics (`decoratorOnUsing`).
        if self.file_has_parse_errors {
            return;
        }
        let Some(decorator) = modifiers.iter().find_map(|modifier| match modifier {
            ModifierLike::Decorator(decorator) => decorator.node_id,
            ModifierLike::Token(_) => None,
        }) else {
            return;
        };
        let Some(file) = self.source_file_of_for_diagnostics(decorator) else { return };
        let span = self.nodes.span(decorator);
        self.report(file, Diagnostic::new(&messages::DECORATORS_ARE_NOT_VALID_HERE, span));
        // `reportObviousDecoratorErrors(node)` is the **first** test in
        // `checkGrammarModifiers` and its `true` returns from the whole
        // function (`grammarchecks.go:218`), so the per-keyword switch never
        // runs for a node whose decorators are already illegal. §877 wired the
        // chain's suppression of the rules split out of it; this is the
        // suppression that runs in the other direction. §878.
        if let Some(owner) = self.nodes.parent(decorator) {
            self.decorator_error_reported.insert(owner);
        }
    }

    /// Does this subtree contain an **assignment** to `this.<text>`?
    ///
    /// [`Self::subtree_accesses_this_member`] answers *"is `this.x` mentioned at
    /// all"*, which is what TS2564 needs — for initialisation any mention is
    /// enough to decline. TS7008 needs the narrower question: upstream infers an
    /// unannotated member's type from a constructor **assignment**, and
    /// `console.log(this.test)` gives it nothing. §795 measured the loose helper
    /// at −3 in that caller.
    ///
    /// `docs/architecture/checker-notes-diag2.md` §796.
    pub(crate) fn subtree_assigns_this_member(&self, node: NodeId, text: &str, depth: u32) -> bool {
        if depth > 64 {
            return false;
        }
        // `getFlowTypeInConstructor` (`flow.go:2466`) asks the flow type of a
        // synthesized `this.<name>`, and `isMatchingReference` matches it
        // against any access expression on `this` whose
        // `getAccessedPropertyName` is the same: a private name
        // (`this.#x`, `controlFlowPrivateClassField`) or a literal element
        // access (`this['x']`, `classPropInitializationInferenceWithElementAccess`).
        if let Some(Node::BinaryExpression(binary)) = self.node_map.get(node)
            && binary.operator_token.is_some_and(|t| t.kind == SyntaxKind::EqualsToken)
            && let Some(left) = binary.left.and_then(|left| left.node_id())
        {
            let assigned = match self.node_map.get(left) {
                Some(Node::PropertyAccessExpression(access)) => {
                    Self::expression_is_this(access.expression)
                        && match access.name {
                            Some(tsr_ast::MemberName::Identifier(name)) => name.text == text,
                            Some(tsr_ast::MemberName::PrivateIdentifier(name)) => name.text == text,
                            _ => false,
                        }
                }
                Some(Node::ElementAccessExpression(access)) => {
                    Self::expression_is_this(access.expression)
                        && match access.argument_expression {
                            Some(tsr_ast::Expression::StringLiteral(name)) => name.text == text,
                            Some(tsr_ast::Expression::NoSubstitutionTemplateLiteral(name)) => {
                                name.text == text
                            }
                            Some(tsr_ast::Expression::NumericLiteral(name)) => name.text == text,
                            _ => false,
                        }
                }
                _ => false,
            };
            if assigned {
                return true;
            }
        }
        let mut children = Vec::new();
        if let Some(typed) = self.node_map.get(node) {
            tsr_ast::for_each_child_id(typed, |child| children.push(child));
        }
        children.into_iter().any(|child| self.subtree_assigns_this_member(child, text, depth + 1))
    }

    /// Does the subtree rooted at `node` reach `this.<text>` in any position?
    ///
    /// The decidable part of `isPropertyInitializedInConstructor`
    /// (`checker.go:4947`), as `checker-notes-diag2.md` §87 sets out: a
    /// constructor body that never names the property cannot assign it on any
    /// path, and that is the one answer the un-runnable flow query has that can
    /// be read straight off the tree.
    ///
    /// **Every uncertain answer is `true`.** `true` means "declines", so the
    /// depth cap, an unmapped node and `this["x"]` — whose argument this does
    /// not evaluate — all take that branch. A `false` returned wrongly is a
    /// diagnostic reported where upstream reports none; a `true` returned
    /// wrongly is silence. Only the first is a wrong line.
    pub(crate) fn subtree_accesses_this_member(
        &self,
        node: NodeId,
        text: &str,
        depth: u32,
    ) -> bool {
        if depth > 64 {
            return true;
        }
        let Some(typed) = self.node_map.get(node) else { return true };
        match typed {
            Node::PropertyAccessExpression(access) => {
                let named = match access.name {
                    Some(tsr_ast::MemberName::Identifier(name)) => name.text == text,
                    Some(tsr_ast::MemberName::PrivateIdentifier(name)) => name.text == text,
                    None => true,
                };
                if named && Self::expression_is_this(access.expression) {
                    return true;
                }
            }
            // `this[…]` is not evaluated here, so any element access on `this`
            // is treated as reaching every member.
            Node::ElementAccessExpression(access)
                if Self::expression_is_this(access.expression) =>
            {
                return true;
            }
            _ => {}
        }
        let mut children = Vec::new();
        tsr_ast::for_each_child_id(typed, |child| children.push(child));
        children.into_iter().any(|child| self.subtree_accesses_this_member(child, text, depth + 1))
    }

    /// Is this expression the `this` keyword itself?
    fn expression_is_this(expression: Option<tsr_ast::Expression<'_>>) -> bool {
        matches!(expression, Some(tsr_ast::Expression::KeywordExpression(keyword))
            if keyword.kind == SyntaxKind::ThisKeyword)
    }

    /// `getControlFlowContainer` (`checker.go:11438`): the innermost enclosing
    /// function that is not immediately invoked, module block, source file or
    /// property declaration. An IIFE's body is part of its caller's flow.
    pub(crate) fn control_flow_container(&self, node: NodeId) -> Option<NodeId> {
        let mut current = self.nodes.parent(node);
        while let Some(id) = current {
            if matches!(
                self.nodes.kind(id),
                SyntaxKind::FunctionExpression | SyntaxKind::ArrowFunction
            ) && self.immediately_invoked_call(id).is_some()
            {
                current = self.nodes.parent(id);
                continue;
            }
            if matches!(
                self.nodes.kind(id),
                SyntaxKind::FunctionDeclaration
                    | SyntaxKind::FunctionExpression
                    | SyntaxKind::ArrowFunction
                    | SyntaxKind::MethodDeclaration
                    | SyntaxKind::Constructor
                    | SyntaxKind::GetAccessor
                    | SyntaxKind::SetAccessor
                    | SyntaxKind::ModuleBlock
                    | SyntaxKind::SourceFile
                    | SyntaxKind::PropertyDeclaration
            ) {
                return Some(id);
            }
            current = self.nodes.parent(id);
        }
        None
    }

    /// `IsInTypeQuery` and `isInAmbientOrTypeNode` (`utilities.go:1057`),
    /// collapsed: both are ancestor walks and both mean "not a value position
    /// the flow graph describes".
    fn is_in_type_query_or_type_node(&self, node: NodeId) -> bool {
        self.nodes.ancestors(node).any(|id| {
            matches!(
                self.nodes.kind(id),
                SyntaxKind::TypeQuery
                    | SyntaxKind::InterfaceDeclaration
                    | SyntaxKind::TypeAliasDeclaration
                    | SyntaxKind::TypeLiteral
            )
        })
    }

    /// TS2369 — `A parameter property is only allowed in a constructor
    /// implementation.`
    ///
    /// `checkParameter` (`checker.go:2670`). `ModifierFlagsParameterPropertyModifier`
    /// is `AccessibilityModifier | Readonly | Override`
    /// (`ast/modifierflags.go:45`), and the containing function must be a
    /// constructor **with a body** — an overload signature is not an
    /// implementation.
    ///
    /// Purely syntactic: no type is computed and nothing is resolved, which is
    /// why it needs none of the bounds the four rules above do. The one decline
    /// is the parse-error gate every rule in this module shares.
    fn check_parameter_property_position(&mut self, node: NodeId, modifiers: &[ModifierLike<'_>]) {
        if self.file_has_parse_errors {
            return;
        }
        if !modifiers.iter().any(|modifier| {
            matches!(modifier, ModifierLike::Token(token)
            if matches!(
                token.kind,
                SyntaxKind::PublicKeyword
                    | SyntaxKind::PrivateKeyword
                    | SyntaxKind::ProtectedKeyword
                    | SyntaxKind::ReadonlyKeyword
                    | SyntaxKind::OverrideKeyword
            ))
        }) {
            return;
        }
        // `ast.GetContainingFunction` — a parameter's parent *is* its owner
        // here, so no walk is needed.
        let is_constructor_implementation = self.nodes.parent(node).is_some_and(|owner| {
            matches!(
                self.node_map.get(owner),
                Some(Node::ConstructorDeclaration(constructor)) if constructor.body.is_some()
            )
        });
        if is_constructor_implementation {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(
            file,
            Diagnostic::new(
                &messages::A_PARAMETER_PROPERTY_IS_ONLY_ALLOWED_IN_A_CONSTRUCTOR_IMPLEMENTATION,
                span,
            ),
        );
    }

    /// TS2695 — `Left side of comma operator is unused and has no side effects.`
    ///
    /// `checkBinaryLikeExpression`'s comma arm (`checker.go:12533`). Reported at
    /// the **left operand**, gated on three things upstream tests in order:
    /// `allowUnreachableCode` is not `true`, the left side is side-effect free
    /// (`isSideEffectFree`, `checker.go:13011`), and the expression is not an
    /// *indirect call* — `(0, x.f)()` and `(0, eval)()`, the idiom for calling
    /// without passing `this` (`isIndirectCall`, `checker.go:13039`).
    ///
    /// Upstream additionally suppresses it where a
    /// `JSX_expressions_must_have_one_parent_element` parse diagnostic covers
    /// the position (`checker.go:12537`); that whole class is already excluded
    /// here by the parse-error gate, which is the first time that gate has paid
    /// for something other than tree shape.
    /// TS7027 — `Unreachable code detected.`
    ///
    /// `Binder.checkUnreachable` (`binder.go`) upstream, and **not a binder rule
    /// here**: `tsr_binder` already records the answer per node.
    /// `bind_children` (`binder.rs:1019`) sets [`NodeFacts::UNREACHABLE`] on
    /// every node `is_potentially_executable_node` accepts while
    /// `current_flow == flow.unreachable()`, and that predicate
    /// (`narrowing.rs:500`) is already upstream's `reportError` condition
    /// including the variable-statement clause — a bare `var x;` is hoisted and
    /// does not report, a `let x;` is temporal-dead-zone observable and does.
    ///
    /// So the standing *"compiler options are not plumbed into
    /// `tsr_binder::bind`"* blocker — carried for TS1212 across three handoffs —
    /// does not apply here. `checker-notes-diag2.md` §82.
    ///
    /// # Three things the per-node fact is not
    ///
    /// **One report per run.** Upstream sets `currentFlow =
    /// reportedUnreachableFlow` after reporting, so every later statement of
    /// the run stays silent. Here that is **structural** rather than walk
    /// state: a node reports only when no ancestor and no preceding sibling
    /// carries the fact. Mutable walk state was tried first and is what §82
    /// records as wrong.
    ///
    /// **`EmptyStatement` and declarations.**
    /// `IsStatementButNotDeclaration(node) && node.Kind != KindEmptyStatement ||
    /// ClassDeclaration || …`. The binder's predicate is the *kind range*, which
    /// is wider on both counts, so the two extra tests are here.
    ///
    /// **Unset is a suggestion.** `unreachableCodeIsError` is
    /// `AllowUnreachableCode == TSFalse`, explicitly — see
    /// [`Checker::set_unreachable_code_is_error`].
    fn check_unreachable(&mut self, node: NodeId, ambient: bool) {
        if self.file_has_parse_errors || !self.is_unreachable_run_member(node) {
            return;
        }
        let Some(parent) = self.nodes.parent(node) else { return };
        // An unreachable statement *inside* an unreachable one is the same run:
        // upstream's `currentFlow` is still `reportedUnreachableFlow` all the
        // way down.
        //
        // §744: upstream's `withinUnreachableCode` (`checker.go:2264`) is set
        // when an ancestor REPORTED, which since the flow-typed arm landed in
        // [`Checker::is_unreachable_run_member`] includes an ancestor the
        // binder did not flag — a block after a `never`-returning call. The
        // ancestor test asks the same predicate the report does.
        let ancestors: Vec<NodeId> = self.nodes.ancestors(node).collect();
        for ancestor in ancestors {
            if self.binder.facts(ancestor).contains(NodeFacts::UNREACHABLE)
                || self.is_unreachable_run_member(ancestor)
            {
                return;
            }
        }
        // …and so is a statement whose **immediately preceding sibling** is
        // itself part of the run. Upstream reports once per *run*, not once per
        // statement and not once per list: `checker.go:2409-2442` scans forward
        // over consecutive statements that are both potentially executable and
        // unreachable, marks them reported, and emits one diagnostic. A
        // statement failing either test breaks the run, and the next unreachable
        // one after it reports again — which is `reachabilityChecks1`'s second
        // top-level report, at the `namespace B` that follows two function
        // declarations. §82 asked whether *any* earlier sibling was unreachable,
        // which can only ever report once per list; §89 narrowed it by one word.
        //
        // Modelling `reportedUnreachableFlow` as mutable walk state was tried
        // first and got this wrong for a different reason: it clears on a
        // reachable statement, and this binder starts a fresh flow inside a
        // namespace body where upstream does not, so the flag came back on and
        // every top-level `namespace` after the first reported — §82.
        let Some(typed) = self.node_map.get(parent) else { return };
        let mut siblings: Vec<NodeId> = Vec::new();
        tsr_ast::for_each_child_id(typed, |child| siblings.push(child));
        let Some(index) = siblings.iter().position(|&child| child == node) else { return };
        if index > 0 && self.is_unreachable_run_member(siblings[index - 1]) {
            return;
        }
        // `!(node.Flags&NodeFlagsAmbient != 0)` — a statement in an ambient
        // context is already an error and upstream declines there.
        if ambient || !self.unreachable_code_is_error {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        // `errorOnEachUnreachableRange` reports on the **statement's own
        // range**, not on `GetErrorRangeForNode`'s narrowing: upstream writes
        // `reachabilityChecks1.ts(47,5)` for a `namespace A { … }` where
        // `error_span` would give the name at column 11. This is the one report
        // site in `crate::check` that must not go through `error_span`, and §48
        // centralised the others precisely so an exception is visible.
        let span = self.nodes.span(node);
        self.report(file, Diagnostic::new(&messages::UNREACHABLE_CODE_DETECTED, span));
    }

    /// Is `node` part of an unreachable run — reportable in its own right, and
    /// therefore also able to swallow the statement that follows it?
    ///
    /// `IsPotentiallyExecutableNode(node) && isSourceElementUnreachable(node)`
    /// (`ast/utilities.go:4229`, `checker.go:2455`), which upstream tests both
    /// at the node it might report on and at every node it scans forward over.
    /// One predicate, because they are the same question — see
    /// `checker-notes-diag2.md` §89.
    ///
    /// §744: the `else` half of `isSourceElementUnreachable`
    /// (`checker.go:2466`) — "for code the binder doesn't know is
    /// unreachable, use control flow / types": a statement the binder gave a
    /// flow node reports when [`Checker::is_reachable_flow_node`] says
    /// control cannot reach it, which is what a `never`-returning call in
    /// the path means (`neverReturningFunctions1`). The walk's CALL arm sees
    /// only callees that VISIBLY declare `never`/`asserts` (§127's
    /// pre-gate), so a `never` that has to be inferred still reads reachable.
    fn is_unreachable_run_member(&mut self, node: NodeId) -> bool {
        if !Self::is_reportable_unreachable_kind(self.nodes.kind(node)) {
            return false;
        }
        if !self.binder.facts(node).contains(NodeFacts::UNREACHABLE) {
            return match self.binder.flow_of(node) {
                Some(flow) => !self.is_reachable_flow_node(flow),
                None => false,
            };
        }
        // `isSourceElementUnreachable`'s per-kind switch (`checker.go:2458`):
        // an enum or a namespace that emits no JavaScript is not unreachable
        // *code*, because it is not code.
        match self.node_map.get(node) {
            Some(Node::EnumDeclaration(declaration)) => {
                !has_modifier(declaration.modifiers, SyntaxKind::ConstKeyword)
                    || self.preserve_const_enums
            }
            Some(Node::ModuleDeclaration(_)) => self.is_instantiated_module(node),
            _ => true,
        }
    }

    /// TS2433 — `A namespace declaration cannot be in a different file from a
    /// class or function with which it is merged.`
    /// TS2434 — `A namespace declaration cannot be located prior to a class or
    /// function with which it is merged.`
    ///
    /// `checkModuleDeclaration`'s merged-declaration branch
    /// (`checker.go:5174`): one predicate and a two-way `else if`, so the two
    /// codes are exclusive. The error node is the module's **name**, which the
    /// corpus's `(1,11)` and `(2,22)` columns confirm.
    ///
    /// `docs/architecture/checker-notes-diag2.md` §548.
    fn check_namespace_merge_position(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors {
            return;
        }
        let Some(Node::ModuleDeclaration(declaration)) = self.node_map.get(node) else { return };
        if has_modifier(declaration.modifiers, SyntaxKind::DeclareKeyword)
            || self.is_ambient_module_node(node)
            || !self.is_instantiated_module(node)
        {
            return;
        }
        let Some(name) = declaration.name.and_then(|name| name.node_id()) else { return };
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        let symbol = self.binder.merged_symbol(symbol);
        if !self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::VALUE_MODULE) {
            return;
        }
        let declarations = self.binder.symbols().get(symbol).declarations.clone();
        if declarations.len() <= 1 {
            return;
        }
        // `getFirstNonAmbientClassOrFunctionDeclaration` (`checker.go:5175`).
        let Some(first) = declarations.iter().copied().find(|&candidate| {
            matches!(
                self.nodes.kind(candidate),
                SyntaxKind::ClassDeclaration | SyntaxKind::FunctionDeclaration
            ) && !self.declaration_is_in_an_ambient_context(candidate)
        }) else {
            return;
        };
        let Some(file) = self.source_file_of_for_diagnostics(name) else { return };
        let span = self.error_span(name);
        // **Exclusive**, as upstream's `else if` makes them.
        let message = if self.source_file_of_for_diagnostics(first) != Some(file) {
            &messages::A_NAMESPACE_DECLARATION_CANNOT_BE_IN_A_DIFFERENT_FILE_FROM_A_CLASS_OR_FUNCTION_WITH_WHICH_IT_IS_MERGED
        } else if self.nodes.span(node).start < self.nodes.span(first).start {
            &messages::A_NAMESPACE_DECLARATION_CANNOT_BE_LOCATED_PRIOR_TO_A_CLASS_OR_FUNCTION_WITH_WHICH_IT_IS_MERGED
        } else {
            return;
        };
        self.report(file, Diagnostic::new(message, span));
    }

    /// TS1344 — `A label is not allowed here.`
    ///
    /// `checkStrictModeLabeledStatement` (`binder.go:1433`). **Upstream emits
    /// this from the binder**, and `tsr-binder`'s module docs record that its
    /// strict-mode diagnostics are not ported; the suite compares
    /// `(file, line, column, code)` and not which component produced a line, so
    /// the check lives here instead. Moving it is the faithful placement and
    /// needs `binder.diagnostics()` wired through to the suite, which nothing
    /// consumes today.
    ///
    /// The error node is the **label**, not the statement
    /// (`errorOnFirstToken(data.Label)`).
    ///
    /// `docs/architecture/checker-notes-diag2.md` §550.
    fn check_label_is_allowed(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(Node::LabeledStatement(labeled)) = self.node_map.get(node) else { return };
        let Some(statement) = labeled.statement.and_then(|s| s.node_id()) else { return };
        if !matches!(
            self.nodes.kind(statement),
            SyntaxKind::VariableStatement
                | SyntaxKind::FunctionDeclaration
                | SyntaxKind::MissingDeclaration
                | SyntaxKind::ClassDeclaration
                | SyntaxKind::InterfaceDeclaration
                | SyntaxKind::TypeAliasDeclaration
                | SyntaxKind::EnumDeclaration
                | SyntaxKind::ModuleDeclaration
                | SyntaxKind::ImportDeclaration
                | SyntaxKind::ImportEqualsDeclaration
                | SyntaxKind::ExportDeclaration
                | SyntaxKind::ExportAssignment
                | SyntaxKind::NamespaceExportDeclaration
        ) {
            return;
        }
        let Some(label) = labeled.label.and_then(|label| label.node_id) else { return };
        let Some(file) = self.source_file_of_for_diagnostics(label) else { return };
        let span = self.nodes.span(label);
        self.report(file, Diagnostic::new(&messages::A_LABEL_IS_NOT_ALLOWED_HERE, span));
    }

    /// `resolveEntityName` restricted to the namespace meaning
    /// (`checker.go:15782`), which is all this rule asks of it.
    ///
    /// An `Identifier` resolves by name; a `QualifiedName` resolves its left
    /// and looks the right up in that symbol's **exports**. An alias is
    /// resolved at every step, as upstream's `resolveAlias` call does. No
    /// types are involved at any level. §556.
    fn resolve_entity_name_to_namespace(&mut self, node: NodeId) -> Option<tsr_binder::SymbolId> {
        match self.node_map.get(node)? {
            Node::Identifier(_) => {
                let text = self.identifier_text(node)?.to_string();
                let symbol = self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    node,
                    &text,
                    SymbolFlags::MODULE | SymbolFlags::ALIAS,
                )?;
                self.resolved_namespace_symbol(symbol)
            }
            Node::QualifiedName(qualified) => {
                let left = qualified.left.and_then(|left| left.node_id())?;
                let right = qualified.right.and_then(|right| right.node_id)?;
                let namespace = self.resolve_entity_name_to_namespace(left)?;
                let text = self.identifier_text(right)?.to_string();
                let member = *self.binder.symbols().get(namespace).exports.get(text.as_str())?;
                self.resolved_namespace_symbol(member)
            }
            _ => None,
        }
    }

    /// `merged_symbol` then `resolveAlias`, the pair every step of an entity
    /// name needs.
    fn resolved_namespace_symbol(
        &mut self,
        symbol: tsr_binder::SymbolId,
    ) -> Option<tsr_binder::SymbolId> {
        let symbol = self.binder.merged_symbol(symbol);
        if self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::ALIAS) {
            let target = self.resolve_alias(symbol)?;
            return Some(self.binder.merged_symbol(target));
        }
        Some(symbol)
    }

    /// The printed form of an entity name — `A.B` for a qualified one. §556.
    fn printed_entity_name(&self, node: NodeId) -> Option<String> {
        match self.node_map.get(node)? {
            Node::Identifier(_) => self.identifier_text(node).map(str::to_string),
            Node::QualifiedName(qualified) => {
                let left = qualified.left.and_then(|left| left.node_id())?;
                let right = qualified.right.and_then(|right| right.node_id)?;
                let left = self.printed_entity_name(left)?;
                let right = self.identifier_text(right)?;
                Some(format!("{left}.{right}"))
            }
            _ => None,
        }
    }

    /// Did the binder walk a body for this namespace symbol? The test that lets
    /// §186's empty-exports decline distinguish *nothing was exported* from
    /// *nothing was recorded*. §558.
    fn namespace_body_was_bound(&self, symbol: tsr_binder::SymbolId) -> bool {
        self.binder.symbols().get(symbol).declarations.iter().any(|&declaration| {
            matches!(
                self.node_map.get(declaration),
                Some(Node::ModuleDeclaration(module)) if module.body.is_some()
            )
        })
    }

    /// TS2310 — `Type '{0}' recursively references itself as a base type.`
    ///
    /// `resolveBaseTypesOfInterface` (`checker.go:19260`), whose test is
    /// `t == reducedBaseType || hasBaseType(reducedBaseType, t)`. Upstream
    /// decides it on resolved types; for interfaces and classes the base-type
    /// graph **is** the heritage-clause graph, so this is a reachability query
    /// over `extends` names and symbols with no types involved.
    ///
    /// Reported once per symbol, at its first declaration — interfaces merge,
    /// and the suite compares multisets.
    ///
    /// `docs/architecture/checker-notes-diag2.md` §563.
    fn check_recursive_base_type(&mut self, node: NodeId, typed: Node<'_>) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        let symbol = self.binder.merged_symbol(symbol);
        let declarations = self.binder.symbols().get(symbol).declarations.clone();
        if declarations.first() != Some(&node) {
            return;
        }
        // **A class cycle is TS2506's, not this code's.**
        // `resolveBaseTypesOfClass` checks the cycle first
        // (`checker.go:16977`) and reaches the recursive-base check
        // (`:19260`) only after it returns. §563 ported the second without the
        // first, and the doubling was invisible until §745 built the first —
        // the case failed for a missing TS2506 either way. §777.
        if matches!(typed, Node::ClassDeclaration(_)) {
            let mut at = self.base_class_declaration_of(node);
            for _ in 0..MAX_ALIAS_HOPS {
                let Some(base) = at else { break };
                if base == node {
                    return;
                }
                at = self.base_class_declaration_of(base);
            }
        }
        let mut seen = vec![symbol];
        let mut frontier = self.base_type_symbols(symbol);
        while let Some(next) = frontier.pop() {
            if next == symbol {
                let name = match typed {
                    Node::InterfaceDeclaration(n) => n.name.and_then(|n| n.node_id),
                    Node::ClassDeclaration(n) => n.name.and_then(|n| n.node_id),
                    _ => None,
                };
                let at = name.unwrap_or(node);
                let text = self.identifier_text(at).unwrap_or_default().to_string();
                let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
                let span = self.nodes.span(at);
                self.report(
                    file,
                    Diagnostic::with_args(
                        &messages::TYPE_0_RECURSIVELY_REFERENCES_ITSELF_AS_A_BASE_TYPE,
                        span,
                        [text],
                    ),
                );
                return;
            }
            if seen.contains(&next) {
                continue;
            }
            seen.push(next);
            frontier.extend(self.base_type_symbols(next));
        }
    }

    /// The symbols named by a type's `extends` clauses, across all of its
    /// declarations. `implements` contributes no base type. §563.
    fn base_type_symbols(&mut self, symbol: tsr_binder::SymbolId) -> Vec<tsr_binder::SymbolId> {
        let declarations = self.binder.symbols().get(symbol).declarations.clone();
        let mut out = Vec::new();
        for declaration in declarations {
            let clauses: &[&tsr_ast::HeritageClause<'_>] = match self.node_map.get(declaration) {
                Some(Node::InterfaceDeclaration(n)) => n.heritage_clauses,
                Some(Node::ClassDeclaration(n)) => n.heritage_clauses,
                _ => continue,
            };
            // **A class has exactly one base type** — `getEffectiveBaseTypeNode`
            // returns a single node, so only the first `extends` clause counts.
            // `class C extends A implements B extends C` is a recovery fixture
            // this parser accepts and upstream rejects, and reading its second
            // `extends` made `C` its own base. §564.
            let is_class =
                matches!(self.node_map.get(declaration), Some(Node::ClassDeclaration(_)));
            let mut taken = false;
            for clause in clauses {
                if clause.token.kind != SyntaxKind::ExtendsKeyword {
                    continue;
                }
                if is_class && taken {
                    break;
                }
                taken = true;
                for expression in clause.types {
                    let Some(head) = expression.expression.and_then(|e| e.node_id()) else {
                        continue;
                    };
                    let Some(text) = self.identifier_text(head).map(str::to_string) else {
                        continue;
                    };
                    if let Some(base) = self.binder.resolve_name(
                        self.nodes,
                        self.node_map,
                        head,
                        &text,
                        SymbolFlags::TYPE,
                    ) {
                        out.push(self.binder.merged_symbol(base));
                    }
                }
            }
        }
        out
    }

    /// The `.js`-file grammar checks — TS8002, TS8004, TS8006, TS8009, TS8010
    /// and their siblings.
    ///
    /// A JavaScript file's `SourceFile.JSDiagnostics()`: what upstream's parser
    /// reports from `checkJSSyntax` (`parser.go:6712`) for every node it
    /// finishes.
    ///
    /// Upstream's set is **syntactic**: `Program.GetSyntacticDiagnostics`
    /// (`compiler/program.go:626`) returns it whether or not the file is
    /// type-checked, and the plain-JavaScript filter of
    /// `getBindAndCheckDiagnosticsWithChecker` never sees it. So it is not part
    /// of [`Checker::check_source_file`]'s collection; the caller asks for it
    /// per file, as a program asks a parsed file. Nothing is cached: the walk
    /// visits every node of the file once, the rules read only the node and its
    /// flags, and the result is returned rather than stored. Empty for a file
    /// that is not JavaScript.
    pub fn js_syntax_diagnostics(&mut self, file: NodeId) -> Vec<(NodeId, Diagnostic)> {
        if !self.nodes.flags(file).contains(tsr_ast::NodeFlags::JAVASCRIPT_FILE) {
            return Vec::new();
        }
        // `report_js_only` appends to the check collection; the walk runs
        // against an empty one and hands back what it gathered.
        let saved = std::mem::take(&mut self.diagnostics);
        let mut stack = vec![(file, 0u32)];
        while let Some((node, depth)) = stack.pop() {
            // The check walk's bound (`MAX_CHECK_DEPTH`, ADR-0029).
            if depth > MAX_CHECK_DEPTH {
                continue;
            }
            let Some(typed) = self.node_map.get(node) else { continue };
            self.check_js_syntax(node, typed);
            let start = stack.len();
            tsr_ast::for_each_child_id(typed, |child| stack.push((child, depth + 1)));
            // Document order, as the parser finishes nodes.
            stack[start..].reverse();
        }
        std::mem::replace(&mut self.diagnostics, saved)
    }

    /// `checkJSSyntax` (`parser.go:6711`). Upstream runs this in the **parser**;
    /// this port's parser does not emit diagnostics, so the rules live here and
    /// [`Checker::js_syntax_diagnostics`] runs them as the separate, syntactic
    /// set upstream's parser produces.
    ///
    /// The `NodeFlagsReparsed` guards are omitted: they are JSDoc-reparse
    /// machinery and this parser keeps JSDoc out of the tree.
    ///
    /// `docs/architecture/checker-notes-diag2.md` §569.
    fn check_js_syntax(&mut self, node: NodeId, typed: Node<'_>) {
        if !self.in_js_file(node) {
            return;
        }
        match typed {
            Node::InterfaceDeclaration(n) => {
                if let Some(name) = n.name.and_then(|name| name.node_id) {
                    self.report_js_only(
                        name,
                        &messages::_0_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                        Some("interface"),
                    );
                }
            }
            Node::EnumDeclaration(n) => {
                if let Some(name) = n.name.and_then(|name| name.node_id) {
                    self.report_js_only(
                        name,
                        &messages::_0_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                        Some("enum"),
                    );
                }
            }
            Node::TypeAliasDeclaration(n) => {
                if let Some(name) = n.name.and_then(|name| name.node_id) {
                    self.report_js_only(
                        name,
                        &messages::TYPE_ALIASES_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                        None,
                    );
                }
            }
            Node::ModuleDeclaration(n) => {
                if let Some(name) = n.name.and_then(|name| name.node_id()) {
                    // `scanner.TokenToString(node.Keyword)` — the declaration
                    // carries the keyword it was written with.
                    let keyword = if n.keyword.kind == SyntaxKind::NamespaceKeyword {
                        "namespace"
                    } else {
                        "module"
                    };
                    self.report_js_only(
                        name,
                        &messages::_0_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                        Some(keyword),
                    );
                }
            }
            Node::ImportEqualsDeclaration(_) => {
                self.report_js_only(
                    node,
                    &messages::IMPORT_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                    None,
                );
            }
            Node::ExportAssignment(n) if n.is_export_equals => {
                self.report_js_only(
                    node,
                    &messages::EXPORT_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                    None,
                );
            }
            Node::NonNullExpression(_) => {
                self.report_js_only(
                    node,
                    &messages::NON_NULL_ASSERTIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                    None,
                );
            }
            Node::AsExpression(n) => {
                if let Some(at) = n.r#type.and_then(|t| t.node_id()) {
                    self.report_js_only(
                        at,
                        &messages::TYPE_ASSERTION_EXPRESSIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                        None,
                    );
                }
            }
            Node::SatisfiesExpression(n) => {
                if let Some(at) = n.r#type.and_then(|t| t.node_id()) {
                    self.report_js_only(at, &messages::TYPE_SATISFACTION_EXPRESSIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES, None);
                }
            }
            Node::HeritageClause(n) => {
                if n.token.kind == SyntaxKind::ImplementsKeyword {
                    self.report_js_only(
                        node,
                        &messages::IMPLEMENTS_CLAUSES_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                        None,
                    );
                }
            }
            // The four type-only forms, each with its own printed spelling.
            Node::ImportDeclaration(n) => {
                if n.import_clause.is_some_and(|clause| {
                    clause.phase_modifier.is_some_and(|m| m.kind == SyntaxKind::TypeKeyword)
                }) {
                    self.report_js_only(
                        node,
                        &messages::_0_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                        Some("import type"),
                    );
                }
            }
            Node::ExportDeclaration(n) if n.is_type_only => {
                self.report_js_only(
                    node,
                    &messages::_0_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                    Some("export type"),
                );
            }
            Node::ImportSpecifier(n) if n.is_type_only => {
                self.report_js_only(
                    node,
                    &messages::_0_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                    Some("import...type"),
                );
            }
            Node::ExportSpecifier(n) if n.is_type_only => {
                self.report_js_only(
                    node,
                    &messages::_0_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                    Some("export...type"),
                );
            }
            _ => {}
        }
        // Type annotations and the `?` token, on the kinds that can carry them.
        let annotation = match typed {
            Node::ParameterDeclaration(n) => n.r#type.and_then(|t| t.node_id()),
            Node::PropertyDeclaration(n) => n.r#type.and_then(|t| t.node_id()),
            Node::VariableDeclaration(n) => n.r#type.and_then(|t| t.node_id()),
            Node::FunctionDeclaration(n) => n.r#type.and_then(|t| t.node_id()),
            Node::MethodDeclaration(n) => n.r#type.and_then(|t| t.node_id()),
            Node::ConstructorDeclaration(n) => n.r#type.and_then(|t| t.node_id()),
            Node::GetAccessorDeclaration(n) => n.r#type.and_then(|t| t.node_id()),
            Node::SetAccessorDeclaration(n) => n.r#type.and_then(|t| t.node_id()),
            Node::FunctionExpression(n) => n.r#type.and_then(|t| t.node_id()),
            // **One known wrong line.** `x ? y => ({y}) : z => ({z})` parses
            // here with `: z` as the arrow's return type, because this parser
            // applies TypeScript grammar to `.js` files — which is exactly the
            // divergence upstream avoids by checking JS syntax *during*
            // parsing. Keeping the arm is +7 against +3 without it, inside the
            // build's stated tolerance. §571.
            Node::ArrowFunction(n) => n.r#type.and_then(|t| t.node_id()),
            Node::IndexSignatureDeclaration(n) => n.r#type.and_then(|t| t.node_id()),
            _ => None,
        };
        // A type parameter list — upstream's second `switch`, first arm.
        let type_parameters: &[&tsr_ast::TypeParameterDeclaration<'_>] = match typed {
            Node::ClassDeclaration(n) => n.type_parameters,
            Node::ClassExpression(n) => n.type_parameters,
            Node::MethodDeclaration(n) => n.type_parameters,
            Node::ConstructorDeclaration(n) => n.type_parameters,
            Node::GetAccessorDeclaration(n) => n.type_parameters,
            Node::SetAccessorDeclaration(n) => n.type_parameters,
            Node::FunctionExpression(n) => n.type_parameters,
            Node::FunctionDeclaration(n) => n.type_parameters,
            Node::ArrowFunction(n) => n.type_parameters,
            _ => &[],
        };
        if let Some(first) = type_parameters.first().and_then(|p| p.node_id) {
            self.report_js_only(
                first,
                &messages::TYPE_PARAMETER_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                None,
            );
        }
        // `ast.IsFunctionLike(node) && node.Body() == nil` takes the place of
        // the annotation check: a bodiless function-like is a signature, and an
        // index signature never has a body.
        let signature = match typed {
            Node::FunctionDeclaration(n) => n.body.is_none(),
            Node::MethodDeclaration(n) => n.body.is_none(),
            Node::ConstructorDeclaration(n) => n.body.is_none(),
            Node::GetAccessorDeclaration(n) => n.body.is_none(),
            Node::SetAccessorDeclaration(n) => n.body.is_none(),
            Node::MethodSignatureDeclaration(_) | Node::IndexSignatureDeclaration(_) => true,
            _ => false,
        };
        if signature {
            self.report_js_only(
                node,
                &messages::SIGNATURE_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                None,
            );
        } else if let Some(annotation) = annotation {
            self.report_js_only(
                annotation,
                &messages::TYPE_ANNOTATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                None,
            );
        }
        // `KindParameter`: any modifier but a decorator, reported over the
        // whole modifier list.
        if let Node::ParameterDeclaration(n) = typed
            && n.modifiers.iter().any(|m| matches!(m, tsr_ast::ModifierLike::Token(_)))
        {
            let ids = n.modifiers.iter().filter_map(|m| match m {
                tsr_ast::ModifierLike::Token(token) => token.node_id,
                tsr_ast::ModifierLike::Decorator(decorator) => decorator.node_id,
            });
            self.report_js_only_over(
                node,
                ids,
                &messages::PARAMETER_MODIFIERS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
            );
        }
        // The type-argument arm, reported over the whole argument list.
        let type_arguments: &[tsr_ast::TypeNode<'_>] = match typed {
            Node::CallExpression(n) => n.type_arguments,
            Node::NewExpression(n) => n.type_arguments,
            Node::ExpressionWithTypeArguments(n) => n.type_arguments,
            Node::JsxSelfClosingElement(n) => n.type_arguments,
            Node::JsxOpeningElement(n) => n.type_arguments,
            Node::TaggedTemplateExpression(n) => n.type_arguments,
            _ => &[],
        };
        if !type_arguments.is_empty() {
            self.report_js_only_over(
                node,
                type_arguments.iter().filter_map(tsr_ast::TypeNode::node_id),
                &messages::TYPE_ARGUMENTS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
            );
        }
        let question = match typed {
            Node::ParameterDeclaration(n) => n.question_token,
            Node::PropertyDeclaration(n) => n.postfix_token,
            Node::MethodDeclaration(n) => n.postfix_token,
            _ => None,
        };
        if let Some(question) = question
            .filter(|token| token.kind == SyntaxKind::QuestionToken)
            .and_then(|token| token.node_id)
        {
            self.report_js_only(
                question,
                &messages::THE_0_MODIFIER_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                Some("?"),
            );
        }
        // A TypeScript-only modifier. `ModifierFlagsJavaScript`
        // (`modifierflags.go:52`) is exactly `export`, `static`, `accessor`,
        // `async` and `default`.
        if matches!(
            typed,
            Node::ClassDeclaration(_)
                | Node::ClassExpression(_)
                | Node::MethodDeclaration(_)
                | Node::ConstructorDeclaration(_)
                | Node::GetAccessorDeclaration(_)
                | Node::SetAccessorDeclaration(_)
                | Node::FunctionExpression(_)
                | Node::FunctionDeclaration(_)
                | Node::ArrowFunction(_)
                | Node::VariableStatement(_)
                | Node::PropertyDeclaration(_)
        ) && let Some(modifiers) = modifiers_of(typed)
        {
            for modifier in modifiers {
                let tsr_ast::ModifierLike::Token(token) = modifier else { continue };
                if matches!(
                    token.kind,
                    SyntaxKind::ExportKeyword
                        | SyntaxKind::StaticKeyword
                        | SyntaxKind::AccessorKeyword
                        | SyntaxKind::AsyncKeyword
                        | SyntaxKind::DefaultKeyword
                ) {
                    continue;
                }
                let Some(at) = token.node_id else { continue };
                let text = js_only_modifier_text(token.kind).to_string();
                self.report_js_only(
                    at,
                    &messages::THE_0_MODIFIER_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                    Some(&text),
                );
            }
        }
    }

    /// `jsErrorAtRange` over a node list's range: from the first listed node's
    /// start (`SkipTrivia` of the list's `pos`) to the last one's end.
    fn report_js_only_over(
        &mut self,
        owner: NodeId,
        mut listed: impl Iterator<Item = NodeId>,
        message: &'static tsr_diagnostics::Message,
    ) {
        let Some(first) = listed.next() else { return };
        let Some(file) = self.source_file_of_for_diagnostics(owner) else { return };
        let start = self.nodes.span(first).start;
        let end =
            listed.last().map_or(self.nodes.span(first).end, |last| self.nodes.span(last).end);
        self.report(file, Diagnostic::new(message, tsr_core::Span::new(start, end)));
    }

    /// `jsErrorAtRange` — the node's own span, with an optional argument. §569.
    fn report_js_only(
        &mut self,
        at: NodeId,
        message: &'static tsr_diagnostics::Message,
        argument: Option<&str>,
    ) {
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.nodes.span(at);
        let diagnostic = match argument {
            Some(argument) => Diagnostic::with_args(message, span, [argument.to_string()]),
            None => Diagnostic::new(message, span),
        };
        self.report(file, diagnostic);
    }

    /// TS2462 — `A rest element must be last in a destructuring pattern.`
    ///
    /// Two sites, one message: `checkGrammarBindingElement`
    /// (`grammarchecks.go:1536`) for `var [...a, x]`, and
    /// `checkArrayLiteralDestructuringAssignment` (`checker.go:12683`) for
    /// `[...a, x] = …`. Both are syntactic.
    ///
    /// The assignment form needs the literal to be an **assignment target** —
    /// `[...a, x]` as a value is legal — which upstream gets from its call site
    /// and this reconstructs as *the left of an `=`*.
    ///
    /// `docs/architecture/checker-notes-diag2.md` §608.
    fn check_rest_element_is_last(&mut self, node: NodeId, typed: Node<'_>) {
        if self.file_has_parse_errors {
            return;
        }
        let offenders: Vec<NodeId> = match typed {
            // Both binding patterns take the same shape; only the `kind`
            // token differs. An object pattern reports on the **first**
            // offender alone, matching upstream's `return` after the error at
            // `checker.go:12622`; the array form reports each, because
            // `checkGrammarBindingElement` runs per element. §611.
            Node::BindingPattern(pattern) => {
                let last = pattern.elements.len().saturating_sub(1);
                pattern
                    .elements
                    .iter()
                    .enumerate()
                    .filter_map(|(index, binding)| {
                        (binding.dot_dot_dot_token.is_some() && index != last)
                            .then_some(binding.node_id)
                            .flatten()
                    })
                    .take(if self.nodes.kind(node) == SyntaxKind::ObjectBindingPattern {
                        1
                    } else {
                        usize::MAX
                    })
                    .collect()
            }
            Node::ObjectLiteralExpression(literal) => {
                if !self.is_assignment_target_literal(node) {
                    return;
                }
                let last = literal.properties.len().saturating_sub(1);
                literal
                    .properties
                    .iter()
                    .enumerate()
                    .filter_map(|(index, property)| match property {
                        tsr_ast::ObjectLiteralElementLike::SpreadAssignment(spread)
                            if index != last =>
                        {
                            spread.node_id
                        }
                        _ => None,
                    })
                    .take(1)
                    .collect()
            }
            Node::ArrayLiteralExpression(literal) => {
                if !self.is_assignment_target_literal(node) {
                    return;
                }
                let last = literal.elements.len().saturating_sub(1);
                literal
                    .elements
                    .iter()
                    .enumerate()
                    .filter_map(|(index, element)| match element {
                        tsr_ast::Expression::SpreadElement(spread) if index != last => {
                            spread.node_id
                        }
                        _ => None,
                    })
                    .collect()
            }
            _ => return,
        };
        for at in offenders {
            let Some(file) = self.source_file_of_for_diagnostics(at) else { continue };
            let span = self.error_span(at);
            self.report(
                file,
                Diagnostic::new(
                    &messages::A_REST_ELEMENT_MUST_BE_LAST_IN_A_DESTRUCTURING_PATTERN,
                    span,
                ),
            );
        }
    }

    /// Is this array literal the left-hand side of an assignment — a
    /// destructuring target rather than a value? §608.
    fn is_assignment_target_literal(&self, node: NodeId) -> bool {
        let Some(parent) = self.nodes.parent(node) else { return false };
        matches!(
            self.node_map.get(parent),
            Some(Node::BinaryExpression(binary))
                if binary.operator_token.is_some_and(|t| t.kind == SyntaxKind::EqualsToken)
                    && binary.left.and_then(|left| left.node_id()) == Some(node)
        )
    }

    /// TS18058 / TS18059 / TS18060 — the deferred-import clause.
    ///
    /// `checkGrammarImportClause`'s `KindDeferKeyword` case
    /// (`grammarchecks.go:2127`): one guard, three exclusive arms in order,
    /// each reporting on the **clause**. A namespace import is the only legal
    /// deferred form.
    ///
    /// `module_kind` comes from §478. `docs/architecture/checker-notes-diag2.md` §615.
    fn check_deferred_import_clause(&mut self, typed: Node<'_>) {
        if self.file_has_parse_errors {
            return;
        }
        let Node::ImportDeclaration(declaration) = typed else { return };
        let Some(clause) = declaration.import_clause else { return };
        if clause.phase_modifier.is_none_or(|m| m.kind != SyntaxKind::DeferKeyword) {
            return;
        }
        let Some(at) = clause.node_id else { return };
        let message = if clause.name.is_some() {
            &messages::DEFAULT_IMPORTS_ARE_NOT_ALLOWED_IN_A_DEFERRED_IMPORT
        } else if matches!(
            clause.named_bindings,
            Some(tsr_ast::NamedImportBindings::NamedImports(_))
        ) {
            &messages::NAMED_IMPORTS_ARE_NOT_ALLOWED_IN_A_DEFERRED_IMPORT
        } else if !matches!(
            self.module_kind,
            tsr_core::ModuleKind::ESNext | tsr_core::ModuleKind::Preserve
        ) {
            &messages::DEFERRED_IMPORTS_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_FLAG_IS_SET_TO_ESNEXT_OR_PRESERVE
        } else {
            return;
        };
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        // `grammarErrorOnNode(&node.Node)` — the **clause's** range, which
        // upstream starts at the phase modifier. This parser puts the modifier
        // outside the clause's span, so the clause alone reports at the `foo`
        // of `import defer foo` and upstream reports at the `defer`. Taking
        // the modifier's span reproduces upstream's column exactly; widening
        // the clause's span is the faithful fix and belongs to `tsr-parser`.
        // §615.
        let span = clause
            .phase_modifier
            .and_then(|m| m.node_id)
            .map_or_else(|| self.nodes.span(at), |m| self.nodes.span(m));
        self.report(file, Diagnostic::new(message, span));
    }

    /// TS1194 — `Export declarations are not permitted in a namespace.`
    ///
    /// Two sites: `checker.go:5525` for the clause-only form, reporting on the
    /// **statement**, and `checker.go:5345` for the form with a module
    /// specifier, reporting on the **module name**.
    ///
    /// `inAmbientNamespaceDeclaration` is what makes
    /// `declare namespace N { export { y } }` legal, so the ambient flag is
    /// read rather than assumed. §584.
    ///
    /// `docs/architecture/checker-notes-diag2.md` §619.
    fn check_export_declaration_in_namespace(&mut self, node: NodeId, typed: Node<'_>) {
        if self.file_has_parse_errors {
            return;
        }
        let Node::ExportDeclaration(declaration) = typed else { return };
        let Some(parent) = self.nodes.parent(node) else { return };
        if self.nodes.kind(parent) == SyntaxKind::SourceFile {
            return;
        }
        let in_module_block = self.nodes.kind(parent) == SyntaxKind::ModuleBlock;
        let in_ambient_external_module = in_module_block
            && self.nodes.parent(parent).is_some_and(|owner| self.is_ambient_module_node(owner));
        if in_ambient_external_module {
            return;
        }
        let specifier = declaration.module_specifier.and_then(|s| s.node_id());
        if specifier.is_none() && in_module_block && self.declaration_is_in_an_ambient_context(node)
        {
            // `inAmbientNamespaceDeclaration` — legal.
            return;
        }
        // `export * from "m"` has no export clause and takes upstream's other
        // branch entirely; only a named clause or a specifier reaches here.
        if declaration.export_clause.is_none() && specifier.is_none() {
            return;
        }
        let at = specifier.unwrap_or(node);
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        self.report(
            file,
            Diagnostic::new(&messages::EXPORT_DECLARATIONS_ARE_NOT_PERMITTED_IN_A_NAMESPACE, span),
        );
    }

    /// TS1221 — `Generators are not allowed in an ambient context.`
    /// TS1222 — `An overload signature cannot be declared as a generator.`
    ///
    /// `checkGrammarForGenerator` (`grammarchecks.go:992`) in full: one guard
    /// on the asterisk and two exclusive arms, both reporting on the
    /// **asterisk** rather than the declaration.
    ///
    /// Transcribed whole because the function is eleven lines and has no third
    /// arm to decline — §634, and §633 for why that distinction decides between
    /// transcribing and taking arms.
    fn check_grammar_for_generator(&mut self, node: NodeId, typed: Node<'_>) {
        if self.file_has_parse_errors {
            return;
        }
        let (asterisk, body_is_present) = match typed {
            Node::FunctionDeclaration(n) => (n.asterisk_token, n.body.is_some()),
            Node::FunctionExpression(n) => (n.asterisk_token, n.body.is_some()),
            Node::MethodDeclaration(n) => (n.asterisk_token, n.body.is_some()),
            _ => return,
        };
        let Some(asterisk) = asterisk.and_then(|token| token.node_id) else { return };
        let message = if self.declaration_is_in_an_ambient_context(node) {
            &messages::GENERATORS_ARE_NOT_ALLOWED_IN_AN_AMBIENT_CONTEXT
        } else if !body_is_present {
            &messages::AN_OVERLOAD_SIGNATURE_CANNOT_BE_DECLARED_AS_A_GENERATOR
        } else {
            return;
        };
        let Some(file) = self.source_file_of_for_diagnostics(asterisk) else { return };
        let span = self.nodes.span(asterisk);
        self.report(file, Diagnostic::new(message, span));
    }

    /// TS1169 — `A computed property name in an interface must refer to an
    /// expression whose type is a literal type or a unique symbol type.`
    ///
    /// `checkGrammarComputedPropertyName`'s interface arm
    /// (`grammarchecks.go:1472`) through `checkGrammarForInvalidDynamicName`,
    /// whose first arm is `!IsEntityNameExpression(expression)` — syntactic.
    ///
    /// The `isNonBindableDynamicName` test above it needs the name's type; an
    /// expression that is not an entity name can never be late-bound, so the
    /// syntactic condition is sufficient and silent everywhere it is not.
    /// The *type literal* and *method overload* siblings carry their own codes
    /// and are not built (§501). §636.
    fn check_interface_computed_name(&mut self, node: NodeId, typed: Node<'_>) {
        if self.file_has_parse_errors {
            return;
        }
        let name = match typed {
            Node::PropertySignatureDeclaration(n) => n.name,
            Node::MethodSignatureDeclaration(n) => n.name,
            _ => return,
        };
        let tsr_ast::PropertyName::ComputedPropertyName(computed) = name else { return };
        // Upstream keys the message on the **parent kind** and shares
        // everything else (`grammarchecks.go:1471-1475`). The method-overload
        // sibling has no corpus case and is not built (§501). §638.
        let Some(parent) = self.nodes.parent(node) else { return };
        let message = match self.nodes.kind(parent) {
            SyntaxKind::InterfaceDeclaration => {
                &messages::A_COMPUTED_PROPERTY_NAME_IN_AN_INTERFACE_MUST_REFER_TO_AN_EXPRESSION_WHOSE_TYPE_IS_A_LITERAL_TYPE_OR_A_UNIQUE_SYMBOL_TYPE
            }
            SyntaxKind::TypeLiteral => {
                &messages::A_COMPUTED_PROPERTY_NAME_IN_A_TYPE_LITERAL_MUST_REFER_TO_AN_EXPRESSION_WHOSE_TYPE_IS_A_LITERAL_TYPE_OR_A_UNIQUE_SYMBOL_TYPE
            }
            _ => return,
        };
        let Some(expression) = computed.expression.and_then(|e| e.node_id()) else { return };
        // An entity name — `a`, `A.b`, `Symbol.iterator` — may be late-bound, so
        // it is upstream's own exclusion; a literal is not dynamic at all.
        if self.is_entity_name_expression(expression)
            || matches!(
                self.nodes.kind(expression),
                SyntaxKind::StringLiteral
                    | SyntaxKind::NumericLiteral
                    | SyntaxKind::NoSubstitutionTemplateLiteral
            )
        {
            return;
        }
        let Some(at) = computed.node_id else { return };
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.nodes.span(at);
        self.report(file, Diagnostic::new(message, span));
    }

    /// `ast.IsEntityNameExpression` — an identifier, or a property access chain
    /// of identifiers. §636.
    pub(crate) fn is_entity_name_expression(&self, node: NodeId) -> bool {
        match self.node_map.get(node) {
            Some(Node::Identifier(_)) => true,
            Some(Node::PropertyAccessExpression(access)) => {
                matches!(access.name, Some(tsr_ast::MemberName::Identifier(_)))
                    && access
                        .expression
                        .and_then(|e| e.node_id())
                        .is_some_and(|inner| self.is_entity_name_expression(inner))
            }
            _ => false,
        }
    }

    /// TS2307 for a **dynamic `import()`**'s specifier.
    ///
    /// The third specifier-bearing syntax after an import declaration (§357)
    /// and an `import(...)` type (§753). Resolved and reported inline for the
    /// same reason §753 is: `check_module_specifier` carries
    /// `checkExternalImportOrExportDeclaration`'s position test, which a call
    /// nested in an expression cannot pass. §755.
    fn check_dynamic_import_specifier(&mut self, typed: Node<'_>) {
        if self.file_has_parse_errors {
            return;
        }
        let Node::CallExpression(call) = typed else { return };
        if !matches!(
            call.expression,
            Some(tsr_ast::Expression::KeywordExpression(keyword))
                if keyword.kind == SyntaxKind::ImportKeyword
        ) {
            return;
        }
        let Some(argument) = call.arguments.first().and_then(tsr_ast::Expression::node_id) else {
            return;
        };
        let Some(Node::StringLiteral(text)) = self.node_map.get(argument) else { return };
        if !self.module_specifier_unfindable_for_diagnostics(argument) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(argument) else { return };
        let span = self.error_span(argument);
        self.report_module_not_found(
            file,
            argument,
            span,
            text.text,
            &messages::CANNOT_FIND_MODULE_0_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS,
        );
    }

    /// TS1323 — `Dynamic imports are only supported when the '--module' flag is
    /// set to es2020, es2022, esnext, commonjs, amd, system, umd, node16,
    /// node18, node20, or nodenext.`
    ///
    /// `checkGrammarImportCallExpression`'s third arm
    /// (`grammarchecks.go:2171`), which is one comparison against
    /// `moduleKind`. The function's other arms are named and not built: the
    /// `verbatimModuleSyntax` one needs an option this port does not read, the
    /// `import.meta` one is TS18060 (§615), and the type-arguments one has no
    /// corpus case. §642.
    fn check_dynamic_import_module_kind(&mut self, node: NodeId, typed: Node<'_>) {
        if self.file_has_parse_errors || self.module_kind != tsr_core::ModuleKind::ES2015 {
            return;
        }
        let Node::CallExpression(call) = typed else { return };
        if !matches!(
            call.expression,
            Some(tsr_ast::Expression::KeywordExpression(keyword))
                if keyword.kind == SyntaxKind::ImportKeyword
        ) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(
            file,
            Diagnostic::new(
                &messages::DYNAMIC_IMPORTS_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_FLAG_IS_SET_TO_ES2020_ES2022_ESNEXT_COMMONJS_AMD_SYSTEM_UMD_NODE16_NODE18_NODE20_OR_NODENEXT,
                span,
            ),
        );
    }

    /// TS1206 — `Decorators are not valid here.`
    ///
    /// `checkGrammarModifiers`' decorator arm (`grammarchecks.go:246`) when
    /// `nodeCanBeDecorated` is false, whose first test is
    /// (`ast/utilities.go:4256`):
    ///
    /// ```go
    /// if useLegacyDecorators && node.Name() != nil && IsPrivateIdentifier(node.Name()) {
    ///     return false
    /// }
    /// ```
    ///
    /// A private name is decoratable under standard decorators and not under
    /// legacy ones, so `experimentalDecorators` decides it. The rest of
    /// `nodeCanBeDecorated` is not ported (§501), and the overload sibling has
    /// its own code. §644.
    fn check_decorated_private_name(&mut self, node: NodeId, typed: Node<'_>) {
        if self.file_has_parse_errors || !self.legacy_decorators {
            return;
        }
        let name = match typed {
            Node::PropertyDeclaration(n) => Some(n.name),
            Node::MethodDeclaration(n) => Some(n.name),
            Node::GetAccessorDeclaration(n) => Some(n.name),
            Node::SetAccessorDeclaration(n) => Some(n.name),
            _ => None,
        };
        if !matches!(name, Some(tsr_ast::PropertyName::PrivateIdentifier(_))) {
            return;
        }
        let Some(modifiers) = modifiers_of(typed) else { return };
        if !modifiers.iter().any(|m| matches!(m, tsr_ast::ModifierLike::Decorator(_))) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        // `grammarErrorOnFirstToken(node)` — the node's own start, which is the
        // decorator's `@`.
        let span = self.nodes.span(node);
        self.report(file, Diagnostic::new(&messages::DECORATORS_ARE_NOT_VALID_HERE, span));
    }

    /// TS18006 — `Classes may not have a field named 'constructor'.`
    ///
    /// `checkGrammarProperty` (`grammarchecks.go:1888`): a class property whose
    /// name is the **string literal** `"constructor"`. The identifier form is a
    /// constructor rather than a field, and a computed `["constructor"]` is
    /// late-bound and legal — the corpus fixture carries both as controls.
    ///
    /// The siblings in that function — the mapped-type check, the class
    /// property's `checkGrammarForInvalidDynamicName` (§636's family) and the
    /// auto-accessor question mark — have no corpus cases and are not built
    /// (§501). §653.
    fn check_field_named_constructor(&mut self, node: NodeId, typed: Node<'_>) {
        if self.file_has_parse_errors {
            return;
        }
        let name = match typed {
            Node::PropertyDeclaration(n) => n.name,
            _ => return,
        };
        let tsr_ast::PropertyName::StringLiteral(literal) = name else { return };
        if literal.text != "constructor" {
            return;
        }
        if self.nodes.parent(node).is_none_or(|parent| {
            !matches!(
                self.nodes.kind(parent),
                SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
            )
        }) {
            return;
        }
        let Some(at) = literal.node_id else { return };
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.nodes.span(at);
        self.report(
            file,
            Diagnostic::new(&messages::CLASSES_MAY_NOT_HAVE_A_FIELD_NAMED_CONSTRUCTOR, span),
        );
    }

    /// `IsInstantiatedModule` (`ast/utilities.go:2443`).
    fn is_instantiated_module(&self, node: NodeId) -> bool {
        let Some(typed) = self.node_map.get(node) else { return true };
        let mut parents: Vec<_> =
            self.nodes.ancestors(node).filter_map(|ancestor| self.node_map.get(ancestor)).collect();
        parents.reverse();
        match tsr_ast::module_instance_state(typed, &parents) {
            tsr_ast::ModuleInstanceState::Instantiated => true,
            tsr_ast::ModuleInstanceState::ConstEnumOnly => self.preserve_const_enums,
            tsr_ast::ModuleInstanceState::NonInstantiated => false,
        }
    }

    /// The kinds `checkUnreachable`'s `reportError` accepts, minus the
    /// variable-statement clause the binder's own predicate already applied —
    /// see [`Checker::check_unreachable`].
    fn is_reportable_unreachable_kind(kind: SyntaxKind) -> bool {
        if kind == SyntaxKind::EmptyStatement {
            return false;
        }
        if matches!(
            kind,
            SyntaxKind::ClassDeclaration
                | SyntaxKind::EnumDeclaration
                | SyntaxKind::ModuleDeclaration
        ) {
            return true;
        }
        // `IsStatementButNotDeclaration`: a declaration is hoisted and is not
        // "code" that control reaches.
        if matches!(
            kind,
            SyntaxKind::FunctionDeclaration
                | SyntaxKind::InterfaceDeclaration
                | SyntaxKind::TypeAliasDeclaration
                | SyntaxKind::ImportDeclaration
                | SyntaxKind::ImportEqualsDeclaration
                | SyntaxKind::ExportDeclaration
                | SyntaxKind::ExportAssignment
                | SyntaxKind::ModuleBlock
        ) {
            return false;
        }
        (SyntaxKind::FIRST_STATEMENT as u16) <= (kind as u16)
            && (kind as u16) <= (SyntaxKind::LAST_STATEMENT as u16)
    }

    fn check_comma_left(&mut self, node: NodeId, left: Option<NodeId>) {
        // **`allowUnreachableCode` is upstream's; `file_has_parse_errors` is
        // not.** `checkComma` suppresses on the first and asks nothing about
        // the second, and this port declined `[a, b, ...]`'s two TS2695 lines
        // because it had already emitted the TS1005 beside them. The class is a
        // mixture (§900), so this is its own measurement. §910.
        if self.allow_unreachable_code {
            return;
        }
        let Some(left) = left else { return };
        if !self.is_side_effect_free(left) || self.is_indirect_call(node) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(left) else { return };
        let span = self.error_span(left);
        self.report(
            file,
            Diagnostic::new(
                &messages::LEFT_SIDE_OF_COMMA_OPERATOR_IS_UNUSED_AND_HAS_NO_SIDE_EFFECTS,
                span,
            ),
        );
    }

    /// `isSideEffectFree` (`checker.go:13011`), kind for kind.
    fn is_side_effect_free(&self, node: NodeId) -> bool {
        let mut node = node;
        // `ast.SkipParentheses`.
        while let Some(Node::ParenthesizedExpression(inner)) = self.node_map.get(node) {
            let Some(id) = inner.expression.and_then(|e| e.node_id()) else { return false };
            node = id;
        }
        match self.node_map.get(node) {
            Some(
                Node::Identifier(_)
                | Node::StringLiteral(_)
                | Node::RegularExpressionLiteral(_)
                | Node::TaggedTemplateExpression(_)
                | Node::TemplateExpression(_)
                | Node::NoSubstitutionTemplateLiteral(_)
                | Node::NumericLiteral(_)
                | Node::BigIntLiteral(_)
                | Node::FunctionExpression(_)
                | Node::ClassExpression(_)
                | Node::ArrowFunction(_)
                | Node::ArrayLiteralExpression(_)
                | Node::ObjectLiteralExpression(_)
                | Node::TypeOfExpression(_)
                | Node::NonNullExpression(_)
                | Node::JsxSelfClosingElement(_)
                | Node::JsxElement(_),
            ) => true,
            // `true`, `false`, `null` and `undefined` are one kind here where
            // upstream has four; the discriminator is the token kind.
            Some(Node::KeywordExpression(_)) => matches!(
                self.nodes.kind(node),
                SyntaxKind::TrueKeyword
                    | SyntaxKind::FalseKeyword
                    | SyntaxKind::NullKeyword
                    | SyntaxKind::UndefinedKeyword
            ),
            Some(Node::ConditionalExpression(conditional)) => {
                conditional
                    .when_true
                    .and_then(|e| e.node_id())
                    .is_some_and(|id| self.is_side_effect_free(id))
                    && conditional
                        .when_false
                        .and_then(|e| e.node_id())
                        .is_some_and(|id| self.is_side_effect_free(id))
            }
            Some(Node::BinaryExpression(binary)) => {
                let assignment = binary.operator_token.is_some_and(|token| {
                    matches!(
                        token.kind,
                        SyntaxKind::EqualsToken
                            | SyntaxKind::PlusEqualsToken
                            | SyntaxKind::MinusEqualsToken
                            | SyntaxKind::AsteriskEqualsToken
                            | SyntaxKind::AsteriskAsteriskEqualsToken
                            | SyntaxKind::SlashEqualsToken
                            | SyntaxKind::PercentEqualsToken
                            | SyntaxKind::LessThanLessThanEqualsToken
                            | SyntaxKind::GreaterThanGreaterThanEqualsToken
                            | SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
                            | SyntaxKind::AmpersandEqualsToken
                            | SyntaxKind::BarEqualsToken
                            | SyntaxKind::CaretEqualsToken
                            | SyntaxKind::BarBarEqualsToken
                            | SyntaxKind::AmpersandAmpersandEqualsToken
                            | SyntaxKind::QuestionQuestionEqualsToken
                    )
                });
                !assignment
                    && binary
                        .left
                        .and_then(|e| e.node_id())
                        .is_some_and(|id| self.is_side_effect_free(id))
                    && binary
                        .right
                        .and_then(|e| e.node_id())
                        .is_some_and(|id| self.is_side_effect_free(id))
            }
            // "Unary operators ~, !, + and - have no side effects. The rest do."
            Some(Node::PrefixUnaryExpression(unary)) => matches!(
                unary.operator.kind,
                SyntaxKind::ExclamationToken
                    | SyntaxKind::PlusToken
                    | SyntaxKind::MinusToken
                    | SyntaxKind::TildeToken
            ),
            _ => false,
        }
    }

    /// `isIndirectCall` (`checker.go:13039`): `(0, x.f)(…)` and `(0, eval)(…)`,
    /// the idiom for calling without passing `this`.
    fn is_indirect_call(&self, comma: NodeId) -> bool {
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(comma) else { return false };
        let zero_left = binary
            .left
            .and_then(|left| left.node_id())
            .and_then(|id| self.node_map.get(id))
            .is_some_and(
                |left| matches!(left, Node::NumericLiteral(literal) if literal.text == "0"),
            );
        if !zero_left {
            return false;
        }
        let right_is_target = binary
            .right
            .and_then(|right| right.node_id())
            .and_then(|id| self.node_map.get(id))
            .is_some_and(|right| match right {
                Node::PropertyAccessExpression(_) | Node::ElementAccessExpression(_) => true,
                Node::Identifier(identifier) => identifier.text == "eval",
                _ => false,
            });
        if !right_is_target {
            return false;
        }
        let Some(parenthesis) = self.nodes.parent(comma) else { return false };
        if self.nodes.kind(parenthesis) != SyntaxKind::ParenthesizedExpression {
            return false;
        }
        self.nodes.parent(parenthesis).is_some_and(|owner| match self.node_map.get(owner) {
            Some(Node::CallExpression(call)) => {
                call.expression.and_then(|e| e.node_id()) == Some(parenthesis)
            }
            Some(Node::TaggedTemplateExpression(_)) => true,
            _ => false,
        })
    }

    /// `checkGrammarBreakOrContinueStatement` (`grammarchecks.go:1480`) — five
    /// diagnostics from one upward walk.
    ///
    /// | code | message |
    /// |---|---|
    /// | TS1107 | `Jump target cannot cross function boundary.` |
    /// | TS1104 | `A 'continue' statement can only be used within an enclosing iteration statement.` |
    /// | TS1105 | `A 'break' statement can only be used within an enclosing iteration or switch statement.` |
    /// | TS1115 | `A 'continue' statement can only jump to a label of an enclosing iteration statement.` |
    /// | TS1116 | `A 'break' statement can only jump to a label of an enclosing statement.` |
    ///
    /// Wholly syntactic — it reads kinds and one label's text — which is why it
    /// is ported entire rather than bounded. `checker-notes-diag2.md` §10
    /// records why that distinction is the one that matters for a diagnostic
    /// rule: there is no incomplete subsystem for this condition to consult, so
    /// there is nothing for it to be wrong about.
    ///
    /// The walk **starts at the statement itself**, not its parent, so a `break`
    /// whose own kind is function-like cannot occur and the first iteration is
    /// always a no-op — kept that way because it is upstream's loop and moving
    /// the start is the kind of edit that silently changes a boundary case.
    fn check_break_or_continue(
        &mut self,
        node: NodeId,
        is_break: bool,
        label: Option<&str>,
        ambient: bool,
    ) {
        // `checkBreakOrContinueStatement` (`checker.go:4081`) runs this grammar
        // check **only if** `checkGrammarStatementInAmbientContext` did not
        // report — and in an ambient context that function reports TS1036
        // `Statements are not allowed in ambient contexts` and short-circuits.
        // TS1036 is not ported (19 cases of its own on `diaggap.rs`'s board), so
        // the short-circuit is reproduced as a refusal: silence where upstream
        // says something else. It was worth exactly the two wrong lines this
        // rule's first measurement produced, `parserBreakStatement1.d` and
        // `parserContinueStatement1.d`, both one-line `.d.ts` files.
        if self.file_has_parse_errors || ambient {
            return;
        }
        let mut current = Some(node);
        while let Some(id) = current {
            if self.is_function_like_or_static_block(id) {
                self.report_grammar(node, &messages::JUMP_TARGET_CANNOT_CROSS_FUNCTION_BOUNDARY);
                return;
            }
            match self.node_map.get(id) {
                Some(Node::LabeledStatement(labeled)) => {
                    let matches_target = label.is_some_and(|target| {
                        labeled.label.is_some_and(|name| name.text == target)
                    });
                    if matches_target {
                        // `continue` may only target a label on an iteration
                        // statement; `break` may target any labelled statement.
                        let misplaced = !is_break
                            && !labeled
                                .statement
                                .and_then(|statement| statement.node_id())
                                .is_some_and(|statement| {
                                    self.is_iteration_statement(statement, true)
                                });
                        if misplaced {
                            self.report_grammar(
                                node,
                                &messages::A_CONTINUE_STATEMENT_CAN_ONLY_JUMP_TO_A_LABEL_OF_AN_ENCLOSING_ITERATION_STATEMENT,
                            );
                        }
                        return;
                    }
                }
                Some(Node::SwitchStatement(_)) => {
                    if is_break && label.is_none() {
                        return;
                    }
                }
                _ => {
                    if label.is_none() && self.is_iteration_statement(id, false) {
                        return;
                    }
                }
            }
            current = self.nodes.parent(id);
        }
        let message = match (label.is_some(), is_break) {
            (true, true) => &messages::A_BREAK_STATEMENT_CAN_ONLY_JUMP_TO_A_LABEL_OF_AN_ENCLOSING_STATEMENT,
            (true, false) => {
                &messages::A_CONTINUE_STATEMENT_CAN_ONLY_JUMP_TO_A_LABEL_OF_AN_ENCLOSING_ITERATION_STATEMENT
            }
            (false, true) => {
                &messages::A_BREAK_STATEMENT_CAN_ONLY_BE_USED_WITHIN_AN_ENCLOSING_ITERATION_OR_SWITCH_STATEMENT
            }
            (false, false) => {
                &messages::A_CONTINUE_STATEMENT_CAN_ONLY_BE_USED_WITHIN_AN_ENCLOSING_ITERATION_STATEMENT
            }
        };
        self.report_grammar(node, message);
    }

    /// `ast.IsIterationStatement` (`ast/utilities.go:463`).
    fn is_iteration_statement(&self, node: NodeId, look_in_labeled: bool) -> bool {
        match self.nodes.kind(node) {
            SyntaxKind::ForStatement
            | SyntaxKind::ForInStatement
            | SyntaxKind::ForOfStatement
            | SyntaxKind::DoStatement
            | SyntaxKind::WhileStatement => true,
            SyntaxKind::LabeledStatement => {
                look_in_labeled
                    && matches!(self.node_map.get(node), Some(Node::LabeledStatement(labeled))
                        if labeled
                            .statement
                            .and_then(|statement| statement.node_id())
                            .is_some_and(|statement| self.is_iteration_statement(statement, true)))
            }
            _ => false,
        }
    }

    /// `ast.IsFunctionLikeOrClassStaticBlockDeclaration`.
    pub(crate) fn is_function_like_or_static_block(&self, node: NodeId) -> bool {
        matches!(
            self.nodes.kind(node),
            SyntaxKind::FunctionDeclaration
                | SyntaxKind::FunctionExpression
                | SyntaxKind::ArrowFunction
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::MethodSignature
                | SyntaxKind::Constructor
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor
                | SyntaxKind::CallSignature
                | SyntaxKind::ConstructSignature
                | SyntaxKind::IndexSignature
                | SyntaxKind::FunctionType
                | SyntaxKind::ConstructorType
                | SyntaxKind::ClassStaticBlockDeclaration
        )
    }

    /// `grammarErrorOnNode` (`grammarchecks.go`): a diagnostic spanning the
    /// whole node, with no arguments.
    fn report_grammar(&mut self, node: NodeId, message: &'static tsr_diagnostics::Message) {
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(file, Diagnostic::new(message, span));
    }

    /// `checkGrammarStatementInAmbientContext` (`grammarchecks.go:2047`) —
    /// TS1036 and TS1183.
    ///
    /// | code | message | when |
    /// |---|---|---|
    /// | TS1183 | `An implementation cannot be declared in ambient contexts.` | the statement's parent is function-like or an accessor |
    /// | TS1036 | `Statements are not allowed in ambient contexts.` | the parent is a `Block`, `ModuleBlock` or `SourceFile`, **once per parent** |
    ///
    /// Returns whether it reported, because thirteen of upstream's
    /// `checkXxxStatement` functions are written as
    /// `if !c.checkGrammarStatementInAmbientContext(node) { … }` — the report is
    /// a short-circuit, not an addition, and §11's two wrong lines came from not
    /// having it.
    ///
    /// **"Once per parent" is the whole of the state.** Upstream keeps
    /// `hasReportedStatementInAmbientContext` on the *block's* node links so a
    /// `declare module "m" { a; b; c; }` reports once rather than three times.
    /// Under exact-multiset comparison reporting three would fail the case as
    /// surely as reporting none, so the set below is load-bearing rather than an
    /// optimisation.
    fn check_grammar_statement_in_ambient_context(&mut self, node: NodeId, ambient: bool) -> bool {
        if !ambient || self.file_has_parse_errors {
            return false;
        }
        let Some(parent) = self.nodes.parent(node) else { return false };
        // **The flag is keyed on the node here and on the *parent* below, and
        // it is the same field upstream** (`links.hasReportedStatementInAmbientContext`,
        // `grammarchecks.go:2051` and `:2064`). That is not a detail: a method
        // body in an ambient class reports TS1183 *for the block*, and the
        // statements inside it then find the block already flagged and stay
        // silent. Keying the two branches separately reported both, which was
        // this rule's one wrong line — `initializersInDeclarations`, where
        // upstream records TS1183 at the body and nothing at the `return`.
        if self.is_function_like_or_static_block(parent)
            || matches!(self.nodes.kind(parent), SyntaxKind::GetAccessor | SyntaxKind::SetAccessor)
        {
            // Both arms report through `grammarErrorOnFirstToken`: the span is
            // the statement's first token, not the whole statement.
            if !self.ambient_statement_reported.contains(&node)
                && self.grammar_error_on_first_token(
                    node,
                    &messages::AN_IMPLEMENTATION_CANNOT_BE_DECLARED_IN_AMBIENT_CONTEXTS,
                )
            {
                self.ambient_statement_reported.insert(node);
                return true;
            }
            return false;
        }
        if matches!(
            self.nodes.kind(parent),
            SyntaxKind::Block | SyntaxKind::ModuleBlock | SyntaxKind::SourceFile
        ) && !self.ambient_statement_reported.contains(&parent)
            && self.grammar_error_on_first_token(
                node,
                &messages::STATEMENTS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS,
            )
        {
            self.ambient_statement_reported.insert(parent);
            return true;
        }
        // "We must be parented by a statement. If so, there's no need to report
        // the error as our parent will have already done it."
        false
    }

    /// `checkFunctionOrConstructorSymbolWorker`'s agreement arms
    /// (`checker.go:3684-3686`): when the symbol has an overload (a
    /// function-like declaration without a body), `checkFlagAgreementBetweenOverloads`
    /// (`:3497`) and `checkQuestionTokenAgreementBetweenOverloads` (`:3539`)
    /// over **all** of the symbol's declarations.
    ///
    /// | code | deviation from the canonical overload |
    /// |---|---|
    /// | TS2383 | `export`, within one file |
    /// | TS2384 | ambience, within one file |
    /// | TS2385 | `private`/`protected` |
    /// | TS2386 | the `?` postfix |
    /// | TS2512 | `abstract` |
    ///
    /// The canonical overload is `getCanonicalOverload`: the implementation
    /// when it shares a parent with the first declaration, else the first.
    /// The `switch` is ordered and reports at most one deviation per
    /// declaration. Run before the worker's single-file and same-parent
    /// bounds, which concern only the implementation-presence arms: upstream
    /// groups by file here itself.
    fn check_overload_agreement(&mut self, declarations: &[NodeId], current_file: NodeId) {
        let function_like: Vec<NodeId> = declarations
            .iter()
            .copied()
            .filter(|&declaration| self.is_function_or_method_or_constructor(declaration))
            .collect();
        if !function_like.iter().any(|&declaration| !self.declaration_has_body(declaration)) {
            return;
        }
        let implementation = function_like
            .iter()
            .copied()
            .find(|&declaration| self.declaration_has_body(declaration));
        let canonical_of = |checker: &Self, overloads: &[NodeId]| -> NodeId {
            match implementation {
                Some(body) if checker.nodes.parent(body) == checker.nodes.parent(overloads[0]) => {
                    body
                }
                _ => overloads[0],
            }
        };
        let (mut some, mut all) = (0u8, OverloadFlags::ALL);
        let (mut some_optional, mut all_optional) = (false, true);
        for &declaration in &function_like {
            let flags = self.overload_effective_flags(declaration, current_file);
            some |= flags;
            all &= flags;
            let optional = self.declaration_is_optional(declaration);
            some_optional |= optional;
            all_optional &= optional;
        }
        if some ^ all != 0 {
            let canonical =
                self.overload_effective_flags(canonical_of(self, declarations), current_file);
            let mut groups: Vec<(Option<NodeId>, Vec<NodeId>)> = Vec::new();
            for &declaration in declarations {
                let file = self.source_file_of_for_diagnostics(declaration);
                match groups.iter_mut().find(|(each, _)| *each == file) {
                    Some((_, members)) => members.push(declaration),
                    None => groups.push((file, vec![declaration])),
                }
            }
            for (_, overloads) in &groups {
                let canonical_for_file =
                    self.overload_effective_flags(canonical_of(self, overloads), current_file);
                for &overload in overloads {
                    let flags = self.overload_effective_flags(overload, current_file);
                    let deviation = flags ^ canonical;
                    let deviation_in_file = flags ^ canonical_for_file;
                    let name = self.declaration_name_of(overload);
                    let (message, at) = if deviation_in_file & OverloadFlags::EXPORT != 0 {
                        (&messages::OVERLOAD_SIGNATURES_MUST_ALL_BE_EXPORTED_OR_NON_EXPORTED, name)
                    } else if deviation_in_file & OverloadFlags::AMBIENT != 0 {
                        (&messages::OVERLOAD_SIGNATURES_MUST_ALL_BE_AMBIENT_OR_NON_AMBIENT, name)
                    } else if deviation & (OverloadFlags::PRIVATE | OverloadFlags::PROTECTED) != 0 {
                        (
                            &messages::OVERLOAD_SIGNATURES_MUST_ALL_BE_PUBLIC_PRIVATE_OR_PROTECTED,
                            Some(name.unwrap_or(overload)),
                        )
                    } else if deviation & OverloadFlags::ABSTRACT != 0 {
                        (&messages::OVERLOAD_SIGNATURES_MUST_ALL_BE_ABSTRACT_OR_NON_ABSTRACT, name)
                    } else {
                        continue;
                    };
                    // A nameless error node is a location-less diagnostic
                    // upstream, which this collection has no slot for.
                    let Some(at) = at else { continue };
                    let Some(file) = self.source_file_of_for_diagnostics(at) else { continue };
                    let span = self.nodes.span(at);
                    self.report(file, Diagnostic::new(message, span));
                }
            }
        }
        if some_optional != all_optional {
            let canonical_optional = self.declaration_is_optional(canonical_of(self, declarations));
            for &overload in declarations {
                if self.declaration_is_optional(overload) == canonical_optional {
                    continue;
                }
                let Some(at) = self.declaration_name_of(overload) else { continue };
                let Some(file) = self.source_file_of_for_diagnostics(at) else { continue };
                let span = self.nodes.span(at);
                self.report(
                    file,
                    Diagnostic::new(
                        &messages::OVERLOAD_SIGNATURES_MUST_ALL_BE_OPTIONAL_OR_REQUIRED,
                        span,
                    ),
                );
            }
        }
    }

    /// `getEffectiveDeclarationFlags(n, Export|Ambient|Private|Protected|Abstract)`
    /// (`checker.go:3701`): the declaration's own modifiers, and outside a
    /// class or interface body an ambient declaration is `Ambient`, and
    /// `Export` too in an ambient export context
    /// ([`Self::is_exported_by_ambient_export_context`]). A declaration of
    /// the file being checked reads the walk's ambient state; one of another
    /// file asks [`Self::is_ambient_declaration`].
    fn overload_effective_flags(&self, declaration: NodeId, current_file: NodeId) -> u8 {
        let modifiers = self.node_map.get(declaration).and_then(modifiers_of);
        let has = |kind| modifiers.is_some_and(|modifiers| has_modifier(modifiers, kind));
        let mut flags = 0;
        for (kind, flag) in [
            (SyntaxKind::ExportKeyword, OverloadFlags::EXPORT),
            (SyntaxKind::DeclareKeyword, OverloadFlags::AMBIENT),
            (SyntaxKind::PrivateKeyword, OverloadFlags::PRIVATE),
            (SyntaxKind::ProtectedKeyword, OverloadFlags::PROTECTED),
            (SyntaxKind::AbstractKeyword, OverloadFlags::ABSTRACT),
        ] {
            if has(kind) {
                flags |= flag;
            }
        }
        let in_class_like = self.nodes.parent(declaration).is_some_and(|parent| {
            matches!(
                self.nodes.kind(parent),
                SyntaxKind::InterfaceDeclaration
                    | SyntaxKind::ClassDeclaration
                    | SyntaxKind::ClassExpression
            )
        });
        if !in_class_like {
            let ambient = if self.source_file_of_for_diagnostics(declaration) == Some(current_file)
            {
                self.is_in_ambient_context_for_overloads(declaration)
            } else {
                self.is_ambient_declaration(declaration)
            };
            if ambient {
                if self.is_exported_by_ambient_export_context(declaration) {
                    flags |= OverloadFlags::EXPORT;
                }
                flags |= OverloadFlags::AMBIENT;
            }
        }
        flags
    }

    /// `node.Flags & ast.NodeFlagsAmbient` for an overload declaration — the
    /// `declare` modifier *or* an enclosing ambient container. §673.
    pub(crate) fn is_in_ambient_context_for_overloads(&self, node: NodeId) -> bool {
        if self.file_is_ambient {
            return true;
        }
        let mut at = Some(node);
        while let Some(current) = at {
            if let Some(modifiers) = self.node_map.get(current).and_then(modifiers_of)
                && tsr_ast::has_syntactic_modifier(modifiers, SyntaxKind::DeclareKeyword)
            {
                return true;
            }
            at = self.nodes.parent(current);
        }
        false
    }

    /// `checkFunctionOrConstructorSymbol` (`checker.go:3461`).
    ///
    /// | code | message |
    /// |---|---|
    /// | TS2391 | `Function implementation is missing or not immediately following the declaration.` |
    /// | TS2390 | `Constructor implementation is missing.` |
    /// | TS2392 | `Multiple constructor implementations are not allowed.` |
    /// | TS2393 | `Duplicate function implementation.` |
    /// | TS2389 | `Function implementation name must be '{0}'.` |
    /// | TS2387/TS2388 | `Function overload must (not) be static.` |
    /// | TS2383–TS2386, TS2512 | overload agreement, [`Self::check_overload_agreement`] |
    /// | TS2394 | [`Self::check_overloads_compatible_with_implementation`] |
    ///
    /// Upstream's worker (`checker.go:3469`) does five jobs: implementation
    /// presence, modifier agreement across overloads, question-token
    /// agreement, class/function merging (`crate::class_function_merge`) and
    /// the implementation-versus-overload relation check. The agreement arms
    /// run on every declaration; the bounds below apply to the rest.
    ///
    /// # Two bounds, both refusals rather than approximations
    ///
    /// - **Single-file symbols only.** A symbol whose declarations span files
    ///   needs each declaration's own ambient context, and this port's ambient
    ///   bit is per-file state supplied by the caller
    ///   ([`FileContext`]) rather than a flag on the node — so for a declaration
    ///   in another file it is simply not available. Upstream reads
    ///   `node.Flags&NodeFlagsAmbient` and has no such problem.
    /// - **Checked once per symbol**, upstream's
    ///   `links.functionOrConstructorChecked` (`checker.go:3463`). Without it a
    ///   three-overload function reports three times, and the suite compares
    ///   multisets.
    ///
    /// `checkFunctionOrMethodDeclaration` (`checker.go:3422-3436`) calls the
    /// worker only for a declaration with a **bindable name** (a dynamic name
    /// that is not late-bindable — `[e]` with `e` unresolved, `[sym]` with
    /// `sym: symbol` — is skipped), and twice for an exported one: on
    /// `node.LocalSymbol()`, the local half that holds only this container's
    /// declarations, and on the export symbol, which holds every merged
    /// block's. Both runs can report the same line; upstream's diagnostic
    /// collection deduplicates, so this does too.
    fn check_function_or_constructor_symbol(&mut self, node: NodeId, ambient: bool) {
        if self.non_bindable_computed_name(node).is_some() {
            return;
        }
        // A **constructor has no symbol in this binder**. Upstream binds one as
        // `__constructor` in the class's member table; here `symbol_of` answers
        // `None`, so an overload set of constructors had nothing to gather its
        // declarations from and this rule returned before it started — every
        // TS2390 §14 converted came in through a method or a function
        // (`checker-notes-diag2.md` §64). The declarations are the enclosing
        // class's constructor members in source order, and the dedup
        // `function_symbol_checked` gives a symbol is given here by running
        // only for the first of them.
        let mut lists: Vec<Vec<NodeId>> = Vec::new();
        if let Some(own) = self.binder.symbol_of(node) {
            let symbol = self.binder.merged_symbol(own);
            let local = self
                .declaration_name_of(node)
                .and_then(|name| self.identifier_text(name))
                .map(str::to_string)
                .and_then(|name| self.export_merge_local_symbol(node, own, &name))
                .filter(|&local| local != own && local != symbol);
            for checked in local.into_iter().chain(std::iter::once(symbol)) {
                if self.function_symbol_checked.insert(checked) {
                    lists.push(
                        self.binder.symbols().get(checked).declarations.iter().copied().collect(),
                    );
                }
            }
        } else {
            let siblings = self.constructor_siblings_of(node);
            if siblings.first() == Some(&node) {
                lists.push(siblings);
            }
        }
        let start = self.diagnostics.len();
        for declarations in lists {
            self.check_function_or_constructor_declarations(node, declarations, ambient);
        }
        let mut seen = rustc_hash::FxHashSet::default();
        let mut index = start;
        while index < self.diagnostics.len() {
            let (file, diagnostic) = &self.diagnostics[index];
            let key = (
                *file,
                diagnostic.span.start,
                diagnostic.span.end,
                diagnostic.message.code(),
                diagnostic.args.clone(),
            );
            if seen.insert(key) {
                index += 1;
            } else {
                self.diagnostics.remove(index);
            }
        }
    }

    /// `checkFunctionOrConstructorSymbolWorker` over one symbol's
    /// declarations; see [`Self::check_function_or_constructor_symbol`].
    fn check_function_or_constructor_declarations(
        &mut self,
        node: NodeId,
        mut declarations: Vec<NodeId>,
        ambient: bool,
    ) {
        let Some(mut file) = self.source_file_of_for_diagnostics(node) else { return };
        self.check_overload_agreement(&declarations, file);
        let mut ambient = ambient;
        // A module augmentation (`mergeModuleAugmentation`,
        // `docs/parity/notes/names-modules.md` §4) adds ambient declarations
        // from another file. In the loop below an ambient declaration only
        // resets the adjacency chain and is never the implementation or the
        // last non-ambient one, so when the declarations span files the
        // ambient ones are dropped and the rest are checked in their own file.
        if declarations
            .iter()
            .any(|&declaration| self.source_file_of_for_diagnostics(declaration) != Some(file))
        {
            let remaining: Vec<NodeId> = declarations
                .iter()
                .copied()
                .filter(|&declaration| !self.is_ambient_declaration(declaration))
                .collect();
            if let Some(&first) = remaining.first()
                && remaining.len() < declarations.len()
                && let Some(remaining_file) = self.source_file_of_for_diagnostics(first)
            {
                if remaining_file != file {
                    ambient = self
                        .module_host
                        .is_some_and(|host| host.is_declaration_file(remaining_file));
                    file = remaining_file;
                }
                declarations = remaining;
            }
        }
        // The single-file bound: anything else and the per-declaration ambient
        // context is unavailable, so nothing is said.
        if declarations
            .iter()
            .any(|&declaration| self.source_file_of_for_diagnostics(declaration) != Some(file))
        {
            return;
        }

        // `hasNonAmbientClass`'s TS2813/TS2814 arm (`checker.go:3660`) is
        // `crate::class_function_merge`, run from the class and function
        // declarations themselves so a symbol merged across files is covered.
        if declarations
            .iter()
            .any(|&declaration| self.nodes.kind(declaration) == SyntaxKind::ClassExpression)
        {
            return;
        }
        // **All declarations must share one parent.** Upstream's overloads are
        // siblings; a symbol whose declarations sit in *different containers* is
        // one this binder merged and upstream did not — `class Point { static
        // Origin() {} }` beside `namespace Point { export function Origin() {} }`
        // is two symbols upstream, which reports TS2300 `Duplicate identifier`
        // from the binder and never reaches this function. Declining is
        // silence where upstream says something else, and it was 18 of the 21
        // wrong lines left after the class-merge decline above.
        let parents_agree = {
            let mut parents = declarations.iter().filter_map(|&d| self.nodes.parent(d));
            let first = parents.next();
            parents.all(|parent| Some(parent) == first)
        };
        if !parents_agree && !self.declarations_are_merged_namespace_exports(&declarations) {
            return;
        }
        let is_constructor = self.nodes.kind(node) == SyntaxKind::Constructor;
        let mut previous: Option<NodeId> = None;
        let mut body_declaration: Option<NodeId> = None;
        let mut last_non_ambient: Option<NodeId> = None;
        let mut function_declarations: Vec<NodeId> = Vec::new();
        let mut multiple_constructor_implementations = false;
        let mut duplicate_function_implementation = false;

        for &declaration in &declarations {
            // `inAmbientContextOrInterface` (`checker.go:3606`): ambient and
            // interface declarations may be interleaved, so they reset the
            // adjacency chain rather than breaking it.
            let parent_kind = self.nodes.parent(declaration).map(|p| self.nodes.kind(p));
            let in_ambient_or_interface = ambient
                || self.declaration_is_ambient(declaration)
                || matches!(
                    parent_kind,
                    Some(SyntaxKind::InterfaceDeclaration | SyntaxKind::TypeLiteral)
                );
            if in_ambient_or_interface {
                previous = None;
            }
            if !self.is_function_or_method_or_constructor(declaration) {
                continue;
            }
            function_declarations.push(declaration);
            let body_present = self.declaration_has_body(declaration);
            if body_present && body_declaration.is_some() {
                if is_constructor {
                    multiple_constructor_implementations = true;
                } else {
                    duplicate_function_implementation = true;
                }
            } else if let Some(earlier) = previous
                && self.nodes.parent(earlier) == self.nodes.parent(declaration)
                && self.next_sibling(earlier) != Some(declaration)
            {
                self.report_implementation_expected(earlier, is_constructor);
            }
            if body_present && body_declaration.is_none() {
                body_declaration = Some(declaration);
            }
            previous = Some(declaration);
            if !in_ambient_or_interface {
                last_non_ambient = Some(declaration);
            }
        }

        if multiple_constructor_implementations {
            for &declaration in &function_declarations {
                let span = self.error_span(declaration);
                self.report(
                    file,
                    Diagnostic::new(
                        &messages::MULTIPLE_CONSTRUCTOR_IMPLEMENTATIONS_ARE_NOT_ALLOWED,
                        span,
                    ),
                );
            }
        }
        if duplicate_function_implementation {
            for &declaration in &function_declarations {
                let at = self.declaration_name_of(declaration).unwrap_or(declaration);
                let span = self.error_span(at);
                self.report(
                    file,
                    Diagnostic::new(&messages::DUPLICATE_FUNCTION_IMPLEMENTATION, span),
                );
            }
        }
        // "Abstract methods can't have an implementation -- in particular, they
        // don't need one." (`checker.go:3679`)
        if let Some(last) = last_non_ambient
            && !self.declaration_has_body(last)
            && !self.declaration_is_abstract(last)
            && !self.declaration_is_optional(last)
        {
            self.report_implementation_expected(last, is_constructor);
        }
        // `if hasOverloads { … if bodyDeclaration != nil { … } }`
        // (`checker.go:3694`): the implementation-versus-overload relation.
        if let Some(body) = body_declaration {
            let overloads: Vec<NodeId> = function_declarations
                .iter()
                .copied()
                .filter(|&declaration| !self.declaration_has_body(declaration))
                .collect();
            if !overloads.is_empty() {
                self.check_overloads_compatible_with_implementation(file, body, &overloads);
            }
        }
    }

    /// Whether every declaration is an `export`ed member of a block of one
    /// (merged) namespace — the one cross-container shape upstream's binder
    /// also merges: `declareModuleMember` puts exported members into the
    /// namespace symbol's `exports`, shared by all its blocks. Locals of two
    /// blocks, or a class body beside a namespace block, stay apart upstream.
    /// `docs/parity/notes/decls.md` §7.
    fn declarations_are_merged_namespace_exports(&self, declarations: &[NodeId]) -> bool {
        let mut namespace = None;
        for &declaration in declarations {
            let exported = self
                .node_map
                .get(declaration)
                .and_then(modifiers_of)
                .is_some_and(|modifiers| has_modifier(modifiers, SyntaxKind::ExportKeyword));
            let Some(block) = self.nodes.parent(declaration) else { return false };
            if !exported || self.nodes.kind(block) != SyntaxKind::ModuleBlock {
                return false;
            }
            let Some(module) = self.nodes.parent(block) else { return false };
            let Some(symbol) = self.binder.symbol_of(module) else { return false };
            let symbol = self.binder.merged_symbol(symbol);
            if namespace.is_some_and(|namespace| namespace != symbol) {
                return false;
            }
            namespace = Some(symbol);
        }
        namespace.is_some()
    }

    /// TS2394 — `This overload signature is not compatible with its
    /// implementation signature.`
    ///
    /// The last block of `checkFunctionOrConstructorSymbol`
    /// (`checker.go:3697-3707`): each overload signature, in declaration
    /// order, against `getSignatureFromDeclaration(bodyDeclaration)`; the
    /// **first** incompatible one is reported on its declaration and the loop
    /// stops. Upstream iterates `getSignaturesOfSymbol(symbol)`, which also
    /// holds a non-adjacent implementation; that one is compatible with
    /// itself, so taking the body-less declarations is the same walk.
    ///
    /// A signature this port cannot build, or a relation it cannot decide,
    /// skips that overload rather than stopping: only a decided `NotRelated`
    /// is upstream's `!isImplementationCompatibleWithOverload`.
    /// `docs/parity/notes/decls.md` §4.
    fn check_overloads_compatible_with_implementation(
        &mut self,
        file: NodeId,
        body: NodeId,
        overloads: &[NodeId],
    ) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(implementation) = self.get_signature_from_declaration(body) else { return };
        let Some(implementation) = self.complete_signature_return(implementation) else { return };
        for &declaration in overloads {
            let Some(overload) = self.get_signature_from_declaration(declaration) else {
                continue;
            };
            let Some(overload) = self.complete_signature_return(overload) else { continue };
            if self.implementation_compatible_with_overload(&implementation, &overload)
                != Ternary::NotRelated
            {
                continue;
            }
            let span = self.error_span(declaration);
            self.report(
                file,
                Diagnostic::new(
                    &messages::THIS_OVERLOAD_SIGNATURE_IS_NOT_COMPATIBLE_WITH_ITS_IMPLEMENTATION_SIGNATURE,
                    span,
                ),
            );
            break;
        }
    }

    /// `isImplementationCompatibleWithOverload` (`checker.go:3716`): erase both
    /// signatures; the return types must be assignable in either direction (or
    /// the overload's be `void`), and then the implementation must be
    /// assignable to the overload ignoring return types
    /// (`isSignatureAssignableTo(…, ignoreReturnTypes=true)`).
    ///
    /// The parameter comparison is [`Self::signature_parameters_assignable`].
    fn implementation_compatible_with_overload(
        &mut self,
        implementation: &crate::signatures::Signature,
        overload: &crate::signatures::Signature,
    ) -> Ternary {
        let Some(source) = self.erased_signature(implementation) else { return Ternary::Unknown };
        let Some(target) = self.erased_signature(overload) else { return Ternary::Unknown };
        let (Some(source_return), Some(target_return)) = (
            self.get_return_type_of_signature(&source),
            self.get_return_type_of_signature(&target),
        ) else {
            return Ternary::Unknown;
        };
        if target_return != self.intrinsics.void {
            let forward = self.relate_ternary(target_return, source_return, Relation::Assignable);
            if forward != Ternary::Related {
                let backward =
                    self.relate_ternary(source_return, target_return, Relation::Assignable);
                match (forward, backward) {
                    (_, Ternary::Related) => {}
                    (Ternary::NotRelated, Ternary::NotRelated) => {
                        if !self.assignability_pair_is_reportable(source_return, target_return) {
                            return Ternary::Unknown;
                        }
                        return Ternary::NotRelated;
                    }
                    _ => return Ternary::Unknown,
                }
            }
        }
        self.signature_parameters_assignable(&source, &target)
    }

    /// `compareSignaturesRelated(source, target, SignatureCheckModeIgnoreReturnTypes,
    /// …, compareTypesAssignable)` (`relater.go:1979`) for two erased,
    /// non-generic signatures: the arity test, the `this` types and the
    /// parameter loop.
    ///
    /// **Why not the relater's own signature comparison.** It is reachable only
    /// through a pair of signature-bearing *types*, and the relater admits a
    /// pure-signature type only for function-expression, function-type, method
    /// and signature declarations — not the function declarations and
    /// constructors an overload set is made of. Minting a type therefore
    /// reaches row 3 ("no members table") and answers `Unknown`. The loop below
    /// is the part `isImplementationCompatibleWithOverload` needs; it should
    /// give way to the relater's once that is reachable (notes §4).
    ///
    /// `strictVariance` is upstream's: `strictFunctionTypes` and a target
    /// declaration that is not a method or constructor. A parameter whose type
    /// is callable would enter upstream's callback arm
    /// (`compareSignaturesRelated` on the two callbacks), which this does not
    /// port, so such a pair is decided only when both directions agree.
    fn signature_parameters_assignable(
        &mut self,
        source: &crate::signatures::Signature,
        target: &crate::signatures::Signature,
    ) -> Ternary {
        let target_count = self.signature_parameter_count(target);
        if !self.signature_has_effective_rest(target)
            && self.signature_min_argument_count(source) > target_count
        {
            return Ternary::NotRelated;
        }
        if self.signature_non_array_rest_type(source).is_some()
            || self.signature_non_array_rest_type(target).is_some()
        {
            return Ternary::Unknown;
        }
        let strict_variance = self.strict_function_types
            && !matches!(
                self.nodes.kind(target.declaration),
                SyntaxKind::MethodDeclaration
                    | SyntaxKind::MethodSignature
                    | SyntaxKind::Constructor
            );
        let mut pairs = Vec::new();
        if let (Some(source_this), Some(target_this)) =
            (&source.this_parameter, &target.this_parameter)
            && self.parameter_type(source_this) != self.intrinsics.void
        {
            pairs.push((self.parameter_type(source_this), self.parameter_type(target_this)));
        }
        let count = self.signature_parameter_count(source).max(target_count);
        for position in 0..count {
            let (Some(s), Some(t)) = (
                self.signature_type_at_position(source, position),
                self.signature_type_at_position(target, position),
            ) else {
                continue;
            };
            if s != t {
                pairs.push((s, t));
            }
        }
        let mut result = Ternary::Related;
        for (s, t) in pairs {
            if self.is_error(s) || self.is_error(t) {
                return Ternary::Unknown;
            }
            let callable = [s, t].into_iter().any(|side| {
                let side = self.get_non_nullable_type(side);
                self.single_call_signature(side).is_some()
            });
            let backward = self.relate_ternary(t, s, Relation::Assignable);
            let related = if strict_variance && !callable {
                backward
            } else {
                let forward = self.relate_ternary(s, t, Relation::Assignable);
                match (forward, backward) {
                    (Ternary::NotRelated, Ternary::NotRelated) => Ternary::NotRelated,
                    (Ternary::Related, Ternary::Related) => Ternary::Related,
                    // Bivariance: one direction suffices — except across the
                    // unported callback arm, which only a unanimous answer
                    // decides.
                    (Ternary::Related, _) | (_, Ternary::Related) if !callable => Ternary::Related,
                    _ => Ternary::Unknown,
                }
            };
            match related {
                Ternary::NotRelated => return Ternary::NotRelated,
                Ternary::Unknown => result = Ternary::Unknown,
                Ternary::Related => {}
            }
        }
        result
    }

    /// `getErasedSignature`: the signature's own type parameters replaced by
    /// `any`.
    fn erased_signature(
        &mut self,
        signature: &crate::signatures::Signature,
    ) -> Option<crate::signatures::Signature> {
        if signature.type_parameters.is_empty() {
            return Some(signature.clone());
        }
        let parameters = self.type_parameter_types(signature)?;
        let any = self.intrinsics.any;
        let map: Vec<(TypeId, TypeId)> =
            parameters.iter().map(|&parameter| (parameter, any)).collect();
        let names: Vec<String> =
            signature.type_parameters.iter().map(|parameter| parameter.name.clone()).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut erased =
            self.instantiate_signature(signature.clone(), &map, &parameters, &names)?;
        erased.type_parameters.clear();
        Some(erased)
    }

    /// `reportImplementationExpectedError` (`checker.go:3549`).
    ///
    /// The subsequent-node scan: when the declaration's next sibling (upstream
    /// tests `subsequentNode.Pos() == node.End()`; see [`Self::next_sibling`])
    /// is of the same kind, either the names match — then a method whose
    /// `static`-ness differs from the next one's is TS2387/TS2388 at the next
    /// one's name, and otherwise nothing is said, since the binder already
    /// reported whatever kept them from merging — or the next one carries a
    /// body and is the misnamed implementation, TS2389. Only past both does
    /// the terminal message (TS2390/TS2391/TS2516) fall.
    fn report_implementation_expected(&mut self, node: NodeId, is_constructor: bool) {
        let name = self.declaration_name_of(node);
        // `name != nil && ast.NodeIsMissing(name)`: a name the parser had to
        // invent (an empty span here) says nothing.
        if name.is_some_and(|name| {
            let span = self.nodes.span(name);
            span.start == span.end
        }) {
            return;
        }
        if let Some(next) = self.next_sibling(node)
            && self.nodes.kind(next) == self.nodes.kind(node)
        {
            let subsequent = self.declaration_name_of(next);
            let at = subsequent.unwrap_or(next);
            if let (Some(name), Some(subsequent)) = (name, subsequent) {
                match self.overload_names_identical(name, subsequent) {
                    Some(true) => {
                        let is_static = |checker: &Self, declaration: NodeId| {
                            checker.node_map.get(declaration).and_then(modifiers_of).is_some_and(
                                |modifiers| has_modifier(modifiers, SyntaxKind::StaticKeyword),
                            )
                        };
                        let node_is_static = is_static(self, node);
                        if matches!(
                            self.nodes.kind(node),
                            SyntaxKind::MethodDeclaration | SyntaxKind::MethodSignature
                        ) && node_is_static != is_static(self, next)
                            && let Some(file) = self.source_file_of_for_diagnostics(at)
                        {
                            let message = if node_is_static {
                                &messages::FUNCTION_OVERLOAD_MUST_BE_STATIC
                            } else {
                                &messages::FUNCTION_OVERLOAD_MUST_NOT_BE_STATIC
                            };
                            let span = self.error_span(at);
                            self.report(file, Diagnostic::new(message, span));
                        }
                        return;
                    }
                    Some(false) => {}
                    // A computed pair the identity relation cannot decide.
                    None => return,
                }
            }
            if self.declaration_has_body(next) {
                // `scanner.DeclarationNameToString(name)`; a declaration
                // without a name (a constructor) has nothing to print.
                let Some(expected) = name.and_then(|name| self.overload_name_to_string(name))
                else {
                    return;
                };
                let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
                let span = self.error_span(at);
                self.report(
                    file,
                    Diagnostic::with_args(
                        &messages::FUNCTION_IMPLEMENTATION_NAME_MUST_BE_0,
                        span,
                        [expected],
                    ),
                );
                return;
            }
        }
        let at = name.unwrap_or(node);
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        let message = if is_constructor {
            &messages::CONSTRUCTOR_IMPLEMENTATION_IS_MISSING
        } else if self.declaration_is_abstract(node) {
            &messages::ALL_DECLARATIONS_OF_AN_ABSTRACT_METHOD_MUST_BE_CONSECUTIVE
        } else {
            &messages::FUNCTION_IMPLEMENTATION_IS_MISSING_OR_NOT_IMMEDIATELY_FOLLOWING_THE_DECLARATION
        };
        self.report(file, Diagnostic::new(message, span));
    }

    /// The name test of `reportImplementationExpectedError`
    /// (`checker.go:3570-3573`): two private names with one text, two
    /// computed names whose `checkComputedPropertyName` types are identical,
    /// or two property-name literals (identifier, string, numeric,
    /// no-substitution template) with one `Text()` — a numeric literal's text
    /// is its canonical value, as the scanner stores it. `None` when the
    /// identity of two computed-name types is undecidable here.
    fn overload_names_identical(&mut self, name: NodeId, subsequent: NodeId) -> Option<bool> {
        let literal_text = |checker: &Self, node: NodeId| -> Option<String> {
            match checker.node_map.get(node)? {
                Node::Identifier(identifier) => Some(identifier.text.to_string()),
                Node::StringLiteral(literal) => Some(literal.text.to_string()),
                Node::NoSubstitutionTemplateLiteral(literal) => Some(literal.text.to_string()),
                Node::NumericLiteral(literal) => {
                    Some(tsr_core::jsnum::canonical_numeric_text(literal.text))
                }
                _ => None,
            }
        };
        match (self.node_map.get(name), self.node_map.get(subsequent)) {
            (Some(Node::PrivateIdentifier(left)), Some(Node::PrivateIdentifier(right))) => {
                Some(left.text == right.text)
            }
            (Some(Node::ComputedPropertyName(left)), Some(Node::ComputedPropertyName(right))) => {
                let (Some(left), Some(right)) = (left.expression, right.expression) else {
                    return Some(false);
                };
                let left = self.check_expression(left);
                let right = self.check_expression(right);
                let left = self.get_regular_type_of_literal_type(left);
                let right = self.get_regular_type_of_literal_type(right);
                if left == right {
                    return Some(true);
                }
                // Literal and unique-symbol types are interned: two distinct
                // ids are two distinct types.
                let unit = TypeFlags::STRING_LITERAL
                    | TypeFlags::NUMBER_LITERAL
                    | TypeFlags::BIG_INT_LITERAL
                    | TypeFlags::BOOLEAN_LITERAL
                    | TypeFlags::UNIQUE_ES_SYMBOL;
                if self.type_of(left).flags.intersects(unit)
                    && self.type_of(right).flags.intersects(unit)
                {
                    return Some(false);
                }
                match self.is_type_identical_to(left, right) {
                    Ternary::Related => Some(true),
                    Ternary::NotRelated => Some(false),
                    Ternary::Unknown => None,
                }
            }
            _ => match (literal_text(self, name), literal_text(self, subsequent)) {
                (Some(left), Some(right)) => Some(left == right),
                _ => Some(false),
            },
        }
    }

    /// `scanner.DeclarationNameToString` for an overload's name. Upstream
    /// prints the source text; a string literal is respelled with double
    /// quotes and a computed name from its expression, which only the
    /// message argument can tell apart.
    fn overload_name_to_string(&self, name: NodeId) -> Option<String> {
        match self.node_map.get(name)? {
            Node::Identifier(identifier) => Some(identifier.text.to_string()),
            Node::PrivateIdentifier(identifier) => Some(identifier.text.to_string()),
            Node::NumericLiteral(literal) => Some(literal.text.to_string()),
            Node::StringLiteral(literal) => Some(format!("\"{}\"", literal.text)),
            Node::ComputedPropertyName(computed) => {
                let inner = match computed.expression? {
                    tsr_ast::Expression::StringLiteral(literal) => format!("\"{}\"", literal.text),
                    tsr_ast::Expression::NumericLiteral(literal) => literal.text.to_string(),
                    _ => self.computed_name_spelling(name)?,
                };
                Some(format!("[{inner}]"))
            }
            _ => None,
        }
    }

    /// Every `constructor` member of the class enclosing `node`, in source
    /// order — the declaration list a constructor overload set would have had
    /// if this binder gave it a symbol. Empty for anything that is not a
    /// constructor. See `checker-notes-diag2.md` §64.
    fn constructor_siblings_of(&self, node: NodeId) -> Vec<NodeId> {
        if self.nodes.kind(node) != SyntaxKind::Constructor {
            return Vec::new();
        }
        let members: &[ClassElement<'_>] =
            match self.nodes.parent(node).and_then(|class| self.node_map.get(class)) {
                Some(Node::ClassDeclaration(class)) => class.members,
                Some(Node::ClassExpression(class)) => class.members,
                _ => return Vec::new(),
            };
        members
            .iter()
            .filter_map(|member| match member {
                ClassElement::ConstructorDeclaration(constructor) => constructor.node_id,
                _ => None,
            })
            .collect()
    }

    /// The text of an identifier or private identifier used as a declaration
    /// name, for the name comparison above.
    pub(crate) fn identifier_text(&self, node: NodeId) -> Option<&str> {
        match self.node_map.get(node)? {
            Node::Identifier(identifier) => Some(identifier.text),
            Node::PrivateIdentifier(identifier) => Some(identifier.text),
            Node::StringLiteral(literal) => Some(literal.text),
            Node::NumericLiteral(literal) => Some(literal.text),
            _ => None,
        }
    }

    /// The node immediately after `node` in its parent's child order.
    ///
    /// **This stands in for upstream's `previousDeclaration.End() == node.Pos()`
    /// and it is not a cosmetic substitution.** `Pos()` upstream is the *full*
    /// start — the end of the preceding token, trivia included — so two
    /// declarations separated by a newline still satisfy it. `tsr_core::Span`
    /// records the token start *after* trivia, so the same expression is false
    /// for every pair of declarations on separate lines, and writing it that way
    /// reported TS2391 on **480 lines** of perfectly ordinary overload sets
    /// (`overloadAssignmentCompat`, `functionOverloadErrors`, and every other
    /// overload case in the corpus).
    ///
    /// Sibling adjacency is what upstream's expression *means*: nothing else was
    /// parsed between them. It is exact where the span test was an accident of a
    /// different position model.
    fn next_sibling(&self, node: NodeId) -> Option<NodeId> {
        let parent = self.nodes.parent(node)?;
        let typed = self.node_map.get(parent)?;
        let mut seen = false;
        let mut next = None;
        tsr_ast::for_each_child_id(typed, |child| {
            if next.is_none() {
                if seen {
                    next = Some(child);
                } else if child == node {
                    seen = true;
                }
            }
        });
        next
    }

    fn is_function_or_method_or_constructor(&self, node: NodeId) -> bool {
        matches!(
            self.nodes.kind(node),
            SyntaxKind::FunctionDeclaration
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::MethodSignature
                | SyntaxKind::Constructor
        )
    }

    /// `ast.NodeIsPresent(node.Body())`.
    fn declaration_has_body(&self, node: NodeId) -> bool {
        match self.node_map.get(node) {
            Some(Node::FunctionDeclaration(declaration)) => declaration.body.is_some(),
            Some(Node::MethodDeclaration(declaration)) => declaration.body.is_some(),
            Some(Node::ConstructorDeclaration(declaration)) => declaration.body.is_some(),
            _ => false,
        }
    }

    fn declaration_is_abstract(&self, node: NodeId) -> bool {
        match self.node_map.get(node) {
            Some(Node::FunctionDeclaration(declaration)) => {
                has_modifier(declaration.modifiers, SyntaxKind::AbstractKeyword)
            }
            Some(Node::MethodDeclaration(declaration)) => {
                has_modifier(declaration.modifiers, SyntaxKind::AbstractKeyword)
            }
            _ => false,
        }
    }

    /// `ast.IsOptionalDeclaration` for the two kinds this rule sees.
    fn declaration_is_optional(&self, node: NodeId) -> bool {
        match self.node_map.get(node) {
            Some(Node::MethodDeclaration(declaration)) => declaration
                .postfix_token
                .is_some_and(|token| token.kind == SyntaxKind::QuestionToken),
            Some(Node::MethodSignatureDeclaration(signature)) => {
                signature.postfix_token.is_some_and(|token| token.kind == SyntaxKind::QuestionToken)
            }
            _ => false,
        }
    }

    /// `mergeModuleAugmentation` (`checker.go:1397`) — TS2664 `Invalid module
    /// name in augmentation, module '{0}' cannot be found.`
    ///
    /// A `declare module "x"` in a file that is itself a module is an
    /// **augmentation**, and upstream passes a not-found message to the
    /// resolution only when the declaration is not in an ambient context: *"do
    /// not validate names of augmentations that are defined in ambient
    /// context."* The same declaration in a `.d.ts` is a module declaration and
    /// gets no error.
    ///
    /// `docs/architecture/checker-notes-diag2.md` §677.
    fn check_module_augmentation_name(&mut self, node: NodeId, name: NodeId) {
        let Some(Node::StringLiteral(literal)) = self.node_map.get(name) else { return };
        if self.file_is_ambient {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        // The augmentation test: the *containing file* must be a module. A
        // `declare module "x"` in a plain script is an ambient external module
        // declaration and declares the module rather than augmenting one.
        //
        // And the declaration must be the file's own statement:
        // `IsModuleAugmentationExternal`'s other arm, an augmentation nested
        // in a script's ambient module, sits in an ambient module block, which
        // `mergeModuleAugmentation`'s `moduleName.Parent.Parent` ambient test
        // exempts; one nested in a namespace is no augmentation at all (TS2435,
        // [`Checker::check_nested_ambient_module`]). `misc-checks.md` §20.
        let is_augmentation = self.nodes.parent(node) == Some(file)
            && matches!(
                self.node_map.get(file),
                Some(Node::SourceFile(source)) if tsr_binder::is_external_module(source)
            );
        if !is_augmentation {
            return;
        }
        let text = literal.text.to_string();
        if self.resolve_external_module_name(name, name).is_some() {
            return;
        }
        let span = self.error_span(name);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::INVALID_MODULE_NAME_IN_AUGMENTATION_MODULE_0_CANNOT_BE_FOUND,
                span,
                [text],
            ),
        );
    }

    /// TS2668 — `'export' modifier cannot be applied to ambient modules and
    /// module augmentations since they are always visible.`
    ///
    /// `bindModuleDeclaration` (`binder.go:773`): an ambient module
    /// (`ast.IsAmbientModule`: a string-literal name or `declare global`)
    /// carrying a syntactic `export`, reported with `errorOnFirstToken` —
    /// the node's first modifier, which need not be the `export`
    /// (`declare export module "m"` reports at `declare`). A binder
    /// diagnostic upstream; reported from the check walk here because the
    /// binder's per-file diagnostics carry no first-token helper, and the rule
    /// reads only syntax. `misc-checks.md` §20.
    fn check_ambient_module_export_modifier(&mut self, node: NodeId) {
        let Some(Node::ModuleDeclaration(module)) = self.node_map.get(node) else { return };
        if !self.is_ambient_module_node(node)
            || !has_modifier(module.modifiers, SyntaxKind::ExportKeyword)
        {
            return;
        }
        let span = match module.modifiers.first() {
            Some(tsr_ast::ModifierLike::Token(token)) => {
                token.node_id.map(|id| self.nodes.span(id))
            }
            _ => None,
        };
        let Some(span) = span else { return };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        self.report(
            file,
            Diagnostic::new(
                &messages::EXPORT_MODIFIER_CANNOT_BE_APPLIED_TO_AMBIENT_MODULES_AND_MODULE_AUGMENTATIONS_SINCE_THEY_ARE_ALWAYS_VISIBLE,
                span,
            ),
        );
    }

    /// TS2435 — `Ambient modules cannot be nested in other modules or
    /// namespaces.`
    ///
    /// `checkModuleDeclaration`'s `isAmbientExternalModule` tail
    /// (`checker.go:5188-5216`), its last arm: a string-named module that is
    /// not an external augmentation (`ast.IsModuleAugmentationExternal`) and
    /// whose parent is not a script (`ast.IsGlobalSourceFile`). A parent that
    /// is a source file is always one or the other, so the arm is reached
    /// exactly when the declaration sits in a module block that is not a
    /// script's top-level ambient module. The `declare global` spelling of the
    /// same arm is TS2669 ([`Checker::check_global_augmentation_position`]).
    ///
    /// Upstream returns before the tail when `checkGrammarModuleElementContext`
    /// fails (a module declaration outside a file or module block), so such a
    /// declaration is skipped. `misc-checks.md` §20.
    fn check_nested_ambient_module(&mut self, node: NodeId) {
        let Some(Node::ModuleDeclaration(module)) = self.node_map.get(node) else { return };
        let Some(tsr_ast::ModuleName::StringLiteral(name)) = module.name else { return };
        let Some(name) = name.node_id else { return };
        let Some(block) = self.nodes.parent(node) else { return };
        if self.nodes.kind(block) != SyntaxKind::ModuleBlock {
            return;
        }
        // `IsModuleAugmentationExternal`'s module-block arm: the block's
        // owner is an ambient module directly in a script.
        let owner = self.nodes.parent(block);
        let augmentation = owner.is_some_and(|owner| {
            self.is_ambient_module_node(owner)
                && matches!(
                    self.nodes.parent(owner).and_then(|file| self.node_map.get(file)),
                    Some(Node::SourceFile(source)) if !tsr_binder::is_external_module(source)
                )
        });
        if augmentation {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(name) else { return };
        let span = self.error_span(name);
        self.report(
            file,
            Diagnostic::new(
                &messages::AMBIENT_MODULES_CANNOT_BE_NESTED_IN_OTHER_MODULES_OR_NAMESPACES,
                span,
            ),
        );
    }

    /// TS1147 — `Import declarations in a namespace cannot reference a
    /// module.`
    ///
    /// `checkExternalImportOrExportDeclaration` (`checker.go:5332`), the
    /// position arm, for `import … from "m"` and `import x = require("m")`:
    /// a declaration whose parent is neither a source file nor an ambient
    /// module's block reports at the module name. A missing or non-string
    /// module name returns earlier upstream (a parse error, or TS1141), and
    /// `checkGrammarModuleElementContext` returns before this for a
    /// declaration outside a file or module block. The export spelling is
    /// TS1194 ([`Checker::check_export_declaration_in_namespace`]).
    /// [`Checker::external_import_is_positioned_for_resolution`] is the same
    /// test, which already withholds resolution. `misc-checks.md` §20.
    fn check_import_in_namespace(&mut self, node: NodeId, module_name: Option<NodeId>) {
        let Some(module_name) = module_name else { return };
        let span = self.nodes.span(module_name);
        if span.start == span.end
            || !matches!(self.node_map.get(module_name), Some(Node::StringLiteral(_)))
        {
            return;
        }
        let Some(parent) = self.nodes.parent(node) else { return };
        if !matches!(
            self.nodes.kind(parent),
            SyntaxKind::ModuleBlock | SyntaxKind::ModuleDeclaration
        ) {
            return;
        }
        if self.external_import_is_positioned_for_resolution(node) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(module_name) else { return };
        let span = self.error_span(module_name);
        self.report(
            file,
            Diagnostic::new(
                &messages::IMPORT_DECLARATIONS_IN_A_NAMESPACE_CANNOT_REFERENCE_A_MODULE,
                span,
            ),
        );
    }

    /// `getSymbol` (`checker.go:1023`): does this symbol satisfy `meaning`,
    /// following an alias chain when it does not carry it directly?
    ///
    /// ```go
    /// if symbol.Flags&meaning != 0 { return symbol }
    /// if symbol.Flags&ast.SymbolFlagsAlias != 0 {
    ///     target := c.resolveAlias(symbol)
    ///     if target.Flags&meaning != 0 { return symbol }
    /// }
    /// ```
    ///
    /// Three things this port must add, each measured against the wrong column
    /// (§676 / §690 / §691 / §692 took one caller from 28 wrong lines to 0):
    ///
    /// - [`Checker::resolve_alias`] declines a **qualified** module reference
    ///   for the printer's sake (§686), so `import I = ns.IMode` needs
    ///   [`Checker::qualified_alias_target`].
    /// - **A target that cannot be resolved is silence, not a negative.**
    ///   `import { Unresolved } from "foo"` with an unresolved `"foo"` has no
    ///   target to ask, and upstream answers the module error and an error
    ///   type. §25's collapse.
    /// - **The meaning is tested at every hop**, not only the first: a hop may
    ///   merge an alias with a real declaration of that meaning, and walking
    ///   past it reaches the module object.
    ///
    /// One helper rather than one per caller: §675 split this test across two
    /// branches of one upstream function and the halves drifted for fourteen
    /// builds. §693.
    fn alias_chain_carries(&mut self, symbol: tsr_binder::SymbolId, meaning: SymbolFlags) -> bool {
        let mut at = Some(symbol);
        for _ in 0..MAX_ALIAS_HOPS {
            let Some(current) = at else { break };
            let flags = self.binder.symbols().get(current).flags;
            if flags.intersects(meaning) {
                return true;
            }
            if !flags.intersects(SymbolFlags::ALIAS) {
                return false;
            }
            at = self.resolve_alias(current).or_else(|| self.qualified_alias_target(current));
        }
        // An unresolvable or over-long chain is unknown, and this rule reports
        // on a negative — so unknown is silence.
        at.is_none()
    }

    /// TS2437 — `Module '{0}' is hidden by a local declaration with the same
    /// name.`
    ///
    /// `checkImportEqualsDeclaration`'s internal-reference arm
    /// (`checker.go:5472`). **The alias resolves and the name does not**:
    /// `resolveAlias` reaches the namespace through the declaration, while an
    /// ordinary lookup from the same position finds a local of the same name
    /// first, and that disagreement is the error.
    ///
    /// `docs/architecture/checker-notes-diag2.md` §743.
    fn check_module_hidden_by_local(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(Node::ImportEqualsDeclaration(declaration)) = self.node_map.get(node) else {
            return;
        };
        // `!ast.IsExternalModuleReference(moduleReference)` — `import x =
        // require("m")` takes the other branch.
        let first = match declaration.module_reference {
            Some(ModuleReference::Identifier(name)) => name.node_id,
            Some(ModuleReference::QualifiedName(qualified)) => {
                let mut left = qualified.left;
                loop {
                    match left {
                        Some(tsr_ast::EntityName::Identifier(name)) => break name.node_id,
                        Some(tsr_ast::EntityName::QualifiedName(inner)) => left = inner.left,
                        None => break None,
                    }
                }
            }
            _ => return,
        };
        let Some(first) = first else { return };
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        let Some(target) =
            self.resolve_alias(symbol).or_else(|| self.qualified_alias_target(symbol))
        else {
            return;
        };
        if !self.binder.symbols().get(target).flags.intersects(SymbolFlags::VALUE) {
            return;
        }
        let Some(text) = self.identifier_text(first).map(str::to_string) else { return };
        let hidden = self
            .binder
            .resolve_name(
                self.nodes,
                self.node_map,
                first,
                &text,
                SymbolFlags::VALUE | SymbolFlags::NAMESPACE,
            )
            .is_some_and(|found| {
                !self
                    .binder
                    .symbols()
                    .get(self.binder.merged_symbol(found))
                    .flags
                    .intersects(SymbolFlags::NAMESPACE)
            });
        if !hidden {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(first) else { return };
        let span = self.nodes.span(first);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::MODULE_0_IS_HIDDEN_BY_A_LOCAL_DECLARATION_WITH_THE_SAME_NAME,
                span,
                [text],
            ),
        );
    }

    /// TS2506 — `'{0}' is referenced directly or indirectly in its own base
    /// expression.`
    ///
    /// `resolveBaseTypesOfClass`'s cycle guard (`checker.go:16977`). Upstream
    /// detects it while resolving the base type, on a stack; this port has no
    /// such stack, so the `extends` chain is followed from each class and
    /// checked for a return to its start.
    ///
    /// **Every class in the cycle reports**, not only the one that closes it,
    /// which is why the walk starts afresh from each class rather than sharing
    /// a visited set. §745.
    fn check_base_chain_is_acyclic(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let mut at = self.base_class_declaration_of(node);
        for _ in 0..MAX_ALIAS_HOPS {
            let Some(base) = at else { return };
            if base == node {
                break;
            }
            at = self.base_class_declaration_of(base);
        }
        if at != Some(node) {
            return;
        }
        let name = match self.node_map.get(node) {
            Some(Node::ClassDeclaration(class)) => class.name.and_then(|name| name.node_id),
            _ => None,
        };
        let Some(name) = name else { return };
        let Some(file) = self.source_file_of_for_diagnostics(name) else { return };
        let span = self.nodes.span(name);
        let text = self.identifier_text(name).unwrap_or_default().to_string();
        self.report(
            file,
            Diagnostic::with_args(
                &messages::_0_IS_REFERENCED_DIRECTLY_OR_INDIRECTLY_IN_ITS_OWN_BASE_EXPRESSION,
                span,
                [text],
            ),
        );
    }

    /// The declaration of a class's `extends` base, resolved from the leftmost
    /// identifier of its heritage expression. §745.
    fn base_class_declaration_of(&mut self, node: NodeId) -> Option<NodeId> {
        let clauses = match self.node_map.get(node)? {
            Node::ClassDeclaration(class) => class.heritage_clauses,
            Node::ClassExpression(class) => class.heritage_clauses,
            _ => return None,
        };
        let base = clauses
            .iter()
            .find(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)?
            .types
            .first()?
            .expression
            .and_then(|expression| expression.node_id())?;
        // **`extends A.B.Base.W` is not a lookup of `W`.** Descending to the
        // rightmost segment and resolving it in the enclosing scope finds the
        // *local* `W` — the class itself — and reports a cycle that is not
        // there. `declFileWithClassNameConflictingWithClassReferredByExtendsClause`
        // is the fixture and it was this build's two wrong lines. The receiver
        // decides the member, which is what §698 built for TS2449. §745.
        let symbol = if let Some(Node::PropertyAccessExpression(access)) = self.node_map.get(base) {
            let member = access.name.and_then(|name| name.node_id())?;
            let text = self.identifier_text(member).map(str::to_string)?;
            self.qualified_member_of_namespace(member, &text)?
        } else {
            let text = self.identifier_text(base).map(str::to_string)?;
            self.binder.resolve_name(self.nodes, self.node_map, base, &text, SymbolFlags::CLASS)?
        };
        self.binder.symbols().get(self.binder.merged_symbol(symbol)).value_declaration
    }

    /// The written spelling of a computed property name, when it is an
    /// identifier or a chain of them.
    ///
    /// `[Symbol.isConcatSpreadable]` answers `Symbol.isConcatSpreadable`; `[f()]`
    /// answers `None`, because a call is textually identical between two
    /// occurrences and names nothing upstream would late-bind. §757.
    fn computed_name_spelling(&self, name: NodeId) -> Option<String> {
        let Some(Node::ComputedPropertyName(computed)) = self.node_map.get(name) else {
            return None;
        };
        let mut at = computed.expression.and_then(|expression| expression.node_id())?;
        let mut parts: Vec<String> = Vec::new();
        loop {
            match self.node_map.get(at)? {
                Node::Identifier(identifier) => {
                    parts.push(identifier.text.to_string());
                    break;
                }
                Node::PropertyAccessExpression(access) => {
                    let member = access.name.and_then(|member| member.node_id())?;
                    parts.push(self.identifier_text(member)?.to_string());
                    at = access.expression.and_then(|expression| expression.node_id())?;
                }
                _ => return None,
            }
        }
        parts.reverse();
        Some(parts.join("."))
    }

    /// TS2515 — `Non-abstract class '{0}' does not implement inherited abstract
    /// member '{1}' from class '{2}'.`
    ///
    /// `checkKindsOfPropertyMemberOverrides`'s `notImplementedInfo` pass
    /// (`checker.go:4664`): the abstract members of every base, minus what the
    /// derived class provides, reported at the derived class's **name**.
    ///
    /// **Only the single-member form is built** — upstream has four by count and
    /// the plural ones join a list of names this port would have to spell the
    /// same way. §679 declined TS2460 on the same ground. §761.
    fn check_abstract_members_implemented(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(Node::ClassDeclaration(class)) = self.node_map.get(node) else { return };
        if has_modifier(class.modifiers, SyntaxKind::AbstractKeyword) {
            return;
        }
        let Some(name) = class.name.and_then(|name| name.node_id) else { return };
        // **A local enumeration, not `class_member_shape`.** That helper omits
        // methods on purpose — it serves TS2610's property-versus-accessor
        // question — and widening it would move a measured rule to serve this
        // one. §686's lesson, applied before the measurement. §761.
        let mut provided: Vec<String> = class.members.iter().filter_map(member_name_text).collect();
        let mut missing: Vec<(String, String)> = Vec::new();
        let mut at = self.base_class_declaration_of(node);
        for _ in 0..MAX_ALIAS_HOPS {
            let Some(base) = at else { break };
            let Some(Node::ClassDeclaration(base_class)) = self.node_map.get(base) else { break };
            let base_name = base_class.name.map_or_else(String::new, |it| it.text.to_string());
            for member in base_class.members {
                let Some(member_name) = member_name_text(member) else { continue };
                let is_abstract = member
                    .node_id()
                    .and_then(|id| self.node_map.get(id))
                    .and_then(modifiers_of)
                    .is_some_and(|m| {
                        tsr_ast::has_syntactic_modifier(m, SyntaxKind::AbstractKeyword)
                    });
                if is_abstract && !provided.contains(&member_name) {
                    missing.push((member_name.clone(), base_name.clone()));
                }
                provided.push(member_name);
            }
            at = self.base_class_declaration_of(base);
        }
        if missing.len() != 1 {
            return;
        }
        let (member_name, base_name) = missing.remove(0);
        let Some(file) = self.source_file_of_for_diagnostics(name) else { return };
        let span = self.nodes.span(name);
        let derived = self.identifier_text(name).unwrap_or_default().to_string();
        self.report(
            file,
            Diagnostic::with_args(
                &messages::NON_ABSTRACT_CLASS_0_DOES_NOT_IMPLEMENT_INHERITED_ABSTRACT_MEMBER_1_FROM_CLASS_2,
                span,
                [derived, member_name, base_name],
            ),
        );
    }

    /// Is this node inside a heritage clause that is not the **first** of its
    /// token kind on its declaration? §792.
    /// Is this name one of the **extra** types in a class's `extends` clause —
    /// `B` in `class C extends A, B`? A class extends one class, so upstream
    /// reports TS1174 on `typeNodes[1]` and resolves none of them beyond the
    /// first; §833 gave a second *clause* the same status and this is the same
    /// fact one level down. `implements` and an interface's `extends` both take
    /// many types and every one is resolved. §874.
    fn is_extra_type_in_a_class_extends_clause(&self, node: NodeId) -> bool {
        let Some(with_arguments) = self.nodes.parent(node) else { return false };
        if self.nodes.kind(with_arguments) != SyntaxKind::ExpressionWithTypeArguments {
            return false;
        }
        let Some(clause) = self.nodes.parent(with_arguments) else { return false };
        let Some(Node::HeritageClause(heritage)) = self.node_map.get(clause) else { return false };
        if heritage.token.kind != SyntaxKind::ExtendsKeyword {
            return false;
        }
        if self
            .nodes
            .parent(clause)
            .is_none_or(|owner| self.nodes.kind(owner) == SyntaxKind::InterfaceDeclaration)
        {
            return false;
        }
        heritage
            .types
            .first()
            .and_then(|first| first.node_id)
            .is_some_and(|first| first != with_arguments)
    }

    fn in_duplicate_heritage_clause(&self, node: NodeId) -> bool {
        let Some(clause) =
            self.nodes.ancestors(node).find(|&a| self.nodes.kind(a) == SyntaxKind::HeritageClause)
        else {
            return false;
        };
        let Some(Node::HeritageClause(this)) = self.node_map.get(clause) else { return false };
        let Some(owner) = self.nodes.parent(clause) else { return false };
        let clauses = match self.node_map.get(owner) {
            Some(Node::ClassDeclaration(class)) => class.heritage_clauses,
            Some(Node::ClassExpression(class)) => class.heritage_clauses,
            Some(Node::InterfaceDeclaration(interface)) => interface.heritage_clauses,
            _ => return false,
        };
        clauses
            .iter()
            .find(|each| each.token.kind == this.token.kind)
            .and_then(|first| first.node_id)
            .is_some_and(|first| first != clause)
    }

    /// TS2842 — `'{0}' is an unused renaming of '{1}'. Did you intend to use it
    /// as a type annotation?`
    ///
    /// `checkVariableLikeDeclaration`'s binding-element arm
    /// (`checker.go:5813`), whose candidate test is entirely syntactic: a
    /// property name, an identifier name, part of a parameter declaration, and
    /// a containing function with **no body**. Upstream's comment is the
    /// rationale — *"variable renaming in function type notation is confusing,
    /// so we forbid it even if `noUnusedLocals` is not enabled."*
    ///
    /// `checkUnusedRenamedBindingElements` then requires zero reference kinds.
    /// In a body-less signature nothing can reference the name, so that test is
    /// upstream being careful rather than discriminating, and it is left out —
    /// the same set on this corpus, stated rather than assumed.
    ///
    /// The whole pass is skipped for a declaration file (`checker.go:2212`,
    /// `docs/parity/notes/misc-checks.md` §7).
    ///
    /// `docs/architecture/checker-notes-diag2.md` §804.
    fn check_renamed_binding_element_in_signature(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(Node::BindingElement(element)) = self.node_map.get(node) else { return };
        let Some(property_name) = element.property_name else { return };
        let Some(tsr_ast::BindingName::Identifier(name)) = element.name else { return };
        let Some(name_id) = name.node_id else { return };
        // `IsPartOfParameterDeclaration` — the walk out of nested patterns ends
        // at a parameter — and `NodeIsMissing(GetContainingFunction(node).Body())`.
        let mut at = self.nodes.parent(node);
        let mut in_parameter = false;
        while let Some(current) = at {
            match self.nodes.kind(current) {
                SyntaxKind::BindingElement
                | SyntaxKind::ObjectBindingPattern
                | SyntaxKind::ArrayBindingPattern => {}
                SyntaxKind::Parameter => in_parameter = true,
                _ => break,
            }
            at = self.nodes.parent(current);
        }
        if !in_parameter {
            return;
        }
        let Some(function) = at else { return };
        if !self.is_function_like_or_static_block(function) {
            return;
        }
        if self.function_like_has_body(function) {
            return;
        }
        // **`referenceKinds == 0` is not free.** §804 dropped upstream's
        // reference test on the premise that a body-less signature has nothing
        // that could reference the name; `({ name: alias }: Named) => typeof
        // alias` is the fixture's own counter-example and was eighteen wrong
        // lines. The scan is of the containing signature, for the name used
        // anywhere but as this element's own binding. §805.
        let renamed = self.identifier_text(name_id).unwrap_or_default().to_string();
        if self.subtree_mentions_identifier(function, &renamed, name_id, 0) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(name_id) else { return };
        // `checkSourceFile` runs `checkUnusedRenamedBindingElements` only
        // `if !sourceFile.IsDeclarationFile` (`checker.go:2212`).
        if self.file_is_ambient
            || self.module_host.is_some_and(|host| host.is_declaration_file(file))
        {
            return;
        }
        let span = self.nodes.span(name_id);
        let original = property_name
            .node_id()
            .and_then(|id| self.identifier_text(id))
            .unwrap_or_default()
            .to_string();
        self.report(
            file,
            Diagnostic::with_args(
                &messages::_0_IS_AN_UNUSED_RENAMING_OF_1_DID_YOU_INTEND_TO_USE_IT_AS_A_TYPE_ANNOTATION,
                span,
                [renamed, original],
            ),
        );
    }

    /// Does this subtree mention `text` as an identifier, other than at
    /// `except`? §805.
    fn subtree_mentions_identifier(
        &self,
        node: NodeId,
        text: &str,
        except: NodeId,
        depth: u32,
    ) -> bool {
        if depth > 64 {
            return false;
        }
        if node != except
            && let Some(Node::Identifier(identifier)) = self.node_map.get(node)
            && identifier.text == text
        {
            return true;
        }
        let mut children = Vec::new();
        if let Some(typed) = self.node_map.get(node) {
            tsr_ast::for_each_child_id(typed, |child| children.push(child));
        }
        // Name positions are declarations, not references: a binding
        // element's property name and bound name, and a type member's name
        // (`({ a: b }, { b: a })` references neither `a` nor `b`).
        let declared_names: Vec<NodeId> = match self.node_map.get(node) {
            Some(Node::BindingElement(element)) => [
                element.property_name.and_then(|name| name.node_id()),
                element.name.and_then(|name| name.node_id()),
            ]
            .into_iter()
            .flatten()
            .filter(|&name| self.nodes.kind(name) == SyntaxKind::Identifier)
            .collect(),
            Some(Node::PropertySignatureDeclaration(signature)) => {
                signature.name.node_id().into_iter().collect()
            }
            Some(Node::MethodSignatureDeclaration(signature)) => {
                signature.name.node_id().into_iter().collect()
            }
            _ => Vec::new(),
        };
        children
            .into_iter()
            .filter(|child| !declared_names.contains(child))
            .any(|child| self.subtree_mentions_identifier(child, text, except, depth + 1))
    }

    /// Does this function-like node have a body? §804.
    fn function_like_has_body(&self, node: NodeId) -> bool {
        match self.node_map.get(node) {
            Some(Node::FunctionDeclaration(f)) => f.body.is_some(),
            Some(Node::FunctionExpression(f)) => f.body.is_some(),
            Some(Node::ArrowFunction(f)) => f.body.is_some(),
            Some(Node::MethodDeclaration(m)) => m.body.is_some(),
            Some(Node::ConstructorDeclaration(c)) => c.body.is_some(),
            Some(Node::GetAccessorDeclaration(a)) => a.body.is_some(),
            Some(Node::SetAccessorDeclaration(a)) => a.body.is_some(),
            _ => false,
        }
    }

    /// A `declare` modifier on the declaration itself.
    ///
    /// The *file* half of the ambient context comes from [`FileContext`]; this
    /// is the declaration half, and together they are what upstream reads off
    /// `node.Flags` in one test.
    fn declaration_is_ambient(&self, node: NodeId) -> bool {
        match self.node_map.get(node) {
            Some(Node::FunctionDeclaration(declaration)) => {
                has_modifier(declaration.modifiers, SyntaxKind::DeclareKeyword)
            }
            Some(Node::MethodDeclaration(declaration)) => {
                has_modifier(declaration.modifiers, SyntaxKind::DeclareKeyword)
            }
            _ => false,
        }
    }

    /// `ast.GetNameOfDeclaration` for the kinds this rule reports on.
    pub(crate) fn declaration_name_of(&self, node: NodeId) -> Option<NodeId> {
        match self.node_map.get(node)? {
            Node::FunctionDeclaration(declaration) => {
                declaration.name.and_then(|name| name.node_id)
            }
            Node::MethodDeclaration(declaration) => declaration.name.node_id(),
            Node::MethodSignatureDeclaration(signature) => signature.name.node_id(),
            Node::VariableDeclaration(declaration) => declaration.name.and_then(|n| n.node_id()),
            Node::BindingElement(element) => element.name.and_then(|n| n.node_id()),
            Node::ClassDeclaration(declaration) => declaration.name.and_then(|name| name.node_id),
            Node::ClassExpression(expression) => expression.name.and_then(|name| name.node_id),
            Node::InterfaceDeclaration(declaration) => declaration.name.and_then(|n| n.node_id),
            Node::ModuleDeclaration(declaration) => declaration.name.and_then(|n| n.node_id()),
            Node::EnumDeclaration(declaration) => declaration.name.and_then(|n| n.node_id),
            Node::EnumMember(member) => member.name.node_id(),
            Node::FunctionExpression(expression) => expression.name.and_then(|name| name.node_id),
            Node::GetAccessorDeclaration(accessor) => accessor.name.node_id(),
            Node::SetAccessorDeclaration(accessor) => accessor.name.node_id(),
            Node::TypeAliasDeclaration(declaration) => declaration.name.and_then(|n| n.node_id),
            Node::PropertyDeclaration(declaration) => declaration.name.node_id(),
            Node::PropertySignatureDeclaration(signature) => signature.name.node_id(),
            Node::NamespaceImport(import) => import.name.and_then(|n| n.node_id),
            _ => None,
        }
    }

    /// `scanner.GetErrorRangeForNode` (`scanner.go:2588`) — the span a
    /// diagnostic naming `node` is actually reported at.
    ///
    /// Upstream routes **every** checker diagnostic through this, in
    /// `NewDiagnosticForNode`. For fifteen declaration kinds the span is the
    /// declaration's *name*, which is why upstream underlines `named` in
    /// `function named() { … } || undefined` and this port underlined
    /// `function` until `checker-notes-diag2.md` §48.
    ///
    /// Four of upstream's arms need the file's **text** or a scanner —
    /// `KindSourceFile`, `KindArrowFunction`, the case/default clauses, and
    /// `return`/`yield`/`constructor` — and this checker holds spans and no
    /// text (ADR-0034). Those keep the node's own span; §48 records the
    /// omission rather than hiding it, because a displaced `return` diagnostic
    /// is the symptom it would produce.
    /// [`Checker::error_span`], exposed for probes.
    ///
    /// A diagnostic's position is `error_span(anchor)`, so joining a baseline
    /// line back to the node that would carry it needs this and nothing else.
    /// §172's split reports *positions*; §173 needs the node **kind** at each
    /// one, and the join is by span. Read-only and allocation-free.
    #[must_use]
    pub fn error_span_of(&self, node: NodeId) -> tsr_core::Span {
        self.error_span(node)
    }

    pub(crate) fn error_span(&self, node: NodeId) -> tsr_core::Span {
        self.declaration_name_of(node).map_or_else(
            || self.nodes.span(node),
            |name| {
                // `if errorNode == nil` upstream falls back to the node's own
                // first token; a name node with an empty span is the same
                // "missing" case and takes the same fallback.
                let span = self.error_span(name);
                if span.start == span.end { self.nodes.span(node) } else { span }
            },
        )
    }

    /// Append to the collection upstream keeps as `c.diagnostics`
    /// (`checker.go:661`), drained by `GetDiagnostics` (`checker.go:13951`).
    pub(crate) fn report(&mut self, file: NodeId, diagnostic: Diagnostic) {
        self.diagnostics.push((file, diagnostic));
    }

    /// Every diagnostic this checker has reported, with the file each is in.
    ///
    /// The consumer *drains*; it does not compute. That is ADR-0040 decision
    /// (1), and the reason the collection lives on the `Checker` rather than
    /// being assembled by whoever walks the tree.
    #[must_use]
    pub fn diagnostics(&self) -> &[(NodeId, Diagnostic)] {
        &self.diagnostics
    }

    /// Is there any `declare module "…*…"` in the program?
    ///
    /// Upstream keeps `c.patternAmbientModules`, filled once while collecting
    /// globals in `initializeChecker` (`checker.go:1318`) and read by
    /// `resolveExternalModule` (`checker.go:15364`); this port has no such
    /// list, so the question is asked of the binder's global table. **A
    /// name-shape test, and it is deliberately coarse**: a global whose name
    /// contains `*` can only have come from a pattern module declaration, and
    /// the cost of a false `true` is silence on one case rather than a wrong
    /// diagnostic on another. The scan runs once, at checker construction
    /// (`Checker::has_pattern_ambient_modules`), as upstream's collection
    /// does, rather than over every global on each module-specifier query.
    fn has_pattern_ambient_module(&self) -> bool {
        self.has_pattern_ambient_modules
    }

    /// [`Checker::ambient_module`] is private to `symbols`; this is the same
    /// question asked from here.
    ///
    /// Duplicated rather than exposed because the `symbols` version is on the
    /// resolution path and its `pub(crate)` surface is what
    /// `checker-notes-modobj.md` §10 pins; a second caller changing that
    /// function's visibility for a diagnostic is the kind of coupling that makes
    /// the next resolution change look risky.
    fn ambient_module_for_diagnostics(&self, name: &str) -> Option<tsr_binder::SymbolId> {
        if tsr_path::is_external_module_name_relative(name) {
            return None;
        }
        let symbol = self.binder.ambient_module(name)?;
        self.binder
            .symbols()
            .get(symbol)
            .flags
            .intersects(tsr_binder::SymbolFlags::VALUE_MODULE)
            .then_some(symbol)
    }

    /// `node.Flags & ast.NodeFlagsInWithStatement`, recomputed from the tree.
    ///
    /// The *statement* of a `with`, not its expression: `with (a) { b }`
    /// resolves `a` normally and refuses `b`.
    fn is_inside_with_statement(&self, node: NodeId) -> bool {
        let mut current = node;
        while let Some(parent) = self.nodes.parent(current) {
            if let Some(Node::WithStatement(with)) = self.node_map.get(parent)
                && with.statement.and_then(|s| s.node_id()) == Some(current)
            {
                return true;
            }
            current = parent;
        }
        false
    }

    /// `ast.GetSourceFileOfNode`, reachable from this module.
    /// [`Checker::source_file_of_for_diagnostics`], exposed for probes (§173).
    #[must_use]
    pub fn source_file_of_diagnostics(&self, node: NodeId) -> Option<NodeId> {
        self.source_file_of_for_diagnostics(node)
    }

    pub(crate) fn source_file_of_for_diagnostics(&self, node: NodeId) -> Option<NodeId> {
        let mut current = node;
        loop {
            if self.nodes.kind(current) == SyntaxKind::SourceFile {
                return Some(current);
            }
            current = self.nodes.parent(current)?;
        }
    }
}

/// `core.NodeCoreModules()` (`internal/core/nodemodules.go`): every
/// `UnprefixedNodeCoreModules` name bare and with `node:`, plus
/// `ExclusivelyPrefixedNodeCoreModules`.
///
/// Upstream substitutes a *different* message for these
/// (`getCannotResolveModuleNameErrorForSpecificModule`, `checker.go:15110`).
/// The set is exact: a `node:` name not on it (`node:foo`) is an ordinary
/// TS2307. Until tsr-2zk.933 this was a refusal list that also admitted every
/// `node:` prefix, because a name on it only cost silence.
fn is_node_core_module(name: &str) -> bool {
    match name.strip_prefix("node:") {
        Some(bare) => {
            NODE_CORE_MODULES.contains(&bare)
                || matches!(bare, "quic" | "sea" | "sqlite" | "test" | "test/reporters")
        }
        None => NODE_CORE_MODULES.contains(&name),
    }
}

/// The names `core.NodeCoreModules()` returns.
const NODE_CORE_MODULES: &[&str] = &[
    "assert",
    "assert/strict",
    "async_hooks",
    "buffer",
    "child_process",
    "cluster",
    "console",
    "constants",
    "crypto",
    "dgram",
    "diagnostics_channel",
    "dns",
    "dns/promises",
    "domain",
    "events",
    "fs",
    "fs/promises",
    "http",
    "http2",
    "https",
    "inspector",
    "inspector/promises",
    "module",
    "net",
    "os",
    "path",
    "path/posix",
    "path/win32",
    "perf_hooks",
    "process",
    "punycode",
    "querystring",
    "readline",
    "readline/promises",
    "repl",
    "stream",
    "stream/consumers",
    "stream/promises",
    "stream/web",
    "string_decoder",
    "sys",
    "timers",
    "timers/promises",
    "tls",
    "trace_events",
    "tty",
    "url",
    "util",
    "util/types",
    "v8",
    "vm",
    "wasi",
    "worker_threads",
    "zlib",
];

/// `getSuggestedLibForNonExistentName` (`checker.go:1733`) — the **first** lib
/// entry of `getFeatureMap` (`utilities.go:1292`) for a name, if it has one.
///
/// The map's values are per-lib property lists that only TS2550 reads; the
/// *keys* are the whole of what TS2583 needs, plus the first entry's lib name
/// for the message. Ported as a sorted name→lib list rather than as the whole
/// nested map, because everything else in it belongs to a row this port has not
/// opened — and a partial copy of a table is easier to keep honest than a
/// partial copy of a table's shape.
///
/// Upstream reports the missing lib **before** the spelling suggestion
/// (`checker.go:1584` versus `:1590`), which is why this is asked first: `Map`
/// has near neighbours in most scopes, so the two arms are not commutative.
/// `getCannotFindNameDiagnosticForName` (`checker.go:13915`) — the name table
/// behind `Cannot find name`, minus the rows handled above it.
///
/// Each row upstream is a pair chosen by `UsesWildcardTypes()`
/// (`slices.Contains(options.Types, "*")`, `core/compileroptions.go:326`). This
/// corpus sets no `types`, so the slice is empty, the test is false and every
/// case takes the second variant — see §247. The wildcard halves (TS2580,
/// TS2581, TS2582, TS2867) are unreachable until `types` is resolved and are
/// deliberately absent rather than guessed at.
///
/// Reaching this needed [`is_specially_diagnosed_name`] narrowed in the same
/// change: it declined these fourteen names outright, which is why §247
/// measured zero. §249.
/// `isInitializerStringOrNumberLiteralExpression` plus the `true`/`false` and
/// `BigInt` arms (`grammarchecks.go:1978`).
/// Every declaration kind that carries modifiers, for the grammar checks that
/// scan them rather than asking about one. §280.
/// Whether a class member is a plain property or an accessor, for §309.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MemberKind {
    Property,
    Accessor,
    Method,
}

/// A class member's name, kind and name-node, skipping `static` and `private`
/// members — upstream skips a private on either side outright, and the rule is
/// about instance members.
fn class_member_shape(member: tsr_ast::ClassElement<'_>) -> Option<(&str, MemberKind, NodeId)> {
    let (name, kind, modifiers) = match member {
        tsr_ast::ClassElement::PropertyDeclaration(p) => {
            (p.name, MemberKind::Property, p.modifiers)
        }
        tsr_ast::ClassElement::GetAccessorDeclaration(a) => {
            (a.name, MemberKind::Accessor, a.modifiers)
        }
        tsr_ast::ClassElement::SetAccessorDeclaration(a) => {
            (a.name, MemberKind::Accessor, a.modifiers)
        }
        tsr_ast::ClassElement::MethodDeclaration(m) => (m.name, MemberKind::Method, m.modifiers),
        _ => return None,
    };
    if modifiers.iter().any(|modifier| {
        matches!(
            modifier,
            tsr_ast::ModifierLike::Token(token)
                if matches!(token.kind, SyntaxKind::StaticKeyword | SyntaxKind::PrivateKeyword)
        )
    }) {
        return None;
    }
    let id = name.node_id()?;
    match name {
        tsr_ast::PropertyName::Identifier(identifier) => Some((identifier.text, kind, id)),
        _ => None,
    }
}

/// Types no signature resolution can make callable. §318.
pub(crate) fn modifiers_of(typed: Node<'_>) -> Option<&[tsr_ast::ModifierLike<'_>]> {
    Some(match typed {
        Node::ClassDeclaration(n) => n.modifiers,
        Node::ClassExpression(n) => n.modifiers,
        Node::InterfaceDeclaration(n) => n.modifiers,
        Node::TypeAliasDeclaration(n) => n.modifiers,
        Node::EnumDeclaration(n) => n.modifiers,
        Node::ModuleDeclaration(n) => n.modifiers,
        Node::FunctionDeclaration(n) => n.modifiers,
        Node::VariableStatement(n) => n.modifiers,
        Node::ImportDeclaration(n) => n.modifiers,
        Node::ImportEqualsDeclaration(n) => n.modifiers,
        Node::ExportDeclaration(n) => n.modifiers,
        Node::ExportAssignment(n) => n.modifiers,
        Node::PropertyDeclaration(n) => n.modifiers,
        Node::MethodDeclaration(n) => n.modifiers,
        Node::ParameterDeclaration(n) => n.modifiers,
        Node::TypeParameterDeclaration(n) => n.modifiers,
        // **The function-like kinds.** `async` lives on these and the walk that
        // reads this accessor could not see it: `async function await() {}` and
        // `async (await) => {}` are TS1359 and were silent. §621.
        Node::FunctionExpression(n) => n.modifiers,
        Node::ArrowFunction(n) => n.modifiers,
        Node::ConstructorDeclaration(n) => n.modifiers,
        Node::GetAccessorDeclaration(n) => n.modifiers,
        Node::SetAccessorDeclaration(n) => n.modifiers,
        Node::PropertySignatureDeclaration(n) => n.modifiers,
        Node::MethodSignatureDeclaration(n) => n.modifiers,
        Node::IndexSignatureDeclaration(n) => n.modifiers,
        // §-earlier widened this from sixteen arms to twenty-four to reach the
        // signature kinds — `CallSignature`, `ConstructSignature`,
        // `IndexSignature`, `MethodSignature` — and stopped one short of the two
        // **type** forms that carry the same modifiers in the same positions.
        // The other seven kinds with a `modifiers` field cannot legally have
        // one: the parser attaches recovered modifiers to whatever it was
        // parsing. §732.
        Node::FunctionTypeNode(n) => n.modifiers,
        Node::ConstructorTypeNode(n) => n.modifiers,
        _ => return None,
    })
}

fn cannot_find_name_message(name: &str) -> Option<&'static tsr_diagnostics::Message> {
    Some(match name {
        "document" | "console" => {
            &messages::CANNOT_FIND_NAME_0_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CHANGING_THE_LIB_COMPILER_OPTION_TO_INCLUDE_DOM
        }
        "$" => {
            &messages::CANNOT_FIND_NAME_0_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_JQUERY_TRY_NPM_I_SAVE_DEV_TYPES_SLASHJQUERY_AND_THEN_ADD_JQUERY_TO_THE_TYPES_FIELD_IN_YOUR_TSCONFIG
        }
        "beforeEach" | "describe" | "suite" | "it" | "test" => {
            &messages::CANNOT_FIND_NAME_0_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_A_TEST_RUNNER_TRY_NPM_I_SAVE_DEV_TYPES_SLASHJEST_OR_NPM_I_SAVE_DEV_TYPES_SLASHMOCHA_AND_THEN_ADD_JEST_OR_MOCHA_TO_THE_TYPES_FIELD_IN_YOUR_TSCONFIG
        }
        "process" | "require" | "Buffer" | "module" | "NodeJS" => {
            &messages::CANNOT_FIND_NAME_0_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE_DEV_TYPES_SLASHNODE_AND_THEN_ADD_NODE_TO_THE_TYPES_FIELD_IN_YOUR_TSCONFIG
        }
        "Bun" => {
            &messages::CANNOT_FIND_NAME_0_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_BUN_TRY_NPM_I_SAVE_DEV_TYPES_SLASHBUN_AND_THEN_ADD_BUN_TO_THE_TYPES_FIELD_IN_YOUR_TSCONFIG
        }
        _ => return None,
    })
}

pub(crate) fn suggested_lib_for(name: &str) -> Option<&'static str> {
    LIB_FEATURE_NAMES
        .binary_search_by_key(&name, |(feature, _)| *feature)
        .ok()
        .map(|index| LIB_FEATURE_NAMES[index].1)
}

/// `getFeatureMap`'s keys with each one's first lib, sorted by name.
const LIB_FEATURE_NAMES: &[(&str, &str)] = &[
    ("Array", "es2015"),
    ("ArrayBuffer", "es2024"),
    ("ArrayConstructor", "es2015"),
    ("AsyncDisposableStack", "esnext"),
    ("AsyncGenerator", "es2018"),
    ("AsyncGeneratorFunction", "es2018"),
    ("AsyncIterable", "es2018"),
    ("AsyncIterableIterator", "es2018"),
    ("AsyncIterator", "es2015"),
    ("Atomics", "es2017"),
    ("BigInt", "es2020"),
    ("BigInt64Array", "es2020"),
    ("BigUint64Array", "es2020"),
    ("DataView", "es2015"),
    ("Date", "es2015"),
    ("DateTimeFormat", "es2017"),
    ("DisposableStack", "esnext"),
    ("Error", "es2022"),
    ("ErrorConstructor", "es2022"),
    ("Float16Array", "esnext"),
    ("Float32Array", "es2015"),
    ("Float64Array", "es2015"),
    ("Int16Array", "es2015"),
    ("Int32Array", "es2015"),
    ("Int8Array", "es2015"),
    ("Intl", "es2015"),
    ("Iterator", "es2015"),
    ("Map", "es2015"),
    ("MapConstructor", "es2015"),
    ("Math", "es2015"),
    ("NumberConstructor", "es2015"),
    ("NumberFormat", "es2015"),
    ("ObjectConstructor", "es2015"),
    ("Promise", "es2015"),
    ("PromiseConstructor", "es2015"),
    ("Reflect", "es2015"),
    ("RegExp", "es2015"),
    ("RegExpConstructor", "es2015"),
    ("RegExpExecArray", "es2015"),
    ("RegExpMatchArray", "es2015"),
    ("RelativeTimeFormat", "es2020"),
    ("Set", "es2015"),
    ("SharedArrayBuffer", "es2017"),
    ("String", "es2015"),
    ("StringConstructor", "es2015"),
    ("Symbol", "es2015"),
    ("SymbolConstructor", "es2015"),
    ("Uint16Array", "es2015"),
    ("Uint32Array", "es2015"),
    ("Uint8Array", "es2015"),
    ("Uint8ArrayConstructor", "esnext"),
    ("Uint8ClampedArray", "es2015"),
    ("WeakMap", "es2015"),
    ("WeakSet", "es2015"),
];

/// `scanner.TokenToString` for the modifiers a `.js` file may not carry. §569.
fn js_only_modifier_text(kind: SyntaxKind) -> &'static str {
    match kind {
        SyntaxKind::DeclareKeyword => "declare",
        SyntaxKind::AbstractKeyword => "abstract",
        SyntaxKind::PublicKeyword => "public",
        SyntaxKind::PrivateKeyword => "private",
        SyntaxKind::ProtectedKeyword => "protected",
        SyntaxKind::ReadonlyKeyword => "readonly",
        SyntaxKind::OverrideKeyword => "override",
        SyntaxKind::ConstKeyword => "const",
        SyntaxKind::InKeyword => "in",
        SyntaxKind::OutKeyword => "out",
        _ => "",
    }
}

fn is_numeric_binary_operator(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::MinusToken
            | SyntaxKind::AsteriskToken
            | SyntaxKind::AsteriskAsteriskToken
            | SyntaxKind::SlashToken
            | SyntaxKind::PercentToken
            | SyntaxKind::LessThanLessThanToken
            | SyntaxKind::GreaterThanGreaterThanToken
            | SyntaxKind::GreaterThanGreaterThanGreaterThanToken
            | SyntaxKind::AmpersandToken
            | SyntaxKind::BarToken
            | SyntaxKind::CaretToken
            | SyntaxKind::PlusToken
            | SyntaxKind::PlusEqualsToken
            | SyntaxKind::LessThanToken
            | SyntaxKind::GreaterThanToken
            | SyntaxKind::LessThanEqualsToken
            | SyntaxKind::GreaterThanEqualsToken
            | SyntaxKind::MinusEqualsToken
            | SyntaxKind::AsteriskEqualsToken
            | SyntaxKind::SlashEqualsToken
            | SyntaxKind::PercentEqualsToken
            // The rest of `checkBinaryLikeExpressionWorker`'s arithmetic arm
            // (`checker.go:12358`): every compound form reaches
            // `checkArithmeticOperandType` before `checkAssignmentOperator`.
            | SyntaxKind::AsteriskAsteriskEqualsToken
            | SyntaxKind::LessThanLessThanEqualsToken
            | SyntaxKind::GreaterThanGreaterThanEqualsToken
            | SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
            | SyntaxKind::AmpersandEqualsToken
            | SyntaxKind::BarEqualsToken
            | SyntaxKind::CaretEqualsToken
    )
}

/// `ast.HasSyntacticModifier` for one keyword.
///
/// A decorator in the modifier list is not a modifier; the enum keeps them
/// together because the parser does (`ModifierLike`).
/// Is this class element declared `static`?
///
/// Upstream's `ast.IsStatic(lastLocation)` (`nameresolver.go:178`).
fn class_element_is_static(node: Option<Node<'_>>) -> bool {
    let modifiers = match node {
        Some(Node::PropertyDeclaration(n)) => n.modifiers,
        Some(Node::MethodDeclaration(n)) => n.modifiers,
        Some(Node::GetAccessorDeclaration(n)) => n.modifiers,
        Some(Node::SetAccessorDeclaration(n)) => n.modifiers,
        _ => return false,
    };
    has_modifier(modifiers, SyntaxKind::StaticKeyword)
}

/// The `<T, U>` list this node declares, empty for a node that declares none.
///
/// The union of the node kinds upstream's four `checkTypeParameters` call sites
/// cover (`checker.go:2742`, `:4297`, `:4998`, `:6887`). Written once because
/// two rules read it and a short list silently drops kinds — `FunctionTypeNode`
/// is the one `duplicateTypeParameters3` needs.
fn type_parameters_of(node: Node<'_>) -> &[&tsr_ast::TypeParameterDeclaration<'_>] {
    match node {
        Node::ClassDeclaration(declaration) => declaration.type_parameters,
        Node::ClassExpression(declaration) => declaration.type_parameters,
        Node::InterfaceDeclaration(declaration) => declaration.type_parameters,
        Node::TypeAliasDeclaration(declaration) => declaration.type_parameters,
        Node::FunctionDeclaration(declaration) => declaration.type_parameters,
        Node::FunctionExpression(declaration) => declaration.type_parameters,
        Node::ArrowFunction(declaration) => declaration.type_parameters,
        Node::MethodDeclaration(declaration) => declaration.type_parameters,
        Node::MethodSignatureDeclaration(signature) => signature.type_parameters,
        Node::ConstructorDeclaration(declaration) => declaration.type_parameters,
        Node::CallSignatureDeclaration(signature) => signature.type_parameters,
        Node::ConstructSignatureDeclaration(signature) => signature.type_parameters,
        Node::FunctionTypeNode(node) => node.type_parameters,
        Node::ConstructorTypeNode(node) => node.type_parameters,
        _ => &[],
    }
}

/// `scanner.TokenToString` for the modifier keywords, which is every kind this
/// module needs it for.
fn modifier_keyword_text(kind: SyntaxKind) -> Option<&'static str> {
    Some(match kind {
        SyntaxKind::PublicKeyword => "public",
        SyntaxKind::PrivateKeyword => "private",
        SyntaxKind::ProtectedKeyword => "protected",
        SyntaxKind::StaticKeyword => "static",
        SyntaxKind::AbstractKeyword => "abstract",
        SyntaxKind::AsyncKeyword => "async",
        SyntaxKind::DeclareKeyword => "declare",
        SyntaxKind::ExportKeyword => "export",
        SyntaxKind::DefaultKeyword => "default",
        SyntaxKind::AccessorKeyword => "accessor",
        SyntaxKind::OverrideKeyword => "override",
        SyntaxKind::ConstKeyword => "const",
        SyntaxKind::InKeyword => "in",
        SyntaxKind::OutKeyword => "out",
        _ => return None,
    })
}

pub(crate) fn has_modifier(modifiers: &[ModifierLike<'_>], keyword: SyntaxKind) -> bool {
    modifiers
        .iter()
        .any(|modifier| matches!(modifier, ModifierLike::Token(token) if token.kind == keyword))
}

/// The fourteen names `getCannotFindNameDiagnosticForName` (`checker.go:13915`)
/// substitutes a different message for.
///
/// Upstream picks TS2580/2581/2582/2583/2584/2591/2593 for these depending on
/// the name and on `UsesWildcardTypes`, so reporting TS2304 for any of them is a
/// wrong code at a right position. A refusal list, like the Node core modules
/// above.
impl Checker<'_, '_> {
    /// Is this reference enclosed by a function-like that owns an `arguments`
    /// object? An **arrow function is not one** — it closes over the
    /// enclosing function's, and at top level there is none. §951.
    pub(crate) fn reference_has_non_arrow_function_container(&self, node: NodeId) -> bool {
        self.nodes.ancestors(node).any(|ancestor| {
            matches!(
                self.nodes.kind(ancestor),
                SyntaxKind::FunctionDeclaration
                    | SyntaxKind::FunctionExpression
                    | SyntaxKind::MethodDeclaration
                    | SyntaxKind::Constructor
                    | SyntaxKind::GetAccessor
                    | SyntaxKind::SetAccessor
            )
        })
    }
}

fn is_specially_diagnosed_name(name: &str) -> bool {
    matches!(
        name,
        // Both are **synthesised** upstream and declared by no symbol here, so
        // every reference would report where upstream reports none. A refusal
        // about *resolution*, which is why §249 narrowed this list to these two
        // rather than deleting it: the other fourteen were declined only
        // because this port had no message for them, and §247 built those.
        //
        // `arguments` is synthesised by `resolveName`'s own arm
        // (`nameresolver.go`) for every function-like container. `bd tsr-o9tl`;
        // the row is 399 lines of the `.types` gradient too (STATUS §4.3).
        //
        // `globalThis` is synthesised by `initializeGlobals`.
        "arguments" | "globalThis"
    )
}

/// `core.GetSpellingSuggestion` (`internal/core/core.go:559`) — the closest
/// candidate to `name`, or `None` when nothing is close enough.
///
/// # Ported exactly, because an approximation is a wrong diagnostic
///
/// The distance is **not** plain Levenshtein. A case-only substitution costs
/// `0.1` and any other substitution costs `2` (`core.go:650`-`:653`), which is
/// what makes `$ERROR` a suggestion for `Error` — one deletion at cost 1 plus
/// five case differences at 0.1 each — while five *character* differences would
/// be 10 and miss by a mile. The acceptance threshold is
/// `floor(0.4 * len) + 0.9` and the length filter is `max(2, 0.34 * len)`.
///
/// Two deliberate departures, both stated:
///
/// - Upstream compares `len(candidateName)` in **bytes** against
///   `len(runeName)` in **runes** (`core.go:583`), a Go slip that only shows on
///   non-ASCII names. This compares runes on both sides. The corpus case that
///   drove this rule — `parserS7.6_A4.2_T1`, the Cyrillic alphabet — is exactly
///   where the two differ, and matching upstream's *behaviour* there would mean
///   reproducing the slip.
/// - The candidate ordering tie-break (`compare(candidate, bestCandidate)`) is
///   dropped: this caller asks only whether a suggestion exists, never which.
///
/// It lives here rather than in `tsr-core` — upstream's home for it — because
/// there is one consumer. The second consumer is the scanner's regular-
/// expression property-name suggestions (`scanner/regexp.go:955`), unported;
/// it moves when that lands.
pub(crate) fn spelling_suggestion<'a>(name: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let target: Vec<char> = name.chars().collect();
    #[allow(clippy::cast_precision_loss, reason = "identifier lengths are small")]
    let length = target.len() as f64;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "0.34 * a small positive length"
    )]
    let maximum_length_difference = usize::max(2, (length * 0.34) as usize);
    let mut best_distance = (length * 0.4).floor() + 0.9;
    let mut best: Option<&'a str> = None;

    for candidate in candidates {
        if candidate.is_empty() || *candidate == name {
            continue;
        }
        let other: Vec<char> = candidate.chars().collect();
        if usize::abs_diff(other.len(), target.len()) > maximum_length_difference {
            continue;
        }
        // "Only consider candidates less than 3 characters long when they
        // differ by case" (`core.go:589`).
        if other.len() < 3 && !candidate.eq_ignore_ascii_case(name) {
            continue;
        }
        if let Some(distance) = levenshtein_with_max(&target, &other, best_distance) {
            // **Only a strict improvement replaces the best** (`core.go:599`),
            // and a tie is broken by the comparator — `strings.Compare` for
            // this caller, so lexicographically. Assigning outside the test
            // accepted every candidate the distance function admitted and kept
            // the *last*, which is a tie-break by iteration order and so not
            // one. §859.
            if distance < best_distance {
                best_distance = distance;
                best = Some(candidate);
            } else if best.is_none_or(|current| *candidate < current) {
                best = Some(candidate);
            }
        }
    }
    best
}

/// `core.levenshteinWithMax` (`internal/core/core.go:627`).
///
/// Returns `None` for upstream's `-1`: the distance exceeded `max_value` and no
/// column can recover, so the candidate is rejected.
fn levenshtein_with_max(s1: &[char], s2: &[char], max_value: f64) -> Option<f64> {
    let width = s2.len() + 1;
    let big = max_value + 0.01;
    #[allow(clippy::cast_precision_loss, reason = "identifier lengths are small")]
    let mut previous: Vec<f64> = (0..width).map(|i| i as f64).collect();
    let mut current: Vec<f64> = vec![0.0; width];

    for i in 1..=s1.len() {
        #[allow(clippy::cast_precision_loss, reason = "identifier lengths are small")]
        let row = i as f64;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped to at least 1 by the max below"
        )]
        let min_j = usize::max((row - max_value).ceil().max(1.0) as usize, 1);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped to s2's length by the min below"
        )]
        let max_j = usize::min((max_value + row).floor().max(0.0) as usize, s2.len());
        let mut column_min = row;
        current[0] = row;
        for slot in current.iter_mut().take(min_j).skip(1) {
            *slot = big;
        }
        for j in min_j..=max_j {
            // A case-only difference costs 0.1; any other substitution costs 2.
            let substitution = if s1[i - 1].to_lowercase().eq(s2[j - 1].to_lowercase()) {
                previous[j - 1] + 0.1
            } else {
                previous[j - 1] + 2.0
            };
            let distance = if s1[i - 1] == s2[j - 1] {
                previous[j - 1]
            } else {
                (previous[j] + 1.0).min((current[j - 1] + 1.0).min(substitution))
            };
            current[j] = distance;
            column_min = column_min.min(distance);
        }
        for slot in current.iter_mut().take(s2.len() + 1).skip(max_j + 1) {
            *slot = big;
        }
        if column_min > max_value {
            return None;
        }
        std::mem::swap(&mut previous, &mut current);
    }
    let result = previous[s2.len()];
    (result <= max_value).then_some(result)
}

/// The written form of a name, a dotted name or a literal — the shapes a
/// computed property name is spelled with in practice.
///
/// A free function because it reads only the tree it is handed: the checker's
/// state has no part in spelling a name back out. See
/// [`declaration_name_to_string`].
fn entity_text_of(expression: tsr_ast::Expression<'_>) -> Option<String> {
    match expression {
        tsr_ast::Expression::Identifier(identifier) => Some(identifier.text.to_string()),
        tsr_ast::Expression::StringLiteral(literal) => Some(format!("\"{}\"", literal.text)),
        tsr_ast::Expression::NumericLiteral(literal) => Some(literal.text.to_string()),
        tsr_ast::Expression::PropertyAccessExpression(access) => {
            let target = entity_text_of(access.expression?)?;
            let member = match access.name? {
                tsr_ast::MemberName::Identifier(identifier) => identifier.text,
                tsr_ast::MemberName::PrivateIdentifier(private) => private.text,
            };
            Some(format!("{target}.{member}"))
        }
        _ => None,
    }
}

/// The `ModifierFlags` `checkFunctionOrConstructorSymbolWorker` checks for
/// agreement (`flagsToCheck`, `checker.go:3470`), as bits of a `u8`.
struct OverloadFlags;

impl OverloadFlags {
    const EXPORT: u8 = 1;
    const AMBIENT: u8 = 1 << 1;
    const PRIVATE: u8 = 1 << 2;
    const PROTECTED: u8 = 1 << 3;
    const ABSTRACT: u8 = 1 << 4;
    const ALL: u8 = Self::EXPORT | Self::AMBIENT | Self::PRIVATE | Self::PROTECTED | Self::ABSTRACT;
}

/// `scanner.DeclarationNameToString` (`internal/scanner/utilities.go`),
/// restricted to the three name kinds `checkPropertyInitialization`
/// accepts — `None` **is** the gate for the other kinds, so the caller
/// reads it as `IsIdentifier || IsPrivateIdentifier ||
/// IsComputedPropertyName` (`checker.go:4944`).
///
/// Upstream reads the node's **source text**. This checker holds spans and
/// no file text (ADR-0034 puts one `NodeTable` across a program and the
/// text stays with the `SourceFile`), so a computed name is reconstructed
/// from the tree. That is safe here and only here: the `diagnostics` suite
/// compares `(file, line, column, code)` and never the arguments. A shape
/// the reconstruction does not cover prints `(Missing)`, which is
/// upstream's own answer for a name node with an empty span.
///
/// A free function: spelling a name back out reads only the tree.
/// See `docs/architecture/checker-notes-diag2.md` §43.
pub(crate) fn declaration_name_to_string(name: tsr_ast::PropertyName<'_>) -> Option<String> {
    match name {
        tsr_ast::PropertyName::Identifier(identifier) => Some(identifier.text.to_string()),
        // The parser keeps the `#` in the text, as upstream's scanner does.
        tsr_ast::PropertyName::PrivateIdentifier(private) => Some(private.text.to_string()),
        tsr_ast::PropertyName::ComputedPropertyName(computed) => Some(format!(
            "[{}]",
            computed.expression.and_then(entity_text_of).unwrap_or_else(|| "(Missing)".to_string())
        )),
        _ => None,
    }
}

/// `ast.HasResolutionModeOverride` (`ast/utilities.go:3213`) over the
/// attributes of a resolution-mode override host:
/// `ImportAttributes.GetResolutionModeOverride` succeeds — exactly one
/// attribute, `resolution-mode`, valued `"import"` or `"require"`.
fn has_resolution_mode_override(attributes: Option<&tsr_ast::ImportAttributes<'_>>) -> bool {
    let Some([attribute]) = attributes.map(|attributes| attributes.attributes) else {
        return false;
    };
    let Some(tsr_ast::ImportAttributeName::StringLiteral(name)) = attribute.name else {
        return false;
    };
    name.text == "resolution-mode"
        && matches!(attribute.value.map(Node::from),
            Some(Node::StringLiteral(literal)) if literal.text == "import" || literal.text == "require")
}

/// [`Checker::alias_body_kind`]'s answer: whether the denoted type carries
/// `ObjectFlagsAnonymous`/`ObjectFlagsMapped`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AliasBodyKind {
    Anonymous,
    NotAnonymous,
}

/// One alias instantiation in [`Checker::alias_body_kind`]'s walk: each
/// substituted type parameter declaration, its argument node, and the frame
/// the argument is read in.
struct AliasFrame {
    bindings: Vec<(NodeId, NodeId, usize)>,
    parent: Option<usize>,
}

impl Checker<'_, '_> {
    /// Whether a property's written annotation is a reference (type
    /// reference, `import()` type or `typeof` query) whose own resolution
    /// failed — its type arguments, if any, all resolved —
    /// so that the property's `error` type is upstream's `errorType` from
    /// `getTypeFromTypeReference` rather than a gap carried up from a nested
    /// node. `docs/parity/notes/decls.md` §17.
    pub(crate) fn annotation_is_failed_reference(&mut self, annotation: Option<NodeId>) -> bool {
        // `getTypeFromTypeReference`, `getTypeFromImportTypeNode` and
        // `getTypeFromTypeQueryNode` each answer `errorType` when the entity
        // they name does not resolve.
        let arguments = match annotation.and_then(|id| self.node_map.get(id)) {
            Some(Node::TypeReferenceNode(reference)) => reference.type_arguments,
            Some(Node::ImportTypeNode(import)) => import.type_arguments,
            Some(Node::TypeQueryNode(query)) => query.type_arguments,
            _ => return false,
        };
        arguments.iter().all(|&argument| {
            let argument = self.get_type_from_type_node(argument);
            !self.is_error(argument)
        })
    }
}
