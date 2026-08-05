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

use crate::{checker::Checker, flags::TypeFlags, printing, signatures::Signature, types::TypeId};

/// One rendered member of a structural object type.
///
/// **Three shapes, not one with optional fields.** A property has a name and a
/// type printed as `name: T`; a call signature has **no name at all** and a
/// method prints `m(): void` rather than `m: () => void`; an index signature has
/// a bracketed *parameter* and a key type. Those are different spellings of
/// different things, and modelling the second as a property with a blank name
/// would put the difference in the renderer instead of in the data.
pub(crate) enum Member {
    /// `a: string`, `readonly a?: string`.
    Property {
        /// The property name as written.
        name: String,
        /// Whether the property carries `?`.
        optional: bool,
        /// Whether the property carries `readonly`.
        readonly: bool,
        /// The member type's printed form.
        printed: String,
    },
    /// A method, call or construct signature, printed whole: `m(): void`,
    /// `(x: number): string`, `new (): C`.
    ///
    /// One field because the three differ only in what precedes the parameter
    /// list, and that prefix is decided where the member is read rather than
    /// where it is rendered.
    Signature {
        /// The entire member text, without its trailing `;`.
        printed: String,
    },
    /// `[k: string]: number`, `readonly [k: number]: T`.
    ///
    /// A third shape rather than a `Property` with a decorated name: it has no
    /// property name at all — the identifier inside the brackets is a
    /// *parameter* name, which is why upstream keeps it on the declaration and
    /// not on the `IndexInfo` — and it carries a key type where a property
    /// carries nothing.
    Index {
        /// Whether the signature carries `readonly`.
        readonly: bool,
        /// The bracketed parameter name **as written**: upstream prints
        /// `[key: string]` for `[key: string]` and `[x: string]` for
        /// `[x: string]`, so this is not normalisable to one spelling.
        name: String,
        /// The key type's printed form — `string` or `number`.
        key: String,
        /// The value type's printed form.
        value: String,
    },
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
///
/// # Order is the caller's, not this function's
///
/// `members` is emitted verbatim. Upstream does **not** print in source order:
/// `createTypeNodesFromResolvedType` (`nodebuilderimpl.go:2627`) emits call
/// signatures, then construct signatures, then index infos, then properties, so
/// `{ a: string; b: string, [key: string]: string }` records
/// `{ [key: string]: string; a: string; b: string; }`
/// (`baselines/reference/submodule/conformance/noUncheckedIndexedAccess.types:377`).
/// That grouping is applied where the member *kind* is known — see
/// [`Checker::get_type_from_type_literal`] — because a method is a `Signature`
/// here but a *property* upstream, so the grouping cannot be recovered from this
/// enum alone.
pub(crate) fn render_object_type(members: &[Member]) -> String {
    if members.is_empty() {
        return "{}".to_string();
    }
    let mut printed = String::from("{ ");
    for member in members {
        match member {
            Member::Property { name, optional, readonly, printed: ty } => {
                if *readonly {
                    printed.push_str("readonly ");
                }
                printed.push_str(name);
                printed.push_str(if *optional { "?: " } else { ": " });
                printed.push_str(ty);
            }
            // A signature member is already whole: no name, no `: ` separator.
            Member::Signature { printed: text } => printed.push_str(text),
            Member::Index { readonly, name, key, value } => {
                if *readonly {
                    printed.push_str("readonly ");
                }
                printed.push('[');
                printed.push_str(name);
                printed.push_str(": ");
                printed.push_str(key);
                printed.push_str("]: ");
                printed.push_str(value);
            }
        }
        printed.push_str("; ");
    }
    printed.push('}');
    printed
}

/// A signature rendered as an object-type **member**: `(x: number): string`.
///
/// Ported from the member half of upstream's node builder
/// (`nodebuilderimpl.go:1792`): a signature in a type literal is emitted as a
/// method or call signature member, whose return type follows a **colon**,
/// where a standalone function type emits a function type node and an
/// **arrow**. Same signature, two spellings, chosen by position:
///
/// ```text
/// { m(): void; }          member  — `): `
/// { m: () => void; }      property holding a function type — `) => `
/// ```
///
/// So this deliberately does **not** reuse `signature_to_string`, which renders
/// the arrow form. Turning one into the other by string surgery would have to
/// find the top-level `) => ` and a parameter type can contain one.
pub(crate) fn signature_member_text(checker: &Checker<'_, '_>, signature: &Signature) -> String {
    let mut out = String::new();
    if !signature.type_parameters.is_empty() {
        out.push('<');
        for (index, parameter) in signature.type_parameters.iter().enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            out.push_str(&parameter.name);
            if let Some(constraint) = parameter.constraint {
                out.push_str(" extends ");
                out.push_str(&checker.type_to_string(constraint));
            }
            if let Some(default) = parameter.default {
                out.push_str(" = ");
                out.push_str(&checker.type_to_string(default));
            }
        }
        out.push('>');
    }
    out.push('(');
    for (index, parameter) in
        signature.this_parameter.iter().chain(signature.parameters.iter()).enumerate()
    {
        if index > 0 {
            out.push_str(", ");
        }
        if parameter.rest {
            out.push_str("...");
        }
        out.push_str(&parameter.name);
        out.push_str(if parameter.optional { "?: " } else { ": " });
        out.push_str(&checker.type_to_string(parameter.r#type));
    }
    out.push_str("): ");
    out.push_str(&checker.type_to_string(signature.r#type));
    out
}

/// Where an object-literal member's type comes from.
///
/// Two shapes because `checkObjectLiteral` dispatches on the member kind
/// (`checker.go:13223`) before it has a type: a property assignment types its
/// initialiser, a shorthand types its own **name as an expression**.
enum PropertyValue<'a> {
    /// `{ a: expr }` — the initialiser.
    Initializer(tsr_ast::Expression<'a>),
    /// `{ a }` — the name, used as an identifier expression.
    Shorthand(&'a tsr_ast::Identifier<'a>),
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
            // `checker.go:13223` dispatches over three member kinds. Only two are
            // reachable here: a method needs `checkObjectLiteralMethod` and a
            // signature member this port cannot print, and a spread or accessor
            // is not in that list at all.
            let (name_node, value) = match property {
                tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                    let Some(initializer) = assignment.initializer else { return error };
                    (assignment.name, PropertyValue::Initializer(initializer))
                }
                // `{ a }`. `checkShorthandPropertyAssignment` (`checker.go:13689`)
                // runs `checkExpressionForMutableLocation` on the **name used as
                // an expression**, so `{ a }` and `{ a: a }` are the same type by
                // construction rather than by coincidence.
                tsr_ast::ObjectLiteralElementLike::ShorthandPropertyAssignment(shorthand) => {
                    // `{ a = 1 }` carries an `ObjectAssignmentInitializer`, which
                    // the grammar only permits inside a destructuring pattern.
                    // `checkShorthandPropertyAssignment` types it through
                    // `checkBinaryLikeExpression` (`checker.go:12562`), a
                    // different path.
                    //
                    // **Unobservable today and load-bearing under one named
                    // edit**, the same status as the spread guard in
                    // [`crate::array_literals`]. The only way to write this form
                    // is as an assignment target, and `check_binary_expression`
                    // gaps the whole assignment before this literal is ever
                    // checked — verified by making this arm answer `never` and
                    // watching the result stay `error`. It becomes live the
                    // moment destructuring assignment is ported, at which point
                    // deleting it would answer `{ a: number; }` for a pattern
                    // rather than a type. Deliberately NOT covered by a test:
                    // the obvious fixture passes whether or not this line
                    // exists, which would be a decoration.
                    if shorthand.object_assignment_initializer.is_some() {
                        return error;
                    }
                    let tsr_ast::PropertyName::Identifier(identifier) = shorthand.name else {
                        // The grammar gives a shorthand an identifier name; any
                        // other spelling is a parse error already reported.
                        return error;
                    };
                    (shorthand.name, PropertyValue::Shorthand(identifier))
                }
                _ => return error,
            };
            let name = match name_node {
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
                // A **numeric** name prints as its normalised value with no
                // quotes: `{ 0: number; }`, and `{ 1.0: x }` prints `1`. The
                // corpus is thick with these — 63 lines of `{ 0: number; }`
                // alone — because they are what an array-like object literal
                // looks like.
                //
                // `normalise_number` is the same function the numeric *literal
                // type* uses, which is what stops `{ 1e3: x }` printing `1e3`
                // here and `1000` there.
                tsr_ast::PropertyName::NumericLiteral(literal) => {
                    printing::normalise_number(literal.text)
                }
                _ => return error,
            };
            let member_type = match value {
                PropertyValue::Initializer(initializer) => {
                    self.check_expression_for_mutable_location(initializer)
                }
                PropertyValue::Shorthand(identifier) => self.check_expression_for_mutable_location(
                    tsr_ast::Expression::Identifier(identifier),
                ),
            };
            if member_type == error {
                return error;
            }
            // The declaration-level `getWidenedType` would turn this into `any`
            // and this port has no call site for it. See the module docs.
            if self.store.get(member_type).flags.intersects(TypeFlags::NULLABLE) {
                return error;
            }
            members.push(Member::Property {
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
