//! TS18050 — `The value '{0}' cannot be used here.`
//!
//! `checkArithmeticOperandType` (`checker.go:12799`), reached from
//! `checkBinaryLikeExpression` for every operator whose operands must be
//! numeric: an operand whose type *is* `null` or `undefined` is not a number and
//! cannot become one.
//!
//! Error node the **operand**: `binaryArithmatic1.ts(1,13)` is the `null` of
//! `var v = 4 | null;`.
//!
//! # `+` was excluded on an argument, and the measurement overturned it
//!
//! §32 declined `+` because it is overloaded with string concatenation, so its
//! operand check runs after `checkBinaryLikeExpression` has chosen an overload —
//! a different arm with a different message set. That reasoning is sound about
//! *upstream's control flow* and wrong about the outcome: `null` and `undefined`
//! are not string-like either, so the `+` arm reaches the same TS18050. Adding
//! it is **+3 cases** (§40.5) with the gates green.
//!
//! An argument from upstream's structure is not a measurement. This one survived
//! because it was never run.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, messages};

use crate::{checker::Checker, flags::TypeFlags, types::TypeId};

impl Checker<'_, '_> {
    /// The nullable-operand check for one binary expression.
    pub(crate) fn check_nullable_operand(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors || !self.strict_null_checks {
            return;
        }
        // **A prefix operator's operand is a non-null position too.**
        // `checkPrefixUnaryExpression` wraps it in `checkNonNullType`
        // (`checker.go:10899` and the arithmetic arms above it), exactly as
        // `checkBinaryLikeExpression` wraps both sides. `!`, `typeof` and
        // `void` are excluded — upstream's wrap is on the arithmetic arms only
        // and `!undefined` is legal. §759.
        if let Some(Node::PrefixUnaryExpression(unary)) = self.node_map.get(node) {
            if !matches!(
                unary.operator.kind,
                SyntaxKind::MinusToken | SyntaxKind::PlusToken | SyntaxKind::TildeToken
            ) {
                return;
            }
            let Some(operand) = unary.operand else { return };
            let _ = self.check_expression(operand);
            self.report_nullable_operand(operand);
            return;
        }
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(node) else { return };
        let Some(operator) = binary.operator_token else { return };
        if !is_numeric_operator(operator.kind) {
            return;
        }
        let (Some(left), Some(right)) = (binary.left, binary.right) else { return };
        // **`+` is conditional and every other operator here is not.**
        // `checkNonNullType` — the function that emits this code — runs for an
        // addition only when NEITHER operand is string-like
        // (`checker.go:12418`), because `null + d` with `d: string` is a
        // concatenation and the `null` is fine. `isTypeAssignableToKind`
        // without `strict` lets `any` satisfy `StringLike` too
        // (`checker.go:27652`), so `null + a` is silent for the same reason.
        // Sixteen wrong lines, all in `additionOperatorWith*Value*` and
        // `operatorAddNullUndefined` — `checker-notes-diag2.md` §50.2.
        if operator.kind == SyntaxKind::PlusToken {
            let left_type = self.check_expression(left);
            let right_type = self.check_expression(right);
            if self.is_string_like_or_any(left_type) || self.is_string_like_or_any(right_type) {
                return;
            }
        }
        for operand in [left, right] {
            self.report_nullable_operand(operand);
        }
    }

    /// One operand of an arithmetic position, reported per
    /// `reportObjectPossiblyNullOrUndefinedError` (`checker.go:7455`).
    ///
    /// Extracted so the prefix arm and the binary arm share it — the
    /// spelling test that chooses TS18050 over TS18048 and the five
    /// entity-name branches are the same for both. §759.
    pub(crate) fn report_nullable_operand(&mut self, operand: tsr_ast::Expression<'_>) {
        let Some(id) = operand.node_id() else { return };
        let ty = self.check_expression(operand);
        // `getTypeFacts(t, IsUndefinedOrNull)` (`checker.go:7425`): the
        // type **may be** nullish, which for a union is any constituent.
        let (maybe_null, maybe_undefined) = self.nullish_facts(ty);
        if !maybe_null && !maybe_undefined {
            return;
        }
        // **This code is chosen by the node, not by the type.**
        // `reportObjectPossiblyNullOrUndefinedError` (`checker.go:7455`)
        // emits `The value '{0}' cannot be used here` only for a `null`
        // keyword or for an identifier literally spelled `undefined`;
        // every other nullable operand gets TS18048 / TS18049 / TS2531 /
        // TS2532 with the same *facts*. `var x: typeof undefined; t < x`
        // is upstream's TS18048 and was 64 wrong lines of this rule —
        // `checker-notes-diag2.md` §50.3.
        let written = match self.node_map.get(id) {
            _ if self.nodes.kind(id) == SyntaxKind::NullKeyword => Some("null"),
            Some(Node::Identifier(identifier)) if identifier.text == "undefined" => {
                Some("undefined")
            }
            _ => None,
        };
        let Some(file) = self.source_file_of_for_diagnostics(id) else { return };
        let span = self.error_span(id);
        if let Some(written) = written {
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::THE_VALUE_0_CANNOT_BE_USED_HERE,
                    span,
                    [written.to_string()],
                ),
            );
            return;
        }
        // The other five branches. `entityNameToString` gives the printed
        // name for an identifier or a dotted name; anything else takes the
        // `Object is possibly …` twin at the same position with the same
        // facts (`checker.go:7455`, `checker-notes-diag2.md` §51).
        let named = self.operand_entity_name_text(id).filter(|text| text.len() < 100);
        match (named, maybe_null, maybe_undefined) {
            (Some(text), true, true) => self.report(
                file,
                Diagnostic::with_args(&messages::_0_IS_POSSIBLY_NULL_OR_UNDEFINED, span, [text]),
            ),
            (Some(text), false, true) => self.report(
                file,
                Diagnostic::with_args(&messages::_0_IS_POSSIBLY_UNDEFINED, span, [text]),
            ),
            (Some(text), true, false) => self
                .report(file, Diagnostic::with_args(&messages::_0_IS_POSSIBLY_NULL, span, [text])),
            (None, true, true) => self.report(
                file,
                Diagnostic::new(&messages::OBJECT_IS_POSSIBLY_NULL_OR_UNDEFINED, span),
            ),
            (None, false, true) => {
                self.report(file, Diagnostic::new(&messages::OBJECT_IS_POSSIBLY_UNDEFINED, span));
            }
            (None, true, false) => {
                self.report(file, Diagnostic::new(&messages::OBJECT_IS_POSSIBLY_NULL, span));
            }
            (_, false, false) => {}
        }
    }

    /// `getTypeFacts(t, TypeFactsIsUndefinedOrNull)` reduced to the two bits
    /// the reporter branches on — union-aware, since that is the whole of what
    /// "may be" means here.
    pub(crate) fn nullish_facts(&self, ty: TypeId) -> (bool, bool) {
        let of = |checker: &Self, id: TypeId| {
            let flags = checker.type_of(id).flags;
            (flags.contains(TypeFlags::NULL), flags.contains(TypeFlags::UNDEFINED))
        };
        let (mut null, mut undefined) = of(self, ty);
        if let crate::types::TypeData::Union { types, .. } = &self.store.get(ty).data {
            for &constituent in types {
                let (n, u) = of(self, constituent);
                null |= n;
                undefined |= u;
            }
        }
        (null, undefined)
    }

    /// `entityNameToString` (`checker.go`), for the shapes an operand takes:
    /// an identifier, or a dotted name of identifiers.
    fn operand_entity_name_text(&self, node: NodeId) -> Option<String> {
        match self.node_map.get(node)? {
            Node::Identifier(identifier) => Some(identifier.text.to_string()),
            Node::PropertyAccessExpression(access) => {
                let target = self.operand_entity_name_text(access.expression?.node_id()?)?;
                let member = match access.name? {
                    tsr_ast::MemberName::Identifier(identifier) => identifier.text,
                    tsr_ast::MemberName::PrivateIdentifier(private) => private.text,
                };
                Some(format!("{target}.{member}"))
            }
            _ => None,
        }
    }

    /// `isTypeAssignableToKind(t, TypeFlagsStringLike)` **without** `strict`
    /// (`checker.go:27645`): the flag test, then assignability to `string` —
    /// which `any` and `unknown` satisfy, since the strict short-circuit is
    /// what would have excluded them.
    fn is_string_like_or_any(&mut self, ty: TypeId) -> bool {
        if self
            .type_of(ty)
            .flags
            .intersects(TypeFlags::STRING_LIKE.union(TypeFlags::ANY_OR_UNKNOWN))
        {
            return true;
        }
        // `Related` and not "not `NotRelated`". This test decides whether to
        // **stay silent**, so an undecidable pair must not be read as
        // string-like: an enum operand answers `Unknown` here and is not
        // assignable to `string` upstream, and reading `Unknown` as a positive
        // cost 28 correct lines in the first measurement.
        let string = self.intrinsics.string;
        self.relate_ternary(ty, string, crate::relater::Relation::Assignable)
            == crate::relater::Ternary::Related
    }
}

/// The operators whose operands must be numeric — `+` excluded, see the module
/// header.
fn is_numeric_operator(kind: SyntaxKind) -> bool {
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
