//! TS2452 — `An enum member cannot have a numeric name.`
//!
//! `computeEnumMemberValue` (`checker.go:23958`) tests the member's name in
//! three arms, of which this ports the two that produce TS2452: a bigint
//! literal name, and any name whose text is a **numeric literal name**.
//!
//! `isNumericLiteralName` (`utilities.go:898`) is one line under fourteen of
//! comment, and the comment is the specification: the test is
//! `ToString(ToNumber(name)) == name`, *not* "looks like a number". Indexing
//! with `"13e-1"` reaches the property `"13e-1"`; indexing with `13e-1` reaches
//! the property `"1.3"`. They are different properties, so `"13e-1"` is not a
//! numeric name and gets no error, while `0xF00D` — whose text the parser has
//! already normalised to `61453` — does.
//!
//! `Infinity` and `NaN` are excluded explicitly upstream: they are the one
//! place the round-trip holds for a name written as a plain identifier.
//!
//! `docs/architecture/checker-notes-diag2.md` §671.

use tsr_ast::{Node, NodeId};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

/// `ToString(ToNumber(name)) == name` (`utilities.go:915`).
///
/// `f64`'s `ToString` is not Rust's `{}`: JavaScript prints an integral value
/// without a fractional part and uses exponent form outside a fixed range.
/// Only the shapes a property name can actually take are handled — an integral
/// value and a finite fractional one — because anything the two disagree on
/// fails the round-trip either way and so is not a numeric name.
fn is_numeric_literal_name(name: &str) -> bool {
    let Ok(value) = name.parse::<f64>() else { return false };
    if !value.is_finite() {
        return false;
    }
    let printed = if (value - value.trunc()).abs() < f64::EPSILON && value.abs() < 1e21 {
        format!("{}", value.trunc())
            .strip_suffix(".0")
            .map_or_else(|| format!("{}", value.trunc()), str::to_string)
    } else {
        format!("{value}")
    };
    printed == name
}

impl Checker<'_, '_> {
    /// TS1066 — `In ambient enum declarations member initializer must be
    /// constant expression.`
    ///
    /// `computeEnumMemberValues` (`checker.go:24016`) switches on the constant
    /// evaluator's answer. This port's evaluator is deliberately **symbol-free**
    /// (§101), so its `None` covers two unrelated situations: *not a constant*,
    /// which upstream also rejects, and *needs the enum's symbols*, which
    /// upstream folds. Only the first may report, so the rule additionally
    /// requires that the initializer contain no identifier in **reference**
    /// position — `'foo'.length` qualifies, `a + 1` and `E1.y` do not and are
    /// declined under the silence policy.
    ///
    /// `docs/architecture/checker-notes-diag2.md` §819.
    fn check_ambient_enum_member_initializer(
        &mut self,
        node: NodeId,
        member: &tsr_ast::EnumMember<'_>,
    ) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(initializer) = member.initializer else { return };
        let Some(at) = initializer.node_id() else { return };
        let Some(parent) = self.nodes.parent(node) else { return };
        // The parent must be an **ambient, non-`const`** enum: `isConstEnum`
        // takes precedence at the same switch and owns its own message.
        let Some(Node::EnumDeclaration(declaration)) = self.node_map.get(parent) else { return };
        let is_const = declaration.modifiers.iter().any(
            |m| matches!(m, tsr_ast::ModifierLike::Token(t) if t.kind == tsr_ast::SyntaxKind::ConstKeyword),
        );
        let is_ambient = declaration
            .modifiers
            .iter()
            .any(|m| matches!(m, tsr_ast::ModifierLike::Token(t) if t.kind == tsr_ast::SyntaxKind::DeclareKeyword));
        if is_const || !is_ambient {
            return;
        }
        if crate::expressions::evaluate_constant_expression(&initializer).is_some() {
            return;
        }
        if self.subtree_has_reference_identifier(at, 0) {
            return;
        }
        if let Some(file) = self.source_file_of_for_diagnostics(at) {
            let span = self.error_span(at);
            self.report(
                file,
                Diagnostic::new(
                    &messages::IN_AMBIENT_ENUM_DECLARATIONS_MEMBER_INITIALIZER_MUST_BE_CONSTANT_EXPRESSION,
                    span,
                ),
            );
        }
    }

    /// Does this subtree mention an identifier in **reference** position — one
    /// that could resolve to an enum member and so make the symbol-free
    /// evaluator's `None` a shortfall rather than a verdict? A property
    /// **name** does not count. §819.
    fn subtree_has_reference_identifier(&self, root: NodeId, depth: u32) -> bool {
        if depth > 64 {
            return false;
        }
        if self.nodes.kind(root) == tsr_ast::SyntaxKind::Identifier {
            let is_property_name = self.nodes.parent(root).is_some_and(|parent| {
                matches!(
                    self.node_map.get(parent),
                    Some(Node::PropertyAccessExpression(access))
                        if access.name.and_then(|n| n.node_id()) == Some(root)
                )
            });
            if !is_property_name {
                return true;
            }
        }
        let mut children = Vec::new();
        if let Some(typed) = self.node_map.get(root) {
            tsr_ast::for_each_child_id(typed, |child| children.push(child));
        }
        children.into_iter().any(|child| self.subtree_has_reference_identifier(child, depth + 1))
    }

    /// The name check for one enum member. §671.
    pub(crate) fn check_enum_member_name(&mut self, node: NodeId) {
        let Some(Node::EnumMember(member)) = self.node_map.get(node) else { return };
        self.check_ambient_enum_member_initializer(node, member);
        let (at, numeric) = match member.name {
            tsr_ast::PropertyName::BigIntLiteral(literal) => (literal.node_id, true),
            // **A numeric literal's name is always a numeric name.** Upstream
            // applies the predicate to `GetTextOfPropertyName`, which for a
            // numeric literal is the *normalised* text — and a normalised text
            // is `ToString(value)` by construction, so the round-trip holds
            // identically. Asking it here of *our* text answered `0xF00D`
            // wrongly, because this port keeps a numeric literal's written
            // spelling so the printer can reproduce it (`printer_round_trip` is
            // 100% and reprints from `text`). The predicate is upstream's; the
            // normalisation it assumes is not this tree's, so the conclusion is
            // taken directly. §671.
            tsr_ast::PropertyName::NumericLiteral(literal) => (literal.node_id, true),
            tsr_ast::PropertyName::StringLiteral(literal) => {
                (literal.node_id, is_numeric_literal_name(literal.text))
            }
            tsr_ast::PropertyName::Identifier(name) => {
                (name.node_id, is_numeric_literal_name(name.text))
            }
            _ => return,
        };
        if !numeric {
            return;
        }
        let Some(at) = at else { return };
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.nodes.span(at);
        self.report(
            file,
            Diagnostic::new(&messages::AN_ENUM_MEMBER_CANNOT_HAVE_A_NUMERIC_NAME, span),
        );
    }
}
