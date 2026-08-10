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

use tsr_binder::{SymbolFlags, SymbolId};

use crate::{
    checker::Checker,
    flags::TypeFlags,
    printing,
    signatures::Signature,
    types::{TypeData, TypeId},
};

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
///
/// # The `new ` of a construct signature member comes from here
///
/// `{ new (): T; }` was rendered until `bd tsr-jril` by
/// `get_type_from_type_literal` writing the literal `"new "` in front of this
/// function's result, because [`crate::signatures::Signature`] had no flag to
/// read. It has one now, and leaving both in place would be a double prefix
/// waiting for the day the caller's arm is reused. `abstract` is deliberately
/// **not** emitted: the grammar admits it on a constructor *type node* only, so
/// an abstract construct signature member is a state upstream cannot produce.
pub(crate) fn signature_member_text(checker: &Checker<'_, '_>, signature: &Signature) -> String {
    let mut out = match signature.kind {
        crate::signatures::SignatureKind::Call => String::new(),
        crate::signatures::SignatureKind::Construct
        | crate::signatures::SignatureKind::AbstractConstruct => "new ".to_string(),
    };
    if !signature.type_parameters.is_empty() {
        out.push('<');
        for (index, parameter) in signature.type_parameters.iter().enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            if parameter.is_const {
                out.push_str("const ");
            }
            out.push_str(&parameter.name);
            if let Some(constraint) = parameter.constraint {
                out.push_str(" extends ");
                match &parameter.written_constraint {
                    Some(written) => out.push_str(written),
                    None => out.push_str(&checker.type_to_string(constraint)),
                }
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
        // The node-reuse rule on `Parameter::written_text`: a `typeof a`
        // annotation prints as written, in this form exactly as in the
        // `FunctionTypeNode` form.
        match &parameter.written_text {
            Some(written) => out.push_str(written),
            None => out.push_str(&checker.type_to_string(parameter.r#type)),
        }
    }
    out.push_str("): ");
    // The member spelling of the same rule `Checker::signature_to_string`
    // carries: `serializeReturnTypeForSignature` consults the predicate before
    // the return type (`nodebuilderimpl.go:1748`), whichever signature-shaped
    // node the builder is filling. `interface I { m(): this is S[]; }` is the
    // form that needs it, and the corpus records it on lib's `every`.
    match (&signature.predicate, &signature.written_return) {
        (Some(predicate), _) => out.push_str(&checker.type_predicate_to_string(predicate)),
        (None, Some(written)) => out.push_str(written),
        (None, None) => out.push_str(&checker.type_to_string(signature.r#type)),
    }
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
        // §105 slice 2a (`checker-notes-narrow.md`): a literal in a const
        // context (`isConstContext`, `checker.go:13615`) answers readonly
        // regular members — gated to literals with NO single-quoted string
        // member VALUE, because the value-spelling carriage is unbuilt and a
        // wrong quote is worse than the gap.
        let const_context = node.node_id.is_some_and(|id| self.is_const_context(id));
        let mut members = Vec::with_capacity(node.properties.len());
        for property in node.properties {
            // `checker.go:13223` dispatches over three member kinds. Only two are
            // reachable here: a method needs `checkObjectLiteralMethod` and a
            // signature member this port cannot print, and a spread or accessor
            // is not in that list at all.
            let mut property_node_id: Option<tsr_ast::NodeId> = None;
            let (name_node, value) = match property {
                tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                    let Some(initializer) = assignment.initializer else { return error };
                    property_node_id = assignment.node_id;
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
                // `{ ...a }`. Ported from `Checker.getSpreadType`
                // (`checker.go:13387`, from `grep -n` on the declaration),
                // **object-typed sources only** — see
                // [`Checker::spread_members_of`] for what is deliberately
                // gapped and why.
                tsr_ast::ObjectLiteralElementLike::SpreadAssignment(spread) => {
                    let Some(expression) = spread.expression else { return error };
                    let source = self.check_expression(expression);
                    let Some(spread_members) = self.spread_members_of(source) else {
                        return error;
                    };
                    for member in spread_members {
                        // §105 slice 2a fired leg (B): `{ ...o } as const`
                        // marks the SPREAD-contributed members readonly too
                        // (constAssertions o5, 0:147-153); their types stay
                        // as the source held them.
                        let member = match member {
                            Member::Property { name, optional, readonly: _, printed }
                                if const_context =>
                            {
                                Member::Property { name, optional, readonly: true, printed }
                            }
                            other => other,
                        };
                        upsert_member(&mut members, member);
                    }
                    continue;
                }
                // `{ m() {} }`. Upstream types the member as a signature and
                // prints it `m(): void`, not `m: () => void` — the distinction
                // [`Member`]'s own doc calls out, and the reason `Signature`
                // exists as a variant rather than a property with a function
                // type.
                //
                // Rendered through the same `signature_member_text` the type
                // literal arm and `crate::symbols`' multi-signature arm use, so
                // the three spellings of a method cannot drift.
                //
                // A method whose signature this port cannot build — an
                // unannotated parameter needing a contextual type, a
                // destructuring parameter — gaps the **whole literal**, which
                // is the rule every other member arm here already follows.
                tsr_ast::ObjectLiteralElementLike::MethodDeclaration(method) => {
                    let Some(id) = method.node_id else { return error };
                    let Some(signature) = self.get_signature_from_declaration(id) else {
                        return error;
                    };
                    let tsr_ast::PropertyName::Identifier(name) = method.name else {
                        // A computed or string-literal method name needs the
                        // same quoting rules the property path has and is not
                        // measured; a gap is one line, a guess is a wrong one.
                        return error;
                    };
                    // `classifyPropertyName` (`nodebuilderimpl.go:2384`) opens
                    // with one special case and it is exactly this: a **method**
                    // named `new` prints as a string literal. The reason is
                    // round-tripping rather than escaping — `{ new<T>(x: T): C<T>; }`
                    // re-parses as a *construct signature*, a different type, so
                    // the quotes are load-bearing. It is upstream's only
                    // name-independent quoting rule, and it is a method-only
                    // rule: a *property* named `new` stays bare.
                    //
                    // Found by `bd tsr-tgov`'s residual — the arm made 40
                    // `objectTypesIdentityWithGenericConstructSignatures*` lines
                    // computable and they printed unquoted. Exposed, not minted.
                    // §105 slice 2a fired leg (C): in a const context a
                    // method member prints as a readonly PROPERTY with the
                    // arrow form — `{ d() {} } as const` is
                    // `{ readonly d: () => void; }` (constAssertions o2/o8,
                    // 0:161-163), never the `d(): void` method spelling.
                    if const_context {
                        let arrow = self.signature_to_string(&signature);
                        upsert_member(
                            &mut members,
                            Member::Property {
                                name: name.text.to_string(),
                                optional: false,
                                readonly: true,
                                printed: arrow,
                            },
                        );
                        continue;
                    }
                    let printed = if name.text == "new" {
                        format!("\"new\"{}", signature_member_text(self, &signature))
                    } else {
                        format!("{}{}", name.text, signature_member_text(self, &signature))
                    };
                    upsert_member(&mut members, Member::Signature { printed });
                    continue;
                }
                _ => return error,
            };
            let name = match name_node {
                tsr_ast::PropertyName::Identifier(name) => name.text.to_string(),
                // A string-named property prints its name **unquoted** when it
                // is a valid identifier and **re-quoted** otherwise, which is
                // upstream's `symbolToString` behaviour: the source spelling is
                // discarded either way, so `{ "a": 1 }` is `{ a: number; }` and
                // `{ "a-b": 1 }` is `{ "a-b": number; }`.
                tsr_ast::PropertyName::StringLiteral(literal)
                    if is_identifier_text(literal.text) =>
                {
                    literal.text.to_string()
                }
                // Re-quoted through `printing::quote`, the **same** function a
                // string literal *type* prints through, rather than a second
                // escape table here. That table is deliberately incomplete
                // (`bd tsr-4sc.1`): it emits an unhandled character raw so a
                // miss shows up as a baseline mismatch rather than as silent
                // corruption. A duplicate would have to be corrected in lockstep
                // when that lands, and nothing would fail if only one were — the
                // incompleteness that makes one copy safe is exactly what would
                // make a divergence between two copies invisible.
                // §77.3 (`checker-notes-narrow.md`): a SINGLE-quoted written
                // name keeps its quote — `{ '1.0': "" }` prints
                // `{ '1.0': string; }`
                // (`assignmentCompatWithObjectMembersStringNumericNames`).
                // Only the re-quoted arm: an identifier-valid name still
                // prints unquoted whichever quote wrote it.
                tsr_ast::PropertyName::StringLiteral(literal)
                    if literal.token_flags.contains(tsr_ast::TokenFlags::SINGLE_QUOTE) =>
                {
                    format!("'{}'", literal.text)
                }
                tsr_ast::PropertyName::StringLiteral(literal) => printing::quote(literal.text),
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
                // Const context first — upstream's own order in
                // `checkExpressionForMutableLocation`: `isConstContext` wins
                // before any contextual retention question is asked.
                PropertyValue::Initializer(initializer) if const_context => {
                    let checked = self.check_expression(initializer);
                    self.get_regular_type_of_literal_type(checked)
                }
                PropertyValue::Initializer(initializer) => {
                    // §56 (`checker-notes-narrow.md`): the PRINT road moves
                    // WITH the symbol road — a fresh literal under a
                    // unit-wanting annotation member retains its literal
                    // form here too, or the object prints one thing while
                    // the member carries another (the §56 bar's leg (b)).
                    let retained = property_node_id
                        .and_then(|id| self.annotation_member_context(id))
                        .and_then(|contextual| {
                            let checked = self.check_expression(initializer);
                            self.type_wants_literal(contextual, checked)
                                .then(|| self.get_regular_type_of_literal_type(checked))
                        });
                    match retained {
                        Some(t) => t,
                        None => self.check_expression_for_mutable_location(initializer),
                    }
                }
                PropertyValue::Shorthand(identifier) => self.check_expression_for_mutable_location(
                    tsr_ast::Expression::Identifier(identifier),
                ),
            };
            // §146 (`checker-notes-narrow.md`): a member whose error is
            // UPSTREAM'S OWN — a bare identifier `resolve_name(VALUE)` finds
            // NOWHERE, the TS2304 form, §144's establishment argument at
            // expression level — prints `any` and the literal PROCEEDS.
            // Every other error member keeps the whole-literal rule: a
            // partial object type is a wrong answer that looks right.
            let member_type = if member_type == error {
                let unresolved = match &value {
                    PropertyValue::Initializer(tsr_ast::Expression::Identifier(identifier))
                    | PropertyValue::Shorthand(identifier) => {
                        identifier.node_id.is_some_and(|id| {
                            self.binder
                                .resolve_name(
                                    self.nodes,
                                    self.node_map,
                                    id,
                                    identifier.text,
                                    SymbolFlags::VALUE,
                                )
                                .is_none()
                        })
                    }
                    PropertyValue::Initializer(_) => false,
                };
                if !unresolved {
                    return error;
                }
                self.intrinsics.any
            } else {
                member_type
            };
            // The declaration-level `getWidenedType` would turn this into `any`
            // and this port has no call site for it. See the module docs.
            if self.store.get(member_type).flags.intersects(TypeFlags::NULLABLE) {
                return error;
            }
            // **`upsert`, not `push`.** A later member of the same name
            // replaces an earlier one *in the earlier one's position*, which is
            // upstream's spread ordering: `{ ...{ a: 1, b: 2 }, a: "x" }` is
            // `{ a: string; b: number; }`, with `a` still first. Plain
            // literals go through the same call because `{ a: 1, ...o }` has to
            // let `o`'s `a` win, and a `push` here would print `a` twice.
            let printed = match (const_context, &value) {
                // SS109: the carried shape is DIRECTLY a single-quoted
                // string literal - it prints single-quoted inside the
                // object type while its standalone line stays double
                // (the SS77.3 name-quote precedent applied to values).
                // Indirect reaches keep the fresh render.
                (true, PropertyValue::Initializer(tsr_ast::Expression::StringLiteral(literal)))
                    if literal.token_flags.contains(tsr_ast::TokenFlags::SINGLE_QUOTE) =>
                {
                    format!("'{}'", literal.text)
                }
                _ => self.type_to_string(member_type),
            };
            upsert_member(
                &mut members,
                Member::Property { name, optional: false, readonly: const_context, printed },
            );
        }
        let printed = render_object_type(&members);
        // The binder gives an object literal its own `__object` symbol, whose
        // members table is where a property access on this type looks — the same
        // arrangement `get_type_from_type_literal` relies on for `__type`.
        let symbol = node.node_id.and_then(|id| self.binder.symbol_of(id));
        self.store.new_named(TypeFlags::OBJECT, printed, symbol)
    }

    /// The members a `{ ...source }` contributes, or `None` when this port
    /// cannot compute them — in which case the whole literal gaps.
    ///
    /// Ported from `Checker.getSpreadType` (`checker.go:13387`) reduced to the
    /// one branch this port can answer: a source that is a **named object type
    /// with a members table**. Everything else returns `None` and the literal
    /// answers `errorType`.
    ///
    /// # What is gapped, and why each is a gap rather than a guess
    ///
    /// - **A non-object source** — a primitive, a union, `any`, `errorType`.
    ///   Upstream distributes a spread over a union and drops primitives;
    ///   answering `{}` for `{ ...someUnion }` would be a confident wrong type.
    /// - **A member whose own type gaps**, and **a member whose type is
    ///   nullable**, which is the same `getWidenedType` limitation the plain
    ///   member path already gaps on (see the module docs). `a?: number` on the
    ///   source yields `number | undefined` and stops the literal.
    /// - **A method member.** A spread copies a *property*; upstream then
    ///   prints it as `m: () => void` where the source printed `m(): void`, and
    ///   this port has no measurement of which side it lands on. A gap here is
    ///   one line; a guess is a wrong line.
    /// - **A member with no declaration**, which would have no source position
    ///   and therefore no defined order.
    ///
    /// # Order comes from the declaration, never from the table
    ///
    /// `SymbolTable` is an `FxHashMap` (`crates/tsr-binder/src/symbol.rs:291`),
    /// so iterating it yields members in **hash order**. Printing from that
    /// would make the rendered type non-deterministic across runs — a defect
    /// that would show up as a flapping baseline and be blamed on anything but
    /// the map. Members are therefore sorted by their declaration's source
    /// position, which is the order upstream prints.
    fn spread_members_of(&mut self, source: TypeId) -> Option<Vec<Member>> {
        let error = self.intrinsics.error;
        if source == error {
            return None;
        }
        // An instantiated reference is deliberately a gap here, not a spread of
        // the target's members: those members' declared types are the
        // uninstantiated ones (`a: T`), and this walk reads them through
        // `get_type_of_symbol` directly rather than through the instantiating
        // seam in `crate::members`. Until it is routed through
        // `get_type_of_property_of_type`, spreading a `C<number>` would print
        // `T` where upstream prints `number` — a wrong line where today there
        // is a missing one. `bd tsr-4qx` flipped `create_type_reference` to
        // carry `Some(symbol)`, which is what made this reachable at all.
        if self.type_reference_targets.contains_key(&source) {
            return None;
        }
        let owner = match &self.store.get(source).data {
            TypeData::Named { members: Some(owner), .. } => *owner,
            _ => return None,
        };
        let mut entries: Vec<(u32, String, SymbolId)> = Vec::new();
        for (name, &member) in &self.binder.symbols().get(owner).members {
            let declaration = *self.binder.symbols().get(member).declarations.first()?;
            entries.push((self.nodes.span(declaration).start, (*name).to_string(), member));
        }
        entries.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));

        let mut spread = Vec::with_capacity(entries.len());
        for (_, name, member) in entries {
            let flags = self.binder.symbols().get(member).flags;
            if flags.intersects(SymbolFlags::METHOD) {
                return None;
            }
            let member_type = self.get_type_of_symbol(member);
            if member_type == error
                || self.store.get(member_type).flags.intersects(TypeFlags::NULLABLE)
            {
                return None;
            }
            spread.push(Member::Property {
                name,
                optional: flags.intersects(SymbolFlags::OPTIONAL),
                readonly: false,
                printed: self.type_to_string(member_type),
            });
        }
        Some(spread)
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

/// Add `member`, replacing an existing property of the same name **in place**.
///
/// Upstream's spread keeps the first occurrence's position and the last
/// occurrence's type: `{ ...{ a: 1, b: 2 }, a: "x" }` prints
/// `{ a: string; b: number; }`. A `push` would print `a` twice, which is not a
/// type upstream can produce.
///
/// Only `Property` members carry names, so only they can collide; a signature
/// or index member is always appended.
fn upsert_member(members: &mut Vec<Member>, member: Member) {
    if let Member::Property { name, .. } = &member
        && let Some(existing) = members.iter_mut().find(
            |held| matches!(held, Member::Property { name: held_name, .. } if held_name == name),
        )
    {
        *existing = member;
        return;
    }
    members.push(member);
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
