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
    /// `for await` iterates `getIteratedTypeOrElementType`'s async arm, which
    /// this port's `for_of_element_type` does not answer, so it is declined.
    /// The left-hand type comes from [`Checker::assignment_target_type`], the
    /// same declared-type reader (and declines) the `=` arm uses.
    pub(crate) fn check_for_of_reference_assignment(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.in_js_file(node) {
            return;
        }
        let Some(Node::ForInOrOfStatement(statement)) = self.node_map.get(node) else { return };
        if statement.kind.kind != SyntaxKind::ForOfStatement || statement.await_modifier.is_some() {
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
        let iterable = self.check_expression_at_node(right_id);
        let Some(source) = self.for_of_element_type(iterable) else { return };
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
                if parameter.question_token.is_some() || parameter.dot_dot_dot_token.is_some() {
                    return;
                }
                (parameter.r#type, parameter.initializer)
            }
            _ => return,
        };
        let (Some(annotation), Some(initializer)) = (annotation, initializer) else { return };
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
    /// Declined: `yield*` (its yielded type is the delegated iterable's
    /// iterated type, `checkIteratedTypeOrElementType`) and async generators
    /// (the yielded type is awaited first, `getAwaitedType`).
    pub(crate) fn check_yield_expression_assignability(&mut self, node: NodeId) {
        if self.in_js_file(node) {
            return;
        }
        let Some(Node::YieldExpression(expression)) = self.node_map.get(node) else { return };
        if expression.asterisk_token.is_some() {
            return;
        }
        let Some(container) = self.containing_function(node) else { return };
        let (asterisk, annotation, modifiers) = match self.node_map.get(container) {
            Some(Node::FunctionDeclaration(f)) => (f.asterisk_token, f.r#type, f.modifiers),
            Some(Node::MethodDeclaration(f)) => (f.asterisk_token, f.r#type, f.modifiers),
            Some(Node::FunctionExpression(f)) => (f.asterisk_token, f.r#type, f.modifiers),
            _ => return,
        };
        if asterisk.is_none() || has_async(modifiers) {
            return;
        }
        let Some(annotation) = annotation else { return };
        let mut return_type = self.get_type_from_type_node(annotation);
        if self.type_of(return_type).flags.contains(TypeFlags::UNION) {
            let mut undecided = false;
            return_type = self.filter_type(return_type, |checker, constituent| {
                checker
                    .generator_instantiation_assignable_to_return_type(constituent, false)
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
            false,
        ) else {
            return;
        };
        let target = yield_type.unwrap_or(self.intrinsics.any);
        if let Some(operand) = expression.expression.and_then(|operand| operand.node_id()) {
            let source = self.check_expression_at_node(operand);
            let before = self.diagnostics.len();
            self.check_excess_properties(target, operand);
            if self.diagnostics.len() != before {
                return;
            }
            self.report_assignability_failure(operand, operand, source, target);
        } else {
            let undefined = self.intrinsics.undefined;
            self.report_assignability_failure_with(node, None, undefined, target);
        }
    }

    /// `checkReturnStatement` (`checker.go:12400`) — the returned expression
    /// against the function's **written** return annotation.
    ///
    /// The error node is the **return statement**, not the expression:
    /// `arrayAssignmentTest1.ts(6,16)` for `IM1():void[] {return null;}` is
    /// column 16, which is the `r` of `return`.
    ///
    /// Only a *written* annotation is used. An inferred return type is computed
    /// from the very returns being checked, so a mismatch against it is not a
    /// diagnostic upstream would ever report.
    pub(crate) fn check_return_statement(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.in_js_file(node) {
            return;
        }
        let Some(Node::ReturnStatement(statement)) = self.node_map.get(node) else { return };
        let expression = statement.expression.and_then(|expression| expression.node_id());
        let Some(container) = self.return_statement_container(node) else { return };
        match self.node_map.get(container) {
            Some(Node::SetAccessorDeclaration(_)) => return,
            Some(Node::ConstructorDeclaration(_)) => {
                if let Some(expression) = expression {
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
    /// container, unwrapped as `unwrapReturnType` would for a plain function: the
    /// written annotation, or for an unannotated get accessor with a bindable
    /// name its set accessor's parameter annotation (`getAnnotatedAccessorType`).
    ///
    /// Answers `(returnType, unwrappedReturnType, isAsync)`. An async
    /// function's target is `unwrapReturnType`'s `getAwaitedTypeNoAlias` of
    /// its annotation (`checker.go:20388`); a gap, or native's `nil` (whose
    /// `errorType` target relates to everything), declines. **Generators are
    /// declined**: their annotation is an `Iterator<…>` whose return
    /// iteration type is not read here.
    fn return_type_from_annotation(&mut self, container: NodeId) -> Option<(TypeId, TypeId, bool)> {
        let (annotation, generator, modifiers) = match self.node_map.get(container)? {
            Node::FunctionDeclaration(n) => (n.r#type, n.asterisk_token.is_some(), n.modifiers),
            Node::FunctionExpression(n) => (n.r#type, n.asterisk_token.is_some(), n.modifiers),
            Node::ArrowFunction(n) => (n.r#type, false, n.modifiers),
            Node::MethodDeclaration(n) => (n.r#type, n.asterisk_token.is_some(), n.modifiers),
            Node::GetAccessorDeclaration(n) => (n.r#type, false, n.modifiers),
            _ => return None,
        };
        if generator {
            return None;
        }
        if has_async(modifiers) {
            let return_type = self.get_type_from_type_node(annotation?);
            let target = self.awaited_type_no_alias(return_type)?;
            return Some((return_type, target, true));
        }
        if let Some(annotation) = annotation {
            let return_type = self.get_type_from_type_node(annotation);
            return Some((return_type, return_type, false));
        }
        let annotation = self.set_accessor_parameter_annotation(container)?;
        let return_type = self.get_type_from_type_node(annotation);
        Some((return_type, return_type, false))
    }

    /// `getAnnotatedAccessorType` of the set accessor paired with an
    /// unannotated get accessor: its first non-`this` parameter's annotation.
    /// A computed name only pairs when the binder bound it (`hasBindableName`).
    fn set_accessor_parameter_annotation(&self, getter: NodeId) -> Option<tsr_ast::TypeNode<'a>> {
        if self.nodes.kind(getter) != SyntaxKind::GetAccessor {
            return None;
        }
        let symbol = self.binder.symbol_of(getter)?;
        let declarations = self.binder.symbols().get(symbol).declarations.clone();
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
    /// `checkReturnExpression`, to `unwrapReturnType` of its written return
    /// annotation. The error node is the body itself. An async arrow relates
    /// against the annotation's `getAwaitedTypeNoAlias`, as a statement does.
    pub(crate) fn check_arrow_expression_body(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.in_js_file(node) {
            return;
        }
        let Some(Node::ArrowFunction(arrow)) = self.node_map.get(node) else { return };
        let (Some(annotation), Some(body)) = (arrow.r#type, arrow.body) else { return };
        let Some(body_id) = body.node_id() else { return };
        if self.nodes.kind(body_id) == SyntaxKind::Block {
            return;
        }
        let is_async = has_async(arrow.modifiers);
        let mut target = self.get_type_from_type_node(annotation);
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
    fn skip_outer_parentheses(&self, mut node: NodeId) -> NodeId {
        while let Some(Node::ParenthesizedExpression(inner)) = self.node_map.get(node)
            && let Some(expression) = inner.expression.and_then(|e| e.node_id())
        {
            node = expression;
        }
        node
    }

    /// `getEffectiveCheckNode` (`checker.go:9381`): `ast.SkipOuterExpressions`
    /// over parentheses and `satisfies`, repeatedly (TS files; the JS-only
    /// JSDoc-assertion exclusion never applies because JS files are declined).
    fn effective_check_node(&self, mut node: NodeId) -> NodeId {
        loop {
            let inner = match self.node_map.get(node) {
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
    fn is_assignable_reference(&self, node: NodeId) -> bool {
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
    fn assignment_target_type(&mut self, node: NodeId) -> Option<TypeId> {
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
        let symbol = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            node,
            text,
            SymbolFlags::VALUE | SymbolFlags::ALIAS,
        )?;
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
        Some(self.get_type_of_symbol(symbol))
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
        // Index-signature-fatal completeness: an index signature on the target
        // makes every name known, so this is the predicate that must decline it
        // — the same one `crate::nonexistent_property` uses, and *not* the
        // property enumeration TS2741 uses.
        if !self.relation_members_are_complete(target) {
            return;
        }
        let Some(known) = self.relation_property_table(target) else { return };
        // An **empty** target is not an excess-property site. `class C {}` with
        // `c = { foo: '' }` reads TS2322 upstream, not TS2353
        // (`conformance/classWithEmptyBody`), because nothing about the literal
        // is assignable in the first place and the excess check only speaks when
        // the rest of the relation would have succeeded. This port runs no
        // relation here, so the emptiness test stands in for that condition —
        // and it was this rule's only loss.
        if known.is_empty() {
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
        for name in names {
            if known.iter().any(|(seen, _)| seen == name) {
                // A **known** property is not excess; its value is checked
                // against the target's member instead.
                // `everyTypeWithAnnotationAndInvalidInitializer.ts(43,28)` is the
                // `id` of `var anObjectLiteral: I = { id: 'a string' }` — the
                // property *name*, not the value and not the declaration.
                self.check_object_literal_member(literal, target, name);
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
        let Some(member) = self.get_type_of_property_of_type(target, name) else { return };
        let source = self.check_expression_at_node(value);
        self.report_assignability_failure(at, value, source, member);
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
        Diagnostic::with_args(message, span, args)
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
        target: TypeId,
        properties: &[String],
    ) -> MissingPropertyHead {
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
        if chain_source == source && chain_target == target {
            return MissingPropertyHead::Suppressed;
        }
        let displayed_source = self.assignability_source_for_error_display(source, target);
        if self.type_to_string(displayed_source) == self.type_to_string(chain_source)
            && self.type_to_string(target) == self.type_to_string(chain_target)
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
        if !self.binder.symbols().get(symbol).members.is_empty() {
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
        let normalized = self.no_infer_base_type(target).unwrap_or(target);
        if not_related
            && let Some(properties) = self
                .missing_required_property(source, normalized)
                .or_else(|| self.unmatched_property_report(source, normalized))
        {
            let MissingPropertyHead::Kept(chain) =
                self.missing_property_chain(span, source, normalized, &properties)
            else {
                self.report_missing_properties(file, span, source, normalized, &properties);
                return true;
            };
            self.report_argument_head(file, span, source, target, None, chain);
            return true;
        }
        self.report_argument_head(file, span, source, target, signature_error, None);
        true
    }

    /// `reportRelationError`'s TS2345 head (`relater.go:4751`) for
    /// [`Checker::report_argument_failure`], with the signature elaboration or
    /// the missing-property message kept under it.
    fn report_argument_head(
        &mut self,
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
        diagnostic.add_message_chain(chain);
        self.report(file, diagnostic);
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
        let normalized = self.no_infer_base_type(target).unwrap_or(target);
        if REPORT_MISSING_REQUIRED_PROPERTY
            && union_literal.is_none()
            && let Some(properties) = self.missing_required_property(source, normalized)
        {
            probe!(PROBE_REPORTED);
            match self.missing_property_chain(span, source, normalized, &properties) {
                MissingPropertyHead::Suppressed => {
                    self.report_missing_properties(file, span, source, normalized, &properties);
                }
                MissingPropertyHead::Kept(chain) => {
                    self.report_relation_head(at, file, span, source, target, head, None, chain);
                }
            }
            return true;
        }
        // **`relate_ternary`, not `is_type_assignable_to`.** The relater is
        // three-valued (`crate::relater::Ternary`) and its own doc comment names
        // the caller this distinction exists for: *"one that acts on a
        // negative"*. TS2322 is exactly that caller, and the binary projection —
        // which collapses `Unknown` into `false` — is what produced this
        // module's first measurement of **947 right against 988 wrong**. Every
        // undecidable pair was being reported as an error.
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
        if !not_related && !self.object_against_primitive(source, target) {
            probe!(PROBE_RELATION_DECLINED);
            return false;
        }
        probe!(PROBE_REPORTED);
        if not_related && self.readonly_to_mutable_array_like(source, target) {
            self.report_readonly_to_mutable(file, span, source, target);
            return true;
        }
        if not_related
            && union_literal.is_none()
            && let Some(properties) = self.unmatched_property_report(source, normalized)
        {
            match self.missing_property_chain(span, source, normalized, &properties) {
                MissingPropertyHead::Suppressed => {
                    self.report_missing_properties(file, span, source, normalized, &properties);
                }
                MissingPropertyHead::Kept(chain) => {
                    self.report_relation_head(at, file, span, source, target, head, None, chain);
                }
            }
            return true;
        }
        self.report_relation_head(at, file, span, source, target, head, signature_error, None);
        true
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
        let source_text = self.type_to_string(displayed_source);
        let target_text = self.type_to_string(target);
        let message = if let Some(head) = head {
            head
        } else if self.exact_optional_property_assignment_mismatch(at, source) {
            &messages::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1_WITH_EXACTOPTIONALPROPERTYTYPES_COLON_TRUE_CONSIDER_ADDING_UNDEFINED_TO_THE_TYPE_OF_THE_TARGET
        } else if source_text != target_text && self.exact_optional_object_mismatch(source, target)
        {
            &messages::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1_WITH_EXACTOPTIONALPROPERTYTYPES_COLON_TRUE_CONSIDER_ADDING_UNDEFINED_TO_THE_TYPES_OF_THE_TARGET_S_PROPERTIES
        } else {
            &messages::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1
        };
        let mut diagnostic =
            self.relation_diagnostic(span, source, target, message, source_text, target_text);
        if let Some(mut signature_error) = signature_error {
            signature_error.span = span;
            diagnostic.add_message_chain(Some(signature_error));
        }
        diagnostic.add_message_chain(chain);
        self.report(file, diagnostic);
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
        if !self.type_of(target).flags.contains(TypeFlags::TYPE_PARAMETER)
            || self.variance_marker_types.contains(&target)
        {
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
        property_type == self.intrinsics.missing
            || matches!(&self.type_of(property_type).data, TypeData::Union { types, .. }
                if types.first() == Some(&self.intrinsics.missing))
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
    /// itself — upstream adds "Did you mean to call this expression?" as
    /// related information, which the suite does not compare. A pair the
    /// relation does not reject falls through to the remaining arms.
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
        self.report_relation_failure(node, span, None, source, target, head)
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
        let span = self.error_span(prop);
        self.report_relation_failure(prop, span, Some(source_node), source, target, head)
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
        for (name_id, next, name, target_property_type, head) in members {
            // `getIndexedAccessTypeOrUndefined(source, nameType, …)` reads the
            // completed source member, including mutable-location widening.
            let Some(source_property_type) = self.get_type_of_property_of_type(source, &name)
            else {
                continue;
            };
            reported |= self.elaborate_element_with(
                name_id,
                next,
                source_property_type,
                target_property_type,
                head,
            );
        }
        Some(reported)
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
        // `!noImplicitAny && JSLiteral`: JavaScript literals are not ported.
        if self.in_js_file(node) {
            return None;
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
    fn filter_primitives_if_contains_non_primitive(&mut self, union: TypeId) -> TypeId {
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
    fn certified_property_names(&mut self, t: TypeId) -> Option<Vec<String>> {
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
        let Some(names) = self.get_property_names_of_type(apparent) else { return false };
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

    /// `isWeakType` (`relater.go:681`): an object type with at least one
    /// property, every property optional, and no signatures or index
    /// signatures; an intersection of only such. `None` where the member
    /// table is not certified.
    fn is_weak_type(&mut self, t: TypeId) -> Option<bool> {
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
        let table = self.relation_property_table(t)?;
        if table.is_empty() || table.iter().any(|(_, optional)| !optional) {
            return Some(false);
        }
        if !self.get_index_infos_of_type(t)?.is_empty() {
            return Some(false);
        }
        Some(!self.type_has_call_or_construct_signatures_certified(t)?)
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
                crate::relater::Ternary::Unknown => return false,
            }
        }
        let source_text = self.type_to_string(source);
        let target_text = self.type_to_string(target);
        let message = if callable {
            &messages::VALUE_OF_TYPE_0_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE_1_DID_YOU_MEAN_TO_CALL_IT
        } else {
            &messages::TYPE_0_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE_1
        };
        self.report(file, Diagnostic::with_args(message, span, [source_text, target_text]));
        true
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
    fn type_of_property_in_types(&mut self, types: &[TypeId], name: &str) -> Option<TypeId> {
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
    fn find_matching_discriminant_type(
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
            if self.is_discriminant_property_of_union(&types, &name)? {
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

    /// `isDiscriminantProperty` (`relater.go:1087`) over the synthetic union
    /// property `createUnionOrIntersectionProperty` (`checker.go:21452`)
    /// would build: `CheckFlagsNonUniformAndLiteral` — the constituents that
    /// have the property disagree on its type (`HasNonUniformType`) and one
    /// of those types is `isLiteralType` or `isPatternLiteralType`
    /// (`HasLiteralType`) — and the property's type is not generic. A
    /// property found in one constituent only is not synthetic-non-uniform.
    /// `None` where a constituent lacking the name has an uncertified table.
    fn is_discriminant_property_of_union(&mut self, types: &[TypeId], name: &str) -> Option<bool> {
        let mut first = None;
        let mut non_uniform = false;
        let mut literal = false;
        let mut generic = false;
        for &part in types {
            let flags = self.type_of(part).flags;
            if self.is_error(part) || flags.contains(TypeFlags::NEVER) {
                continue;
            }
            let Some(member) = self.get_type_of_property_of_type(part, name) else {
                if flags.intersects(TypeFlags::OBJECT | TypeFlags::INTERSECTION) {
                    self.type_of_property_or_index_signature(part, name).ok()?;
                }
                continue;
            };
            match first {
                None => first = Some(member),
                Some(seen) if seen != member => non_uniform = true,
                Some(_) => {}
            }
            let member_flags = self.type_of(member).flags;
            literal |= self.is_literal_type_for_discriminant(member)
                || member_flags.intersects(TypeFlags::TEMPLATE_LITERAL | TypeFlags::STRING_MAPPING);
            generic |= self.maybe_type_of_kind(member, TypeFlags::INSTANTIABLE);
        }
        Some(non_uniform && literal && !generic)
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
        self.elaborate_element(body, Some(body), source_return, target_return)
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
