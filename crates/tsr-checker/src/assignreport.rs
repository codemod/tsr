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
        if !self.assignability_is_decidable(source, target)
            || self.source_is_an_unnarrowed_reference(right_id, source)
            || self.is_type_assignable_to(source, target)
        {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(left_id) else { return };
        let span = self.nodes.span(left_id);
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
        if ambient || self.file_has_parse_errors {
            return;
        }
        let (Some(annotation), Some(initializer)) = (declaration.r#type, declaration.initializer)
        else {
            return;
        };
        let target = self.get_type_from_type_node(annotation);
        let source = self.check_expression(initializer);
        let Some(initializer_id) = initializer.node_id() else { return };
        if !self.assignability_is_decidable(source, target)
            || self.source_is_an_unnarrowed_reference(initializer_id, source)
            || self.is_type_assignable_to(source, target)
        {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.nodes.span(node);
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
        if ambient || self.file_has_parse_errors {
            return;
        }
        let Some(Node::ReturnStatement(statement)) = self.node_map.get(node) else { return };
        let Some(expression) = statement.expression else { return };
        let Some(annotation) = self.enclosing_return_annotation(node) else { return };
        let target = self.get_type_from_type_node(annotation);
        let source = self.check_expression(expression);
        let Some(expression_id) = expression.node_id() else { return };
        if !self.assignability_is_decidable(source, target)
            || self.source_is_an_unnarrowed_reference(expression_id, source)
            || self.is_type_assignable_to(source, target)
        {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.nodes.span(node);
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
