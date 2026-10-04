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
use tsr_binder::SymbolFlags;
use tsr_diagnostics::{Diagnostic, messages};

use crate::{
    checker::Checker,
    flags::TypeFlags,
    types::{TypeData, TypeId},
};

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

impl<'a> Checker<'a, '_> {
    /// `checkAssignmentOperator` (`checker.go:12757`), the `=` arm.
    ///
    /// The error node is the **left operand**, not the whole expression:
    /// `aliasAssignments_1.ts(3,1)` for `x = 1` puts the caret on `x`.
    pub(crate) fn check_assignment_operator(
        &mut self,
        binary: &BinaryExpression<'_>,
        ambient: bool,
    ) {
        if ambient || self.file_has_parse_errors {
            return;
        }
        let (Some(left), Some(right)) = (binary.left, binary.right) else { return };
        let Some(left_id) = left.node_id() else { return };

        // `checkReferenceExpression` (`checker.go:12769`): upstream checks
        // assignability **only** when the left-hand side is a reference, and
        // reports a different code when it is not. A destructuring target is a
        // reference too, but its assignability check is
        // `checkDestructuringAssignment`'s per-element one and not this
        // position, so it is declined here rather than approximated.
        //
        // A **property or element access** target is declined whole, and the
        // reason is `divergentAccessorsTypes2`: a `set` accessor whose parameter
        // type differs from its `get` return type makes the *write* type the
        // setter's, which needs `getWriteTypeOfSymbol`. Reporting from the read
        // type there is a wrong diagnostic on correct code.
        let Some(target) = self.assignment_target_type(left_id) else { return };
        let source = self.check_expression(right);
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
        if ambient || self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let (Some(annotation), Some(initializer)) = (declaration.r#type, declaration.initializer)
        else {
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
        if ambient || self.file_has_parse_errors || self.in_js_file(node) {
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

    /// `checkVariableLikeDeclaration`'s binding-element default check, limited
    /// to annotated variable leaves with literal sources and string-unit targets.
    /// The symbol supplier owns default adjustment; context is not a write target.
    pub(crate) fn check_binding_element_initializer(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::BindingElement(element)) = self.node_map.get(node) else { return };
        if !matches!(element.name, Some(tsr_ast::BindingName::Identifier(_)))
            || element.dot_dot_dot_token.is_some()
            || !matches!(
                element.property_name,
                None | Some(
                    tsr_ast::PropertyName::Identifier(_)
                        | tsr_ast::PropertyName::StringLiteral(_)
                        | tsr_ast::PropertyName::NumericLiteral(_)
                )
            )
        {
            return;
        }
        let Some(pattern) = self.nodes.parent(node) else { return };
        if !matches!(
            self.nodes.kind(pattern),
            SyntaxKind::ObjectBindingPattern | SyntaxKind::ArrayBindingPattern
        ) {
            return;
        }
        let root = self.root_declaration_of(node);
        let Some(Node::VariableDeclaration(declaration)) = self.node_map.get(root) else { return };
        if declaration.r#type.is_none() {
            return;
        }
        let Some(initializer) = element.initializer else { return };
        let undefined = match initializer {
            tsr_ast::Expression::StringLiteral(_) => false,
            tsr_ast::Expression::KeywordExpression(keyword)
                if keyword.kind == SyntaxKind::NullKeyword =>
            {
                false
            }
            tsr_ast::Expression::Identifier(identifier) if identifier.text == "undefined" => true,
            // Primitive-typed expressions can still have unsupported production
            // or circularity; structured defaults widen under nullable contexts.
            _ => return,
        };
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        if self.binder.symbols().get(symbol).value_declaration != Some(node) {
            return;
        }
        // Native computes the adjusted symbol type before checking the source.
        let target = self.get_type_of_symbol(symbol);
        let supported_part = |ty| {
            let flags = self.type_of(ty).flags;
            flags == TypeFlags::STRING_LITERAL
                || flags == TypeFlags::NULL
                || flags == TypeFlags::UNDEFINED
        };
        let supported = match &self.type_of(target).data {
            TypeData::StringLiteral(_) => true,
            TypeData::Union { types, .. } => {
                types.iter().copied().all(supported_part)
                    && types.iter().any(|&ty| self.type_of(ty).flags == TypeFlags::STRING_LITERAL)
            }
            _ => false,
        };
        if !supported {
            return;
        }
        let source = self.check_expression(initializer);
        if undefined && source != self.intrinsics.undefined {
            return;
        }
        let Some(initializer_id) = initializer.node_id() else { return };
        self.report_assignability_failure(node, initializer_id, source, target);
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
        if ambient || self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::ReturnStatement(statement)) = self.node_map.get(node) else { return };
        let Some(expression) = statement.expression else { return };
        let Some(annotation) = self.enclosing_return_annotation(node) else { return };
        let target = self.get_type_from_type_node(annotation);
        let source = self.check_expression(expression);
        let Some(expression_id) = expression.node_id() else { return };
        // §73: the elaboration reports the member instead of the outer message.
        let before = self.diagnostics.len();
        self.check_excess_properties(target, expression_id);
        if self.diagnostics.len() != before {
            return;
        }
        self.report_assignability_failure(node, expression_id, source, target);
    }

    /// The return annotation of the function a `return` belongs to, where this
    /// port can use it directly.
    ///
    /// **Async and generator functions are declined**: their annotation is a
    /// `Promise<T>` or an `Iterator<…>` and the value returned is compared
    /// against the *unwrapped* `T` (`checkReturnStatement`'s
    /// `getReturnTypeFromAnnotation` unwrapping, `checker.go:12420`). Comparing
    /// against the wrapper is a wrong diagnostic on correct code.
    fn enclosing_return_annotation(&self, node: NodeId) -> Option<tsr_ast::TypeNode<'a>> {
        let mut at = self.nodes.parent(node);
        while let Some(current) = at {
            let typed = self.node_map.get(current)?;
            let parts = match typed {
                Node::FunctionDeclaration(n) => {
                    Some((n.r#type, n.asterisk_token.is_some(), n.modifiers))
                }
                Node::FunctionExpression(n) => {
                    Some((n.r#type, n.asterisk_token.is_some(), n.modifiers))
                }
                Node::ArrowFunction(n) => Some((n.r#type, false, n.modifiers)),
                Node::MethodDeclaration(n) => {
                    Some((n.r#type, n.asterisk_token.is_some(), n.modifiers))
                }
                Node::GetAccessorDeclaration(n) => Some((n.r#type, false, n.modifiers)),
                Node::ConstructorDeclaration(_) | Node::SetAccessorDeclaration(_) => return None,
                _ => None,
            };
            if let Some((annotation, generator, modifiers)) = parts {
                if generator || has_async(modifiers) {
                    return None;
                }
                return annotation;
            }
            at = self.nodes.parent(current);
        }
        None
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
        if self.nodes.kind(node) != SyntaxKind::Identifier {
            return None;
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
        // Two declarations of one name merge their types
        // (`duplicateLocalVariable1`), and this port's merge is not upstream's;
        // and a `const` target is TS2588, reported *instead of* the relation.
        let declarations: Vec<NodeId> = entry.declarations.to_vec();
        if declarations.len() != 1 {
            return None;
        }
        if self.declaration_is_constant(declarations[0])
            || self.declaration_is_auto_typed(declarations[0])
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
        if !self.declared_members_are_complete(target) {
            return;
        }
        let Some(known) = self.declared_property_table(target) else { return };
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
            // A near miss is TS2561, a different code at the same position.
            let candidates: Vec<&str> = known.iter().map(|(seen, _)| seen.as_str()).collect();
            if crate::check::spelling_suggestion(name, &candidates).is_some() {
                return;
            }
            let Some(at) = self.excess_property_name_node(literal, name) else { return };
            let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
            let span = self.error_span(at);
            let printed = self.type_to_string(target);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_0_DOES_NOT_EXIST_IN_TYPE_1,
                    span,
                    [name.to_string(), printed],
                ),
            );
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
    fn excess_property_name_node(
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
        let target_properties = self.declared_property_table(target)?;
        let source_properties = self.declared_property_table(source)?;
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

    /// TS2345 at an argument position — the same verdict machinery as
    /// [`Checker::report_assignability_failure`] with a different code and no
    /// TS2741 arm (an argument's missing property is elaborated differently
    /// upstream).
    /// Answers **whether it reported**, so the caller can stop:
    /// `getSignatureApplicabilityError` returns on the first failing argument
    /// (`checker-notes-diag2.md` §59).
    pub(crate) fn report_argument_failure(
        &mut self,
        at: NodeId,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        if self.nodes.kind(at) == SyntaxKind::ObjectLiteralExpression
            && self.type_of(target).flags.contains(TypeFlags::UNION)
        {
            return false;
        }
        if !self.pair_is_reportable(source, target) {
            return false;
        }
        if self.relate_ternary(source, target, crate::relater::Relation::Assignable)
            != crate::relater::Ternary::NotRelated
            && !self.object_against_primitive(source, target)
        {
            return false;
        }
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return false };
        let span = self.error_span(at);
        let displayed_source = self.assignability_source_for_error_display(source, target);
        let source_text = self.type_to_string(displayed_source);
        let target_text = self.type_to_string(target);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::ARGUMENT_OF_TYPE_0_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE_1,
                span,
                [source_text, target_text],
            ),
        );
        true
    }

    /// Report the assignability failure at `span`, choosing the code the way
    /// upstream's relation does: absent required properties are TS2741/2739/2740,
    /// a direct exact-optional missing-property write is TS2412, a whole-object
    /// exact-optional mismatch is TS2375, and other failures are TS2322.
    fn report_assignability_failure(
        &mut self,
        at: NodeId,
        source_node: NodeId,
        source: TypeId,
        target: TypeId,
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
                    let probe_span = self.error_span(at);
                    self.assignability_probe.push((probe_file, probe_span, $verdict));
                }
            };
        }
        if self.nodes.kind(source_node) == SyntaxKind::ObjectLiteralExpression
            && self.type_of(target).flags.contains(TypeFlags::UNION)
        {
            probe!(PROBE_OBJECT_LITERAL_UNION);
            return false;
        }
        // `elaborateError` (`relater.go:440`) runs **before** the whole-expression
        // report and, when it speaks, `checkTypeRelatedToEx` stays silent. The
        // hand-off is exclusive by construction here because both live in this
        // one function: elaborating returns, it does not fall through. §176.
        if self.elaborate_object_literal(source_node, source, target) {
            probe!(PROBE_REPORTED);
            return true;
        }
        if !self.pair_is_reportable(source, target) {
            probe!(PROBE_PAIR_NOT_REPORTABLE);
            return false;
        }
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return false };
        let span = self.error_span(at);
        if REPORT_MISSING_REQUIRED_PROPERTY
            && let Some(properties) = self.missing_required_property(source, target)
        {
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
            probe!(PROBE_REPORTED);
            self.report(file, Diagnostic::with_args(message, span, args));
            return true;
        }
        // **`relate_ternary`, not `is_type_assignable_to`.** The relater is
        // three-valued (`crate::relater::Ternary`) and its own doc comment names
        // the caller this distinction exists for: *"one that acts on a
        // negative"*. TS2322 is exactly that caller, and the binary projection —
        // which collapses `Unknown` into `false` — is what produced this
        // module's first measurement of **947 right against 988 wrong**. Every
        // undecidable pair was being reported as an error.
        if self.relate_ternary(source, target, crate::relater::Relation::Assignable)
            != crate::relater::Ternary::NotRelated
            && !self.object_against_primitive(source, target)
        {
            probe!(PROBE_RELATION_DECLINED);
            return false;
        }
        probe!(PROBE_REPORTED);
        let displayed_source = self.assignability_source_for_error_display(source, target);
        let source_text = self.type_to_string(displayed_source);
        let target_text = self.type_to_string(target);
        let message = if self.exact_optional_property_assignment_mismatch(at, source) {
            &messages::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1_WITH_EXACTOPTIONALPROPERTYTYPES_COLON_TRUE_CONSIDER_ADDING_UNDEFINED_TO_THE_TYPE_OF_THE_TARGET
        } else if source_text != target_text && self.exact_optional_object_mismatch(source, target)
        {
            &messages::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1_WITH_EXACTOPTIONALPROPERTYTYPES_COLON_TRUE_CONSIDER_ADDING_UNDEFINED_TO_THE_TYPES_OF_THE_TARGET_S_PROPERTIES
        } else {
            &messages::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1
        };
        self.report(file, Diagnostic::with_args(message, span, [source_text, target_text]));
        true
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
    /// The enum veto stays where it was, in
    /// [`Checker::assignability_is_decidable`]'s successor below.
    pub(crate) fn pair_is_reportable(&mut self, source: TypeId, target: TypeId) -> bool {
        let intrinsics = self.intrinsics();
        let (unknown, any) = (intrinsics.unknown, intrinsics.any);
        for side in [source, target] {
            // `Checker::is_error` and not `== intrinsics.error`: an unresolved
            // type REFERENCE mints a `TypeData::Named` carrying the written
            // text and answers `is_error` without being that intrinsic
            // (`checker-notes-diag2.md` §44). It is the same accident the arm
            // above describes, under a different type id — a type the port
            // could not build, which the relater cannot distinguish from one
            // that failed.
            if self.is_error(side) || side == unknown || side == any {
                return false;
            }
            if self.type_of(side).flags.intersects(UNDECIDABLE_HERE) {
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

    /// `elaborateObjectLiteral` (`relater.go:498`) — report on the offending
    /// **property** rather than on the literal.
    ///
    /// Returns whether it reported, which is upstream's contract: a `true` here
    /// is what stops the whole-expression diagnostic being issued at all.
    ///
    /// §175 ranked this anchor first on the board — 19 of the 132 TS2322 cases
    /// that are gated on a reporting anchor and nothing else, and 101 of the
    /// 691 never-reached lines.
    fn elaborate_object_literal(
        &mut self,
        source_node: NodeId,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some(Node::ObjectLiteralExpression(literal)) = self.node_map.get(source_node) else {
            return false;
        };
        // `target.flags&(TypeFlagsPrimitive|TypeFlagsNever) != 0` — a primitive
        // or `never` target has no properties to elaborate against, and
        // upstream returns before the loop.
        if self.type_of(target).flags.intersects(TypeFlags::PRIMITIVE | TypeFlags::NEVER) {
            return false;
        }
        // `getBestMatchIndexedAccessTypeOrUndefined` picks a union constituent;
        // this port has only the non-union lookup, and a union target is
        // already declined by the caller's own object-literal arm, so the two
        // agree on every input that reaches here. §176.
        if self.type_of(target).flags.contains(TypeFlags::UNION) {
            return false;
        }
        // A spread contributes properties this port cannot enumerate — the same
        // decline `check_excess_properties` makes, for the same reason.
        if literal.properties.iter().any(|property| {
            matches!(property, tsr_ast::ObjectLiteralElementLike::SpreadAssignment(_))
        }) {
            return false;
        }
        let mut reported = false;
        for property in literal.properties {
            // `ast.KindPropertyAssignment` — the arm that carries an
            // initialiser. The accessor and shorthand arms elaborate through a
            // different path (`elaborateElement` with `next == nil`) and are in
            // §175's tail, not its head.
            let tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) = property else {
                continue;
            };
            let Some(name_id) = assignment.name.node_id() else { continue };
            // `getLiteralTypeFromProperty(…, StringOrNumberLiteralOrUnique)` —
            // a computed non-literal name yields no usable name type and
            // upstream `continue`s.
            let Some(name) = self.identifier_text(name_id).map(str::to_string) else { continue };
            if assignment.initializer.and_then(|e| e.node_id()).is_none() {
                continue;
            }
            // `getBestMatchIndexedAccessTypeOrUndefined(source, target, nameType)`
            // — absent from the target means excess, which is TS2353's row and
            // not this one.
            // The indexed-access result uses the concrete target receiver.
            // Reading the declaration symbol alone loses its mapper, so a
            // member declared as T on C<number> would be compared against T.
            let Some(target_property_type) = self.get_type_of_property_of_type(target, &name)
            else {
                continue;
            };
            // `getIndexedAccessTypeOrUndefined(source, nameType, …)` reads the
            // completed source member, including mutable-location widening.
            // A fresh initializer alone still has its literal type here.
            let Some(source_property_type) = self.get_type_of_property_of_type(source, &name)
            else {
                continue;
            };
            // `checkTypeRelatedTo(sourcePropType, targetPropType, …)` — the
            // three-valued form, and reporting only on a **confident**
            // `NotRelated`, which is §25's rule for a rule acting on a negative.
            if self.relate_ternary(
                source_property_type,
                target_property_type,
                crate::relater::Relation::Assignable,
            ) != crate::relater::Ternary::NotRelated
            {
                continue;
            }
            if !self.pair_is_reportable(source_property_type, target_property_type) {
                continue;
            }
            let Some(file) = self.source_file_of_for_diagnostics(name_id) else { continue };
            // `createDiagnosticForNode(prop, …)` — the property **name**, which
            // is the anchor §175 measured and the one `check_excess_properties`
            // already uses.
            let span = self.nodes.span(name_id);
            let displayed_source = self
                .assignability_source_for_error_display(source_property_type, target_property_type);
            let source_text = self.type_to_string(displayed_source);
            let target_text = self.type_to_string(target_property_type);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1,
                    span,
                    [source_text, target_text],
                ),
            );
            reported = true;
        }
        reported
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

/// Flags that veto [`Checker::pair_is_reportable`] outright.
///
/// The **enum** flags are here for a measured reason: `isTypeRelatedTo`'s enum arms (`checker.go`'s `EnumLiteral` /
/// `EnumLike` special cases, plus `numberAssignableToEnum`'s numeric-enum
/// widening) are not ported, and enums accounted for 11 of one measurement's 40
/// wrong lines *and its only loss*. They are admitted to the constant only so
/// that the two sets read as one list; the veto is what decides.
const UNDECIDABLE_HERE: TypeFlags =
    TypeFlags::ANY.union(TypeFlags::UNKNOWN).union(TypeFlags::ENUM).union(TypeFlags::ENUM_LITERAL);
