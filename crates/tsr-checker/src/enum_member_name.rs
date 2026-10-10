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

use tsr_binder::SymbolFlags;

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
    /// `computeConstantEnumMemberValue` (`checker.go:24016`) switches on the
    /// checker's evaluator's answer for the member, asked with the member as
    /// `location` ([`Checker::evaluate_constant`]); a `nil` value reports.
    ///
    /// §819/§821 of `docs/architecture/checker-notes-diag2.md` decided this
    /// with a symbol-free evaluator plus a reference-identifier/call guard;
    /// both are superseded by the symbol-aware port
    /// (`docs/parity/notes/r4-templates.md` §3).
    fn check_ambient_enum_member_initializer(
        &mut self,
        node: NodeId,
        member: &tsr_ast::EnumMember<'_>,
    ) {
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
        if !is_const && !is_ambient {
            return;
        }
        if self.evaluate_constant(at, node).is_some() {
            return;
        }
        // Upstream's arm order (`checker.go:24014`): `isConstEnum` first, so a
        // `const enum` in a `.d.ts` gets TS2474 and not TS1066.
        let message = if is_const {
            &messages::CONST_ENUM_MEMBER_INITIALIZERS_MUST_BE_CONSTANT_EXPRESSIONS
        } else {
            &messages::IN_AMBIENT_ENUM_DECLARATIONS_MEMBER_INITIALIZER_MUST_BE_CONSTANT_EXPRESSION
        };
        if let Some(file) = self.source_file_of_for_diagnostics(at) {
            let span = self.error_span(at);
            self.report(file, Diagnostic::new(message, span));
        }
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

    /// TS2432 — `In an enum with multiple declarations, only one declaration
    /// can omit an initializer for its first enum element.`
    ///
    /// `checkEnumDeclaration` (`checker.go:5079`-`:5117`). Self-contained: the
    /// symbol's declarations in source order, and the *second* one whose first
    /// member has no initializer is the error, reported on that member's
    /// **name**.
    ///
    /// Guarded once per symbol by [`Checker::enum_checked`], because upstream's
    /// loop runs over every declaration — without it each violation reports
    /// once per declaration, and under multiset comparison an N-fold right line
    /// fails a case exactly as a wrong one does.
    ///
    /// `docs/architecture/checker-notes-diag2.md` §953.
    pub(crate) fn check_enum_first_member_initializers(&mut self, node: NodeId) {
        let Some(name) = self.declaration_name_of(node) else { return };
        let Some(text) = self.identifier_text(name).map(str::to_string) else { return };
        let Some(symbol) =
            self.binder.resolve_name(self.nodes, self.node_map, name, &text, SymbolFlags::TYPE)
        else {
            return;
        };
        let symbol = self.binder.merged_symbol(symbol);
        if !self.enum_checked.insert(symbol) {
            return;
        }
        let declarations = self.binder.symbols().get(symbol).declarations.clone();
        let mut seen_missing_initial_initializer = false;
        for declaration in declarations {
            // **`continue`, not reset.** A merged `namespace E {}` between two
            // enums does not clear the flag, and neither does an empty enum.
            let Some(Node::EnumDeclaration(enum_declaration)) = self.node_map.get(declaration)
            else {
                continue;
            };
            let Some(first) = enum_declaration.members.first() else { continue };
            if first.initializer.is_some() {
                continue;
            }
            if !seen_missing_initial_initializer {
                seen_missing_initial_initializer = true;
                continue;
            }
            let Some(member) = first.node_id else { continue };
            let Some(member_name) = self.declaration_name_of(member) else { continue };
            let Some(file) = self.source_file_of_for_diagnostics(member_name) else { continue };
            let span = self.error_span(member_name);
            self.report(
                file,
                Diagnostic::new(
                    &messages::IN_AN_ENUM_WITH_MULTIPLE_DECLARATIONS_ONLY_ONE_DECLARATION_CAN_OMIT_AN_INITIALIZER_FOR_ITS_FIRST_ENUM_ELEMENT,
                    span,
                ),
            );
        }
    }
}
