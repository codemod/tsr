//! TS2322 — `Type '{0}' is not assignable to type '{1}'.`
//!
//! [ADR-0040](../../../docs/adr/0040-diagnostics-come-from-a-check-traversal-and-assignability-gets-a-reporting-twin.md)
//! decision (3), and the largest single row the `diagnostics` board has ever
//! carried: 544 cases blocked on this code and nothing else. See
//! `docs/architecture/checker-notes-diag2.md` §16 for the bar.
//!
//! # The message is not part of the comparison
//!
//! `diagnostics` compares the multiset of `(file, line, column, code)`. TS2322's
//! two type arguments — the whole of `Type 'X' is not assignable to type 'Y'` —
//! are **not compared**, which detaches this rule from the entire type-printing
//! subsystem. What must be right is the *position* and the *predicate*, and only
//! those.
//!
//! # Why a wrong answer here is worse than elsewhere
//!
//! Every other rule in [`crate::check`] reports on a syntactic fact or on a
//! failure to resolve. This one reports on a **relation**, and this port's
//! relation is incomplete: `checker_types` reads a 74% gradient, so roughly a
//! quarter of the types the relation is handed are not the types upstream would
//! hand it. An incomplete relation answers "not assignable" where upstream
//! answers "assignable", and that is a *wrong diagnostic* — which fails its own
//! case and can break one that passes.
//!
//! So the rule is written as a set of **declines**, each naming the situation
//! where this port's answer cannot be trusted, and the measurement in §16 is
//! what decides whether the declines are drawn in the right place.

use tsr_ast::{BinaryExpression, Node, NodeId, SyntaxKind};
use tsr_binder::{SymbolFlags, SymbolId};
use tsr_diagnostics::{Diagnostic, messages};

use crate::{
    checker::Checker,
    flags::TypeFlags,
    types::{TypeData, TypeId},
};

/// [`Checker::missing_property_chain`]'s answer: `reportRelationError`
/// (`relater.go:4816`) suppresses its head for the missing-property message,
/// or keeps it with that message (when one could be named) as the chain child.
enum MissingPropertyHead {
    Suppressed,
    Kept(Option<Diagnostic>),
}

/// Verdicts recorded by §172's probe, in the order
/// `report_assignability_failure` tests them.
pub const PROBE_REPORTED: u8 = 0;
/// Declined: an object literal against a union target — TS2353/TS2561/TS2739's
/// machinery, not this site's.
pub const PROBE_OBJECT_LITERAL_UNION: u8 = 1;
/// Declined by `pair_is_reportable` — one side is a type this port will not
/// speak about.
pub const PROBE_PAIR_NOT_REPORTABLE: u8 = 2;
/// Declined because the three-valued relation did not answer `NotRelated`.
/// **This is the `checker_types` bucket**: `Unknown` means the relation could
/// not decide, which is the members-table gap.
pub const PROBE_RELATION_DECLINED: u8 = 3;

fn assign_probe_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("TSR_ASSIGN_PROBE").is_ok())
}

/// [`Checker::excess_properties_verdict`]'s answer: what `hasExcessProperties`
/// (`relater.go:2714`) would do for a fresh object literal.
enum ExcessProperties {
    /// Silent: `isRelatedTo` goes on to the structural relation.
    None,
    /// `isKnownProperty` failed for the member named `name` at `at`; the
    /// error target is `filterType(reducedTarget, isExcessPropertyCheckTarget)`.
    Excess { at: NodeId, name: String, error_target: TypeId },
    /// A known member's type does not relate to the discriminated target's
    /// member types (the `checkTypes` arm): the relation is false and the
    /// error is the outer one.
    Incompatible,
}

/// [`Checker::union_array_literal_target_element`]'s memo of
/// `getBestMatchingType`, asked at most once per array literal.
#[derive(Clone, Copy)]
enum BestMatch {
    Unasked,
    /// Upstream's answer: a constituent, or nil.
    Chosen(Option<TypeId>),
}

/// [`Checker::union_object_literal_failure`]'s answer.
enum UnionLiteralFailure {
    /// The check ends here; whether it reported.
    Settled(bool),
    /// The relation failed and nothing finer was reported: the caller reports
    /// the whole expression. `excess` is a failure only `hasExcessProperties`
    /// saw, which the relater's verdict must not override.
    Outer { excess: bool },
}

impl<'a> Checker<'a, '_> {
    /// `checkAssignmentOperator` (`checker.go:12757`), reached from
    /// `checkBinaryLikeExpressionWorker` for `=`, `+=`, `&&=`, `||=`, `??=`
    /// and (behind `leftOk && rightOk`) the arithmetic compound forms.
    ///
    /// The error node is the **left operand**, not the whole expression:
    /// `aliasAssignments_1.ts(3,1)` for `x = 1` puts the caret on `x`. The
    /// source is the right operand's type for `=` and the logical forms, and
    /// the operation's result type (`resultType`) for `+=` and the arithmetic
    /// forms — `x += ''` relates `string` to `x`.
    pub(crate) fn check_assignment_operator(
        &mut self,
        node: NodeId,
        binary: &BinaryExpression<'_>,
        ambient: bool,
    ) {
        if ambient {
            return;
        }
        let (Some(left), Some(right), Some(operator)) =
            (binary.left, binary.right, binary.operator_token)
        else {
            return;
        };
        let Some(left_id) = left.node_id() else { return };
        // `checkBinaryLikeExpression` (`checker.go:12338`) short-circuits a
        // destructuring `=` to `checkDestructuringAssignment`, which relates
        // per element and never reaches this site.
        if operator.kind == SyntaxKind::EqualsToken
            && matches!(
                self.nodes.kind(left_id),
                SyntaxKind::ObjectLiteralExpression | SyntaxKind::ArrayLiteralExpression
            )
        {
            return;
        }
        // `checkReferenceExpression` (`checker.go:13130`): assignability is
        // checked only when the left-hand side is a reference.
        if !self.is_assignable_reference(left_id) {
            return;
        }
        let Some(mut target) = self.assignment_target_type(left_id) else { return };
        // "getters can be a subtype of setters, so to check for assignability
        // we use the setter's type instead" (`checker.go:12765`): a compound
        // write through a property access reads `checkPropertyAccessExpression`
        // with `writeOnly`, whose divergent-accessor answer is the setter's
        // parameter type (`getWriteTypeOfAccessors`).
        if operator.kind != SyntaxKind::EqualsToken
            && let tsr_ast::Expression::PropertyAccessExpression(access) = left
            && let Some(tsr_ast::MemberName::Identifier(name)) = access.name
            && let Some(receiver) = access.expression
        {
            let receiver = self.check_expression(receiver);
            let receiver = self.check_non_null_type(receiver);
            if let Some(written) = self.write_type_of_property_of_type(receiver, name.text) {
                target = written;
            }
        }
        let source = match operator.kind {
            SyntaxKind::EqualsToken
            | SyntaxKind::AmpersandAmpersandEqualsToken
            | SyntaxKind::BarBarEqualsToken
            | SyntaxKind::QuestionQuestionEqualsToken => self.check_expression(right),
            // `resultType`: `checkBinaryLikeExpressionWorker`'s `+` and
            // arithmetic arms (`checker.go:12401`, `:12458`). An operator
            // error (`errorType`) returns before the call upstream.
            _ => {
                // `+=`'s `checkForDisallowedESSymbolOperand` (`checker.go:12444`):
                // a symbol-like operand reports TS2469 and returns the
                // result type before `checkAssignmentOperator`.
                if operator.kind == SyntaxKind::PlusEqualsToken {
                    let left_type = self.check_expression(left);
                    let right_type = self.check_expression(right);
                    let symbol_like = crate::flags::TypeFlags::ES_SYMBOL_LIKE;
                    if self.maybe_type_of_kind_considering_base_constraint(left_type, symbol_like)
                        || self
                            .maybe_type_of_kind_considering_base_constraint(right_type, symbol_like)
                    {
                        return;
                    }
                }
                let result = self.check_expression_at_node(node);
                if result == self.intrinsics().error {
                    return;
                }
                result
            }
        };
        // checkAssignmentOperator (native 5b1047d1 checker.go:12760) ignores
        // undefined writes to named CommonJS exports with multiple declarations.
        // Unlike inference's first-initializer rule, this applies to later
        // undefined writes too. Resolve the actual receiver's property, since a
        // shadowed `exports` may not name the binder's file-module declaration.
        if self.is_commonjs_export_property_assignment(binary)
            && self.type_of(source).flags.intersects(TypeFlags::UNDEFINED)
            && let tsr_ast::Expression::PropertyAccessExpression(access) = left
            && let Some(tsr_ast::MemberName::Identifier(name)) = access.name
            && let Some(receiver) = access.expression
        {
            let receiver = self.check_expression(receiver);
            if self
                .get_property_of_type(receiver, name.text)
                .is_some_and(|property| self.binder.symbols().get(property).declarations.len() > 1)
            {
                return;
            }
        }
        let Some(right_id) = right.node_id() else { return };
        // §72's rule: if the **elaboration** spoke, the outer message does not.
        // `checkTypeRelatedToAndOptionallyElaborate` reports an object
        // literal's offending member instead of the outer assignability
        // failure — §73.
        let before = self.diagnostics.len();
        self.check_excess_properties(target, right_id);
        if self.diagnostics.len() != before {
            return;
        }
        self.report_assignability_failure(left_id, right_id, source, target);
    }

    /// `checkInExpression` (`checker.go:13077`): the left operand must be
    /// assignable to `string | number | symbol` (unless it is a private name)
    /// and the right operand to `object`, each through `checkTypeAssignableTo`
    /// with the operand as error node and no expression elaboration. Operand
    /// types are `checkNonNullType`'s; its own nullability diagnostics belong
    /// to that rule, and a refused (error) operand is silent here. When the
    /// right operand does relate, `hasEmptyObjectIntersection` still rejects a
    /// `{}` that may be a primitive (TS2638).
    pub(crate) fn check_in_expression(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.in_js_file(node) {
            return;
        }
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(node) else { return };
        if binary.operator_token.is_none_or(|token| token.kind != SyntaxKind::InKeyword) {
            return;
        }
        let (Some(left), Some(right)) = (
            binary.left.and_then(|left| left.node_id()),
            binary.right.and_then(|right| right.node_id()),
        ) else {
            return;
        };
        let left_type = self.check_expression_at_node(left);
        let right_type = self.check_expression_at_node(right);
        if self.nodes.kind(left) != SyntaxKind::PrivateIdentifier {
            let intrinsics = self.intrinsics();
            let (string, number, symbol) =
                (intrinsics.string, intrinsics.number, intrinsics.es_symbol);
            let target = self.get_union_type(&[string, number, symbol]);
            let source = match binary.left {
                Some(operand) => self.check_non_null_type_reporting(left_type, operand),
                None => self.check_non_null_type(left_type),
            };
            self.report_assignability_failure(left, left, source, target);
        }
        let source = match binary.right {
            Some(operand) => self.check_non_null_type_reporting(right_type, operand),
            None => self.check_non_null_type(right_type),
        };
        let non_primitive = self.intrinsics().non_primitive;
        if self.report_assignability_failure(right, right, source, non_primitive)
            || self.relate_ternary(source, non_primitive, crate::relater::Relation::Assignable)
                != crate::relater::Ternary::Related
            || !self.has_empty_object_intersection(right_type)
        {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(right) else { return };
        let span = self.error_span(right);
        let text = self.type_to_string(right_type);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::TYPE_0_MAY_REPRESENT_A_PRIMITIVE_VALUE_WHICH_IS_NOT_PERMITTED_AS_THE_RIGHT_OPERAND_OF_THE_IN_OPERATOR,
                span,
                [text],
            ),
        );
    }

    /// `hasEmptyObjectIntersection` (`checker.go:13111`).
    fn has_empty_object_intersection(&mut self, t: TypeId) -> bool {
        let parts = match &self.type_of(t).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => vec![t],
        };
        let unknown_empty_object = self.intrinsics().unknown_empty_object;
        parts.into_iter().any(|part| {
            part == unknown_empty_object
                || (self.type_of(part).flags.contains(TypeFlags::INTERSECTION) && {
                    let constraint = self.base_constraint_or_type(part);
                    self.is_empty_anonymous_object_type(constraint)
                })
        })
    }

    /// `checkForOfStatement` (`checker.go:4032`), the reference-expression arm:
    /// `for (v of xs)` relates the iterated element type to `v`'s type through
    /// `checkTypeAssignableToAndOptionallyElaborate`, error node the left
    /// expression. A declaration list is `checkVariableDeclarationList`'s, and
    /// an array/object literal is `checkDestructuringAssignment`'s, so neither
    /// is this position. An unresolved iterated type (native nil) is silent.
    ///
    /// The iterated type is `checkRightHandSideOfForOf`'s
    /// (`for_of_statement_element_type`), async-first for `for await`.
    /// The left-hand type comes from [`Checker::assignment_target_type`], the
    /// same declared-type reader (and declines) the `=` arm uses.
    pub(crate) fn check_for_of_reference_assignment(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.in_js_file(node) {
            return;
        }
        let Some(Node::ForInOrOfStatement(statement)) = self.node_map.get(node) else { return };
        if statement.kind.kind != SyntaxKind::ForOfStatement {
            return;
        }
        let (Some(initializer), Some(expression)) = (statement.initializer, statement.expression)
        else {
            return;
        };
        let (Some(left_id), Some(right_id)) = (initializer.node_id(), expression.node_id()) else {
            return;
        };
        if matches!(
            self.nodes.kind(left_id),
            SyntaxKind::VariableDeclarationList
                | SyntaxKind::ArrayLiteralExpression
                | SyntaxKind::ObjectLiteralExpression
        ) {
            return;
        }
        // `checkReferenceAssignment`'s `checkReferenceExpression` gate.
        if !self.is_assignable_reference(left_id) {
            return;
        }
        let Some(target) = self.assignment_target_type(left_id) else { return };
        let is_async = statement.await_modifier.is_some();
        let Some(source) = self.for_of_statement_element_type(expression, is_async) else {
            return;
        };
        self.report_assignability_failure(left_id, right_id, source, target);
    }

    /// `checkVariableLikeDeclaration`'s `getTypeOfSymbol(symbol)`
    /// (`checker.go:5893`): every variable-like declaration that reaches the
    /// symbol arms resolves its symbol's type, used or not. That is what
    /// makes `var r: typeof r;` report TS2502 with no reference anywhere — the
    /// circularity report lives in the type resolution
    /// (`report_circularity_error`), and resolution is otherwise lazy here.
    ///
    /// Upstream's exits before the call are mirrored: no name, a binding
    /// pattern name, and a `CommonJS` `require` alias. The resolution runs
    /// whether or not the declaration is ambient: upstream has no ambient
    /// exit on this road (`declare global { const foo: typeof foo }` reports).
    ///
    /// **Three declines, all waiting on deferred resolution upstream has and
    /// this port does not** (`docs/parity/notes/r5-vardecl.md` §1):
    ///
    /// - a parameter: upstream's `getTypeOfSymbol` on a parameter never
    ///   resolves its owner's signature, whose members are deferred; this
    ///   port builds a function's type with its parameters, so
    ///   `var i: (x: typeof i) => typeof x` cycles through `x` here
    ///   (`recursiveTypesWithTypeof`). The parameter arm does not call this;
    /// - an unannotated declaration: its type is its initializer's, and this
    ///   port computes a function's return type and parameter types eagerly
    ///   as part of its symbol type, so forcing from the declaration finds
    ///   cycles upstream's `ResolvedReturnType` frame never sees (TS7022,
    ///   TS7023, TS7024 in `cyclicGenericTypeInstantiation`,
    ///   `functionWithDefaultParameterWithNoStatements16`);
    /// - an annotation that writes a type literal, mapped type or function
    ///   type: upstream resolves those members lazily
    ///   (`resolveStructuredTypeMembers`), this port mints them with the
    ///   annotation, so a self-reference through a member is a cycle here
    ///   and not upstream (`recursiveTypesWithTypeof`'s `hy2`,
    ///   `unionTypeWithRecursiveSubtypeReduction3`).
    pub(crate) fn resolve_variable_like_symbol_type(&mut self, node: NodeId) {
        let (name, annotation) = match self.node_map.get(node) {
            Some(Node::VariableDeclaration(declaration)) => (
                declaration.name.as_ref().and_then(tsr_ast::BindingName::node_id),
                declaration.r#type,
            ),
            Some(Node::PropertyDeclaration(declaration)) => {
                (declaration.name.node_id(), declaration.r#type)
            }
            Some(Node::PropertySignatureDeclaration(declaration)) => {
                (declaration.name.node_id(), declaration.r#type)
            }
            _ => return,
        };
        let Some(name) = name else { return };
        if matches!(
            self.nodes.kind(name),
            SyntaxKind::ObjectBindingPattern | SyntaxKind::ArrayBindingPattern
        ) {
            return;
        }
        let Some(annotation) = annotation.and_then(|annotation| annotation.node_id()) else {
            return;
        };
        if self.annotation_writes_deferred_members(annotation) {
            return;
        }
        let Some(own) = self.binder.symbol_of(node) else { return };
        let symbol = self.binder.merged_symbol(own);
        if self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::ALIAS) {
            return;
        }
        self.get_type_of_symbol(symbol);
    }

    /// `checkAccessorDeclaration`'s `getTypeOfAccessors(getSymbolOfDeclaration(node))`
    /// (`checker.go:2974`): every accessor resolves its symbol's type, so a
    /// self-referencing annotation in a type nobody uses still reports TS2502
    /// (`type T2 = { set foo(value: T2["foo"]) }`,
    /// `circularAccessorAnnotations`). The same two declines as
    /// [`Checker::resolve_variable_like_symbol_type`]: the annotation
    /// `getTypeOfAccessors` reads (the getter's, else the setter's
    /// parameter's) must be written and must not write deferred members;
    /// an unannotated getter is inferred from its body, which this port does
    /// eagerly (`report_accessor_circularity` records the same limit).
    pub(crate) fn resolve_accessor_symbol_type(&mut self, node: NodeId) {
        // `getTypeOfAccessors`' implicit-`any` fallback and the set
        // accessor's parameter (r7-reports §3).
        self.check_implicit_any_accessor(node);
        let Some(own) = self.binder.symbol_of(node) else { return };
        let symbol = self.binder.merged_symbol(own);
        let declarations = self.binder.symbols().get(symbol).declarations.clone();
        let of_kind = |kind| {
            declarations.iter().copied().find(|&declaration| self.nodes.kind(declaration) == kind)
        };
        let Some(annotation) = [of_kind(SyntaxKind::GetAccessor), of_kind(SyntaxKind::SetAccessor)]
            .into_iter()
            .flatten()
            .find_map(|accessor| self.accessor_annotation(accessor))
            .and_then(|annotation| annotation.node_id())
        else {
            return;
        };
        if self.annotation_writes_deferred_members(annotation) {
            return;
        }
        self.get_type_of_symbol(symbol);
    }

    /// Does a written annotation contain a type node whose members upstream
    /// resolves lazily — a type literal, a mapped type, a function or
    /// constructor type? [`Checker::resolve_variable_like_symbol_type`]'s
    /// second decline.
    fn annotation_writes_deferred_members(&self, node: NodeId) -> bool {
        if matches!(
            self.nodes.kind(node),
            SyntaxKind::TypeLiteral
                | SyntaxKind::MappedType
                | SyntaxKind::FunctionType
                | SyntaxKind::ConstructorType
        ) {
            return true;
        }
        let mut children = Vec::new();
        if let Some(typed) = self.node_map.get(node) {
            tsr_ast::for_each_child_id(typed, |child| children.push(child));
        }
        children.into_iter().any(|child| self.annotation_writes_deferred_members(child))
    }

    /// `checkVariableLikeDeclaration` (`checker.go:9967`) — the annotation
    /// against the initialiser.
    ///
    /// The error node is the **declaration**, and this port's span for a
    /// `VariableDeclaration` starts at its name, which is what
    /// `arrayAssignmentTest1.ts(46,5)` records for
    /// `var i1_error: I1 = [];` — column 5 is the `i`.
    pub(crate) fn check_variable_like_declaration(
        &mut self,
        node: NodeId,
        declaration: &tsr_ast::VariableDeclaration<'a>,
        ambient: bool,
    ) {
        if ambient {
            return;
        }
        let annotation = declaration.r#type.or_else(|| self.jsdoc_type_annotation(node));
        let (Some(annotation), Some(initializer)) = (annotation, declaration.initializer) else {
            return;
        };
        let target = self.get_type_from_type_node(annotation);
        let source = self.check_expression(initializer);
        let Some(initializer_id) = initializer.node_id() else { return };
        // §73: the elaboration reports the member instead of the outer message.
        let before = self.diagnostics.len();
        self.check_excess_properties(target, initializer_id);
        if self.diagnostics.len() != before {
            return;
        }
        self.report_assignability_failure(node, initializer_id, source, target);
    }

    /// `checkVariableLikeDeclaration`'s other two callers: a **property
    /// declaration** and a **parameter default**.
    ///
    /// Both are the same shape as the variable arm — a written annotation and an
    /// initialiser — and both report at the declaration node. They are separate
    /// only because the node kinds are, and each carries one decline the
    /// variable arm does not need: a property declaration with a `declare` or
    /// `abstract` modifier has no initialiser to check, and an **optional**
    /// parameter's default is compared against the type *without* `undefined`
    /// (`checker.go:9993`), which this port does not strip.
    pub(crate) fn check_annotated_initializer(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.in_js_file(node) {
            return;
        }
        let (annotation, initializer) = match self.node_map.get(node) {
            Some(Node::PropertyDeclaration(property)) => (property.r#type, property.initializer),
            Some(Node::ParameterDeclaration(parameter)) => {
                if parameter.dot_dot_dot_token.is_some() {
                    return;
                }
                (parameter.r#type, parameter.initializer)
            }
            _ => return,
        };
        let Some(initializer) = initializer else { return };
        let target = match annotation {
            Some(annotation) if self.nodes.kind(node) == SyntaxKind::PropertyDeclaration => {
                self.get_type_from_type_node(annotation)
            }
            _ if self.nodes.kind(node) == SyntaxKind::PropertyDeclaration => return,
            // `checkVariableLikeDeclaration` (`checker.go:5893`) relates a
            // parameter default to `getTypeOfSymbol` of the parameter: the
            // annotation, or the contextual parameter type of an
            // unannotated parameter of a contextually typed function.
            _ => {
                let Some(target) = self.parameter_initializer_target(node) else { return };
                target
            }
        };
        let source = self.check_expression(initializer);
        let Some(initializer_id) = initializer.node_id() else { return };
        // §73: the elaboration reports the member instead of the outer message.
        let before = self.diagnostics.len();
        self.check_excess_properties(target, initializer_id);
        if self.diagnostics.len() != before {
            return;
        }
        self.report_assignability_failure(node, initializer_id, source, target);
    }

    /// `getTypeOfSymbol` of a parameter that is its symbol's value
    /// declaration, for its default's relation. An undecided type (a gap,
    /// or the error type of an unresolved contextual parameter) declines.
    ///
    /// An unannotated parameter of a function that cannot be contextually
    /// typed (a declaration, a class method, an accessor or constructor) has
    /// the widened type of its own default, to which the default is always
    /// assignable, so it is not asked: asking would resolve its type earlier
    /// than native does, and this port's signature resolution reports a
    /// circularity (TS7022) native never reaches for
    /// `function foo(a = bar()) {}` beside `function bar(a = foo()) {}`
    /// (`functionWithDefaultParameterWithNoStatements16`).
    fn parameter_initializer_target(&mut self, node: NodeId) -> Option<TypeId> {
        if let Some(Node::ParameterDeclaration(parameter)) = self.node_map.get(node)
            && parameter.r#type.is_none()
        {
            let function = self.nodes.parent(node)?;
            let contextual = match self.nodes.kind(function) {
                SyntaxKind::FunctionExpression | SyntaxKind::ArrowFunction => true,
                SyntaxKind::MethodDeclaration => self
                    .nodes
                    .parent(function)
                    .is_some_and(|p| self.nodes.kind(p) == SyntaxKind::ObjectLiteralExpression),
                _ => false,
            };
            if !contextual {
                return None;
            }
        }
        let symbol = self.binder.symbol_of(node)?;
        if self.binder.symbols().get(symbol).value_declaration != Some(node) {
            return None;
        }
        let target = self.get_type_of_symbol(symbol);
        (!self.is_gap(target) && !self.is_error(target)).then_some(target)
    }

    /// `checkVariableLikeDeclaration` (`checker.go:5790`) on a binding element
    /// with an identifier name and an initializer: the element is its
    /// symbol's value declaration, so the initializer's type is checked with
    /// `checkTypeAssignableToAndOptionallyElaborate` against
    /// `getTypeOfSymbol` — the element's type from its parent
    /// (`getBindingElementTypeFromParentType`, which already folds the
    /// default in when the binding root carries no annotation) — at the
    /// element, elaborating into the initializer.
    ///
    /// A parameter element whose containing function has no body exits
    /// before the check (TS2371 is that arm's report), as does an ambient or
    /// type-node position (`isInAmbientOrTypeNode`). An element named by a
    /// nested pattern takes the binding-pattern arm, which is not this one.
    pub(crate) fn check_binding_element_initializer(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.in_js_file(node) {
            return;
        }
        let Some(Node::BindingElement(element)) = self.node_map.get(node) else { return };
        if !matches!(element.name, Some(tsr_ast::BindingName::Identifier(_))) {
            return;
        }
        let Some(initializer) = element.initializer else { return };
        let root = self.root_declaration_of(node);
        match self.node_map.get(root) {
            Some(Node::VariableDeclaration(_)) => {}
            Some(Node::ParameterDeclaration(_)) => {
                let Some(function) = self.nodes.parent(root) else { return };
                let has_body = match self.node_map.get(function) {
                    Some(Node::FunctionDeclaration(f)) => f.body.is_some(),
                    Some(Node::MethodDeclaration(f)) => f.body.is_some(),
                    Some(Node::ConstructorDeclaration(f)) => f.body.is_some(),
                    Some(Node::GetAccessorDeclaration(f)) => f.body.is_some(),
                    Some(Node::SetAccessorDeclaration(f)) => f.body.is_some(),
                    Some(Node::FunctionExpression(_) | Node::ArrowFunction(_)) => true,
                    _ => false,
                };
                if !has_body {
                    return;
                }
            }
            _ => return,
        }
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        if self.binder.symbols().get(symbol).value_declaration != Some(node) {
            return;
        }
        let Some(initializer_id) = initializer.node_id() else { return };
        // Supplier decline, not an upstream rule: an object, array or class
        // literal is typed under the element's contextual type, and this
        // port's literal checking loses member context through a union with
        // a nullable constituent (`const f: [I?] = [{ tag: "right" }]` widens
        // `tag` to `string`). Such a source is not upstream's source.
        if matches!(
            initializer,
            tsr_ast::Expression::ObjectLiteralExpression(_)
                | tsr_ast::Expression::ArrayLiteralExpression(_)
                | tsr_ast::Expression::ClassExpression(_)
        ) && self.get_contextual_type(initializer_id).is_some_and(|contextual| {
            matches!(&self.type_of(contextual).data, TypeData::Union { types, .. }
                if types.iter().any(|&part| self.type_of(part).flags.intersects(TypeFlags::NULLABLE)))
        }) {
            return;
        }
        // Native computes the adjusted symbol type before checking the source.
        let target = self.get_type_of_symbol(symbol);
        let source = self.check_expression(initializer);
        let before = self.diagnostics.len();
        self.check_excess_properties(target, initializer_id);
        if self.diagnostics.len() != before {
            return;
        }
        self.report_assignability_failure(node, initializer_id, source, target);
    }

    /// `checkYieldExpression` (`checker.go:10952`), the assignability half:
    /// in a generator with a written return annotation (a union filtered by
    /// `checkGeneratorInstantiationAssignabilityToReturnType`), the yielded
    /// type (`getYieldedTypeOfYieldExpression`: the operand's type, or
    /// `undefinedWideningType` for a bare `yield`) is checked with
    /// `checkTypeAssignableToAndOptionallyElaborate` against the annotation's
    /// yield iteration type (`getIterationTypesOfGeneratorFunctionReturnType`,
    /// orElse `anyType`), at the operand or else the `yield` itself.
    ///
    /// For `yield*`, the yielded type is the delegated iterable's iterated
    /// type (`checkIteratedTypeOrElementType` with `IterationUseYieldStar`,
    /// [`Checker::yield_star_operand_types`]: `anyType` for a `never` or
    /// non-iterable operand, whose own errors the iteration check reports);
    /// an undecided protocol declines. The excess-property check is the
    /// plain operand's only: a `yield*` operand's elements are related, not
    /// the written literal.
    ///
    /// An async generator relates the awaited yielded type
    /// ([`Checker::awaited_yielded_type`]) against the annotation's async
    /// yield iteration type; `yield*` iterates with
    /// `IterationUseAsyncYieldStar`.
    pub(crate) fn check_yield_expression_assignability(&mut self, node: NodeId) {
        if self.in_js_file(node) {
            return;
        }
        let Some(Node::YieldExpression(expression)) = self.node_map.get(node) else { return };
        let yield_star = expression.asterisk_token.is_some();
        let Some(container) = self.containing_function(node) else { return };
        let (asterisk, annotation, modifiers) = match self.node_map.get(container) {
            Some(Node::FunctionDeclaration(f)) => (f.asterisk_token, f.r#type, f.modifiers),
            Some(Node::MethodDeclaration(f)) => (f.asterisk_token, f.r#type, f.modifiers),
            Some(Node::FunctionExpression(f)) => (f.asterisk_token, f.r#type, f.modifiers),
            _ => return,
        };
        if asterisk.is_none() {
            return;
        }
        let is_async = has_async(modifiers);
        let Some(annotation) = annotation else { return };
        let mut return_type = self.get_type_from_type_node(annotation);
        if self.type_of(return_type).flags.contains(TypeFlags::UNION) {
            let mut undecided = false;
            return_type = self.filter_type(return_type, |checker, constituent| {
                checker
                    .generator_instantiation_assignable_to_return_type(constituent, is_async)
                    .unwrap_or_else(|()| {
                        undecided = true;
                        false
                    })
            });
            if undecided {
                return;
            }
        }
        let Ok(yield_type) = self.get_iteration_type_of_generator_function_return_type(
            crate::iteration::IterationTypeKind::Yield,
            return_type,
            is_async,
        ) else {
            return;
        };
        let target = yield_type.unwrap_or(self.intrinsics.any);
        if let Some(operand) = expression.expression.and_then(|operand| operand.node_id()) {
            let operand_type = self.check_expression_at_node(operand);
            if yield_star {
                // getYieldedTypeOfYieldExpression (`checker.go:11019`).
                let Some((yielded, _)) = self.yield_star_operand_types(operand_type, is_async)
                else {
                    return;
                };
                let Some(yielded) = self.awaited_yielded_type(yielded, operand, is_async) else {
                    return;
                };
                self.report_assignability_failure(operand, operand, yielded, target);
                return;
            }
            let Some(operand_type) = self.awaited_yielded_type(operand_type, operand, is_async)
            else {
                return;
            };
            let before = self.diagnostics.len();
            self.check_excess_properties(target, operand);
            if self.diagnostics.len() != before {
                return;
            }
            self.report_assignability_failure(operand, operand, operand_type, target);
        } else if !yield_star {
            let undefined = self.intrinsics.undefined;
            self.report_assignability_failure_with(node, None, undefined, target);
        }
    }

    /// The async tail of `getYieldedTypeOfYieldExpression` (`checker.go:11019`):
    /// an async generator yields `getAwaitedTypeEx(yieldedType, errorNode,
    /// TS1322)`, reported at the operand. A sync generator yields the type
    /// unchanged. `None` declines (a gap, or native's `nil`).
    fn awaited_yielded_type(
        &mut self,
        yielded: TypeId,
        operand: NodeId,
        is_async: bool,
    ) -> Option<TypeId> {
        if !is_async {
            return Some(yielded);
        }
        self.check_awaited_type(
            yielded,
            false,
            operand,
            &messages::TYPE_OF_ITERATED_ELEMENTS_OF_A_YIELD_ASTERISK_OPERAND_MUST_EITHER_BE_A_VALID_PROMISE_OR_MUST_NOT_CONTAIN_A_CALLABLE_THEN_MEMBER,
        )
    }

    /// `checkSignatureDeclaration`'s generator arm (`checker.go:2757`): a
    /// generator with a body (`FunctionFlagsInvalid` clear) and a written
    /// return type reports TS2505 at the annotation when it is `void`, else
    /// `checkGeneratorInstantiationAssignabilityToReturnType`
    /// (`checker.go:29697`) with the annotation as error node: `Generator`
    /// (`AsyncGenerator`) instantiated with the annotation's own yield,
    /// return (orElse yield) and next (orElse `unknown`) iteration types must
    /// be assignable to it. The instantiation is the one
    /// `generator_instantiation_assignable_to_return_type` (`iteration.rs`)
    /// relates without an error node; an undecided iteration query declines.
    /// No cache or side table.
    pub(crate) fn check_generator_return_annotation(&mut self, node: NodeId) {
        if self.in_js_file(node) {
            return;
        }
        let (asterisk, annotation, modifiers, has_body) = match self.node_map.get(node) {
            Some(Node::FunctionDeclaration(f)) => {
                (f.asterisk_token, f.r#type, f.modifiers, f.body.is_some())
            }
            Some(Node::MethodDeclaration(f)) => {
                (f.asterisk_token, f.r#type, f.modifiers, f.body.is_some())
            }
            Some(Node::FunctionExpression(f)) => {
                (f.asterisk_token, f.r#type, f.modifiers, f.body.is_some())
            }
            _ => return,
        };
        if asterisk.is_none() || !has_body {
            return;
        }
        let Some(annotation) = annotation else { return };
        let Some(at) = annotation.node_id() else { return };
        let return_type = self.get_type_from_type_node(annotation);
        if self.is_gap(return_type) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        if return_type == self.intrinsics.void {
            self.report(
                file,
                Diagnostic::new(&messages::A_GENERATOR_CANNOT_HAVE_A_VOID_TYPE_ANNOTATION, span),
            );
            return;
        }
        let is_async = has_async(modifiers);
        let slot = |checker: &mut Self, kind| {
            checker.get_iteration_type_of_generator_function_return_type(
                kind,
                return_type,
                is_async,
            )
        };
        let Ok(yield_type) = slot(self, crate::iteration::IterationTypeKind::Yield) else { return };
        let yield_type = yield_type.unwrap_or(self.intrinsics.any);
        let Ok(generator_return) = slot(self, crate::iteration::IterationTypeKind::Return) else {
            return;
        };
        let generator_return = generator_return.unwrap_or(yield_type);
        let Ok(next_type) = slot(self, crate::iteration::IterationTypeKind::Next) else { return };
        let next_type = next_type.unwrap_or(self.intrinsics.unknown);
        let Ok(instantiation) =
            self.create_generator_type(yield_type, generator_return, next_type, is_async)
        else {
            return;
        };
        self.report_relation_failure(at, span, None, instantiation, return_type, None);
    }

    /// `checkObjectLiteral`'s binding-pattern arm (`checker.go:13250`): a
    /// literal whose contextual type is the implied type of an object binding
    /// pattern (`contextualTypeHasPattern`, found by
    /// `contextual_binding_pattern`) reports TS2353 at each property,
    /// shorthand or method member whose symbol name the implied type lacks —
    /// unless the pattern has a computed name that is not a property-name
    /// literal (`ObjectLiteralPatternWithComputedProperties`) or a rest
    /// element (the implied type's string index). The member's name is its
    /// binder name: a computed name that is not a string or numeric literal
    /// is `__computed`, which no implied property has, printed `[k]`. The
    /// type printed is the implied type itself.
    ///
    /// `getContextualTypeForInitializerExpression` (`checker.go:29431`) gives
    /// a declaration's own initializer the implied type only when the pattern
    /// has elements; a nested literal's contextual type is the implied
    /// property's (pattern) type whatever its arity. Run once per literal
    /// from the check walk, as `checkObjectLiteral`'s result is cached. No
    /// cache or side table.
    pub(crate) fn check_object_literal_binding_pattern_members(&mut self, node: NodeId) {
        let Some(Node::ObjectLiteralExpression(literal)) = self.node_map.get(node) else {
            return;
        };
        let Some(pattern) = self.contextual_binding_pattern(node) else {
            self.check_object_literal_assignment_pattern_members(node, literal);
            return;
        };
        let own_initializer = self.nodes.parent(node).is_some_and(|parent| {
            matches!(
                self.nodes.kind(parent),
                SyntaxKind::VariableDeclaration
                    | SyntaxKind::Parameter
                    | SyntaxKind::BindingElement
            )
        });
        if own_initializer && pattern.elements.is_empty() {
            return;
        }
        let mut implied_names = Vec::with_capacity(pattern.elements.len());
        for element in pattern.elements {
            if element.dot_dot_dot_token.is_some() {
                return;
            }
            let name = match element.property_name {
                Some(name) => {
                    let Some(name) = name.node_id() else { return };
                    match self.object_literal_member_name(name) {
                        Ok(Some((text, _))) => text,
                        Ok(None) | Err(()) => return,
                    }
                }
                None => match element.name {
                    Some(tsr_ast::BindingName::Identifier(identifier)) => {
                        identifier.text.to_string()
                    }
                    _ => return,
                },
            };
            implied_names.push(name);
        }
        let mut excess = Vec::new();
        for property in literal.properties {
            let name = match property {
                tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                    assignment.name
                }
                tsr_ast::ObjectLiteralElementLike::ShorthandPropertyAssignment(shorthand) => {
                    shorthand.name
                }
                tsr_ast::ObjectLiteralElementLike::MethodDeclaration(method) => method.name,
                _ => continue,
            };
            let Some(name) = name.node_id() else { return };
            let Some((symbol_name, printed)) = self.object_literal_binder_name(name) else {
                return;
            };
            if symbol_name.as_deref().is_none_or(|text| !implied_names.iter().any(|n| n == text)) {
                excess.push((name, printed));
            }
        }
        if excess.is_empty() {
            return;
        }
        let Some(implied) = self.binding_pattern_implied_type(pattern) else { return };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let printed_type = self.type_to_string(implied);
        for (at, printed) in excess {
            let span = self.error_span(at);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_0_DOES_NOT_EXIST_IN_TYPE_1,
                    span,
                    [printed, printed_type.clone()],
                ),
            );
        }
    }

    /// The assignment half of `contextualTypeHasPattern`: the right of a
    /// destructuring assignment is contextually typed by the left literal's
    /// type, which `checkObjectLiteral` records in `patternForType` for an
    /// assignment target (`checker.go:13212`), whatever its arity. Its
    /// properties are the left members' names (a computed name by its
    /// literal type); a computed name that is not a property-name literal
    /// sets `ObjectLiteralPatternWithComputedProperties`, and a spread makes
    /// the left's type a spread type with no pattern, so either ends the
    /// check. Members are named and printed as in the binding arm.
    fn check_object_literal_assignment_pattern_members(
        &mut self,
        node: NodeId,
        literal: &tsr_ast::ObjectLiteralExpression<'a>,
    ) {
        let Some(pattern) = self.contextual_assignment_pattern(node) else { return };
        let mut pattern_names = Vec::with_capacity(pattern.properties.len());
        for property in pattern.properties {
            let name = match property {
                tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                    assignment.name
                }
                tsr_ast::ObjectLiteralElementLike::ShorthandPropertyAssignment(shorthand) => {
                    shorthand.name
                }
                _ => return,
            };
            let Some(name) = name.node_id() else { return };
            match self.object_literal_member_name(name) {
                Ok(Some((text, _))) => pattern_names.push(text),
                Ok(None) | Err(()) => return,
            }
        }
        let mut excess = Vec::new();
        for property in literal.properties {
            let name = match property {
                tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                    assignment.name
                }
                tsr_ast::ObjectLiteralElementLike::ShorthandPropertyAssignment(shorthand) => {
                    shorthand.name
                }
                tsr_ast::ObjectLiteralElementLike::MethodDeclaration(method) => method.name,
                _ => continue,
            };
            let Some(name) = name.node_id() else { return };
            let Some((symbol_name, printed)) = self.object_literal_binder_name(name) else {
                return;
            };
            if symbol_name.as_deref().is_none_or(|text| !pattern_names.iter().any(|n| n == text)) {
                excess.push((name, printed));
            }
        }
        if excess.is_empty() {
            return;
        }
        let pattern_type =
            self.check_expression(tsr_ast::Expression::ObjectLiteralExpression(pattern));
        if self.is_gap(pattern_type) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let printed_type = self.type_to_string(pattern_type);
        for (at, printed) in excess {
            let span = self.error_span(at);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_0_DOES_NOT_EXIST_IN_TYPE_1,
                    span,
                    [printed, printed_type.clone()],
                ),
            );
        }
    }

    /// An object-literal member's binder name and its `symbolToString`
    /// spelling: an identifier, string or numeric name binds its text (a
    /// non-identifier string prints quoted). A computed name binds the text
    /// of a string, template or numeric literal expression and anything else
    /// binds `__computed` (`None` here); either way it prints as the written
    /// expression in brackets (`["x"]`, `[k]`). `None` when the name cannot
    /// be spelled (no source text, or a prefix operator other than a sign).
    fn object_literal_binder_name(&self, name: NodeId) -> Option<(Option<String>, String)> {
        if let Some(text) = self.written_member_name(name) {
            let printed = if crate::symbols::is_identifier_text(&text)
                || text.parse::<u64>().is_ok_and(|number| number.to_string() == text)
            {
                text.clone()
            } else {
                crate::printing::quote_ascii(&text)
            };
            return Some((Some(text), printed));
        }
        let Some(Node::ComputedPropertyName(computed)) = self.node_map.get(name) else {
            return None;
        };
        let expression = computed.expression?.node_id()?;
        let bound = match self.node_map.get(expression)? {
            Node::StringLiteral(literal) => Some(literal.text.to_string()),
            Node::NoSubstitutionTemplateLiteral(literal) => Some(literal.text.to_string()),
            Node::NumericLiteral(literal) => Some(crate::printing::normalise_number(literal.text)),
            // `ast.IsSignedNumericLiteral`: the operator's text and the number.
            Node::PrefixUnaryExpression(unary) => {
                let sign = match unary.operator.kind {
                    SyntaxKind::MinusToken => "-",
                    SyntaxKind::PlusToken => "+",
                    _ => return None,
                };
                let Some(tsr_ast::Expression::NumericLiteral(number)) = unary.operand else {
                    return None;
                };
                Some(format!("{sign}{}", crate::printing::normalise_number(number.text)))
            }
            _ => None,
        };
        let file = self.source_file_of_for_diagnostics(expression)?;
        let text = self.module_host.and_then(|host| host.source_text(file, self.nodes))?;
        let span = self.nodes.span(expression);
        let written = text.get(span.start as usize..span.end as usize)?.trim();
        Some((bound, format!("[{written}]")))
    }

    /// `checkReturnStatement` (`checker.go:4086`) — the returned expression
    /// against the function's annotated return type
    /// ([`Checker::annotated_return_type`]: written, a JS `@returns`, or a JS
    /// `@type` full signature).
    ///
    /// The error node is the **return statement**, not the expression:
    /// `arrayAssignmentTest1.ts(6,16)` for `IM1():void[] {return null;}` is
    /// column 16, which is the `r` of `return`.
    ///
    /// Only an annotation is used (`getReturnTypeFromAnnotation != nil`). An
    /// inferred return type is computed from the very returns being checked,
    /// so a mismatch against it is not a diagnostic upstream would ever report.
    pub(crate) fn check_return_statement(&mut self, node: NodeId, ambient: bool) {
        if ambient {
            return;
        }
        let Some(Node::ReturnStatement(statement)) = self.node_map.get(node) else { return };
        let expression = statement.expression.and_then(|expression| expression.node_id());
        let Some(container) = self.return_statement_container(node) else { return };
        match self.node_map.get(container) {
            Some(Node::SetAccessorDeclaration(_)) => return,
            Some(Node::ConstructorDeclaration(_)) => {
                // Declined in JS: a class's `@template {C} T` constraint is
                // not read there, so `return a` with `@param {T} a` relates
                // an unconstrained `T` to the instance type and reports a
                // TS2409 native does not (`extendsTag5`).
                if let Some(expression) = expression
                    && !self.in_js_file(node)
                {
                    self.check_constructor_return(container, node, expression);
                }
                return;
            }
            _ => {}
        }
        let Some((return_type, target, is_async)) = self.return_type_from_annotation(container)
        else {
            return;
        };
        if !self.strict_null_checks
            && expression.is_none()
            && !self.type_of(return_type).flags.contains(TypeFlags::NEVER)
        {
            return;
        }
        if let Some(expression) = expression {
            let source = self.check_expression_at_node(expression);
            self.check_return_expression(target, node, expression, source, false, is_async);
        } else {
            let undefined = self.intrinsics().undefined;
            self.report_assignability_failure(node, node, undefined, target);
        }
    }

    /// `checkReturnStatement`'s constructor arm (`checker.go:4115`): the
    /// returned value against `getReturnTypeFromAnnotation`'s class instance
    /// type (`checker.go:20058`), and TS2409 at the return statement when that
    /// relation fails.
    fn check_constructor_return(&mut self, constructor: NodeId, node: NodeId, expression: NodeId) {
        let Some(class) = self.nodes.parent(constructor) else { return };
        let Some(symbol) = self.binder.symbol_of(class) else { return };
        let symbol = self.binder.merged_symbol(symbol);
        let target = self.get_declared_type_of_symbol(symbol);
        let source = self.check_expression_at_node(expression);
        let before = self.diagnostics.len();
        self.check_excess_properties(target, expression);
        if self.diagnostics.len() == before
            && !self.report_assignability_failure(node, expression, source, target)
        {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(
            file,
            Diagnostic::new(
                &messages::RETURN_TYPE_OF_CONSTRUCTOR_SIGNATURE_MUST_BE_ASSIGNABLE_TO_THE_INSTANCE_TYPE_OF_THE_CLASS,
                span,
            ),
        );
    }

    /// `getContainingFunctionOrClassStaticBlock` for a return statement; a
    /// static block (TS18041's rule) answers `None`.
    fn return_statement_container(&self, node: NodeId) -> Option<NodeId> {
        let mut at = self.nodes.parent(node);
        while let Some(current) = at {
            match self.nodes.kind(current) {
                SyntaxKind::FunctionDeclaration
                | SyntaxKind::FunctionExpression
                | SyntaxKind::ArrowFunction
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor
                | SyntaxKind::Constructor => return Some(current),
                SyntaxKind::ClassStaticBlockDeclaration => return None,
                _ => at = self.nodes.parent(current),
            }
        }
        None
    }

    /// `getReturnTypeFromAnnotation` (`checker.go:20058`) for a non-constructor
    /// container ([`Checker::annotated_return_type`]), unwrapped as
    /// `unwrapReturnType` would for a plain function.
    ///
    /// Answers `(returnType, unwrappedReturnType, isAsync)`. An async
    /// function's target is `unwrapReturnType`'s `getAwaitedTypeNoAlias` of
    /// its annotation (`checker.go:20388`); a gap, or native's `nil` (whose
    /// `errorType` target relates to everything), declines. A generator's
    /// target is `unwrapReturnType`'s generator arm: the annotation's return
    /// iteration type (`getIterationTypeOfGeneratorFunctionReturnType`; for
    /// an async generator native then takes `getAwaitedTypeNoAlias(
    /// unwrapAwaitedType(…))`). No annotated return type is no check.
    ///
    /// **Async generators are declined**, as `check_yield_expression_assignability`
    /// declines them: the contextual type of an async generator's return
    /// operand (`getContextualTypeForReturnExpression`'s async arm) is not
    /// this port's, so `return Promise.resolve({ x: 'x' })` widens `'x'` and
    /// relates a source native never sees (`generatorReturnContextualType`
    /// 23:3, 27:3; `docs/parity/notes/r5-ts2322.md` §2.2).
    fn return_type_from_annotation(&mut self, container: NodeId) -> Option<(TypeId, TypeId, bool)> {
        let (generator, modifiers) = match self.node_map.get(container)? {
            Node::FunctionDeclaration(n) => (n.asterisk_token.is_some(), n.modifiers),
            Node::FunctionExpression(n) => (n.asterisk_token.is_some(), n.modifiers),
            Node::ArrowFunction(n) => (false, n.modifiers),
            Node::MethodDeclaration(n) => (n.asterisk_token.is_some(), n.modifiers),
            Node::GetAccessorDeclaration(n) => (false, n.modifiers),
            _ => return None,
        };
        if generator {
            let is_async = has_async(modifiers);
            if is_async {
                return None;
            }
            let return_type = self.annotated_return_type(container)?;
            // Native's nil (an `any` annotation, a missing slot) is
            // `errorType`, which relates to everything; a gap declines.
            let slot = self
                .get_iteration_type_of_generator_function_return_type(
                    crate::iteration::IterationTypeKind::Return,
                    return_type,
                    is_async,
                )
                .ok()??;
            return Some((return_type, slot, false));
        }
        let return_type = self.annotated_return_type(container)?;
        if has_async(modifiers) {
            let target = self.awaited_type_no_alias(return_type)?;
            return Some((return_type, target, true));
        }
        Some((return_type, return_type, false))
    }

    /// `getReturnTypeFromAnnotation` (`checker.go:20058`) for a
    /// non-constructor function-like, before unwrapping:
    /// - `declaration.Type()`: the written annotation, or in a JS file the
    ///   `@returns` type `reparseHosted` stores there
    ///   ([`Checker::jsdoc_reparsed_function`]'s `return_type`);
    /// - an unannotated get accessor's set accessor annotation
    ///   (`getAnnotatedAccessorType`);
    /// - `getReturnTypeOfFullSignature` (`checker.go:20091`): a JS function's
    ///   `@type` signature ([`Checker::jsdoc_full_signature_return_type`],
    ///   which reads function and method declarations).
    fn annotated_return_type(&mut self, container: NodeId) -> Option<TypeId> {
        let written = match self.node_map.get(container)? {
            Node::FunctionDeclaration(n) => n.r#type,
            Node::FunctionExpression(n) => n.r#type,
            Node::ArrowFunction(n) => n.r#type,
            Node::MethodDeclaration(n) => n.r#type,
            Node::GetAccessorDeclaration(n) => n.r#type,
            _ => return None,
        };
        let annotation = written.or_else(|| {
            self.in_js_file(container)
                .then(|| self.jsdoc_reparsed_function(container).return_type)
                .flatten()
        });
        if let Some(annotation) = annotation {
            return Some(self.get_type_from_type_node(annotation));
        }
        if let Some(annotation) = self.set_accessor_parameter_annotation(container) {
            return Some(self.get_type_from_type_node(annotation));
        }
        self.jsdoc_full_signature_return_type(container)
    }

    /// `getAnnotatedAccessorType` of the set accessor paired with an
    /// unannotated get accessor: its first non-`this` parameter's annotation.
    /// A computed name pairs when it is late-bindable (`hasBindableName`):
    /// the binder gives each such declaration its own `__computed` symbol,
    /// and the pair is the declarations `lateBindMember` gives one name
    /// ([`Checker::late_bound_members_of`] of the owner, on the getter's side
    /// of `static`).
    fn set_accessor_parameter_annotation(
        &mut self,
        getter: NodeId,
    ) -> Option<tsr_ast::TypeNode<'a>> {
        if self.nodes.kind(getter) != SyntaxKind::GetAccessor {
            return None;
        }
        let symbol = self.binder.symbol_of(getter)?;
        let declarations = if self.binder.symbols().get(symbol).name == "__computed" {
            let owner = self.binder.symbol_of(self.nodes.parent(getter)?)?;
            let owner = self.binder.merged_symbol(owner);
            let is_static = self.member_declaration_has_modifier(getter, SyntaxKind::StaticKeyword);
            let members = self.late_bound_members_of(owner, is_static);
            let name = members.iter().find(|(_, declaration)| *declaration == getter)?.0.clone();
            members
                .into_iter()
                .filter(|(member, _)| *member == name)
                .map(|(_, declaration)| declaration)
                .collect()
        } else {
            self.binder.symbols().get(symbol).declarations.clone()
        };
        declarations.into_iter().find_map(|declaration| {
            let Some(Node::SetAccessorDeclaration(setter)) = self.node_map.get(declaration) else {
                return None;
            };
            setter
                .parameters
                .iter()
                .find(|parameter| {
                    !matches!(parameter.name, Some(tsr_ast::BindingName::Identifier(name))
                        if name.text == "this")
                })
                .and_then(|parameter| parameter.r#type)
        })
    }

    /// `checkFunctionExpressionOrObjectLiteralMethodDeferred`
    /// (`checker.go:10206`), the concise-body arm: an arrow function whose body
    /// is an expression relates that expression, through
    /// `checkReturnExpression`, to `unwrapReturnType` of its annotated return
    /// type (`getReturnTypeFromAnnotation`, in JS the `@returns` tag). The error node is the body itself. An async arrow relates
    /// against the annotation's `getAwaitedTypeNoAlias`, as a statement does.
    pub(crate) fn check_arrow_expression_body(&mut self, node: NodeId, ambient: bool) {
        if ambient {
            return;
        }
        let Some(Node::ArrowFunction(arrow)) = self.node_map.get(node) else { return };
        let Some(body) = arrow.body else { return };
        let Some(body_id) = body.node_id() else { return };
        if self.nodes.kind(body_id) == SyntaxKind::Block {
            return;
        }
        let Some(mut target) = self.annotated_return_type(node) else { return };
        let is_async = has_async(arrow.modifiers);
        if is_async {
            let Some(awaited) = self.awaited_type_no_alias(target) else { return };
            target = awaited;
        }
        let source = self.check_expression_at_node(body_id);
        self.check_return_expression(target, body_id, body_id, source, false, is_async);
    }

    /// `checkReturnExpression` (`checker.go:4131`). A conditional expression
    /// (under parentheses) checks each branch on its own, reporting at the
    /// branch. Otherwise the error node is the return statement, or the
    /// effective expression for a concise body or a conditional branch. An async
    /// container relates the operand's `checkAwaitedType` (TS1058 at `node`).
    fn check_return_expression(
        &mut self,
        target: TypeId,
        node: NodeId,
        expression: NodeId,
        source: TypeId,
        in_conditional: bool,
        is_async: bool,
    ) {
        let unwrapped = self.skip_outer_parentheses(expression);
        if let Some(Node::ConditionalExpression(conditional)) = self.node_map.get(unwrapped) {
            for branch in [conditional.when_true, conditional.when_false].into_iter().flatten() {
                let Some(branch_id) = branch.node_id() else { continue };
                let branch_type = self.check_expression_at_node(branch_id);
                self.check_return_expression(target, node, branch_id, branch_type, true, is_async);
            }
            return;
        }
        // `checkAwaitedType(exprType, false, node, TS1058)` for an async
        // container; a gap declines, native's `nil` is `errorType`.
        let source = if is_async {
            let Some(awaited) = self.check_awaited_type(
                source,
                false,
                node,
                &messages::THE_RETURN_TYPE_OF_AN_ASYNC_FUNCTION_MUST_EITHER_BE_A_VALID_PROMISE_OR_MUST_NOT_CONTAIN_A_CALLABLE_THEN_MEMBER,
            ) else {
                return;
            };
            awaited
        } else {
            source
        };
        let effective = self.effective_check_node(expression);
        let error_node = if self.nodes.kind(node) == SyntaxKind::ReturnStatement && !in_conditional
        {
            node
        } else {
            effective
        };
        // §73: the elaboration reports the member instead of the outer message.
        let before = self.diagnostics.len();
        self.check_excess_properties(target, effective);
        if self.diagnostics.len() != before {
            return;
        }
        self.report_assignability_failure(error_node, effective, source, target);
    }

    /// The object literal expression a fresh object-literal type was checked
    /// from (its symbol's single declaration), and whether it has a spread.
    fn fresh_object_literal_node(&self, source: TypeId) -> Option<(NodeId, bool)> {
        if !self.fresh_object_literal_types.contains(&source) {
            return None;
        }
        let TypeData::Named { members: Some(owner), .. } = self.store.get(source).data else {
            return None;
        };
        let &[literal] = self.binder.symbols().get(owner).declarations.as_slice() else {
            return None;
        };
        let Some(Node::ObjectLiteralExpression(node)) = self.node_map.get(literal) else {
            return None;
        };
        let spread = node.properties.iter().any(|property| {
            matches!(property, tsr_ast::ObjectLiteralElementLike::SpreadAssignment(_))
        });
        Some((literal, spread))
    }

    /// `hasExcessProperties` (`relater.go:2714`) for a fresh literal with a
    /// spread against a non-union target. The walk is over the literal
    /// type's *final* properties: `shouldCheckAsExcessProperty` admits only
    /// one whose value declaration's parent is the literal itself, so a
    /// spread's keys, and a written key a later spread overrides, are never
    /// excess. The first one `isKnownProperty` rejects is reported at its
    /// declaration's name. `None` for a union target (its discriminant
    /// reduction reads written members) and wherever a step is undecidable.
    fn spread_literal_excess_property(
        &mut self,
        literal: NodeId,
        source: TypeId,
        target: TypeId,
    ) -> Option<(NodeId, String, TypeId)> {
        if self.type_of(target).flags.contains(TypeFlags::UNION)
            || !self.is_excess_property_check_target(target)
            || self.in_js_file(literal)
            || self.excess_check_target_admits_any_property(target)?
        {
            return None;
        }
        let properties = self.anonymous_properties.get(&source)?.0.clone();
        for property in properties {
            let declaration = self.binder.symbols().get(property.origin?).value_declaration?;
            if self.nodes.parent(declaration) != Some(literal) {
                continue;
            }
            if self.is_known_property(target, &property.name)? {
                continue;
            }
            let name = match self.node_map.get(declaration) {
                Some(Node::PropertyAssignment(node)) => node.name.node_id(),
                Some(Node::ShorthandPropertyAssignment(node)) => node.name.node_id(),
                Some(Node::MethodDeclaration(node)) => node.name.node_id(),
                Some(Node::GetAccessorDeclaration(node)) => node.name.node_id(),
                Some(Node::SetAccessorDeclaration(node)) => node.name.node_id(),
                _ => None,
            }?;
            return Some((name, property.name, target));
        }
        None
    }

    /// `ast.SkipParentheses`.
    ///
    /// A JS `@type` cast's parentheses hold a reparsed `AsExpression`
    /// natively (`ast.IsJSDocTypeAssertion`, `ast/utilities.go:759`), where
    /// `SkipParentheses` stops; the port keeps the cast in a side table, so
    /// the walk stops at those parentheses instead.
    fn skip_outer_parentheses(&self, mut node: NodeId) -> NodeId {
        while let Some(Node::ParenthesizedExpression(inner)) = self.node_map.get(node)
            && !self.is_jsdoc_cast_parentheses(node)
            && let Some(expression) = inner.expression.and_then(|e| e.node_id())
        {
            node = expression;
        }
        node
    }

    /// `ast.IsJSDocTypeAssertion` (`ast/utilities.go:759`): parentheses in a
    /// JS file carrying a `@type` cast.
    fn is_jsdoc_cast_parentheses(&self, node: NodeId) -> bool {
        self.in_js_file(node) && self.jsdoc_cast_annotation(node).is_some()
    }

    /// `getEffectiveCheckNode` (`checker.go:9381`): `ast.SkipOuterExpressions`
    /// over parentheses and `satisfies`, repeatedly; in a JS file a JSDoc type
    /// assertion's parentheses are kept (`OEKExcludeJSDocTypeAssertion`).
    fn effective_check_node(&self, mut node: NodeId) -> NodeId {
        loop {
            let inner = match self.node_map.get(node) {
                Some(Node::ParenthesizedExpression(_)) if self.is_jsdoc_cast_parentheses(node) => {
                    return node;
                }
                Some(Node::ParenthesizedExpression(inner)) => inner.expression,
                Some(Node::SatisfiesExpression(inner)) => inner.expression,
                _ => return node,
            };
            match inner.and_then(|expression| expression.node_id()) {
                Some(inner) => node = inner,
                None => return node,
            }
        }
    }

    /// `checkReferenceExpression`'s verdict (`checker.go:13130`) without its
    /// reports (those are `check_reference_expression`'s): an identifier or
    /// access under assertions and parentheses, not an optional chain.
    pub(crate) fn is_assignable_reference(&self, node: NodeId) -> bool {
        let spine = self.skip_reference_spine(node, true);
        matches!(
            self.nodes.kind(spine),
            SyntaxKind::Identifier
                | SyntaxKind::PropertyAccessExpression
                | SyntaxKind::ElementAccessExpression
        ) && !self.spine_has_optional_chain(spine)
    }

    /// The type an assignment writes *into*, or `None` where this port declines.
    ///
    /// # Why this is `getTypeOfSymbol` and not `checkExpression`
    ///
    /// `checkIdentifier` (`checker.go:11109`) returns early with the **declared**
    /// type when the reference is a definite assignment target — the flow type
    /// is what the variable holds *now*, and an assignment writes into what it
    /// was declared as. Using the flow type instead was 16 of one measurement's
    /// wrong lines, all in `controlFlowNoImplicitAny`: `let x;` narrows to
    /// `undefined` before its first assignment, so `x = 1` read as
    /// *number not assignable to undefined*. The declared type of an auto-typed
    /// variable is `any` and upstream reports nothing — the same auto-to-any
    /// mechanism `docs/architecture/checker-notes-narrow.md` §9 measures from the
    /// `.types` side.
    pub(crate) fn assignment_target_type(&mut self, node: NodeId) -> Option<TypeId> {
        // `checkParenthesizedExpression` answers its operand's type, and
        // `getAssignmentTargetKind` looks through parentheses, so `(x) = ''`
        // writes into `x`'s declared type exactly as `x = ''` does.
        let node = self.skip_outer_parentheses(node);
        // **A property-access target is admitted**, and §16's third decline —
        // which refused it because a `set` accessor's write type differs from
        // its getter's — is retired by measurement: +16 cases. The divergent-
        // accessor cases it was drawn for (`divergentAccessorsTypes2`) are a
        // handful; the ordinary `obj.field = value` it was also refusing is
        // sixteen. `getWriteTypeOfSymbol` would recover the handful.
        if self.nodes.kind(node) == SyntaxKind::PropertyAccessExpression {
            // An **inaccessible** property's access answers `errorType`
            // upstream — `checkPropertyAccessExpression` returns after
            // reporting TS2341/TS2445 — so no assignment check follows it.
            // `c.y = 1` on a private accessor is TS2341 alone, and this port
            // was adding a TS2322 beside it
            // (`checker-notes-diag2.md` §70).
            if self.inaccessible_property(node).is_some() {
                return None;
            }
            let ty = self.check_expression_at_node(node);
            return (ty != self.intrinsics().error).then_some(ty);
        }
        // `leftType := c.checkExpressionEx(left, checkMode)`
        // (`checker.go:12341`): an element access in a definite
        // assignment-target position answers the write type
        // (`checkElementAccessExpression`), and an asserted reference answers
        // the assertion's type.
        //
        // **Declined: a `unique symbol` key.** The write type is
        // `getIndexedAccessType(…, AccessFlagsWriting)`, which reaches the
        // setter of a late-bound accessor pair (`getWriteTypeOfSymbol`);
        // `check_element_access_expression`'s write arm resolves literal keys
        // only and answers the getter's type here
        // (`computedPropertiesWithSetterAssignment`).
        //
        // **Declined: an element access in a JS file.** The right operand's
        // contextual type there is `getContextualTypeForAssignmentDeclaration`'s
        // JS arm, which this port does not answer for element-access
        // assignments, so `handlers[++id] = [resolve, reject]` types the
        // literal as an array instead of the target's tuple
        // (`jsDeclarationsTypedefFunction`).
        if let Some(Node::ElementAccessExpression(access)) = self.node_map.get(node)
            && let Some(index) = access.argument_expression
        {
            if self.in_js_file(node) {
                return None;
            }
            let index = self.check_expression(index);
            if self.type_of(index).flags.intersects(TypeFlags::UNIQUE_ES_SYMBOL) {
                return None;
            }
        }
        if self.nodes.kind(node) != SyntaxKind::Identifier {
            let ty = self.check_expression_at_node(node);
            return (ty != self.intrinsics().error).then_some(ty);
        }
        let Some(Node::Identifier(identifier)) = self.node_map.get(node) else { return None };
        let text = identifier.text;
        let Some(symbol) = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            node,
            text,
            SymbolFlags::VALUE | SymbolFlags::ALIAS,
        ) else {
            // `resolveName`'s function-scope `argumentsSymbol`
            // (`binder/nameresolver.go`), whose type is the global
            // `IArguments`: `checkIdentifier` answers it, and an assignment
            // to `arguments` relates against it
            // (`parseClassDeclarationInStrictModeByDefaultInES6`). The
            // identifier's own check owns the container rule; an `arguments`
            // outside a function answers the error type, which declines.
            if text == "arguments" {
                let ty = self.check_expression_at_node(node);
                return (!self.is_gap(ty) && !self.is_error(ty)).then_some(ty);
            }
            return None;
        };
        let symbol = self.binder.merged_symbol(symbol);
        let entry = self.binder.symbols().get(symbol);
        // `checkIdentifier`'s assignment arms report TS2628/2629/2630/2631/2632
        // and return the error type, so the relation never runs for any of them.
        if entry.flags.intersects(
            SymbolFlags::ENUM
                | SymbolFlags::CLASS
                | SymbolFlags::FUNCTION
                | SymbolFlags::MODULE
                | SymbolFlags::ALIAS,
        ) {
            return None;
        }
        // A `const` target is TS2588, reported *instead of* the relation. Two
        // `var` declarations of one name share the first declaration's type
        // (`getTypeOfVariableOrParameterOrProperty`); a later conflicting one
        // is TS2403's, not this site's.
        let declarations: Vec<NodeId> = entry.declarations.to_vec();
        if declarations.is_empty()
            || declarations.iter().any(|&declaration| {
                self.declaration_is_constant(declaration)
                    || self.declaration_is_auto_typed(declaration)
            })
        {
            return None;
        }
        let declared = self.get_type_of_symbol(symbol);
        // `checkIdentifier` (`checker.go:11110`): a definite target of a
        // compound-like assignment (`x = x + 1`) reads at the literal's base,
        // exactly as `x += 1` would.
        if self.is_in_compound_like_assignment(node) {
            return Some(self.get_base_type_of_literal_type(declared));
        }
        Some(declared)
    }

    /// Is this an **auto-typed** declaration — `let x;`, no annotation and no
    /// initialiser?
    ///
    /// `convertAutoToAny` (`checker.go:11182`) makes such a variable's declared
    /// type `any` once flow analysis is done with it, and upstream therefore
    /// never reports an assignment into one. This port's declared type for the
    /// same declaration is `undefined`, which is the same divergence
    /// `docs/architecture/checker-notes-narrow.md` §9.1 measured from the
    /// `.types` side and **refused there** — so it is declined here rather than
    /// worked around, and the refusal keeps one owner.
    /// `controlFlowNoImplicitAny` is the whole of what it costs.
    fn declaration_is_auto_typed(&self, declaration: NodeId) -> bool {
        let Some(Node::VariableDeclaration(variable)) = self.node_map.get(declaration) else {
            return false;
        };
        if variable.r#type.is_some() {
            return false;
        }
        // `getTypeForVariableLikeDeclaration` (`checker.go`) gives the auto type
        // to `let x;` **and** to `let x = undefined;` — `controlFlowNoImplicitAny`
        // writes both spellings against the same expectation, and only the second
        // survived the first version of this predicate.
        match variable.initializer {
            None => true,
            Some(initializer) => matches!(
                initializer.node_id().and_then(|id| self.node_map.get(id)),
                Some(Node::Identifier(name)) if name.text == "undefined"
            ),
        }
    }

    /// Is this declaration a `const` (or `using`) binding?
    ///
    /// Assigning to one is TS2588 `Cannot_assign_to_0_because_it_is_a_constant`,
    /// which upstream reports at the same position and *instead of* TS2322 —
    /// `constDeclarations-access2` and its siblings.
    fn declaration_is_constant(&self, declaration: NodeId) -> bool {
        self.combined_node_flags(declaration).intersects(tsr_ast::NodeFlags::CONSTANT)
    }

    /// TS2353 — `Object literal may only specify known properties, and '{0}'
    /// does not exist in type '{1}'.`
    ///
    /// `hasExcessProperties` (`relater.go`), reached when a **fresh** object
    /// literal type is checked against a target. Upstream's freshness marker is
    /// on the type; here the question is asked of the syntax — is the expression
    /// *written* as an object literal at this position — which is the same set
    /// for every anchor this module has, because none of them is a place a
    /// literal's type can arrive already widened.
    ///
    /// The error node is the offending **property name**:
    /// `arrayCast.ts(3,23)` is the `foo` of `{ foo: "s" }`.
    ///
    /// # Only the first, exactly as upstream
    ///
    /// `hasExcessProperties` reports and returns on the first excess property it
    /// finds. Reporting every one would fail the case under the exact-multiset
    /// rule just as surely as reporting none.
    pub(crate) fn check_excess_properties(&mut self, target: TypeId, initializer: NodeId) {
        let Some(Node::ObjectLiteralExpression(literal)) = self.node_map.get(initializer) else {
            return;
        };
        // A spread contributes properties this port cannot enumerate.
        if literal.properties.iter().any(|property| {
            matches!(property, tsr_ast::ObjectLiteralElementLike::SpreadAssignment(_))
        }) {
            return;
        }
        // `isKnownProperty` (`relater.go:719`) knows a declared property or a
        // name an applicable index signature admits
        // (`getApplicableIndexInfoForName`). The declared table is complete
        // except for index signatures when `relation_property_table` answers
        // and `relation_members_are_complete` does not; those are read here
        // rather than declined, so `{ s: … }` against a number-only index
        // signature is excess (`objectLitIndexerContextualType` (18,5);
        // r7-reports §24).
        let Some(known) = self.relation_property_table(target) else { return };
        let indexed = if self.relation_members_are_complete(target) {
            false
        } else {
            match self.get_index_infos_of_type(target) {
                Some(infos) if !infos.is_empty() => true,
                _ => return,
            }
        };
        // An **empty** target is not an excess-property site. `class C {}` with
        // `c = { foo: '' }` reads TS2322 upstream, not TS2353
        // (`conformance/classWithEmptyBody`), because nothing about the literal
        // is assignable in the first place and the excess check only speaks when
        // the rest of the relation would have succeeded. This port runs no
        // relation here, so the emptiness test stands in for that condition —
        // and it was this rule's only loss.
        if known.is_empty() && !indexed {
            return;
        }
        let names: Vec<&str> = literal
            .properties
            .iter()
            .filter_map(|property| match property {
                tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                    Some(assignment.name)
                }
                tsr_ast::ObjectLiteralElementLike::MethodDeclaration(method) => Some(method.name),
                _ => None,
            })
            .filter_map(|name| {
                let id = name.node_id()?;
                match self.node_map.get(id) {
                    Some(Node::Identifier(identifier)) => Some(identifier.text),
                    Some(Node::StringLiteral(text)) => Some(text.text),
                    _ => None,
                }
            })
            .collect();
        // A shorthand or a computed name in the literal means the name list is
        // incomplete, and an incomplete list cannot say what is *excess*.
        if names.len() != literal.properties.len() {
            return;
        }
        // `checkTypeRelatedToAndOptionallyElaborate` runs `elaborateError`
        // (`elaborateObjectLiteral`) before the relation: every member whose
        // target member `getBestMatchIndexedAccessTypeOrUndefined` finds — a
        // declared one, or an apparent `Object` member such as `toString`
        // that `isKnownProperty` does not know — is related and reported in
        // source order. Only when none reports does `hasExcessProperties`
        // speak, for the first unknown member (r7-reports §23).
        // `everyTypeWithAnnotationAndInvalidInitializer.ts(43,28)` is the
        // `id` of `var anObjectLiteral: I = { id: 'a string' }` — the
        // property *name*, not the value and not the declaration.
        let before = self.diagnostics.len();
        let mut seen_names: Vec<&str> = Vec::new();
        for &name in &names {
            if !(known.iter().any(|(seen, _)| seen == name)
                || self.get_type_of_property_of_type(target, name).is_some()
                || indexed && self.applicable_index_value(target, name).is_some())
            {
                continue;
            }
            if names.iter().filter(|&&other| other == name).count() > 1 {
                // A name written twice: the literal's member is the last
                // occurrence's (last wins), and `elaborateObjectLiteral`
                // relates that one member at every occurrence
                // (`lastPropertyInLiteralWins`; r7-reports §35).
                if !seen_names.contains(&name) {
                    seen_names.push(name);
                    self.check_duplicated_object_literal_member(literal, target, name);
                }
                continue;
            }
            self.check_object_literal_member(literal, target, name);
        }
        if self.diagnostics.len() != before {
            return;
        }
        for name in names {
            if known.iter().any(|(seen, _)| seen == name)
                || indexed && self.applicable_index_value(target, name).is_some()
            {
                continue;
            }
            let Some(at) = self.excess_property_name_node(literal, name) else { return };
            let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
            let span = self.error_span(at);
            let printed = self.type_to_string(target);
            // hasExcessProperties (relater.go:2714): an identifier name with a
            // spelling suggestion among the target's properties
            // (getSuggestionForNonexistentProperty) is TS2561; a string-literal
            // name is never given a suggestion.
            let candidates: Vec<&str> = known.iter().map(|(seen, _)| seen.as_str()).collect();
            let suggestion = (self.nodes.kind(at) == SyntaxKind::Identifier)
                .then(|| crate::check::spelling_suggestion(name, &candidates))
                .flatten()
                .map(str::to_string);
            let diagnostic = if let Some(suggestion) = suggestion {
                Diagnostic::with_args(
                    &messages::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_BUT_0_DOES_NOT_EXIST_IN_TYPE_1_DID_YOU_MEAN_TO_WRITE_2,
                    span,
                    [name.to_string(), printed, suggestion],
                )
            } else {
                Diagnostic::with_args(
                    &messages::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_0_DOES_NOT_EXIST_IN_TYPE_1,
                    span,
                    [name.to_string(), printed],
                )
            };
            self.report(file, diagnostic);
            return;
        }
    }

    /// One known property of an object literal, against the target's member of
    /// the same name.
    ///
    /// `checkObjectLiteral`'s per-property contextual check, reduced to the
    /// comparison: the contextual type is the target the caller already has, and
    /// the verdict is the same `relate_ternary` every other rule in this module
    /// reads.
    fn check_object_literal_member(
        &mut self,
        literal: &tsr_ast::ObjectLiteralExpression<'_>,
        target: TypeId,
        name: &str,
    ) {
        let Some(at) = self.excess_property_name_node(literal, name) else { return };
        let Some(value) = self.object_literal_member_value(literal, name) else { return };
        // getIndexedAccessTypeOrUndefined: the property, else an applicable
        // index signature's value.
        let Some(member) = self
            .get_type_of_property_of_type(target, name)
            .or_else(|| self.applicable_index_value(target, name))
        else {
            return;
        };
        let source = self.check_expression_at_node(value);
        let before = self.diagnostics.len();
        self.report_assignability_failure(at, value, source, member);
        self.add_expected_type_related_info(before, at, Some(value), target, name);
    }

    /// `getApplicableIndexInfoForName(target, name)`'s value type.
    fn applicable_index_value(&mut self, target: TypeId, name: &str) -> Option<TypeId> {
        let key = self.property_name_key_type(name);
        self.get_applicable_index_info(target, key).map(|info| info.value)
    }

    /// The initializer of the last property assignment named `name`.
    fn last_object_literal_member_value(
        &self,
        literal: &tsr_ast::ObjectLiteralExpression<'_>,
        name: &str,
    ) -> Option<NodeId> {
        literal.properties.iter().rev().find_map(|property| {
            let tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) = property else {
                return None;
            };
            let id = assignment.name.node_id()?;
            let text = match self.node_map.get(id) {
                Some(Node::Identifier(identifier)) => identifier.text,
                Some(Node::StringLiteral(literal)) => literal.text,
                _ => return None,
            };
            (text == name)
                .then(|| assignment.initializer.and_then(|value| value.node_id()))
                .flatten()
        })
    }

    /// [`Checker::check_object_literal_member`] for a name the literal writes
    /// more than once: every occurrence is related with the last
    /// occurrence's value type, the literal's member, and reported at its
    /// own name with its own initializer elaborated.
    fn check_duplicated_object_literal_member(
        &mut self,
        literal: &tsr_ast::ObjectLiteralExpression<'_>,
        target: TypeId,
        name: &str,
    ) {
        let mut occurrences = Vec::new();
        for property in literal.properties {
            let tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) = property else {
                continue;
            };
            let Some(id) = assignment.name.node_id() else { continue };
            let text = match self.node_map.get(id) {
                Some(Node::Identifier(identifier)) => identifier.text,
                Some(Node::StringLiteral(literal)) => literal.text,
                _ => continue,
            };
            if text != name {
                continue;
            }
            let Some(value) = assignment.initializer.and_then(|value| value.node_id()) else {
                return;
            };
            occurrences.push((id, value));
        }
        let Some(last) = self.last_object_literal_member_value(literal, name) else { return };
        if occurrences.is_empty() {
            return;
        }
        let Some(member) = self
            .get_type_of_property_of_type(target, name)
            .or_else(|| self.applicable_index_value(target, name))
        else {
            return;
        };
        let source = self.check_expression_at_node(last);
        for (at, value) in occurrences {
            let before = self.diagnostics.len();
            self.report_assignability_failure(at, value, source, member);
            self.add_expected_type_related_info(before, at, Some(value), target, name);
        }
    }

    /// The initialiser node of the literal's property called `name`.
    fn object_literal_member_value(
        &self,
        literal: &tsr_ast::ObjectLiteralExpression<'_>,
        name: &str,
    ) -> Option<NodeId> {
        for property in literal.properties {
            let tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) = property else {
                continue;
            };
            let id = assignment.name.node_id()?;
            let text = match self.node_map.get(id) {
                Some(Node::Identifier(identifier)) => identifier.text,
                Some(Node::StringLiteral(literal)) => literal.text,
                _ => continue,
            };
            if text == name {
                return assignment.initializer.and_then(|value| value.node_id());
            }
        }
        None
    }

    /// [`Checker::check_expression`] reached from a [`NodeId`] — ADR-0013's
    /// read-drop-recurse, as [`crate::index_constraint`] does for type nodes.
    pub(crate) fn check_expression_at_node(&mut self, node: NodeId) -> TypeId {
        let error = self.intrinsics().error;
        let Some(typed) = self.node_map.get(node) else { return error };
        let Ok(expression) = tsr_ast::Expression::try_from(typed) else { return error };
        self.check_expression(expression)
    }

    /// The name node of the literal's property called `name`.
    pub(crate) fn excess_property_name_node(
        &self,
        literal: &tsr_ast::ObjectLiteralExpression<'_>,
        name: &str,
    ) -> Option<NodeId> {
        for property in literal.properties {
            let written = match property {
                tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                    assignment.name
                }
                tsr_ast::ObjectLiteralElementLike::MethodDeclaration(method) => method.name,
                _ => continue,
            };
            let id = written.node_id()?;
            let text = match self.node_map.get(id) {
                Some(Node::Identifier(identifier)) => identifier.text,
                Some(Node::StringLiteral(literal)) => literal.text,
                _ => continue,
            };
            if text == name {
                return Some(id);
            }
        }
        None
    }

    /// `reportUnmatchedProperty` (`relater.go:4345`): the required own keys
    /// absent from a source whose property table is complete.
    ///
    /// **Reported at the same position TS2322 would be**, and *instead of* it:
    /// `assignmentCompat1.ts(4,1)` is the `x` of `x = y`, and reporting TS2322
    /// there is a wrong code at a right position.
    ///
    /// # Why this can run where §16's gate declines
    ///
    /// §16's gate admits only types whose assignability is settled by flags,
    /// because the *structural relation* is incomplete. This asks a different
    /// question — **is a required property absent** — and that one is answered by
    /// the member tables alone, which [`crate::member_completeness`] can now
    /// certify. No relation runs, so no incompleteness leaks.
    ///
    /// Complete declared object tables exclude arrays and tuples, so native's
    /// `tryElaborateArrayLikeErrors` permits the multi-property head here. No
    /// array/tuple elaboration is inferred from incomplete tables.
    fn missing_required_property(&mut self, source: TypeId, target: TypeId) -> Option<Vec<String>> {
        let target_properties = self.relation_property_table(target)?;
        let source_properties = self.relation_property_table(source)?;
        if self.fresh_object_literal_types.contains(&source) {
            // hasExcessProperties precedes reportUnmatchedProperty. Captured
            // names alone cannot license a missing head when a written key
            // belongs to that earlier, possibly unported error path.
            let TypeData::Named { members: Some(owner), .. } = self.store.get(source).data else {
                return None;
            };
            let &[literal] = self.binder.symbols().get(owner).declarations.as_slice() else {
                return None;
            };
            if self.nodes.kind(literal) != SyntaxKind::ObjectLiteralExpression {
                return None;
            }
            let properties = self.anonymous_properties.get(&source)?.0.clone();
            for property in properties {
                // shouldCheckAsExcessProperty compares declaration parents:
                // a spread's keys retain their original declaration, and a
                // later spread can replace an earlier written property's origin.
                let origin = property.origin?;
                let declaration = self.binder.symbols().get(origin).value_declaration?;
                if self.nodes.parent(declaration) != Some(literal) {
                    continue;
                }
                // isKnownProperty reads the object's own/inherited table, not
                // global Object augmentation. The certified table also keeps
                // a mapped container's keys distinct from its origin's keys.
                if target_properties.iter().any(|(name, _)| name == &property.name) {
                    continue;
                }
                // None is an unresolved index table, not proof of no index.
                self.get_index_infos_of_type(target)?;
                let key = self.store.intern_literal(
                    TypeFlags::STRING_LITERAL,
                    TypeData::StringLiteral(property.name),
                    false,
                );
                self.get_applicable_index_info(target, key)?;
            }
        }
        let mut missing = Vec::new();
        for (name, optional) in target_properties {
            if !optional
                && !source_properties.iter().any(|(seen, _)| seen == &name)
                // getUnmatchedProperties uses getPropertyOfType, whose misses
                // can still be supplied by Function/Object augmentation.
                && self.get_type_of_property_of_type(source, &name).is_none()
            {
                missing.push(name);
            }
        }
        (!missing.is_empty()).then_some(missing)
    }

    /// `reportUnmatchedProperty`'s messages (`relater.go:4345`): TS2741 for one
    /// missing property, else TS2739, or TS2740 past five names.
    fn report_missing_properties(
        &mut self,
        file: NodeId,
        span: tsr_core::Span,
        source: TypeId,
        target: TypeId,
        properties: &[String],
    ) {
        let diagnostic = self.missing_properties_diagnostic(span, source, target, properties);
        self.report(file, diagnostic);
    }

    /// The message [`Checker::report_missing_properties`] reports, unreported.
    fn missing_properties_diagnostic(
        &mut self,
        span: tsr_core::Span,
        source: TypeId,
        target: TypeId,
        properties: &[String],
    ) -> Diagnostic {
        let source_text = self.type_to_string(source);
        let target_text = self.type_to_string(target);
        let (message, args) = if properties.len() == 1 {
            (
                &messages::PROPERTY_0_IS_MISSING_IN_TYPE_1_BUT_REQUIRED_IN_TYPE_2,
                vec![properties[0].clone(), source_text, target_text],
            )
        } else if properties.len() > 5 {
            (
                &messages::TYPE_0_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_1_COLON_2_AND_3_MORE,
                vec![
                    source_text,
                    target_text,
                    properties[..4].join(", "),
                    (properties.len() - 4).to_string(),
                ],
            )
        } else {
            (
                &messages::TYPE_0_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_1_COLON_2,
                vec![source_text, target_text, properties.join(", ")],
            )
        };
        let mut diagnostic = Diagnostic::with_args(message, span, args);
        // `reportUnmatchedProperty`: `'p' is declared here.` at the missing
        // property's first declaration (r7-reports §38).
        if let [name] = properties
            && let Some(property) = self
                .get_property_of_type(target, name)
                .or_else(|| self.generic_heritage_property_symbol(target, name, &mut Vec::new()))
            && let Some(&declaration) = self.binder.symbols().get(property).declarations.first()
        {
            let declared_span = self.error_span(declaration);
            let related = self.related_diagnostic_at(
                declaration,
                &messages::_0_IS_DECLARED_HERE,
                declared_span,
                vec![name.clone()],
            );
            if related.file().is_some() {
                diagnostic.set_related_information(std::sync::Arc::new(vec![related]));
            }
        }
        diagnostic
    }

    /// `reportRelationError`'s missing-property suppression (`relater.go:4816`)
    /// for a failure whose chain ends in `reportUnmatchedProperty`'s message:
    /// whether the missing-property message stands alone, or stays as the
    /// chain child under the caller's head (TS2322/TS2345).
    ///
    /// The missing-property message names the pair `propertiesRelatedTo` saw:
    /// `getNormalizedType` (`checker.go:27865`) has reduced each side through
    /// `getSingleBaseForNonAugmentingSubtype`, and `structuredTypeRelatedTo`'s
    /// type-variable arm (`relater.go:3665`) has moved a type-parameter source
    /// to its constraint. The head names the pair `reportErrorResults`
    /// (`relater.go:4705`) displays: the original side when it has an alias
    /// or a single base. `chainArgsMatch` compares the printed strings, so
    /// this does too. A type-parameter source always keeps the head (`T` never
    /// prints as its constraint); an undecided constraint leaves no child.
    fn missing_property_chain(
        &mut self,
        span: tsr_core::Span,
        source: TypeId,
        display_target: TypeId,
        target: TypeId,
        properties: &[String],
        head: Option<&'static tsr_diagnostics::Message>,
    ) -> MissingPropertyHead {
        // `isConversionOrInterfaceImplementationMessage` (`relater.go:4934`):
        // these heads are never suppressed (r7-reports §45).
        let keeps_head = head.is_some_and(is_conversion_or_interface_implementation_message);
        let mut chain_source = source;
        if self.type_of(source).flags.contains(TypeFlags::TYPE_PARAMETER) {
            let crate::constraints::ConstraintOfType::Constraint(constraint) =
                self.constraint_of_type(source)
            else {
                return MissingPropertyHead::Kept(None);
            };
            chain_source = constraint;
        }
        let chain_source = self.single_base_normalized(chain_source);
        let chain_target = self.single_base_normalized(target);
        if !keeps_head && chain_source == source && chain_target == display_target {
            return MissingPropertyHead::Suppressed;
        }
        let displayed_source = self.assignability_source_for_error_display(source, display_target);
        if !keeps_head
            && self.type_to_string(displayed_source) == self.type_to_string(chain_source)
            && self.type_to_string(display_target) == self.type_to_string(chain_target)
        {
            return MissingPropertyHead::Suppressed;
        }
        MissingPropertyHead::Kept(Some(self.missing_properties_diagnostic(
            span,
            chain_source,
            chain_target,
            properties,
        )))
    }

    /// `getNormalizedType`'s reference arm (`checker.go:27865`), iterated:
    /// each step replaces a non-augmenting subtype with its single base.
    fn single_base_normalized(&mut self, mut t: TypeId) -> TypeId {
        // Each step moves to a base, and `get_base_types` refuses a circular
        // base, so the walk ends; the bound only guards a malformed table.
        for _ in 0..64 {
            match self.single_base_for_non_augmenting_subtype(t) {
                Some(base) if base != t => t = base,
                _ => break,
            }
        }
        t
    }

    /// `getSingleBaseForNonAugmentingSubtype` (`checker.go:28087`): the one
    /// base of a class or interface reference that declares no members.
    ///
    /// `ObjectFlagsReference` holds for every class and for an interface that
    /// is generic or not `isThislessInterface` (`getDeclaredTypeOfClassOrInterface`,
    /// `checker.go`). Native's `getMembersOfSymbol` counts the type parameters
    /// the binder files in the members table, so a generic target never has a
    /// single base and no instantiation through its type arguments is needed;
    /// a `this` argument does not change the printed base. Not modelled: an
    /// interface that is a reference only through *outer* type parameters is
    /// read as thisless.
    fn single_base_for_non_augmenting_subtype(&mut self, t: TypeId) -> Option<TypeId> {
        let symbol = match self.type_reference_targets.get(&t) {
            Some(&(symbol, _)) => symbol,
            None => match self.type_of(t).data {
                TypeData::Named { members: Some(symbol), .. } => symbol,
                _ => return None,
            },
        };
        let symbol = self.binder.merged_symbol(symbol);
        let flags = self.binder.symbols().get(symbol).flags;
        if !flags.intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE) {
            return None;
        }
        // `getMembersOfSymbol` includes the late-bound members, which the
        // binder's table does not hold (`private [x]: number`; r7-reports §88).
        if !self.binder.symbols().get(symbol).members.is_empty()
            || !self.late_bound_members_of(symbol, false).is_empty()
        {
            return None;
        }
        if flags.contains(SymbolFlags::CLASS) {
            // A base expression other than a simple (qualified) name may
            // circularly reference the class itself.
            if !self.class_base_expression_is_simple_name(symbol) {
                return None;
            }
        } else if self.is_thisless_interface(symbol) {
            return None;
        }
        let bases = self.get_base_types(symbol);
        let &[base] = bases.as_slice() else { return None };
        Some(base)
    }

    /// The class half of `getSingleBaseForNonAugmentingSubtype`'s gate: no
    /// `extends` clause, or one whose expression is an identifier or property
    /// access (`getBaseTypeNodeOfClass`).
    fn class_base_expression_is_simple_name(&self, class: SymbolId) -> bool {
        let clauses = self.binder.symbols().get(class).declarations.iter().find_map(
            |&declaration| match self.node_map.get(declaration) {
                Some(Node::ClassDeclaration(class)) => Some(class.heritage_clauses),
                Some(Node::ClassExpression(class)) => Some(class.heritage_clauses),
                _ => None,
            },
        );
        let Some(base) = clauses.and_then(|clauses| {
            clauses
                .iter()
                .find(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)
                .and_then(|clause| clause.types.first())
        }) else {
            return true;
        };
        matches!(
            base.expression,
            Some(
                tsr_ast::Expression::Identifier(_)
                    | tsr_ast::Expression::PropertyAccessExpression(_)
            )
        )
    }

    /// `isThislessInterface` (`checker.go:17356`): no declaration uses `this`,
    /// and every entity-name base is an interface whose declared type has no
    /// `thisType` (it is neither generic nor itself `this`-using).
    fn is_thisless_interface(&mut self, symbol: SymbolId) -> bool {
        self.is_thisless_interface_at(symbol, 0)
    }

    fn is_thisless_interface_at(&mut self, symbol: SymbolId, depth: u32) -> bool {
        if depth > 32 {
            return false;
        }
        let declarations = self.binder.symbols().get(symbol).declarations.clone();
        for declaration in declarations {
            let Some(Node::InterfaceDeclaration(interface)) = self.node_map.get(declaration) else {
                continue;
            };
            if self.binder.facts(declaration).contains(tsr_binder::NodeFacts::CONTAINS_THIS) {
                return false;
            }
            let Some(clause) = interface
                .heritage_clauses
                .iter()
                .find(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)
            else {
                continue;
            };
            for base in clause.types {
                // Only an entity-name expression is resolved (isEntityNameExpression).
                let Some(
                    expression @ (tsr_ast::Expression::Identifier(_)
                    | tsr_ast::Expression::PropertyAccessExpression(_)),
                ) = base.expression
                else {
                    continue;
                };
                let Some(base_symbol) = self.heritage_entity_symbol(expression, SymbolFlags::TYPE)
                else {
                    return false;
                };
                let base_symbol = self.binder.merged_symbol(base_symbol);
                let base_flags = self.binder.symbols().get(base_symbol).flags;
                // getDeclaredTypeOfClassOrInterface gives a thisType to a
                // class, a generic declaration and a this-using interface.
                if !base_flags.contains(SymbolFlags::INTERFACE)
                    || base_flags.contains(SymbolFlags::CLASS)
                    || self.symbol_declares_type_parameters(base_symbol)
                    || !self.is_thisless_interface_at(base_symbol, depth + 1)
                {
                    return false;
                }
            }
        }
        true
    }

    /// Whether any declaration of a class or interface declares its own type
    /// parameters: `getDeclaredTypeOfClassOrInterface`'s `localTypeParameters`.
    fn symbol_declares_type_parameters(&self, symbol: SymbolId) -> bool {
        self.binder.symbols().get(symbol).declarations.iter().any(|&declaration| {
            match self.node_map.get(declaration) {
                Some(Node::InterfaceDeclaration(node)) => !node.type_parameters.is_empty(),
                Some(Node::ClassDeclaration(node)) => !node.type_parameters.is_empty(),
                Some(Node::ClassExpression(node)) => !node.type_parameters.is_empty(),
                _ => false,
            }
        })
    }

    /// `tryElaborateArrayLikeErrors`' TS4104 (`relater.go:4379`), reported by
    /// `reportErrorResults` (`relater.go:4705`) in place of the head message,
    /// which `reportRelationError` (`relater.go:4751`) suppresses for the
    /// same pair.
    fn report_readonly_to_mutable(
        &mut self,
        file: NodeId,
        span: tsr_core::Span,
        source: TypeId,
        target: TypeId,
    ) {
        let source_text = self.type_to_string(source);
        let target_text = self.type_to_string(target);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::THE_TYPE_0_IS_READONLY_AND_CANNOT_BE_ASSIGNED_TO_THE_MUTABLE_TYPE_1,
                span,
                [source_text, target_text],
            ),
        );
    }

    /// TS2345 at an argument position — the same verdict machinery as
    /// [`Checker::report_assignability_failure`] with a different head code.
    /// Answers **whether it reported**, so the caller can stop:
    /// `getSignatureApplicabilityError` returns on the first failing argument
    /// (`checker-notes-diag2.md` §59).
    ///
    /// The argument is checked through `checkTypeRelatedToAndOptionallyElaborate`
    /// (`getSignatureApplicabilityError`, `checker.go:9302`) with the argument
    /// as both error node and expression, so a failed relation first runs
    /// `elaborateError` (`relater.go:440`) and reports the offending member
    /// (TS2322 at a property, element or arrow return) instead of TS2345.
    pub(crate) fn report_argument_failure(
        &mut self,
        at: NodeId,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let union_literal = self.nodes.kind(at) == SyntaxKind::ObjectLiteralExpression
            && self.type_of(target).flags.contains(TypeFlags::UNION);
        let mut excess_failed = false;
        if union_literal {
            match self.union_object_literal_failure(at, source, target) {
                UnionLiteralFailure::Settled(reported) => return reported,
                UnionLiteralFailure::Outer { excess } => excess_failed = excess,
            }
        }
        if !self.assignability_pair_is_reportable(source, target) {
            return false;
        }
        let (relation, signature_error) = if excess_failed {
            (crate::relater::Ternary::NotRelated, None)
        } else {
            self.relate_with_signature_diagnostic(
                source,
                target,
                crate::relater::Relation::Assignable,
                true,
            )
        };
        let not_related = relation == crate::relater::Ternary::NotRelated;
        if not_related
            && !union_literal
            && self.elaborate_error(
                at,
                source,
                target,
                Some(&messages::ARGUMENT_OF_TYPE_0_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE_1),
            )
        {
            return true;
        }
        let span = self.error_span(at);
        if self.report_weak_type_failure(at, span, source, target) {
            return true;
        }
        if !not_related && !self.object_against_primitive(source, target) {
            return false;
        }
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return false };
        if not_related && self.readonly_to_mutable_array_like(source, target) {
            self.report_readonly_to_mutable(file, span, source, target);
            return true;
        }
        // reportRelationError suppresses the TS2345 head when the chain ends in
        // the pair's missing-property message (relater.go:4751), exactly as it
        // does for TS2322; a fresh literal keeps the written-key guard.
        // getNormalizedType (relater.go:2619) unwraps `NoInfer<T>`; the
        // missing-property messages name the normalized target.
        let (normalized, chain_target, display_target) =
            self.relation_error_targets(source, target);
        if not_related
            && let Some(properties) = self
                .missing_required_property(source, normalized)
                .or_else(|| self.unmatched_property_report(source, normalized))
        {
            let MissingPropertyHead::Kept(chain) = self.missing_property_chain(
                span,
                source,
                chain_target,
                normalized,
                &properties,
                None,
            ) else {
                self.report_missing_properties(file, span, source, normalized, &properties);
                return true;
            };
            self.report_argument_head(at, file, span, source, display_target, None, chain);
            return true;
        }
        // checkTypeRelatedToEx's error chain under the head (r7-reports §42).
        let chain = if not_related && signature_error.is_none() {
            self.relation_failure_chain(at, span, source, normalized)
        } else {
            None
        };
        if let Some(alone) =
            self.suppressed_head_chain(chain.as_ref(), source, display_target, None)
        {
            self.report(file, alone);
            return true;
        }
        self.report_argument_head(at, file, span, source, display_target, signature_error, chain);
        true
    }

    /// The pair `isRelatedToEx` (`relater.go:2600`) relates after its
    /// normalization, and the target `reportErrorResults` (`relater.go:4705`)
    /// displays. `getNormalizedType` (`relater.go:2619`) unwraps `NoInfer<T>`;
    /// then a definitely non-nullable source against a union of `null` and/or
    /// `undefined` plus one other type is related to that type alone
    /// (`relater.go:2646`), so the missing-property messages name it. The
    /// head shows the normalized target unless the original has an alias
    /// (a union printed by its alias name), as `reportErrorResults` keeps
    /// `originalTarget` then. Answers (related target, the target
    /// `chainArgsMatch` compares, the head's printed target); a `NoInfer`
    /// target keeps its original head, as before.
    fn relation_error_targets(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> (TypeId, TypeId, TypeId) {
        let normalized = self.no_infer_base_type(target).unwrap_or(target);
        let Some(candidate) = self.non_nullable_union_candidate(source, normalized) else {
            return (normalized, normalized, target);
        };
        let aliased =
            matches!(self.type_of(normalized).data, TypeData::Union { symbol: Some(_), .. });
        let display = if aliased { target } else { candidate };
        (candidate, display, display)
    }

    /// `reportRelationError`'s TS2345 head (`relater.go:4751`) for
    /// [`Checker::report_argument_failure`], with the signature elaboration or
    /// the missing-property message kept under it.
    #[allow(clippy::too_many_arguments)]
    fn report_argument_head(
        &mut self,
        at: NodeId,
        file: NodeId,
        span: tsr_core::Span,
        source: TypeId,
        target: TypeId,
        signature_error: Option<Diagnostic>,
        chain: Option<Diagnostic>,
    ) {
        let displayed_source = self.assignability_source_for_error_display(source, target);
        let source_text = self.type_to_string(displayed_source);
        let target_text = self.type_to_string(target);
        let mut diagnostic = self.relation_diagnostic(
            span,
            source,
            target,
            &messages::ARGUMENT_OF_TYPE_0_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE_1,
            source_text,
            target_text,
        );
        if let Some(mut signature_error) = signature_error {
            signature_error.span = span;
            diagnostic.add_message_chain(Some(signature_error));
        }
        add_relation_chain(&mut diagnostic, chain);
        self.attach_chain_file(at, span, &mut diagnostic);
        self.report_relation_head_diagnostic(file, source, target, diagnostic);
    }

    /// Report the assignability failure at `span`, choosing the code the way
    /// upstream's relation does: absent required properties are TS2741/2739/2740,
    /// a direct exact-optional missing-property write is TS2412, a whole-object
    /// exact-optional mismatch is TS2375, and other failures are TS2322.
    pub(crate) fn report_assignability_failure(
        &mut self,
        at: NodeId,
        source_node: NodeId,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        self.report_assignability_failure_with(at, Some(source_node), source, target)
    }

    /// [`Checker::report_assignability_failure`]; with no `source_node` the
    /// expression is not elaborated — `checkTypeRelatedToEx` reached from an
    /// elaboration that already chose its error node.
    fn report_assignability_failure_with(
        &mut self,
        at: NodeId,
        source_node: Option<NodeId>,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let span = self.error_span(at);
        self.report_relation_failure(at, span, source_node, source, target, None)
    }

    /// [`Checker::report_assignability_failure`] with a caller's head message
    /// and error span: `checkTypeAssignableToAndOptionallyElaborate`'s
    /// `headMessage` (relater.go). `reportRelationError` (relater.go:4751)
    /// uses the head as given; the TS2322 defaults (exact-optional variants)
    /// apply only when there is none. A missing-property chain still replaces
    /// the head (relater.go:4816), since no caller here passes a conversion or
    /// interface-implementation message.
    pub(crate) fn report_relation_failure(
        &mut self,
        at: NodeId,
        span: tsr_core::Span,
        source_node: Option<NodeId>,
        source: TypeId,
        target: TypeId,
        head: Option<&'static tsr_diagnostics::Message>,
    ) -> bool {
        // An object literal against a **union** target is the excess-property
        // and discriminated-union machinery
        // (`getMatchingUnionConstituentForObjectLiteral`,
        // `findMatchingDiscriminantType`), and upstream reports TS2353 / TS2561 /
        // TS2739 there rather than TS2322. Asked of the *syntax* because the
        // literal's type is synthesised and carries no declaration to enumerate:
        // 31 wrong lines across six cases — `excessPropertyCheckWithUnions`,
        // `assignmentCompatWithDiscriminatedUnion`, both `missingDiscriminants`.
        // §172's probe. Records the gate that decided every position this site
        // was asked about, so the unemitted TS2322 population can be split into
        // "never visited" and "visited and declined". Off unless
        // `TSR_ASSIGN_PROBE` is set; the `OnceLock` keeps it to one `getenv`.
        macro_rules! probe {
            ($verdict:expr) => {
                if assign_probe_enabled()
                    && let Some(probe_file) = self.source_file_of_for_diagnostics(at)
                {
                    self.assignability_probe.push((probe_file, span, $verdict));
                }
            };
        }
        // An object literal against a **union** target. `isRelatedTo` runs
        // `hasExcessProperties` (`relater.go:2714`) before the structural
        // relation; a failure then goes to `elaborateObjectLiteral`
        // (`relater.go:498`), which reads each member through
        // `getBestMatchIndexedAccessTypeOrUndefined` (`relater.go:620`): the
        // union's own indexed access first, then `getBestMatchingType`'s
        // constituent. When no member elaborates, `checkTypeRelatedToEx`
        // reports the excess member (TS2353/TS2561), or else TS2322 at the
        // error node with the union as target, since `reportRelationError`'s
        // missing-property suppression matches chain arguments against the
        // union and never fires. Where this port cannot decide a member's
        // target type or the excess question, it declines.
        // checkTypeRelatedToAndOptionallyElaborate (relater.go:431): the
        // relation runs first, and a related pair is neither elaborated nor
        // reported. This is the same reporting relation the road below would
        // make, computed once and kept, so an overflow it publishes is read as
        // before. A fresh object literal keeps the full road, because its
        // excess-property arms run here, outside the relation
        // (`r7-perf.md` §33).
        let mut related_first = None;
        if source_node
            .is_none_or(|node| self.nodes.kind(node) != SyntaxKind::ObjectLiteralExpression)
            && !self.fresh_object_literal_types.contains(&source)
        {
            let answer = self.relate_with_signature_diagnostic(
                source,
                target,
                crate::relater::Relation::Assignable,
                true,
            );
            if answer.0 == crate::relater::Ternary::Related {
                probe!(PROBE_RELATION_DECLINED);
                return false;
            }
            related_first = Some(answer);
        }
        let union_literal = source_node.filter(|&node| {
            self.nodes.kind(node) == SyntaxKind::ObjectLiteralExpression
                && self.type_of(target).flags.contains(TypeFlags::UNION)
        });
        let mut excess_failed = false;
        if let Some(node) = union_literal {
            match self.union_object_literal_failure(node, source, target) {
                UnionLiteralFailure::Settled(reported) => {
                    probe!(if reported { PROBE_REPORTED } else { PROBE_OBJECT_LITERAL_UNION });
                    return reported;
                }
                UnionLiteralFailure::Outer { excess } => excess_failed = excess,
            }
        }
        // `elaborateError` (`relater.go:440`) runs **before** the whole-expression
        // report and, when it speaks, `checkTypeRelatedToEx` stays silent. The
        // hand-off is exclusive by construction here because both live in this
        // one function: elaborating returns, it does not fall through. §176.
        if union_literal.is_none()
            && source_node.is_some_and(|node| self.elaborate_error(node, source, target, head))
        {
            probe!(PROBE_REPORTED);
            return true;
        }
        if !self.assignability_pair_is_reportable(source, target) {
            probe!(PROBE_PAIR_NOT_REPORTABLE);
            return false;
        }
        // When `elaborateError` stays silent, `checkTypeRelatedToEx` relates
        // the fresh literal, and `isRelatedTo` meets `hasExcessProperties`
        // (`relater.go:2714`) before the structural relation; its report
        // moves the error node to the excess member (TS2353/TS2561). A union
        // target took that path in `union_object_literal_failure`.
        if union_literal.is_none()
            && let Some((literal, spread)) = self.fresh_object_literal_node(source)
            && let Some((excess_at, name, error_target)) = if spread {
                self.spread_literal_excess_property(literal, source, target)
            } else {
                match self.excess_properties_verdict(literal, source, target) {
                    Some(ExcessProperties::Excess { at, name, error_target }) => {
                        Some((at, name, error_target))
                    }
                    _ => None,
                }
            }
            && self.report_excess_property(excess_at, &name, error_target)
        {
            probe!(PROBE_REPORTED);
            return true;
        }
        if self.report_weak_type_failure(at, span, source, target) {
            probe!(PROBE_REPORTED);
            return true;
        }
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return false };
        // getNormalizedType (relater.go:2619) unwraps `NoInfer<T>`; the
        // missing-property messages name the normalized target.
        let (normalized, chain_target, display_target) =
            self.relation_error_targets(source, target);
        let narrowed = normalized != self.no_infer_base_type(target).unwrap_or(target);
        // A union literal's own machinery decided the excess question; once
        // the target narrows to one type, the missing-property report is the
        // plain one again (unless the excess check already failed).
        let missing_property_report = union_literal.is_none() || (narrowed && !excess_failed);
        // **`relate_ternary`, not `is_type_assignable_to`.** The relater is
        // three-valued (`crate::relater::Ternary`) and its own doc comment names
        // the caller this distinction exists for: *"one that acts on a
        // negative"*. TS2322 is exactly that caller, and the binary projection —
        // which collapses `Unknown` into `false` — is what produced this
        // module's first measurement of **947 right against 988 wrong**. Every
        // undecidable pair was being reported as an error.
        let (relation, signature_error) = if excess_failed {
            (crate::relater::Ternary::NotRelated, None)
        } else if let Some(answer) = related_first {
            answer
        } else {
            self.relate_with_signature_diagnostic(
                source,
                target,
                crate::relater::Relation::Assignable,
                true,
            )
        };
        // checkTypeRelatedToEx (relater.go:371-382): an overflowing check
        // reports TS2859 (relation-count budget) or TS2321 (a 100-entry
        // source or target stack) instead of its relation error. Native's
        // preceding silent isTypeRelatedTo overflows too and reports at
        // `c.currentNode` (the second TS2859 of `relationComplexityError`);
        // the port has no currentNode, so that report needs the check
        // site's node.
        if relation == crate::relater::Ternary::Unknown
            && let Some(overflow) = self.assignability_overflow(source, target)
        {
            let message = if overflow == crate::relation_cache::CachedRelation::ComplexityOverflow {
                &tsr_diagnostics::messages::EXCESSIVE_COMPLEXITY_COMPARING_TYPES_0_AND_1
            } else {
                &tsr_diagnostics::messages::EXCESSIVE_STACK_DEPTH_COMPARING_TYPES_0_AND_1
            };
            let source_text = self.type_to_string(source);
            let target_text = self.type_to_string(target);
            self.report(
                file,
                tsr_diagnostics::Diagnostic::with_args(message, span, [source_text, target_text]),
            );
            return true;
        }
        // The missing-property report is checkTypeRelatedToEx's elaboration of
        // a failed relation (propertiesRelatedTo's unmatched arm,
        // relater.go:4233): a pair the relation relates reports nothing
        // (`boolean -> NotBoolean` through an augmented `Boolean`,
        // `assignFromBooleanInterface2`).
        if REPORT_MISSING_REQUIRED_PROPERTY
            && missing_property_report
            && relation != crate::relater::Ternary::Related
            && let Some(properties) = self.missing_required_property(source, normalized)
        {
            probe!(PROBE_REPORTED);
            match self.missing_property_chain(
                span,
                source,
                chain_target,
                normalized,
                &properties,
                head,
            ) {
                MissingPropertyHead::Suppressed => {
                    self.report_missing_properties(file, span, source, normalized, &properties);
                }
                MissingPropertyHead::Kept(chain) => {
                    self.report_relation_head(
                        at,
                        file,
                        span,
                        source,
                        display_target,
                        head,
                        None,
                        chain,
                    );
                }
            }
            return true;
        }
        let not_related = relation == crate::relater::Ternary::NotRelated;
        // `object_against_primitive` stands in for a relation the relater
        // could not decide; it never overrides a decided Related (native
        // reports nothing for a related pair: `true -> NotBoolean` through
        // an augmented `Boolean`, `assignFromBooleanInterface2`).
        if relation == crate::relater::Ternary::Related
            || (!not_related && !self.object_against_primitive(source, target))
        {
            probe!(PROBE_RELATION_DECLINED);
            return false;
        }
        probe!(PROBE_REPORTED);
        if not_related && self.readonly_to_mutable_array_like(source, target) {
            self.report_readonly_to_mutable(file, span, source, target);
            return true;
        }
        if not_related
            && missing_property_report
            && let Some(properties) = self.unmatched_property_report(source, normalized)
        {
            match self.missing_property_chain(
                span,
                source,
                chain_target,
                normalized,
                &properties,
                head,
            ) {
                MissingPropertyHead::Suppressed => {
                    self.report_missing_properties(file, span, source, normalized, &properties);
                }
                MissingPropertyHead::Kept(chain) => {
                    self.report_relation_head(
                        at,
                        file,
                        span,
                        source,
                        display_target,
                        head,
                        None,
                        chain,
                    );
                }
            }
            return true;
        }
        // checkTypeRelatedToEx's error chain under the head, retraced where
        // every step is decided (r7-reports §36).
        let explained = not_related && signature_error.is_none();
        let (source, normalized, display_target) = if explained {
            self.normalized_head_pair(source, normalized, display_target)
        } else {
            (source, normalized, display_target)
        };
        let chain = if explained {
            self.relation_failure_chain(at, span, source, normalized)
        } else {
            None
        };
        if let Some(alone) =
            self.suppressed_head_chain(chain.as_ref(), source, display_target, head)
        {
            self.report(file, alone);
            return true;
        }
        self.report_relation_head(
            at,
            file,
            span,
            source,
            display_target,
            head,
            signature_error,
            chain,
        );
        true
    }

    /// The constituents of an intersection target the each-type arm can
    /// relate as they are: no union, and an instantiable constituent only
    /// where `isRelatedTo` would not normalize it first (a type parameter,
    /// a template literal, a string mapping, an unsimplifiable conditional).
    fn intersection_target_parts_explainable(&mut self, types: &[TypeId]) -> bool {
        for &part in types {
            let flags = self.type_of(part).flags;
            if flags.contains(TypeFlags::UNION) {
                return false;
            }
            if flags.intersects(TypeFlags::INSTANTIABLE)
                && !flags.intersects(
                    TypeFlags::TYPE_PARAMETER
                        | TypeFlags::TEMPLATE_LITERAL
                        | TypeFlags::STRING_MAPPING,
                )
                && !self.conditional_is_unsimplifiable(part)
            {
                return false;
            }
        }
        true
    }

    /// A conditional type `getSimplifiedConditionalType` leaves as it is:
    /// neither branch is `never`, so `isRelatedTo`'s normalization does not
    /// rewrite it (r7-reports §100).
    fn conditional_is_unsimplifiable(&mut self, ty: TypeId) -> bool {
        if !self.type_of(ty).flags.contains(TypeFlags::CONDITIONAL) {
            return false;
        }
        let Some([_, _, yes, no]) = self.conditional_inference_operands(ty) else {
            return false;
        };
        !self.type_of(yes).flags.contains(TypeFlags::NEVER)
            && !self.type_of(no).flags.contains(TypeFlags::NEVER)
    }

    /// The global `Object` interface's own declared type.
    fn is_global_object_type(&mut self, ty: TypeId) -> bool {
        matches!(self.type_of(ty).data, TypeData::Named { members: Some(owner), .. }
            if self.global_type_symbol_with_arity("Object", 0) == Some(owner))
            && !self.type_reference_targets.contains_key(&ty)
    }

    /// The enum symbol of a union-enum type (`getDeclaredTypeOfEnum`'s
    /// union, which carries the enum symbol).
    fn chain_enum_symbol(&self, ty: TypeId) -> Option<SymbolId> {
        let TypeData::Union { symbol: Some(symbol), .. } = &self.type_of(ty).data else {
            return None;
        };
        let symbol = self.binder.merged_symbol(*symbol);
        self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::ENUM).then_some(symbol)
    }

    /// `isEnumTypeRelatedTo` (`relater.go:282`) with its error reporter: the
    /// first member that fails, as its chain entry; `None` when the enums
    /// relate or fail without a report (different names, a const enum).
    fn enum_relation_failure(&mut self, source: SymbolId, target: SymbolId) -> Option<ChainEntry> {
        let symbols = self.binder.symbols();
        let (source_data, target_data) = (symbols.get(source), symbols.get(target));
        if source_data.name != target_data.name
            || !source_data.flags.contains(SymbolFlags::REGULAR_ENUM)
            || !target_data.flags.contains(SymbolFlags::REGULAR_ENUM)
        {
            return None;
        }
        let target_name = target_data.name.to_string();
        let mut members: Vec<(String, SymbolId)> = source_data
            .exports
            .as_ref()
            .map(|table| table.iter().map(|(name, &id)| (name.to_string(), id)).collect())
            .unwrap_or_default();
        members.sort_by_key(|&(_, id)| id);
        for (name, source_member) in members {
            let symbols = self.binder.symbols();
            if !symbols.get(source_member).flags.contains(SymbolFlags::ENUM_MEMBER) {
                continue;
            }
            let target_member = symbols
                .get(target)
                .exports
                .as_ref()
                .and_then(|table| table.get(name.as_str()).copied())
                .filter(|&member| symbols.get(member).flags.contains(SymbolFlags::ENUM_MEMBER));
            let Some(target_member) = target_member else {
                let declared = self.get_declared_type_of_symbol(target);
                let target_text = self.type_name_for_error_display(declared);
                return Some(ChainEntry {
                    message: &messages::PROPERTY_0_IS_MISSING_IN_TYPE_1,
                    args: vec![name, target_text],
                });
            };
            let source_type = self.get_declared_type_of_symbol(source_member);
            let target_type = self.get_declared_type_of_symbol(target_member);
            // `computeEnumMemberValue`: in an ambient non-const enum a member
            // without an initializer is computed, its value unknown.
            let ambient_computed = |checker: &Self, member: SymbolId| {
                let Some(declaration) = checker.binder.symbols().get(member).value_declaration
                else {
                    return false;
                };
                let Some(Node::EnumMember(node)) = checker.node_map.get(declaration) else {
                    return false;
                };
                node.initializer.is_none()
                    && checker.nodes.parent(declaration).is_some_and(|enumeration| {
                        checker.is_in_ambient_context(enumeration)
                            && !matches!(checker.node_map.get(enumeration),
                                Some(Node::EnumDeclaration(declaration))
                                    if declaration.modifiers.iter().any(|modifier| matches!(modifier,
                                        tsr_ast::ModifierLike::Token(token)
                                            if token.kind == SyntaxKind::ConstKeyword)))
                    })
            };
            let source_unknown = ambient_computed(self, source_member);
            let target_unknown = ambient_computed(self, target_member);
            let value = |checker: &Self, ty: TypeId| {
                checker.enum_member_payload(ty).map(|(_, value)| match value {
                    crate::types::EnumLiteralValue::Number(text) => (false, text.clone()),
                    crate::types::EnumLiteralValue::String(text) => {
                        (true, crate::printing::quote(text))
                    }
                })
            };
            let source_value = if source_unknown { None } else { value(self, source_type) };
            let target_value = if target_unknown { None } else { value(self, target_type) };
            if source_value == target_value {
                continue;
            }
            match (source_value, target_value) {
                (Some((_, source_text)), Some((_, target_text))) => {
                    return Some(ChainEntry {
                        message: &messages::EACH_DECLARATION_OF_0_1_DIFFERS_IN_ITS_VALUE_WHERE_2_WAS_EXPECTED_BUT_3_WAS_GIVEN,
                        args: vec![target_name, name, target_text, source_text],
                    });
                }
                (Some((true, text)), None) | (None, Some((true, text))) => {
                    return Some(ChainEntry {
                        message: &messages::ONE_VALUE_OF_0_1_IS_THE_STRING_2_AND_THE_OTHER_IS_ASSUMED_TO_BE_AN_UNKNOWN_NUMERIC_VALUE,
                        args: vec![target_name, name, text],
                    });
                }
                // An unknown value beside a numeric one is assumed equal;
                // two unknown values compare equal above.
                _ => {}
            }
        }
        None
    }

    /// `reportErrorResults` (`relater.go:4700`) names the pair `isRelatedTo`
    /// normalized unless the original has an alias: an indexed access whose
    /// object is a mapped type is printed as its substitution
    /// (`Partial<T>[keyof T]` reads as `T[keyof T] | undefined`; as a target,
    /// written to, `Readonly<U>[K]` is `U[K]`). An alias instantiation, which
    /// the port records as a reference target, keeps its own name
    /// (r7-reports §83).
    fn normalized_head_pair(
        &mut self,
        source: TypeId,
        target: TypeId,
        display_target: TypeId,
    ) -> (TypeId, TypeId, TypeId) {
        let normalize = |checker: &mut Self, ty: TypeId, writing: bool| {
            if !checker.deferred_indexed_access_types.contains_key(&ty)
                || checker.type_reference_targets.contains_key(&ty)
            {
                return ty;
            }
            checker.chain_simplified_indexed_access_for(ty, writing)
        };
        let source = normalize(self, source, false);
        if target != display_target {
            return (source, target, display_target);
        }
        let target = normalize(self, target, true);
        (source, target, target)
    }

    /// `reportRelationError`'s missing-property suppression (`relater.go:4816`)
    /// for a retraced chain: when its outermost message is the pair's own
    /// TS2741/TS2739/TS2740 (`chainArgsMatch` on the printed source and
    /// target), the head is never pushed and that message is the report,
    /// unless the head is a conversion or implementation message (§45).
    fn suppressed_head_chain(
        &mut self,
        chain: Option<&Diagnostic>,
        source: TypeId,
        target: TypeId,
        head: Option<&'static tsr_diagnostics::Message>,
    ) -> Option<Diagnostic> {
        let chain = chain?;
        if head.is_some_and(is_conversion_or_interface_implementation_message) {
            return None;
        }
        let missing = [
            messages::PROPERTY_0_IS_MISSING_IN_TYPE_1_BUT_REQUIRED_IN_TYPE_2.code(),
            messages::TYPE_0_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_1_COLON_2.code(),
            messages::TYPE_0_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_1_COLON_2_AND_3_MORE
                .code(),
        ];
        if !missing.contains(&chain.message.code()) {
            return None;
        }
        let (source_index, target_index) =
            if chain.message.code() == missing[0] { (1, 2) } else { (0, 1) };
        let source_text = self.type_to_string(source);
        let target_text = self.type_to_string(target);
        (chain.args.get(source_index) == Some(&source_text)
            && chain.args.get(target_index) == Some(&target_text))
        .then(|| chain.clone())
    }

    /// Report `diagnostic` as the relation's head, with the head pair's
    /// `This type parameter might need…` hint appended: its own
    /// `reportErrorResults` runs last, so the hint follows every inner
    /// level's related information (r7-reports §39).
    fn report_relation_head_diagnostic(
        &mut self,
        file: NodeId,
        source: TypeId,
        target: TypeId,
        mut diagnostic: Diagnostic,
    ) {
        // Every chain level carries the walk's related information
        // (`createDiagnosticChainFromErrorChain`), the head's hint included.
        #[allow(clippy::items_after_statements)]
        fn append(diagnostic: &mut Diagnostic, hint: &Diagnostic) {
            let mut list = diagnostic.related_information().to_vec();
            list.push(hint.clone());
            diagnostic.set_related_information(std::sync::Arc::new(list));
            let mut children = diagnostic.message_chain().to_vec();
            for child in &mut children {
                append(child, hint);
            }
            diagnostic.set_message_chain(children);
        }
        if let Some(Some(hint)) = self.type_parameter_constraint_hint(source, target) {
            append(&mut diagnostic, &hint);
        }
        self.report(file, diagnostic);
    }

    /// `reportRelationError`'s head (`relater.go:4751`) for
    /// [`Checker::report_relation_failure`]: the caller's message, else the
    /// TS2322 family, with the relation's signature elaboration or the
    /// missing-property message [`Checker::missing_property_chain`] kept
    /// under it.
    #[allow(clippy::too_many_arguments)]
    fn report_relation_head(
        &mut self,
        at: NodeId,
        file: NodeId,
        span: tsr_core::Span,
        source: TypeId,
        target: TypeId,
        head: Option<&'static tsr_diagnostics::Message>,
        signature_error: Option<Diagnostic>,
        chain: Option<Diagnostic>,
    ) {
        let displayed_source = self.assignability_source_for_error_display(source, target);
        let mut source_text = self.type_to_string(displayed_source);
        let mut target_text = self.type_to_string(target);
        // `getTypeNamesForErrorDisplay` (`relater.go:1277`): when the two
        // print alike, both are re-printed fully qualified
        // (`getTypeNameForErrorDisplay`), and `reportRelationError`'s
        // `sourceType == targetType` arm (TS2719) asks whether even those
        // collide: two different types with one name.
        let mut same_name = false;
        if self.type_to_string(source) == target_text {
            let qualified_source = self.type_name_for_error_display(source);
            let qualified_target = self.type_name_for_error_display(target);
            same_name = qualified_source == qualified_target;
            if displayed_source == source {
                source_text = qualified_source;
            }
            target_text = qualified_target;
        }
        let message = if let Some(head) = head {
            head
        } else if self.exact_optional_property_assignment_mismatch(at, source) {
            &messages::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1_WITH_EXACTOPTIONALPROPERTYTYPES_COLON_TRUE_CONSIDER_ADDING_UNDEFINED_TO_THE_TYPE_OF_THE_TARGET
        } else if same_name {
            &messages::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1_TWO_DIFFERENT_TYPES_WITH_THIS_NAME_EXIST_BUT_THEY_ARE_UNRELATED
        } else if source_text != target_text && self.exact_optional_object_mismatch(source, target)
        {
            &messages::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1_WITH_EXACTOPTIONALPROPERTYTYPES_COLON_TRUE_CONSIDER_ADDING_UNDEFINED_TO_THE_TYPES_OF_THE_TARGET_S_PROPERTIES
        } else {
            &messages::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1
        };
        // `reportRelationError`'s default arm (`relater.go:4790`): a string
        // literal source against a union target whose string-literal members
        // hold a spelling suggestion is TS2820 instead, and the report ends
        // there — before the type-parameter elaboration and the chain
        // suppressions below it.
        if head.is_none()
            && std::ptr::eq(message, &raw const messages::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1)
            && let Some(suggestion) =
                self.suggested_type_for_nonexistent_string_literal_type(source, target)
        {
            let suggestion = self.type_to_string(suggestion);
            let mut diagnostic = Diagnostic::with_args(
                &messages::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1_DID_YOU_MEAN_2,
                span,
                [source_text, target_text, suggestion],
            );
            if let Some(mut signature_error) = signature_error {
                signature_error.span = span;
                diagnostic.add_message_chain(Some(signature_error));
            }
            add_relation_chain(&mut diagnostic, chain);
            self.attach_chain_file(at, span, &mut diagnostic);
            self.report_relation_head_diagnostic(file, source, target, diagnostic);
            return;
        }
        let mut diagnostic =
            self.relation_diagnostic(span, source, target, message, source_text, target_text);
        if let Some(mut signature_error) = signature_error {
            signature_error.span = span;
            diagnostic.add_message_chain(Some(signature_error));
        }
        add_relation_chain(&mut diagnostic, chain);
        self.attach_chain_file(at, span, &mut diagnostic);
        self.report_relation_head_diagnostic(file, source, target, diagnostic);
    }

    /// `NewDiagnosticChain` gives every chain level the error node's file
    /// (`createDiagnosticChainFromErrorChain`); a chain child built here
    /// without one takes the head's (r7-reports §36).
    pub(crate) fn attach_chain_file(
        &self,
        at: NodeId,
        span: tsr_core::Span,
        diagnostic: &mut Diagnostic,
    ) {
        if diagnostic.message_chain().is_empty() {
            return;
        }
        let Some(file) = self
            .related_diagnostic_at(at, diagnostic.message, span, Vec::new())
            .file()
            .map(|file| std::sync::Arc::new(file.clone()))
        else {
            return;
        };
        #[allow(clippy::items_after_statements)]
        fn deep(
            child: &Diagnostic,
            file: &std::sync::Arc<tsr_diagnostics::format::DiagnosticFile>,
        ) -> Diagnostic {
            let mut copy = child.clone();
            if copy.file().is_none() {
                copy.set_file(file.clone());
            }
            let children =
                child.message_chain().iter().map(|grandchild| deep(grandchild, file)).collect();
            copy.set_message_chain(children);
            copy
        }
        let children: Vec<Diagnostic> =
            diagnostic.message_chain().iter().map(|child| deep(child, &file)).collect();
        // `NewDiagnosticChain` copies the child's related information up.
        if diagnostic.related_information().is_empty()
            && let Some(first) = children.first()
            && !first.related_information().is_empty()
        {
            let related = std::sync::Arc::new(first.related_information().to_vec());
            diagnostic.set_related_information(related);
        }
        diagnostic.set_message_chain(children);
    }

    /// `getSuggestedTypeForNonexistentStringLiteralType` (`checker.go:27252`):
    /// the target union's string-literal member closest in spelling to a
    /// string-literal source, `GetSpellingSuggestionWithMaxCandidateCount`
    /// with at most 1,000 candidates (more answers nil). Ties keep the
    /// earlier member in union order, which is `CompareTypes` order.
    fn suggested_type_for_nonexistent_string_literal_type(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<TypeId> {
        let source_type = self.type_of(source);
        if !source_type.flags.contains(TypeFlags::STRING_LITERAL)
            || !self.type_of(target).flags.contains(TypeFlags::UNION)
        {
            return None;
        }
        let literal_text = |data: &TypeData| match data {
            TypeData::StringLiteral(text)
            | TypeData::EnumLiteral {
                value: crate::types::EnumLiteralValue::String(text), ..
            } => Some(text.clone()),
            _ => None,
        };
        let name = literal_text(&source_type.data)?;
        let TypeData::Union { types, .. } = &self.type_of(target).data else { return None };
        let candidates: Vec<(usize, TypeId, String)> = types
            .iter()
            .filter(|&&member| self.type_of(member).flags.contains(TypeFlags::STRING_LITERAL))
            .filter_map(|&member| {
                literal_text(&self.type_of(member).data).map(|text| (member, text))
            })
            .enumerate()
            .map(|(index, (member, text))| (index, member, text))
            .collect();
        if candidates.len() > 1000 {
            return None;
        }
        tsr_core::spelling::get_spelling_suggestion(
            &name,
            candidates.iter(),
            |(_, _, text)| text.as_str(),
            |(left, _, _), (right, _, _)| left.cmp(right),
        )
        .map(|(_, member, _)| *member)
    }

    /// [`Checker::report_relation_failure`] for a JSX attributes source
    /// (`ObjectFlagsJsxAttributes`, which TSR's types do not carry: the JSX
    /// caller states it). `reportErrorResults` (`relater.go:4722`) returns
    /// without an outer head when the target is an intersection holding
    /// `JSX.IntrinsicAttributes` or `JSX.IntrinsicClassAttributes` (both
    /// `getJsxType`s non-error), so only the chain of the constituent that
    /// failed is reported.
    ///
    /// That constituent is the first one `typeRelatedToEachType`
    /// (`relater.go`) fails, related under `IntersectionStateTarget`: no
    /// excess-property check (the source is read regular; the caller already
    /// ran `hasExcessProperties` on the whole target) and no common-property
    /// check (a weak constituent sharing no property with the source has no
    /// required member and no signature or index to fail, so it relates).
    /// An undecided constituent, or every constituent relating (the failure
    /// was the combined property pass, `relater.go:3232`), reports nothing.
    pub(crate) fn report_jsx_attributes_relation_failure(
        &mut self,
        at: NodeId,
        span: tsr_core::Span,
        location: NodeId,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let TypeData::Intersection { types, .. } = self.type_of(target).data.clone() else {
            return self.report_relation_failure(at, span, None, source, target, None);
        };
        let intrinsic = |checker: &mut Self, name: &str| {
            checker
                .jsx_type_symbol(location, name)
                .filter(|&symbol| {
                    checker.binder.symbols().get(symbol).flags.intersects(SymbolFlags::TYPE)
                })
                .map(|symbol| checker.get_declared_type_of_symbol(symbol))
                .filter(|&declared| !checker.is_error(declared))
        };
        let (Some(attributes), Some(class_attributes)) =
            (intrinsic(self, "IntrinsicAttributes"), intrinsic(self, "IntrinsicClassAttributes"))
        else {
            return self.report_relation_failure(at, span, None, source, target, None);
        };
        if !types.contains(&attributes) && !types.contains(&class_attributes) {
            return self.report_relation_failure(at, span, None, source, target, None);
        }
        let regular = self.get_regular_type_of_object_literal(source);
        // `isRelatedTo`'s common-property check runs on the whole target
        // before the intersection is decomposed (`relater.go:2676`): a weak
        // intersection sharing no property with the attributes is TS2559 on
        // its own (`tsxSpreadAttributesResolution5` (35,10); r7-reports §32).
        if self.report_weak_type_failure(at, span, regular, target) {
            return true;
        }
        for constituent in types {
            if self.fails_common_property_check(regular, constituent) {
                continue;
            }
            match self.relate_ternary(regular, constituent, crate::relater::Relation::Assignable) {
                crate::relater::Ternary::Related => {}
                crate::relater::Ternary::NotRelated => {
                    return self.report_relation_failure(
                        at,
                        span,
                        None,
                        regular,
                        constituent,
                        None,
                    );
                }
                crate::relater::Ternary::Unknown => return false,
            }
        }
        false
    }

    /// Ported from typescript-go's `Relater.reportRelationError`
    /// (`internal/checker/relater.go`): the type-parameter explanation branch.
    /// Native also launches Checker-level assignability queries here, with no
    /// error node, first for generalized source and then for original source.
    /// These queries choose the explanation, never re-decide the outer failure.
    /// Callers retain the original Checker-local source/target identities after
    /// their failed assignable relation; display generalization changes no key.
    /// Uses existing constraint publication and relation workers, not a second
    /// member walk or semantic cache. This direct-target port has no recursive
    /// Relater errorChain to preserve/reset; it does not certify those consumers.
    fn relation_diagnostic(
        &mut self,
        span: tsr_core::Span,
        source: TypeId,
        target: TypeId,
        message: &'static tsr_diagnostics::Message,
        source_text: String,
        target_text: String,
    ) -> Diagnostic {
        let target_flags = self.type_of(target).flags;
        let explains_type_parameter = if target_flags.contains(TypeFlags::INDEXED_ACCESS) {
            self.indexed_access_target_explained(source, target) == Some(true)
        } else {
            target_flags.contains(TypeFlags::TYPE_PARAMETER)
        };
        if !explains_type_parameter || self.variance_marker_types.contains(&target) {
            return Diagnostic::with_args(message, span, [source_text, target_text]);
        }
        let generalized = self.assignability_source_for_error_display(source, target);
        let constraint = self.base_constraint_of_type(target);
        let assignable_source = constraint.and_then(|constraint| {
            if self.is_type_assignable_to(generalized, constraint) {
                Some((generalized, constraint))
            } else if self.is_type_assignable_to(source, constraint) {
                Some((source, constraint))
            } else {
                None
            }
        });
        let child = if let Some((source, constraint)) = assignable_source {
            Diagnostic::with_args(
                &messages::_0_IS_ASSIGNABLE_TO_THE_CONSTRAINT_OF_TYPE_1_BUT_1_COULD_BE_INSTANTIATED_WITH_A_DIFFERENT_SUBTYPE_OF_CONSTRAINT_2,
                span,
                [self.type_to_string(source), target_text.clone(), self.type_to_string(constraint)],
            )
        } else {
            Diagnostic::with_args(
                &messages::_0_COULD_BE_INSTANTIATED_WITH_AN_ARBITRARY_TYPE_WHICH_COULD_BE_UNRELATED_TO_1,
                span,
                [target_text.clone(), self.type_to_string(generalized)],
            )
        };
        let mut diagnostic = Diagnostic::with_args(message, span, [source_text, target_text]);
        diagnostic.add_message_chain(Some(child));
        diagnostic
    }

    /// `reportRelationError`'s target-flag choice (`relater.go:4760`): an
    /// indexed-access target against a source that is not one is explained
    /// by its object type's flags, so `T[K]` with a type-parameter `T` gets
    /// the type-parameter explanation over `getBaseConstraintOfType(T[K])`.
    /// `Some(false)` where no explanation applies, `None` where the target is
    /// one `isRelatedTo` would first normalize (a mapped, alias or nested
    /// object), which this retrace does not (r7-reports §81).
    fn indexed_access_target_explained(&mut self, source: TypeId, target: TypeId) -> Option<bool> {
        let &(object, _, _) = self.deferred_indexed_access_types.get(&target)?;
        self.ensure_mapped_type_info(object);
        if self.mapped_types.contains_key(&object)
            || self.type_reference_targets.contains_key(&object)
            || self.deferred_indexed_access_types.contains_key(&object)
        {
            return None;
        }
        if self.type_of(source).flags.contains(TypeFlags::INDEXED_ACCESS) {
            return Some(false);
        }
        Some(self.type_of(object).flags.contains(TypeFlags::TYPE_PARAMETER))
    }

    /// An object literal against a union target, up to the outer report:
    /// `hasExcessProperties` (`relater.go:2714`) ahead of the relation, then
    /// `elaborateObjectLiteral` (`relater.go:498`) for a failed one, then the
    /// excess member's TS2353/TS2561. `Settled(reported)` ends the check
    /// (related, undecidable, elaborated or excess); `Outer` leaves the
    /// whole-expression report to the caller, `excess` saying the relation
    /// failed in `hasExcessProperties`' member-type arm.
    fn union_object_literal_failure(
        &mut self,
        node: NodeId,
        source: TypeId,
        target: TypeId,
    ) -> UnionLiteralFailure {
        let Some(excess) = self.excess_properties_verdict(node, source, target) else {
            return UnionLiteralFailure::Settled(false);
        };
        let excess_failed = !matches!(excess, ExcessProperties::None);
        if !excess_failed
            && self.relate_ternary(source, target, crate::relater::Relation::Assignable)
                != crate::relater::Ternary::NotRelated
        {
            return UnionLiteralFailure::Settled(false);
        }
        match self.elaborate_object_literal_members(node, source, target) {
            Some(true) => return UnionLiteralFailure::Settled(true),
            Some(false) => {}
            None => return UnionLiteralFailure::Settled(false),
        }
        if let ExcessProperties::Excess { at, name, error_target } = excess {
            return UnionLiteralFailure::Settled(self.report_excess_property(
                at,
                &name,
                error_target,
            ));
        }
        UnionLiteralFailure::Outer { excess: excess_failed }
    }

    /// `getExactOptionalUnassignableProperties` (`checker.go:13115`): inspect
    /// corresponding read members, not assignability or optional symbol flags.
    /// This port resolves instantiated members through the concrete receiver
    /// rather than native's instantiated property symbols. As in
    /// `containsMissingType`, explicit undefined ahead of missing admits it.
    fn exact_optional_object_mismatch(&mut self, source: TypeId, target: TypeId) -> bool {
        if !self.exact_optional_property_types
            || (self.tuple_element_lists.contains_key(&source)
                && self.tuple_element_lists.contains_key(&target))
        {
            return false;
        }
        let Some(names) = self.get_property_names_of_type(target) else { return false };
        names.iter().any(|name| {
            let Some(source_property) = self.get_type_of_property_of_type(source, name) else {
                return false;
            };
            if !self.maybe_type_of_kind(source_property, TypeFlags::UNDEFINED) {
                return false;
            }
            let Some(target_property) = self.get_type_of_property_of_type(target, name) else {
                return false;
            };
            target_property == self.intrinsics.missing
                || matches!(&self.type_of(target_property).data, TypeData::Union { types, .. }
                    if types.first() == Some(&self.intrinsics.missing))
        })
    }

    /// `checkAssignmentOperator` (`checker.go:12777`): the TS2412 head message
    /// reads the concrete receiver's property before the write type removes
    /// missing. `containsMissingType` (`checker.go:1250`) tests the first union
    /// constituent: explicit undefined can precede missing and admits undefined.
    fn exact_optional_property_assignment_mismatch(&mut self, at: NodeId, source: TypeId) -> bool {
        if !self.exact_optional_property_types
            || !self.maybe_type_of_kind(source, TypeFlags::UNDEFINED)
        {
            return false;
        }
        let Some(Node::PropertyAccessExpression(access)) = self.node_map.get(at) else {
            return false;
        };
        let (Some(receiver), Some(name)) = (access.expression, access.name) else {
            return false;
        };
        let name = match name {
            tsr_ast::MemberName::Identifier(name) => name.text,
            tsr_ast::MemberName::PrivateIdentifier(name) => name.text,
        };
        let receiver = self.check_expression(receiver);
        let Some(property_type) = self.get_type_of_property_of_type(receiver, name) else {
            return false;
        };
        // `containsMissingType`: the missing type anywhere in the union
        // (r7-reports §94).
        property_type == self.intrinsics.missing
            || matches!(&self.type_of(property_type).data, TypeData::Union { types, .. }
                if types.contains(&self.intrinsics.missing))
    }

    /// `reportRelationError`: generalize literal source names only for targets
    /// that cannot contain top-level singleton types. This changes the display,
    /// never the types passed to the relation.
    fn assignability_source_for_error_display(&mut self, source: TypeId, target: TypeId) -> TypeId {
        let source_type = self.type_of(source);
        let is_literal = source_type.flags.intersects(TypeFlags::BOOLEAN | TypeFlags::UNIT)
            || matches!(&source_type.data, TypeData::Union { types, .. }
                if types.iter().all(|&ty| self.type_of(ty).flags.intersects(TypeFlags::UNIT)));
        if !is_literal
            || self.type_of(target).flags.contains(TypeFlags::NEVER)
            || self.type_could_have_top_level_singleton_types(target, &mut Vec::new())
        {
            source
        } else {
            self.get_base_type_of_literal_type(source)
        }
    }

    /// `typeCouldHaveTopLevelSingletonTypes` (`relater.go:1305`). Constraint
    /// resolution stays in this checker's existing constraint domain.
    fn type_could_have_top_level_singleton_types(
        &mut self,
        target: TypeId,
        active: &mut Vec<TypeId>,
    ) -> bool {
        let flags = self.type_of(target).flags;
        if flags.contains(TypeFlags::BOOLEAN) {
            return false;
        }
        let singleton = flags
            .intersects(TypeFlags::UNIT | TypeFlags::TEMPLATE_LITERAL | TypeFlags::STRING_MAPPING);
        if active.contains(&target) {
            return singleton;
        }
        active.push(target);
        let result = if let TypeData::Union { types, .. } | TypeData::Intersection { types, .. } =
            self.type_of(target).data.clone()
        {
            types.iter().any(|&ty| self.type_could_have_top_level_singleton_types(ty, active))
        } else if flags.intersects(TypeFlags::INSTANTIABLE)
            && let Some(constraint) = self.base_constraint_of_type(target)
            && constraint != target
        {
            self.type_could_have_top_level_singleton_types(constraint, active)
        } else {
            singleton
        };
        active.pop();
        result
    }

    /// The declines that survive `relate_ternary` — situations where the
    /// relation answers a confident `NotRelated` that upstream would not.
    ///
    /// # What changed, and why this list is now short
    ///
    /// This gate used to admit **primitives only**, because the rule read
    /// `is_type_assignable_to` and that binary projection collapses
    /// `Ternary::Unknown` — *"this port cannot decide"* — into `false`. Every
    /// undecidable pair was reported as an error, which is the whole of the
    /// 988-wrong measurement in §16's build 0, and the primitives gate was the
    /// bound that made it survivable.
    ///
    /// `relate_ternary` reports the negative only when the relater is
    /// **entitled** to say so, which is precisely what
    /// `crate::relater::Ternary`'s own doc comment says it exists for. With it
    /// the gate no longer has to model the relation's incompleteness at all, and
    /// what is left is three places where the relater is confidently wrong
    /// rather than undecided:
    ///
    /// - **`unknown` on either side.** Its arms are not ported and it produced
    ///   25 wrong lines in one case (`conformance/unknownType2`) — the largest
    ///   single family after the switch.
    /// - **An object literal against a union target.** That is the
    ///   excess-property and discriminated-union machinery
    ///   (`getMatchingUnionConstituentForObjectLiteral`,
    ///   `findMatchingDiscriminantType`), and upstream reports TS2353 / TS2561 /
    ///   TS2739 there. 38 wrong lines across six cases —
    ///   `excessPropertyCheckWithUnions`, `assignmentCompatWithDiscriminatedUnion`,
    ///   both `missingDiscriminants`, and two more.
    /// - **`any` and the error type**, unchanged: neither can fail a relation,
    ///   so admitting them can only produce accidents.
    ///
    /// The **enum** veto was added when `isSimpleTypeRelatedTo`'s enum arms
    /// (`relater.go:206`) were unported. They are ported now (`relater.rs`'s
    /// member-level arms), and the assignability reporters ask
    /// [`Checker::assignability_pair_is_reportable`], which lifts the enum veto.
    /// The veto stays for the other rules sharing this gate (operators,
    /// comparisons, heritage, assertions), whose own enum-literal handling — e.g.
    /// `getBaseTypeOfLiteralTypeForComparison` for relational operators — is not
    /// ported (`mixedTypeEnumComparison`).
    pub(crate) fn pair_is_reportable(&mut self, source: TypeId, target: TypeId) -> bool {
        self.assignability_pair_is_reportable(source, target)
            && ![source, target].iter().any(|&side| {
                self.type_of(side).flags.intersects(TypeFlags::ENUM | TypeFlags::ENUM_LITERAL)
            })
    }

    /// [`Checker::pair_is_reportable`] without the enum veto: the gate for
    /// `checkTypeRelatedTo`'s own reporters (TS2322/TS2345 and their
    /// elaborations), whose verdict comes from the relater's ported enum arms.
    pub(crate) fn assignability_pair_is_reportable(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let intrinsics = self.intrinsics();
        let (unknown, any) = (intrinsics.unknown, intrinsics.any);
        for side in [source, target] {
            // An exact `unknown` SOURCE is upstream's answer where the
            // instantiation road now computes it (`getNoInferType`,
            // `getIndexedAccessTypeEx`'s nil-node `unknownType`); the veto
            // below stays for flagged look-alikes and every target.
            // Measured: no corpus verdict changes (tsr-2zk.44).
            if side == source && side == unknown {
                continue;
            }
            // `any` relates to every target but `never` (`isSimpleTypeRelatedTo`
            // admits an `any` source only against a non-`never` target), so an
            // `any` source against `never` is a definite failure whatever this
            // port's `any` stands for: `x1[k] = 'bar' as any` into a reduced
            // `never` property is TS2322 natively (`intersectionReduction`).
            if side == source
                && side == any
                && self.type_of(target).flags.contains(TypeFlags::NEVER)
            {
                continue;
            }
            // `Checker::is_error` and not `== intrinsics.error`: an unresolved
            // type REFERENCE mints a `TypeData::Named` carrying the written
            // text and answers `is_error` without being that intrinsic
            // (`checker-notes-diag2.md` §44). It is the same accident the arm
            // above describes, under a different type id — a type the port
            // could not build, which the relater cannot distinguish from one
            // that failed.
            if self.is_gap(side) || side == any {
                return false;
            }
            // The `unknown` SOURCE is a real type that fails real relations
            // (`unknown` to `string` is TS2322 upstream, `isSimpleTypeRelatedTo`
            // relates it only to `any`/`unknown` targets and through the
            // `{} | null | undefined` arm). A callback parameter typed by
            // `getIndexedAccessTypeEx`'s nil answer (`checker.go:26930`) is
            // such a source. An `unknown` target relates every source.
            if side == unknown && side == source {
                continue;
            }
            if side == unknown
                || self.type_of(side).flags.intersects(TypeFlags::ANY | TypeFlags::UNKNOWN)
            {
                return false;
            }
        }
        // `getMatchingUnionConstituentForObjectLiteral` and the discriminant
        // machinery decide this pair upstream, and neither is ported.
        true
    }

    /// An **object** source against a **primitive** target — a definite
    /// negative the relater declines to give.
    ///
    /// `is_related_to` answers `Unknown` for a pair whose source is an object
    /// type with no members table (a function type, an index-signature-only
    /// type) because its structural arm was never reached — row 3 of
    /// `checker-notes-assign.md` §2, and correct as a statement about the
    /// *structural* comparison.
    ///
    /// But nothing structured is assignable to `string`, and no members table is
    /// needed to know it: `let x1: string = demoNS.f` is TS2322 whatever `f`'s
    /// shape turns out to be. This is asked here rather than added to
    /// `crate::relater` deliberately — the relater is read by overload selection
    /// and by narrowing, and widening what it calls a definite negative moves
    /// `checker_types`. As a rule-local decision it moves nothing else.
    ///
    /// **One direction only.** The converse is false: `let x: {} = 5` is legal,
    /// because an object *target* can be satisfied by a primitive through its
    /// apparent type.
    pub(crate) fn object_against_primitive(&mut self, source: TypeId, target: TypeId) -> bool {
        if self.type_of(source).flags.intersects(TypeFlags::OBJECT)
            && self.type_of(target).flags.intersects(PRIMITIVE_TARGET)
        {
            return true;
        }
        // The converse, and it needs a condition. `let x: {} = 5` is legal
        // because an empty object target is satisfied through the primitive's
        // apparent type; `let x: { a: number } = 5` is not, because a number has
        // no `a`. So a primitive source is a definite negative against an object
        // target **that requires at least one property**.
        self.type_of(source).flags.intersects(PRIMITIVE_TARGET)
            && self
                .declared_property_table(target)
                .is_some_and(|table| table.iter().any(|(_, optional)| !optional))
    }

    /// `elaborateError` (`relater.go:440`): descend into the source expression
    /// to report on the innermost node that explains the failure. Answers
    /// whether it reported; the caller then stays silent. A generic conditional
    /// target is not elaborated. `head` is the caller's head message, which
    /// only `elaborateDidYouMeanToCallOrConstruct` reports with (TS2345 at an
    /// argument); the member arms report their own TS2322.
    fn elaborate_error(
        &mut self,
        node: NodeId,
        source: TypeId,
        target: TypeId,
        head: Option<&'static tsr_diagnostics::Message>,
    ) -> bool {
        if self.is_or_has_generic_conditional(target) {
            return false;
        }
        if self.elaborate_did_you_mean_to_call_or_construct(
            node,
            source,
            target,
            crate::signatures::SignatureKind::Construct,
            head,
        ) || self.elaborate_did_you_mean_to_call_or_construct(
            node,
            source,
            target,
            crate::signatures::SignatureKind::Call,
            head,
        ) {
            return true;
        }
        let inner = match self.node_map.get(node) {
            Some(Node::ParenthesizedExpression(parenthesized)) => parenthesized.expression,
            Some(Node::AsExpression(assertion))
                if assertion.r#type.is_some_and(crate::assertions::is_const_type_reference) =>
            {
                assertion.expression
            }
            Some(Node::BinaryExpression(binary))
                if binary.operator_token.is_some_and(|token| {
                    matches!(token.kind, SyntaxKind::EqualsToken | SyntaxKind::CommaToken)
                }) =>
            {
                binary.right
            }
            Some(Node::ObjectLiteralExpression(_)) => {
                return self.elaborate_object_literal(node, source, target);
            }
            // checkTypeRelatedToAndOptionallyElaborate elaborates only a failed
            // relation. A member mismatch need not fail the whole (a `void`
            // target return accepts any source return), so these arms ask.
            Some(Node::ArrayLiteralExpression(_)) => {
                return self.relate_ternary(source, target, crate::relater::Relation::Assignable)
                    == crate::relater::Ternary::NotRelated
                    && self.elaborate_array_literal(node, source, target);
            }
            Some(Node::ArrowFunction(_)) => {
                return self.relate_ternary(source, target, crate::relater::Relation::Assignable)
                    == crate::relater::Ternary::NotRelated
                    && self.elaborate_arrow_function(node, source, target);
            }
            _ => return false,
        };
        inner
            .and_then(|inner| inner.node_id())
            .is_some_and(|inner| self.elaborate_error(inner, source, target, head))
    }

    /// `elaborateDidYouMeanToCallOrConstruct` (`relater.go:480`): when some
    /// `kind` signature of the source returns a type (not `any`/`never`)
    /// related to the target, the failure is reported at the expression
    /// itself, with "Did you mean to call this expression?" (or `new`) as
    /// related information on its first diagnostic. A pair the relation
    /// does not reject falls through to the remaining arms.
    fn elaborate_did_you_mean_to_call_or_construct(
        &mut self,
        node: NodeId,
        source: TypeId,
        target: TypeId,
        kind: crate::signatures::SignatureKind,
        head: Option<&'static tsr_diagnostics::Message>,
    ) -> bool {
        let Some(signatures) = self.signatures_of_type_kind(source, kind) else { return false };
        let mut callable = false;
        for signature in &signatures {
            let Some(return_type) = self.get_return_type_of_signature(signature) else {
                continue;
            };
            if self.type_of(return_type).flags.intersects(TypeFlags::ANY | TypeFlags::NEVER) {
                continue;
            }
            if self.relate_ternary(return_type, target, crate::relater::Relation::Assignable)
                == crate::relater::Ternary::Related
            {
                callable = true;
                break;
            }
        }
        if !callable {
            return false;
        }
        let span = self.error_span(node);
        let before = self.diagnostics.len();
        if !self.report_relation_failure(node, span, None, source, target, head) {
            return false;
        }
        // `diags[0]` gains the hint at the expression (r7-reports §43).
        if self.diagnostics.len() > before {
            let message = if kind == crate::signatures::SignatureKind::Construct {
                &messages::DID_YOU_MEAN_TO_USE_NEW_WITH_THIS_EXPRESSION
            } else {
                &messages::DID_YOU_MEAN_TO_CALL_THIS_EXPRESSION
            };
            let hint = self.related_diagnostic_at(node, message, span, Vec::new());
            if hint.file().is_some() {
                let diagnostic = &mut self.diagnostics[before].1;
                let mut list = diagnostic.related_information().to_vec();
                list.push(hint);
                diagnostic.set_related_information(std::sync::Arc::new(list));
            }
        }
        true
    }

    /// `getBestMatchingType` (`relater.go:879`) for an object-literal source
    /// against a union. `Ok(None)` is upstream's nil; `Err` is a choice this
    /// port cannot make faithfully.
    ///
    /// - `findMatchingDiscriminantType`:
    ///   [`Checker::find_matching_discriminant_type`].
    /// - `findMatchingTypeReferenceOrTypeAliasReference` and
    ///   `findBestTypeForInvokable` never match a signature-less, alias-less
    ///   literal.
    /// - `findBestTypeForObjectLiteral`: with an array-like constituent, the
    ///   first constituent that is not array-like.
    /// - `findMostOverlappyType`: the last non-primitive constituent sharing
    ///   the most property names with the source (ties go to the later one,
    ///   `length >= matchingCount`); one sharing none is skipped.
    fn best_matching_type_for_object_literal(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Result<Option<TypeId>, ()> {
        let TypeData::Union { types, .. } = self.type_of(target).data.clone() else {
            return Ok(None);
        };
        let discriminated = self.find_matching_discriminant_type(source, target).ok_or(())?;
        if discriminated != target {
            return Ok(Some(discriminated));
        }
        let source_names = self.get_property_names_of_type(source).ok_or(())?;
        // `isArrayLikeType`: assignable to `readonly any[]`. A primitive is
        // not, and neither is an object type with no `length` member — the
        // target requires one — so the relation is asked only otherwise.
        let mut array_like = Vec::with_capacity(types.len());
        for &part in &types {
            let flags = self.type_of(part).flags;
            let is = if flags.intersects(TypeFlags::PRIMITIVE)
                || (flags.contains(TypeFlags::OBJECT)
                    && self.get_type_of_property_of_type(part, "length").is_none())
            {
                false
            } else {
                self.binding_parent_is_array_like(part).ok_or(())?
            };
            array_like.push(is);
        }
        if array_like.iter().any(|&is| is) {
            return Ok(types.iter().zip(&array_like).find(|(_, is)| !**is).map(|(&part, _)| part));
        }
        let mut best = None;
        let mut matching_count = 0usize;
        for &part in &types {
            let flags = self.type_of(part).flags;
            if flags.intersects(TypeFlags::PRIMITIVE) {
                continue;
            }
            if flags.intersects(TypeFlags::INSTANTIABLE) {
                return Err(());
            }
            // `getIntersectionType(keyof source, keyof target)`: the source's
            // literal keys that are the constituent's property names or
            // applicable index-signature keys (`isKnownProperty`'s question).
            let mut overlap = 0usize;
            for name in &source_names {
                if self.is_known_property(part, name).ok_or(())? {
                    overlap += 1;
                }
            }
            if overlap > 0 && overlap >= matching_count {
                best = Some(part);
                matching_count = overlap;
            }
        }
        Ok(best)
    }

    /// `isLiteralType` (`utilities.go`): `boolean`, a unit type, or a union
    /// whose every constituent is a unit type.
    fn is_literal_type_for_discriminant(&self, t: TypeId) -> bool {
        let ty = self.type_of(t);
        if ty.flags.intersects(TypeFlags::BOOLEAN) {
            return true;
        }
        match &ty.data {
            TypeData::Union { types, .. } => {
                types.iter().all(|&part| self.type_of(part).flags.intersects(TypeFlags::UNIT))
            }
            _ => ty.flags.intersects(TypeFlags::UNIT),
        }
    }

    /// `isOrHasGenericConditional` (`relater.go:474`).
    fn is_or_has_generic_conditional(&self, t: TypeId) -> bool {
        let ty = self.type_of(t);
        ty.flags.contains(TypeFlags::CONDITIONAL)
            || matches!(&ty.data, TypeData::Intersection { types, .. }
                if types.iter().any(|&part| self.is_or_has_generic_conditional(part)))
    }

    /// `elaborateElement` (`relater.go:546`) once its member types are known:
    /// a related pair is not elaborated; otherwise `next` elaborates first and
    /// the failure is reported on `prop` (excess properties of a fresh `next`
    /// first, as `checkTypeRelatedToEx` would meet them).
    fn elaborate_element(
        &mut self,
        prop: NodeId,
        next: Option<NodeId>,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        self.elaborate_element_with(prop, next, source, target, None)
    }

    /// [`Checker::elaborate_element`] with `elaborateElement`'s
    /// `errorMessage`, the head of the report at `prop`.
    fn elaborate_element_with(
        &mut self,
        prop: NodeId,
        next: Option<NodeId>,
        source: TypeId,
        target: TypeId,
        head: Option<&'static tsr_diagnostics::Message>,
    ) -> bool {
        // getBestMatchIndexedAccessTypeOrUndefined: no elaboration into an
        // index on a generic variable.
        if self.type_of(target).flags.contains(TypeFlags::INDEXED_ACCESS)
            || self.relate_ternary(source, target, crate::relater::Relation::Assignable)
                != crate::relater::Ternary::NotRelated
        {
            return false;
        }
        let source_node = next.unwrap_or(prop);
        let before = self.diagnostics.len();
        self.check_excess_properties(target, source_node);
        if self.diagnostics.len() != before {
            return true;
        }
        // `specificSource` (`relater.go:565`): the expression's own type
        // against the member's as contextual type; the member's type again
        // when that one relates.
        let source = match next.and_then(|next| self.elaboration_specific_source(next, source)) {
            Some(specific)
                if specific != source
                    && self.relate_ternary(
                        specific,
                        target,
                        crate::relater::Relation::Assignable,
                    ) == crate::relater::Ternary::NotRelated =>
            {
                specific
            }
            _ => source,
        };
        let span = self.error_span(prop);
        self.report_relation_failure(prop, span, Some(source_node), source, target, head)
    }

    /// `checkExpressionForMutableLocationWithContextualType(next, contextual)`
    /// for a literal-typed expression: kept (regular) in a const context,
    /// under an assertion, or when `isLiteralOfContextualType` holds, else
    /// widened. `None` for any other expression, whose type the caller's
    /// member type already is (r7-reports §41).
    fn elaboration_specific_source(&mut self, next: NodeId, contextual: TypeId) -> Option<TypeId> {
        let literalish = TypeFlags::STRING_LITERAL
            | TypeFlags::NUMBER_LITERAL
            | TypeFlags::BIG_INT_LITERAL
            | TypeFlags::BOOLEAN_LITERAL
            | TypeFlags::UNIQUE_ES_SYMBOL;
        let checked = self.check_expression_at_node(next);
        if !self.maybe_type_of_kind(checked, literalish) {
            return None;
        }
        let mut inner = next;
        while let Some(Node::ParenthesizedExpression(parenthesized)) = self.node_map.get(inner) {
            inner = parenthesized.expression.and_then(|expression| expression.node_id())?;
        }
        let assertion = matches!(
            self.nodes.kind(inner),
            SyntaxKind::AsExpression | SyntaxKind::TypeAssertionExpression
        );
        if assertion {
            return Some(checked);
        }
        if self.is_const_context(next)
            || self.is_literal_of_contextual_type(checked, contextual) == Some(true)
        {
            return Some(self.get_regular_type_of_literal_type(checked));
        }
        let widened = self.get_widened_literal_type_with_unions(checked);
        let widened = self.get_widened_unique_es_symbol_type(widened);
        Some(self.get_regular_type_of_literal_type(widened))
    }

    /// Whether a computed property name's expression is a string or numeric
    /// literal (the negation of `ast.IsComputedNonLiteralName`).
    fn computed_name_is_literal(&self, name: NodeId) -> bool {
        let Some(Node::ComputedPropertyName(computed)) = self.node_map.get(name) else {
            return false;
        };
        matches!(
            computed.expression,
            Some(
                tsr_ast::Expression::StringLiteral(_)
                    | tsr_ast::Expression::NumericLiteral(_)
                    | tsr_ast::Expression::NoSubstitutionTemplateLiteral(_)
            )
        )
    }

    /// `elaborateObjectLiteral` (`relater.go:498`): each named member is an
    /// element — a property assignment elaborates into its initializer, and
    /// shorthand, method and accessor members report at their name.
    ///
    /// Returns whether it reported, which is upstream's contract: a `true` here
    /// is what stops the whole-expression diagnostic being issued at all.
    fn elaborate_object_literal(
        &mut self,
        source_node: NodeId,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        self.elaborate_object_literal_members(source_node, source, target).unwrap_or(false)
    }

    /// [`Checker::elaborate_object_literal`], answering `None` where this port
    /// cannot decide what upstream would elaborate: a union member whose `getBestMatchIndexedAccessTypeOrUndefined` needs a
    /// `getBestMatchingType` choice this port cannot make. Every member's
    /// target type is settled before anything is reported, so a `None` never
    /// follows a partial report.
    fn elaborate_object_literal_members(
        &mut self,
        source_node: NodeId,
        source: TypeId,
        target: TypeId,
    ) -> Option<bool> {
        let Some(Node::ObjectLiteralExpression(literal)) = self.node_map.get(source_node) else {
            return Some(false);
        };
        // `target.flags&(TypeFlagsPrimitive|TypeFlagsNever) != 0` — a primitive
        // or `never` target has no properties to elaborate against, and
        // upstream returns before the loop.
        if self.type_of(target).flags.intersects(TypeFlags::PRIMITIVE | TypeFlags::NEVER) {
            return Some(false);
        }
        // A spread member is skipped (`ast.IsSpreadAssignment`); each written
        // member's source type is read from the final literal type, so a key a
        // later spread overrides compares the spread's property.
        let is_union = self.type_of(target).flags.contains(TypeFlags::UNION);
        let mut members = Vec::with_capacity(literal.properties.len());
        for property in literal.properties {
            let (name, next) = match property {
                tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                    let Some(next) = assignment.initializer.and_then(|e| e.node_id()) else {
                        continue;
                    };
                    (assignment.name.node_id(), Some(next))
                }
                tsr_ast::ObjectLiteralElementLike::ShorthandPropertyAssignment(shorthand) => {
                    (shorthand.name.node_id(), None)
                }
                tsr_ast::ObjectLiteralElementLike::MethodDeclaration(method) => {
                    (method.name.node_id(), None)
                }
                tsr_ast::ObjectLiteralElementLike::GetAccessorDeclaration(accessor) => {
                    (accessor.name.node_id(), None)
                }
                tsr_ast::ObjectLiteralElementLike::SetAccessorDeclaration(accessor) => {
                    (accessor.name.node_id(), None)
                }
                tsr_ast::ObjectLiteralElementLike::SpreadAssignment(_) => continue,
            };
            let Some(name_id) = name else { continue };
            // `getLiteralTypeFromProperty(…, StringOrNumberLiteralOrUnique)` —
            // a computed name whose type is no literal or unique symbol
            // yields no usable name type and upstream `continue`s.
            let computed = self.nodes.kind(name_id) == SyntaxKind::ComputedPropertyName;
            let Some((name, key)) = (match self.object_literal_member_name(name_id) {
                Ok(name) => name,
                Err(()) if computed => continue,
                Err(()) => return None,
            }) else {
                continue;
            };
            // `ast.IsComputedNonLiteralName`: the property assignment's
            // report reads TS2418.
            let head = (computed && next.is_some() && !self.computed_name_is_literal(name_id))
                .then_some(
                &messages::TYPE_OF_COMPUTED_PROPERTY_S_VALUE_IS_0_WHICH_IS_NOT_ASSIGNABLE_TO_TYPE_1,
            );
            if computed && is_union {
                continue;
            }
            // The indexed-access result uses the concrete target receiver.
            // Reading the declaration symbol alone loses its mapper, so a
            // member declared as T on C<number> would be compared against T.
            // `getIndexedAccessTypeOrUndefined` falls back to the target's
            // applicable index signature; absent from both means excess,
            // TS2353's row.
            let target_property_type = if is_union {
                self.union_member_target_type(source, target, name_id, &name).ok()?
            } else if let Some(key) = key {
                self.get_type_of_property_of_type(target, &name)
                    .or_else(|| self.get_applicable_index_info(target, key).map(|info| info.value))
            } else {
                self.get_type_of_property_of_type(target, &name)
                    .or_else(|| self.elaboration_index_value(target, name_id, &name))
            };
            let Some(target_property_type) = target_property_type else { continue };
            members.push((name_id, next, name, target_property_type, head));
        }
        let mut reported = false;
        let written: Vec<String> = members.iter().map(|(_, _, name, _, _)| name.clone()).collect();
        for (name_id, next, name, target_property_type, head) in members {
            // `getIndexedAccessTypeOrUndefined(source, nameType, …)` reads the
            // completed source member, including mutable-location widening.
            // A name written twice is the last occurrence's member, which
            // this port's literal member type does not yet read (r7-reports
            // §35).
            let duplicated = written.iter().filter(|&other| *other == name).count() > 1;
            let source_property_type = if duplicated {
                self.last_object_literal_member_value(literal, &name)
                    .map(|value| self.check_expression_at_node(value))
            } else {
                self.get_type_of_property_of_type(source, &name)
            };
            let Some(source_property_type) = source_property_type else {
                continue;
            };
            let before = self.diagnostics.len();
            reported |= self.elaborate_element_with(
                name_id,
                next,
                source_property_type,
                target_property_type,
                head,
            );
            self.add_expected_type_related_info(before, name_id, next, target, &name);
        }
        Some(reported)
    }

    /// `elaborateElement`'s related information (`relater.go:575`): when its
    /// report landed on the member itself (not an inner elaboration), `The
    /// expected type comes from property 'p' which is declared here on type
    /// 'T'` at the target member's declaration, unless that is in a default
    /// library (r7-reports §38).
    pub(crate) fn add_expected_type_related_info(
        &mut self,
        before: usize,
        prop: NodeId,
        next: Option<NodeId>,
        target: TypeId,
        name: &str,
    ) {
        if self.diagnostics.len() != before + 1 {
            return;
        }
        if !self.is_member_relation_report(before, prop, next) {
            return;
        }
        // A union target's property is the synthesized union property,
        // whose declarations are its constituents' in union order
        // (`createUnionOrIntersectionProperty`).
        // A member missing from any constituent (`undefined` included) makes
        // that property partial, and `getPropertyOfType` answers none for a
        // partial union property; the union itself has no symbol either.
        let union = matches!(self.type_of(target).data, TypeData::Union { .. });
        let property = if union {
            let TypeData::Union { types, .. } = self.type_of(target).data.clone() else {
                return;
            };
            let mut first = None;
            for part in types {
                let Some(found) = self.get_property_of_type(part, name) else { return };
                first.get_or_insert(found);
            }
            first
        } else {
            self.get_property_of_type(target, name)
        };
        // No target member: the applicable index signature's declaration
        // (TS6501). Otherwise, or when that has none, the member's first
        // declaration, else the target's own (`target.symbol`): a mapped
        // member without a modifiers property has none of its own.
        let mut related = None;
        if property.is_none() {
            let key = self.property_name_key_type(name);
            if let Some(declaration) =
                self.get_applicable_index_info(target, key).and_then(|info| info.declaration)
                && !self.in_default_library(declaration)
            {
                let declared_span = self.error_span(declaration);
                related = Some(self.related_diagnostic_at(
                    declaration,
                    &messages::THE_EXPECTED_TYPE_COMES_FROM_THIS_INDEX_SIGNATURE,
                    declared_span,
                    Vec::new(),
                ));
            }
        }
        if related.is_none() {
            let target_symbol = match &self.type_of(target).data {
                TypeData::Anonymous { symbol, .. } => Some(*symbol),
                TypeData::Named { members, .. } => *members,
                _ => None,
            };
            // A mapped type's symbol is its own declaration's.
            let mapped = self.mapped_types.get(&target).map(|info| info.declaration);
            let declaration = property
                .and_then(|property| {
                    self.binder.symbols().get(property).declarations.first().copied()
                })
                .or(mapped)
                .or_else(|| {
                    target_symbol.and_then(|symbol| {
                        self.binder.symbols().get(symbol).declarations.first().copied()
                    })
                });
            if let Some(declaration) = declaration
                && !self.in_default_library(declaration)
            {
                let declared_span = self.error_span(declaration);
                let target_text = self.type_to_string(target);
                related = Some(self.related_diagnostic_at(
                    declaration,
                    &messages::THE_EXPECTED_TYPE_COMES_FROM_PROPERTY_0_WHICH_IS_DECLARED_HERE_ON_TYPE_1,
                    declared_span,
                    vec![name.to_owned(), target_text],
                ));
            }
        }
        let Some(related) = related else { return };
        if related.file().is_none() {
            return;
        }
        let diagnostic = &mut self.diagnostics[before].1;
        let mut list = diagnostic.related_information().to_vec();
        list.push(related);
        diagnostic.set_related_information(std::sync::Arc::new(list));
    }

    /// Is the diagnostic at `index` the one `elaborateElement`'s own
    /// `checkTypeRelatedToEx(…, prop, …)` produced, rather than an inner
    /// elaboration's? It is either at the member itself (the relation
    /// error, whatever related information it already carries), or the
    /// excess-property error that relation moved to a direct member of the
    /// value literal (TS2353 / TS2561, r7-reports §39).
    fn is_member_relation_report(&self, index: usize, prop: NodeId, next: Option<NodeId>) -> bool {
        let diagnostic = &self.diagnostics[index].1;
        if diagnostic.span == self.error_span(prop) {
            return true;
        }
        let excess = [
            &messages::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_0_DOES_NOT_EXIST_IN_TYPE_1,
            &messages::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_BUT_0_DOES_NOT_EXIST_IN_TYPE_1_DID_YOU_MEAN_TO_WRITE_2,
        ];
        if !excess.iter().any(|message| message.code() == diagnostic.message.code()) {
            return false;
        }
        let Some(Node::ObjectLiteralExpression(literal)) =
            next.and_then(|value| self.node_map.get(value))
        else {
            return false;
        };
        literal.properties.iter().any(|property| {
            let name = match property {
                tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                    assignment.name.node_id()
                }
                tsr_ast::ObjectLiteralElementLike::ShorthandPropertyAssignment(shorthand) => {
                    shorthand.name.node_id()
                }
                tsr_ast::ObjectLiteralElementLike::MethodDeclaration(method) => {
                    method.name.node_id()
                }
                _ => None,
            };
            name.is_some_and(|name| self.error_span(name) == diagnostic.span)
        })
    }

    /// `getBestMatchIndexedAccessTypeOrUndefined` (`relater.go:620`) for a
    /// union target: the union's own indexed access when every constituent
    /// has the member (`getPropertyOfType` on a union, or each constituent's
    /// applicable index signature), else the member of `getBestMatchingType`'s
    /// constituent. `Ok(None)` is upstream's nil (no elaboration for this
    /// member); `Err` is a best match this port cannot choose.
    fn union_member_target_type(
        &mut self,
        source: TypeId,
        target: TypeId,
        name_id: NodeId,
        name: &str,
    ) -> Result<Option<TypeId>, ()> {
        let TypeData::Union { types, .. } = self.type_of(target).data.clone() else {
            return Ok(None);
        };
        let mut found = Vec::with_capacity(types.len());
        for &part in &types {
            if let Some(member) = self
                .get_type_of_property_of_type(part, name)
                .or_else(|| self.elaboration_index_value(part, name_id, name))
            {
                found.push(member);
            }
        }
        if found.len() == types.len() {
            return Ok(Some(self.get_union_type(&found)));
        }
        if found.is_empty() {
            // No constituent has the member: whichever constituent upstream's
            // best match picks, its indexed access is nil.
            return Ok(None);
        }
        let best = self.best_matching_type_for_object_literal(source, target)?;
        Ok(best.and_then(|best| {
            self.get_type_of_property_of_type(best, name)
                .or_else(|| self.elaboration_index_value(best, name_id, name))
        }))
    }

    /// `hasExcessProperties` (`relater.go:2714`) for the fresh object literal
    /// written at `node`, asked without reporting. `None` is a question this
    /// port cannot decide (a spread or computed member, a constituent whose
    /// member table is incomplete, an `Unknown` relation); the caller then
    /// declines rather than guesses.
    ///
    /// - `isExcessPropertyCheckTarget`, then the assignable relation's
    ///   `isTypeSubsetOf(globalObjectType, target) || isEmptyObjectType(target)`
    ///   exemption.
    /// - A union target is reduced by `findMatchingDiscriminantType`, else by
    ///   `filterPrimitivesIfContainsNonPrimitive`, and its constituents are the
    ///   `checkTypes`.
    /// - Each written member (`shouldCheckAsExcessProperty`) must be
    ///   `isKnownProperty` in the reduced target; the first that is not is the
    ///   excess member, reported against
    ///   `filterType(reducedTarget, isExcessPropertyCheckTarget)`.
    /// - Against `checkTypes`, each member's type must relate to
    ///   `getTypeOfPropertyInTypes`; a nested fresh literal runs this check
    ///   first, and its excess member is the one reported (`reportError`
    ///   drops the `Types of property` link over an excess-property message).
    ///   A failed member relation is the outer relation failure.
    fn excess_properties_verdict(
        &mut self,
        node: NodeId,
        source: TypeId,
        target: TypeId,
    ) -> Option<ExcessProperties> {
        let node = self.skip_parenthesized_expression(node);
        let Some(Node::ObjectLiteralExpression(literal)) = self.node_map.get(node) else {
            return Some(ExcessProperties::None);
        };
        if !self.is_excess_property_check_target(target) {
            return Some(ExcessProperties::None);
        }
        // `!noImplicitAny && target.objectFlags&ObjectFlagsJSLiteral`
        // (`relater.go:2715`): the TARGET is a JS object literal's own type,
        // which `checkObjectLiteral` marks only when it had no contextual
        // type (`checker.go:13206`), and it takes any property. A JS source
        // literal is checked like any other.
        if !self.no_implicit_any && self.js_literal_types.contains(&target) {
            return Some(ExcessProperties::None);
        }
        if self.excess_check_target_admits_any_property(target)? {
            return Some(ExcessProperties::None);
        }
        let (reduced, check_types) = match &self.type_of(target).data {
            TypeData::Union { .. } => {
                let discriminated = self.find_matching_discriminant_type(source, target)?;
                let reduced = if discriminated == target {
                    self.filter_primitives_if_contains_non_primitive(target)
                } else {
                    discriminated
                };
                let check_types = match &self.type_of(reduced).data {
                    TypeData::Union { types, .. } => types.clone(),
                    _ => vec![reduced],
                };
                (reduced, Some(check_types))
            }
            _ => (target, None),
        };
        for property in literal.properties {
            let (name, value) = match property {
                tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                    (assignment.name.node_id(), assignment.initializer.and_then(|e| e.node_id()))
                }
                tsr_ast::ObjectLiteralElementLike::ShorthandPropertyAssignment(shorthand) => {
                    (shorthand.name.node_id(), None)
                }
                tsr_ast::ObjectLiteralElementLike::MethodDeclaration(method) => {
                    (method.name.node_id(), None)
                }
                tsr_ast::ObjectLiteralElementLike::GetAccessorDeclaration(accessor) => {
                    (accessor.name.node_id(), None)
                }
                tsr_ast::ObjectLiteralElementLike::SetAccessorDeclaration(accessor) => {
                    (accessor.name.node_id(), None)
                }
                tsr_ast::ObjectLiteralElementLike::SpreadAssignment(_) => return None,
            };
            let name_id = name?;
            let Some((name, key)) = self.object_literal_member_name(name_id).ok()? else {
                continue;
            };
            if !self.is_known_property_keyed(reduced, &name, key)? {
                let error_target = self
                    .filter_type(reduced, |checker, t| checker.is_excess_property_check_target(t));
                return Some(ExcessProperties::Excess { at: name_id, name, error_target });
            }
            let Some(check_types) = &check_types else { continue };
            let source_property = self.get_type_of_property_of_type(source, &name)?;
            let target_property = self.type_of_property_in_types(check_types, &name)?;
            if let Some(value) = value {
                match self.excess_properties_verdict(value, source_property, target_property)? {
                    ExcessProperties::None => {}
                    verdict => return Some(verdict),
                }
            }
            match self.relate_ternary(
                source_property,
                target_property,
                crate::relater::Relation::Assignable,
            ) {
                crate::relater::Ternary::Related => {}
                crate::relater::Ternary::NotRelated => return Some(ExcessProperties::Incompatible),
                crate::relater::Ternary::Unknown => return None,
            }
        }
        Some(ExcessProperties::None)
    }

    /// `hasExcessProperties` (`relater.go:2714`) as `isRelatedTo` runs it for
    /// a fresh object literal source in any relation, reported: TS2353 /
    /// TS2561 at the excess member. Answers whether it reported; a decided
    /// "no excess" and an undecided verdict both answer `false`.
    pub(crate) fn report_fresh_literal_excess_property(
        &mut self,
        node: NodeId,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        match self.excess_properties_verdict(node, source, target) {
            Some(ExcessProperties::Excess { at, name, error_target }) => {
                self.report_excess_property(at, &name, error_target)
            }
            _ => false,
        }
    }

    /// TS2353 / TS2561 for [`ExcessProperties::Excess`]: `hasExcessProperties`'
    /// object-literal report (`relater.go:2714`). An identifier name with a
    /// spelling suggestion among the error target's properties
    /// (`getSuggestionForNonexistentProperty`) is TS2561. Answers whether it
    /// reported; an error target whose property names this port cannot
    /// enumerate declines.
    fn report_excess_property(&mut self, at: NodeId, name: &str, error_target: TypeId) -> bool {
        let candidates = if self.nodes.kind(at) == SyntaxKind::Identifier {
            let Some(names) = self.property_names_for_suggestion(error_target) else {
                return false;
            };
            names
        } else {
            Vec::new()
        };
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return false };
        let span = self.error_span(at);
        let printed = self.type_to_string(error_target);
        let candidates: Vec<&str> = candidates.iter().map(String::as_str).collect();
        let diagnostic = match crate::check::spelling_suggestion(name, &candidates) {
            Some(suggestion) => Diagnostic::with_args(
                &messages::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_BUT_0_DOES_NOT_EXIST_IN_TYPE_1_DID_YOU_MEAN_TO_WRITE_2,
                span,
                [name.to_string(), printed, suggestion.to_string()],
            ),
            None => Diagnostic::with_args(
                &messages::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_0_DOES_NOT_EXIST_IN_TYPE_1,
                span,
                [name.to_string(), printed],
            ),
        };
        self.report(file, diagnostic);
        true
    }

    /// The property name an object-literal member's written name binds,
    /// spelled as the member tables spell it, with the unique-symbol name type
    /// for a late-bound symbol name. `Ok(None)` is a computed name
    /// whose type is not `StringOrNumberLiteralOrUnique`: `checkObjectLiteral`
    /// folds it into an index signature, so the literal type has no property
    /// of that name and `getPropertiesOfType` never yields it. `Err` is a
    /// literal or unique-symbol name this port cannot spell.
    fn object_literal_member_name(
        &mut self,
        name: NodeId,
    ) -> Result<Option<(String, Option<TypeId>)>, ()> {
        if let Some(text) = self.written_member_name(name) {
            return Ok(Some((text, None)));
        }
        let Some(Node::ComputedPropertyName(computed)) = self.node_map.get(name) else {
            return Err(());
        };
        let expression = computed.expression.ok_or(())?;
        let name_type = self.check_expression(expression);
        if !self.type_of(name_type).flags.intersects(
            TypeFlags::STRING_LITERAL | TypeFlags::NUMBER_LITERAL | TypeFlags::UNIQUE_ES_SYMBOL,
        ) {
            return Ok(None);
        }
        // `late_bound_member_names` (`crate::members`) spells a computed
        // member of a declared type the same two ways.
        if let Some(text) = self.property_name_from_index(name_type) {
            return Ok(Some((text, None)));
        }
        let (spelled, _) = self.late_bound_symbol_member_name(computed).ok_or(())?;
        Ok(Some((spelled, Some(name_type))))
    }

    /// The property name a written (non-computed) member name binds: the
    /// identifier or string text, and for a numeric literal its canonical
    /// number text (`getPropertyNameForPropertyNameNode`: `2.0:` binds `"2"`).
    fn written_member_name(&self, name: NodeId) -> Option<String> {
        match self.node_map.get(name)? {
            Node::NumericLiteral(literal) => Some(crate::printing::normalise_number(literal.text)),
            _ => self.identifier_text(name).map(str::to_string),
        }
    }

    /// `getPropertiesOfType(containingType)`'s names for
    /// `getSuggestionForNonexistentProperty`. A union's properties are the
    /// names every constituent has (`getPropertiesOfUnionOrIntersectionType`
    /// drops a `ReadPartial` one): its own property, an applicable index
    /// signature, or, for an object literal type without a spread, an
    /// implied `undefined` (`createUnionOrIntersectionProperty`). So the
    /// candidates are one enumerable constituent's names that every other
    /// constituent has; a constituent that can neither confirm nor rule out a
    /// candidate declines. An intersection's are every constituent's names.
    fn property_names_for_suggestion(&mut self, t: TypeId) -> Option<Vec<String>> {
        if let Some(names) = self.get_property_names_of_type(t) {
            return Some(names);
        }
        let types = match self.type_of(t).data.clone() {
            TypeData::Union { types, .. } => types,
            TypeData::Intersection { types, .. } => {
                let mut names = Vec::new();
                for part in types {
                    for name in self.certified_property_names(part)? {
                        if !names.contains(&name) {
                            names.push(name);
                        }
                    }
                }
                return Some(names);
            }
            _ => return None,
        };
        let (first, names) =
            types.iter().find_map(|&part| Some((part, self.certified_property_names(part)?)))?;
        let mut candidates = Vec::with_capacity(names.len());
        for name in names {
            let mut everywhere = true;
            for &part in &types {
                if part == first {
                    continue;
                }
                match self.is_known_property(part, &name) {
                    Some(true) => {}
                    Some(false) if self.object_literal_spread_flags.get(&part) == Some(&false) => {}
                    Some(false) => {
                        everywhere = false;
                        break;
                    }
                    None => return None,
                }
            }
            if everywhere {
                candidates.push(name);
            }
        }
        Some(candidates)
    }

    /// `ast.SkipParentheses`.
    fn skip_parenthesized_expression(&self, mut node: NodeId) -> NodeId {
        while let Some(Node::ParenthesizedExpression(inner)) = self.node_map.get(node)
            && let Some(next) = inner.expression.and_then(|e| e.node_id())
        {
            node = next;
        }
        node
    }

    /// `hasExcessProperties`' assignable-relation exemption:
    /// `isTypeSubsetOf(globalObjectType, target) || isEmptyObjectType(target)`
    /// (`isEmptyObjectType`, `checker.go`: an object type with no properties,
    /// signatures or index signatures, `object`, a union with some such
    /// constituent, an intersection of only such). `None` where a
    /// constituent's emptiness cannot be decided.
    fn excess_check_target_admits_any_property(&mut self, target: TypeId) -> Option<bool> {
        let global_object = self
            .global_type_symbol_with_arity("Object", 0)
            .map(|symbol| self.get_declared_type_of_symbol(symbol));
        let parts = match &self.type_of(target).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => vec![target],
        };
        if global_object.is_some_and(|object| parts.contains(&object)) {
            return Some(true);
        }
        let mut undecided = false;
        for part in parts {
            match self.is_empty_object_type_for_excess(part) {
                Some(true) => return Some(true),
                Some(false) => {}
                None => undecided = true,
            }
        }
        (!undecided).then_some(false)
    }

    /// `isEmptyObjectType` for one union constituent.
    fn is_empty_object_type_for_excess(&mut self, t: TypeId) -> Option<bool> {
        let flags = self.type_of(t).flags;
        if flags.contains(TypeFlags::NON_PRIMITIVE) {
            return Some(true);
        }
        if let TypeData::Intersection { types, .. } = self.type_of(t).data.clone() {
            let mut all = true;
            for part in types {
                all &= self.is_empty_object_type_for_excess(part)?;
            }
            return Some(all);
        }
        if !flags.contains(TypeFlags::OBJECT) {
            return Some(false);
        }
        if self.is_empty_anonymous_object_type(t) {
            return Some(true);
        }
        // A named property settles it; an empty or unknown table also needs
        // the index and signature lists.
        if self.get_property_names_of_type(t).is_some_and(|names| !names.is_empty()) {
            return Some(false);
        }
        if self.certified_property_names(t)?.is_empty()
            && self.get_index_infos_of_type(t)?.is_empty()
        {
            for kind in [
                crate::signatures::SignatureKind::Call,
                crate::signatures::SignatureKind::Construct,
            ] {
                if !self.signatures_of_type_kind(t, kind)?.is_empty() {
                    return Some(false);
                }
            }
            return Some(true);
        }
        Some(false)
    }

    /// `filterPrimitivesIfContainsNonPrimitive` (`relater.go`): only a union
    /// containing `object` drops its primitive constituents.
    pub(crate) fn filter_primitives_if_contains_non_primitive(&mut self, union: TypeId) -> TypeId {
        if self.maybe_type_of_kind(union, TypeFlags::NON_PRIMITIVE) {
            let result = self.filter_type(union, |checker, t| {
                !checker.type_of(t).flags.intersects(TypeFlags::PRIMITIVE)
            });
            if !self.type_of(result).flags.contains(TypeFlags::NEVER) {
                return result;
            }
        }
        union
    }

    /// `isKnownProperty` (`relater.go:719`) without JSX attributes: an
    /// object type's own (declared or inherited) property or applicable index
    /// signature; in a union or intersection that is an excess-property check
    /// target, any constituent's. `None` where an object constituent's member
    /// table is not certified and no other constituent knows the name.
    fn is_known_property(&mut self, target: TypeId, name: &str) -> Option<bool> {
        self.is_known_property_keyed(target, name, None)
    }

    /// [`Checker::is_known_property`] for a property whose name type is
    /// `key` (`getLiteralTypeFromProperty`): a unique-symbol name meets the
    /// index signatures as its symbol type, not as a string literal.
    fn is_known_property_keyed(
        &mut self,
        target: TypeId,
        name: &str,
        key: Option<TypeId>,
    ) -> Option<bool> {
        let ty = self.type_of(target);
        let flags = ty.flags;
        if let TypeData::Union { types, .. } | TypeData::Intersection { types, .. } = &ty.data {
            if !self.is_excess_property_check_target(target) {
                return Some(false);
            }
            let types = types.clone();
            let mut undecided = false;
            for part in types {
                match self.is_known_property_keyed(part, name, key) {
                    Some(true) => return Some(true),
                    Some(false) => {}
                    None => undecided = true,
                }
            }
            return (!undecided).then_some(false);
        }
        if !flags.contains(TypeFlags::OBJECT) {
            return Some(false);
        }
        let names = self.certified_property_names(target);
        if names.as_ref().is_some_and(|names| names.iter().any(|seen| seen == name)) {
            return Some(true);
        }
        let infos = self.get_index_infos_of_type(target)?;
        if !infos.is_empty() {
            // `isLateBoundName(name) && getIndexInfoOfType(target, string)`:
            // for backwards compatibility a string index signature accepts a
            // symbol-named property.
            if key.is_some() && infos.iter().any(|info| info.key == self.intrinsics.string) {
                return Some(true);
            }
            let key = key.unwrap_or_else(|| self.property_name_key_type(name));
            if self.get_applicable_index_info(target, key).is_some() {
                return Some(true);
            }
        }
        names.map(|_| false)
    }

    /// A complete property-name list for an object type: the relation
    /// reporters' certified table, else `getPropertiesOfType`'s names.
    ///
    /// A function or constructor type literal (`() => any`) is an anonymous
    /// type whose `__type` symbol holds only its signature, so
    /// `resolveAnonymousTypeMembers` gives it no properties (the global
    /// `Function` members are `getPropertyOfType`'s apparent-type fallback,
    /// which `isKnownProperty`'s `getPropertyOfObjectType` does not take).
    fn certified_property_names(&mut self, t: TypeId) -> Option<Vec<String>> {
        if let TypeData::Anonymous { symbol, .. } = self.type_of(t).data {
            let declarations = &self.binder.symbols().get(symbol).declarations;
            if !declarations.is_empty()
                && declarations.iter().all(|&declaration| {
                    matches!(
                        self.nodes.kind(declaration),
                        SyntaxKind::FunctionType | SyntaxKind::ConstructorType
                    )
                })
            {
                return Some(Vec::new());
            }
        }
        match self.relation_property_table(t) {
            Some(table) => Some(table.into_iter().map(|(name, _)| name).collect()),
            None => self.get_property_names_of_type(t),
        }
    }

    /// `isPerformingCommonPropertyChecks && !hasCommonProperties`
    /// (`isRelatedToEx`, `relater.go:2676`): a source with properties or
    /// signatures against a weak target
    /// (`isWeakType`, `relater.go:681`) that knows none of the source's
    /// property names (`isKnownProperty`). `false` wherever this port cannot
    /// decide (an uncertified member table), so it never rejects a pair
    /// upstream accepts. The relater's arm (`crate::relater`) and the TS2559/
    /// TS2560 reporter both ask it.
    pub(crate) fn fails_common_property_check(&mut self, source: TypeId, target: TypeId) -> bool {
        if !self
            .type_of(source)
            .flags
            .intersects(TypeFlags::PRIMITIVE | TypeFlags::OBJECT | TypeFlags::INTERSECTION)
            || !self.type_of(target).flags.intersects(TypeFlags::OBJECT | TypeFlags::INTERSECTION)
        {
            return false;
        }
        // `source != globalObjectType`.
        if let TypeData::Named { members: Some(owner), .. } = self.type_of(source).data
            && self.global_type_symbol_with_arity("Object", 0) == Some(owner)
        {
            return false;
        }
        if self.is_weak_type(target) != Some(true) {
            return false;
        }
        // `getPropertiesOfType` reads the reduced apparent type.
        let apparent = self.apparent_type(source);
        let Some(names) = self.common_check_property_names(apparent) else { return false };
        if names.is_empty() && !self.type_has_call_or_construct_signatures(source) {
            return false;
        }
        for name in &names {
            if self.is_known_property(target, name) != Some(false) {
                return false;
            }
        }
        true
    }

    /// `getPropertiesOfType` of a reduced apparent type, for
    /// `hasCommonProperties`: an intersection's properties are every
    /// constituent's (`getPropertiesOfUnionOrIntersectionType` creates a
    /// property for a name any constituent declares), so
    /// `T & { "ignore-prop": true }` over `T extends { b: number }` has `b`
    /// and `ignore-prop` (`tsxStatelessFunctionComponentsWithTypeArguments4`).
    /// `None` where a constituent's names are not enumerable.
    fn common_check_property_names(&mut self, apparent: TypeId) -> Option<Vec<String>> {
        let TypeData::Intersection { types, .. } = self.type_of(apparent).data.clone() else {
            return self.get_property_names_of_type(apparent);
        };
        let mut names = Vec::new();
        for part in types {
            let part = self.apparent_type(part);
            for name in self.common_check_property_names(part)? {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
        Some(names)
    }

    /// `isWeakType` (`relater.go:681`): an object type with at least one
    /// property, every property optional, and no signatures or index
    /// signatures; an intersection of only such. `None` where the member
    /// table is not certified.
    fn is_weak_type(&mut self, t: TypeId) -> Option<bool> {
        // The substitution arm: `NoInfer<T>` is weak exactly when `T` is.
        if let Some(base) = self.no_infer_base_type(t) {
            return self.is_weak_type(base);
        }
        if let TypeData::Intersection { types, .. } = self.type_of(t).data.clone() {
            for part in types {
                if !self.is_weak_type(part)? {
                    return Some(false);
                }
            }
            return Some(true);
        }
        if !self.type_of(t).flags.contains(TypeFlags::OBJECT) {
            return Some(false);
        }
        match self.relation_property_table(t) {
            Some(table) => {
                if table.is_empty() || table.iter().any(|(_, optional)| !optional) {
                    return Some(false);
                }
            }
            None => {
                if !self.instantiated_reference_all_optional(t)? {
                    return Some(false);
                }
            }
        }
        if !self.get_index_infos_of_type(t)?.is_empty() {
            return Some(false);
        }
        Some(!self.type_has_call_or_construct_signatures_certified(t)?)
    }

    /// The optionality of a class or interface instantiation's members
    /// (`IntrinsicClassAttributes<C>`), whose declared walk
    /// (`relation_property_table`) declines a reference with type
    /// arguments. A member's optionality is its declaration's `?`, which no
    /// type argument changes, so the complete name list and each name's
    /// property symbol decide it (r7-reports §32). A mapped type's `?`
    /// comes from its modifiers, so a mapped type is not read here.
    ///
    /// Answers whether the list is non-empty and every member optional,
    /// stopping at the first required member: the weak test asks this on
    /// every relation against such a target, and nearly every
    /// instantiation has one.
    fn instantiated_reference_all_optional(&mut self, t: TypeId) -> Option<bool> {
        let &(symbol, _) = self.type_reference_targets.get(&t)?;
        if !self
            .binder
            .symbols()
            .get(symbol)
            .flags
            .intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE)
            || self.mapped_types.contains_key(&t)
        {
            return None;
        }
        let names = self.get_property_names_of_type_shared(t)?;
        if names.is_empty() {
            return Some(false);
        }
        for name in names.iter() {
            // A member inherited through a generic base has no symbol on the
            // ordinary road (`generic_heritage_property_symbol`).
            let property = match self.get_property_of_type(t, name) {
                Some(property) => property,
                None => self.generic_heritage_property_symbol(t, name, &mut Vec::new())?,
            };
            if !self.property_is_optional(property) {
                return Some(false);
            }
        }
        Some(true)
    }

    /// `typeHasCallOrConstructSignatures`, `false` where undecidable.
    fn type_has_call_or_construct_signatures(&mut self, t: TypeId) -> bool {
        self.type_has_call_or_construct_signatures_certified(t) == Some(true)
    }

    fn type_has_call_or_construct_signatures_certified(&mut self, t: TypeId) -> Option<bool> {
        for kind in
            [crate::signatures::SignatureKind::Call, crate::signatures::SignatureKind::Construct]
        {
            if !self.signatures_of_type_kind(t, kind)?.is_empty() {
                return Some(true);
            }
        }
        Some(false)
    }

    /// TS2559 / TS2560 — the weak-type failure `isRelatedToEx`
    /// (`relater.go:2676`) reports with `reportError` and no head message, so
    /// it is the whole diagnostic at the error node, for assignments and
    /// arguments alike. Asked of `isRelatedToEx`'s normalized pair: a
    /// `NoInfer` target is its base, and a definitely non-nullable source
    /// against `null`/`undefined` plus one other type is related to that
    /// type. TS2560 when the source's first call (or construct) signature
    /// returns a type related to the target. Answers whether it reported.
    pub(crate) fn report_weak_type_failure(
        &mut self,
        at: NodeId,
        span: tsr_core::Span,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let target = self.no_infer_base_type(target).unwrap_or(target);
        let target = self.non_nullable_union_candidate(source, target).unwrap_or(target);
        if !self.fails_common_property_check(source, target) {
            return false;
        }
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return false };
        let Some((message, args)) = self.weak_type_message(source, target) else { return false };
        self.report(file, Diagnostic::with_args(message, span, args));
        true
    }

    /// `isRelatedTo`'s common-property report (`relater.go:2731`) for a pair
    /// that fails the check: TS2559, or TS2560 when the source's first call
    /// or construct signature returns something related to the target.
    /// `None` where that return relation is undecided.
    fn weak_type_message(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<(&'static tsr_diagnostics::Message, Vec<String>)> {
        let mut callable = false;
        for kind in
            [crate::signatures::SignatureKind::Call, crate::signatures::SignatureKind::Construct]
        {
            let Some(first) = self
                .signatures_of_type_kind(source, kind)
                .and_then(|signatures| signatures.into_iter().next())
            else {
                continue;
            };
            let Some(return_type) = self.get_return_type_of_signature(&first) else { continue };
            match self.relate_ternary(return_type, target, crate::relater::Relation::Assignable) {
                crate::relater::Ternary::Related => {
                    callable = true;
                    break;
                }
                crate::relater::Ternary::NotRelated => {}
                crate::relater::Ternary::Unknown => return None,
            }
        }
        let source_text = self.type_to_string(source);
        let target_text = self.type_to_string(target);
        let message = if callable {
            &messages::VALUE_OF_TYPE_0_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE_1_DID_YOU_MEAN_TO_CALL_IT
        } else {
            &messages::TYPE_0_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE_1
        };
        Some((message, vec![source_text, target_text]))
    }

    /// `isRelatedToEx`'s nullable-stripping step (`relater.go:2640`): a
    /// `TypeFlagsDefinitelyNonNullable` source against a union of two (or
    /// three) types, all but one `null`/`undefined`, is related to the
    /// remaining one.
    fn non_nullable_union_candidate(&mut self, source: TypeId, target: TypeId) -> Option<TypeId> {
        if !self.type_of(source).flags.intersects(TypeFlags::DEFINITELY_NON_NULLABLE) {
            return None;
        }
        let TypeData::Union { types, .. } = &self.type_of(target).data else { return None };
        if !(2..=3).contains(&types.len()) {
            return None;
        }
        let mut candidate = None;
        for &part in types {
            if self.type_of(part).flags.intersects(TypeFlags::NULLABLE) {
                continue;
            }
            if candidate.replace(part).is_some() {
                return None;
            }
        }
        candidate
    }

    /// `getApplicableIndexInfoForName`'s key: the name's string-literal type
    /// (`isApplicableIndexType` admits a numeric-literal name to a number
    /// index signature).
    fn property_name_key_type(&mut self, name: &str) -> TypeId {
        self.store.intern_literal(
            TypeFlags::STRING_LITERAL,
            TypeData::StringLiteral(name.to_owned()),
            false,
        )
    }

    /// `getTypeOfPropertyInTypes` (`relater.go:2795`): the union of each
    /// type's `getTypeOfPropertyInType` — its property, else its applicable
    /// index signature's value, else `undefined`. `None` where a type lacking
    /// the property has an uncertified member table.
    pub(crate) fn type_of_property_in_types(
        &mut self,
        types: &[TypeId],
        name: &str,
    ) -> Option<TypeId> {
        let mut found = Vec::with_capacity(types.len());
        for &t in types {
            found.push(
                self.type_of_property_or_index_signature(t, name)
                    .ok()?
                    .unwrap_or(self.intrinsics.undefined),
            );
        }
        Some(self.get_union_type(&found))
    }

    /// `getTypeOfPropertyOrIndexSignatureOfType`: `Ok(None)` is upstream's
    /// nil (no property, no applicable index signature, certified table);
    /// `Err` is undecidable. Both lookups read the apparent type
    /// (`getReducedApparentType`), so a primitive answers from its wrapper
    /// interface.
    fn type_of_property_or_index_signature(
        &mut self,
        t: TypeId,
        name: &str,
    ) -> Result<Option<TypeId>, ()> {
        if let Some(member) = self.get_type_of_property_of_type(t, name) {
            return Ok(Some(member));
        }
        let apparent = self.apparent_type(t);
        // getApparentType leaves `undefined`, `null` and `void` as they are,
        // and getPropertyOfObjectType and the index lookup find nothing on a
        // non-object type: an optional member's `T | undefined` reads
        // `undefined` for that constituent (getTypeOfPropertyInType).
        if self.type_of(apparent).flags.intersects(TypeFlags::NULLABLE | TypeFlags::VOID) {
            return Ok(None);
        }
        let key = self.property_name_key_type(name);
        if let Some(info) = self.get_applicable_index_info(apparent, key) {
            return Ok(Some(info.value));
        }
        // A type whose property names are certified, without `name`, answers
        // nil; a name it lists that the lookup missed is undecidable.
        let parts = match &self.type_of(apparent).data {
            TypeData::Union { types, .. } | TypeData::Intersection { types, .. } => types.clone(),
            _ => vec![apparent],
        };
        for part in parts {
            if self.certified_property_names(part).ok_or(())?.iter().any(|seen| seen == name) {
                return Err(());
            }
        }
        Ok(None)
    }

    /// `findMatchingDiscriminantType` (`relater.go:1062`): `Some(target)`
    /// itself is upstream's nil (`discriminateTypeByDiscriminableItems`
    /// answering the target unchanged); `None` is undecidable.
    /// `getMatchingUnionConstituentForType` answers only for unions of ten or
    /// more object constituents (`computeKeyPropertyNameAndMap`); that key map
    /// is not ported, so such unions are undecidable.
    pub(crate) fn find_matching_discriminant_type(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<TypeId> {
        let TypeData::Union { types, .. } = self.type_of(target).data.clone() else {
            return Some(target);
        };
        if !self.type_of(source).flags.intersects(TypeFlags::OBJECT | TypeFlags::INTERSECTION) {
            return Some(target);
        }
        if types.len() >= 10
            && types
                .iter()
                .filter(|&&t| {
                    self.type_of(t)
                        .flags
                        .intersects(TypeFlags::OBJECT | TypeFlags::INSTANTIABLE_NON_PRIMITIVE)
                })
                .count()
                >= 10
        {
            return None;
        }
        // findDiscriminantProperties(getPropertiesOfType(source), target).
        let mut discriminants = Vec::new();
        let mut unwidened = Vec::new();
        let mut widened = false;
        for name in self.get_property_names_of_type(source)? {
            if self.is_discriminant_property_of_types(&types, &name)? {
                let source_property = self.get_type_of_property_of_type(source, &name)?;
                let written =
                    self.discriminant_member_written_literal(source, &name, source_property);
                widened |= written.is_some();
                unwidened.push((name.clone(), written.unwrap_or(source_property)));
                discriminants.push((name, source_property));
            }
        }
        if discriminants.is_empty() {
            return Some(target);
        }
        let discriminated =
            self.discriminate_type_by_discriminable_items(target, &types, &discriminants)?;
        if widened
            && self.discriminate_type_by_discriminable_items(target, &types, &unwidened)?
                != discriminated
        {
            return None;
        }
        Some(discriminated)
    }

    /// The literal written for the object literal's member `name` when the
    /// member is typed as that literal's widened base. Upstream widens a fresh
    /// literal only when its contextual type holds no literal of its kind
    /// (`getWidenedLiteralLikeTypeForContextualType`) and never widens a
    /// non-fresh one such as a `const`'s, while this port's object-literal
    /// typing can widen either (`{ kind }` with `const kind = "a"` is
    /// `{ kind: string }`, `contextuallyTypedByDiscriminableUnion`). Which of
    /// the two types upstream has is not decidable here, so
    /// [`Checker::find_matching_discriminant_type`] answers only when both
    /// discriminate alike.
    fn discriminant_member_written_literal(
        &mut self,
        source: TypeId,
        name: &str,
        source_property: TypeId,
    ) -> Option<TypeId> {
        if self.is_literal_type_for_discriminant(source_property) {
            return None;
        }
        let TypeData::Named { members: Some(owner), .. } = self.type_of(source).data else {
            return None;
        };
        let &literal = self.binder.symbols().get(owner).declarations.first()?;
        let Some(Node::ObjectLiteralExpression(object)) = self.node_map.get(literal) else {
            return None;
        };
        let value = object.properties.iter().find_map(|property| match property {
            tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment)
                if assignment.name.node_id().and_then(|id| self.identifier_text(id))
                    == Some(name) =>
            {
                assignment.initializer.and_then(|value| value.node_id())
            }
            tsr_ast::ObjectLiteralElementLike::ShorthandPropertyAssignment(shorthand)
                if shorthand.name.node_id().and_then(|id| self.identifier_text(id))
                    == Some(name) =>
            {
                shorthand.name.node_id()
            }
            _ => None,
        })?;
        let written = self.check_expression_at_node(value);
        (self.type_of(written).flags.intersects(TypeFlags::UNIT)
            && self.get_base_type_of_literal_type(written) == source_property)
            .then(|| self.get_regular_type_of_literal_type(written))
    }

    /// `discriminateTypeByDiscriminableItems` (`relater.go:1212`) with
    /// `findMatchingDiscriminantType`'s `TypeDiscriminator`: a constituent's
    /// discriminant matches when some constituent of the source member's type
    /// is not `TernaryFalse`-related to it. An `Unknown` relation is
    /// undecidable (`None`).
    fn discriminate_type_by_discriminable_items(
        &mut self,
        target: TypeId,
        types: &[TypeId],
        discriminants: &[(String, TypeId)],
    ) -> Option<TypeId> {
        #[derive(Clone, Copy, PartialEq)]
        enum Include {
            False,
            True,
            Maybe,
        }
        let mut include = Vec::with_capacity(types.len());
        for &t in types {
            let keep = !self.type_of(t).flags.intersects(TypeFlags::PRIMITIVE)
                && !self.intersection_has_never_discriminant(t);
            include.push(if keep { Include::True } else { Include::False });
        }
        for (name, source_property) in discriminants {
            let sources = match &self.type_of(*source_property).data {
                TypeData::Union { types, .. } => types.clone(),
                _ => vec![*source_property],
            };
            let mut matched = false;
            for (index, &t) in types.iter().enumerate() {
                if include[index] == Include::False {
                    continue;
                }
                let Some(target_type) = self.type_of_property_or_index_signature(t, name).ok()?
                else {
                    continue;
                };
                let mut related = false;
                for &s in &sources {
                    match self.relate_ternary(s, target_type, crate::relater::Relation::Assignable)
                    {
                        crate::relater::Ternary::NotRelated => {}
                        crate::relater::Ternary::Related => {
                            related = true;
                            break;
                        }
                        crate::relater::Ternary::Unknown => return None,
                    }
                }
                if related {
                    matched = true;
                } else {
                    include[index] = Include::Maybe;
                }
            }
            for state in &mut include {
                if *state == Include::Maybe {
                    *state = if matched { Include::False } else { Include::True };
                }
            }
        }
        if include.contains(&Include::False) {
            let filtered: Vec<TypeId> = types
                .iter()
                .zip(&include)
                .filter(|(_, state)| **state == Include::True)
                .map(|(&t, _)| t)
                .collect();
            let filtered = self.get_union_type(&filtered);
            if !self.type_of(filtered).flags.contains(TypeFlags::NEVER) {
                return Some(filtered);
            }
        }
        Some(target)
    }

    /// `getIndexedAccessTypeOrUndefined(target, nameType)`'s index-signature
    /// arm (`getPropertyTypeForIndexType`, `checker.go`) for a member name the
    /// target has no property for: the value of `getApplicableIndexInfo` for
    /// the name's literal type (`getLiteralTypeFromPropertyName` — a numeric
    /// name is a number literal, any other a string literal).
    fn elaboration_index_value(
        &mut self,
        target: TypeId,
        name_id: NodeId,
        name: &str,
    ) -> Option<TypeId> {
        let name_type = if self.nodes.kind(name_id) == SyntaxKind::NumericLiteral {
            let literal = self.check_expression_at_node(name_id);
            self.get_regular_type_of_literal_type(literal)
        } else {
            self.store.intern_literal(
                TypeFlags::STRING_LITERAL,
                TypeData::StringLiteral(name.to_owned()),
                false,
            )
        };
        self.get_applicable_index_info(target, name_type).map(|info| info.value)
    }

    /// `elaborateArrayLiteral` (`relater.go:522`): each element is an element
    /// of the tuple-like source, keyed by its index, reported at
    /// `getEffectiveCheckNode` of the element. A non-tuple source is re-read
    /// upstream as a contextually typed tuple; each element's checked type is
    /// that tuple's member here; with spreads, the forced tuple is
    /// [`Checker::forced_tuple_entries`]. A union target reads each element
    /// through [`Checker::union_array_literal_target_element`].
    fn elaborate_array_literal(&mut self, node: NodeId, source: TypeId, target: TypeId) -> bool {
        let Some(Node::ArrayLiteralExpression(literal)) = self.node_map.get(node) else {
            return false;
        };
        let target_flags = self.type_of(target).flags;
        if target_flags.intersects(TypeFlags::PRIMITIVE | TypeFlags::NEVER) {
            return false;
        }
        let elements: Vec<NodeId> =
            literal.elements.iter().filter_map(tsr_ast::Expression::node_id).collect();
        let source_tuple = self.tuple_element_lists.contains_key(&source);
        let forced = if source_tuple
            || !elements.iter().any(|&id| self.nodes.kind(id) == SyntaxKind::SpreadElement)
        {
            None
        } else {
            let Some(entries) = self.forced_tuple_entries(&elements) else { return false };
            // `getTupleTargetType`: a lone rest element is the array type
            // itself, which `isTupleLikeType` rejects, so nothing elaborates.
            if let [(_, true)] = entries.as_slice() {
                return false;
            }
            Some(entries)
        };
        // Every element's target type is settled before anything is
        // reported, so a decline never follows a partial report.
        let mut targets = Vec::with_capacity(elements.len());
        if target_flags.contains(TypeFlags::UNION) {
            let mut best = BestMatch::Unasked;
            for index in 0..elements.len() {
                let Ok(member) = self.union_array_literal_target_element(
                    node,
                    target,
                    index,
                    elements.len(),
                    &mut best,
                ) else {
                    return false;
                };
                targets.push(member);
            }
        } else {
            for index in 0..elements.len() {
                targets.push(self.array_literal_target_element(target, index));
            }
        }
        let mut reported = false;
        for ((index, element), target_element) in elements.into_iter().enumerate().zip(targets) {
            if self.nodes.kind(element) == SyntaxKind::OmittedExpression {
                continue;
            }
            let Some(target_element) = target_element else { continue };
            let check_node = self.effective_check_node(element);
            let source_element = if let Some(entries) = &forced {
                let Some(member) = self.forced_tuple_element(entries, index) else { continue };
                member
            } else if source_tuple {
                let Some(member) = self.get_type_of_property_of_type(source, &index.to_string())
                else {
                    continue;
                };
                member
            } else {
                self.check_expression_at_node(check_node)
            };
            reported |= self.elaborate_element(
                check_node,
                Some(check_node),
                source_element,
                target_element,
            );
        }
        reported
    }

    /// The element list of the tuple `checkArrayLiteral` builds under
    /// `CheckModeForceTuple` for a literal with spreads: an ordinary element
    /// is its checked type; a spread of a tuple contributes its elements in
    /// place (`createTupleTypeEx` normalizes a variadic tuple element); a
    /// spread of an array or iterable contributes one rest element of its
    /// element type. `true` marks the rest element. `None` where TSR's
    /// operand type is not the forced one: an array-literal operand of more
    /// than one element (native reads it as a tuple, TSR as an array), an
    /// optional or variadic tuple or any other tuple-like operand that is not
    /// an `Array` reference, a second rest element (normalization merges
    /// them), or an operand with no element type.
    fn forced_tuple_entries(&mut self, elements: &[NodeId]) -> Option<Vec<(TypeId, bool)>> {
        let mut entries = Vec::with_capacity(elements.len());
        let mut rest = false;
        for &element in elements {
            let Some(Node::SpreadElement(spread)) = self.node_map.get(element) else {
                if self.nodes.kind(element) == SyntaxKind::OmittedExpression {
                    entries.push((self.intrinsics.undefined, false));
                } else {
                    let check_node = self.effective_check_node(element);
                    entries.push((self.check_expression_at_node(check_node), false));
                }
                continue;
            };
            let expression = spread.expression?.node_id()?;
            let operand = self.check_expression_at_node(expression);
            if let Some((list, _)) = self.tuple_element_lists.get(&operand).cloned() {
                if self.variadic_tuple_elements.contains_key(&operand)
                    || self
                        .tuple_optional_masks
                        .get(&operand)
                        .is_some_and(|mask| mask.iter().any(|&optional| optional))
                {
                    return None;
                }
                entries.extend(list.into_iter().map(|t| (t, false)));
                continue;
            }
            // A variadic or other tuple-like operand normalizes its fixed
            // elements in place, which this list does not model.
            if self.tuple_array_like(operand) && self.tuple_spread_array_element(operand).is_none()
            {
                return None;
            }
            let element_type = self.array_spread_element_type(operand)?;
            if let Some(Node::ArrayLiteralExpression(inner)) = self.node_map.get(expression) {
                let [only] = inner.elements else { return None };
                if only.node_id().is_none_or(|id| {
                    matches!(
                        self.nodes.kind(id),
                        SyntaxKind::SpreadElement | SyntaxKind::OmittedExpression
                    )
                }) {
                    return None;
                }
                entries.push((element_type, false));
                continue;
            }
            if std::mem::replace(&mut rest, true) {
                return None;
            }
            entries.push((element_type, true));
        }
        Some(entries)
    }

    /// `getIndexedAccessTypeOrUndefined(forcedTuple, index)`: a fixed
    /// position before the rest element reads its element; from the rest
    /// element on, `getTupleElementTypeOutOfStartCount` is the union of the
    /// element list from `index` (the rest and every trailing element).
    /// `None` past the end, `sourcePropType == nil`.
    fn forced_tuple_element(&mut self, entries: &[(TypeId, bool)], index: usize) -> Option<TypeId> {
        let first_rest = entries.iter().position(|&(_, rest)| rest);
        let tail = entries.get(index..).filter(|tail| !tail.is_empty())?;
        if first_rest.is_none_or(|rest| index < rest) {
            return Some(tail[0].0);
        }
        let types: Vec<TypeId> = tail.iter().map(|&(t, _)| t).collect();
        Some(self.get_union_type(&types))
    }

    /// `getIndexedAccessTypeOrUndefined(target, index)` for one element of
    /// an array literal against a non-union target, with
    /// `elaborateArrayLiteral`'s skip of an index a tuple-like target has no
    /// property for. `None` skips the element.
    fn array_literal_target_element(&mut self, target: TypeId, index: usize) -> Option<TypeId> {
        // A variadic tuple's properties are its leading fixed elements
        // (generateLimitedTupleElements skips an index the tuple-like target
        // has no property for).
        if let Some((elements, _)) = self.variadic_tuple_elements.get(&target) {
            let fixed: Vec<_> = elements
                .iter()
                .take_while(|element| !element.spread)
                .map(|element| (element.r#type, element.optional))
                .collect();
            let &(t, optional) = fixed.get(index)?;
            return Some(if optional && self.strict_null_checks {
                self.get_union_type(&[t, self.intrinsics.undefined])
            } else {
                t
            });
        }
        if let Some((list, _)) = self.tuple_element_lists.get(&target) {
            // isTupleLikeType(target) && no property `index`: skipped.
            if index >= list.len() {
                return None;
            }
            return self.get_type_of_property_of_type(target, &index.to_string());
        }
        if let Some(element_type) = self.tuple_spread_array_element(target) {
            return Some(element_type);
        }
        // getIndexedAccessTypeOrUndefined(target, i) on a non-array object
        // target: a property named `i`, else the applicable (numeric or
        // string) index signature; neither skips it.
        let name = index.to_string();
        let index_type = self.store.intern_literal(
            TypeFlags::NUMBER_LITERAL,
            TypeData::NumberLiteral(name.clone()),
            false,
        );
        self.get_type_of_property_of_type(target, &name)
            .or_else(|| self.get_applicable_index_info(target, index_type).map(|info| info.value))
    }

    /// `getBestMatchIndexedAccessTypeOrUndefined` (`relater.go:620`) for an
    /// array-literal element against a union target. The union's own
    /// indexed access answers when every constituent has the element
    /// (`getPropertyOfType` on a union, index signatures included); an
    /// index a tuple-like union (`isTupleLikeType`: the union has a property
    /// `"0"`) lacks is skipped. Otherwise the element is read from
    /// `getBestMatchingType`'s constituent,
    /// [`Checker::best_matching_type_for_array_literal`], computed once into
    /// `best`. `Ok(None)` skips the element; `Err` declines.
    fn union_array_literal_target_element(
        &mut self,
        node: NodeId,
        target: TypeId,
        index: usize,
        count: usize,
        best: &mut BestMatch,
    ) -> Result<Option<TypeId>, ()> {
        let TypeData::Union { types, .. } = self.type_of(target).data.clone() else {
            return Err(());
        };
        let mut found = Vec::with_capacity(types.len());
        for &part in &types {
            if !self.type_of(part).flags.intersects(TypeFlags::OBJECT) {
                break;
            }
            match self.array_literal_target_element(part, index) {
                Some(member) => found.push(member),
                None => break,
            }
        }
        if found.len() == types.len() {
            return Ok(Some(self.get_union_type(&found)));
        }
        let tuple_like = types.iter().any(|part| {
            self.tuple_element_lists.get(part).is_some_and(|(list, _)| !list.is_empty())
        }) && types.iter().all(|&part| {
            self.type_of(part).flags.intersects(TypeFlags::OBJECT)
                && self.array_literal_target_element(part, 0).is_some()
        });
        if tuple_like {
            return Ok(None);
        }
        let best = if let BestMatch::Chosen(best) = *best {
            best
        } else {
            let chosen = self.best_matching_type_for_array_literal(node, target, count)?;
            *best = BestMatch::Chosen(chosen);
            chosen
        };
        Ok(best.and_then(|best| self.array_literal_target_element(best, index)))
    }

    /// `getBestMatchingType` (`relater.go:879`) for an array literal, whose
    /// source `elaborateArrayLiteral` reads as a forced tuple (a plain,
    /// mutable, unlabeled tuple of the literal's arity). Ported for the
    /// shapes where every arm is decidable here, `Err` otherwise:
    ///
    /// - constituents other than objects are `undefined`, `null` or `void`.
    ///   Those are primitives, which `findMostOverlappyType` skips, and they
    ///   have no members, so no union property — and so no discriminant for
    ///   `findMatchingDiscriminantType` — spans them;
    /// - `findMatchingTypeReferenceOrTypeAliasReference`: a tuple constituent
    ///   with the forced tuple's target (same arity, every element required,
    ///   not readonly, no labels);
    /// - `findBestTypeForObjectLiteral` and `findBestTypeForInvokable` never
    ///   match a tuple source;
    /// - `findMostOverlappyType`: the one object constituent whose keys
    ///   overlap the tuple's ([`Checker::overlaps_tuple_keys`]; an array or
    ///   tuple always does), every other one sharing no key and so skipped;
    ///   nil when none overlaps. Two overlapping constituents would need the
    ///   overlap counts compared and decline, as do a literal in a const
    ///   context (a readonly tuple) and any other shape.
    fn best_matching_type_for_array_literal(
        &mut self,
        node: NodeId,
        target: TypeId,
        count: usize,
    ) -> Result<Option<TypeId>, ()> {
        let TypeData::Union { types, .. } = self.type_of(target).data.clone() else {
            return Ok(None);
        };
        if self.is_const_context(node) {
            return Err(());
        }
        let mut objects = Vec::with_capacity(types.len());
        for &part in &types {
            let flags = self.type_of(part).flags;
            if flags.intersects(TypeFlags::NULLABLE | TypeFlags::VOID) {
                continue;
            }
            if !flags.contains(TypeFlags::OBJECT) {
                return Err(());
            }
            objects.push(part);
        }
        let plain_tuple_of_arity = |checker: &Self, part: TypeId| {
            checker.tuple_element_lists.get(&part).is_some_and(|(list, readonly)| {
                list.len() == count
                    && !readonly
                    && !checker.variadic_tuple_elements.contains_key(&part)
                    && checker
                        .tuple_optional_masks
                        .get(&part)
                        .is_none_or(|mask| mask.iter().all(|optional| !optional))
                    && checker
                        .tuple_labels
                        .get(&part)
                        .is_none_or(|labels| labels.iter().all(Option::is_none))
            })
        };
        if let Some(&matched) = objects.iter().find(|&&part| plain_tuple_of_arity(self, part)) {
            return Ok(Some(matched));
        }
        let mut overlapping = None;
        for &part in &objects {
            let overlaps = if self.tuple_element_lists.contains_key(&part)
                || self.variadic_tuple_elements.contains_key(&part)
                || self.tuple_spread_array_element(part).is_some()
            {
                true
            } else {
                self.overlaps_tuple_keys(part).ok_or(())?
            };
            if overlaps && overlapping.replace(part).is_some() {
                return Err(());
            }
        }
        Ok(overlapping)
    }

    /// Whether `findMostOverlappyType` counts `t` against a tuple source:
    /// `keyof t` meets the tuple's keys (its element names, `number`,
    /// `length` and the global `Array` members) in at least one literal key.
    /// `Some(true)` when a certified property name is numeric, `length` or
    /// an `Array` member (the overlap is that unit or a union holding it);
    /// `Some(false)` when no name is and there is no index signature (the
    /// overlap is `never`, and the constituent is skipped). An index
    /// signature alone gives a non-literal overlap, which this port does
    /// not weigh, and an uncertified table answers `None`.
    fn overlaps_tuple_keys(&mut self, t: TypeId) -> Option<bool> {
        if self.type_of(t).flags.intersects(TypeFlags::INSTANTIABLE) {
            return None;
        }
        let names = self.certified_property_names(t)?;
        let array = self.global_type_symbol("Array")?;
        let array = self.binder.merged_symbol(array);
        for name in &names {
            if name == "length"
                || crate::index_signatures::is_numeric_literal_name(name)
                || self.binder.symbols().get(array).members.contains_key(name.as_str())
            {
                return Some(true);
            }
        }
        if !self.get_index_infos_of_type(t)?.is_empty() {
            return None;
        }
        Some(false)
    }

    /// `elaborateArrowFunction` (`relater.go:641`): an expression-bodied arrow
    /// with no annotated parameter relates its single call signature's return
    /// type to the union of the target's call signature returns, elaborating
    /// into the body and otherwise reporting at it.
    fn elaborate_arrow_function(&mut self, node: NodeId, source: TypeId, target: TypeId) -> bool {
        let Some(Node::ArrowFunction(arrow)) = self.node_map.get(node) else { return false };
        let Some(body) = arrow.body.and_then(|body| body.node_id()) else { return false };
        if self.nodes.kind(body) == SyntaxKind::Block
            || arrow.parameters.iter().any(|parameter| parameter.r#type.is_some())
        {
            return false;
        }
        let Some(signature) = self.single_call_signature(source) else { return false };
        let Some(targets) =
            self.signatures_of_type_kind(target, crate::signatures::SignatureKind::Call)
        else {
            return false;
        };
        if targets.is_empty() {
            return false;
        }
        let Some(source_return) = self.get_return_type_of_signature(&signature) else {
            return false;
        };
        let mut returns = Vec::with_capacity(targets.len());
        for target_signature in &targets {
            let Some(target_return) = self.get_return_type_of_signature(target_signature) else {
                return false;
            };
            returns.push(target_return);
        }
        let target_return = self.get_union_type(&returns);
        if self.relate_ternary(source_return, target_return, crate::relater::Relation::Assignable)
            != crate::relater::Ternary::NotRelated
        {
            return false;
        }
        // `elaborateError(returnExpression, …)` first: an inner elaboration
        // carries none of the related information below.
        if self.elaborate_error(body, source_return, target_return, None) {
            return true;
        }
        let before = self.diagnostics.len();
        if !self.elaborate_element(body, Some(body), source_return, target_return) {
            return false;
        }
        if self.diagnostics.len() == before + 1 {
            let async_arrow = has_async(arrow.modifiers);
            self.add_arrow_return_related_info(
                before,
                node,
                async_arrow,
                source_return,
                target,
                target_return,
            );
        }
        true
    }

    /// `elaborateArrowFunction`'s related information (`relater.go:667`):
    /// `The expected type comes from the return type of this signature` at
    /// the target's first declaration, then `Did you mean to mark this
    /// function as 'async'?` at the arrow when a non-async arrow's return,
    /// promised, would have been assignable (r7-reports §39).
    fn add_arrow_return_related_info(
        &mut self,
        index: usize,
        arrow: NodeId,
        async_arrow: bool,
        source_return: TypeId,
        target: TypeId,
        target_return: TypeId,
    ) {
        // Appended after what the relation already attached (`AddRelatedInfo`).
        let mut list = Vec::new();
        // `target.symbol`: an anonymous type's own symbol, or the declared
        // type's; a union or intersection has none.
        let symbol = match &self.type_of(target).data {
            crate::types::TypeData::Anonymous { symbol, .. } => Some(*symbol),
            crate::types::TypeData::Named { members, .. } => *members,
            _ => None,
        };
        if let Some(&declaration) =
            symbol.and_then(|symbol| self.binder.symbols().get(symbol).declarations.first())
        {
            let span = self.error_span(declaration);
            let related = self.related_diagnostic_at(
                declaration,
                &messages::THE_EXPECTED_TYPE_COMES_FROM_THE_RETURN_TYPE_OF_THIS_SIGNATURE,
                span,
                Vec::new(),
            );
            if related.file().is_some() {
                list.push(related);
            }
        }
        // `createPromiseType(sourceReturn)` is `Promise<Awaited<T>>`; for a
        // type with no `then` and nothing generic, `Awaited<T>` is `T`. A
        // generic return would need the `Awaited` alias, so the hint declines.
        if !async_arrow
            && self.get_type_of_property_of_type(source_return, "then").is_none()
            && !self.is_generic_type(source_return)
            && let Some(promise) = self.global_type_symbol_with_arity("Promise", 1)
        {
            let promised = self.create_type_reference(promise, vec![source_return]);
            if self.relate_ternary(promised, target_return, crate::relater::Relation::Assignable)
                != crate::relater::Ternary::NotRelated
            {
                let span = self.error_span(arrow);
                let related = self.related_diagnostic_at(
                    arrow,
                    &messages::DID_YOU_MEAN_TO_MARK_THIS_FUNCTION_AS_ASYNC,
                    span,
                    Vec::new(),
                );
                if related.file().is_some() {
                    list.push(related);
                }
            }
        }
        if !list.is_empty() {
            let diagnostic = &mut self.diagnostics[index].1;
            let mut related = diagnostic.related_information().to_vec();
            related.extend(list);
            diagnostic.set_related_information(std::sync::Arc::new(related));
        }
    }
}

/// Does this modifier list carry `async`?
fn has_async(modifiers: &[tsr_ast::ModifierLike<'_>]) -> bool {
    modifiers.iter().any(|modifier| {
        matches!(modifier, tsr_ast::ModifierLike::Token(token) if token.kind == SyntaxKind::AsyncKeyword)
    })
}

/// The target flags that no object type can ever satisfy.
///
/// `void`, `null` and `undefined` are absent: their relation to an object source
/// depends on `strictNullChecks` and on `void`'s own arm, and this list must
/// hold only the pairs that are unrelated under every configuration.
const PRIMITIVE_TARGET: TypeFlags = TypeFlags::STRING
    .union(TypeFlags::NUMBER)
    .union(TypeFlags::BOOLEAN)
    .union(TypeFlags::BIG_INT)
    .union(TypeFlags::ES_SYMBOL)
    .union(TypeFlags::STRING_LITERAL)
    .union(TypeFlags::NUMBER_LITERAL)
    .union(TypeFlags::BOOLEAN_LITERAL)
    .union(TypeFlags::BIG_INT_LITERAL);

/// Is the TS2741 arm live?
///
/// **`true` since §41.2.** §22 refused it at 1 conversion for 8 wrong lines,
/// measured against a `declared_property_table` that declined instantiated
/// references, generic declarations and every `Anonymous` receiver. §35, §38.2
/// and §41.1 removed all three; re-running the switch is now positive.
///
/// A constant rather than a deletion, and the distinction is the point: the
/// machinery it switches — [`Checker::missing_required_property`] and
/// [`crate::member_completeness`]'s property enumeration — is *correct* and is
/// what four named residual families stand between and a positive score. §22
/// lists them. Deleting the code would make the refusal unrevisitable, which is
/// the one thing `docs/conventions.md` forbids about a refusal.
const REPORT_MISSING_REQUIRED_PROPERTY: bool = true;

/// The retraced chain under a head. A type-parameter head target's
/// `reportRelationError` has already put its explanation under the head:
/// TS5082 resets the chain ("only report this error once"), so nothing
/// follows it; TS5075 is pushed above the chain, so the chain goes under
/// it (r7-reports §60).
fn add_relation_chain(diagnostic: &mut Diagnostic, chain: Option<Diagnostic>) {
    let Some(chain) = chain else { return };
    let mut children = diagnostic.message_chain().to_vec();
    if let Some(last) = children.last_mut() {
        if last.message.code()
            == messages::_0_COULD_BE_INSTANTIATED_WITH_AN_ARBITRARY_TYPE_WHICH_COULD_BE_UNRELATED_TO_1.code()
        {
            return;
        }
        if last.message.code()
            == messages::_0_IS_ASSIGNABLE_TO_THE_CONSTRAINT_OF_TYPE_1_BUT_1_COULD_BE_INSTANTIATED_WITH_A_DIFFERENT_SUBTYPE_OF_CONSTRAINT_2.code()
        {
            last.add_message_chain(Some(chain));
            diagnostic.set_message_chain(children);
            return;
        }
    }
    diagnostic.add_message_chain(Some(chain));
}

/// `isConversionOrInterfaceImplementationMessage` (`relater.go:4934`): a
/// head `reportRelationError` keeps over the missing-property message below.
fn is_conversion_or_interface_implementation_message(message: &tsr_diagnostics::Message) -> bool {
    [
        &messages::CLASS_0_INCORRECTLY_IMPLEMENTS_INTERFACE_1,
        &messages::CLASS_0_INCORRECTLY_IMPLEMENTS_CLASS_1_DID_YOU_MEAN_TO_EXTEND_1_AND_INHERIT_ITS_MEMBERS_AS_A_SUBCLASS,
        &messages::CONVERSION_OF_TYPE_0_TO_TYPE_1_MAY_BE_A_MISTAKE_BECAUSE_NEITHER_TYPE_SUFFICIENTLY_OVERLAPS_WITH_THE_OTHER_IF_THIS_WAS_INTENTIONAL_CONVERT_THE_EXPRESSION_TO_UNKNOWN_FIRST,
        &messages::ITS_INSTANCE_TYPE_0_IS_NOT_A_VALID_JSX_ELEMENT,
        &messages::ITS_RETURN_TYPE_0_IS_NOT_A_VALID_JSX_ELEMENT,
        &messages::ITS_ELEMENT_TYPE_0_IS_NOT_A_VALID_JSX_ELEMENT,
    ]
    .iter()
    .any(|candidate| candidate.code() == message.code())
}

/// One `ErrorChain` entry (`relater.go`): a message and its arguments, as
/// `reportError` pushes them.
#[derive(Clone)]
struct ChainEntry {
    message: &'static tsr_diagnostics::Message,
    args: Vec<String>,
}

/// `errorChain` and `relatedInfo` as one retrace accumulates them.
#[derive(Default)]
struct ChainStack {
    entries: Vec<ChainEntry>,
    related: Vec<Diagnostic>,
    /// `IntersectionStateTarget`: explaining one constituent of a target
    /// intersection, where nested pairs skip the common-property check.
    intersection_target: bool,
    /// The pairs being explained, outermost first, with their intersection
    /// state: the walk's maybe stack when a member relation is asked.
    assumed: Vec<(TypeId, TypeId, bool)>,
    /// Per `assumed` pair: entered as a constraint descent, which pushes
    /// its source only (`RecursionFlagsSource`).
    assumed_source_only: Vec<bool>,
    /// The next pair `explain_pair` enters is a constraint descent.
    source_only_next: bool,
}

impl std::ops::Deref for ChainStack {
    type Target = Vec<ChainEntry>;
    fn deref(&self) -> &Vec<ChainEntry> {
        &self.entries
    }
}

impl std::ops::DerefMut for ChainStack {
    fn deref_mut(&mut self) -> &mut Vec<ChainEntry> {
        &mut self.entries
    }
}

/// The four return-type markers `compareSignaturesRelated` reports only to
/// steer `reportError`'s chain reduction; `ElidedInCompatibilityPyramid`
/// drops them from the final diagnostic.
fn is_return_marker(message: &'static tsr_diagnostics::Message) -> bool {
    [
        &messages::CALL_SIGNATURE_RETURN_TYPES_0_AND_1_ARE_INCOMPATIBLE,
        &messages::CONSTRUCT_SIGNATURE_RETURN_TYPES_0_AND_1_ARE_INCOMPATIBLE,
        &messages::CALL_SIGNATURES_WITH_NO_ARGUMENTS_HAVE_INCOMPATIBLE_RETURN_TYPES_0_AND_1,
        &messages::CONSTRUCT_SIGNATURES_WITH_NO_ARGUMENTS_HAVE_INCOMPATIBLE_RETURN_TYPES_0_AND_1,
    ]
    .iter()
    .any(|marker| marker.code() == message.code())
}

fn is_excess_property_message(message: &'static tsr_diagnostics::Message) -> bool {
    std::ptr::eq(message, &raw const messages::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_0_DOES_NOT_EXIST_IN_TYPE_1)
        || std::ptr::eq(message, &raw const messages::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_BUT_0_DOES_NOT_EXIST_IN_TYPE_1_DID_YOU_MEAN_TO_WRITE_2,
        )
}

/// `getPropertyNameArg` (`relater.go:4926`).
fn property_name_arg(name: &str) -> String {
    if name.starts_with(['"', '\'', '`']) { format!("[{name}]") } else { name.to_owned() }
}

/// `addToDottedName` (`relater.go:4879`).
fn add_to_dotted_name(head: &str, tail: &str) -> String {
    let head = if head.starts_with("new ") { format!("({head})") } else { head.to_owned() };
    let mut pos = 0;
    loop {
        if tail[pos..].starts_with('(') {
            pos += 1;
        } else if tail[pos..].starts_with("new ") {
            pos += 4;
        } else {
            break;
        }
    }
    let (prefix, suffix) = tail.split_at(pos);
    if suffix.starts_with('[') {
        format!("{prefix}{head}{suffix}")
    } else {
        format!("{prefix}{head}.{suffix}")
    }
}

/// The relation error chain: `checkTypeRelatedToEx` re-runs a failed relation
/// with `reportErrors`, and every arm on the failing path pushes its message
/// onto `errorChain`; the outermost push is the head (r7-reports §36).
///
/// This port's relater keeps no chain, so the failing path is retraced here
/// over decided answers only (`relate_ternary`). Each arm mirrors the native
/// arm it names; any step whose failing member, constituent or signature
/// this port cannot pin down abandons the whole chain, so a partial or
/// guessed chain is never reported.
impl Checker<'_, '_> {
    /// The chain under the head for a decided `source -> target` failure,
    /// outermost first, or `None` where it cannot be retraced.
    pub(crate) fn relation_failure_chain(
        &mut self,
        at: NodeId,
        span: tsr_core::Span,
        source: TypeId,
        target: TypeId,
    ) -> Option<Diagnostic> {
        let mut stack = ChainStack::default();
        stack.assumed.push((source, target, false));
        stack.assumed_source_only.push(false);
        self.explain_relation(&mut stack, source, target, 0)?;
        stack.assumed.pop();
        stack.assumed_source_only.pop();
        self.chain_error_results(&mut stack, source, target);
        let related = std::mem::take(&mut stack.related);
        let entries: Vec<ChainEntry> = stack
            .entries
            .into_iter()
            .rev()
            .filter(|entry| !is_return_marker(entry.message))
            .collect();
        if entries.is_empty() {
            return None;
        }
        // `NewDiagnosticChain` carries the innermost diagnostic's file and
        // location, the error node's, to every level.
        let file = self
            .related_diagnostic_at(at, entries[0].message, span, Vec::new())
            .file()
            .map(|file| std::sync::Arc::new(file.clone()));
        let related = std::sync::Arc::new(related);
        let mut chain: Option<Diagnostic> = None;
        for entry in entries.into_iter().rev() {
            let mut diagnostic = Diagnostic::with_args(entry.message, span, entry.args);
            if let Some(file) = &file {
                diagnostic.set_file(file.clone());
            }
            if !related.is_empty() {
                diagnostic.set_related_information(related.clone());
            }
            diagnostic.add_message_chain(chain);
            chain = Some(diagnostic);
        }
        chain
    }

    /// `reportRelationError` (`relater.go:4751`) for an inner pair, with the
    /// suppressions keyed on the message below it. `None` where its text is
    /// not this port's to choose (a type-parameter target's explanations).
    fn chain_report_relation_error(
        &mut self,
        stack: &mut ChainStack,
        source: TypeId,
        target: TypeId,
    ) -> Option<()> {
        let target_flags = self.type_of(target).flags;
        let explains_type_parameter = if target_flags.contains(TypeFlags::INDEXED_ACCESS) {
            self.indexed_access_target_explained(source, target)?
        } else {
            target_flags.contains(TypeFlags::TYPE_PARAMETER)
        };
        let displayed_source = self.assignability_source_for_error_display(source, target);
        let mut source_text = self.type_to_string(displayed_source);
        let mut target_text = self.type_to_string(target);
        // The type-parameter target's explanation, pushed before the
        // relation error (`relater.go:4768`).
        if explains_type_parameter && !self.variance_marker_types.contains(&target) {
            let constraint = self.base_constraint_of_type(target);
            let assignable = constraint.and_then(|constraint| {
                if self.is_type_assignable_to(displayed_source, constraint) {
                    Some((displayed_source, constraint))
                } else if self.is_type_assignable_to(source, constraint) {
                    Some((source, constraint))
                } else {
                    None
                }
            });
            if let Some((assignable_source, constraint)) = assignable {
                let assignable_text = self.type_to_string(assignable_source);
                let constraint_text = self.type_to_string(constraint);
                stack.push(ChainEntry {
                    message: &messages::_0_IS_ASSIGNABLE_TO_THE_CONSTRAINT_OF_TYPE_1_BUT_1_COULD_BE_INSTANTIATED_WITH_A_DIFFERENT_SUBTYPE_OF_CONSTRAINT_2,
                    args: vec![assignable_text, target_text.clone(), constraint_text],
                });
            } else {
                // "Only report this error once": the chain below is dropped.
                stack.clear();
                stack.push(ChainEntry {
                    message: &messages::_0_COULD_BE_INSTANTIATED_WITH_AN_ARBITRARY_TYPE_WHICH_COULD_BE_UNRELATED_TO_1,
                    args: vec![target_text.clone(), source_text.clone()],
                });
            }
        }
        let mut same_name = false;
        if self.type_to_string(source) == target_text {
            let qualified_source = self.type_name_for_error_display(source);
            let qualified_target = self.type_name_for_error_display(target);
            same_name = qualified_source == qualified_target;
            if displayed_source == source {
                source_text = qualified_source;
            }
            target_text = qualified_target;
        }
        let message = if same_name {
            &messages::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1_TWO_DIFFERENT_TYPES_WITH_THIS_NAME_EXIST_BUT_THEY_ARE_UNRELATED
        } else if self.exact_optional_object_mismatch(source, target) {
            &messages::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1_WITH_EXACTOPTIONALPROPERTYTYPES_COLON_TRUE_CONSIDER_ADDING_UNDEFINED_TO_THE_TYPES_OF_THE_TARGET_S_PROPERTIES
        } else {
            if let Some(suggestion) =
                self.suggested_type_for_nonexistent_string_literal_type(source, target)
            {
                let suggestion = self.type_to_string(suggestion);
                stack.push(ChainEntry {
                    message: &messages::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1_DID_YOU_MEAN_2,
                    args: vec![source_text, target_text, suggestion],
                });
                return Some(());
            }
            &messages::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1
        };
        if let Some(top) = stack.last() {
            if is_excess_property_message(top.message) {
                return Some(());
            }
            if std::ptr::eq(
                top.message,
                &raw const messages::THE_TYPE_0_IS_READONLY_AND_CANNOT_BE_ASSIGNED_TO_THE_MUTABLE_TYPE_1,
            ) && top.args.first() == Some(&source_text)
                && top.args.get(1) == Some(&target_text)
            {
                return Some(());
            }
            if std::ptr::eq(
                top.message,
                &raw const messages::PROPERTY_0_IS_MISSING_IN_TYPE_1_BUT_REQUIRED_IN_TYPE_2,
            ) && top.args.get(1) == Some(&source_text)
                && top.args.get(2) == Some(&target_text)
            {
                return Some(());
            }
            if (std::ptr::eq(top.message, &raw const messages::TYPE_0_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_1_COLON_2_AND_3_MORE)
                || std::ptr::eq(top.message, &raw const messages::TYPE_0_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_1_COLON_2))
                && top.args.first() == Some(&source_text)
                && top.args.get(1) == Some(&target_text)
            {
                return Some(());
            }
        }
        stack.push(ChainEntry { message, args: vec![source_text, target_text] });
        Some(())
    }

    /// `isRelatedTo` with `reportErrors` for a decided failure: the failing
    /// arm's chain, then `reportErrorResults`' relation error for the pair.
    fn explain_pair(
        &mut self,
        stack: &mut ChainStack,
        source: TypeId,
        target: TypeId,
        depth: usize,
    ) -> Option<()> {
        let target = self.chain_normalized_target(source, target);
        // The common-property check fails the pair before any structural
        // comparison, with its own message and no relation error
        // (`relater.go:2731`).
        if !stack.intersection_target && self.fails_common_property_check(source, target) {
            let (message, args) = self.weak_type_message(source, target)?;
            stack.push(ChainEntry { message, args });
            return Some(());
        }
        let source_only = std::mem::take(&mut stack.source_only_next);
        stack.assumed.push((source, target, stack.intersection_target));
        stack.assumed_source_only.push(source_only);
        let explained = self.explain_relation(stack, source, target, depth + 1);
        stack.assumed.pop();
        stack.assumed_source_only.pop();
        explained?;
        self.chain_error_results(stack, source, target);
        self.chain_report_relation_error(stack, source, target)?;
        let hint = self.type_parameter_constraint_hint(source, target)?;
        stack.related.extend(hint);
        Some(())
    }

    /// `reportErrorResults`' switch (`relater.go:4717`), between the
    /// structural report and the relation error: an object source against
    /// its own primitive (`String` → `string`, `Number`, `Boolean`, the
    /// global `Symbol`) is TS2692; the global `Object` source is TS2696
    /// (r7-reports §47).
    fn chain_error_results(&mut self, stack: &mut ChainStack, source: TypeId, target: TypeId) {
        let source_flags = self.type_of(source).flags;
        if !source_flags.contains(TypeFlags::OBJECT) {
            return;
        }
        // `tryElaborateArrayLikeErrors` with `reportErrors`, ahead of the
        // switch: a readonly array or tuple against a mutable one is TS4104,
        // which then suppresses the pair's own relation error
        // (`chainArgsMatch`; r7-reports §97).
        if self.readonly_to_mutable_array_like(source, target) {
            let source_text = self.type_to_string(source);
            let target_text = self.type_to_string(target);
            stack.push(ChainEntry {
                message:
                    &messages::THE_TYPE_0_IS_READONLY_AND_CANNOT_BE_ASSIGNED_TO_THE_MUTABLE_TYPE_1,
                args: vec![source_text, target_text],
            });
            return;
        }
        // `source == c.globalStringType`: the global interface's own
        // declared type, read here through the symbol it names.
        let source_symbol = match &self.type_of(source).data {
            TypeData::Named { members: Some(symbol), .. } => {
                Some(self.binder.merged_symbol(*symbol))
            }
            _ => None,
        };
        let global = |checker: &mut Self, name: &str| {
            let symbol = checker.global_type_symbol_with_arity(name, 0)?;
            let symbol = checker.binder.merged_symbol(symbol);
            (source_symbol == Some(symbol) && !checker.type_reference_targets.contains_key(&source))
                .then_some(source)
        };
        if self.type_of(target).flags.intersects(TypeFlags::PRIMITIVE) {
            let pairs = [
                ("String", self.intrinsics.string),
                ("Number", self.intrinsics.number),
                ("Boolean", self.intrinsics.boolean),
                ("Symbol", self.intrinsics.es_symbol),
            ];
            for (name, primitive) in pairs {
                if target == primitive && global(self, name) == Some(source) {
                    let target_text = self.type_to_string(target);
                    let source_text = self.type_to_string(source);
                    stack.push(ChainEntry {
                        message: &messages::_0_IS_A_PRIMITIVE_BUT_1_IS_A_WRAPPER_OBJECT_PREFER_USING_0_WHEN_POSSIBLE,
                        args: vec![target_text, source_text],
                    });
                }
            }
            return;
        }
        if global(self, "Object") == Some(source) {
            stack.push(ChainEntry {
                message: &messages::THE_OBJECT_TYPE_IS_ASSIGNABLE_TO_VERY_FEW_OTHER_TYPES_DID_YOU_MEAN_TO_USE_THE_ANY_TYPE_INSTEAD,
                args: Vec::new(),
            });
        }
    }

    /// `reportErrorResults`' last arm (`relater.go:4741`): an unconstrained,
    /// declared type-parameter source gets TS2208 (*This type parameter might
    /// need an extends T constraint*) at its declaration when `T`, as its
    /// constraint, would not be circular. `Some(None)` where no hint applies;
    /// `None` where the circularity question is not decided here (an
    /// instantiable target other than a type parameter).
    #[allow(clippy::option_option)]
    fn type_parameter_constraint_hint(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<Option<Diagnostic>> {
        if !self.type_of(source).flags.contains(TypeFlags::TYPE_PARAMETER)
            || self.this_types.values().any(|&this| this == source)
            || self.this_type_nodes.values().any(|&this| this == source)
        {
            return Some(None);
        }
        let Some(&symbol) = self.type_parameter_symbols.get(&source) else { return Some(None) };
        let Some(&declaration) = self.binder.symbols().get(symbol).declarations.first() else {
            return Some(None);
        };
        if self.type_parameter_constraint(source).is_some() {
            return Some(None);
        }
        // `hasNonCircularBaseConstraint` of a clone constrained to the target
        // (with the source replaced by the clone): circular when the target
        // is a union or intersection with the source as a constituent.
        // A type-parameter target is the clone's constraint as written; its
        // own constraint chain does not reach the clone.
        let target_type = self.type_of(target);
        if target_type.flags.intersects(TypeFlags::INSTANTIABLE)
            && !target_type.flags.contains(TypeFlags::TYPE_PARAMETER)
        {
            return None;
        }
        // Except that instantiating `T & P` (P a primitive, `object` or `{}`)
        // asks `getBaseConstraintOfType` of the clone before its constraint
        // is set (`getIntersectionType`'s constraint reduction,
        // `checker.go:26131`), which caches "no constraint": that clone is
        // not circular (`NonNullable<T>`, `conditionalTypes1`; r7-reports §79).
        let circular = match target_type.data.clone() {
            TypeData::Union { types, .. } => types.contains(&source),
            TypeData::Intersection { types, .. } => {
                if let [first, second] = types[..]
                    && (first == source || second == source)
                {
                    let other = if first == source { second } else { first };
                    let flags = self.type_of(other).flags;
                    if flags.intersects(TypeFlags::TEMPLATE_LITERAL | TypeFlags::STRING_MAPPING) {
                        return None;
                    }
                    !(flags.intersects(TypeFlags::PRIMITIVE | TypeFlags::NON_PRIMITIVE)
                        || self.is_empty_anonymous_object_type(other))
                } else {
                    types.contains(&source)
                }
            }
            _ => false,
        };
        if circular {
            return Some(None);
        }
        let target_text = self.type_to_string(target);
        let span = self.error_span(declaration);
        let related = self.related_diagnostic_at(
            declaration,
            &messages::THIS_TYPE_PARAMETER_MIGHT_NEED_AN_EXTENDS_0_CONSTRAINT,
            span,
            vec![target_text],
        );
        Some(related.file().is_some().then_some(related))
    }

    /// `isRelatedTo`'s nullable stripping (`relater.go`): a definitely
    /// non-nullable source against `null | undefined | X` is related to `X`,
    /// and `reportErrorResults` names `X` unless the union has an alias
    /// (which prints without a `|`).
    fn chain_normalized_target(&mut self, source: TypeId, target: TypeId) -> TypeId {
        match self.non_nullable_union_candidate(source, target) {
            Some(candidate) if self.type_to_string(target).contains(" | ") => candidate,
            _ => target,
        }
    }

    fn chain_relate(
        &mut self,
        stack: &ChainStack,
        source: TypeId,
        target: TypeId,
    ) -> crate::relater::Ternary {
        if !stack.assumed.is_empty() {
            self.relate_ternary_assuming(
                source,
                target,
                &stack.assumed,
                &stack.assumed_source_only,
                stack.intersection_target,
            )
        } else if stack.intersection_target {
            self.relate_ternary_in_intersection_target(
                source,
                target,
                crate::relater::Relation::Assignable,
            )
        } else {
            self.relate_ternary(source, target, crate::relater::Relation::Assignable)
        }
    }

    /// [`Self::chain_relate`] for a type variable's constraint against the
    /// pair's target (`isRelatedToEx(constraint, target,
    /// RecursionFlagsSource)`, `relater.go:3672`), then the explanation of
    /// that pair when it fails, entered the same way (r7-reports §78).
    fn explain_constraint_descent(
        &mut self,
        stack: &mut ChainStack,
        constraint: TypeId,
        target: TypeId,
        depth: usize,
    ) -> Option<()> {
        use crate::relater::Ternary;
        let mut source_only = stack.assumed_source_only.clone();
        source_only.push(true);
        let answer = self.relate_ternary_assuming(
            constraint,
            target,
            &stack.assumed,
            &source_only,
            stack.intersection_target,
        );
        match answer {
            Ternary::NotRelated => {
                stack.source_only_next = true;
                self.explain_pair(stack, constraint, target, depth)
            }
            _ => None,
        }
    }

    /// The failing arm of `structuredTypeRelatedTo` for a decided failure,
    /// without the pair's own relation error.
    fn explain_relation(
        &mut self,
        stack: &mut ChainStack,
        source: TypeId,
        target: TypeId,
        depth: usize,
    ) -> Option<()> {
        use crate::relater::Ternary;
        if depth > 12 || self.is_gap(source) || self.is_gap(target) {
            return None;
        }
        let target = self.chain_normalized_target(source, target);
        // `getNormalizedType`'s reference arm: the structural comparison
        // reads a non-augmenting subtype as its single base, while the pair's
        // own relation error keeps the original (`reportErrorResults`), so
        // `Property 'foo' is private in type 'Base'` names the base of an
        // empty `interface I extends Base {}` (r7-reports §88).
        let source = self.single_base_normalized(source);
        let target = self.single_base_normalized(target);
        let source_flags = self.type_of(source).flags;
        let target_flags = self.type_of(target).flags;
        // The non-primitive `object` against an object type:
        // `structuredTypeRelatedToWorker` relates its apparent type, the
        // empty object type `{}` (`getApparentType`), whose structural
        // report names `{}` (r7-reports §92).
        if source_flags.contains(TypeFlags::NON_PRIMITIVE)
            && target_flags.contains(TypeFlags::OBJECT)
            && !target_flags.intersects(TypeFlags::UNION | TypeFlags::INTERSECTION)
        {
            let empty = self.intrinsics.empty_object;
            return self.explain_object_relation(stack, empty, target, depth, false);
        }
        // An `any` source fails only where `isSimpleTypeRelatedTo` decided
        // it (a `never` target), and nothing structural reports below
        // (r7-reports §90).
        if source_flags.intersects(TypeFlags::ANY) {
            return Some(());
        }
        // `isSimpleTypeRelatedTo`'s union-enum arm (`relater.go:243`) runs
        // `isEnumTypeRelatedTo` with the error reporter: its member walk's
        // first failure is the chain, and the union's members then relate
        // without reporting (r7-reports §85).
        if let (Some(source_enum), Some(target_enum)) =
            (self.chain_enum_symbol(source), self.chain_enum_symbol(target))
            && source_enum != target_enum
        {
            if let Some(entry) = self.enum_relation_failure(source_enum, target_enum) {
                stack.push(entry);
            }
            return Some(());
        }
        // `eachTypeRelatedToType`: the first source constituent that fails
        // (constituents in union order). A primitive union (`boolean`, an
        // enum) is related with `reportErrors` off (`relater.go:2873`).
        if let TypeData::Union { types, .. } = self.type_of(source).data.clone() {
            if source_flags.intersects(TypeFlags::PRIMITIVE) {
                return Some(());
            }
            for part in types {
                match self.chain_relate(stack, part, target) {
                    Ternary::Related => {}
                    Ternary::NotRelated => return self.explain_pair(stack, part, target, depth),
                    Ternary::Unknown => return None,
                }
            }
            return None;
        }
        // A primitive source: against a non-object, `isSimpleTypeRelatedTo`
        // decided it; against a union, `typeRelatedToSomeType` elaborates only
        // through `getBestMatchingType`, which has no candidate for a
        // primitive (`findMostOverlappyType` skips one). Nothing reports
        // below the relation error. Against an object,
        // `structuredTypeRelatedToWorker` relates the apparent type with
        // `reportStructuralErrors` off for a primitive source.
        // `unknown` is not structured and no simple arm relates it to
        // anything but `unknown`/`any`: nothing reports below (§77).
        if source_flags.contains(TypeFlags::UNKNOWN)
            && !target_flags.intersects(TypeFlags::INTERSECTION)
        {
            return Some(());
        }
        if source_flags.intersects(TypeFlags::PRIMITIVE)
            && !source_flags.intersects(TypeFlags::INSTANTIABLE)
            && !target_flags.intersects(TypeFlags::INTERSECTION | TypeFlags::INSTANTIABLE)
        {
            return Some(());
        }
        // A conditional target (`structuredTypeRelatedToWorker`'s target arm,
        // `relater.go:3594`) relates its branches with `reportErrors` off,
        // and a source that is not a type variable has no arm after it that
        // reports: nothing below the pair (r7-reports §100).
        if target_flags.contains(TypeFlags::CONDITIONAL)
            && !source_flags.intersects(TypeFlags::INSTANTIABLE | TypeFlags::UNION)
            && !(source_flags.contains(TypeFlags::INTERSECTION)
                && self.intersection_has_instantiable(source))
        {
            return Some(());
        }
        // A primitive against a template literal or string mapping: the
        // target arm matches the source's text and reports nothing below
        // (r7-reports §99).
        if source_flags.intersects(TypeFlags::PRIMITIVE)
            && !source_flags.intersects(TypeFlags::INSTANTIABLE)
            && target_flags.intersects(TypeFlags::TEMPLATE_LITERAL | TypeFlags::STRING_MAPPING)
        {
            return Some(());
        }
        // `typeRelatedToSomeType`: only `getBestMatchingType`'s pick is
        // elaborated, with no elaboration when there is none.
        if target_flags.contains(TypeFlags::UNION)
            && (source_flags.intersects(TypeFlags::OBJECT)
                && !source_flags.intersects(TypeFlags::INSTANTIABLE | TypeFlags::INTERSECTION)
                || source_flags.contains(TypeFlags::INTERSECTION)
                    && !self.intersection_has_instantiable(source))
        {
            let regular = self.get_regular_type_of_object_literal(source);
            return match self.chain_best_matching_type(regular, target)? {
                Some(best) => match self.chain_relate(stack, regular, best) {
                    Ternary::NotRelated => self.explain_pair(stack, regular, best, depth),
                    _ => None,
                },
                None => Some(()),
            };
        }
        // An intersection target (`typeRelatedToEachType`, reporting): the
        // first constituent that fails. When every one relates, `isRelatedTo`'s
        // extra `propertiesRelatedTo` over the intersection as one object
        // (`relater.go:2770`) is what failed.
        // An unconstrained type-parameter source fails here too, and its
        // fallthrough to the type-variable arm relates `unknown` without
        // reporting (`relater.go:3672`), so this arm's report is the chain.
        // A type-parameter constituent of the target is related as itself;
        // other instantiable or union constituents are normalized first, and
        // decline (r7-reports §79).
        let unconstrained_type_parameter = source_flags.contains(TypeFlags::TYPE_PARAMETER)
            && !self.this_types.values().any(|&this| this == source)
            && !self.this_type_nodes.values().any(|&this| this == source)
            && self
                .type_parameter_constraint(source)
                .is_none_or(|constraint| constraint == self.intrinsics.unknown);
        if let TypeData::Intersection { types, .. } = self.type_of(target).data.clone()
            && (source_flags.intersects(
                TypeFlags::OBJECT
                    | TypeFlags::INTERSECTION
                    | TypeFlags::PRIMITIVE
                    | TypeFlags::UNKNOWN,
            ) && !source_flags.intersects(TypeFlags::INSTANTIABLE)
                || unconstrained_type_parameter)
            && !(source_flags.contains(TypeFlags::INTERSECTION)
                && self.intersection_has_instantiable(source))
            && self.intersection_target_parts_explainable(&types)
        {
            let previous = std::mem::replace(&mut stack.intersection_target, true);
            for part in types {
                let explained = match self.chain_relate(stack, source, part) {
                    Ternary::Related => continue,
                    Ternary::NotRelated => self.explain_pair(stack, source, part, depth),
                    Ternary::Unknown => None,
                };
                stack.intersection_target = previous;
                return explained;
            }
            stack.intersection_target = previous;
            // Under `IntersectionStateTarget` the whole-object pass is skipped.
            if previous {
                return None;
            }
            return self.explain_object_relation(stack, source, target, depth, true);
        }
        // An intersection source failed every constituent silently; the
        // structural pass over its combined members reports. A primitive
        // constituent makes that pass read `getApparentType` of the
        // intersection (`Number & { __brand: T }`), which this arm does not
        // build, so it declines (r7-reports §78).
        if source_flags.contains(TypeFlags::INTERSECTION)
            && target_flags.contains(TypeFlags::OBJECT)
            && !target_flags.intersects(TypeFlags::INSTANTIABLE | TypeFlags::UNION)
            && !self.intersection_has_instantiable(source)
        {
            if let TypeData::Intersection { types, .. } = &self.type_of(source).data
                && types
                    .iter()
                    .any(|&part| self.type_of(part).flags.intersects(TypeFlags::PRIMITIVE))
            {
                return None;
            }
            return self.explain_object_relation(stack, source, target, depth, false);
        }
        // Two indexed accesses (`structuredTypeRelatedToWorker`'s target arm,
        // `relater.go:3542`): `S[K]` relates to `T[J]` when `S` relates to
        // `T` and then `K` to `J`, each reporting. The write-constraint
        // fallback that follows is skipped for a generic target index; any
        // other target, or a mapped or alias object the pair would first
        // normalize, declines (r7-reports §81).
        if let Some(&(source_object, source_index, _)) =
            self.deferred_indexed_access_types.get(&source)
            && let Some(&(target_object, target_index, _)) =
                self.deferred_indexed_access_types.get(&target)
        {
            for object in [source_object, target_object] {
                self.ensure_mapped_type_info(object);
            }
            if !self.indexed_access_index_is_generic(target_index)
                || [source_object, target_object].iter().any(|object| {
                    self.mapped_types.contains_key(object)
                        || self.type_reference_targets.contains_key(object)
                        || self.deferred_indexed_access_types.contains_key(object)
                })
            {
                return None;
            }
            return match self.chain_relate(stack, source_object, target_object) {
                Ternary::NotRelated => {
                    self.explain_pair(stack, source_object, target_object, depth)
                }
                Ternary::Related => match self.chain_relate(stack, source_index, target_index) {
                    Ternary::NotRelated => {
                        self.explain_pair(stack, source_index, target_index, depth)
                    }
                    _ => None,
                },
                Ternary::Unknown => None,
            };
        }
        // A template literal or string mapping source (`relater.go:3770`,
        // `:3782`) relates its base constraint, reporting; two string
        // mappings of one intrinsic relate their operands (r7-reports §101).
        if source_flags.intersects(TypeFlags::TEMPLATE_LITERAL | TypeFlags::STRING_MAPPING)
            && !target_flags
                .intersects(TypeFlags::OBJECT | TypeFlags::UNION | TypeFlags::INTERSECTION)
        {
            if let Some((source_symbol, source_operand)) =
                self.string_mapping_types.get(&source).cloned()
                && let Some((target_symbol, target_operand)) =
                    self.string_mapping_types.get(&target).cloned()
            {
                if source_symbol != target_symbol {
                    return Some(());
                }
                return match self.chain_relate(stack, source_operand, target_operand) {
                    Ternary::NotRelated => {
                        self.explain_pair(stack, source_operand, target_operand, depth)
                    }
                    _ => None,
                };
            }
            if source_flags.contains(TypeFlags::TEMPLATE_LITERAL)
                && target_flags.contains(TypeFlags::TEMPLATE_LITERAL)
            {
                return Some(());
            }
            // No constraint other than itself: nothing reports below.
            return match self.base_constraint_of_type(source) {
                Some(constraint) if constraint != source => {
                    self.explain_constraint_descent(stack, constraint, target, depth)
                }
                _ => Some(()),
            };
        }
        // A `keyof` source (`relater.go:3694`) relates `string | number |
        // symbol`, reporting, unless it is a deferred mapped index. A
        // `keyof T` target over a type with no constraint has nothing that
        // reports for a source that is not a `keyof` (r7-reports §102).
        if self.deferred_keyof_operands.contains_key(&source)
            && !target_flags.intersects(TypeFlags::UNION | TypeFlags::INTERSECTION)
        {
            let operand = self.deferred_keyof_operands[&source];
            self.ensure_mapped_type_info(operand);
            if self.mapped_types.contains_key(&operand) {
                return None;
            }
            if let Some(&target_operand) = self.deferred_keyof_operands.get(&target) {
                // `keyof S → keyof T` first relates `T → S` without reports.
                if self.chain_relate(stack, target_operand, operand) != Ternary::NotRelated {
                    return None;
                }
            }
            let keys = self.get_union_type(&[
                self.intrinsics.string,
                self.intrinsics.number,
                self.intrinsics.es_symbol,
            ]);
            return self.explain_constraint_descent(stack, keys, target, depth);
        }
        if let Some(&target_operand) = self.deferred_keyof_operands.get(&target)
            && source_flags.intersects(TypeFlags::PRIMITIVE)
            && !source_flags.intersects(TypeFlags::INSTANTIABLE | TypeFlags::UNION)
            && self.type_of(target_operand).flags.contains(TypeFlags::TYPE_PARAMETER)
            && self.constraint_of_type(target_operand) == crate::constraints::ConstraintOfType::Nil
        {
            return Some(());
        }
        // Two generic mapped types (`mappedTypeRelatedTo`, `relater.go:3972`):
        // the target's constraint against the source's, then the source's
        // template under `P := Q` against the target's, each reporting. A
        // modifier difference or an `as` clause declines.
        if self.is_generic_mapped_type(source) && self.is_generic_mapped_type(target) {
            let source_info = self.mapped_types.get(&source)?.clone();
            let target_info = self.mapped_types.get(&target)?.clone();
            if source_info.optionality != target_info.optionality
                || source_info.name_type.is_some()
                || target_info.name_type.is_some()
            {
                return None;
            }
            return match self.chain_relate(stack, target_info.constraint, source_info.constraint) {
                Ternary::NotRelated => {
                    self.explain_pair(stack, target_info.constraint, source_info.constraint, depth)
                }
                Ternary::Unknown => None,
                Ternary::Related => {
                    let map = [(source_info.parameter, target_info.parameter)];
                    let template = self.mapped_template_type(&source_info);
                    let source_template =
                        self.instantiate_type(template, &map, &[source_info.parameter], &[]);
                    if source_template == self.intrinsics.error {
                        return None;
                    }
                    let target_template = self.mapped_template_type(&target_info);
                    match self.chain_relate(stack, source_template, target_template) {
                        Ternary::NotRelated => {
                            self.explain_pair(stack, source_template, target_template, depth)
                        }
                        _ => None,
                    }
                }
            };
        }
        // An indexed-access source (the same type-variable case,
        // `relater.go:3665`) relates `getConstraintOfType`, reporting when
        // that is not `unknown` (a nil constraint reports nothing). Against
        // an indexed-access target the object/index arm owns the relation; a
        // mapped object (`isMappedTypeGenericIndexedAccess` explores a second
        // constraint) or an undecided constraint declines (r7-reports §78).
        if let Some(&(object, _, _)) = self.deferred_indexed_access_types.get(&source)
            && !target_flags.intersects(TypeFlags::INDEXED_ACCESS | TypeFlags::INTERSECTION)
        {
            self.ensure_mapped_type_info(object);
            if self.mapped_types.contains_key(&object)
                || self.type_reference_targets.contains_key(&object)
            {
                return None;
            }
            return match self.constraint_of_type(source) {
                crate::constraints::ConstraintOfType::Nil => Some(()),
                crate::constraints::ConstraintOfType::Constraint(constraint)
                    if constraint != source && constraint != self.intrinsics.unknown =>
                {
                    let constraint = self.chain_simplified_indexed_access(constraint);
                    self.explain_constraint_descent(stack, constraint, target, depth)
                }
                crate::constraints::ConstraintOfType::Constraint(constraint)
                    if constraint == self.intrinsics.unknown =>
                {
                    Some(())
                }
                _ => None,
            };
        }
        // A type-parameter source (`structuredTypeRelatedToWorker`,
        // `relater.go:3665`) relates its constraint, reporting only when that
        // constraint is not `unknown` and the target is not a type parameter
        // too. A polymorphic `this` relates `getTypeWithThisArgument` of its
        // constraint, which this arm does not build.
        // A union target first fails `typeRelatedToSomeType`, whose
        // best-match elaboration has no candidate for a type-parameter source
        // (every `getBestMatchingType` arm reads an object source), so the
        // constraint is still what reports (r7-reports §71).
        if source_flags.contains(TypeFlags::TYPE_PARAMETER)
            && !target_flags.intersects(TypeFlags::INTERSECTION)
            && !self.this_types.values().any(|&this| this == source)
            && !self.this_type_nodes.values().any(|&this| this == source)
        {
            let constraint = self.type_parameter_constraint(source);
            return match constraint {
                None => Some(()),
                Some(constraint)
                    if constraint == self.intrinsics.unknown
                        || target_flags.contains(TypeFlags::TYPE_PARAMETER) =>
                {
                    Some(())
                }
                Some(constraint) => {
                    self.explain_constraint_descent(stack, constraint, target, depth)
                }
            };
        }
        // A type-parameter target (`structuredTypeRelatedToWorker`'s target
        // arm, `relater.go:3462`) relates only a mapped or generic source;
        // anything else fails with nothing below its relation error, which
        // names the constraint (r7-reports §46).
        if target_flags.contains(TypeFlags::TYPE_PARAMETER)
            && source_flags.intersects(TypeFlags::OBJECT | TypeFlags::PRIMITIVE)
            && !source_flags.intersects(TypeFlags::INSTANTIABLE | TypeFlags::INTERSECTION)
            && !self.mapped_types.contains_key(&source)
        {
            return Some(());
        }
        if source_flags.intersects(TypeFlags::INSTANTIABLE | TypeFlags::INTERSECTION)
            || target_flags
                .intersects(TypeFlags::INSTANTIABLE | TypeFlags::INTERSECTION | TypeFlags::UNION)
        {
            return None;
        }
        // An object source against a primitive: no structural arm applies,
        // so nothing reports below the relation error (`reportErrorResults`
        // adds the wrapper message).
        if source_flags.contains(TypeFlags::OBJECT)
            && target_flags.intersects(TypeFlags::PRIMITIVE)
            && !target_flags.intersects(TypeFlags::INSTANTIABLE)
        {
            return Some(());
        }
        if !source_flags.contains(TypeFlags::OBJECT) || !target_flags.contains(TypeFlags::OBJECT) {
            return None;
        }
        // Array references: `typeArgumentsRelatedTo` over the covariant
        // element (`Array<T>`'s measured variance).
        if let (Some(source_readonly), Some(target_readonly)) =
            (self.array_reference_readonly(source), self.array_reference_readonly(target))
        {
            if source_readonly && !target_readonly {
                return Some(());
            }
            let source_element = self.type_reference_targets.get(&source)?.1.first().copied()?;
            let target_element = self.type_reference_targets.get(&target)?.1.first().copied()?;
            return match self.chain_relate(stack, source_element, target_element) {
                Ternary::NotRelated => {
                    self.explain_pair(stack, source_element, target_element, depth)
                }
                _ => None,
            };
        }
        // An array target against tuples (`relater.go:3780`): a readonly
        // array accepts any array or tuple, a mutable one a mutable tuple,
        // and the number-index types relate with errors.
        if let Some(target_readonly) = self.array_reference_readonly(target)
            && let Some((_, source_readonly)) = self.tuple_element_lists.get(&source).cloned()
            && (target_readonly || !source_readonly)
            && !self.tuple_optional_masks.get(&source).is_some_and(|mask| mask.contains(&true))
        {
            let number = self.intrinsics.number;
            let source_index = self.array_or_tuple_element_access(source, number, false)?;
            let target_element = self.type_reference_targets.get(&target)?.1.first().copied()?;
            return match self.chain_relate(stack, source_index, target_element) {
                Ternary::NotRelated => {
                    self.explain_pair(stack, source_index, target_element, depth)
                }
                _ => None,
            };
        }
        // Array and tuple pairs have arms of their own (tuple elements,
        // `tryElaborateArrayLikeErrors`), not retraced here. An array or
        // tuple against a plain object, or the reverse, is structural.
        let array_like = |checker: &Self, ty: TypeId| {
            checker.array_reference_readonly(ty).is_some()
                || checker.tuple_element_lists.contains_key(&ty)
                || checker.variadic_tuple_elements.contains_key(&ty)
        };
        if array_like(self, source) && array_like(self, target) {
            return self.explain_fixed_tuple_target(stack, source, target, depth);
        }
        if let Some(explained) = self.explain_reference_variances(stack, source, target, depth) {
            return explained;
        }
        self.explain_object_relation(stack, source, target, depth, false)
    }

    /// `propertiesRelatedTo`'s tuple-target arm (`relater.go:4103`) for tuples
    /// of required, optional and rest elements, and an array source (all
    /// rest): the arity messages in native's order, then each source position
    /// against its target position (`Source provides no match for required
    /// element at position i in target.`, then the element relation, with
    /// `Type at position(s) …` when either side has more than one element).
    /// A readonly source against a mutable target, a variadic element, and an
    /// element failure where either side is optional (whose types native
    /// compares with `removeMissingType`) decline (r7-reports §52, §87, §89).
    fn explain_fixed_tuple_target(
        &mut self,
        stack: &mut ChainStack,
        source: TypeId,
        target: TypeId,
        depth: usize,
    ) -> Option<()> {
        use crate::relater::Ternary;
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum Kind {
            Required,
            Optional,
            Rest,
            Variadic,
        }
        let shape = |checker: &Self, ty: TypeId| -> Option<(Vec<(TypeId, Kind)>, bool)> {
            if let Some((elements, readonly)) = checker.variadic_tuple_elements.get(&ty) {
                let mut shape = Vec::with_capacity(elements.len());
                for element in elements {
                    if element.spread {
                        // A rest element over an array; a variadic `...T`
                        // over a type variable; anything else declines.
                        if checker.array_reference_readonly(element.r#type).is_some() {
                            let item =
                                checker.type_reference_targets.get(&element.r#type)?.1.first()?;
                            shape.push((*item, Kind::Rest));
                        } else if checker
                            .type_of(element.r#type)
                            .flags
                            .intersects(TypeFlags::TYPE_PARAMETER | TypeFlags::INDEXED_ACCESS)
                        {
                            shape.push((element.r#type, Kind::Variadic));
                        } else {
                            return None;
                        }
                    } else if element.optional {
                        shape.push((element.r#type, Kind::Optional));
                    } else {
                        shape.push((element.r#type, Kind::Required));
                    }
                }
                return Some((shape, *readonly));
            }
            if checker.tuple_rest_tails.contains_key(&ty) {
                return None;
            }
            let (types, readonly) = checker.tuple_element_lists.get(&ty)?;
            let mask = checker.tuple_optional_masks.get(&ty);
            let kind = |index: usize| {
                if mask.and_then(|mask| mask.get(index)).copied().unwrap_or(false) {
                    Kind::Optional
                } else {
                    Kind::Required
                }
            };
            Some((
                types.iter().enumerate().map(|(index, &ty)| (ty, kind(index))).collect(),
                *readonly,
            ))
        };
        let (target_elements, target_readonly) = shape(self, target)?;
        let (source_elements, source_readonly, source_is_array) =
            if let Some(readonly) = self.array_reference_readonly(source) {
                let element = *self.type_reference_targets.get(&source)?.1.first()?;
                (vec![(element, Kind::Rest)], readonly, true)
            } else {
                let (elements, readonly) = shape(self, source)?;
                (elements, readonly, false)
            };
        // The tuple arm fails a readonly source against a mutable target
        // with no report of its own; `reportErrorResults` adds TS4104.
        if source_readonly && !target_readonly {
            return Some(());
        }
        let push_count = |stack: &mut ChainStack,
                          message: &'static tsr_diagnostics::Message,
                          args: Vec<usize>| {
            stack.push(ChainEntry {
                message,
                args: args.into_iter().map(|n| n.to_string()).collect(),
            });
            Some(())
        };
        // `minLength` counts required and variadic elements.
        let required = |elements: &[(TypeId, Kind)]| {
            elements
                .iter()
                .filter(|element| matches!(element.1, Kind::Required | Kind::Variadic))
                .count()
        };
        let source_arity = source_elements.len();
        let target_arity = target_elements.len();
        let source_rest = source_elements.iter().any(|element| element.1 == Kind::Rest);
        let target_has_rest = target_elements.iter().any(|element| element.1 == Kind::Rest);
        let target_has_variable =
            target_elements.iter().any(|element| matches!(element.1, Kind::Rest | Kind::Variadic));
        let source_min = if source_is_array { 0 } else { required(&source_elements) };
        let target_min = required(&target_elements);
        if !source_rest && source_arity < target_min {
            return push_count(
                stack,
                &messages::SOURCE_HAS_0_ELEMENT_S_BUT_TARGET_REQUIRES_1,
                vec![source_arity, target_min],
            );
        }
        if !target_has_variable && target_arity < source_min {
            return push_count(
                stack,
                &messages::SOURCE_HAS_0_ELEMENT_S_BUT_TARGET_ALLOWS_ONLY_1,
                vec![source_min, target_arity],
            );
        }
        if !target_has_variable && (source_rest || target_arity < source_arity) {
            return if source_min < target_min {
                push_count(
                    stack,
                    &messages::TARGET_REQUIRES_0_ELEMENT_S_BUT_SOURCE_MAY_HAVE_FEWER,
                    vec![target_min],
                )
            } else {
                push_count(
                    stack,
                    &messages::TARGET_ALLOWS_ONLY_0_ELEMENT_S_BUT_SOURCE_MAY_HAVE_MORE,
                    vec![target_arity],
                )
            };
        }
        let target_start =
            target_elements.iter().take_while(|element| element.1 != Kind::Rest).count();
        let target_end =
            target_elements.iter().rev().take_while(|element| element.1 != Kind::Rest).count();
        for (source_position, &(from, source_kind)) in source_elements.iter().enumerate() {
            let from_end = source_arity - 1 - source_position;
            let target_position = if target_has_rest && source_position >= target_start {
                target_arity - 1 - from_end.min(target_end)
            } else {
                if source_position >= target_arity {
                    return push_count(
                        stack,
                        &messages::TARGET_ALLOWS_ONLY_0_ELEMENT_S_BUT_SOURCE_MAY_HAVE_MORE,
                        vec![target_arity],
                    );
                }
                source_position
            };
            let (to, target_kind) = target_elements[target_position];
            if target_kind == Kind::Variadic && source_kind != Kind::Variadic {
                return push_count(
                    stack,
                    &messages::SOURCE_PROVIDES_NO_MATCH_FOR_VARIADIC_ELEMENT_AT_POSITION_0_IN_TARGET,
                    vec![target_position],
                );
            }
            if source_kind == Kind::Variadic && !matches!(target_kind, Kind::Rest | Kind::Variadic)
            {
                return push_count(
                    stack,
                    &messages::VARIADIC_ELEMENT_AT_POSITION_0_IN_SOURCE_DOES_NOT_MATCH_ELEMENT_AT_POSITION_1_IN_TARGET,
                    vec![source_position, target_position],
                );
            }
            if target_kind == Kind::Required && source_kind != Kind::Required {
                return push_count(
                    stack,
                    &messages::SOURCE_PROVIDES_NO_MATCH_FOR_REQUIRED_ELEMENT_AT_POSITION_0_IN_TARGET,
                    vec![target_position],
                );
            }
            // A variadic source against a rest target relates against the
            // rest's array type, which this walk does not build.
            if source_kind == Kind::Variadic && target_kind == Kind::Rest {
                return None;
            }
            // `removeMissingType`: under exactOptionalPropertyTypes an
            // optional target (and a source optional on both sides) drops its
            // missing type; otherwise the optional element keeps the
            // `undefined` its optionality added, which a list without one
            // here cannot reproduce.
            let optional_type = |checker: &mut Self, ty: TypeId, strip: bool| {
                if checker.exact_optional_property_types {
                    Some(if strip { checker.remove_missing_type(ty) } else { ty })
                } else if !checker.strict_null_checks
                    || ty == checker.intrinsics.undefined
                    || matches!(&checker.type_of(ty).data, TypeData::Union { types, .. }
                        if types.contains(&checker.intrinsics.undefined))
                {
                    Some(ty)
                } else {
                    None
                }
            };
            let to =
                if target_kind == Kind::Optional { optional_type(self, to, true)? } else { to };
            let from = if source_kind == Kind::Optional {
                optional_type(self, from, target_kind == Kind::Optional)?
            } else {
                from
            };
            match self.chain_relate(stack, from, to) {
                Ternary::Related => {}
                Ternary::Unknown => return None,
                Ternary::NotRelated => {
                    self.explain_pair(stack, from, to, depth)?;
                    if target_arity > 1 || source_arity > 1 {
                        if target_has_rest
                            && source_position >= target_start
                            && from_end >= target_end
                            && target_start != source_arity - target_end - 1
                        {
                            push_count(
                                stack,
                                &messages::TYPE_AT_POSITIONS_0_THROUGH_1_IN_SOURCE_IS_NOT_COMPATIBLE_WITH_TYPE_AT_POSITION_2_IN_TARGET,
                                vec![target_start, source_arity - target_end - 1, target_position],
                            );
                        } else {
                            push_count(
                                stack,
                                &messages::TYPE_AT_POSITION_0_IN_SOURCE_IS_NOT_COMPATIBLE_WITH_TYPE_AT_POSITION_1_IN_TARGET,
                                vec![source_position, target_position],
                            );
                        }
                    }
                    return Some(());
                }
            }
        }
        None
    }

    /// `getDeclaringClass`: the class whose member declaration `property`
    /// has as its value declaration.
    fn chain_declaring_class(&self, property: SymbolId) -> Option<SymbolId> {
        let declaration = self.binder.symbols().get(property).value_declaration?;
        let class = self.nodes.parent(declaration)?;
        if !matches!(
            self.nodes.kind(class),
            SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
        ) {
            return None;
        }
        self.binder.symbol_of(class).map(|symbol| self.binder.merged_symbol(symbol))
    }

    /// `hasBaseType` over class symbols: `class` is `base` or extends it,
    /// directly or through its bases. `None` where a base list is unreadable.
    fn chain_class_derives_from(
        &mut self,
        class: SymbolId,
        base: SymbolId,
        depth: usize,
    ) -> Option<bool> {
        if class == base {
            return Some(true);
        }
        if depth > 32 {
            return None;
        }
        for parent in self.base_symbols_of_ex(class, false)? {
            let parent = self.binder.merged_symbol(parent);
            if self.chain_class_derives_from(parent, base, depth + 1)? {
                return Some(true);
            }
        }
        Some(false)
    }

    /// `symbolToString` of `owner`'s property `name`: a name written as a
    /// string literal keeps its quotes as written (`'ry'`, `"e"`); any
    /// other is [`chain_property_display`] (r7-reports §55).
    fn chain_property_symbol_display(&mut self, owner: TypeId, name: &str) -> String {
        let written = self.get_property_of_type(owner, name).and_then(|symbol| {
            let declaration = *self.binder.symbols().get(symbol).declarations.first()?;
            let written = match self.node_map.get(declaration)? {
                Node::PropertyAssignment(node) => node.name,
                Node::PropertySignatureDeclaration(node) => node.name,
                Node::PropertyDeclaration(node) => node.name,
                Node::MethodSignatureDeclaration(node) => node.name,
                Node::MethodDeclaration(node) => node.name,
                _ => return None,
            };
            match written {
                tsr_ast::PropertyName::StringLiteral(literal) => {
                    let quote = if literal.token_flags.contains(tsr_ast::TokenFlags::SINGLE_QUOTE) {
                        '\''
                    } else {
                        '"'
                    };
                    Some(format!("{quote}{}{quote}", literal.text))
                }
                _ => None,
            }
        });
        written.unwrap_or_else(|| chain_property_display(name))
    }

    /// `compareTypePredicateRelatedTo` (`relater.go:1662`): kinds (this or
    /// parameter, asserting or not) must agree, a parameter predicate must
    /// name the same position, and the predicate types must relate; each
    /// failure ends with `Type predicate 'P' is not assignable to 'Q'.`
    fn explain_type_predicates(
        &mut self,
        stack: &mut ChainStack,
        source: &crate::signatures::Signature,
        target: &crate::signatures::Signature,
        source_predicate: &crate::signatures::TypePredicate,
        target_predicate: &crate::signatures::TypePredicate,
        depth: usize,
    ) -> Option<()> {
        use crate::relater::Ternary;
        let source_text = self.type_predicate_to_string(source_predicate);
        let target_text = self.type_predicate_to_string(target_predicate);
        let not_assignable = ChainEntry {
            message: &messages::TYPE_PREDICATE_0_IS_NOT_ASSIGNABLE_TO_1,
            args: vec![source_text, target_text],
        };
        let kind = |predicate: &crate::signatures::TypePredicate| {
            (predicate.asserts, predicate.parameter_name.is_some())
        };
        if kind(source_predicate) != kind(target_predicate) {
            stack.push(ChainEntry {
                message: &messages::A_THIS_BASED_TYPE_GUARD_IS_NOT_COMPATIBLE_WITH_A_PARAMETER_BASED_TYPE_GUARD,
                args: Vec::new(),
            });
            stack.push(not_assignable);
            return Some(());
        }
        if let (Some(source_name), Some(target_name)) =
            (&source_predicate.parameter_name, &target_predicate.parameter_name)
        {
            let position = |signature: &crate::signatures::Signature, name: &str| {
                signature.parameters.iter().position(|parameter| parameter.name == name)
            };
            let (source_index, target_index) =
                (position(source, source_name)?, position(target, target_name)?);
            if source_index != target_index {
                stack.push(ChainEntry {
                    message: &messages::PARAMETER_0_IS_NOT_IN_THE_SAME_POSITION_AS_PARAMETER_1,
                    args: vec![source_name.clone(), target_name.clone()],
                });
                stack.push(not_assignable);
                return Some(());
            }
        }
        match (source_predicate.r#type, target_predicate.r#type) {
            (Some(from), Some(to)) if from == to => None,
            (Some(from), Some(to)) => match self.chain_relate(stack, from, to) {
                Ternary::Related | Ternary::Unknown => None,
                Ternary::NotRelated => {
                    self.explain_pair(stack, from, to, depth)?;
                    stack.push(not_assignable);
                    Some(())
                }
            },
            (None, None) => None,
            _ => {
                stack.push(not_assignable);
                Some(())
            }
        }
    }

    /// `getParameterNameAtPosition` (`relater.go:1929`): a non-rest
    /// parameter's own name, else the rest parameter's. A tuple rest names
    /// its element by label (`getTupleElementLabel`), which declines here.
    fn parameter_name_at_position(
        &mut self,
        signature: &crate::signatures::Signature,
        index: usize,
    ) -> Option<String> {
        let has_rest = signature.parameters.last().is_some_and(|parameter| parameter.rest);
        let fixed = signature.parameters.len() - usize::from(has_rest);
        // A binding-pattern parameter's symbol is named `__<index>` by the
        // binder (its position in the written list, `this` included); the
        // port's parameter keeps the pattern's printed text.
        let symbol_name = |position: usize, name: &str| {
            if name.starts_with('{') || name.starts_with('[') {
                format!("__{}", position + usize::from(signature.this_parameter.is_some()))
            } else {
                name.to_owned()
            }
        };
        if index < fixed {
            return Some(symbol_name(index, &signature.parameters[index].name));
        }
        let rest = signature.parameters.get(fixed)?;
        let rest_type = self.parameter_type(rest);
        if self.tuple_element_lists.contains_key(&rest_type)
            || self.variadic_tuple_elements.contains_key(&rest_type)
        {
            return None;
        }
        Some(symbol_name(fixed, &rest.name))
    }

    /// `relateVariances` for two references to one class or interface
    /// (`relater.go:3266`, `typeArgumentsRelatedTo` at `:3903`): the first
    /// type argument that fails under its variance is the explanation.
    /// `None` where the structural comparison decides instead: a variance
    /// with `Unmeasurable`/`Unreliable` flags, a covariant `void` target
    /// argument, or an invariant parameter (whose errors upstream resets to
    /// show the structural reason). `Some(None)` declines.
    #[allow(clippy::option_option)]
    fn explain_reference_variances(
        &mut self,
        stack: &mut ChainStack,
        source: TypeId,
        target: TypeId,
        depth: usize,
    ) -> Option<Option<()>> {
        use crate::relater::Ternary;
        use crate::variances::Variance;
        let (source_symbol, source_arguments) = self.type_reference_targets.get(&source)?.clone();
        let (target_symbol, target_arguments) = self.type_reference_targets.get(&target)?.clone();
        let symbol = self.binder.merged_symbol(source_symbol);
        // An alias relates its own arguments under its own variances
        // (`relater.go:3392`) unless it is written as a reference, which
        // relates as the reference it names; that case declines.
        let alias_of_reference = self
            .binder
            .symbols()
            .get(symbol)
            .flags
            .contains(SymbolFlags::TYPE_ALIAS)
            && self.binder.symbols().get(symbol).declarations.iter().any(|&declaration| {
                !matches!(
                    self.node_map.get(declaration),
                    Some(Node::TypeAliasDeclaration(alias))
                        if !matches!(alias.r#type, Some(tsr_ast::TypeNode::TypeReferenceNode(_)))
                )
            });
        if symbol != self.binder.merged_symbol(target_symbol)
            || source_arguments.len() != target_arguments.len()
            || alias_of_reference
        {
            return None;
        }
        let Some(variances) = self.inference_variances(symbol) else { return Some(None) };
        if variances.len() != source_arguments.len() {
            return Some(None);
        }
        let reliability = self.variance_reliability(symbol, variances.len());
        // A covariant `void` argument (`hasCovariantVoidArgument`) allows the
        // structural fallback without restoring the error state: the type
        // argument's report stays, and the structural pass then runs with
        // `reportStructuralErrors` off because the chain has changed
        // (`relater.go:3291`), so the argument's report is the chain
        // (r7-reports §90).
        if reliability.iter().any(|flags| !flags.is_empty())
            || variances.contains(&Variance::Invariant)
        {
            return None;
        }
        for ((&s, &t), variance) in source_arguments.iter().zip(&target_arguments).zip(&variances) {
            let (from, to) = match variance {
                Variance::Independent | Variance::Invariant => continue,
                Variance::Covariant => (s, t),
                Variance::Contravariant => (t, s),
                Variance::Bivariant => match self.chain_relate(stack, t, s) {
                    Ternary::Related => continue,
                    Ternary::Unknown => return Some(None),
                    Ternary::NotRelated => (s, t),
                },
            };
            match self.chain_relate(stack, from, to) {
                Ternary::Related => {}
                Ternary::Unknown => return Some(None),
                Ternary::NotRelated => return Some(self.explain_pair(stack, from, to, depth)),
            }
        }
        Some(None)
    }

    /// `propertiesRelatedTo`, then `signaturesRelatedTo(Call)`, for two plain
    /// object types.
    fn explain_object_relation(
        &mut self,
        stack: &mut ChainStack,
        source: TypeId,
        target: TypeId,
        depth: usize,
        properties_only: bool,
    ) -> Option<()> {
        use crate::relater::Ternary;
        use crate::signatures::SignatureKind;
        // `propertiesRelatedTo` walks the target's properties and looks each
        // up on the source by name; the source's own list is never read.
        // A method's or function's own type (its overload set, with no
        // members of its own) has no answer from the property enumeration;
        // its property list is empty (`resolveAnonymousTypeMembers` of a
        // function symbol without exports).
        let names = match self.get_property_names_of_type(target) {
            Some(names) => names,
            // A tuple target's members: its synthesized elements and
            // `length`, then the `Array` members it inherits.
            None if self.tuple_element_lists.contains_key(&target)
                || self.variadic_tuple_elements.contains_key(&target) =>
            {
                self.tuple_target_properties(target)?.into_iter().map(|(name, _)| name).collect()
            }
            None if self
                .completed_callable_symbol(target)
                .is_some_and(|symbol| self.binder.symbols().get(symbol).exports.is_empty()) =>
            {
                Vec::new()
            }
            None => return None,
        };
        // A tuple's own members (`0`, `1`, …, `length`) are synthesized
        // without declarations, so `getNamedMembers`' `compareSymbols` puts
        // them after the declared `Array` members they extend or override
        // (r7-reports §95).
        let names = if self.tuple_element_lists.contains_key(&target)
            || self.variadic_tuple_elements.contains_key(&target)
        {
            let synthesized =
                |name: &String| name == "length" || name.bytes().all(|byte| byte.is_ascii_digit());
            let (own, inherited): (Vec<String>, Vec<String>) =
                names.into_iter().partition(|name| synthesized(name));
            inherited.into_iter().chain(own).collect()
        } else {
            names
        };
        // `getUnmatchedProperty` first.
        for name in &names {
            if self.get_type_of_property_of_type(source, name).is_none()
                && !self.chain_property_is_optional(target, name)?
            {
                // Under `IntersectionStateTarget` a fresh literal meets no
                // excess-property check (`isPerformingExcessPropertyChecks`),
                // so its regular type answers (r7-reports §92).
                let unmatched_source = if stack.intersection_target {
                    self.get_regular_type_of_object_literal(source)
                } else {
                    source
                };
                let properties = match self.unmatched_property_report(unmatched_source, target) {
                    Some(properties) => properties,
                    // `getUnmatchedProperties` over an intersection source of
                    // plain objects: the target's required members its
                    // combined members lack, in target order (§71).
                    // The same for the global `Object` source, which
                    // `unmatched_property_report` leaves to its caller's head
                    // because `reportErrorResults` wraps the message (§88).
                    None if self.type_of(source).flags.contains(TypeFlags::INTERSECTION)
                        && !self.intersection_has_instantiable(source)
                        || self.is_global_object_type(source) =>
                    {
                        let mut missing = Vec::new();
                        for candidate in &names {
                            if self.get_type_of_property_of_type(source, candidate).is_none()
                                && !self.chain_property_is_optional(target, candidate)?
                            {
                                missing.push(candidate.clone());
                            }
                        }
                        missing
                    }
                    None => return None,
                };
                let diagnostic = self.missing_properties_diagnostic(
                    tsr_core::Span::default(),
                    source,
                    target,
                    &properties,
                );
                stack.related.extend(diagnostic.related_information().iter().cloned());
                stack.push(ChainEntry { message: diagnostic.message, args: diagnostic.args });
                return Some(());
            }
        }
        for name in &names {
            // A class's `prototype` is never compared (`relater.go:4256`);
            // the construct signatures relate the instances.
            // The port mints a class's `prototype` without a symbol.
            let class_prototype = name == "prototype"
                && matches!(self.type_of(target).data, TypeData::Anonymous { symbol, .. }
                    if self.binder.symbols().get(self.binder.merged_symbol(symbol)).flags.contains(SymbolFlags::CLASS));
            if class_prototype
                || self.get_property_of_type(target, name).is_some_and(|symbol| {
                    self.binder.symbols().get(symbol).flags.contains(SymbolFlags::PROTOTYPE)
                })
            {
                continue;
            }
            let Some(source_member) = self.get_type_of_property_of_type(source, name) else {
                continue;
            };
            let target_member = self.get_type_of_property_of_type(target, name)?;
            // `getNonMissingTypeOfSymbol` on both sides (`relater.go:4334`):
            // under exactOptionalPropertyTypes an optional member's missing
            // type is removed, as the relater's own walk does.
            let (source_member, target_member) = if self.exact_optional_property_types {
                (self.remove_missing_type(source_member), self.remove_missing_type(target_member))
            } else {
                (source_member, target_member)
            };
            match self.chain_property_privacy(stack, source, target, name) {
                Some(true) => return Some(()),
                Some(false) => {}
                None => return None,
            }
            match self.chain_relate(stack, source_member, target_member) {
                Ternary::Related => {}
                Ternary::Unknown => return None,
                Ternary::NotRelated => {
                    self.explain_pair(stack, source_member, target_member, depth)?;
                    let display = self.chain_property_symbol_display(target, name);
                    chain_report(
                        stack,
                        &messages::TYPES_OF_PROPERTY_0_ARE_INCOMPATIBLE,
                        vec![display],
                    );
                    return Some(());
                }
            }
            // `propertyRelatedTo`'s optionality arm (`relater.go:4309`): a
            // related optional source member against a required class
            // member (r7-reports §51).
            let target_class_member =
                self.get_property_of_type(target, name).is_some_and(|symbol| {
                    self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::CLASS_MEMBER)
                });
            // An unreadable optionality (an apparent member the source does
            // not own) is not the optional arm's.
            if target_class_member
                && self.chain_property_is_optional(source, name) == Some(true)
                && self.chain_property_is_optional(target, name) == Some(false)
            {
                let source_text = self.type_to_string(source);
                let target_text = self.type_to_string(target);
                stack.push(ChainEntry {
                    message: &messages::PROPERTY_0_IS_OPTIONAL_IN_TYPE_1_BUT_REQUIRED_IN_TYPE_2,
                    args: vec![
                        self.chain_property_symbol_display(target, name),
                        source_text,
                        target_text,
                    ],
                });
                return Some(());
            }
        }
        if properties_only {
            return None;
        }
        // `signaturesRelatedTo(Call)`, then `(Construct)`. A resolved
        // (non-generic) mapped type has neither kind:
        // `resolveMappedTypeMembers` sets no signatures (r7-reports §82).
        for kind in [SignatureKind::Call, SignatureKind::Construct] {
            let target_signatures = self.chain_signatures_of_kind(target, kind)?;
            if target_signatures.is_empty() {
                continue;
            }
            // A kind whose signatures relate passes on to the next
            // (`structuredTypeRelatedToWorker` relates call, then construct
            // signatures; r7-reports §96).
            match self.chain_signature_kind_related(source, target, kind) {
                crate::relater::Ternary::Related => continue,
                crate::relater::Ternary::Unknown => return None,
                crate::relater::Ternary::NotRelated => {}
            }
            let source_signatures = self.chain_signatures_of_kind(source, kind)?;
            // An abstract constructor type is not assignable to a
            // non-abstract one (`relater.go:4455`).
            if kind == SignatureKind::Construct
                && let (Some(first_source), Some(first_target)) =
                    (source_signatures.first(), target_signatures.first())
                && first_source.kind == SignatureKind::AbstractConstruct
                && first_target.kind != SignatureKind::AbstractConstruct
            {
                stack.push(ChainEntry {
                    message: &messages::CANNOT_ASSIGN_AN_ABSTRACT_CONSTRUCTOR_TYPE_TO_A_NON_ABSTRACT_CONSTRUCTOR_TYPE,
                    args: Vec::new(),
                });
                return Some(());
            }
            // `constructorVisibilitiesAreCompatible` (`relater.go:4520`), with
            // its report (r7-reports §88).
            if kind == SignatureKind::Construct
                && let (Some(first_source), Some(first_target)) =
                    (source_signatures.first(), target_signatures.first())
                && let Some((source_visibility, target_visibility)) =
                    self.constructor_visibility_mismatch(first_source, first_target)
            {
                let text = |visibility: Option<SyntaxKind>| match visibility {
                    Some(SyntaxKind::PrivateKeyword) => "private",
                    Some(SyntaxKind::ProtectedKeyword) => "protected",
                    _ => "public",
                };
                stack.push(ChainEntry {
                    message: &messages::CANNOT_ASSIGN_A_0_CONSTRUCTOR_TYPE_TO_A_1_CONSTRUCTOR_TYPE,
                    args: vec![
                        text(source_visibility).to_string(),
                        text(target_visibility).to_string(),
                    ],
                });
                return Some(());
            }
            return match (source_signatures.as_slice(), target_signatures.as_slice()) {
                ([], [first, ..]) => {
                    let source_text = self.type_to_string(source);
                    let signature_text = signature_colon_form(&self.signature_to_string(first));
                    stack.push(ChainEntry {
                        message: &messages::TYPE_0_PROVIDES_NO_MATCH_FOR_THE_SIGNATURE_1,
                        args: vec![source_text, signature_text],
                    });
                    Some(())
                }
                ([source_signature], [target_signature]) => {
                    let (source_signature, target_signature) =
                        (source_signature.clone(), target_signature.clone());
                    self.explain_signature(
                        stack,
                        &source_signature,
                        &target_signature,
                        depth,
                        (false, false),
                    )
                }
                (sources, targets) => {
                    let (sources, targets) = (sources.to_vec(), targets.to_vec());
                    self.explain_signature_sets(stack, source, target, &sources, &targets, depth)
                }
            };
        }
        self.explain_index_signatures(stack, source, target, depth)
    }

    /// The signatures of one kind for the retrace: an instance of a
    /// non-generic mapped type has none (`resolveMappedTypeMembers`), which
    /// the port's signature reader leaves undecided.
    fn chain_signatures_of_kind(
        &mut self,
        ty: TypeId,
        kind: crate::signatures::SignatureKind,
    ) -> Option<Vec<crate::signatures::Signature>> {
        if let Some(signatures) = self.signatures_of_type_kind(ty, kind) {
            return Some(signatures);
        }
        self.ensure_mapped_type_info(ty);
        (self.mapped_types.contains_key(&ty) && !self.is_generic_mapped_type(ty)).then(Vec::new)
    }

    /// `indexSignaturesRelatedTo` (`relater.go:4578`): each target index
    /// signature in order. A `string` index of `any` beside a string key
    /// relates outright. An applicable source index relates its value
    /// (`indexInfoRelatedTo`: `'K' index signatures are incompatible.`).
    /// Otherwise an inferable-index source relates each applicable member
    /// (`membersRelatedToIndexInfo`: `Property 'p' is incompatible with
    /// index signature.`), and any other source misses the signature
    /// (TS2329). A generic mapped source, a key other than `string` or
    /// `number`, and an optional member decline (r7-reports §46).
    fn explain_index_signatures(
        &mut self,
        stack: &mut ChainStack,
        source: TypeId,
        target: TypeId,
        depth: usize,
    ) -> Option<()> {
        use crate::relater::Ternary;
        let target_infos = self.get_index_infos_of_type(target)?;
        let string = self.intrinsics.string;
        let number = self.intrinsics.number;
        let target_has_string = target_infos.iter().any(|info| info.key == string);
        for info in &target_infos {
            if target_has_string && self.type_of(info.value).flags.intersects(TypeFlags::ANY) {
                continue;
            }
            if target_has_string && self.is_generic_mapped_type(source) {
                return None;
            }
            if let Some(source_info) = self.get_applicable_index_info(source, info.key) {
                match self.chain_relate(stack, source_info.value, info.value) {
                    Ternary::Related => continue,
                    Ternary::Unknown => return None,
                    Ternary::NotRelated => {
                        return self.explain_index_info(stack, &source_info, info, depth);
                    }
                }
            }
            if self.chain_source_index_not_inferable(source) {
                let key_text = self.type_to_string(info.key);
                let source_text = self.type_to_string(source);
                stack.push(ChainEntry {
                    message: &messages::INDEX_SIGNATURE_FOR_TYPE_0_IS_MISSING_IN_TYPE_1,
                    args: vec![key_text, source_text],
                });
                return Some(());
            }
            if info.key != string && info.key != number {
                return None;
            }
            for name in self.get_property_names_of_type(source)? {
                let numeric = crate::index_signatures::is_numeric_literal_name(&name);
                if info.key == number && !numeric {
                    continue;
                }
                if name.starts_with("__@") || name.starts_with("__#") {
                    return None;
                }
                // A member without a readable optionality is a synthesized
                // required one (an import-attributes literal). An optional
                // member against a `string` index relates without its
                // `undefined` (`getTypeWithFacts(NEUndefined)`, outside
                // exactOptionalPropertyTypes); against a `number` index it
                // keeps its optionality's `undefined`, and a member type
                // without one where strict null checks would add it declines
                // (r7-reports §91).
                let optional = self.chain_property_is_optional(source, &name) == Some(true);
                if optional && self.exact_optional_property_types {
                    return None;
                }
                let member = self.get_type_of_property_of_type(source, &name)?;
                let member = if !optional || member == self.intrinsics.undefined {
                    member
                } else if info.key == string {
                    self.get_type_with_facts(member, crate::flow::TypeFacts::NE_UNDEFINED)
                } else if self.strict_null_checks
                    && !matches!(&self.type_of(member).data, TypeData::Union { types, .. }
                        if types.contains(&self.intrinsics.undefined))
                {
                    return None;
                } else {
                    member
                };
                match self.chain_relate(stack, member, info.value) {
                    Ternary::Related => {}
                    Ternary::Unknown => return None,
                    Ternary::NotRelated => {
                        self.explain_pair(stack, member, info.value, depth)?;
                        stack.push(ChainEntry {
                            message: &messages::PROPERTY_0_IS_INCOMPATIBLE_WITH_INDEX_SIGNATURE,
                            args: vec![self.chain_property_symbol_display(source, &name)],
                        });
                        return Some(());
                    }
                }
            }
            let source_infos = self.get_index_infos_of_type(source)?;
            for source_info in source_infos {
                if !self.is_applicable_index_type(source_info.key, info.key) {
                    continue;
                }
                match self.chain_relate(stack, source_info.value, info.value) {
                    Ternary::Related => {}
                    Ternary::Unknown => return None,
                    Ternary::NotRelated => {
                        return self.explain_index_info(stack, &source_info, info, depth);
                    }
                }
            }
        }
        None
    }

    /// `indexInfoRelatedTo`'s report (`relater.go:4677`).
    fn explain_index_info(
        &mut self,
        stack: &mut ChainStack,
        source_info: &crate::index_signatures::IndexInfo,
        target_info: &crate::index_signatures::IndexInfo,
        depth: usize,
    ) -> Option<()> {
        self.explain_pair(stack, source_info.value, target_info.value, depth)?;
        let source_key = self.type_to_string(source_info.key);
        let entry = if source_info.key == target_info.key {
            ChainEntry {
                message: &messages::_0_INDEX_SIGNATURES_ARE_INCOMPATIBLE,
                args: vec![source_key],
            }
        } else {
            let target_key = self.type_to_string(target_info.key);
            ChainEntry {
                message: &messages::_0_AND_1_INDEX_SIGNATURES_ARE_INCOMPATIBLE,
                args: vec![source_key, target_key],
            }
        };
        stack.push(entry);
        Some(())
    }

    /// `signaturesRelatedTo`' N×M arm (`relater.go:4495`): the first target
    /// signature no source signature matches, both erased. Only the first
    /// source's failure elaborates, and with other sources present no
    /// `provides no match` follows. Pairwise instantiations of one
    /// anonymous type or reference target (the arm before it) decline.
    fn explain_signature_sets(
        &mut self,
        stack: &mut ChainStack,
        source: TypeId,
        target: TypeId,
        sources: &[crate::signatures::Signature],
        targets: &[crate::signatures::Signature],
        depth: usize,
    ) -> Option<()> {
        use crate::relater::Ternary;
        let same_anonymous = matches!(
            (&self.type_of(source).data, &self.type_of(target).data),
            (TypeData::Anonymous { symbol: a, .. }, TypeData::Anonymous { symbol: b, .. }) if a == b
        );
        let same_reference = match (
            self.type_reference_targets.get(&source),
            self.type_reference_targets.get(&target),
        ) {
            (Some((a, _)), Some((b, _))) => a == b,
            _ => false,
        };
        if same_anonymous || same_reference {
            return None;
        }
        let first = self.chain_erased_signature(sources.first()?)?;
        for target_signature in targets {
            let erased_target = self.chain_erased_signature(target_signature)?;
            let mut matched = false;
            for source_signature in sources {
                let erased_source = self.chain_erased_signature(source_signature)?;
                match self.compare_signature_ternary(&erased_source, &erased_target)? {
                    Ternary::Related => {
                        matched = true;
                        break;
                    }
                    Ternary::NotRelated => {}
                    Ternary::Unknown => return None,
                }
            }
            if !matched {
                return self.explain_signature(
                    stack,
                    &first,
                    &erased_target,
                    depth,
                    (false, false),
                );
            }
        }
        None
    }

    /// `getErasedSignature`: the signature's own type parameters as `any`.
    fn chain_erased_signature(
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

    /// `isObjectTypeWithInferableIndex` is false: a class or interface
    /// instance (no type literal, object literal, enum or module symbol).
    fn chain_source_index_not_inferable(&mut self, source: TypeId) -> bool {
        // `isObjectTypeWithInferableIndex` also refuses a type with call or
        // construct signatures (`typeHasCallOrConstructSignatures`): a
        // function type misses the signature (r7-reports §88).
        if self.type_of(source).flags.contains(TypeFlags::OBJECT)
            && [crate::signatures::SignatureKind::Call, crate::signatures::SignatureKind::Construct]
                .into_iter()
                .any(|kind| {
                    self.signatures_of_type_kind(source, kind)
                        .is_some_and(|signatures| !signatures.is_empty())
                })
        {
            return true;
        }
        let symbol = match self.type_reference_targets.get(&source) {
            Some(&(symbol, _)) => symbol,
            None => match self.type_of(source).data {
                TypeData::Named { members: Some(symbol), .. } => symbol,
                _ => return false,
            },
        };
        self.binder
            .symbols()
            .get(symbol)
            .flags
            .intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE)
    }

    /// `compareSignaturesRelated` (`relater.go:1458`) for two non-generic
    /// signatures of matching arity: the first failing parameter, else the
    /// return types.
    fn explain_signature(
        &mut self,
        stack: &mut ChainStack,
        source: &crate::signatures::Signature,
        target: &crate::signatures::Signature,
        depth: usize,
        (callback, bivariant_callback): (bool, bool),
    ) -> Option<()> {
        use crate::relater::Ternary;

        // compareSignaturesRelated (`relater.go:1462`): a generic source whose
        // type parameters are not the target's own is instantiated in the
        // context of the target's canonical signature, as
        // `one_signature_related_to` does for the verdict (r7-reports §39).
        let shared = source.type_parameters.is_empty()
            || source.type_parameters.len() == target.type_parameters.len()
                && match (self.type_parameter_types(source), self.type_parameter_types(target)) {
                    (Some(source), Some(target)) => source == target,
                    _ => false,
                };
        let (source, target) = if shared {
            (source.clone(), target.clone())
        } else {
            let target = self.canonical_signature(target.clone())?;
            let source = self.instantiate_signature_in_context(source.clone(), &target)?;
            (source, target)
        };
        let (source, target) = (&source, &target);
        let source_count = self.signature_parameter_count(source);
        let target_count = self.signature_parameter_count(target);
        // `sourceHasMoreParameters` (`relater.go:1470`), reported outside
        // strict arity.
        let source_minimum = self.signature_min_argument_count(source);
        if !self.signature_has_effective_rest(target) && source_minimum > target_count {
            stack.push(ChainEntry {
                message: &messages::TARGET_SIGNATURE_PROVIDES_TOO_FEW_ARGUMENTS_EXPECTED_0_OR_MORE_BUT_GOT_1,
                args: vec![source_minimum.to_string(), target_count.to_string()],
            });
            return Some(());
        }
        let kind = self.nodes.kind(target.declaration);
        let strict_variance = !callback
            && self.strict_function_types
            && !matches!(
                kind,
                SyntaxKind::MethodDeclaration
                    | SyntaxKind::MethodSignature
                    | SyntaxKind::Constructor
            );
        // The `this` types (`relater.go:1505`): a `void` source `this`
        // accepts anything; otherwise forward (unless strict), then reverse.
        if let (Some(source_this), Some(target_this)) =
            (&source.this_parameter, &target.this_parameter)
        {
            let source_this = self.parameter_type(source_this);
            let target_this = self.parameter_type(target_this);
            if source_this != self.intrinsics.void {
                let forward = if strict_variance {
                    Ternary::NotRelated
                } else {
                    self.chain_relate(stack, source_this, target_this)
                };
                match forward {
                    Ternary::Related => {}
                    Ternary::Unknown => return None,
                    Ternary::NotRelated => {
                        match self.chain_relate(stack, target_this, source_this) {
                            Ternary::Related => {}
                            Ternary::Unknown => return None,
                            Ternary::NotRelated => {
                                self.explain_pair(stack, target_this, source_this, depth)?;
                                stack.push(ChainEntry {
                                message: &messages::THE_THIS_TYPES_OF_EACH_SIGNATURE_ARE_INCOMPATIBLE,
                                args: Vec::new(),
                            });
                                return Some(());
                            }
                        }
                    }
                }
            }
        }
        // A non-array rest on either side caps the walk at the shorter list,
        // its last position read as the rest (`relater.go:1532`).
        let non_array_rest = self.signature_non_array_rest_type(source).is_some()
            || self.signature_non_array_rest_type(target).is_some();
        let parameter_count = if non_array_rest {
            source_count.min(target_count)
        } else {
            source_count.max(target_count)
        };
        let rest_index = non_array_rest.then(|| parameter_count.checked_sub(1)).flatten();
        for index in 0..parameter_count {
            let (source_type, target_type) = if rest_index == Some(index) {
                (
                    Some(self.signature_rest_or_any_at_position(source, index)),
                    Some(self.signature_rest_or_any_at_position(target, index)),
                )
            } else {
                (
                    self.signature_type_at_position(source, index),
                    self.signature_type_at_position(target, index),
                )
            };
            let (Some(source_type), Some(target_type)) = (source_type, target_type) else {
                continue;
            };
            if source_type == target_type {
                continue;
            }
            // Callback parameters (`getSingleCallSignature` of each
            // non-nullable side, outside a callback check) compare their
            // signatures in a callback mode, target against source
            // (r7-reports §49).
            if !callback {
                let source_callback =
                    if self.signature_has_instantiated_generic_parameter(source, index) {
                        None
                    } else {
                        let non_nullable = self.get_non_nullable_type(source_type);
                        self.single_call_signature(non_nullable)
                    };
                let target_callback =
                    if self.signature_has_instantiated_generic_parameter(target, index) {
                        None
                    } else {
                        let non_nullable = self.get_non_nullable_type(target_type);
                        self.single_call_signature(non_nullable)
                    };
                let nullable =
                    crate::flow::TypeFacts::IS_UNDEFINED | crate::flow::TypeFacts::IS_NULL;
                if let (Some(source_callback), Some(target_callback)) =
                    (source_callback, target_callback)
                    && source_callback.predicate.is_none()
                    && target_callback.predicate.is_none()
                    && self.get_type_facts(source_type) & nullable
                        == self.get_type_facts(target_type) & nullable
                {
                    let mode = (true, !strict_variance);
                    match self.compare_signature_ternary_in_mode(
                        &target_callback,
                        &source_callback,
                        mode.0,
                        mode.1,
                    )? {
                        Ternary::Related => continue,
                        Ternary::Unknown => return None,
                        Ternary::NotRelated => {
                            self.explain_signature(
                                stack,
                                &target_callback,
                                &source_callback,
                                depth + 1,
                                mode,
                            )?;
                            let source_name = self.parameter_name_at_position(source, index)?;
                            let target_name = self.parameter_name_at_position(target, index)?;
                            stack.push(ChainEntry {
                                message: &messages::TYPES_OF_PARAMETERS_0_AND_1_ARE_INCOMPATIBLE,
                                args: vec![source_name, target_name],
                            });
                            return Some(());
                        }
                    }
                }
            }
            let related = if callback || strict_variance {
                self.chain_relate(stack, target_type, source_type)
            } else {
                match self.chain_relate(stack, source_type, target_type) {
                    Ternary::Related => Ternary::Related,
                    Ternary::Unknown => return None,
                    Ternary::NotRelated => self.chain_relate(stack, target_type, source_type),
                }
            };
            match related {
                Ternary::Related => {}
                Ternary::Unknown => return None,
                Ternary::NotRelated => {
                    self.explain_pair(stack, target_type, source_type, depth)?;
                    let source_name = self.parameter_name_at_position(source, index)?;
                    let target_name = self.parameter_name_at_position(target, index)?;
                    stack.push(ChainEntry {
                        message: &messages::TYPES_OF_PARAMETERS_0_AND_1_ARE_INCOMPATIBLE,
                        args: vec![source_name, target_name],
                    });
                    return Some(());
                }
            }
        }
        let target_return = self.get_return_type_of_signature(target)?;
        if target_return == self.intrinsics.void || target_return == self.intrinsics.any {
            return None;
        }
        // A target type predicate replaces the return comparison
        // (`relater.go:1606`).
        if let Some(target_predicate) = target.predicate.clone() {
            return match source.predicate.clone() {
                Some(source_predicate) => self.explain_type_predicates(
                    stack,
                    source,
                    target,
                    &source_predicate,
                    &target_predicate,
                    depth,
                ),
                None if !target_predicate.asserts => {
                    // `signatureToString` without `WriteArrowStyleSignature`:
                    // `(): void`, not `() => void` (r7-reports §86).
                    let signature = signature_colon_form(&self.signature_to_string(source));
                    stack.push(ChainEntry {
                        message: &messages::SIGNATURE_0_MUST_BE_A_TYPE_PREDICATE,
                        args: vec![signature],
                    });
                    Some(())
                }
                None => None,
            };
        }
        let source_return = self.get_return_type_of_signature(source)?;
        // A bivariant callback check accepts the reverse return relation
        // first (`relater.go:1622`).
        if bivariant_callback
            && self.chain_relate(stack, target_return, source_return) != Ternary::NotRelated
        {
            return None;
        }
        if self.chain_relate(stack, source_return, target_return) != Ternary::NotRelated {
            return None;
        }
        self.explain_pair(stack, source_return, target_return, depth)?;
        let construct = source.kind != crate::signatures::SignatureKind::Call;
        let message = match (source.parameters.is_empty() && target.parameters.is_empty(), construct) {
            (true, false) => &messages::CALL_SIGNATURES_WITH_NO_ARGUMENTS_HAVE_INCOMPATIBLE_RETURN_TYPES_0_AND_1,
            (true, true) => &messages::CONSTRUCT_SIGNATURES_WITH_NO_ARGUMENTS_HAVE_INCOMPATIBLE_RETURN_TYPES_0_AND_1,
            (false, false) => &messages::CALL_SIGNATURE_RETURN_TYPES_0_AND_1_ARE_INCOMPATIBLE,
            (false, true) => &messages::CONSTRUCT_SIGNATURE_RETURN_TYPES_0_AND_1_ARE_INCOMPATIBLE,
        };
        let source_text = self.type_to_string(source_return);
        let target_text = self.type_to_string(target_return);
        stack.push(ChainEntry { message, args: vec![source_text, target_text] });
        Some(())
    }

    fn intersection_has_instantiable(&self, source: TypeId) -> bool {
        match &self.type_of(source).data {
            TypeData::Intersection { types, .. } => types.iter().any(|&part| {
                self.type_of(part).flags.intersects(TypeFlags::INSTANTIABLE | TypeFlags::UNION)
            }),
            _ => true,
        }
    }

    /// `getBestMatchingType` (`relater.go:879`) for an object source:
    /// `Some(Some)` the pick, `Some(None)` none, `None` undecidable.
    #[allow(clippy::option_option)]
    fn chain_best_matching_type(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<Option<TypeId>> {
        use crate::signatures::SignatureKind;
        let TypeData::Union { types, .. } = self.type_of(target).data.clone() else {
            return Some(None);
        };
        // findMatchingDiscriminantType.
        let discriminated = self.find_matching_discriminant_type(source, target)?;
        if discriminated != target {
            return Some(Some(discriminated));
        }
        // findMatchingTypeReferenceOrTypeAliasReference.
        if let Some(&(symbol, _)) = self.type_reference_targets.get(&source) {
            let symbol = self.binder.merged_symbol(symbol);
            if let Some(&found) = types.iter().find(|&&part| {
                self.type_reference_targets
                    .get(&part)
                    .is_some_and(|&(other, _)| self.binder.merged_symbol(other) == symbol)
            }) {
                return Some(Some(found));
            }
        }
        // findBestTypeForObjectLiteral.
        if self.is_object_literal_type(source) || self.fresh_object_literal_types.contains(&source)
        {
            let mut array_like = Vec::with_capacity(types.len());
            for &part in &types {
                array_like.push(
                    !self.type_of(part).flags.intersects(TypeFlags::PRIMITIVE)
                        && self.binding_parent_is_array_like(part)?,
                );
            }
            if array_like.iter().any(|&is| is) {
                return Some(
                    types.iter().zip(&array_like).find(|(_, is)| !**is).map(|(&part, _)| part),
                );
            }
        }
        // findBestTypeForInvokable, call then construct.
        for kind in [SignatureKind::Call, SignatureKind::Construct] {
            if !self.signatures_of_type_kind(source, kind)?.is_empty() {
                for &part in &types {
                    if self.type_of(part).flags.intersects(TypeFlags::PRIMITIVE) {
                        continue;
                    }
                    if !self.signatures_of_type_kind(part, kind)?.is_empty() {
                        return Some(Some(part));
                    }
                }
            }
        }
        // findMostOverlappyType: the last non-primitive constituent sharing
        // the most literal keys (ties go to the later one).
        let source_names = self.get_property_names_of_type(source)?;
        if !self.get_index_infos_of_type(source)?.is_empty() {
            return None;
        }
        let mut best = None;
        let mut count = 0usize;
        for &part in &types {
            if self.type_of(part).flags.intersects(TypeFlags::PRIMITIVE) {
                continue;
            }
            if !self.get_index_infos_of_type(part)?.is_empty() {
                return None;
            }
            let names = self.get_property_names_of_type(part)?;
            let shared = names.iter().filter(|name| source_names.contains(name)).count();
            if shared > 0 && shared >= count {
                best = Some(part);
                count = shared;
            }
        }
        Some(best)
    }

    /// Whether the target's member `name` is optional, from its symbol or
    /// its member table; `None` where neither answers.
    fn chain_property_is_optional(&mut self, target: TypeId, name: &str) -> Option<bool> {
        if let Some(table) = self.relation_property_table(target) {
            return table.iter().find(|(seen, _)| seen == name).map(|&(_, optional)| optional);
        }
        let property = self.get_property_of_type(target, name)?;
        Some(self.property_is_optional(property))
    }

    /// `propertyRelatedTo`'s modifier switch (`relater.go:4271`) for
    /// `name`: `Some(true)` when its private arm reported, `Some(false)`
    /// when no modifier arm applies, `None` for the protected arms (not
    /// retraced) or an unreadable member.
    fn chain_property_privacy(
        &mut self,
        stack: &mut ChainStack,
        source: TypeId,
        target: TypeId,
        name: &str,
    ) -> Option<bool> {
        // A member whose symbol the port cannot name (an inherited member
        // of an instantiated derived class) has no modifier to read; the
        // relation that failed did not fail on one either.
        let (Some(source_property), Some(target_property)) =
            (self.get_property_of_type(source, name), self.get_property_of_type(target, name))
        else {
            return Some(false);
        };
        let private = |checker: &Self, property| {
            checker.property_has_modifier(property, SyntaxKind::PrivateKeyword)
                || name.starts_with('#')
        };
        let protected = |checker: &Self, property| {
            checker.property_has_modifier(property, SyntaxKind::ProtectedKeyword)
        };
        let (source_private, target_private) =
            (private(self, source_property), private(self, target_property));
        if source_private || target_private {
            let same = self.binder.symbols().get(source_property).value_declaration
                == self.binder.symbols().get(target_property).value_declaration;
            if same {
                return Some(false);
            }
            let display = self.chain_property_symbol_display(target, name);
            if source_private && target_private {
                stack.push(ChainEntry {
                    message: &messages::TYPES_HAVE_SEPARATE_DECLARATIONS_OF_A_PRIVATE_PROPERTY_0,
                    args: vec![display],
                });
            } else {
                let (private_side, other) =
                    if source_private { (source, target) } else { (target, source) };
                let private_text = self.type_to_string(private_side);
                let other_text = self.type_to_string(other);
                stack.push(ChainEntry {
                    message: &messages::PROPERTY_0_IS_PRIVATE_IN_TYPE_1_BUT_NOT_IN_TYPE_2,
                    args: vec![display, private_text, other_text],
                });
            }
            return Some(true);
        }
        // A protected target (`relater.go:4285`): `isValidOverrideOf` asks
        // whether the source member's declaring class derives from the
        // target member's; when not, TS2446-family `…is protected but type
        // 'S' is not a class derived from 'T'` (r7-reports §73).
        if protected(self, target_property) {
            let source_class = self.chain_declaring_class(source_property);
            let target_class = self.chain_declaring_class(target_property)?;
            let derived = match source_class {
                Some(class) => self.chain_class_derives_from(class, target_class, 0)?,
                None => false,
            };
            if derived {
                return Some(false);
            }
            let display = self.chain_property_symbol_display(target, name);
            let source_text = match source_class {
                Some(class) => {
                    let declared = self.get_declared_type_of_symbol(class);
                    self.type_to_string(declared)
                }
                None => self.type_to_string(source),
            };
            let target_declared = self.get_declared_type_of_symbol(target_class);
            let target_text = self.type_to_string(target_declared);
            stack.push(ChainEntry {
                message:
                    &messages::PROPERTY_0_IS_PROTECTED_BUT_TYPE_1_IS_NOT_A_CLASS_DERIVED_FROM_2,
                args: vec![display, source_text, target_text],
            });
            return Some(true);
        }
        // `propertyRelatedTo`'s last modifier arm (`relater.go:4296`).
        if protected(self, source_property) {
            let display = self.chain_property_symbol_display(target, name);
            let source_text = self.type_to_string(source);
            let target_text = self.type_to_string(target);
            stack.push(ChainEntry {
                message: &messages::PROPERTY_0_IS_PROTECTED_IN_TYPE_1_BUT_PUBLIC_IN_TYPE_2,
                args: vec![display, source_text, target_text],
            });
            return Some(true);
        }
        Some(false)
    }
}

/// `reportError` (`relater.go:4831`) with its property-message
/// reductions.
fn chain_report(
    stack: &mut ChainStack,
    message: &'static tsr_diagnostics::Message,
    mut args: Vec<String>,
) {
    let message_at = |stack: &Vec<ChainEntry>, index: usize| {
        stack.len().checked_sub(index + 1).map(|at| stack[at].message)
    };
    let mut message = message;
    if std::ptr::eq(message, &raw const messages::TYPES_OF_PROPERTY_0_ARE_INCOMPATIBLE) {
        if message_at(stack, 0).is_some_and(is_excess_property_message) {
            return;
        }
        let returned = message_at(stack, 1).and_then(|next| {
            if std::ptr::eq(next, &raw const messages::CALL_SIGNATURES_WITH_NO_ARGUMENTS_HAVE_INCOMPATIBLE_RETURN_TYPES_0_AND_1) {
                Some(format!("{}()", property_name_arg(&args[0])))
            } else if std::ptr::eq(next, &raw const messages::CONSTRUCT_SIGNATURES_WITH_NO_ARGUMENTS_HAVE_INCOMPATIBLE_RETURN_TYPES_0_AND_1) {
                Some(format!("new {}()", property_name_arg(&args[0])))
            } else if std::ptr::eq(next, &raw const messages::CALL_SIGNATURE_RETURN_TYPES_0_AND_1_ARE_INCOMPATIBLE) {
                Some(format!("{}(...)", property_name_arg(&args[0])))
            } else if std::ptr::eq(next, &raw const messages::CONSTRUCT_SIGNATURE_RETURN_TYPES_0_AND_1_ARE_INCOMPATIBLE) {
                Some(format!("new {}(...)", property_name_arg(&args[0])))
            } else {
                None
            }
        });
        if let Some(arg) = returned {
            message = &messages::THE_TYPES_RETURNED_BY_0_ARE_INCOMPATIBLE_BETWEEN_THESE_TYPES;
            args[0] = arg;
            let keep = stack.len() - 2;
            stack.truncate(keep);
        }
        if let Some(next) = message_at(stack, 1)
            && (std::ptr::eq(next, &raw const messages::TYPES_OF_PROPERTY_0_ARE_INCOMPATIBLE)
                || std::ptr::eq(next, &raw const messages::THE_TYPES_OF_0_ARE_INCOMPATIBLE_BETWEEN_THESE_TYPES,
                )
                || std::ptr::eq(next, &raw const messages::THE_TYPES_RETURNED_BY_0_ARE_INCOMPATIBLE_BETWEEN_THESE_TYPES,
                ))
        {
            let head = property_name_arg(&args[0]);
            let tail = property_name_arg(&stack[stack.len() - 2].args[0]);
            let arg = add_to_dotted_name(&head, &tail);
            let keep = stack.len() - 2;
            stack.truncate(keep);
            if std::ptr::eq(message, &raw const messages::TYPES_OF_PROPERTY_0_ARE_INCOMPATIBLE) {
                message = &messages::THE_TYPES_OF_0_ARE_INCOMPATIBLE_BETWEEN_THESE_TYPES;
            }
            chain_report(stack, message, vec![arg]);
            return;
        }
    }
    stack.push(ChainEntry { message, args });
}

/// `signatureToString`'s declaration form (`(a: T): R`) of this port's
/// arrow-form text (`(a: T) => R`): the arrow after the parameter list's
/// closing parenthesis.
fn signature_colon_form(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut angle = 0usize;
    for (index, &byte) in bytes.iter().enumerate() {
        match byte {
            b'<' if depth == 0 => angle += 1,
            b'>' if depth == 0 && angle > 0 && index > 0 && bytes[index - 1] != b'=' => angle -= 1,
            b'(' if angle == 0 => depth += 1,
            b')' if angle == 0 && depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    if text[index + 1..].starts_with(" => ") {
                        return format!("{}: {}", &text[..=index], &text[index + 5..]);
                    }
                    return text.to_owned();
                }
            }
            _ => {}
        }
    }
    text.to_owned()
}

/// `symbolToString` of a property: an identifier or numeric name as written,
/// any other name quoted.
fn chain_property_display(name: &str) -> String {
    if name.starts_with('[')
        || crate::objects::is_identifier_text(name)
        || (!name.is_empty() && name.bytes().all(|byte| byte.is_ascii_digit()))
    {
        name.to_owned()
    } else {
        format!("\"{name}\"")
    }
}
