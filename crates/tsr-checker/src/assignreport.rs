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
        self.check_excess_properties(target, right_id);
        if self.source_is_an_unnarrowed_reference(right_id, source) {
            return;
        }
        self.report_assignability_failure(left_id, source, target);
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
        self.check_excess_properties(target, initializer_id);
        if self.source_is_an_unnarrowed_reference(initializer_id, source) {
            return;
        }
        self.report_assignability_failure(node, source, target);
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
        self.check_excess_properties(target, initializer_id);
        if self.source_is_an_unnarrowed_reference(initializer_id, source) {
            return;
        }
        self.report_assignability_failure(node, source, target);
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
        self.check_excess_properties(target, expression_id);
        if self.source_is_an_unnarrowed_reference(expression_id, source) {
            return;
        }
        self.report_assignability_failure(node, source, target);
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
    fn check_excess_properties(&mut self, target: TypeId, initializer: NodeId) {
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
                continue;
            }
            // A near miss is TS2561, a different code at the same position.
            let candidates: Vec<&str> = known.iter().map(|(seen, _)| seen.as_str()).collect();
            if crate::check::spelling_suggestion(name, &candidates).is_some() {
                return;
            }
            let Some(at) = self.excess_property_name_node(literal, name) else { return };
            let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
            let span = self.nodes.span(at);
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

    /// TS2741 — `Property '{0}' is missing in type '{1}' but required in type
    /// '{2}'.`
    ///
    /// `reportUnmatchedProperty` (`relater.go:4345`), the `len(props) == 1` arm.
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
    /// Only the one-missing-property arm is ported. Upstream's 2-and-more arms
    /// (TS2739 / TS2740) are gated on `tryElaborateArrayLikeErrors`
    /// (`relater.go:4367`) and fall back to the plain TS2322 head when it
    /// declines; reproducing that needs the elaboration machinery, and the board
    /// row is the single-property one.
    fn missing_required_property(&mut self, source: TypeId, target: TypeId) -> Option<String> {
        let target_properties = self.declared_property_table(target)?;
        let source_properties = self.declared_property_table(source)?;
        let mut missing = target_properties.into_iter().filter(|(name, optional)| {
            !optional && !source_properties.iter().any(|(seen, _)| seen == name)
        });
        let first = missing.next()?;
        // Two or more is upstream's other arm and this port does not have it.
        if missing.next().is_some() {
            return None;
        }
        Some(first.0)
    }

    /// Report the assignability failure at `span`, choosing the code the way
    /// upstream's relation does: a single absent required property is TS2741 and
    /// everything else this port will speak about is TS2322.
    fn report_assignability_failure(&mut self, at: NodeId, source: TypeId, target: TypeId) -> bool {
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return false };
        let span = self.nodes.span(at);
        if REPORT_MISSING_REQUIRED_PROPERTY
            && let Some(property) = self.missing_required_property(source, target)
        {
            let source_text = self.type_to_string(source);
            let target_text = self.type_to_string(target);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::PROPERTY_0_IS_MISSING_IN_TYPE_1_BUT_REQUIRED_IN_TYPE_2,
                    span,
                    [property, source_text, target_text],
                ),
            );
            return true;
        }
        if !self.assignability_is_decidable(source, target)
            || self.is_type_assignable_to(source, target)
        {
            return false;
        }
        let source_text = self.type_to_string(source);
        let target_text = self.type_to_string(target);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1,
                span,
                [source_text, target_text],
            ),
        );
        true
    }

    /// The declines: situations where this port's relation cannot be trusted to
    /// disagree with upstream.
    ///
    /// Each is a *silence*, and silence costs a missing diagnostic. The
    /// alternative — reporting on an answer this port computed from types it did
    /// not finish computing — costs a wrong one, and a wrong one can break a
    /// case that already passes.
    ///
    /// # Why the gate is *primitives only*, and what it cost to learn that
    ///
    /// The first build gated on nothing but `error`/`any`/`unknown` and measured
    /// **947 right against 988 wrong, 100 converts and 29 losses** — the
    /// relation disagreeing with upstream almost exactly half the time. The
    /// wrong column was not one family: `arr_i1 = arr_c1` where `C1 implements
    /// I1` is *assignable* upstream and not here, and every structural row
    /// behaves the same way. That is `checker_types`' 26% non-gradient arriving
    /// as diagnostics.
    ///
    /// Two other classes were in it and are separate rows rather than gate
    /// business:
    ///
    /// - `assignmentCompat1` — `x = y` where a **property is missing** from the
    ///   source is TS2741 / TS2739 / TS2740, not TS2322. The relation's *reason*
    ///   selects the code, so TS2322 at that position is a wrong code at a right
    ///   position.
    /// - `assignToEnum` — `A = undefined` for an `enum A` is TS2628, and
    ///   upstream's `checkIdentifier` returns the error type from that arm, so
    ///   the assignability check never runs.
    ///
    /// What survives is the question the relation answers *without consulting a
    /// members table at all*: both sides primitive, literal, enum-literal, or a
    /// union of those. That is the part of the relation this port has finished.
    fn assignability_is_decidable(&mut self, source: TypeId, target: TypeId) -> bool {
        // A **union source is declined outright**, and this is the narrowing
        // decline rather than a relation one: a union arriving at an assignment
        // position is what narrowing exists to reduce, and the two narrowing
        // mechanisms this port has not built — aliased conditional expressions
        // and inferred type predicates — leave it un-reduced. `controlFlowAliasing`
        // (13 lines) and `inferTypePredicates` (5) are the whole of the family,
        // and both write `let t: string = x` after a narrowing this port does not
        // perform. Reporting there is a wrong diagnostic on correct code.
        self.is_decidable_primitive(source, 0) && self.is_decidable_primitive(target, 0)
    }

    /// Is the source expression a **reference** whose type is still a union?
    ///
    /// Narrowing only ever applies to a reference, so this is the exact shape in
    /// which an unported narrowing mechanism can leave a union that upstream had
    /// already reduced. Restricting the decline to references rather than to
    /// every union source is worth 7 conversions: `var x: number = f()` returning
    /// a union is a real error and no narrowing was ever going to touch it.
    fn source_is_an_unnarrowed_reference(&self, expression: NodeId, source: TypeId) -> bool {
        matches!(
            self.nodes.kind(expression),
            SyntaxKind::Identifier
                | SyntaxKind::PropertyAccessExpression
                | SyntaxKind::ElementAccessExpression
        ) && self.type_of(source).flags.contains(TypeFlags::UNION)
    }

    /// Is this type one the relation can decide from flags alone?
    fn is_decidable_primitive(&self, ty: TypeId, depth: u32) -> bool {
        if depth > 8 {
            return false;
        }
        let type_ = self.type_of(ty);
        if type_.flags.contains(TypeFlags::UNION) {
            let TypeData::Union { types, .. } = &type_.data else { return false };
            // The constituent list is owned by the store, and `self` is borrowed
            // immutably for the whole walk — so the ids are copied out first.
            let constituents: Vec<TypeId> = types.clone();
            return constituents.iter().all(|id| self.is_decidable_primitive(*id, depth + 1));
        }
        DECIDABLE_WITHOUT_MEMBERS.intersects(type_.flags)
            && !type_.flags.intersects(UNDECIDABLE_HERE)
    }
}

/// Does this modifier list carry `async`?
fn has_async(modifiers: &[tsr_ast::ModifierLike<'_>]) -> bool {
    modifiers.iter().any(|modifier| {
        matches!(modifier, tsr_ast::ModifierLike::Token(token) if token.kind == SyntaxKind::AsyncKeyword)
    })
}

/// Is the TS2741 arm live?
///
/// **`false`, and refused with its number** — `checker-notes-diag2.md` §22
/// measured it at **1 conversion for 8 wrong lines**, 0.125 gained per wrong,
/// against a project refusal band of 0.47–1.03.
///
/// A constant rather than a deletion, and the distinction is the point: the
/// machinery it switches — [`Checker::missing_required_property`] and
/// [`crate::member_completeness`]'s property enumeration — is *correct* and is
/// what four named residual families stand between and a positive score. §22
/// lists them. Deleting the code would make the refusal unrevisitable, which is
/// the one thing `docs/conventions.md` forbids about a refusal.
const REPORT_MISSING_REQUIRED_PROPERTY: bool = false;

/// The type flags whose assignability is settled by the flags themselves.
///
/// Everything absent from this set needs a members table, a signature list or an
/// instantiation, and each of those is a place this port's relation can still
/// disagree with upstream — see [`Checker::assignability_is_decidable`].
const DECIDABLE_WITHOUT_MEMBERS: TypeFlags = TypeFlags::STRING
    .union(TypeFlags::NUMBER)
    .union(TypeFlags::BIG_INT)
    .union(TypeFlags::BOOLEAN)
    .union(TypeFlags::ES_SYMBOL)
    .union(TypeFlags::VOID)
    .union(TypeFlags::UNDEFINED)
    .union(TypeFlags::NULL)
    .union(TypeFlags::NEVER)
    .union(TypeFlags::STRING_LITERAL)
    .union(TypeFlags::NUMBER_LITERAL)
    .union(TypeFlags::BIG_INT_LITERAL)
    .union(TypeFlags::BOOLEAN_LITERAL)
    .union(TypeFlags::UNIQUE_ES_SYMBOL)
    .union(TypeFlags::ENUM_LITERAL)
    .union(TypeFlags::ENUM);

/// Flags that veto the gate even when [`DECIDABLE_WITHOUT_MEMBERS`] would admit
/// the type.
///
/// `any` and `unknown` never *fail* the relation, so admitting them can only
/// produce accidents. The **enum** flags are here for a different and measured
/// reason: `isTypeRelatedTo`'s enum arms (`checker.go`'s `EnumLiteral` /
/// `EnumLike` special cases, plus `numberAssignableToEnum`'s numeric-enum
/// widening) are not ported, and enums accounted for 11 of one measurement's 40
/// wrong lines *and its only loss*. They are admitted to the constant only so
/// that the two sets read as one list; the veto is what decides.
const UNDECIDABLE_HERE: TypeFlags =
    TypeFlags::ANY.union(TypeFlags::UNKNOWN).union(TypeFlags::ENUM).union(TypeFlags::ENUM_LITERAL);
