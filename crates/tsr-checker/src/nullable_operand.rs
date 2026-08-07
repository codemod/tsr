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
//! # `+` is excluded, and it is the only interesting decision here
//!
//! Every other arithmetic and bitwise operator takes numeric operands and
//! nothing else. `+` is overloaded with string concatenation, so its operand
//! check runs after `checkBinaryLikeExpression` has decided which overload
//! applies — a different arm with a different message set. Declining it costs
//! whatever `+` cases exist and cannot produce a wrong one.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, messages};

use crate::{checker::Checker, flags::TypeFlags};

impl Checker<'_, '_> {
    /// The nullable-operand check for one binary expression.
    pub(crate) fn check_nullable_operand(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors || !self.strict_null_checks {
            return;
        }
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(node) else { return };
        let Some(operator) = binary.operator_token else { return };
        if !is_numeric_operator(operator.kind) {
            return;
        }
        let (Some(left), Some(right)) = (binary.left, binary.right) else { return };
        for operand in [left, right] {
            let Some(id) = operand.node_id() else { continue };
            let ty = self.check_expression(operand);
            let flags = self.type_of(ty).flags;
            let printed = if flags.contains(TypeFlags::NULL) {
                "null"
            } else if flags.contains(TypeFlags::UNDEFINED) {
                "undefined"
            } else {
                continue;
            };
            let Some(file) = self.source_file_of_for_diagnostics(id) else { continue };
            let span = self.nodes.span(id);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::THE_VALUE_0_CANNOT_BE_USED_HERE,
                    span,
                    [printed.to_string()],
                ),
            );
        }
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
    )
}
