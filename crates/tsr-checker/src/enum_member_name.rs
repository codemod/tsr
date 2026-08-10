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
    /// The name check for one enum member. §671.
    pub(crate) fn check_enum_member_name(&mut self, node: NodeId) {
        let Some(Node::EnumMember(member)) = self.node_map.get(node) else { return };
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
