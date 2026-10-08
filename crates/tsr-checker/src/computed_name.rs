//! TS2464 — `A computed property name must be of type 'string', 'number',
//! 'symbol', or 'any'.`
//!
//! `checkComputedPropertyName` (`checker.go:26802`). The comment above
//! upstream's test is its specification: *"This will allow types number,
//! string, symbol or any. It will also allow enums, the unknown type, and any
//! union of these types (like `string | number`)."*
//!
//! The two disjuncts are not redundant. The first is a **flag** test over
//! `StringLike | NumberLike | ESSymbolLike`, which a union type cannot satisfy
//! because a union carries `UNION` and not its constituents' flags; the second
//! is assignability to `string | number | symbol`, which is what admits
//! `string | number`. §45's `either_is_composite` names the same blindness from
//! the other side — there it is a decline, here upstream repairs it with a
//! relation call.
//!
//! # Direction
//!
//! This rule reports on a **negative**, so §25's collapse applies: both tests
//! must be a *confident* negative before it fires. That is the opposite of §49's
//! and §50.2's readings of the same three-valued relation, which is why each is
//! written where it is used.
//!
//! `docs/architecture/checker-notes-diag2.md` §52.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, messages};

use crate::{
    checker::Checker,
    flags::TypeFlags,
    relater::{Relation, Ternary},
};

/// `StringLike | NumberLike | ESSymbolLike` (`checker.go:26814`).
const ALLOWED_KINDS: TypeFlags = TypeFlags::STRING_LIKE
    .union(TypeFlags::NUMBER_LIKE)
    .union(TypeFlags::ES_SYMBOL)
    .union(TypeFlags::UNIQUE_ES_SYMBOL);

impl Checker<'_, '_> {
    /// The type check for one `[…]` property name.
    pub(crate) fn check_computed_property_name(&mut self, node: NodeId, ambient: bool) {
        // §81's hazard: the walk-threaded `ambient` widens at `VariableStatement`
        // and `FunctionDeclaration` and nowhere else, so a **member's own**
        // `declare` is invisible to it. §134 hit this in the type-only alias
        // rule; this is the sweep for it. §135.
        if ambient || self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        if self.nodes.parent(node).is_some_and(|member| self.member_has_declare_modifier(member)) {
            return;
        }
        let Some(Node::ComputedPropertyName(computed)) = self.node_map.get(node) else { return };
        let Some(expression) = computed.expression else { return };
        if self.is_invalid_computed_property_name(node, expression) {
            return;
        }
        self.check_computed_name_type_parameter_reference(node);
        let named = self.check_expression(expression);
        // `errorType` and `any` both satisfy the rule upstream — `any` by the
        // message's own wording, `errorType` because it carries `TypeFlagsAny`
        // (§43). `is_error` and not identity, for the same reason.
        if self.is_type_any(named)
            || self.type_of(named).flags.intersects(TypeFlags::ANY_OR_UNKNOWN)
        {
            return;
        }
        // **An unconstrained type parameter cannot be a computed name**, and the
        // relation cannot say so — it has nothing to relate. The syntax can: a
        // `TypeParameterDeclaration` with no `constraint`. The fixture's own
        // control is `K extends keyof T`, which **is** the allowed union, so the
        // bound is *unconstrained* rather than *is a type parameter*. §456.
        if self.name_is_unconstrained_type_parameter(expression) {
            if let Some(file) = self.source_file_of_for_diagnostics(node) {
                let span = self.error_span(node);
                self.report(
                    file,
                    Diagnostic::new(
                        &messages::A_COMPUTED_PROPERTY_NAME_MUST_BE_OF_TYPE_STRING_NUMBER_SYMBOL_OR_ANY,
                        span,
                    ),
                );
            }
            return;
        }
        // **A written union with a constituent that can never be a name.** §52
        // built this row on the relation and §274 recorded its silence policy:
        // an undecidable relation reports nothing. `string | boolean` is
        // decidable and already fires; `number | number[]` is not, and the
        // written annotation settles it without a type — the name must be valid
        // for *every* constituent, which is §1002's reading of the same shape.
        //
        // Additive and exclusive: this runs before the relation guard and
        // returns, so the decidable path is untouched and the two cannot both
        // report the same line. §1003.
        if self.name_is_union_with_a_non_nameable_constituent(expression) {
            if let Some(file) = self.source_file_of_for_diagnostics(node) {
                let span = self.error_span(node);
                self.report(
                    file,
                    Diagnostic::new(
                        &messages::A_COMPUTED_PROPERTY_NAME_MUST_BE_OF_TYPE_STRING_NUMBER_SYMBOL_OR_ANY,
                        span,
                    ),
                );
            }
            return;
        }
        if !self.type_of(named).flags.intersects(TypeFlags::NULLABLE) {
            if self.type_of(named).flags.intersects(ALLOWED_KINDS) {
                return;
            }
            // The union repair: `string | number` is allowed and carries none
            // of the kind flags. Both this and the flag test must fail before
            // the rule fires, and an undecidable relation is silence.
            //
            // **Asked of the union itself, not constituent by constituent.**
            // The per-constituent form was measured — it is the *sound*
            // decomposition for a non-union source — and it added two wrong
            // lines and no right ones (`checker-notes-diag2.md` §52). The
            // relation already answers the union target for every pair this
            // rule can decide.
            let string = self.intrinsics.string;
            let number = self.intrinsics.number;
            let symbol = self.intrinsics.es_symbol;
            let allowed = self.get_union_type_unprinted(&[string, number, symbol]);
            // **An object type does not need the relation.** No object type is
            // assignable to a union of `string`, `number` and `symbol` — not
            // one with a call signature, not one with an index signature. `any`
            // and `unknown` were excluded two guards above, and enum and
            // literal types carry `STRING_LIKE`/`NUMBER_LIKE` and were allowed
            // by `ALLOWED_KINDS`.
            //
            // The guard below is right in general and is what costs this row:
            // `symbolProperty59`'s `[Symbol.keyFor]` types **identically to
            // upstream** here — `(sym: symbol) => string | undefined` — and the
            // relation cannot decide it, so the silence policy declines a case
            // whose answer is not in doubt. §274.
            if !self.type_of(named).flags.intersects(TypeFlags::OBJECT)
                && self.relate_ternary(named, allowed, Relation::Assignable) != Ternary::NotRelated
            {
                return;
            }
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(
            file,
            Diagnostic::new(
                &messages::A_COMPUTED_PROPERTY_NAME_MUST_BE_OF_TYPE_STRING_NUMBER_SYMBOL_OR_ANY,
                span,
            ),
        );
    }

    /// `isInvalidComputedPropertyName` (`checker.go:26796`): `[a in b]` in a
    /// type literal, class or interface that is not an accessor — a mapped-type
    /// head the parser recovered as a computed name. Upstream answers
    /// `errorType` and reports nothing.
    fn is_invalid_computed_property_name(
        &self,
        node: NodeId,
        expression: tsr_ast::Expression<'_>,
    ) -> bool {
        let Some(Node::BinaryExpression(binary)) =
            expression.node_id().and_then(|id| self.node_map.get(id))
        else {
            return false;
        };
        if binary.operator_token.is_none_or(|token| token.kind != SyntaxKind::InKeyword) {
            return false;
        }
        let Some(member) = self.nodes.parent(node) else { return false };
        if matches!(self.nodes.kind(member), SyntaxKind::GetAccessor | SyntaxKind::SetAccessor) {
            return false;
        }
        self.nodes.parent(member).is_some_and(|owner| {
            matches!(
                self.nodes.kind(owner),
                SyntaxKind::TypeLiteral
                    | SyntaxKind::ClassDeclaration
                    | SyntaxKind::ClassExpression
                    | SyntaxKind::InterfaceDeclaration
            )
        })
    }
    /// Is this name an identifier whose written annotation is a **union** with
    /// a constituent that can never be a property name? §1003.
    fn name_is_union_with_a_non_nameable_constituent(
        &mut self,
        expression: tsr_ast::Expression<'_>,
    ) -> bool {
        let Some(id) = expression.node_id() else { return false };
        let Some(text) = self.identifier_text(id).map(str::to_string) else { return false };
        let Some(symbol) = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            id,
            &text,
            tsr_binder::SymbolFlags::VALUE,
        ) else {
            return false;
        };
        let declarations =
            self.binder.symbols().get(self.binder.merged_symbol(symbol)).declarations.clone();
        let [declaration] = declarations.as_slice() else { return false };
        let Some(Node::VariableDeclaration(variable)) = self.node_map.get(*declaration) else {
            return false;
        };
        let Some(annotation) = variable.r#type.and_then(|t| t.node_id()) else { return false };
        let Some(Node::UnionTypeNode(union)) = self.node_map.get(annotation) else { return false };
        union.types.iter().any(|member| {
            member.node_id().is_some_and(|id| {
                matches!(
                    self.nodes.kind(id),
                    SyntaxKind::ArrayType | SyntaxKind::TupleType | SyntaxKind::TypeLiteral
                )
            })
        })
    }

    /// Is this computed name an identifier whose declared type is a type
    /// parameter with **no constraint**?
    ///
    /// Resolved syntactically — the variable's written annotation names a type
    /// parameter, and that declaration carries no `extends`. A constrained one
    /// declines whatever the constraint is, because `keyof T` is the corpus's
    /// counter-example. §456.
    fn name_is_unconstrained_type_parameter(
        &mut self,
        expression: tsr_ast::Expression<'_>,
    ) -> bool {
        let Some(id) = expression.node_id() else { return false };
        let Some(text) = self.identifier_text(id).map(str::to_string) else { return false };
        let Some(symbol) = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            id,
            &text,
            tsr_binder::SymbolFlags::VALUE,
        ) else {
            return false;
        };
        let declarations =
            self.binder.symbols().get(self.binder.merged_symbol(symbol)).declarations.clone();
        for declaration in declarations {
            let Some(Node::VariableDeclaration(variable)) = self.node_map.get(declaration) else {
                continue;
            };
            let Some(annotation) = variable.r#type.and_then(|t| t.node_id()) else { continue };
            let Some(Node::TypeReferenceNode(reference)) = self.node_map.get(annotation) else {
                continue;
            };
            let Some(name) = reference.type_name.and_then(|n| n.node_id()) else { continue };
            let Some(type_text) = self.identifier_text(name).map(str::to_string) else { continue };
            let Some(type_symbol) = self.binder.resolve_name(
                self.nodes,
                self.node_map,
                name,
                &type_text,
                tsr_binder::SymbolFlags::TYPE,
            ) else {
                continue;
            };
            let type_declarations = self
                .binder
                .symbols()
                .get(self.binder.merged_symbol(type_symbol))
                .declarations
                .clone();
            if type_declarations.iter().any(|&d| {
                matches!(
                    self.node_map.get(d),
                    Some(Node::TypeParameterDeclaration(parameter))
                        if parameter.constraint.is_none()
                )
            }) {
                return true;
            }
        }
        false
    }

    /// TS2467 — `A computed property name cannot reference a type parameter
    /// from its containing type.`
    ///
    /// The error is on the **type argument**, not the name: a computed name is
    /// evaluated once per declaration and the containing type's parameter has
    /// no value there, so it is the reference that is illegal. §1027.
    fn check_computed_name_type_parameter_reference(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(owner) = self.nodes.parent(node).and_then(|member| self.nodes.parent(member))
        else {
            return;
        };
        let parameters = match self.node_map.get(owner) {
            Some(Node::ClassDeclaration(class)) => class.type_parameters,
            Some(Node::ClassExpression(class)) => class.type_parameters,
            Some(Node::InterfaceDeclaration(interface)) => interface.type_parameters,
            _ => return,
        };
        if parameters.is_empty() {
            return;
        }
        let names: Vec<String> = parameters
            .iter()
            .filter_map(|p| p.name.and_then(|n| n.node_id))
            .filter_map(|id| self.identifier_text(id).map(str::to_string))
            .collect();
        let mut found = Vec::new();
        self.collect_type_reference_names(node, &names, &mut found);
        for at in found {
            let Some(file) = self.source_file_of_for_diagnostics(at) else { continue };
            let span = self.nodes.span(at);
            self.report(
                file,
                Diagnostic::new(
                    &messages::A_COMPUTED_PROPERTY_NAME_CANNOT_REFERENCE_A_TYPE_PARAMETER_FROM_ITS_CONTAINING_TYPE,
                    span,
                ),
            );
        }
    }

    /// Every `TypeReference` name in this subtree that spells one of `names`.
    ///
    /// A **name match**, not a resolution: a type parameter of an enclosing
    /// function that shadows the class's would be a false positive, and the
    /// corpus contains none — recorded rather than silently assumed. §1027.
    fn collect_type_reference_names(
        &self,
        root: NodeId,
        names: &[String],
        found: &mut Vec<NodeId>,
    ) {
        if self.nodes.kind(root) == SyntaxKind::TypeReference
            && let Some(Node::TypeReferenceNode(reference)) = self.node_map.get(root)
            && let Some(name) = reference.type_name.and_then(|n| n.node_id())
            && self.identifier_text(name).is_some_and(|text| names.iter().any(|n| n == text))
        {
            found.push(name);
        }
        let mut children = Vec::new();
        if let Some(typed) = self.node_map.get(root) {
            tsr_ast::for_each_child_id(typed, |child| children.push(child));
        }
        for child in children {
            self.collect_type_reference_names(child, names, found);
        }
    }
}
