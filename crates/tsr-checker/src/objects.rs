//! Object literal **expressions**, and the structural form both object types print.
//!
//! Ported from `Checker.checkObjectLiteral` (`checker.go:13144`) and
//! `checkExpressionForMutableLocation` (`checker.go:13878`).
//!
//! # Why this is a separate module from [`crate::declared`]
//!
//! `{ a: string }` written as a *type* and `{ a: 1 }` written as an *expression*
//! produce types that print identically and are computed by unrelated functions.
//! `docs/architecture/checker.md` recorded that as the reason the object-literal
//! answer bucket barely moved when the type node was ported: "a bucket names the
//! answer's shape and not the feature that computes it". The two share
//! [`render_object_type`] and nothing else.
//!
//! # The member types are widened *here*, not at the declaration
//!
//! This is the trap in an object-literal port, and the baselines are unambiguous
//! about which of the two widenings is which
//! (`baselines/reference/submodule/compiler/widenedTypes1.types:11`):
//!
//! ```text
//! var c = {x: null};
//! >c : { x: any; }            ← getWidenedType, at the declaration
//! >{x: null} : { x: null; }   ← checkObjectLiteral, here
//! >x : null
//! ```
//!
//! So a member's *literal* type is widened as the literal is checked —
//! `checkExpressionForMutableLocation` calls `getWidenedLiteralLikeTypeForContextualType`
//! (`checker.go:13885`), which is why `const obj = { a: 1 }` records
//! `>obj : { a: number; }` and **not** `{ a: 1; }` even though a bare
//! `const n = 1` stays `1`. Freshness stops at the property boundary.
//!
//! A member's *nullable* type is widened much later, by `getWidenedType`
//! (`checker.go:18355`) at the declaration. That call site is in
//! [`crate::symbols`], which this workstream does not own, so an object literal
//! with a nullable member is a **gap** here rather than a line that would be
//! right as an expression and wrong as a declaration. `bd tsr-mli`.

use tsr_ast::ObjectLiteralExpression;

use crate::{checker::Checker, flags::TypeFlags, types::TypeId};

/// One rendered member of a structural object type.
pub(crate) struct Member {
    /// The property name as written.
    pub name: String,
    /// Whether the property carries `?`.
    pub optional: bool,
    /// Whether the property carries `readonly`.
    pub readonly: bool,
    /// The member type's printed form.
    pub printed: String,
}

/// The structural form upstream's printer emits for an anonymous object type.
///
/// `{ a: string; }` — one space inside each brace, `; ` after every member
/// including the last, and `{}` when there are none. Not a style choice: it is
/// compared character for character against 26,686 corpus lines.
///
/// Shared by the type-node path ([`Checker::get_type_from_type_literal`]) and the
/// expression path ([`Checker::check_object_literal`]) so the two cannot drift.
/// They previously had one renderer each in draft, which is exactly how a port
/// ends up with `{ a: string }` in one position and `{ a: string; }` in another.
pub(crate) fn render_object_type(members: &[Member]) -> String {
    if members.is_empty() {
        return "{}".to_string();
    }
    let mut printed = String::from("{ ");
    for member in members {
        if member.readonly {
            printed.push_str("readonly ");
        }
        printed.push_str(&member.name);
        printed.push_str(if member.optional { "?: " } else { ": " });
        printed.push_str(&member.printed);
        printed.push_str("; ");
    }
    printed.push('}');
    printed
}

impl Checker<'_, '_> {
    /// Ported from `Checker.checkObjectLiteral` (`checker.go:13144`).
    ///
    /// # What this slice covers
    ///
    /// Property assignments with an identifier or string name, whose initialiser
    /// this port can type. Everything else is a gap, and each for a reason rather
    /// than for want of code:
    ///
    /// - **Methods, accessors and shorthand properties.** A method needs
    ///   `checkObjectLiteralMethod` and a signature; shorthand needs the
    ///   identifier resolved as a *value* and then the same widening. Both are
    ///   ordinary follow-ups (`bd tsr-mli`).
    /// - **Spread properties.** Upstream folds them with `getSpreadType`
    ///   (`checker.go:13290`), which merges two symbol tables and has its own
    ///   rules for optionality and index signatures.
    /// - **Computed names.** These become index signatures rather than
    ///   properties, and this port has no index signatures.
    /// - **A member whose type is a gap**, on the rule already established for
    ///   the type-node path: a partial object type is a wrong answer that looks
    ///   like a right one.
    /// - **A member whose type is nullable**, because the declaration's type
    ///   would then need `getWidenedType` — see the module docs.
    ///
    /// # No contextual type, and therefore no const context
    ///
    /// `checkExpressionForMutableLocation` picks between three behaviours by
    /// asking `isConstContext` and then the contextual type
    /// (`checker.go:13878`). Both are unported: `as const` is a type assertion,
    /// and contextual typing is the machinery that makes an object literal's
    /// members take their shape from what they are assigned to. Only the third
    /// branch — widen the literal, take its regular form — is reachable, so the
    /// other two are *not written* rather than written and left dead. They become
    /// live with contextual typing, and `isConstContext` is what will need
    /// porting first because it recurses through enclosing literals.
    pub(crate) fn check_object_literal(&mut self, node: &ObjectLiteralExpression<'_>) -> TypeId {
        let error = self.intrinsics.error;
        let mut members = Vec::with_capacity(node.properties.len());
        for property in node.properties {
            let tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) = property else {
                return error;
            };
            let name = match assignment.name {
                tsr_ast::PropertyName::Identifier(name) => name.text.to_string(),
                // A string-named property prints its name *unquoted* when it is
                // a valid identifier and quoted otherwise; only the first is
                // ported, because the second needs upstream's `isIdentifierText`
                // over the full Unicode identifier tables.
                tsr_ast::PropertyName::StringLiteral(literal)
                    if is_identifier_text(literal.text) =>
                {
                    literal.text.to_string()
                }
                _ => return error,
            };
            let Some(initializer) = assignment.initializer else { return error };
            let member_type = self.check_expression_for_mutable_location(initializer);
            if member_type == error {
                return error;
            }
            // The declaration-level `getWidenedType` would turn this into `any`
            // and this port has no call site for it. See the module docs.
            if self.store.get(member_type).flags.intersects(TypeFlags::NULLABLE) {
                return error;
            }
            members.push(Member {
                name,
                optional: false,
                readonly: false,
                printed: self.type_to_string(member_type),
            });
        }
        let printed = render_object_type(&members);
        // The binder gives an object literal its own `__object` symbol, whose
        // members table is where a property access on this type looks — the same
        // arrangement `get_type_from_type_literal` relies on for `__type`.
        let symbol = node.node_id.and_then(|id| self.binder.symbol_of(id));
        self.store.new_named(TypeFlags::OBJECT, printed, symbol)
    }

    /// Ported from `Checker.checkExpressionForMutableLocation`
    /// (`checker.go:13878`), third branch only — see
    /// [`Checker::check_object_literal`] for why the other two are not written.
    ///
    /// `getWidenedLiteralLikeTypeForContextualType(t, nil)` reduces to
    /// `getRegularTypeOfLiteralType(getWidenedLiteralType(t))`
    /// (`checker.go:25515`) when there is no contextual type, because
    /// `isLiteralOfContextualType` is false for a nil contextual type.
    ///
    /// **This is the property boundary freshness stops at.** `const n = 1` is
    /// `1`, and `const o = { a: 1 }` is `{ a: number; }`.
    pub(crate) fn check_expression_for_mutable_location(
        &mut self,
        expression: tsr_ast::Expression<'_>,
    ) -> TypeId {
        let id = self.check_expression(expression);
        let widened = self.get_widened_literal_type(id);
        self.get_regular_type_of_literal_type(widened)
    }
}

/// Whether a property name can be printed without quotes.
///
/// Upstream asks `scanner.IsIdentifierText(name, LanguageVariantStandard)`
/// before emitting a bare name (`nodebuilderimpl.go:3269`). This covers the
/// ASCII identifier subset only; a name outside it is a gap rather than a guess,
/// because printing `{ "a-b": string; }` as `{ a-b: string; }` fails the line.
fn is_identifier_text(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else { return false };
    (first.is_ascii_alphabetic() || first == '_' || first == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}
