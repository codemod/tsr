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

use tsr_ast::{ClassElement, ModifierLike, Node, NodeId, SyntaxKind};
use tsr_binder::{NodeFacts, SymbolFlags};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

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
        let ambient = match typed {
            Node::ImportDeclaration(declaration) => {
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
                // Only the `require("x")` spelling names a module; `import a = b.c`
                // is an entity-name alias and resolves through the scope.
                if let Some(tsr_ast::ModuleReference::ExternalModuleReference(reference)) =
                    declaration.module_reference
                {
                    self.check_module_specifier(
                        node,
                        reference.expression.and_then(|e| e.node_id()),
                        false,
                    );
                }
                ambient
            }
            Node::ClassDeclaration(declaration) => {
                // `declare class C { x: number }` puts every member in an
                // ambient context, which is where upstream's flag would already
                // be set on the members themselves.
                let ambient =
                    ambient || has_modifier(declaration.modifiers, SyntaxKind::DeclareKeyword);
                self.check_property_initialization(declaration.members, ambient);
                self.check_heritage_conformance(node);
                self.check_property_overrides(node);
                self.check_index_constraints(node);
                self.check_duplicate_index_signatures(node);
                ambient
            }
            Node::ClassExpression(declaration) => {
                let ambient =
                    ambient || has_modifier(declaration.modifiers, SyntaxKind::DeclareKeyword);
                self.check_property_initialization(declaration.members, ambient);
                ambient
            }
            // `declare module "m" { … }` and `declare namespace N { … }` are
            // ambient contexts, and so is an *ambient* module's body whether or
            // not the keyword is repeated inside it.
            Node::ModuleDeclaration(declaration) => {
                ambient
                    || has_modifier(declaration.modifiers, SyntaxKind::DeclareKeyword)
                    || self.is_ambient_module_node(node)
            }
            Node::VariableStatement(statement) => {
                ambient || has_modifier(statement.modifiers, SyntaxKind::DeclareKeyword)
            }
            Node::FunctionDeclaration(declaration) => {
                self.check_function_or_constructor_symbol(node, ambient);
                let ambient =
                    ambient || has_modifier(declaration.modifiers, SyntaxKind::DeclareKeyword);
                self.check_generator_in_ambient_context(declaration.asterisk_token, ambient);
                self.check_implicit_any_parameters(node, ambient);
                self.check_implicit_any_return(node, ambient);
                ambient
            }
            Node::InterfaceDeclaration(_) => {
                self.check_heritage_conformance(node);
                self.check_index_constraints(node);
                self.check_duplicate_index_signatures(node);
                ambient
            }
            Node::EnumDeclaration(declaration) => {
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
                self.check_optional_parameter_initializer(node);
                self.check_parameter_initializer_needs_body(node);
                self.check_parameter_property_position(node, parameter.modifiers);
                self.check_annotated_initializer(node, ambient);
                ambient
            }
            Node::PropertySignatureDeclaration(_) => {
                self.check_implicit_any_member(node, ambient);
                ambient
            }
            Node::GetAccessorDeclaration(accessor) => {
                self.check_get_accessor_returns(accessor, ambient);
                ambient
            }
            Node::PropertyDeclaration(property) => {
                self.check_ambient_initializer(
                    node,
                    property.initializer,
                    property.r#type,
                    ambient,
                );
                self.check_annotated_initializer(node, ambient);
                ambient
            }
            Node::VariableDeclaration(declaration) => {
                self.check_outer_scoped_variable(node);
                self.check_ambient_initializer(
                    node,
                    declaration.initializer,
                    declaration.r#type,
                    ambient,
                );
                self.check_subsequent_declaration_type(node, declaration);
                self.check_variable_like_declaration(node, declaration, ambient);
                self.check_const_is_initialized(node, declaration, ambient);
                ambient
            }
            Node::ReturnStatement(_) => {
                self.check_grammar_statement_in_ambient_context(node, ambient);
                self.check_return_container(node, ambient);
                self.check_return_statement(node, ambient);
                ambient
            }
            Node::BinaryExpression(binary)
                if binary.operator_token.is_some_and(|t| is_numeric_binary_operator(t.kind)) =>
            {
                self.check_nullable_operand(node, ambient);
                self.check_operator_operands(node, ambient);
                self.check_arithmetic_operand_types(node, ambient);
                ambient
            }
            Node::BinaryExpression(binary)
                if binary.operator_token.is_some_and(|t| t.kind == SyntaxKind::CommaToken) =>
            {
                self.check_comma_left(node, binary.left.and_then(|left| left.node_id()));
                ambient
            }
            Node::BinaryExpression(binary)
                if binary.operator_token.is_some_and(|t| t.kind == SyntaxKind::EqualsToken) =>
            {
                self.check_assignment_operator(binary, ambient);
                self.check_reference_expression(node);
                ambient
            }
            Node::MethodDeclaration(_) | Node::ConstructorDeclaration(_) => {
                self.check_function_or_constructor_symbol(node, ambient);
                self.check_implicit_any_parameters(node, ambient);
                self.check_implicit_any_return(node, ambient);
                ambient
            }
            // §81. No walk arm claimed this kind before, which is what §49's
            // trap says to check before pricing a rule that measures zero.
            Node::MethodSignatureDeclaration(_) => {
                self.check_implicit_any_return(node, ambient);
                ambient
            }
            Node::FunctionExpression(_) | Node::ArrowFunction(_) => {
                self.check_implicit_any_parameters(node, ambient);
                ambient
            }
            Node::PropertyAccessExpression(_) => {
                self.check_nonexistent_property(node, ambient);
                self.check_readonly_assignment_target(node, ambient);
                self.check_private_property_access(node, ambient);
                ambient
            }
            Node::AsExpression(_) | Node::TypeAssertion(_) => {
                self.check_assertion_overlap(node, ambient);
                ambient
            }
            Node::BinaryExpression(_) => {
                self.check_comparison_overlap(node, ambient);
                self.check_operator_operands(node, ambient);
                ambient
            }
            Node::ComputedPropertyName(_) => {
                self.check_computed_property_name(node, ambient);
                ambient
            }
            Node::ObjectLiteralExpression(_) => {
                self.check_duplicate_object_literal_names(node);
                ambient
            }
            Node::DeleteExpression(_) => {
                self.check_reference_expression(node);
                ambient
            }
            Node::CallExpression(_) => {
                self.check_call_arity(node);
                ambient
            }
            Node::NewExpression(_) => {
                self.check_new_arity(node);
                ambient
            }
            Node::TypeReferenceNode(_) | Node::ExpressionWithTypeArguments(_) => {
                self.check_type_argument_arity(node);
                ambient
            }
            Node::QualifiedName(_) => {
                self.check_qualified_type_name(node);
                ambient
            }
            Node::PrefixUnaryExpression(_) | Node::PostfixUnaryExpression(_) => {
                self.check_increment_operand_type(node, ambient);
                ambient
            }
            Node::Identifier(identifier) => {
                self.check_identifier_assignment_target(node, ambient);
                self.check_readonly_identifier_assignment(node, ambient);
                self.check_value_identifier(node, identifier.text);
                self.check_type_reference_name(node, identifier.text);
                self.check_used_before_assigned(node, identifier.text);
                self.check_used_before_its_declaration(node, identifier.text);
                self.mark_identifier_reference(node, identifier.text);
                ambient
            }
            _ => ambient,
        };
        self.check_unreachable(node, ambient);
        // `checkGrammarModifiers` runs on the declaration that carries the
        // list. Bounded to class elements and parameters — `defaultKeywordWithoutExport1`
        // is the statement-level shape and is declined, §103.
        match typed {
            Node::YieldExpression(_) => self.check_yield_grammar(node),
            Node::AwaitExpression(_) => {
                self.check_await_in_parameter_initializer(node);
                self.check_await_in_non_async_function(node);
            }
            Node::ImportSpecifier(_) | Node::ExportSpecifier(_) => {
                self.report_missing_module_export(node);
                self.check_alias_symbol(node);
            }
            // `checkImportBinding` (`checker.go:5287`-`:5303`, `:5473`) and
            // `checkExportDeclaration`'s clause (`:5534`).
            Node::ImportClause(_) | Node::NamespaceImport(_) | Node::NamespaceExport(_) => {
                self.check_alias_symbol(node);
            }
            // `NodeCanBeDecorated` rejects every one of these outright.
            Node::EnumDeclaration(n) => self.check_illegal_decorator(n.modifiers),
            Node::ClassDeclaration(_) => self.check_type_parameter_lists_identical(node),
            Node::FunctionDeclaration(n) => self.check_illegal_decorator(n.modifiers),
            Node::InterfaceDeclaration(n) => {
                self.check_illegal_decorator(n.modifiers);
                self.check_type_parameter_lists_identical(node);
            }
            Node::TypeAliasDeclaration(n) => self.check_illegal_decorator(n.modifiers),
            Node::VariableStatement(n) => self.check_illegal_decorator(n.modifiers),
            Node::ImportEqualsDeclaration(n) => {
                self.check_illegal_decorator(n.modifiers);
                self.check_alias_symbol(node);
            }
            Node::ModuleDeclaration(n) => self.check_illegal_decorator(n.modifiers),
            Node::ImportDeclaration(n) => self.check_illegal_decorator(n.modifiers),
            Node::ExportDeclaration(n) => self.check_illegal_decorator(n.modifiers),
            Node::PropertyDeclaration(n) => self.check_modifier_order(node, n.modifiers),
            Node::MethodDeclaration(n) => self.check_modifier_order(node, n.modifiers),
            Node::GetAccessorDeclaration(n) => self.check_modifier_order(node, n.modifiers),
            Node::SetAccessorDeclaration(n) => self.check_modifier_order(node, n.modifiers),
            Node::ConstructorDeclaration(n) => self.check_modifier_order(node, n.modifiers),
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
        self.check_grammar_heritage_clauses(typed);
        self.check_override_kind(node, typed);
        self.check_this_before_super(node);
        self.check_super_in_derived_class(node);
        if matches!(typed, Node::GetAccessorDeclaration(_) | Node::SetAccessorDeclaration(_)) {
            self.check_grammar_accessor(node, typed);
        }
        self.check_grammar_parameter_list(node);
        self.check_grammar_modifier_shapes(node, typed);
        self.check_jsx_intrinsic_element(node, typed);
        self.check_jsx_factory_in_scope(typed);
        self.check_strict_mode_eval_or_arguments_sites(node, typed, ambient);
        self.check_contextual_identifier(node, ambient);
        self.check_type_parameter_list(type_parameters_of(typed));
        self.check_truthiness_sites(node, ambient);
        self.note_member_name_at(node);
        self.register_for_unused_check(node);
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
    /// | a **pattern** ambient module could match (`declare module "foo/*"`) | resolved by `FindBestPatternMatch` (`checker.go:15364`), which this port does not implement — so a match is *possible* and silence is the only sound answer |
    /// | a Node core module name (`fs`, `path`, …) | TS2580/TS2591, substituted by `getCannotResolveModuleNameErrorForSpecificModule` (`checker.go:15109`) |
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
        if !self.module_specifier_unfindable(specifier) {
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
        self.report(importing, Diagnostic::with_args(message, span, [text.to_string()]));
    }

    /// The boolean core of the TS2307 emitter, shared with
    /// `get_type_of_alias` (`checker-notes-callres.md` §31) so the
    /// diagnostic and the alias's `any` read ONE calibrated answer: TRUE
    /// only when every decline-gate passes and the host, consulted, found
    /// nothing. Position is the caller's test.
    pub(crate) fn module_specifier_unfindable(&mut self, specifier: NodeId) -> bool {
        let Some(Node::StringLiteral(literal)) = self.node_map.get(specifier) else {
            return false;
        };
        let text = literal.text;
        // `tryFindAmbientModule` (`checker.go:15154`), consulted before the host
        // exactly as `resolveExternalModule` does.
        if self.ambient_module_for_diagnostics(text).is_some() {
            return false;
        }
        // A pattern ambient module is unported (`checker.go:15364`); declining
        // whenever one *exists* is the sound bound, not whenever one matches.
        if self.has_pattern_ambient_module() {
            return false;
        }
        if is_node_core_module(text) {
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
        !host.module_resolution_found(importing, text)
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
            if self.is_error(declared)
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
        // Inside a `with` block upstream reports TS2410 — *"All symbols in a
        // 'with' block will have type 'any'"* — and resolves nothing
        // (`NodeFlagsInWithStatement`, read at `checker.go:29344` and four other
        // sites). The flag is another the parser here never sets, so the
        // question is asked of the ancestors.
        if self.is_inside_with_statement(node) {
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
        ) {
            return;
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
            .binder
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
            .resolve_name(
                self.nodes,
                self.node_map,
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
                true
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
        if let Some(suggestion) = self.spelling_suggestion_for(node, text) {
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
        // §247, reachable since §249.
        let message = cannot_find_name_message(text).unwrap_or(&messages::CANNOT_FIND_NAME_0);
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
    fn type_only_alias_declaration(&mut self, symbol: tsr_binder::SymbolId) -> Option<bool> {
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
            Node::ImportSpecifier(_) | Node::NamespaceImport(_) | Node::ImportClause(_) => {
                match self.node_map.get(enclosing(SyntaxKind::ImportClause)?)? {
                    Node::ImportClause(clause) => clause
                        .phase_modifier
                        .is_some_and(|token| token.kind == SyntaxKind::TypeKeyword)
                        .then_some(false),
                    _ => None,
                }
            }
            Node::ExportSpecifier(n) if n.is_type_only => Some(true),
            Node::ExportSpecifier(_) => {
                match self.node_map.get(enclosing(SyntaxKind::ExportDeclaration)?)? {
                    Node::ExportDeclaration(n) if n.is_type_only => Some(true),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn check_type_reference_name(&mut self, node: NodeId, text: &str) {
        if self.file_has_parse_errors || is_specially_diagnosed_name(text) {
            return;
        }
        let Some(parent) = self.nodes.parent(node) else { return };
        let Some(Node::TypeReferenceNode(reference)) = self.node_map.get(parent) else { return };
        if reference.type_name.and_then(|name| name.node_id()) != Some(node) {
            return;
        }
        let span = self.error_span(node);
        if text.is_empty() || span.start == span.end {
            return;
        }
        // A `TYPE` hit is a correct resolution and stays silent. §167: the
        // other two are TS2709 and TS2749, which the cascade now selects —
        // reachable only since §166 stopped the globals fallback from
        // answering every meaning.
        if self
            .binder
            .resolve_name(self.nodes, self.node_map, node, text, SymbolFlags::TYPE)
            .is_some()
        {
            return;
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
        // The suggestion arm, over the names in scope **with the TYPE
        // meaning**. §55 refused it outright because `names_in_scope` was
        // meaning-blind and answered a missing *type* with a nearby
        // *variable* — `parserRealSource13` was 105 wrong TS2552 lines for one
        // `AST`. `names_in_scope_with_meaning` is that refusal's named unlock
        // (§57).
        let candidates = self.binder.names_in_scope_with_meaning(
            self.nodes,
            self.node_map,
            node,
            SymbolFlags::TYPE,
        );
        if let Some(suggestion) = spelling_suggestion(text, &candidates) {
            let suggestion = suggestion.to_string();
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
        }
        None
    }

    /// Would `getSuggestedSymbolForNonexistentSymbol` (`checker.go:1591`) find
    /// something, so that upstream reports TS2552 rather than TS2304?
    ///
    /// The **whole** of TS2304 turns on this. `onFailedToResolveSymbol`
    /// (`checker.go:1564`) tries a spelling suggestion immediately before its
    /// fallthrough, so every name with a near neighbour in scope is a TS2552 and
    /// reporting TS2304 there is a wrong code at a right position. A first
    /// attempt used a hand-rolled within-one-edit test and
    /// `conformance/parserS7.6_A4.2_T1` alone produced **20 wrong lines** from
    /// it: `$ERROR` against `Error` is one deletion plus five case differences,
    /// which upstream's weighted distance accepts and a plain edit count does
    /// not. The algorithm is ported instead — see [`spelling_suggestion`].
    fn spelling_suggestion_for(&self, node: NodeId, text: &str) -> Option<String> {
        let candidates = self.binder.names_in_scope_with_meaning(
            self.nodes,
            self.node_map,
            node,
            SymbolFlags::VALUE,
        );
        spelling_suggestion(text, &candidates).map(ToString::to_string)
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
    fn is_value_reference(&self, node: NodeId) -> bool {
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
            Node::ExpressionWithTypeArguments(n) => {
                is(n.expression.and_then(|e| e.node_id()))
                    && self.nodes.parent(parent).is_some_and(|clause| {
                        matches!(
                            self.node_map.get(clause),
                            Some(Node::HeritageClause(heritage))
                                if heritage.token.kind == SyntaxKind::ExtendsKeyword
                        )
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
    fn check_used_before_its_declaration(&mut self, node: NodeId, text: &str) {
        if self.file_has_parse_errors {
            return;
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
            return;
        };
        let symbol = self.binder.merged_symbol(symbol);
        let entry = self.binder.symbols().get(symbol);
        // `checkResolvedBlockScopedVariable` (`checker.go:1888`) picks the
        // message off the symbol's flags and shares everything below.
        let is_class = entry.flags.intersects(SymbolFlags::CLASS);
        let (message, kinds): (_, &[SyntaxKind]) =
            if entry.flags.intersects(SymbolFlags::BLOCK_SCOPED_VARIABLE) {
                (
                    &messages::BLOCK_SCOPED_VARIABLE_0_USED_BEFORE_ITS_DECLARATION,
                    &[SyntaxKind::VariableDeclaration],
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
        if self.nodes.span(declaration).start <= self.nodes.span(node).start {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(file, Diagnostic::with_args(message, span, [text.to_string()]));
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
        let container = self.enclosing_block_scope_container(declaration);
        let mut came_from = node;
        for ancestor in self.nodes.ancestors(node) {
            if Some(ancestor) == container {
                return true;
            }
            // **Edge, not node** — §108. Upstream's own test is
            // `initializerOfProperty := propertyDeclaration.Initializer() == current`
            // (`checker.go:2025`): a `PropertyDeclaration` defers a use in its
            // *initialiser*, and a computed property name is not that. It is
            // evaluated where the class is, so it defers nothing.
            if self.nodes.kind(came_from) == SyntaxKind::ComputedPropertyName
                && matches!(
                    self.nodes.kind(ancestor),
                    SyntaxKind::PropertyDeclaration
                        | SyntaxKind::MethodDeclaration
                        | SyntaxKind::GetAccessor
                        | SyntaxKind::SetAccessor
                )
            {
                came_from = ancestor;
                continue;
            }
            if matches!(
                self.nodes.kind(ancestor),
                SyntaxKind::FunctionDeclaration
                    | SyntaxKind::FunctionExpression
                    | SyntaxKind::ArrowFunction
                    | SyntaxKind::MethodDeclaration
                    | SyntaxKind::GetAccessor
                    | SyntaxKind::SetAccessor
                    | SyntaxKind::Constructor
                    | SyntaxKind::ClassStaticBlockDeclaration
                    | SyntaxKind::PropertyDeclaration
                    | SyntaxKind::InterfaceDeclaration
                    | SyntaxKind::TypeAliasDeclaration
                    | SyntaxKind::TypeLiteral
                    | SyntaxKind::ComputedPropertyName
                    | SyntaxKind::Decorator
            ) {
                return false;
            }
            came_from = ancestor;
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

    /// Is this declaration in an ambient **context** — its own `declare`, or
    /// any containing one?
    ///
    /// [`Checker::declaration_is_ambient`] answers only the first half, which
    /// is all its callers need. `NodeFlagsAmbient` is upstream's *transitive*
    /// answer and this parser never sets it (see [`tsr_ast::NodeFlags::AMBIENT`],
    /// declared and written by nothing), so a rule reading the flag of a
    /// declaration rather than of a use has to walk.
    fn declaration_is_in_an_ambient_context(&self, declaration: NodeId) -> bool {
        std::iter::once(declaration).chain(self.nodes.ancestors(declaration)).any(|at| {
            // Every kind that can carry `declare`, not just the two §83 needed.
            // A `declare const o` puts the modifier on the enclosing
            // **VariableStatement**, so a rule handed the `VariableDeclaration`
            // sees no modifier at all — which is the whole of why
            // `controlFlowNullishCoalesce` broke under §96-§98 (`checker-notes-diag2.md`
            // §99). `NodeFlags::AMBIENT` would answer this in one read and is
            // one of the three flags this parser never sets.
            match self.node_map.get(at) {
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

    /// TS2454 — `Variable '{0}' is used before being assigned.`
    ///
    /// `checkIdentifier` (`checker.go:11191`), the arm reached when
    /// `assumeInitialized` is false and the *flow* type carries `undefined`
    /// while the declared type does not.
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
    fn check_used_before_assigned(&mut self, node: NodeId, text: &str) {
        if self.file_has_parse_errors || !self.strict_null_checks || !self.is_value_reference(node)
        {
            return;
        }
        // `assignmentKind == AssignmentKindDefinite` returns before the flow
        // section (`checker.go:11109`), so `x = 1` never reports even though the
        // flow type at `x` carries `undefined`.
        if self.is_definite_assignment_target(node) {
            return;
        }
        // A **destructuring** target is a definite assignment too, and
        // `is_definite_assignment_target` only knows the `x = 1` spelling.
        // `accessKind` (`ast.go:1426`) already answers for every spelling —
        // `({ x } = obj)`, `[x] = arr`, `({ a: x } = obj)` — and
        // `crate::unused` ported it whole for its own reasons. Reusing it here
        // is the same question asked once: `shorthandPropertyAssignmentsInDestructuring_ES6`,
        // `destructuringAssignmentWithDefault2`, `destructuringAssignment_private`
        // and `noUnusedLocals_destructuringAssignment` were 12 of this rule's
        // remaining wrong lines and all four are that shape.
        if self.is_write_only_access(node) {
            return;
        }
        if self.is_inside_with_statement(node) || self.is_in_type_query_or_type_node(node) {
            return;
        }
        let Some(parent) = self.nodes.parent(node) else { return };
        if matches!(
            self.nodes.kind(parent),
            SyntaxKind::ExportSpecifier | SyntaxKind::NonNullExpression
        ) {
            return;
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
            return;
        };
        let Some(declaration) = self.binder.symbols().get(symbol).declarations.first().copied()
        else {
            return;
        };
        // A reference **guarded by a condition that mentions the same name** is
        // one upstream has narrowed before it gets here, and the narrowings this
        // port does not model all leave `undefined` in the flow type.
        // `typeGuardOfFormIsType`'s `isC1(c1Orc2) && c1Orc2.p1` is the family:
        // the user-defined predicate removes `undefined` upstream, and the
        // second `c1Orc2` read as *used before being assigned* here. 43 of this
        // rule's 114 wrong lines are that shape.
        if self.reference_is_guarded_by_a_condition_on(node, text) {
            return;
        }
        let Some(Node::VariableDeclaration(variable)) = self.node_map.get(declaration) else {
            return;
        };
        // No initialiser, no `!`, and an explicit annotation — the annotation is
        // what keeps the auto-typed path (`t == autoType`, a different
        // diagnostic entirely) out of this rule.
        if variable.initializer.is_some()
            || variable.exclamation_token.is_some()
            || variable.r#type.is_none()
        {
            return;
        }
        // A `const` with no initialiser only occurs in an ambient context or
        // after a grammar error (TS1155), and upstream reaches neither: the
        // ambient flag short-circuits `assumeInitialized`
        // (`checker.go:11158`). `declare const b: B` supplied **227 of the
        // first measurement's 4,781 wrong lines from one case**
        // (`compiler/genericDefaults`), which is what put both tests here.
        let Some(list) = self.nodes.parent(declaration) else { return };
        if self.nodes.flags(list).intersects(tsr_ast::NodeFlags::CONST) {
            return;
        }
        if self.nodes.parent(list).and_then(|statement| self.node_map.get(statement)).is_some_and(
            |statement| match statement {
                Node::VariableStatement(variable) => {
                    has_modifier(variable.modifiers, SyntaxKind::DeclareKeyword)
                }
                _ => false,
            },
        ) {
            return;
        }
        // `for (x of …)` and `for (x in …)` assign on entry.
        if self.nodes.parent(list).is_some_and(|owner| {
            matches!(
                self.nodes.kind(owner),
                SyntaxKind::ForInStatement | SyntaxKind::ForOfStatement
            )
        }) {
            return;
        }
        // `isOuterVariable` (`checker.go:11128`) is a disjunct of
        // `assumeInitialized` **only when the variable is not never-initialized**:
        // `(isOuterVariable && !isNeverInitialized)` (`checker.go:11152`). A
        // `let x: T;` that no assignment anywhere targets is reported even from
        // inside a nested function, and that is the whole of what §8's bound
        // gave up.
        //
        // `isNeverInitialized` (`checker.go:11147`) is a `VariableDeclaration`,
        // not a `for-in`/`for-of` head, with no initializer and no `!` — all
        // four already established above — that
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
            let is_never_initialized = self.is_mutable_local_variable_declaration(declaration)
                && !self.is_symbol_assigned_definitely(symbol);
            if !is_never_initialized {
                return;
            }
        }
        // The annotation is read directly rather than through
        // `get_type_of_symbol`, because a union of *named* types declares
        // `errorType` on the printing road and this rule prints no type —
        // `checker-notes-diag2.md` §76, which is §42.1 one level up. Every
        // other annotation shape answers identically through both.
        let declared = match variable.r#type {
            Some(annotation) => self.get_type_from_type_node_unprinted(annotation),
            None => self.get_type_of_symbol(symbol),
        };
        if declared == self.intrinsics.error
            || declared == self.intrinsics.any
            || declared == self.intrinsics.unknown
            || declared == self.intrinsics.void
            || self.contains_undefined_type(declared)
        {
            return;
        }
        let initial = self.get_optional_type_unprinted(declared);
        let flow = self.get_flow_type_of_reference_ex(node, Some(symbol), declared, Some(initial));
        if flow == self.intrinsics.error || !self.contains_undefined_type(flow) {
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

    /// Is this reference in a position a *condition naming the same identifier*
    /// dominates?
    ///
    /// A syntactic over-approximation of "upstream narrowed this before the
    /// check", and deliberately one: every narrowing this port does not model —
    /// user-defined type predicates, `instanceof` on an interface,
    /// discriminated switches — removes `undefined` upstream and leaves it here,
    /// and each of them is written as a guard. Declining costs a *missing*
    /// diagnostic, which is the direction this rule may fail in.
    ///
    /// The three guard shapes, all of which put the reference in a subtree the
    /// condition dominates:
    ///
    /// - the right operand of `&&`, `||` or `??` whose left mentions the name;
    /// - the then/else branch of a conditional expression;
    /// - the body of an `if`, `while` or `do` whose condition mentions it.
    fn reference_is_guarded_by_a_condition_on(&self, node: NodeId, text: &str) -> bool {
        let mut child = node;
        let mut at = self.nodes.parent(node);
        let mut depth = 0u32;
        while let Some(current) = at {
            depth += 1;
            if depth > 64 {
                return false;
            }
            let condition = match self.node_map.get(current) {
                Some(Node::BinaryExpression(binary))
                    if matches!(
                        binary.operator_token.map(|token| token.kind),
                        Some(
                            SyntaxKind::AmpersandAmpersandToken
                                | SyntaxKind::BarBarToken
                                | SyntaxKind::QuestionQuestionToken
                        )
                    ) && binary.right.and_then(|right| right.node_id()) == Some(child) =>
                {
                    binary.left.and_then(|left| left.node_id())
                }
                Some(Node::ConditionalExpression(conditional))
                    if conditional.condition.and_then(|c| c.node_id()) != Some(child) =>
                {
                    conditional.condition.and_then(|c| c.node_id())
                }
                Some(Node::IfStatement(statement))
                    if statement.expression.and_then(|e| e.node_id()) != Some(child) =>
                {
                    statement.expression.and_then(|e| e.node_id())
                }
                Some(Node::WhileStatement(statement))
                    if statement.expression.and_then(|e| e.node_id()) != Some(child) =>
                {
                    statement.expression.and_then(|e| e.node_id())
                }
                Some(Node::DoStatement(statement))
                    if statement.expression.and_then(|e| e.node_id()) != Some(child) =>
                {
                    statement.expression.and_then(|e| e.node_id())
                }
                _ => None,
            };
            // The condition must both name the reference **and** contain one of
            // the narrowing mechanisms this port does not model — a call
            // (a user-defined type predicate), an `instanceof`, or a
            // `.constructor === C` comparison. Requiring the mechanism as well
            // as the name is what keeps the guard from declining a TS2454
            // upstream really does report.
            if let Some(condition) = condition
                && self.subtree_mentions(condition, text, 0)
                && self.subtree_has_unported_narrowing(condition, 0)
            {
                return true;
            }
            child = current;
            at = self.nodes.parent(current);
        }
        false
    }

    /// Does the subtree contain one of the narrowing mechanisms `crate::flow`
    /// does not model — a call, an `instanceof`, or a `.constructor`
    /// comparison?
    ///
    /// The list names *mechanisms* rather than a syntax and is meant to grow:
    /// `narrowTypeByConstructor` was the third
    /// (`checker-notes-diag2.md` §58), and it is the whole of TS2454's share of
    /// `extraonly.rs`'s single-false-positive cases.
    fn subtree_has_unported_narrowing(&self, node: NodeId, depth: u32) -> bool {
        if depth > 32 {
            return false;
        }
        let Some(typed) = self.node_map.get(node) else { return false };
        if matches!(typed, Node::CallExpression(_))
            || matches!(typed, Node::BinaryExpression(binary)
                if binary.operator_token.is_some_and(|t| t.kind == SyntaxKind::InstanceOfKeyword))
            || matches!(typed, Node::PropertyAccessExpression(access)
                if matches!(access.name, Some(tsr_ast::MemberName::Identifier(name))
                    if name.text == "constructor"))
        {
            return true;
        }
        let mut children = Vec::new();
        tsr_ast::for_each_child_id(typed, |child| children.push(child));
        children.into_iter().any(|child| self.subtree_has_unported_narrowing(child, depth + 1))
    }

    /// Does the subtree rooted at `node` contain an identifier spelled `text`?
    pub(crate) fn subtree_mentions(&self, node: NodeId, text: &str, depth: u32) -> bool {
        if depth > 32 {
            return false;
        }
        let Some(typed) = self.node_map.get(node) else { return false };
        if matches!(typed, Node::Identifier(identifier) if identifier.text == text) {
            return true;
        }
        let mut children = Vec::new();
        tsr_ast::for_each_child_id(typed, |child| children.push(child));
        children.into_iter().any(|child| self.subtree_mentions(child, text, depth + 1))
    }

    /// TS2428 — `All declarations of '{0}' must have identical type
    /// parameters.`
    ///
    /// `checkTypeParameterListsIdentical` (`checker.go:4416`): a symbol with
    /// more than one class-or-interface declaration whose type parameter lists
    /// disagree is reported **on every one of them**, which is why this fires
    /// at each declaration the walk visits rather than once at the symbol.
    ///
    /// `areTypeParametersIdentical` compares count, names, constraints and
    /// defaults. **Only count and name are ported**: a constraint comparison
    /// needs the declared types, and `nonIdenticalTypeConstraints` is the case
    /// that wants it — a miss, never a wrong line. §140.
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
        let names = |declaration: NodeId| -> Vec<String> {
            self.node_map.get(declaration).map_or_else(Vec::new, |typed| {
                type_parameters_of(typed)
                    .iter()
                    .map(|parameter| {
                        parameter.name.map_or_else(String::new, |name| name.text.to_string())
                    })
                    .collect()
            })
        };
        let first = names(declarations[0]);
        if declarations[1..].iter().all(|&other| names(other) == first) {
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
                [String::new()],
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
    fn check_modifier_order(&mut self, node: NodeId, modifiers: &[ModifierLike<'_>]) {
        if self.file_has_parse_errors {
            return;
        }
        let mut seen: Vec<SyntaxKind> = Vec::new();
        for modifier in modifiers {
            let ModifierLike::Token(token) = modifier else { continue };
            let kind = token.kind;
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
            if ((kind == SyntaxKind::ExportKeyword && parent_is_class_like)
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
            if kind == SyntaxKind::DeclareKeyword
                && self.file_is_ambient
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
    }

    /// `grammarErrorOnNode(modifier, …)` — every arm of
    /// `checkGrammarModifiers` reports on the modifier token itself.
    fn report_modifier_error(
        &mut self,
        token: &tsr_ast::Token<'_>,
        message: &'static tsr_diagnostics::Message,
        args: &[String],
    ) {
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
        if !self.nodes.flags(list).intersects(tsr_ast::NodeFlags::CONST) {
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
    fn check_generator_in_ambient_context(
        &mut self,
        asterisk: Option<&tsr_ast::Token<'_>>,
        ambient: bool,
    ) {
        if !ambient || self.file_has_parse_errors {
            return;
        }
        let Some(token) = asterisk else { return };
        let Some(id) = token.node_id else { return };
        let Some(file) = self.source_file_of_for_diagnostics(id) else { return };
        let span = self.nodes.span(id);
        self.report(
            file,
            Diagnostic::new(&messages::GENERATORS_ARE_NOT_ALLOWED_IN_AN_AMBIENT_CONTEXT, span),
        );
    }

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
        if self.file_has_parse_errors {
            return;
        }
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
            Node::ImportClause(n) => (n.name, &messages::IMPORT_NAME_CANNOT_BE_0),
            Node::NamespaceImport(n) => (n.name, &messages::IMPORT_NAME_CANNOT_BE_0),
            Node::ImportSpecifier(n) => (n.name, &messages::IMPORT_NAME_CANNOT_BE_0),
            _ => return,
        };
        let Some(name) = name else { return };
        self.check_type_name_is_reserved(name.node_id, name.text, message);
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
    /// `isInitializerSimpleLiteralEnumReference` is **not** ported: it resolves
    /// the reference to a literal enum member, and without it a
    /// `declare const x = E.A` takes the invalid-initialiser branch. A *wrong
    /// line* rather than a missing one, so the enum-reference shape declines
    /// instead — see the `QualifiedName`/`PropertyAccess` arm below. §259.
    fn check_ambient_initializer(
        &mut self,
        node: NodeId,
        initializer: Option<tsr_ast::Expression<'_>>,
        annotation: Option<tsr_ast::TypeNode<'_>>,
        ambient: bool,
    ) {
        if !ambient || self.file_has_parse_errors {
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
            // A reference — `E.A` — needs `isInitializerSimpleLiteralEnumReference`
            // to judge, which is not ported. Declining is a missing line; the
            // alternative is a wrong one.
            if matches!(
                initializer,
                tsr_ast::Expression::PropertyAccessExpression(_)
                    | tsr_ast::Expression::Identifier(_)
            ) {
                return;
            }
            if !is_simple_literal_initializer(initializer) {
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
    fn declaration_is_readonly(&self, node: NodeId) -> bool {
        let modifiers = match self.node_map.get(node) {
            Some(Node::PropertyDeclaration(property)) => property.modifiers,
            _ => return false,
        };
        modifiers.iter().any(|modifier| {
            matches!(modifier, tsr_ast::ModifierLike::Token(m) if m.kind == SyntaxKind::ReadonlyKeyword)
        })
    }

    /// TS2403 — `Subsequent variable declarations must have the same type.`
    ///
    /// `checkVariableLikeDeclaration`'s secondary-declaration arm
    /// (`checker.go:5928`), whose test is `!c.isTypeIdenticalTo(t,
    /// declarationType)` — the **identity** relation, a third beside
    /// assignability and comparability, and one this port does not have.
    ///
    /// # The decidable fragment
    ///
    /// Identity cannot be approximated by [`crate::types::TypeId`] equality:
    /// two structurally identical types with different ids would compare
    /// unequal and this would report where upstream is silent — an error in the
    /// *reporting* direction.
    ///
    /// It is decidable for the **intrinsic primitives**, which are singletons
    /// in [`crate::intrinsics::Intrinsics`]. Between two of them `a != b` is
    /// identity-false with no interning assumption at all, so the rule answers
    /// only where it is certain and declines everywhere else. §257.
    fn check_subsequent_declaration_type(
        &mut self,
        node: NodeId,
        declaration: &tsr_ast::VariableDeclaration<'_>,
    ) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(name) = declaration.name.as_ref().and_then(tsr_ast::BindingName::node_id) else {
            return;
        };
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        let symbol = self.binder.merged_symbol(symbol);
        // `symbol.ValueDeclaration` is the primary; this arm is only for the
        // ones after it.
        let Some(primary) = self.binder.symbols().get(symbol).value_declaration else { return };
        if primary == node {
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
        let first = self.get_widened_type_for_variable_like_declaration(primary);
        let next = self.get_widened_type_for_variable_like_declaration(node);
        if first == next
            || !self.is_decidable_primitive(first)
            || !self.is_decidable_primitive(next)
        {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(name) else { return };
        let span = self.error_span(name);
        let (Some(text), Some(next_text)) =
            (self.type_to_string_at(first, node), self.type_to_string_at(next, node))
        else {
            return;
        };
        let Some(Node::Identifier(identifier)) = self.node_map.get(name) else { return };
        self.report(
            file,
            Diagnostic::with_args(
                &messages::SUBSEQUENT_VARIABLE_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_VARIABLE_0_MUST_BE_OF_TYPE_1_BUT_HERE_HAS_TYPE_2,
                span,
                [identifier.text.to_string(), text, next_text],
            ),
        );
    }

    /// An intrinsic primitive: a singleton id, so inequality *is* non-identity.
    fn is_decidable_primitive(&self, id: crate::types::TypeId) -> bool {
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
        let Some(Node::ClassDeclaration(base_class)) = self.node_map.get(declaration) else {
            return;
        };
        let _ = node;
        // **A `get`/`set` pair is one symbol upstream and two declarations
        // here.** `accessorsOverrideProperty` wants one TS2611 per name and
        // this reported one per accessor — six wrong lines, every one the
        // second half of a pair. §310.
        let mut reported: Vec<&str> = Vec::new();
        for member in members {
            let Some((derived_name, derived_kind, derived_at)) = class_member_shape(*member) else {
                continue;
            };
            let Some((_, base_kind, _)) = base_class
                .members
                .iter()
                .filter_map(|it| class_member_shape(*it))
                .find(|(seen, _, _)| *seen == derived_name)
            else {
                continue;
            };
            let message = match (base_kind, derived_kind) {
                (MemberKind::Property, MemberKind::Accessor) => {
                    &messages::_0_IS_DEFINED_AS_A_PROPERTY_IN_CLASS_1_BUT_IS_OVERRIDDEN_HERE_IN_2_AS_AN_ACCESSOR
                }
                (MemberKind::Accessor, MemberKind::Property) => {
                    &messages::_0_IS_DEFINED_AS_AN_ACCESSOR_IN_CLASS_1_BUT_IS_OVERRIDDEN_HERE_IN_2_AS_AN_INSTANCE_PROPERTY
                }
                _ => continue,
            };
            if reported.contains(&derived_name) {
                continue;
            }
            reported.push(derived_name);
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

    /// TS2335 — `'super' can only be referenced in a derived class.`
    ///
    /// `checkSuperExpression`'s extends test (`checker.go:7922`). §226 refused
    /// this because four arms report *other* `super` messages before it; every
    /// one of those is a node-kind list or an ancestor walk, so their conditions
    /// are evaluated here and their messages declined — §228's shape. §313.
    fn check_super_in_derived_class(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        if self.nodes.kind(node) != SyntaxKind::SuperKeyword {
            return;
        }
        let Some(container) =
            self.nodes.ancestors(node).find(|&it| self.is_function_like_or_static_block(it))
        else {
            // `container == nil` — an earlier arm's message.
            return;
        };
        // **A super _call_ is legal only in a constructor.**
        // `isLegalUsageOfSuperExpression` has two lists and picks by
        // `isCallExpression` (`checker.go:7880`, `:7882`); a `super()` in a
        // method is `Super_calls_are_not_permitted_outside_constructors`
        // (TS2337), not this code. `errorSuperCalls` was seven wrong lines
        // without the split. §314.
        let is_call =
            self.nodes.parent(node).and_then(|parent| self.node_map.get(parent)).is_some_and(
                |typed| {
                    matches!(typed, Node::CallExpression(call)
                    if call.expression.and_then(|e| e.node_id()) == Some(node))
                },
            );
        if is_call {
            if self.nodes.kind(container) != SyntaxKind::Constructor {
                return;
            }
        } else if !matches!(
            self.nodes.kind(container),
            SyntaxKind::MethodDeclaration
                | SyntaxKind::MethodSignature
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor
                | SyntaxKind::PropertyDeclaration
                | SyntaxKind::PropertySignature
                | SyntaxKind::Constructor
                | SyntaxKind::ClassStaticBlockDeclaration
        ) {
            return;
        }
        // `super` inside a computed property name is TS2466, and the walk stops
        // at the container exactly as upstream's `FindAncestorOrQuit` does.
        if self
            .nodes
            .ancestors(node)
            .take_while(|&it| it != container)
            .any(|it| self.nodes.kind(it) == SyntaxKind::ComputedPropertyName)
        {
            return;
        }
        let Some(parent) = self.nodes.parent(container) else { return };
        // An object-literal method's `super` is `any` upstream, with no error.
        let clauses = match self.node_map.get(parent) {
            Some(Node::ClassDeclaration(class)) => class.heritage_clauses,
            Some(Node::ClassExpression(class)) => class.heritage_clauses,
            _ => return,
        };
        if clauses.iter().any(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword) {
            return;
        }
        self.report_grammar_at(
            Some(node),
            &messages::SUPER_CAN_ONLY_BE_REFERENCED_IN_A_DERIVED_CLASS,
        );
    }

    /// TS17009 — `'super' must be called before accessing 'this' in the
    /// constructor of a derived class.`
    ///
    /// `checkThisBeforeSuper` (`checker.go:12263`), whose real test is
    /// `!isPostSuperFlowNode(...)` — flow analysis, not ported. Two shapes are
    /// decidable without it and both are sound in the reporting direction: a
    /// constructor with **no `super()` at all**, and a `this` at the body's
    /// **statement level** in a statement strictly before the one containing
    /// `super()`. A `this` inside a nested function-like declines, because an
    /// arrow captures the constructor's `this` and upstream decides it by where
    /// the arrow *runs*. §307.
    fn check_this_before_super(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        if self.nodes.kind(node) != SyntaxKind::ThisKeyword {
            return;
        }
        // The nearest function-like must be the constructor itself; anything
        // nested captures and is upstream's flow question.
        let Some(container) =
            self.nodes.ancestors(node).find(|&it| self.is_function_like_or_static_block(it))
        else {
            return;
        };
        if self.nodes.kind(container) != SyntaxKind::Constructor {
            return;
        }
        let Some(class) = self.nodes.parent(container) else { return };
        let extends = match self.node_map.get(class) {
            Some(Node::ClassDeclaration(n)) => n.heritage_clauses,
            Some(Node::ClassExpression(n)) => n.heritage_clauses,
            _ => return,
        }
        .iter()
        .find(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword);
        let Some(extends) = extends else { return };
        // `classDeclarationExtendsNull` — `extends null` has no `super` to
        // call, and upstream reports TS17005 there instead.
        //
        // **This parser makes the `null` an `Identifier`, not a
        // `NullKeyword`** — probed directly, because testing the keyword kind
        // matched nothing and `superCallBeforeThisAccessing4` was two of three
        // wrong lines. `null` is a reserved word, so an identifier carrying that
        // text can only be the literal. §308.
        if extends.types.iter().any(|it| {
            matches!(it.expression, Some(tsr_ast::Expression::Identifier(name)) if name.text == "null")
        }) {
            return;
        }
        let Some(Node::ConstructorDeclaration(constructor)) = self.node_map.get(container) else {
            return;
        };
        let Some(body) = constructor.body.and_then(|body| body.node_id()) else { return };
        let Some(Node::Block(block)) = self.node_map.get(body) else { return };
        // Which top-level statement holds `super()`, and which holds this
        // `this`? Both are indices into the same list, so no branch can reorder
        // them.
        let mut super_at = None;
        let mut this_at = None;
        for (index, statement) in block.statements.iter().enumerate() {
            let Some(id) = statement.node_id() else { continue };
            if super_at.is_none() && self.subtree_calls_super(id) {
                super_at = Some(index);
            }
            if this_at.is_none() && self.nodes.ancestors(node).any(|it| it == id) {
                this_at = Some(index);
            }
        }
        let Some(this_at) = this_at else { return };
        if super_at.is_some_and(|at| at < this_at) {
            return;
        }
        self.report_grammar_at(
            Some(node),
            &messages::SUPER_MUST_BE_CALLED_BEFORE_ACCESSING_THIS_IN_THE_CONSTRUCTOR_OF_A_DERIVED_CLASS,
        );
    }

    /// Does this subtree contain a `super(...)` call, not descending into a
    /// nested function-like?
    fn subtree_calls_super(&self, node: NodeId) -> bool {
        if let Some(Node::CallExpression(call)) = self.node_map.get(node)
            && call
                .expression
                .and_then(|e| e.node_id())
                .is_some_and(|id| self.nodes.kind(id) == SyntaxKind::SuperKeyword)
        {
            return true;
        }
        let mut children = Vec::new();
        if let Some(typed) = self.node_map.get(node) {
            tsr_ast::for_each_child_id(typed, |child| children.push(child));
        }
        children.into_iter().any(|child| {
            !self.is_function_like_or_static_block(child) && self.subtree_calls_super(child)
        })
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
        let Some(Node::VariableDeclaration(declaration)) = self.node_map.get(node) else { return };
        let Some(tsr_ast::BindingName::Identifier(name)) = declaration.name else { return };
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
        let Some(statement) = self.nodes.parent(list) else { return };
        if self.nodes.kind(statement) != SyntaxKind::VariableStatement {
            return;
        }
        let Some(container) = self.nodes.parent(statement) else { return };
        let names_share_scope = match self.nodes.kind(container) {
            SyntaxKind::Block => self
                .nodes
                .parent(container)
                .is_some_and(|owner| self.is_function_like_or_static_block(owner)),
            SyntaxKind::ModuleBlock | SyntaxKind::ModuleDeclaration | SyntaxKind::SourceFile => {
                true
            }
            _ => false,
        };
        if names_share_scope {
            return;
        }
        let text = self.binder.symbols().get(local).name.to_string();
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.nodes.span(node);
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
        let mut seen_optional = false;
        for (index, parameter) in parameters.into_iter().enumerate() {
            let Some(Node::ParameterDeclaration(declaration)) = self.node_map.get(parameter) else {
                continue;
            };
            let name = declaration.name.as_ref().and_then(tsr_ast::BindingName::node_id);
            if let Some(rest) = declaration.dot_dot_dot_token {
                let (at, message) = if index != count - 1 {
                    (rest.node_id, &messages::A_REST_PARAMETER_MUST_BE_LAST_IN_A_PARAMETER_LIST)
                } else if let Some(question) = declaration.question_token {
                    (question.node_id, &messages::A_REST_PARAMETER_CANNOT_BE_OPTIONAL)
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
            if declaration.question_token.is_some() {
                seen_optional = true;
                // TS1015 is §103's, reported once per list from its own arm.
                continue;
            }
            if seen_optional {
                self.report_grammar_at(
                    name,
                    &messages::A_REQUIRED_PARAMETER_CANNOT_FOLLOW_AN_OPTIONAL_PARAMETER,
                );
                return;
            }
        }
    }

    /// `checkGrammarAccessor`'s parameter arms (`grammarchecks.go:1332`,
    /// `:1345`, `:1349`).
    fn check_grammar_accessor(&mut self, node: NodeId, typed: Node<'_>) {
        if self.file_has_parse_errors {
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
        let Some(container) = self
            .nodes
            .ancestors(node)
            .find(|&ancestor| self.is_function_like_or_static_block(ancestor))
        else {
            // No container: `IsInTopLevelContext`, declined above.
            return;
        };
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
    fn is_in_parameter_initializer_before_containing_function(&self, node: NodeId) -> bool {
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
        let mut seen: Vec<&str> = Vec::new();
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
            // computed name, which upstream `continue`s past.
            let Some(text) = self.identifier_text(name_id) else { continue };
            if seen.contains(&text) {
                repeats.push((name_id, text.to_string()));
            } else {
                seen.push(text);
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
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let (target, message, skip_assertions) = match self.node_map.get(node) {
            Some(Node::BinaryExpression(binary))
                if binary.operator_token.is_some_and(|t| t.kind == SyntaxKind::EqualsToken) =>
            {
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
                if matches!(
                    self.nodes.kind(left),
                    SyntaxKind::ObjectLiteralExpression | SyntaxKind::ArrayLiteralExpression
                ) {
                    return;
                }
                (
                    left,
                    &messages::THE_LEFT_HAND_SIDE_OF_AN_ASSIGNMENT_EXPRESSION_MUST_BE_A_VARIABLE_OR_A_PROPERTY_ACCESS,
                    true,
                )
            }
            Some(Node::DeleteExpression(delete)) => {
                let Some(operand) = delete.expression.and_then(|e| e.node_id()) else { return };
                (
                    operand,
                    &messages::THE_OPERAND_OF_A_DELETE_OPERATOR_MUST_BE_A_PROPERTY_REFERENCE,
                    false,
                )
            }
            _ => return,
        };
        let spine = self.skip_reference_spine(target, skip_assertions);
        // `node.Flags&ast.NodeFlagsOptionalChain != 0` is upstream's *second*
        // arm and carries its own code (TS2779). This parser does not set
        // `NodeFlags::OPTIONAL_CHAIN`, so the syntax the flag is derived from
        // stands in for it and the rule declines rather than emitting the
        // wrong code. §181.
        if self.spine_has_optional_chain(target) {
            return;
        }
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
    fn skip_reference_spine(&self, mut node: NodeId, assertions: bool) -> NodeId {
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

    /// Whether the spine contains a `?.`, which is what
    /// `NodeFlags::OPTIONAL_CHAIN` would record if this parser set it.
    fn spine_has_optional_chain(&self, mut node: NodeId) -> bool {
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
        let Some(Node::QualifiedName(qualified)) = self.node_map.get(node) else { return };
        // Only as the `type_name` of a bare type reference — the same slot
        // `check_type_reference_name` claims, so the two are disjoint.
        let Some(parent) = self.nodes.parent(node) else { return };
        let Some(Node::TypeReferenceNode(reference)) = self.node_map.get(parent) else { return };
        if reference.type_name.and_then(|name| name.node_id()) != Some(node) {
            return;
        }
        let Some(left) = qualified.left.and_then(|left| left.node_id()) else { return };
        let Some(right) = qualified.right.and_then(|right| right.node_id) else { return };
        // Two deep: `A.B`, not `A.B.C`. A deeper chain is upstream's
        // `canSuggestTypeof` and type-but-not-namespace arms, which carry
        // TS2749 and TS2713.
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
        if self.binder.symbols().get(namespace).exports.is_empty() {
            return;
        }
        let found = self.binder.symbols().get(namespace).exports.get(member.as_str()).is_some_and(
            |&symbol| {
                self.binder
                    .symbols()
                    .get(symbol)
                    .flags
                    .intersects(SymbolFlags::TYPE | SymbolFlags::NAMESPACE | SymbolFlags::ALIAS)
            },
        );
        if found {
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
                Some(Node::MethodDeclaration(n)) => Some(n.asterisk_token.is_some()),
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
        let Some(decorator) = modifiers.iter().find_map(|modifier| match modifier {
            ModifierLike::Decorator(decorator) => decorator.node_id,
            ModifierLike::Token(_) => None,
        }) else {
            return;
        };
        let Some(file) = self.source_file_of_for_diagnostics(decorator) else { return };
        let span = self.nodes.span(decorator);
        self.report(file, Diagnostic::new(&messages::DECORATORS_ARE_NOT_VALID_HERE, span));
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
    /// function, module block, source file or property declaration.
    pub(crate) fn control_flow_container(&self, node: NodeId) -> Option<NodeId> {
        let mut current = self.nodes.parent(node);
        while let Some(id) = current {
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

    /// Is this identifier the left side of a plain `=`?
    ///
    /// `AssignmentKindDefinite`. A compound assignment (`x += 1`) reads before
    /// it writes and is *not* excluded, which is upstream's split at
    /// `checker.go:11110` (`isInCompoundLikeAssignment`).
    fn is_definite_assignment_target(&self, node: NodeId) -> bool {
        self.nodes.parent(node).is_some_and(|parent| {
            matches!(
                self.node_map.get(parent),
                Some(Node::BinaryExpression(binary))
                    if binary.operator_token.is_some_and(|token| token.kind == SyntaxKind::EqualsToken)
                        && binary.left.and_then(|left| left.node_id()) == Some(node)
            )
        })
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
        if self
            .nodes
            .ancestors(node)
            .any(|ancestor| self.binder.facts(ancestor).contains(NodeFacts::UNREACHABLE))
        {
            return;
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
    fn is_unreachable_run_member(&self, node: NodeId) -> bool {
        if !self.binder.facts(node).contains(NodeFacts::UNREACHABLE)
            || !Self::is_reportable_unreachable_kind(self.nodes.kind(node))
        {
            return false;
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

    /// `IsInstantiatedModule` (`ast/utilities.go:2443`).
    fn is_instantiated_module(&self, node: NodeId) -> bool {
        let Some(typed) = self.node_map.get(node) else { return true };
        match tsr_ast::module_instance_state(typed) {
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
        if self.file_has_parse_errors || self.allow_unreachable_code {
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
    fn is_function_like_or_static_block(&self, node: NodeId) -> bool {
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
            if self.ambient_statement_reported.insert(node) {
                self.report_grammar(
                    node,
                    &messages::AN_IMPLEMENTATION_CANNOT_BE_DECLARED_IN_AMBIENT_CONTEXTS,
                );
                return true;
            }
            return false;
        }
        if matches!(
            self.nodes.kind(parent),
            SyntaxKind::Block | SyntaxKind::ModuleBlock | SyntaxKind::SourceFile
        ) && self.ambient_statement_reported.insert(parent)
        {
            self.report_grammar(node, &messages::STATEMENTS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS);
            return true;
        }
        // "We must be parented by a statement. If so, there's no need to report
        // the error as our parent will have already done it."
        false
    }

    /// `checkFunctionOrConstructorSymbol` (`checker.go:3461`) — the
    /// **implementation-expected** arms only.
    ///
    /// | code | message |
    /// |---|---|
    /// | TS2391 | `Function implementation is missing or not immediately following the declaration.` |
    /// | TS2390 | `Constructor implementation is missing.` |
    /// | TS2392 | `Multiple constructor implementations are not allowed.` |
    /// | TS2393 | `Duplicate function implementation.` |
    /// | TS2389 | `Function implementation name must be '{0}'.` |
    /// | TS2384 | `Overload signatures must all be ambient or non-ambient.` — *not ported* |
    ///
    /// Upstream's worker (`checker.go:3469`) is 240 lines doing five unrelated
    /// jobs: implementation presence, modifier agreement across overloads,
    /// question-token agreement, class/function merging, and an
    /// implementation-versus-overload *relation* check. Only the first is
    /// ported; the rest are their own items and the last needs the relation.
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
    fn check_function_or_constructor_symbol(&mut self, node: NodeId, ambient: bool) {
        if self.file_has_parse_errors {
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
        let declarations: Vec<NodeId> = if let Some(symbol) = self.binder.symbol_of(node) {
            let symbol = self.binder.merged_symbol(symbol);
            if !self.function_symbol_checked.insert(symbol) {
                return;
            }
            self.binder.symbols().get(symbol).declarations.iter().copied().collect()
        } else {
            let siblings = self.constructor_siblings_of(node);
            if siblings.first() != Some(&node) {
                return;
            }
            siblings
        };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        // The single-file bound: anything else and the per-declaration ambient
        // context is unavailable, so nothing is said.
        if declarations
            .iter()
            .any(|&declaration| self.source_file_of_for_diagnostics(declaration) != Some(file))
        {
            return;
        }

        // `hasNonAmbientClass` (`checker.go:3660`): a symbol that merges a class
        // with a function has its own arm upstream — TS2813
        // `Class declaration cannot implement overload list for '{0}'` and
        // TS2814 `Function with bodies can only merge with classes that are
        // ambient` — reached *instead of* the duplicate-implementation report.
        // Neither is ported, so the whole symbol is declined: it was 18 of the
        // 34 wrong lines this rule's second measurement produced, all in the
        // `ClassAndModuleThatMerge…` family.
        if declarations.iter().any(|&declaration| {
            matches!(
                self.nodes.kind(declaration),
                SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
            )
        }) {
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
        if !parents_agree {
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
    }

    /// `reportImplementationExpectedError` (`checker.go:3549`), reduced to its
    /// two terminal messages.
    ///
    /// The subsequent-node scan above them selects TS2389
    /// `Function implementation name must be '{0}'` and the static/instance
    /// overload pair, and needs the *parent's* child order. It is not ported;
    /// the effect is that a case wanting TS2389 gets TS2391 instead, which is a
    /// wrong code — so the scan's guard is reproduced instead: if the next
    /// sibling is adjacent, of the same kind and carries a body, say nothing.
    fn report_implementation_expected(&mut self, node: NodeId, is_constructor: bool) {
        // TS2389 first: an adjacent subsequent declaration of the same kind
        // that **carries a body** and does **not** share this one's name is
        // the implementation, misnamed (`checker.go:3585`). The error node is
        // the *subsequent* declaration's name and the argument is this one's —
        // `checker-notes-diag2.md` §63.
        if let Some((at, expected)) = self.misnamed_implementation(node) {
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
        if self.next_sibling_is_the_implementation(node) {
            return;
        }
        let at = self.declaration_name_of(node).unwrap_or(node);
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

    /// The subsequent declaration's name node and this one's written name,
    /// when the pair is upstream's TS2389 shape: adjacent, same kind, the
    /// subsequent one has a body, and the names differ.
    ///
    /// `None` for every other shape, including the name-matching one — that is
    /// the static/instance arm (TS2387/TS2388), which is its own row and stays
    /// declined. See `checker-notes-diag2.md` §63.
    fn misnamed_implementation(&self, node: NodeId) -> Option<(NodeId, String)> {
        let next = self.next_sibling(node)?;
        if self.nodes.kind(next) != self.nodes.kind(node) || !self.declaration_has_body(next) {
            return None;
        }
        let name = self.declaration_name_of(node)?;
        let subsequent = self.declaration_name_of(next)?;
        let written = self.identifier_text(name)?;
        if self.identifier_text(subsequent) == Some(written) {
            return None;
        }
        Some((subsequent, written.to_string()))
    }

    /// The guard `reportImplementationExpectedError` puts in front of its
    /// terminal messages (`checker.go:3566`): a *subsequent* node that starts
    /// exactly where this one ends, of the same kind, carrying a body, is the
    /// implementation — upstream reports TS2389 there instead, and this port
    /// stays silent rather than report the wrong code.
    fn next_sibling_is_the_implementation(&self, node: NodeId) -> bool {
        let Some(next) = self.next_sibling(node) else { return false };
        if self.nodes.kind(next) != self.nodes.kind(node) {
            return false;
        }
        // Upstream's branch structure at `checker.go:3567`, read exactly: with
        // an adjacent subsequent node of the **same kind**, it reports the
        // static/instance mismatch (TS2387/TS2388) or returns when the names
        // match, and TS2389 `Function implementation name must be '{0}'` when
        // they do not and the subsequent node has a body. In none of those does
        // it reach TS2391. So the decline is exact rather than approximate: the
        // only path that falls through is *different name, no body*.
        let names_match = match (self.declaration_name_of(node), self.declaration_name_of(next)) {
            (Some(left), Some(right)) => self.identifier_text(left) == self.identifier_text(right),
            _ => false,
        };
        names_match || self.declaration_has_body(next)
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
    fn declaration_name_of(&self, node: NodeId) -> Option<NodeId> {
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
    /// Upstream keeps `c.patternAmbientModules`, filled while collecting
    /// globals; this port has no such list, so the question is asked of the
    /// binder's global table. **A name-shape test, and it is deliberately
    /// coarse**: a global whose name contains `*` can only have come from a
    /// pattern module declaration, and the cost of a false `true` is silence on
    /// one case rather than a wrong diagnostic on another.
    fn has_pattern_ambient_module(&self) -> bool {
        self.binder.globals().keys().any(|name| name.contains('*'))
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

/// `core.NodeCoreModules()` (`internal/core/nodemodules.go`).
///
/// Upstream substitutes a *different* message for these
/// (`getCannotResolveModuleNameErrorForSpecificModule`, `checker.go:15109`), so
/// the list is a **refusal list** here rather than a resolution one: a name on
/// it is never reported as TS2307. Transcribed from upstream rather than
/// guessed, because a name missing from it becomes a false positive and a name
/// wrongly on it costs only silence.
fn is_node_core_module(name: &str) -> bool {
    let bare = name.strip_prefix("node:").unwrap_or(name);
    NODE_CORE_MODULES.contains(&bare) || name.starts_with("node:")
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

fn modifiers_of(typed: Node<'_>) -> Option<&[tsr_ast::ModifierLike<'_>]> {
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
        _ => return None,
    })
}

fn is_simple_literal_initializer(initializer: tsr_ast::Expression<'_>) -> bool {
    match initializer {
        tsr_ast::Expression::StringLiteral(_)
        | tsr_ast::Expression::NumericLiteral(_)
        | tsr_ast::Expression::BigIntLiteral(_)
        | tsr_ast::Expression::NoSubstitutionTemplateLiteral(_) => true,
        // `-1` is `isInitializerStringOrNumberLiteralExpression`'s second arm:
        // a prefix minus over a numeric literal, and nothing else.
        tsr_ast::Expression::PrefixUnaryExpression(unary) => {
            unary.operator.kind == SyntaxKind::MinusToken
                && matches!(unary.operand, Some(tsr_ast::Expression::NumericLiteral(_)))
        }
        _ => false,
    }
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

fn suggested_lib_for(name: &str) -> Option<&'static str> {
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

/// The operators [`crate::nullable_operand`] checks, named here so the walk's
/// guard and the rule agree.
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
            | SyntaxKind::LessThanToken
            | SyntaxKind::GreaterThanToken
            | SyntaxKind::LessThanEqualsToken
            | SyntaxKind::GreaterThanEqualsToken
            | SyntaxKind::MinusEqualsToken
            | SyntaxKind::AsteriskEqualsToken
            | SyntaxKind::SlashEqualsToken
            | SyntaxKind::PercentEqualsToken
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
            if distance < best_distance {
                best_distance = distance;
            }
            best = Some(candidate);
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
