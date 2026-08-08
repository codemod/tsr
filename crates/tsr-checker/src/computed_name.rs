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
        if ambient || self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::ComputedPropertyName(computed)) = self.node_map.get(node) else { return };
        let Some(expression) = computed.expression else { return };
        if self.is_invalid_computed_property_name(node, expression) {
            return;
        }
        let named = self.check_expression(expression);
        // `errorType` and `any` both satisfy the rule upstream — `any` by the
        // message's own wording, `errorType` because it carries `TypeFlagsAny`
        // (§43). `is_error` and not identity, for the same reason.
        if self.is_error(named) || self.type_of(named).flags.intersects(TypeFlags::ANY_OR_UNKNOWN) {
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
            if self.relate_ternary(named, allowed, Relation::Assignable) != Ternary::NotRelated {
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
}
