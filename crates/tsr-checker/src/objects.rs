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

/// A property-only anonymous object's member types, retained for
/// `instantiateAnonymousType` and `instantiateSymbol` (checker.go).
#[derive(Clone)]
pub(crate) struct AnonymousProperty {
    pub(crate) name: String,
    pub(crate) printed_name: String,
    pub(crate) printed_type: String,
    pub(crate) optional: bool,
    pub(crate) readonly: bool,
    pub(crate) r#type: TypeId,
}

/// One rendered member of a structural object type.
///
/// **Three shapes, not one with optional fields.** A property has a name and a
/// type printed as `name: T`; a call signature has **no name at all** and a
/// method prints `m(): void` rather than `m: () => void`; an index signature has
/// a bracketed *parameter* and a key type. Those are different spellings of
/// different things, and modelling the second as a property with a blank name
/// would put the difference in the renderer instead of in the data.
#[derive(Clone)]
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

/// SS307: what a computed member name contributes to an object literal.
pub(crate) enum ComputedNameKey {
    /// The name is LATE-BOUND (a string/number literal, a unique symbol, or a
    /// union of them names a real member, unported) or unreadable - the
    /// caller gaps the literal.
    LateBound,
    /// The name keys nothing (SS201) - the member contributes nothing, and
    /// the caller skips it.
    Nothing,
    /// The member contributes an index signature of this key kind.
    Index(&'static str),
}

impl Checker<'_, '_> {
    /// getRegularTypeOfObjectLiteral (checker.go:28159). Preserve the source
    /// expression's identity and member metadata; the clone alone is regular.
    pub(crate) fn get_regular_type_of_object_literal(&mut self, id: TypeId) -> TypeId {
        if !self.fresh_object_literal_types.contains(&id) {
            return id;
        }
        if let Some(&regular) = self.regular_object_literal_types.get(&id) {
            return regular;
        }
        let TypeData::Named { text, members } = self.store.get(id).data.clone() else {
            return id;
        };
        let regular = self.store.new_named(self.store.get(id).flags, text, members);
        self.regular_object_literal_types.insert(id, regular);
        if let Some(&spread) = self.object_literal_spread_flags.get(&id) {
            self.object_literal_spread_flags.insert(regular, spread);
        }
        if let Some(members) = self.object_literal_members.get(&id).cloned() {
            self.object_literal_members.insert(regular, members);
        }
        let mut properties =
            self.anonymous_properties.get(&id).map(|(properties, _)| properties.clone());
        if properties.is_none() {
            // Literals with methods/accessors do not have a captured property
            // list. transformTypeOfMembers reads their resolved symbol types.
            properties = self
                .property_names_of(id)
                .into_iter()
                .map(|name| {
                    let ty = self.get_type_of_property_of_type(id, &name)?;
                    let symbol = self.get_property_of_type(id, &name);
                    Some(AnonymousProperty {
                        printed_name: name.clone(),
                        printed_type: self.type_to_string(ty),
                        name,
                        r#type: ty,
                        optional: symbol.is_some_and(|symbol| self.property_is_optional(symbol)),
                        readonly: symbol.is_some_and(|symbol| self.is_readonly_property(symbol)),
                    })
                })
                .collect();
        }
        if let Some(mut properties) = properties {
            for property in &mut properties {
                property.r#type = self.get_regular_type_of_object_literal(property.r#type);
            }
            self.anonymous_properties.insert(regular, (properties, true));
        }
        if let Some(infos) = self.object_literal_index_infos.get(&id).cloned() {
            self.object_literal_index_infos.insert(regular, infos);
        }
        if let Some(signatures) = self.signature_types.get(&id).cloned() {
            self.signature_types.insert(regular, signatures);
        }
        if self.js_literal_types.contains(&id) {
            self.js_literal_types.insert(regular);
        }
        regular
    }

    /// getWidenedTypeWithContext (checker.go:18359), including sibling and
    /// nested-property normalization for object literals.
    pub(crate) fn widen_object_literal_freshness(&mut self, id: TypeId) -> TypeId {
        self.widen_type_with_context(id, None, &mut Vec::new())
    }

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
    /// - **Computed names.** ~~These become index signatures rather than
    ///   properties, and this port has no index signatures.~~ **Both halves
    ///   of that sentence are false** (checker-1's §203 heuristic: a refusal
    ///   naming a prerequisite SUBSYSTEM is the easiest kind to write
    ///   carelessly, because it sounds like architecture). This port has
    ///   `index_signatures.rs`, 339 lines of it; and upstream does not turn
    ///   a computed name into an index signature unconditionally —
    ///   `checkObjectLiteral` (`checker.go:13317-13332`) puts the member in
    ///   `propertiesTable` when its name type carries
    ///   `StringOrNumberLiteralOrUnique`, sets an index-signature flag when
    ///   it does not, and DROPS the member either way rather than gapping
    ///   the literal. checker-1's §201 already corrected the behaviour; this
    ///   corrects the stated reason, which would otherwise have sent the
    ///   next reader to build a subsystem that exists.
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
    /// SS323: the printed NAME of a late-bound member whose computed name is
    /// a UNIQUE SYMBOL reference - `{ [s]: 0 }` prints `{ [s]: number; }` and
    /// `{ [Symbol.isConcatSpreadable]: 0 }` prints the dotted chain in
    /// brackets (`conformance/symbolProperty1`, `symbolDeclarationEmit7-9`).
    /// Upstream renders the entity name the source wrote
    /// (the node builder's late-bound leg); an expression that is not an
    /// identifier chain, or whose type is any other late-bound kind (a
    /// string/number literal spells WITHOUT brackets - `{ [1]: 1 }` is
    /// `{ 1: number; }`), answers `None` and the caller keeps its gap.
    /// The `(name, unique)` pair: a UNIQUE-symbol name is a real late-bound
    /// member (method spelling, readonly getters, get/set merge - SS323/325);
    /// a PLAIN-symbol entity name is an index-info COMPONENT row, displayed
    /// as a property whatever the declaration kind - `{ [s]: () => void }`
    /// for a method, no readonly, no merge (`symbolProperty1/2` vs
    /// `symbolProperty5`/`symbolDeclarationEmit10`).
    pub(crate) fn late_bound_symbol_member_name(
        &mut self,
        computed: &tsr_ast::ComputedPropertyName<'_>,
    ) -> Option<(String, bool)> {
        fn chain_text(expression: &tsr_ast::Expression<'_>) -> Option<String> {
            match expression {
                tsr_ast::Expression::Identifier(identifier) => Some(identifier.text.to_string()),
                tsr_ast::Expression::PropertyAccessExpression(access) => {
                    let base = chain_text(access.expression.as_ref()?)?;
                    let Some(tsr_ast::MemberName::Identifier(name)) = access.name else {
                        return None;
                    };
                    Some(format!("{base}.{}", name.text))
                }
                _ => None,
            }
        }
        let expression = computed.expression?;
        let name_type = self.check_expression(expression);
        // SS331 widened UNIQUE to SYMBOL-LIKE: the discriminator between the
        // per-member display and the `[x: symbol]` index form is whether the
        // name is an ENTITY REFERENCE, not whether its symbol is unique -
        // `var s = Symbol()` types PLAIN `symbol` and still displays
        // `{ [s]: number; ... }` (`symbolProperty2`), while the inline
        // `[Symbol()]` of `symbolProperty4` is no entity and takes the index
        // route.
        let flags = self.type_of(name_type).flags;
        // §553: the STRING- and NUMBER-LITERAL halves of
        // `StringOrNumberLiteralOrUnique` (`checker.go:13317`).
        // `computed_member_index_key` already routes them here — that guard is
        // upstream's own and was ported with SS206 — but this function only
        // ever answered the SYMBOL half, so `{ ["a"]: 1 }` and `{ [1]: 2 }`
        // fell to the caller's `return error` and gapped the whole literal.
        //
        // A late-bound literal name IS the member's name, and it takes the
        // same spelling rules a WRITTEN property name takes a few lines below:
        // an identifier-valid string prints bare (`{ ["a"]: 1 }` is
        // `{ a: number; }`), anything else is re-quoted through the shared
        // `printing::quote`, and a number prints normalised
        // (`{ [1]: 2 }` is `{ 1: number; }`, `{ [1.0]: 2 }` is `{ 1: number; }`).
        //
        // Reading the LITERAL TYPE rather than the written expression is what
        // makes `{ ["a" + ""]: 1 }` stay on the index route: its name type is
        // plain `string`, not a literal, so this arm does not fire.
        //
        // `true` for the second element. The flag's CALLER meaning is **"keep
        // the method spelling"**, not "is a unique symbol" — SS323 named it for
        // `UNIQUE_ES_SYMBOL` because that was the only thing reaching it, and
        // the method arm branches on it to choose `m(): number` over
        // `m: () => number`. §413 already settled that a literal-named method
        // spells like the property path does — `{ 0() { } }` is
        // `{ 0(): void; }`, `{ "foo"() { } }` is `{ foo(): void; }` — so
        // `{ ["m"]() { } }` is `{ m(): number; }`.
        match &self.type_of(name_type).data {
            crate::types::TypeData::StringLiteral(text) => {
                let text = text.clone();
                return Some((
                    if is_identifier_text(&text) { text } else { printing::quote(&text) },
                    true,
                ));
            }
            crate::types::TypeData::NumberLiteral(text) => {
                let text = text.clone();
                let spelled = printing::normalise_number(&text);
                // §557: a NEGATIVE numeric name keeps the BRACKETED written
                // form. `computedPropertiesNarrowed.types` records
                // `>t6 : { [-1]: number; }` and
                // `duplicateObjectLiteralProperty_computedName1` the same —
                // upstream prints `[-1]`, never a bare `-1`, because a
                // negative number is not spellable as a property name (it is a
                // unary expression, not a numeric literal token). §553's arm
                // spelled it bare, which was a residue of its own conversion:
                // `computedPropertiesNarrowed` is one of the cases §553 moved.
                if spelled.starts_with('-') {
                    return Some((format!("[{spelled}]"), true));
                }
                return Some((spelled, true));
            }
            _ => {}
        }
        if !flags.intersects(TypeFlags::ES_SYMBOL_LIKE) {
            return None;
        }
        let text = chain_text(&expression)?;
        Some((format!("[{text}]"), flags.intersects(TypeFlags::UNIQUE_ES_SYMBOL)))
    }

    /// SS307: the computed-name key dispatch of `checker.go:13317-13324`,
    /// See [`ComputedNameKey`] for the three answers.
    /// shared by the property (SS206), method, and accessor arms so the three
    /// cannot drift.
    ///
    /// Upstream's order, and it is not the obvious one:
    /// `isTypeAssignableTo(nameType, numberType)` is asked FIRST, then
    /// `esSymbolType`, then string (`checker.go:13319-13324`). So an `any`
    /// name yields a **number** index - which is why `{ [await]: foo }` with
    /// an un-typeable `await` records `{ [x: number]: any; }` and not a
    /// string index.
    pub(crate) fn computed_member_index_key(
        &mut self,
        computed: &tsr_ast::ComputedPropertyName<'_>,
    ) -> ComputedNameKey {
        let Some(expression) = computed.expression else {
            return ComputedNameKey::LateBound;
        };
        let name_type = self.check_expression(expression);
        let flags = self.type_of(name_type).flags;
        // **`StringOrNumberLiteralOrUnique` first** - upstream's own guard
        // (`checker.go:13317`), and the arms below are its `else`. SS206's
        // first draft tested `NUMBER_LIKE`, which contains `NUMBER_LITERAL`,
        // and turned `{ [1]: 1 }` into `{ [x: number]: number; }` - SS201's
        // own control caught it.
        if flags.intersects(
            TypeFlags::STRING_LITERAL | TypeFlags::NUMBER_LITERAL | TypeFlags::UNIQUE_ES_SYMBOL,
        ) {
            return ComputedNameKey::LateBound;
        }
        // SS331: a symbol-typed ENTITY name late-binds even when the symbol
        // is not unique - see `late_bound_symbol_member_name`'s note on
        // `symbolProperty2` vs `symbolProperty4`. The chain test keeps the
        // inline `[Symbol()]` on the index route below.
        if flags.intersects(TypeFlags::ES_SYMBOL_LIKE)
            && matches!(
                expression,
                tsr_ast::Expression::Identifier(_)
                    | tsr_ast::Expression::PropertyAccessExpression(_)
            )
        {
            return ComputedNameKey::LateBound;
        }
        // A UNION whose constituents are usable as property names is
        // late-bound too - `Math.random() > 0.5 ? "f1" : "f2"` names a member
        // upstream prints as the WRITTEN `[fieldName]`
        // (`compiler/declarationEmitSimpleComputedNames1`), unported. A
        // boolean name is also a union - of `true | false`, which name
        // nothing - and keeps contributing nothing (SS201's
        // `{ [0 in []]: true }` control).
        if flags.intersects(TypeFlags::UNION)
            && let crate::types::TypeData::Union { types, .. } = &self.store.get(name_type).data
            && types.iter().any(|&member| {
                self.store.get(member).flags.intersects(
                    TypeFlags::STRING_LITERAL
                        | TypeFlags::NUMBER_LITERAL
                        | TypeFlags::UNIQUE_ES_SYMBOL,
                )
            })
        {
            return ComputedNameKey::LateBound;
        }
        if flags.intersects(TypeFlags::NUMBER_LIKE | TypeFlags::ANY) {
            ComputedNameKey::Index("number")
        } else if flags.intersects(TypeFlags::ES_SYMBOL_LIKE) {
            ComputedNameKey::Index("symbol")
        } else if flags.intersects(TypeFlags::STRING_LIKE) {
            ComputedNameKey::Index("string")
        } else if flags.intersects(TypeFlags::TYPE_PARAMETER) {
            // §652. `{ [t]: 0 }` where `t: T` contributes a STRING index rather
            // than nothing: upstream prints `{ [x: string]: number; }` for both
            // an unconstrained `T` and a `U extends string`
            // (`computedPropertyNames51_ES5`/`_ES6`, `computedPropertyNames8_ES6`).
            // Dropping the property instead printed `{}`.
            ComputedNameKey::Index("string")
        } else {
            ComputedNameKey::Nothing
        }
    }

    /// §365: whether a literal sits inside a destructuring-ASSIGNMENT target —
    /// upstream's `ast.IsAssignmentTarget` reduced to the `=` form. The walk
    /// climbs the pattern spine (literals, member assignments, spreads) and
    /// answers at the first binary: true exactly when the spine hangs off the
    /// LEFT of a simple assignment. Binding patterns (`var {x} = …`) never
    /// reach this — their pattern is not an expression.
    pub(crate) fn is_assignment_pattern_target(&self, start: tsr_ast::NodeId) -> bool {
        let mut current = start;
        while let Some(parent) = self.nodes.parent(current) {
            match self.node_map.get(parent) {
                Some(tsr_ast::Node::BinaryExpression(binary)) => {
                    return binary
                        .operator_token
                        .is_some_and(|t| t.kind == tsr_ast::SyntaxKind::EqualsToken)
                        && binary.left.and_then(|l| l.node_id()) == Some(current);
                }
                // §451: `getAssignmentTargetKind`'s for-of/for-in head arm —
                // `for ({x, y = E.x} of array)` is an assignment target too
                // (`ast.IsAssignmentTarget` walks `KindForOfStatement`/
                // `KindForInStatement` initializers), so its defaulted
                // members print optional exactly as the `=` form's do
                // (`for-of47` wants `{ x: string; y?: E; }`).
                Some(tsr_ast::Node::ForInOrOfStatement(head)) => {
                    return head.initializer.as_ref().is_some_and(|initializer| {
                        tsr_ast::Node::from(*initializer).node_id() == Some(current)
                    });
                }
                Some(
                    tsr_ast::Node::ArrayLiteralExpression(_)
                    | tsr_ast::Node::ObjectLiteralExpression(_)
                    | tsr_ast::Node::PropertyAssignment(_)
                    | tsr_ast::Node::ShorthandPropertyAssignment(_)
                    | tsr_ast::Node::SpreadAssignment(_)
                    | tsr_ast::Node::SpreadElement(_),
                ) => current = parent,
                _ => return false,
            }
        }
        false
    }
}

impl<'a> Checker<'a, '_> {
    /// §489: the OBJECT BINDING PATTERN whose implied type is this literal's
    /// contextual type, found syntactically — either the literal is the
    /// initializer of a variable declaration whose name is such a pattern
    /// (`getContextualTypeForInitializerExpression` serving
    /// `getTypeFromBindingPattern`'s implied type), or it is a property value
    /// inside a literal that has one, through the matching element whose own
    /// name is a nested pattern. Everything else is `None` and the members
    /// print exactly as before.
    fn contextual_binding_pattern(
        &self,
        literal: tsr_ast::NodeId,
    ) -> Option<&'a tsr_ast::BindingPattern<'a>> {
        let parent = self.nodes.parent(literal)?;
        match self.node_map.get(parent)? {
            tsr_ast::Node::VariableDeclaration(declaration) => {
                if declaration.initializer.and_then(|i| i.node_id()) != Some(literal) {
                    return None;
                }
                // An ANNOTATED declaration's contextual type is the annotation
                // (`getContextualTypeForVariableLikeDeclaration` consults the
                // type node first); the implied-pattern road only exists
                // without one — `destructuringVariableDeclaration1ES5`'s
                // `{g: {g1 = …}}: { g: { g1: any[] } }` wants NO `?`.
                if declaration.r#type.is_some() {
                    return None;
                }
                let Some(tsr_ast::BindingName::BindingPattern(pattern)) = declaration.name else {
                    return None;
                };
                (self.nodes.kind(pattern.node_id?) == tsr_ast::SyntaxKind::ObjectBindingPattern)
                    .then_some(pattern)
            }
            tsr_ast::Node::PropertyAssignment(assignment) => {
                let object = self.nodes.parent(parent)?;
                let outer = self.contextual_binding_pattern(object)?;
                let element = matching_pattern_element(outer, &assignment.name)?;
                let Some(tsr_ast::BindingName::BindingPattern(inner)) = element.name else {
                    return None;
                };
                (self.nodes.kind(inner.node_id?) == tsr_ast::SyntaxKind::ObjectBindingPattern)
                    .then_some(inner)
            }
            _ => None,
        }
    }

    /// §897: the ASSIGNMENT pattern whose type is this literal's contextual
    /// type — `contextualTypeHasPattern` (`checker.go:13252`) for the half
    /// §489 did not search for.
    ///
    /// `({ a: x = 1 } = { a: 2 })` types the RIGHT literal against the LEFT
    /// pattern, and the left's defaulted properties are optional
    /// (`checker.go:13248`), so the right's copy them. Structurally this is
    /// §489 with `ObjectLiteralExpression` in place of `BindingPattern` and
    /// "the right of this `=`" in place of "the initializer of this
    /// declaration".
    ///
    /// **Purely syntactic.** It never asks for the left-hand pattern's *type*,
    /// only whether its matching member writes a default — so checking the
    /// right of an assignment never enters a resolution for the left. That is
    /// what makes the arm safe to run here at all; §890–§892 spent three
    /// entries on re-entrancy of exactly this shape.
    fn contextual_assignment_pattern(
        &self,
        literal: tsr_ast::NodeId,
    ) -> Option<&'a tsr_ast::ObjectLiteralExpression<'a>> {
        let parent = self.nodes.parent(literal)?;
        match self.node_map.get(parent)? {
            tsr_ast::Node::BinaryExpression(binary) => {
                if binary.operator_token.is_none_or(|t| t.kind != tsr_ast::SyntaxKind::EqualsToken)
                    || binary.right.and_then(|r| r.node_id()) != Some(literal)
                {
                    return None;
                }
                let Some(tsr_ast::Expression::ObjectLiteralExpression(pattern)) = binary.left
                else {
                    return None;
                };
                // The left must really BE a pattern, not an ordinary literal in
                // an expression position the grammar happens to allow.
                pattern
                    .node_id
                    .is_some_and(|id| self.is_assignment_pattern_target(id))
                    .then_some(pattern)
            }
            // A property value inside a literal that has one, through the
            // matching element whose own value is a nested pattern — §489's
            // second arm, mirrored.
            tsr_ast::Node::PropertyAssignment(assignment) => {
                let object = self.nodes.parent(parent)?;
                let outer = self.contextual_assignment_pattern(object)?;
                let member = matching_assignment_member(outer, &assignment.name)?;
                match assignment_member_pattern(member) {
                    Some(inner) => Some(inner),
                    None => None,
                }
            }
            _ => None,
        }
    }
}

impl Checker<'_, '_> {
    /// §735: render one object-literal member's type **from the literal's own
    /// site** rather than from its baked text.
    ///
    /// The render-at-reference cut §734 priced. A member type is minted
    /// wherever its declaration is, and [`Checker::type_to_string`] replays the
    /// text baked at that mint — the INSIDE view. Upstream never does that: it
    /// builds the name through `symbolToTypeNode` → `lookupSymbolChain`
    /// (`nodebuilderimpl.go:1061`), which walks the scope chain **from the
    /// reference**, so a nested module's member prints qualified, an aliased
    /// import prints under the alias in scope, and an embedded named type takes
    /// its rename.
    ///
    /// # A decline keeps the baked text, and that is the safety property
    ///
    /// [`Checker::type_to_string_at`] answers `None` for *"this port cannot
    /// name this type here"*. Falling back to `type_to_string` reproduces
    /// exactly what this position printed before the cut existed, so the change
    /// is structurally incapable of turning a right line wrong through a
    /// decline — only through a *better* name that is nonetheless wrong, which
    /// is what the score measures. Measured over the corpus at the first cut
    /// (the plain-property site alone): **+48 W→R, zero adverse transitions**.
    ///
    /// The literal's own node is the reference. Upstream's reference is the
    /// declaration whose `.types` line is being printed, which encloses this
    /// literal — same scope chain, since a literal opens no scope of its own.
    fn member_text_at(&mut self, id: TypeId, reference: Option<tsr_ast::NodeId>) -> String {
        match reference.and_then(|reference| self.type_to_string_at(id, reference)) {
            Some(text) => text,
            None => self.type_to_string(id),
        }
    }

    pub(crate) fn check_object_literal(&mut self, node: &ObjectLiteralExpression<'_>) -> TypeId {
        let error = self.intrinsics.error;
        // §105 slice 2a (`checker-notes-narrow.md`): a literal in a const
        // context (`isConstContext`, `checker.go:13615`) answers readonly
        // regular members — gated to literals with NO single-quoted string
        // member VALUE, because the value-spelling carriage is unbuilt and a
        // wrong quote is worse than the gap.
        // §799: an OBJECT literal reaching a `const` type parameter's argument
        // keeps its literal member types but is NOT readonly at the literal —
        // §798's rule, on the object road. Kept as a SECOND flag rather than
        // folded into `const_context`, which means "readonly regular members"
        // at seven sites below and only the regular-members half applies here;
        // conflating them measured 4 RIGHT→WRONG. The readonly is applied at
        // the inference site by `Checker::readonly_tuple_image`'s object arm.
        let const_parameter_context =
            node.node_id.is_some_and(|id| self.literal_in_const_type_variable_context(id));
        let const_context = node.node_id.is_some_and(|id| self.is_const_context(id));
        let regular_members = const_context || const_parameter_context;
        // §365: whether this literal IS a destructuring-assignment target —
        // upstream's `inDestructuringPattern := ast.IsAssignmentTarget(node)`
        // (checker.go:13155). A defaulted member in that position is OPTIONAL
        // (`inDestructuringPattern && hasDefaultValue`, checker.go:13248):
        // `({name: nameA = "noName"} = robot)` prints `{ name?: string; }`.
        let in_destructuring_pattern =
            node.node_id.is_some_and(|id| self.is_assignment_pattern_target(id));
        // §897: the ASSIGNMENT pattern this literal is typed against, if any —
        // `({ a: x = 1 } = { a: 2 })` gives the right literal `{ a?: number; }`.
        let contextual_assignment =
            node.node_id.and_then(|id| self.contextual_assignment_pattern(id));
        // §489: `contextualTypeHasPattern`'s BINDING half (`checker.go:13253`,
        // §365 built the assignment half above): a literal contextually typed
        // by the IMPLIED TYPE of an object binding pattern copies each implied
        // property's optionality, and the implied type is optional exactly
        // where the element writes a default
        // (`getTypeFromObjectBindingPattern`, `checker.go:17938`).
        // `let {x1 = 10} = { x1: 1 }` prints `{ x1?: number; }`. The pattern
        // is found syntactically — the declaration whose initializer this
        // literal is, or a matching element of an enclosing literal's pattern
        // — which is exactly where `getContextualType`'s variable-declaration
        // arm would have served the implied type.
        let contextual_pattern = node.node_id.and_then(|id| self.contextual_binding_pattern(id));
        let mut members = Vec::with_capacity(node.properties.len());
        let mut typed_properties: Vec<AnonymousProperty> = Vec::new();
        let property_only = node.properties.iter().all(|property| {
            matches!(
                property,
                tsr_ast::ObjectLiteralElementLike::PropertyAssignment(_)
                    | tsr_ast::ObjectLiteralElementLike::ShorthandPropertyAssignment(_)
                    | tsr_ast::ObjectLiteralElementLike::SpreadAssignment(_)
            )
        });

        // §206: the index-signature half §201 left as a gap. A computed name
        // whose type IS string-, number- or symbol-like contributes an INDEX
        // SIGNATURE rather than a member (`checker.go:13195-13205`), whose
        // value type is the union of the contributing members' types
        // (`getObjectLiteralIndexInfo`, `:19721`).
        let mut index_values: Vec<(&'static str, TypeId)> = Vec::new();
        // §539 — see the push below and `Checker::object_literal_index_infos`.
        let mut minted_index_info: Option<crate::index_signatures::IndexInfo> = None;
        // SS325: late-bound ACCESSOR members merge by name - a get/set pair
        // is one property (the getter's type wins the display), a getter
        // without a setter is `readonly` (`symbolDeclarationEmit10`,
        // `symbolProperty5`). Non-accessor members of the same name stay
        // separate rows (`symbolProperty1`'s triple).
        let mut accessor_members: Vec<(String, usize)> = Vec::new();
        for property in node.properties {
            let mut pending_index_key: Option<&'static str> = None;
            // §365: `hasDefaultValue(memberDecl)` (checker.go:13248) — a
            // property assignment whose value is an `=` binary, or a
            // shorthand carrying an object-assignment initializer.
            let mut member_optional = false;
            // `checker.go:13223` dispatches over three member kinds. Only two are
            // reachable here: a method needs `checkObjectLiteralMethod` and a
            // signature member this port cannot print, and a spread or accessor
            // is not in that list at all.
            let mut property_node_id: Option<tsr_ast::NodeId> = None;
            let (name_node, value) = match property {
                tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                    let Some(initializer) = assignment.initializer else { return error };
                    property_node_id = assignment.node_id;
                    if in_destructuring_pattern
                        && matches!(initializer, tsr_ast::Expression::BinaryExpression(binary)
                            if binary.operator_token.is_some_and(|t| t.kind == tsr_ast::SyntaxKind::EqualsToken))
                    {
                        member_optional = true;
                    } else if let Some(pattern) = contextual_pattern
                        && implied_pattern_member_is_optional(pattern, &assignment.name)
                    {
                        // §489 — the `impliedProp.Flags & Optional` copy.
                        member_optional = true;
                    } else if let Some(pattern) = contextual_assignment
                        && assignment_pattern_member_is_optional(pattern, &assignment.name)
                    {
                        // §897 — the same copy, from an ASSIGNMENT pattern.
                        member_optional = true;
                    }
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
                        // §365: the named edit arrived — destructuring
                        // assignment is ported, so `{ nameA = "noName" } = x`
                        // reaches this literal. In pattern position the
                        // member types as the NAME expression (its declared
                        // binding) and the default makes it optional
                        // (checker.go:13248). Outside a pattern the form is
                        // a grammar error and keeps the gap.
                        if !in_destructuring_pattern {
                            return error;
                        }
                        member_optional = true;
                    } else if let Some(pattern) = contextual_pattern
                        && implied_pattern_member_is_optional(pattern, &shorthand.name)
                    {
                        // §489 — the shorthand member reads the same implied
                        // optionality (`{x}` against `let {x = 1} = …`).
                        member_optional = true;
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
                    if !self.is_valid_spread_type(source) {
                        return error;
                    }
                    let source = self.try_merge_union_of_object_type_and_empty_object(source);
                    let Some(spread_members) = self.spread_members_of(source) else {
                        return error;
                    };
                    for mut member in spread_members {
                        if property_only
                            && let Member::Property { name, optional, readonly, printed } = &member
                        {
                            let semantic_name = self
                                .anonymous_properties
                                .get(&source)
                                .and_then(|(properties, _)| {
                                    properties
                                        .iter()
                                        .find(|property| property.printed_name == *name)
                                })
                                .map_or_else(|| name.clone(), |property| property.name.clone());
                            if let Some(value) =
                                self.get_type_of_property_of_type(source, &semantic_name)
                                && value != error
                            {
                                let mut property = AnonymousProperty {
                                    name: semantic_name,
                                    printed_name: name.clone(),
                                    printed_type: printed.clone(),
                                    optional: *optional,
                                    readonly: const_context || *readonly,
                                    r#type: value,
                                };
                                if let Some(index) = typed_properties
                                    .iter()
                                    .position(|held| held.name == property.name)
                                {
                                    // getSpreadType (checker.go:13463): an optional
                                    // right property preserves the left's presence and
                                    // unions the values that can actually be written.
                                    let left = &typed_properties[index];
                                    if property.optional {
                                        let left_type = left.r#type;
                                        property.optional = left.optional;
                                        let left_present =
                                            self.remove_missing_or_undefined_type(left_type);
                                        let right_present =
                                            self.remove_missing_or_undefined_type(value);
                                        property.r#type = if left_present == right_present {
                                            left_type
                                        } else {
                                            let Some(merged) =
                                                self.union_with_subtype_reduction(&[
                                                    left_type,
                                                    right_present,
                                                ])
                                            else {
                                                return error;
                                            };
                                            merged
                                        };
                                        let displayed = self.remove_missing_type(property.r#type);
                                        property.printed_type = self.type_to_string(displayed);
                                        member = Member::Property {
                                            name: property.printed_name.clone(),
                                            optional: property.optional,
                                            readonly: property.readonly,
                                            printed: property.printed_type.clone(),
                                        };
                                    }
                                    typed_properties[index] = property;
                                } else {
                                    typed_properties.push(property);
                                }
                            }
                        }
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
                    // §201: a private-named METHOD is the same non-member as a
                    // private-named property — the grammar refuses it and
                    // upstream's `propertiesTable` never sees it, so
                    // `{ #foo() {} }` is `{}`
                    // (`conformance/privateNameInObjectLiteral-2.types`).
                    if matches!(method.name, tsr_ast::PropertyName::PrivateIdentifier(_)) {
                        continue;
                    }
                    // SS307: a computed-name METHOD takes the same index route
                    // the property arm built at SS206 - `{ [e]() { } }` with an
                    // untypeable `e` is `{ [x: number]: () => void; }`
                    // (`conformance/parserComputedPropertyName3`). The value is
                    // the method's own function type - the same road that
                    // already prints the member's `.types` line.
                    if let tsr_ast::PropertyName::ComputedPropertyName(computed) = method.name {
                        let key = match self.computed_member_index_key(computed) {
                            ComputedNameKey::LateBound => {
                                // SS323: a UNIQUE late-bound METHOD keeps the
                                // method spelling - `{ [Symbol.hasInstance]
                                // (value: any): boolean; }`
                                // (`modularizeLibrary_*`; the arrow-form
                                // draft was 19 G->W). A PLAIN-symbol
                                // component row is a PROPERTY of the arrow
                                // form (`symbolProperty1/2`).
                                let Some((name, unique)) =
                                    self.late_bound_symbol_member_name(computed)
                                else {
                                    return error;
                                };
                                if unique {
                                    let printed = format!(
                                        "{name}{}",
                                        signature_member_text(self, &signature)
                                    );
                                    members.push(Member::Signature { printed });
                                } else {
                                    let printed = self.signature_to_string(&signature);
                                    members.push(Member::Property {
                                        name,
                                        optional: false,
                                        readonly: const_context,
                                        printed,
                                    });
                                }
                                continue;
                            }
                            ComputedNameKey::Nothing => continue,
                            ComputedNameKey::Index(key) => key,
                        };
                        let Some(symbol) = self.binder.symbol_of(id) else { return error };
                        let member_type = self.get_type_of_symbol(symbol);
                        if member_type == error {
                            return error;
                        }
                        index_values.push((key, member_type));
                        continue;
                    }
                    // §413: numeric and string method names take the SAME
                    // spelling rules the property path has — `{ 0() { } }`
                    // is `{ 0(): void; }` and `{ "foo"() { } }` is
                    // `{ foo(): void; }` (`parserFunctionPropertyAssignment2/3/4`).
                    let spelled_name;
                    let name = match method.name {
                        tsr_ast::PropertyName::Identifier(name) => name.text,
                        tsr_ast::PropertyName::NumericLiteral(literal) => {
                            spelled_name = printing::normalise_number(literal.text);
                            spelled_name.as_str()
                        }
                        tsr_ast::PropertyName::StringLiteral(literal)
                            if is_identifier_text(literal.text) =>
                        {
                            literal.text
                        }
                        tsr_ast::PropertyName::StringLiteral(literal) => {
                            spelled_name = printing::quote(literal.text);
                            spelled_name.as_str()
                        }
                        _ => return error,
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
                                name: name.to_string(),
                                optional: false,
                                readonly: true,
                                printed: arrow,
                            },
                        );
                        continue;
                    }
                    // §735: the method arm's slots render at the literal's site
                    // too, through `signature_member_text_at` — the slot-for-slot
                    // twin of `signature_member_text` with a per-slot fallback to
                    // the baked text. No reference (a literal with no node id)
                    // keeps the site-less spelling.
                    let member_text = match node.node_id {
                        Some(reference) => self.signature_member_text_at(&signature, reference),
                        None => signature_member_text(self, &signature),
                    };
                    let printed = if name == "new" {
                        format!("\"new\"{member_text}")
                    } else {
                        format!("{name}{member_text}")
                    };
                    upsert_member(&mut members, Member::Signature { printed });
                    continue;
                }
                // SS307: computed-name ACCESSORS join the index route. A
                // getter contributes its RETURN type (`{ get [e]() { } }` is
                // `{ [x: number]: void; }`, `parserComputedPropertyName4`); a
                // setter its first PARAMETER's type, `any` when unannotated
                // (`parserComputedPropertyName17`). Identifier-named accessors
                // keep gapping - their printed form (`readonly x`, getter/
                // setter merging) is `assignmentCompatBug3`'s own question.
                tsr_ast::ObjectLiteralElementLike::GetAccessorDeclaration(accessor) => {
                    // §415: an IDENTIFIER-named getter is a property of the
                    // accessor pair's type — `{ get x() { return 1 } }` is
                    // `{ x: number; }`, readonly only when no setter sibling
                    // names it (§325's merge, keyed on written names). The
                    // pair's type goes through get_type_of_accessors, the
                    // same road class accessors take.
                    if let tsr_ast::PropertyName::Identifier(name) = accessor.name {
                        let Some(id) = accessor.node_id else { return error };
                        let Some(symbol) = self.binder.symbol_of(id) else { return error };
                        let member_type = self.get_type_of_symbol(symbol);
                        if member_type == error {
                            return error;
                        }
                        let setter_sibling =
                            node.properties.iter().find_map(|sibling| match sibling {
                                tsr_ast::ObjectLiteralElementLike::SetAccessorDeclaration(
                                    setter,
                                ) if matches!(setter.name,
                                    tsr_ast::PropertyName::Identifier(other)
                                        if other.text == name.text) =>
                                {
                                    Some(setter)
                                }
                                _ => None,
                            });
                        // §417: a pair whose getter and setter types DIFFER
                        // prints the accessor forms —
                        // `{ get x(): string; set x(a: number); }`
                        // (declarationEmitObjectLiteralAccessors1), §415's
                        // recorded rung.
                        let setter_annotation = setter_sibling
                            .and_then(|setter| setter.node_id)
                            .and_then(|setter_id| match self.node_map.get(setter_id) {
                                Some(tsr_ast::Node::SetAccessorDeclaration(fetched)) => {
                                    fetched.parameters.first().and_then(|p| p.r#type)
                                }
                                _ => None,
                            });
                        if let Some(annotation) = setter_annotation {
                            let setter_type = self.get_type_from_type_node(annotation);
                            if setter_type != error
                                && self.type_to_string(setter_type)
                                    != self.type_to_string(member_type)
                            {
                                let Some(signature) = self.get_signature_from_declaration(id)
                                else {
                                    return error;
                                };
                                let printed = format!(
                                    "get {}(): {}",
                                    name.text,
                                    self.type_to_string(signature.r#type)
                                );
                                members.push(Member::Signature { printed });
                                continue;
                            }
                        }
                        let printed = self.member_text_at(member_type, node.node_id);
                        upsert_member(
                            &mut members,
                            Member::Property {
                                name: name.text.to_string(),
                                optional: false,
                                readonly: const_context || setter_sibling.is_none(),
                                printed,
                            },
                        );
                        continue;
                    }
                    let tsr_ast::PropertyName::ComputedPropertyName(computed) = accessor.name
                    else {
                        return error;
                    };
                    let Some(id) = accessor.node_id else { return error };
                    let key = match self.computed_member_index_key(computed) {
                        // SS325: a late-bound GETTER prints as a property of
                        // its return type - the third `[s]: number` row of
                        // `symbolProperty1`'s literal.
                        ComputedNameKey::LateBound => {
                            let Some((name, unique)) = self.late_bound_symbol_member_name(computed)
                            else {
                                return error;
                            };
                            let Some(signature) = self.get_signature_from_declaration(id) else {
                                return error;
                            };
                            let printed = self.member_text_at(signature.r#type, node.node_id);
                            if !unique {
                                // A component-row getter: plain property, no
                                // readonly, no merge (`symbolProperty1/2`).
                                members.push(Member::Property {
                                    name,
                                    optional: false,
                                    readonly: const_context,
                                    printed,
                                });
                                continue;
                            }
                            let mut has_setter_sibling = false;
                            for sibling in node.properties {
                                if let tsr_ast::ObjectLiteralElementLike::SetAccessorDeclaration(
                                    setter,
                                ) = sibling
                                    && let tsr_ast::PropertyName::ComputedPropertyName(sibling_name) =
                                        setter.name
                                    && self
                                        .late_bound_symbol_member_name(sibling_name)
                                        .is_some_and(|(sibling, _)| sibling == name)
                                {
                                    has_setter_sibling = true;
                                }
                            }
                            let member = Member::Property {
                                name: name.clone(),
                                optional: false,
                                readonly: const_context || !has_setter_sibling,
                                printed,
                            };
                            if let Some(&(_, index)) =
                                accessor_members.iter().find(|(existing, _)| existing == &name)
                            {
                                members[index] = member;
                            } else {
                                accessor_members.push((name, members.len()));
                                members.push(member);
                            }
                            continue;
                        }
                        ComputedNameKey::Nothing => continue,
                        ComputedNameKey::Index(key) => key,
                    };
                    let Some(signature) = self.get_signature_from_declaration(id) else {
                        return error;
                    };
                    index_values.push((key, signature.r#type));
                    continue;
                }
                tsr_ast::ObjectLiteralElementLike::SetAccessorDeclaration(accessor) => {
                    // §415: the setter half of the identifier-named pair —
                    // the binder merges same-named accessors into one symbol,
                    // so both arms compute the SAME get_type_of_accessors
                    // answer and the upsert keeps one row.
                    if let tsr_ast::PropertyName::Identifier(name) = accessor.name {
                        let Some(id) = accessor.node_id else { return error };
                        let Some(symbol) = self.binder.symbol_of(id) else { return error };
                        let member_type = self.get_type_of_symbol(symbol);
                        if member_type == error {
                            return error;
                        }
                        // §417: the setter half of the divergent pair —
                        // `set x(a: number)` prints whole when its annotated
                        // parameter differs from the pair's type (which the
                        // getter arm's road computed as the getter's).
                        let fetched_parameter = match self.node_map.get(id) {
                            Some(tsr_ast::Node::SetAccessorDeclaration(fetched)) => {
                                fetched.parameters.first().copied()
                            }
                            _ => None,
                        };
                        if let Some(parameter) = fetched_parameter
                            && let Some(annotation) = parameter.r#type
                        {
                            let setter_type = self.get_type_from_type_node(annotation);
                            // Compared by PRINTED form: an anonymous mint
                            // (a function-type annotation) is not interned,
                            // so TypeId inequality alone would split
                            // setter-only pairs whose types agree
                            // (`setParamType1`, the draft's 4 R->GAP).
                            if setter_type != error
                                && self.type_to_string(setter_type)
                                    != self.type_to_string(member_type)
                            {
                                let parameter_name = match parameter.name {
                                    Some(tsr_ast::BindingName::Identifier(p)) => p.text,
                                    _ => return error,
                                };
                                let printed = format!(
                                    "set {}({}: {})",
                                    name.text,
                                    parameter_name,
                                    self.type_to_string(setter_type)
                                );
                                members.push(Member::Signature { printed });
                                continue;
                            }
                        }
                        let printed = self.member_text_at(member_type, node.node_id);
                        upsert_member(
                            &mut members,
                            Member::Property {
                                name: name.text.to_string(),
                                optional: false,
                                readonly: const_context,
                                printed,
                            },
                        );
                        continue;
                    }
                    let tsr_ast::PropertyName::ComputedPropertyName(computed) = accessor.name
                    else {
                        return error;
                    };
                    let Some(id) = accessor.node_id else { return error };
                    let key = match self.computed_member_index_key(computed) {
                        // SS325: the setter half - a property of its first
                        // parameter's type, `any` when unannotated, the same
                        // value rule the index route uses.
                        ComputedNameKey::LateBound => {
                            let Some((name, unique)) = self.late_bound_symbol_member_name(computed)
                            else {
                                return error;
                            };
                            if unique
                                && accessor_members.iter().any(|(existing, _)| existing == &name)
                            {
                                // The getter already owns the display; a
                                // getter appearing LATER replaces in place.
                                continue;
                            }
                            let Some(signature) = self.get_signature_from_declaration(id) else {
                                return error;
                            };
                            let member_type = signature
                                .parameters
                                .first()
                                .map_or(self.intrinsics.any, |parameter| parameter.r#type);
                            let printed = self.member_text_at(member_type, node.node_id);
                            if unique {
                                accessor_members.push((name.clone(), members.len()));
                            }
                            members.push(Member::Property {
                                name,
                                optional: false,
                                readonly: const_context,
                                printed,
                            });
                            continue;
                        }
                        ComputedNameKey::Nothing => continue,
                        ComputedNameKey::Index(key) => key,
                    };
                    let Some(signature) = self.get_signature_from_declaration(id) else {
                        return error;
                    };
                    let member_type = signature
                        .parameters
                        .first()
                        .map_or(self.intrinsics.any, |parameter| parameter.r#type);
                    index_values.push((key, member_type));
                    continue;
                }
            };
            let name = match name_node {
                // §537: a **parser-recovery placeholder** contributes nothing,
                // for the same reason the private-name arm above does — a
                // member with no spellable name is not a member.
                //
                // `var x = { `a`: 321 }` is a syntax error: a template literal
                // cannot be a property name. The parser reports it and, to keep
                // going, hands the property assignment a **missing identifier**
                // — an `Identifier` whose `text` is empty. Nothing in
                // `checkObjectLiteral` puts such a thing in `propertiesTable`,
                // and `templateStringInPropertyName1.types` records the literal
                // as `>{ : {}`.
                //
                // Without this the port printed **`{ : any; }`** — a shape no
                // compiler emits, with a colon and no name in front of it — in
                // the four `templateStringInPropertyName*` cases, each of which
                // is one line from passing.
                //
                // Gated on the empty text rather than on the node kind: the
                // recovery placeholder is the only way an identifier reaches
                // here with no text, and gating on the kind would need one arm
                // per token the parser might have swallowed.
                tsr_ast::PropertyName::Identifier(name) if name.text.is_empty() => continue,
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
                // **A computed name that cannot name a property contributes
                // NOTHING — not a member, and not a gap.** `checkObjectLiteral`
                // (`checker.go:13317-13332`) never puts a computed-name member
                // in `propertiesTable` unless its name type carries
                // `StringOrNumberLiteralOrUnique`; otherwise it either sets an
                // index-signature flag (when the name type is assignable to
                // `string | number | symbol`) or drops the member on the floor.
                //
                // So `{ [0 in []]: true }` — a `boolean` name — is `{}`, and
                // `{ [Symbol.prototype]: 0 }` is `{}`. This port gapped the
                // whole literal instead. §201.
                //
                // The two arms this does NOT take, each still a gap:
                //
                // - A name type that IS a string/number literal or a unique
                //   symbol is **late-bound** — it names a real member — and
                //   printing that name is unported.
                // - A name type assignable to `string | number | symbol`
                //   yields an INDEX SIGNATURE, also unported. It is separated
                //   by flags rather than by assignability here, which is
                //   narrower than upstream in the safe direction: a type this
                //   port cannot see as string-like keeps gapping instead of
                //   silently losing an index signature.
                tsr_ast::PropertyName::ComputedPropertyName(computed) => {
                    // The dispatch lives in `computed_member_index_key`,
                    // shared with the SS307 method/accessor arms.
                    match self.computed_member_index_key(computed) {
                        // SS323: a unique-symbol name is the one late-bound
                        // kind whose printed member this port can spell.
                        ComputedNameKey::LateBound => {
                            match self.late_bound_symbol_member_name(computed) {
                                Some((name, _)) => name,
                                None => return error,
                            }
                        }
                        // SS201: a name that cannot key anything contributes
                        // nothing at all - not a member and not a signature.
                        ComputedNameKey::Nothing => continue,
                        ComputedNameKey::Index(key) => {
                            pending_index_key = Some(key);
                            String::new()
                        }
                    }
                }
                // A **private name** cannot be an object-literal member. The
                // grammar refuses it, the parser has already reported, and the
                // binder declares nothing — so upstream's `propertiesTable`
                // never sees it and `{ #foo: 1 }` is `{}`
                // (`conformance/privateNameInObjectLiteral-1.types`). Gapping
                // the literal turned a reported syntax error into a second,
                // silent type failure. §201.
                tsr_ast::PropertyName::PrivateIdentifier(_) => continue,
                _ => return error,
            };
            let member_type = match value {
                // Const context first — upstream's own order in
                // `checkExpressionForMutableLocation`: `isConstContext` wins
                // before any contextual retention question is asked.
                PropertyValue::Initializer(initializer) if regular_members => {
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
            //
            // # §930.2: §929's argument does NOT reach here, and this is the
            // measurement
            //
            // §929 and §930 removed exactly this rule one and two floors up —
            // an unresolvable *annotation* no longer declines a signature or a
            // type literal — so the obvious next step was to open this gate
            // too. Measured: **23 `WRONG->RIGHT` against 335 `GAP->WRONG` and
            // 3 `RIGHT->WRONG`** (`correlatedUnions` 20,
            // `mappedTypeContextualTypesApplied` 18,
            // `contextualTypeWithUnionTypeIndexSignatures` 16). Reverted.
            //
            // **The difference is what there is to print.** An annotation that
            // fails to resolve still has the text the user wrote, and upstream
            // prints exactly that; §929's `any` is a placeholder behind a
            // faithful spelling. An *expression* that fails has no such text —
            // `any` here is not a placeholder, it is an invention, and 335 rows
            // say so. §146's narrowness was earned, and its one admitted
            // population is admitted because upstream reports TS2304 and
            // genuinely answers `any` there.
            //
            // Reopening condition: not a wider gate, but the *members* that
            // error. Each is a separate defect upstream computes; the gate is
            // the messenger.
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
            // §853: the declaration-level `getWidenedType` would turn a
            // nullable member into `any`, and this port has no call site for
            // it — **but only when `strictNullChecks` is OFF does upstream
            // widen at all**. `createWideningType`
            // (`checker.go:25027`):
            //
            // ```go
            // func (c *Checker) createWideningType(nonWideningType *Type) *Type {
            //     if c.strictNullChecks {
            //         return nonWideningType
            //     }
            //     t := c.newIntrinsicType(...)
            //     t.objectFlags |= ObjectFlagsContainsWideningType
            //     return t
            // }
            // ```
            //
            // `undefinedWideningType` and `nullWideningType` are what a
            // `null`/`undefined` expression answers, and under strict they ARE
            // the plain types with no `ContainsWideningType`.
            // `getWidenedTypeWithContext` is gated on `RequiresWidening`, so
            // under strict there is nothing to widen and `{ p: null }` is
            // `{ p: null; }`.
            //
            // Non-strict keeps the refusal for its original reason: there the
            // port really would need `getWidenedType`, and answering
            // `{ p: null; }` where upstream answers `{ p: any; }` would be a
            // confident wrong line in place of an honest gap.
            if !self.strict_null_checks
                && self.store.get(member_type).flags.intersects(TypeFlags::NULLABLE)
            {
                return error;
            }
            // §892: record the member's type on its SYMBOL, which is what
            // upstream's `checkObjectLiteral` does through
            // `links.resolvedType` — so `getTypeOfSymbol` for an
            // object-literal property *reads* this rather than recomputing it.
            //
            // Recomputing is a divergence with consequences beyond the wasted
            // work: the recompute runs inside a resolution frame for this very
            // symbol, so anything it consults that leads back to the enclosing
            // literal re-enters and answers `any`. That is the mechanism §891
            // finally isolated (`:100` the literal right, `:101` the property
            // `any`) and the reason §890 had to exclude call arguments.
            //
            // Landed on its own first: caching a value the recompute would have
            // produced anyway must be a no-op, and measuring it separately is
            // what tells us whether the two roads already disagree.
            if let Some(symbol) = property_node_id.and_then(|id| self.binder.symbol_of(id)) {
                self.symbol_types.entry(symbol).or_insert(member_type);
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
                // §735 — see [`Checker::member_text_at`].
                _ => self.member_text_at(member_type, node.node_id),
            };
            if let Some(key) = pending_index_key {
                index_values.push((key, member_type));
                continue;
            }
            // SS329 corrects SS323's push rule: `symbolProperty1`'s three
            // `[s]` rows come from three DIFFERENT ARMS (property, method,
            // getter), so the plain-property flow upserts for bracketed
            // names exactly as for written ones - duplicate late-bound
            // PROPERTY assignments collapse to one row
            // (`symbolProperty36`'s `{ [Symbol.isConcatSpreadable]: 0,
            // [Symbol.isConcatSpreadable]: 1 }` prints one member).
            if property_only {
                let semantic_name = property_node_id
                    .and_then(|id| self.binder.symbol_of(id))
                    .map(|symbol| self.binder.symbols().get(symbol).name.to_string())
                    .or_else(|| property_name_text(&name_node).map(str::to_string));
                if let Some(semantic_name) = semantic_name {
                    let property = AnonymousProperty {
                        name: semantic_name,
                        printed_name: name.clone(),
                        printed_type: printed.clone(),
                        optional: member_optional,
                        readonly: const_context,
                        r#type: member_type,
                    };
                    if let Some(index) =
                        typed_properties.iter().position(|p| p.name == property.name)
                    {
                        typed_properties[index] = property;
                    } else {
                        typed_properties.push(property);
                    }
                }
            }
            upsert_member(
                &mut members,
                Member::Property {
                    name,
                    optional: member_optional,
                    readonly: const_context,
                    printed,
                },
            );
        }
        if !index_values.is_empty() {
            // **Slice 1: every member is a computed name of one key kind.**
            // Upstream's `getObjectLiteralIndexInfo` (`checker.go:19721`)
            // filters `propertiesArray` by whether each property's name suits
            // the key — numeric-named for a number key, symbol-named for a
            // symbol key, everything-but-symbol for a string key — and this
            // port has no numeric-name predicate for a *written* name. With no
            // named members present there is nothing to filter, so the union is
            // simply every contributor's type and the two agree by
            // construction. A literal mixing named and computed members keeps
            // gapping, and so does one mixing key kinds: upstream emits one
            // index info per kind, in string/number/symbol order, and getting
            // that order wrong prints a plausible wrong line. §206.
            // §551: the NUMBER-key slice of upstream's filter.
            // `getObjectLiteralIndexInfo` (`checker.go:19721`) does not decline
            // a mixed literal — it filters `propertiesArray` by whether each
            // property's name suits the key, and for a NUMBER key that is the
            // numerically-named members only. So
            // `{ x: 1, [k]: 2 }` with `k: number` is
            // `{ x: number; [x: number]: number; }`: `x` stays a PROPERTY and
            // contributes nothing to the index value.
            //
            // The decline's stated reason was *"this port has no numeric-name
            // predicate for a written name"*. It does: a written numeric name
            // is normalised through `printing::normalise_number` at the mint
            // above, so a member whose name is all digits is exactly the
            // numeric case. When NO named member is numeric the filter removes
            // nothing and the two agree by construction — which is the slice
            // taken here.
            //
            // Still declining, each for its own reason:
            //
            // - **A numerically-named member beside a number key.** It joins
            //   the index value union, and the union's ORDER against upstream's
            //   is unverified. A wrong order prints a plausible wrong line.
            // - **A STRING key beside named members.** Upstream's filter keeps
            //   everything but symbol-named properties, so every named member
            //   contributes to the value union — a different computation, not
            //   this one.
            //
            //   **§859 measured it, and the refusal stands.** The account of
            //   upstream above is exactly right — `getObjectLiteralIndexInfo`
            //   (`checker.go:19721`) unions `getTypeOfSymbol` over every
            //   non-symbol-named property — but the population is **tens of
            //   lines**: a corpus query for wanted object types carrying both
            //   an index signature and a named member returns 131 non-RIGHT
            //   lines across 46 cases, of which only **18 are gaps** and most
            //   of those are *declared* object types rather than the object
            //   LITERALS this guard governs.
            //
            //   And it is not free. `Member` holds printed strings, not
            //   `TypeId`s, so supplying upstream's union means threading a
            //   parallel `(name, TypeId)` list through the eight-plus
            //   `members.push`/`upsert_member` sites in this function. A
            //   medium refactor of a delicate function for tens of lines is
            //   the wrong trade.
            //
            //   **Reopen only with a re-run of that query**, and note that
            //   `computedPropertyNames10_ES6` — the case that surfaced this —
            //   also needs the mixed-key-kinds half below, so it will not fall
            //   to this half alone.
            // - **Mixed key kinds**, unchanged below: upstream emits one index
            //   info per kind in string/number/symbol order and getting that
            //   order wrong prints a plausible wrong line.
            let numeric_named = members.iter().any(|member| match member {
                Member::Property { name, .. } => {
                    !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit())
                }
                _ => true,
            });
            if !members.is_empty() && (index_values[0].0 != "number" || numeric_named) {
                return error;
            }
            let key = index_values[0].0;
            if index_values.iter().any(|(k, _)| *k != key) {
                return error;
            }
            let mut distinct: Vec<TypeId> = Vec::new();
            for (_, value) in &index_values {
                if !distinct.contains(value) {
                    distinct.push(*value);
                }
            }
            let value = match distinct.as_slice() {
                [single] => *single,
                many => {
                    // SS331: the relation answers UNKNOWN for any pair
                    // involving a signature-shaped type, and one Unknown
                    // declines the whole reduction - which gapped
                    // `{ [Symbol()]: 0, [Symbol()]() { }, get ... }`
                    // (`symbolProperty4`, want
                    // `number | (() => void)`). A callable is never a strict
                    // subtype of a non-callable, so the callables pass
                    // through and only the plain constituents reduce.
                    let (callable, plain): (Vec<TypeId>, Vec<TypeId>) = many
                        .iter()
                        .partition(|candidate| self.signature_types.contains_key(candidate));
                    let mut kept = if plain.len() > 1 {
                        let Some(reduced) = self.union_with_subtype_reduction(&plain) else {
                            return error;
                        };
                        match &self.store.get(reduced).data {
                            crate::types::TypeData::Union { types, .. } => types.clone(),
                            _ => vec![reduced],
                        }
                    } else {
                        plain
                    };
                    kept.extend(callable);
                    self.get_union_type(&kept)
                }
            };
            // The parameter name is upstream's synthesized `x` — a real index
            // signature prints the name its declaration wrote, but this one has
            // no declaration (`newIndexInfo(..., declaration: nil, ...)`).
            // §593: BEFORE the properties, not after them. Upstream's node
            // builder emits an anonymous object's index signatures ahead of its
            // properties — `{ [x: number]: any; p1: number; … }`
            // (`computedPropertyNames49_ES5`/`50`, ES5 and ES6 halves) — and
            // `get_type_from_type_literal` (`declared.rs`) already orders its
            // own three groups `signatures, indexes, properties`. This road
            // pushed the index onto the END, so the two spellings of one rule
            // disagreed and this one printed the index last.
            //
            // Inserted before the first PROPERTY rather than at index 0, which
            // keeps it behind any call/construct signature exactly as the
            // TypeLiteral road's group order does.
            let first_property = members
                .iter()
                .position(|member| matches!(member, Member::Property { .. }))
                .unwrap_or(members.len());
            members.insert(
                first_property,
                Member::Index {
                    readonly: const_context,
                    name: "x".to_string(),
                    key: key.to_string(),
                    value: self.member_text_at(value, node.node_id),
                },
            );
            // §539: the same info, kept so the LOOKUP can consult it. Until
            // now this signature existed only in the printed text — the type
            // is a `Named` over the binder's `__object` symbol and
            // `get_index_infos_of_type` recovers infos from a symbol's
            // DECLARATIONS, which an object literal has none of. So
            // `{ [this.bar()]: 1 }` printed `{ [x: number]: number; }` and
            // `{ [this.bar()]: 1 }[0]` answered `errorType`.
            //
            // `symbol` keys are deliberately NOT recorded: `is_applicable_index_type`
            // decides applicability for the `string`/`number` intrinsics and
            // their literal types only, and a key it cannot judge must stay a
            // gap rather than become a confident wrong value.
            minted_index_info = match key {
                "number" => Some(crate::index_signatures::IndexInfo {
                    key: self.intrinsics.number,
                    value,
                    readonly: const_context,
                }),
                "string" => Some(crate::index_signatures::IndexInfo {
                    key: self.intrinsics.string,
                    value,
                    readonly: const_context,
                }),
                _ => None,
            };
        }
        let printed = render_object_type(&members);
        // The binder gives an object literal its own `__object` symbol, whose
        // members table is where a property access on this type looks — the same
        // arrangement `get_type_from_type_literal` relies on for `__type`.
        let symbol = node.node_id.and_then(|id| self.binder.symbol_of(id));
        let minted = self.store.new_named(TypeFlags::OBJECT, printed, symbol);
        if property_only && typed_properties.len() == members.len() {
            // Spread properties have no binder symbol on this literal. Use
            // their resolved semantic types for member lookup (getSpreadType).
            let synthetic = node.properties.iter().any(|property| {
                matches!(property, tsr_ast::ObjectLiteralElementLike::SpreadAssignment(_))
            });
            self.anonymous_properties.insert(minted, (typed_properties, synthetic));
        }
        // SS185: upstream's `ObjectFlagsJSLiteral` — an object literal
        // created in a JS FILE is a "JS literal" type, which
        // `getPropertyTypeForIndexType`'s failure path answers `any` for
        // (`checker.go:27130` and `:27189` via `isJSLiteralType`,
        // `utilities.go:1753`). Recorded in a side table per ADR-0003.
        if node.node_id.is_some_and(|id| self.in_js_file(id)) {
            self.js_literal_types.insert(minted);
        }
        // §453: `ObjectFlagsFreshLiteral` — see the side table's doc on
        // `Checker::fresh_object_literal_types`.
        self.fresh_object_literal_types.insert(minted);
        self.object_literal_spread_flags.insert(
            minted,
            node.properties.iter().any(|property| {
                matches!(property, tsr_ast::ObjectLiteralElementLike::SpreadAssignment(_))
            }),
        );
        // §800: the members AS PRINTED, so a re-mint does not have to go back
        // through the symbol road and widen what this literal retained.
        self.object_literal_members.insert(minted, members.clone());
        // §539: keyed by the minted type id, so the element-access lookup
        // reaches the signature this literal prints.
        if let Some(info) = minted_index_info {
            self.object_literal_index_infos.insert(minted, vec![info]);
        }
        minted
    }

    /// Ported from `Checker.isValidSpreadType` (`checker.go:13504`).
    /// Filter definitely falsy alternatives only after resolving base constraints;
    /// a primitive that can be truthy still makes the operand invalid.
    fn is_valid_spread_type(&mut self, source: TypeId) -> bool {
        let constrained = if let TypeData::Union { types, .. } = self.store.get(source).data.clone()
        {
            let types: Vec<_> = types
                .into_iter()
                .map(|ty| self.base_constraint_of_type(ty).unwrap_or(ty))
                .collect();
            self.get_union_type(&types)
        } else {
            self.base_constraint_of_type(source).unwrap_or(source)
        };
        let source = self.remove_definitely_falsy_types(constrained);
        if self.store.get(source).flags.intersects(
            TypeFlags::ANY
                | TypeFlags::NON_PRIMITIVE
                | TypeFlags::OBJECT
                | TypeFlags::INSTANTIABLE_NON_PRIMITIVE,
        ) {
            return true;
        }
        match self.store.get(source).data.clone() {
            TypeData::Union { types, .. } | TypeData::Intersection { types, .. } => {
                types.into_iter().all(|ty| self.is_valid_spread_type(ty))
            }
            _ => false,
        }
    }

    /// Ported from `Checker.tryMergeUnionOfObjectTypeAndEmptyObject`
    /// (`checker.go:13530`). A union with one nonempty object spreads as a
    /// partial object. Multiple nonempty alternatives still need distribution.
    fn try_merge_union_of_object_type_and_empty_object(&mut self, source: TypeId) -> TypeId {
        let TypeData::Union { types, .. } = self.store.get(source).data.clone() else {
            return source;
        };
        let mut nonempty = None;
        let mut empty = None;
        for ty in types {
            if self.is_empty_anonymous_object_type(ty) {
                empty = Some(ty);
            } else if !self.store.get(ty).flags.intersects(
                TypeFlags::NULLABLE
                    | TypeFlags::BOOLEAN_LIKE
                    | TypeFlags::NUMBER_LIKE
                    | TypeFlags::BIG_INT_LIKE
                    | TypeFlags::STRING_LIKE
                    | TypeFlags::ENUM_LIKE
                    | TypeFlags::NON_PRIMITIVE
                    | TypeFlags::INDEX,
            ) {
                if nonempty.is_some_and(|previous| previous != ty) {
                    return source;
                }
                nonempty = Some(ty);
            }
        }
        let Some(first) = nonempty else {
            return empty.unwrap_or(self.intrinsics.empty_object);
        };
        let Some(members) = self.spread_members_of(first) else { return source };
        let Some(infos) = self.get_index_infos_of_type(first) else { return source };
        // The current spread-member representation cannot carry index infos.
        if !infos.is_empty() {
            return source;
        }
        let mut properties = Vec::with_capacity(members.len());
        let mut partial_members = Vec::with_capacity(members.len());
        for member in members {
            let Member::Property { name, .. } = member else { return source };
            let semantic_name = self
                .anonymous_properties
                .get(&first)
                .and_then(|(properties, _)| {
                    properties.iter().find(|property| property.printed_name == name)
                })
                .map_or_else(|| name.clone(), |property| property.name.clone());
            let Some(value) = self.get_type_of_property_of_type(first, &semantic_name) else {
                return source;
            };
            let value =
                if self.strict_null_checks { self.get_optional_type(value, true) } else { value };
            let displayed = self.remove_missing_type(value);
            let printed = self.type_to_string(displayed);
            properties.push(AnonymousProperty {
                name: semantic_name,
                printed_name: name.clone(),
                printed_type: printed.clone(),
                optional: true,
                readonly: false,
                r#type: value,
            });
            partial_members.push(Member::Property {
                name,
                optional: true,
                readonly: false,
                printed,
            });
        }
        let owner = match self.store.get(first).data {
            TypeData::Named { members, .. } => members,
            _ => None,
        };
        let result =
            self.store.new_named(TypeFlags::OBJECT, render_object_type(&partial_members), owner);
        self.anonymous_properties.insert(result, (properties, true));
        self.object_literal_members.insert(result, partial_members);
        self.object_literal_spread_flags.insert(result, false);
        result
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
    /// - **A member whose own type gaps.** Nullable member values are retained;
    ///   the declaration's widening path now handles their later widening.
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
    pub(crate) fn spread_members_of(&mut self, source: TypeId) -> Option<Vec<Member>> {
        let error = self.intrinsics.error;
        if source == error {
            return None;
        }
        if let Some((properties, true)) = self.anonymous_properties.get(&source) {
            return Some(
                properties
                    .iter()
                    .map(|property| Member::Property {
                        name: property.printed_name.clone(),
                        optional: property.optional,
                        readonly: false,
                        printed: property.printed_type.clone(),
                    })
                    .collect(),
            );
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
            if member_type == error {
                return None;
            }
            let optional = self.property_is_optional(member);
            let displayed = self.remove_missing_type(member_type);
            spread.push(Member::Property {
                name,
                optional,
                readonly: false,
                printed: self.type_to_string(displayed),
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
    ///
    /// # §890: all three branches
    ///
    /// The doc above described the third branch with `nil` for the contextual
    /// type, which is what this function used to be. Upstream has three, and the
    /// first two are the ones that *keep* a literal:
    ///
    /// ```go
    /// case c.isConstContext(node):   return c.getRegularTypeOfLiteralType(t)
    /// case isTypeAssertion(node):    return t
    /// default:                       return c.getWidenedLiteralLikeTypeForContextualType(t,
    ///     c.instantiateContextualType(c.getContextualType(node, …), node, …))
    /// ```
    ///
    /// With a real contextual type the third branch keeps a literal too:
    /// `{ largestUnit: "hour" }` against a parameter
    /// `{ largestUnit: "hour" | "minute" }` records `{ largestUnit: "hour"; }`.
    ///
    /// `isTypeAssertion` is `IsAssertionExpression(SkipParentheses(node))`
    /// (`utilities.go:347`) — an `as` or angle-bracket assertion, reached
    /// through parentheses.
    ///
    /// **`instantiateContextualType` is not ported**, so the raw contextual type
    /// is passed. An undecidable `is_literal_of_contextual_type` (`None`) widens,
    /// which is this function's previous behaviour: the tri-state's other callers
    /// keep a gap instead, but there is no gap to keep here and declining would
    /// print `error` where a widened literal is at worst a near miss.
    pub(crate) fn check_expression_for_mutable_location(
        &mut self,
        expression: tsr_ast::Expression<'_>,
    ) -> TypeId {
        use crate::flags::TypeFlags as TF;
        let id = self.check_expression(expression);
        let node_id = tsr_ast::Node::from(expression).node_id();
        if node_id.is_some_and(|node| self.is_const_context(node)) {
            return self.get_regular_type_of_literal_type(id);
        }
        if expression_is_a_type_assertion(expression) {
            return id;
        }
        // Ask for the contextual type ONLY when the candidate could possibly
        // keep a literal. `is_literal_of_contextual_type` answers `Some(true)`
        // solely through a `maybe_type_of_kind(candidate, …literal…)` conjunct,
        // so for a candidate with no literal-flavoured constituent the result is
        // `Some(false)` or `None`, and both widen. **The short-circuit therefore
        // cannot change an answer** — it only skips a call whose result is
        // discarded.
        //
        // It is not an optimisation. `get_contextual_type` on an object-literal
        // member re-enters contextual signature resolution, and where that
        // re-entry hits an in-flight guard it answers `any`. The first build
        // measured 26 `RIGHT→WRONG` from exactly that — `bbb : () => void`,
        // `tag : string` and `value : number` collapsing to `any` in
        // `thislessFunctionsNotContextSensitive1/2` — on members that could
        // never have kept a literal anyway. Upstream computes the contextual
        // type unconditionally because its resolution is re-entrant-safe; this
        // port's is not yet, and the honest way to say so is to not ask a
        // question whose answer is already determined.
        let literalish = TF::STRING_LITERAL
            .union(TF::NUMBER_LITERAL)
            .union(TF::BIG_INT_LITERAL)
            .union(TF::BOOLEAN_LITERAL)
            .union(TF::UNIQUE_ES_SYMBOL);
        // isConstContext also applies to primitive literal initializers; an
        // indexed const parameter can supply this context without making the
        // enclosing object itself a homomorphic const target.
        if self.maybe_type_of_kind(id, literalish)
            && node_id.is_some_and(|node| self.literal_in_const_type_variable_context(node))
        {
            return self.get_regular_type_of_literal_type(id);
        }
        // §890's call-argument exclusion, kept. §892 predicted the cache would
        // retire it and **measured that it does not**: the recompute was never
        // the only entry. The probe for member `x` runs *while* `x`'s type is
        // being computed, so a cache written *after* that computation cannot be
        // read by it — circular by construction. Removing the exclusion on top
        // of §892 measures 122 W→R / 28 R→W, with the same 22
        // `thislessFunctionsNotContextSensitive2` rows §890 declined.
        // §910: the exclusion narrowed from "anywhere under a call" to "under a
        // NESTED object literal in a call argument". Every row §890 lost was a
        // member of `context: { tag: "A", value: 1 }` — a literal INSIDE the
        // argument literal — and the re-entry needs that second level: checking
        // the inner literal asks for its contextual type, which asks for the
        // outer literal's, which is the argument whose signature is being
        // resolved. A member of the argument literal ITSELF is one hop short of
        // the cycle.
        let in_call_argument = node_id.is_some_and(|n| {
            let mut seen_literal = false;
            for ancestor in self.nodes.ancestors(n) {
                match self.nodes.kind(ancestor) {
                    tsr_ast::SyntaxKind::ObjectLiteralExpression => {
                        if seen_literal {
                            // A second enclosing literal: this member is nested.
                            return self.nodes.ancestors(ancestor).any(|a| {
                                matches!(
                                    self.nodes.kind(a),
                                    tsr_ast::SyntaxKind::CallExpression
                                        | tsr_ast::SyntaxKind::NewExpression
                                )
                            });
                        }
                        seen_literal = true;
                    }
                    tsr_ast::SyntaxKind::CallExpression | tsr_ast::SyntaxKind::NewExpression => {
                        return false;
                    }
                    _ => {}
                }
            }
            false
        });
        let keeps_literal = if !in_call_argument && self.maybe_type_of_kind(id, literalish) {
            let first = node_id
                .and_then(|node| self.get_contextual_type(node))
                .and_then(|contextual| self.is_literal_of_contextual_type(id, contextual));
            // §946: upstream's two-pass argument check, for the FRESHNESS
            // question only. Pass one reads the parameter type as WRITTEN, where
            // `isLiteralOfContextualType` sees a type variable and consults its
            // primitive constraint. This port has a single pass, so by the time
            // the question is asked the fixing mapper has already replaced `A`
            // with `unknown` — and `nested({ fields: "z" })` widened `"z"` to
            // `string`, so inference never saw the literal at all (§937.1's
            // finding, whose reopening condition this is).
            if first == Some(true) {
                first
            } else {
                let saved = self.contextual_prefers_uninstantiated;
                self.contextual_prefers_uninstantiated = true;
                let retried = node_id
                    .and_then(|node| self.get_contextual_type(node))
                    .and_then(|contextual| self.is_literal_of_contextual_type(id, contextual));
                self.contextual_prefers_uninstantiated = saved;
                if retried == Some(true) { retried } else { first }
            }
        } else {
            Some(false)
        };
        if keeps_literal == Some(true) {
            return self.get_regular_type_of_literal_type(id);
        }
        // §898: upstream's pair — `getWidenedUniqueESSymbolType(getWidenedLiteralType(t))`
        // (`checker.go:25517`). A `unique symbol` widens to plain `symbol` at a
        // mutable location, which is the only place upstream calls it.
        let widened = self.get_widened_literal_type(id);
        let widened = self.get_widened_unique_es_symbol_type(widened);
        self.get_regular_type_of_literal_type(widened)
    }
}

/// `isTypeAssertion` (`utilities.go:347`):
/// `ast.IsAssertionExpression(ast.SkipParentheses(node))`. A free function
/// because it reads no checker state.
fn expression_is_a_type_assertion(expression: tsr_ast::Expression<'_>) -> bool {
    let mut current = expression;
    loop {
        match current {
            tsr_ast::Expression::AsExpression(_) | tsr_ast::Expression::TypeAssertion(_) => {
                return true;
            }
            tsr_ast::Expression::ParenthesizedExpression(inner) => match inner.expression {
                Some(inner) => current = inner,
                None => return false,
            },
            _ => return false,
        }
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
pub(crate) fn is_identifier_text(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else { return false };
    (first.is_ascii_alphabetic() || first == '_' || first == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// §489: the pattern element a literal member's name matches — the syntactic
/// half of upstream's `getPropertyOfType(contextualType, member.Name)` over an
/// implied binding-pattern type. A pattern carrying a COMPUTED element name is
/// upstream's `ObjectLiteralPatternWithComputedProperties`, which disables the
/// optionality copy whole (`checker.go:13253`'s second conjunct), so it
/// answers `None` for every member.
fn matching_pattern_element<'a>(
    pattern: &'a tsr_ast::BindingPattern<'a>,
    name: &tsr_ast::PropertyName<'_>,
) -> Option<&'a tsr_ast::BindingElement<'a>> {
    let member_name = property_name_text(name)?;
    if pattern.elements.iter().any(|element| {
        matches!(element.property_name, Some(tsr_ast::PropertyName::ComputedPropertyName(_)))
    }) {
        return None;
    }
    pattern.elements.iter().copied().find(|element| {
        element.dot_dot_dot_token.is_none()
            && element_name_text(element).is_some_and(|text| text == member_name)
    })
}

/// §897: the member of an ASSIGNMENT pattern that a written name matches.
/// `matching_pattern_element`'s mirror for object-literal members.
fn matching_assignment_member<'a>(
    pattern: &'a tsr_ast::ObjectLiteralExpression<'a>,
    name: &tsr_ast::PropertyName<'_>,
) -> Option<&'a tsr_ast::ObjectLiteralElementLike<'a>> {
    let member_name = property_name_text(name)?;
    pattern.properties.iter().find(|property| match property {
        tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) => {
            property_name_text(&assignment.name).is_some_and(|text| text == member_name)
        }
        tsr_ast::ObjectLiteralElementLike::ShorthandPropertyAssignment(shorthand) => {
            property_name_text(&shorthand.name).is_some_and(|text| text == member_name)
        }
        _ => false,
    })
}

/// §897: the nested assignment pattern a member's value is, if it is one —
/// either `{ a: { b } }` or `{ a: { b } = d }`, whose left is the pattern.
fn assignment_member_pattern<'a>(
    member: &'a tsr_ast::ObjectLiteralElementLike<'a>,
) -> Option<&'a tsr_ast::ObjectLiteralExpression<'a>> {
    let tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) = member else {
        return None;
    };
    match assignment.initializer? {
        tsr_ast::Expression::ObjectLiteralExpression(inner) => Some(inner),
        tsr_ast::Expression::BinaryExpression(binary)
            if binary
                .operator_token
                .is_some_and(|t| t.kind == tsr_ast::SyntaxKind::EqualsToken) =>
        {
            match binary.left {
                Some(tsr_ast::Expression::ObjectLiteralExpression(inner)) => Some(inner),
                _ => None,
            }
        }
        _ => None,
    }
}

/// §897: whether the assignment pattern's matching member writes a DEFAULT,
/// which is what makes the implied property optional (`checker.go:13248`).
fn assignment_pattern_member_is_optional(
    pattern: &tsr_ast::ObjectLiteralExpression<'_>,
    name: &tsr_ast::PropertyName<'_>,
) -> bool {
    matching_assignment_member(pattern, name).is_some_and(|member| match member {
        tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) => matches!(
            assignment.initializer,
            Some(tsr_ast::Expression::BinaryExpression(binary))
                if binary.operator_token.is_some_and(|t| t.kind == tsr_ast::SyntaxKind::EqualsToken)
        ),
        tsr_ast::ObjectLiteralElementLike::ShorthandPropertyAssignment(shorthand) => {
            shorthand.object_assignment_initializer.is_some()
        }
        _ => false,
    })
}

/// §489: whether the implied property a member matches is OPTIONAL — which
/// `getTypeFromObjectBindingPattern` (`checker.go:17938`) answers exactly
/// where the element writes a default.
fn implied_pattern_member_is_optional(
    pattern: &tsr_ast::BindingPattern<'_>,
    name: &tsr_ast::PropertyName<'_>,
) -> bool {
    matching_pattern_element(pattern, name).is_some_and(|element| element.initializer.is_some())
}

/// How a **written** property name PRINTS as a member name — upstream's
/// `symbolToString` spelling.
///
/// **Called by the `TypeLiteral` road only.** The object-literal road above
/// (`:1132`) still carries the original inline arms this was extracted from;
/// they are the same four answers, and this is deliberately their copy rather
/// than their replacement. Wiring that road through here means restructuring a
/// match whose remaining arms `continue` and contribute index signatures, on a
/// road with no measured defect — churn on working code for no conversion.
///
/// Stated rather than done silently, because a reader is entitled to know the
/// two copies exist. **If they ever disagree, this one is the copy to delete**:
/// the object-literal road's arms are the ones with the corpus behind them.
/// Both call `printing::quote` and `printing::normalise_number`, so the escape
/// table the `:1166` comment warns about is still single-sourced — what is
/// duplicated is the four-way dispatch, not the table.
///
/// Answers `None` for a computed or template name and for the parser's empty
/// recovery identifier: those are not name-rendering questions, and each
/// caller already owns a different answer for them (`continue`, a late-bound
/// lookup, an index contribution). Returning `None` rather than guessing is
/// what keeps that ownership with the caller.
///
/// # Why this is one function and not two
///
/// §584: `get_type_from_type_literal` (`declared.rs:1019`) declined the WHOLE
/// literal for any non-identifier `PropertySignature` name, so
/// `var a: { 1: number }` printed `any` — 18 corpus cases, each exactly one
/// line from passing. The object-literal road (`:1155`) had already solved the
/// identical question. Duplicating its three string arms would have put the
/// single-quote rule (§77.3) and the `normalise_number` call in two places
/// that must agree, which is the divergence the re-quoting comment at `:1166`
/// argues against for the escape table itself.
pub(crate) fn written_property_name(name: &tsr_ast::PropertyName<'_>) -> Option<String> {
    match name {
        // The parser's recovery placeholder — an `Identifier` with no text.
        // See `:1145`: printing it yields `{ : any; }`, a shape no compiler
        // emits.
        tsr_ast::PropertyName::Identifier(identifier) if identifier.text.is_empty() => None,
        tsr_ast::PropertyName::Identifier(identifier) => Some(identifier.text.to_string()),
        // Unquoted when the name is a valid identifier, re-quoted otherwise:
        // `{ "a": 1 }` is `{ a: number; }` and `{ "a-b": 1 }` is
        // `{ "a-b": number; }`.
        tsr_ast::PropertyName::StringLiteral(literal) if is_identifier_text(literal.text) => {
            Some(literal.text.to_string())
        }
        // §77.3: a SINGLE-quoted written name keeps its quote — `{ '1.0': "" }`
        // prints `{ '1.0': string; }`. Only on the re-quoted arm; an
        // identifier-valid name prints bare whichever quote wrote it.
        tsr_ast::PropertyName::StringLiteral(literal)
            if literal.token_flags.contains(tsr_ast::TokenFlags::SINGLE_QUOTE) =>
        {
            Some(format!("'{}'", literal.text))
        }
        tsr_ast::PropertyName::StringLiteral(literal) => Some(printing::quote(literal.text)),
        // A numeric name prints as its NORMALISED value, unquoted: `{ 1.0: x }`
        // prints `1`. `normalise_number` is the same function the numeric
        // literal TYPE prints through, which is what stops `{ 1e3: x }`
        // printing `1e3` here and `1000` there.
        tsr_ast::PropertyName::NumericLiteral(literal) => {
            Some(printing::normalise_number(literal.text))
        }
        _ => None,
    }
}

/// The literal text a property name binds under, for the §489 match. Computed
/// and template names answer `None` — the callers decline there.
fn property_name_text<'a>(name: &tsr_ast::PropertyName<'a>) -> Option<&'a str> {
    match name {
        tsr_ast::PropertyName::Identifier(identifier) => Some(identifier.text),
        tsr_ast::PropertyName::StringLiteral(literal) => Some(literal.text),
        tsr_ast::PropertyName::NumericLiteral(literal) => Some(literal.text),
        _ => None,
    }
}

/// The name a binding element matches members under: its written property
/// name, else its own binding identifier (`{x = 1}` binds and matches `x`).
fn element_name_text<'a>(element: &tsr_ast::BindingElement<'a>) -> Option<&'a str> {
    if let Some(property_name) = &element.property_name {
        return property_name_text(property_name);
    }
    match element.name {
        Some(tsr_ast::BindingName::Identifier(identifier)) => Some(identifier.text),
        _ => None,
    }
}
