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

pub(crate) use property_slot::{PrintedSlot, PropertySlot};

mod property_slot {
    use tsr_binder::SymbolId;

    use crate::types::TypeId;

    /// An [`super::AnonymousProperty`]'s type slot and its publication state.
    /// Private: every reader goes through
    /// [`crate::checker::Checker::property_type`] (or, for a `&self`
    /// structural walk, [`crate::checker::Checker::peek_property_type`]), the
    /// port of native `getTypeOfSymbol` on the property symbol, so the slot's
    /// publication state is decided in one place.
    ///
    /// Native `checkObjectLiteral` (`checker.go:13144`) puts an object-literal
    /// accessor's own symbol in the member table and only defers a check of
    /// its declaration (`checkNodeDeferred`, `checker.go:13315`); the type is
    /// `getTypeOfAccessors` (`checker.go:18511`) on the first read. The owner
    /// of that answer is the accessor symbol's `symbol_types` entry, so an
    /// [`Slot::Accessor`] holds no copy and costs no work until read.
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct PropertySlot(Slot);

    #[derive(Clone, Copy, Debug)]
    pub(super) enum Slot {
        /// A resolved property type.
        Resolved(TypeId),
        /// An object-literal accessor whose type was not read when the literal
        /// was built, because a resolution the getter body can re-enter was in
        /// progress (the accessor's own, or a variable whose initializer holds
        /// the literal) — a re-entry native never makes. Read on demand.
        Accessor(SymbolId),
    }

    impl PropertySlot {
        /// A resolved property type.
        pub(crate) fn resolved(r#type: TypeId) -> Self {
            Self(Slot::Resolved(r#type))
        }

        /// The type of an object-literal accessor symbol, read on demand.
        pub(crate) fn of_accessor(symbol: SymbolId) -> Self {
            Self(Slot::Accessor(symbol))
        }

        /// The stored slot, for the canonical accessors only.
        pub(super) fn get(self) -> Slot {
            self.0
        }
    }

    /// An [`super::AnonymousProperty`]'s printed type. Private: every reader
    /// goes through [`crate::checker::Checker::property_printed_type`], the
    /// node builder's `serializeTypeForDeclaration` of the property type.
    /// `None` is a slot whose type is read on demand
    /// ([`PropertySlot::of_accessor`]); it is printed from that type.
    #[derive(Clone, Debug)]
    pub(crate) struct PrintedSlot(Option<String>);

    impl PrintedSlot {
        /// Text printed by the property's producer.
        pub(crate) fn printed(text: String) -> Self {
            Self(Some(text))
        }

        /// No producer text: printed from the type read on demand.
        pub(crate) fn on_demand() -> Self {
            Self(None)
        }

        /// The stored text, for the canonical accessor only.
        pub(super) fn get(&self) -> Option<&str> {
            self.0.as_deref()
        }
    }
}

/// An anonymous object's typed properties and method flags, retained for
/// `instantiateAnonymousType` and `instantiateSymbol` (checker.go).
#[derive(Clone)]
pub(crate) struct AnonymousProperty {
    /// Setter parameter retained only while the native accessor symbol is reused.
    pub(crate) accessor_write: Option<crate::signatures::Parameter>,
    /// Native Method flag; provenance alone cannot distinguish a copied method
    /// from a synthesized property originating at that method's declaration.
    pub(crate) method: bool,
    /// First declaration provenance used by getNamedMembers/compareSymbols.
    /// A merged optional spread keeps the left origin, independently of its type.
    pub(crate) origin: Option<SymbolId>,
    /// Pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb,
    /// `checkObjectLiteral` (checker.go:13225-13332): the assignment
    /// actually checked for the surviving property, not its merged symbol's
    /// first value declaration. Together with name/origin this is the producer's
    /// display-slot key. Only the original property writer supplies it, in the
    /// typed/display upsert; the completed image's `TypeId` owns publication.
    /// Regular/widened clones retain it as member types transform. Synthetic
    /// or reconstructed images leave it absent; it never enters an intern key.
    /// This private Checker metadata certifies node-builder source reuse only;
    /// recording/copying it forces no signature or member work.
    pub(crate) checked_declaration: Option<tsr_ast::NodeId>,
    pub(crate) name: String,
    pub(crate) printed_name: String,
    /// Read only through [`crate::checker::Checker::property_printed_type`].
    pub(crate) printed_slot: PrintedSlot,
    pub(crate) optional: bool,
    pub(crate) readonly: bool,
    /// Read only through [`crate::checker::Checker::property_type`].
    pub(crate) slot: PropertySlot,
}

impl AnonymousProperty {
    /// Whether the type slot is read on demand ([`PropertySlot::of_accessor`]).
    pub(crate) fn reads_on_demand(&self) -> bool {
        matches!(self.slot.get(), property_slot::Slot::Accessor(_))
    }
}

/// The text an on-demand accessor member bakes into its literal's
/// print-at-creation display. Its type is unread at the mint; the site
/// renderer (`object_literal_text_at`) prints the read type instead. `any` is
/// native's own spelling for the one shape that reaches it — the literal's
/// image re-entered through its accessor, which the node builder elides to
/// `any` (`createAnonymousTypeNode`'s visited check).
const DEFERRED_ACCESSOR_TEXT: &str = "any";

impl Checker<'_, '_> {
    /// The type of one anonymous-object property — the canonical reader of an
    /// [`AnonymousProperty`]'s slot: native `getTypeOfSymbol` on the property
    /// symbol (`checker.go:16493`), which every native consumer reaches
    /// through the member table.
    pub(crate) fn property_type(&mut self, property: &AnonymousProperty) -> TypeId {
        self.property_slot_type(property.slot)
    }

    /// [`Checker::property_type`] of a bare slot. An accessor slot asks
    /// `getTypeOfSymbol` (→ `getTypeOfAccessors`) at the read: the work runs
    /// once, in the accessor symbol's frame, and a read while that frame is
    /// still active closes native's cycle exactly where native's read would.
    pub(crate) fn property_slot_type(&mut self, slot: PropertySlot) -> TypeId {
        match slot.get() {
            property_slot::Slot::Resolved(r#type) => r#type,
            property_slot::Slot::Accessor(symbol) => self.get_type_of_symbol(symbol),
        }
    }

    /// The type of one anonymous-object property for a read that cannot
    /// resolve — a `&self` structural walk over completed types. `None` is an
    /// accessor slot whose symbol type is not yet published; such a walk
    /// follows no edge for it.
    pub(crate) fn peek_property_type(&self, property: &AnonymousProperty) -> Option<TypeId> {
        match property.slot.get() {
            property_slot::Slot::Resolved(r#type) => Some(r#type),
            property_slot::Slot::Accessor(symbol) => self.symbol_types.get(&symbol).copied(),
        }
    }

    /// The printed type of one anonymous-object property — the canonical
    /// reader of [`AnonymousProperty`]'s printed slot. An on-demand slot
    /// prints its type read at this point.
    pub(crate) fn property_printed_type<'p>(
        &mut self,
        property: &'p AnonymousProperty,
    ) -> std::borrow::Cow<'p, str> {
        if let Some(text) = property.printed_slot.get() {
            std::borrow::Cow::Borrowed(text)
        } else {
            let r#type = self.property_type(property);
            std::borrow::Cow::Owned(self.type_to_string(r#type))
        }
    }

    /// Whether an object-literal accessor's type is left to its first reader
    /// ([`PropertySlot::of_accessor`]) instead of being read while the literal
    /// is built.
    ///
    /// Native `checkObjectLiteral` never reads it (`checkNodeDeferred`,
    /// `checker.go:13315`); this port reads it to print the member, which is
    /// indistinguishable except when the getter body re-enters a resolution
    /// in progress: the accessor's own type, or the type of a variable whose
    /// initializer holds this literal, as in
    /// `const a = { get self() { return a; } }` (native reads `self` only
    /// after `a` is published). Only an accessor with no annotation can be
    /// there: its type is the getter body's (`getTypeOfAccessors`,
    /// `checker.go:18511`). Its member text is then a placeholder that the
    /// site renderer (`object_literal_text_at`) prints from the type read
    /// at that point.
    pub(crate) fn accessor_type_deferred(&self, symbol: SymbolId) -> bool {
        let declarations = &self.binder.symbols().get(symbol).declarations;
        !self.symbol_types.contains_key(&symbol)
            && (self.resolutions.on_stack(symbol, crate::resolution::PropertyName::Type)
                || declarations
                    .iter()
                    .any(|&declaration| self.enclosing_variable_resolving(declaration)))
            && declarations
                .iter()
                .all(|&declaration| self.accessor_annotation(declaration).is_none())
    }

    /// Whether a variable declaration enclosing `node` has its type resolving.
    pub(crate) fn enclosing_variable_resolving(&self, node: tsr_ast::NodeId) -> bool {
        let mut parent = self.nodes.parent(node);
        while let Some(node) = parent {
            if self.nodes.kind(node) == tsr_ast::SyntaxKind::VariableDeclaration
                && let Some(symbol) = self.binder.symbol_of(node)
                && self.resolutions.on_stack(symbol, crate::resolution::PropertyName::Type)
            {
                return true;
            }
            parent = self.nodes.parent(node);
        }
        false
    }
}

/// One rendered member of a structural object type.
///
/// A property has a name and a
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
    /// A named method retains its key for object-literal duplicate replacement.
    Method { name: String, printed: String },
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
            Member::Signature { printed: text } | Member::Method { printed: text, .. } => {
                printed.push_str(text);
            }
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
pub(crate) fn signature_member_text(
    checker: &mut Checker<'_, '_>,
    signature: &Signature,
) -> String {
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
        // The node-reuse rule on `Parameter::written_text`
        // (`crate::node_reuse`), for a printer with no print site.
        let error = checker.intrinsics.error;
        let parameter_type = checker.parameter_type(parameter);
        match parameter
            .written_text
            .as_ref()
            .and_then(|written| written.site_free_text(parameter_type, error))
        {
            Some(written) => out.push_str(written),
            None => out.push_str(&checker.type_to_string(parameter_type)),
        }
    }
    out.push_str("): ");
    // The member spelling of the same rule `Checker::signature_to_string`
    // carries: `serializeReturnTypeForSignature` consults the predicate before
    // the return type (`nodebuilderimpl.go:1748`), whichever signature-shaped
    // node the builder is filling. `interface I { m(): this is S[]; }` is the
    // form that needs it, and the corpus records it on lib's `every`.
    let written_return = signature
        .written_return
        .as_ref()
        .and_then(|written| written.site_free_text(signature.r#type, checker.intrinsics.error));
    match (&signature.predicate, written_return) {
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
    /// A string/number literal or unique symbol supplies a named member.
    LateBound,
    /// Invalid key: no named member or new index, but its value can still
    /// contribute to an index requested by another member.
    Nothing,
    /// The member contributes an index signature of this key kind.
    Index(&'static str),
}

/// Entity-name expression spelling used by computed member serialization.
pub(crate) fn entity_name_expression_text(expression: &tsr_ast::Expression<'_>) -> Option<String> {
    match expression {
        tsr_ast::Expression::Identifier(identifier) => Some(identifier.text.to_string()),
        tsr_ast::Expression::PropertyAccessExpression(access) => {
            let base = entity_name_expression_text(access.expression.as_ref()?)?;
            let Some(tsr_ast::MemberName::Identifier(name)) = access.name else {
                return None;
            };
            Some(format!("{base}.{}", name.text))
        }
        _ => None,
    }
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
                        accessor_write: None,
                        method: symbol.is_some_and(|symbol| {
                            self.binder.symbols().get(symbol).flags.contains(SymbolFlags::METHOD)
                        }),
                        origin: symbol,
                        checked_declaration: None,
                        printed_name: name.clone(),
                        printed_slot: PrintedSlot::printed(self.type_to_string(ty)),
                        name,
                        slot: PropertySlot::resolved(ty),
                        optional: symbol.is_some_and(|symbol| self.property_is_optional(symbol)),
                        readonly: symbol.is_some_and(|symbol| self.is_readonly_property(symbol)),
                    })
                })
                .collect();
        }
        if let Some(mut properties) = properties {
            for property in &mut properties {
                // transformTypeOfMembers reads getTypeOfSymbol of every member;
                // an accessor's type is its getter's widened return, which
                // regularization leaves unchanged, so the on-demand slot is
                // carried over unread instead of being forced at the copy.
                if property.reads_on_demand() {
                    continue;
                }
                let r#type = self.property_type(property);
                property.slot =
                    PropertySlot::resolved(self.get_regular_type_of_object_literal(r#type));
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
            crate::types::TypeData::StringLiteral(text)
            | crate::types::TypeData::EnumLiteral {
                value: crate::types::EnumLiteralValue::String(text),
                ..
            } => {
                let text = text.clone();
                return Some((
                    if is_identifier_text(&text) { text } else { printing::quote(&text) },
                    true,
                ));
            }
            crate::types::TypeData::NumberLiteral(text)
            | crate::types::TypeData::EnumLiteral {
                value: crate::types::EnumLiteralValue::Number(text),
                ..
            } => {
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
        let text = entity_name_expression_text(&expression)?;
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
        let name_type = self.base_constraint_of_type(name_type).unwrap_or(name_type);
        let allowed = self.get_union_type(&[
            self.intrinsics.string,
            self.intrinsics.number,
            self.intrinsics.es_symbol,
        ]);
        if !self.is_type_assignable_to(name_type, allowed) {
            return ComputedNameKey::Nothing;
        }
        if self.is_type_assignable_to(name_type, self.intrinsics.number) {
            ComputedNameKey::Index("number")
        } else if self.is_type_assignable_to(name_type, self.intrinsics.es_symbol) {
            ComputedNameKey::Index("symbol")
        } else {
            ComputedNameKey::Index("string")
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
            tsr_ast::Node::ParameterDeclaration(declaration) => {
                if declaration.r#type.is_some()
                    || declaration.initializer.and_then(|initializer| initializer.node_id())
                        != Some(literal)
                {
                    return None;
                }
                let function = self.nodes.parent(parent)?;
                if self.immediately_invoked_call(function).is_some()
                    || !matches!(
                        self.nodes.kind(function),
                        tsr_ast::SyntaxKind::FunctionDeclaration
                            | tsr_ast::SyntaxKind::MethodDeclaration
                    ) && !self.has_no_contextual_type(function)
                {
                    return None;
                }
                let Some(tsr_ast::BindingName::BindingPattern(pattern)) = declaration.name else {
                    return None;
                };
                let tsr_ast::Node::ObjectLiteralExpression(node) = self.node_map.get(literal)?
                else {
                    return None;
                };
                (self.nodes.kind(pattern.node_id?) == tsr_ast::SyntaxKind::ObjectBindingPattern
                    && self.binding_default_pattern_context_available(
                        pattern,
                        tsr_ast::Expression::ObjectLiteralExpression(node),
                    ))
                .then_some(pattern)
            }
            tsr_ast::Node::BindingElement(element) => {
                if element.initializer.and_then(|initializer| initializer.node_id())
                    != Some(literal)
                {
                    return None;
                }
                // getContextualTypeForBindingElement (checker.go:29583)
                // projects an annotation/initializer/contextual parent before
                // the implied-pattern fallback. Certify only a parameter
                // whose parent has none of those sources; never replace a
                // present source's property type with the implied pattern.
                let root = self.root_declaration_of(parent);
                let tsr_ast::Node::ParameterDeclaration(declaration) = self.node_map.get(root)?
                else {
                    return None;
                };
                if declaration.r#type.is_some() || declaration.initializer.is_some() {
                    return None;
                }
                let function = self.nodes.parent(root)?;
                if self.immediately_invoked_call(function).is_some()
                    || !matches!(
                        self.nodes.kind(function),
                        tsr_ast::SyntaxKind::FunctionDeclaration
                            | tsr_ast::SyntaxKind::MethodDeclaration
                    ) && !self.has_no_contextual_type(function)
                {
                    return None;
                }
                let Some(tsr_ast::BindingName::BindingPattern(pattern)) = element.name else {
                    return None;
                };
                let tsr_ast::Node::ObjectLiteralExpression(node) = self.node_map.get(literal)?
                else {
                    return None;
                };
                (self.nodes.kind(pattern.node_id?) == tsr_ast::SyntaxKind::ObjectBindingPattern
                    && self.binding_default_pattern_context_available(
                        pattern,
                        tsr_ast::Expression::ObjectLiteralExpression(node),
                    ))
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
        // checkObjectLiteral does not run the native node builder while collecting
        // members. This port still bakes a placeholder at the mint; a declaration-
        // owned Pending return must not be demanded by that bookkeeping before
        // the object publishes. Actual site rendering later uses semantic slots.
        if self.signature_types.get(&id).is_some_and(|signatures| {
            signatures.iter().any(|signature| {
                signature.target.is_none()
                    && !signature.non_inferrable
                    && self
                        .pending_signature_returns
                        .get(&self.type_literal_key(signature.declaration))
                        == Some(&crate::signatures::LazyReturnState::Pending)
            })
        }) {
            return self.type_to_string(id);
        }
        match reference.and_then(|reference| self.type_to_string_at(id, reference)) {
            Some(text) => text,
            None => self.type_to_string(id),
        }
    }

    /// The pseudochecker's `typeFromExpression` (`pseudochecker/lookup.go:262`)
    /// for a property initializer, narrowed to the arms whose pseudo-type is a
    /// single-quoted string literal node `serializeTypeForDeclaration` reuses:
    /// a string literal in a const location, `'x' as const` (via
    /// `typeFromTypeAssertion`, `lookup.go:510`, which recurses for a const
    /// assertion) and `e as 'x'` / `<'x'>e` (a direct type node, reused when
    /// it denotes the member's type).
    fn reused_single_quoted_literal(
        &mut self,
        expression: tsr_ast::Expression<'_>,
        const_context: bool,
        member_type: TypeId,
    ) -> Option<String> {
        use tsr_ast::{Expression, Node, TokenFlags, TypeNode};
        let (inner, type_node) = match expression {
            Expression::ParenthesizedExpression(parenthesized) => {
                return self.reused_single_quoted_literal(
                    parenthesized.expression?,
                    const_context,
                    member_type,
                );
            }
            Expression::StringLiteral(literal)
                if const_context && literal.token_flags.contains(TokenFlags::SINGLE_QUOTE) =>
            {
                return Some(format!("'{}'", literal.text));
            }
            Expression::AsExpression(assertion) => (assertion.expression?, assertion.r#type?),
            Expression::TypeAssertion(assertion) => (assertion.expression?, assertion.r#type?),
            _ => return None,
        };
        if crate::assertions::is_const_type_reference(type_node) {
            return self.reused_single_quoted_literal(inner, true, member_type);
        }
        let TypeNode::LiteralTypeNode(literal_type) = type_node else { return None };
        let Some(Node::StringLiteral(literal)) = literal_type.literal else { return None };
        if !literal.token_flags.contains(TokenFlags::SINGLE_QUOTE) {
            return None;
        }
        // Read-drop-recurse (ADR-0013): the arena node carries the checker's
        // lifetime that `get_type_from_type_node` needs.
        let Some(Node::LiteralTypeNode(arena)) = self.node_map.get(literal_type.node_id?) else {
            return None;
        };
        if self.get_type_from_type_node(TypeNode::LiteralTypeNode(arena)) != member_type {
            return None;
        }
        Some(format!("'{}'", literal.text))
    }

    pub(crate) fn check_object_literal(&mut self, node: &ObjectLiteralExpression<'_>) -> TypeId {
        let has_spread = node.properties.iter().any(|property| {
            matches!(property, tsr_ast::ObjectLiteralElementLike::SpreadAssignment(_))
        });
        if has_spread {
            return self.check_object_spread_literal(node);
        }
        self.check_object_literal_members(node)
    }

    pub(crate) fn check_object_literal_members(
        &mut self,
        node: &ObjectLiteralExpression<'_>,
    ) -> TypeId {
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
        // the literal source-view builder before inference collects candidates.
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
        let mut capture_complete = true;
        let mut checked_members = Vec::new();
        let property_only = node.properties.iter().all(|property| {
            matches!(
                property,
                tsr_ast::ObjectLiteralElementLike::PropertyAssignment(_)
                    | tsr_ast::ObjectLiteralElementLike::ShorthandPropertyAssignment(_)
                    | tsr_ast::ObjectLiteralElementLike::SpreadAssignment(_)
            )
        });

        // SS325: late-bound ACCESSOR members merge by name - a get/set pair
        // is one property (the getter's type wins the display), a getter
        // without a setter is `readonly` (`symbolDeclarationEmit10`,
        // `symbolProperty5`). Non-accessor members of the same name stay
        // separate rows (`symbolProperty1`'s triple).
        let mut accessor_members: Vec<(String, usize)> = Vec::new();
        for property in node.properties {
            let mut unnamed = false;
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
                // Every spread is folded by check_object_spread_literal;
                // this collector only receives contiguous ordinary batches.
                tsr_ast::ObjectLiteralElementLike::SpreadAssignment(_) => return error,
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
                        capture_complete &= self.capture_checked_object_member(
                            property,
                            const_context,
                            &mut typed_properties,
                            &mut checked_members,
                        );
                        continue;
                    }
                    // SS307: a computed-name METHOD takes the same index route
                    // the property arm built at SS206 - `{ [e]() { } }` with an
                    // untypeable `e` is `{ [x: number]: () => void; }`
                    // (`conformance/parserComputedPropertyName3`). The value is
                    // the method's own function type - the same road that
                    // already prints the member's `.types` line.
                    if let tsr_ast::PropertyName::ComputedPropertyName(computed) = method.name {
                        match self.computed_member_index_key(computed) {
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
                                        "{}{}",
                                        classified_method_name(&name),
                                        signature_member_text(self, &signature)
                                    );
                                    upsert_member(&mut members, Member::Method { name, printed });
                                } else {
                                    let printed = self.signature_to_string(&signature);
                                    members.push(Member::Property {
                                        name,
                                        optional: false,
                                        readonly: const_context,
                                        printed,
                                    });
                                }
                                capture_complete &= self.capture_checked_object_member(
                                    property,
                                    const_context,
                                    &mut typed_properties,
                                    &mut checked_members,
                                );
                                continue;
                            }
                            ComputedNameKey::Nothing | ComputedNameKey::Index(_) => {}
                        }
                        capture_complete &= self.capture_checked_object_member(
                            property,
                            const_context,
                            &mut typed_properties,
                            &mut checked_members,
                        );
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
                        capture_complete &= self.capture_checked_object_member(
                            property,
                            const_context,
                            &mut typed_properties,
                            &mut checked_members,
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
                    let printed = format!("{}{member_text}", classified_method_name(name));
                    upsert_member(&mut members, Member::Method { name: name.to_owned(), printed });
                    capture_complete &= self.capture_checked_object_member(
                        property,
                        const_context,
                        &mut typed_properties,
                        &mut checked_members,
                    );
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
                        if self.accessor_type_deferred(symbol) {
                            upsert_member(
                                &mut members,
                                Member::Property {
                                    name: name.text.to_string(),
                                    optional: false,
                                    readonly: const_context || setter_sibling.is_none(),
                                    printed: DEFERRED_ACCESSOR_TEXT.to_string(),
                                },
                            );
                            capture_complete &= self.capture_checked_object_member(
                                property,
                                const_context,
                                &mut typed_properties,
                                &mut checked_members,
                            );
                            continue;
                        }
                        let member_type = self.get_type_of_symbol(symbol);
                        if member_type == error {
                            return error;
                        }
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
                                capture_complete &= self.capture_checked_object_member(
                                    property,
                                    const_context,
                                    &mut typed_properties,
                                    &mut checked_members,
                                );
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
                        capture_complete &= self.capture_checked_object_member(
                            property,
                            const_context,
                            &mut typed_properties,
                            &mut checked_members,
                        );
                        continue;
                    }
                    let tsr_ast::PropertyName::ComputedPropertyName(computed) = accessor.name
                    else {
                        return error;
                    };
                    let Some(id) = accessor.node_id else { return error };
                    match self.computed_member_index_key(computed) {
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
                                capture_complete &= self.capture_checked_object_member(
                                    property,
                                    const_context,
                                    &mut typed_properties,
                                    &mut checked_members,
                                );
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
                            capture_complete &= self.capture_checked_object_member(
                                property,
                                const_context,
                                &mut typed_properties,
                                &mut checked_members,
                            );
                            continue;
                        }
                        ComputedNameKey::Nothing | ComputedNameKey::Index(_) => {}
                    }
                    capture_complete &= self.capture_checked_object_member(
                        property,
                        const_context,
                        &mut typed_properties,
                        &mut checked_members,
                    );
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
                        if self.accessor_type_deferred(symbol) {
                            upsert_member(
                                &mut members,
                                Member::Property {
                                    name: name.text.to_string(),
                                    optional: false,
                                    readonly: const_context,
                                    printed: DEFERRED_ACCESSOR_TEXT.to_string(),
                                },
                            );
                            capture_complete &= self.capture_checked_object_member(
                                property,
                                const_context,
                                &mut typed_properties,
                                &mut checked_members,
                            );
                            continue;
                        }
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
                                    self.written_annotation_text(annotation)
                                        .unwrap_or_else(|| self.type_to_string(setter_type))
                                );
                                members.push(Member::Signature { printed });
                                capture_complete &= self.capture_checked_object_member(
                                    property,
                                    const_context,
                                    &mut typed_properties,
                                    &mut checked_members,
                                );
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
                        capture_complete &= self.capture_checked_object_member(
                            property,
                            const_context,
                            &mut typed_properties,
                            &mut checked_members,
                        );
                        continue;
                    }
                    let tsr_ast::PropertyName::ComputedPropertyName(computed) = accessor.name
                    else {
                        return error;
                    };
                    let Some(id) = accessor.node_id else { return error };
                    match self.computed_member_index_key(computed) {
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
                                capture_complete &= self.capture_checked_object_member(
                                    property,
                                    const_context,
                                    &mut typed_properties,
                                    &mut checked_members,
                                );
                                continue;
                            }
                            let Some(signature) = self.get_signature_from_declaration(id) else {
                                return error;
                            };
                            let member_type = signature
                                .parameters
                                .first()
                                .map_or(self.intrinsics.any, |parameter| {
                                    self.parameter_type(parameter)
                                });
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
                            capture_complete &= self.capture_checked_object_member(
                                property,
                                const_context,
                                &mut typed_properties,
                                &mut checked_members,
                            );
                            continue;
                        }
                        ComputedNameKey::Nothing | ComputedNameKey::Index(_) => {}
                    }
                    capture_complete &= self.capture_checked_object_member(
                        property,
                        const_context,
                        &mut typed_properties,
                        &mut checked_members,
                    );
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
                        ComputedNameKey::Nothing | ComputedNameKey::Index(_) => {
                            unnamed = true;
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
            if let PropertyValue::Initializer(initializer) = value {
                // Native property function values have a completed callable
                // shape but a lazy original return when no context requests it.
                // Object identity/member publication remains at the normal tail.
                self.defer_object_member_return(initializer);
            }
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
            if let PropertyValue::Initializer(initializer) = value
                && self.is_context_sensitive_argument(&initializer)
                && let Some(id) = initializer.node_id()
            {
                self.add_intra_expression_inference_site(id, member_type);
            }
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
            // Upsert prevents duplicate members while collecting the literal.
            // Final ordering uses surviving declaration provenance below:
            // `{ ...{ a: 1, b: 2 }, a: "x" }` prints `{ b: number; a: string; }`.
            // SS109: a single-quoted string literal the pseudochecker reuses
            // prints single-quoted inside the object type while its
            // standalone line stays double (the SS77.3 name-quote precedent
            // applied to values). Other reaches keep the fresh render.
            let reused = match &value {
                PropertyValue::Initializer(initializer) => {
                    self.reused_single_quoted_literal(*initializer, const_context, member_type)
                }
                PropertyValue::Shorthand(_) => None,
            };
            // §735 — see [`Checker::member_text_at`].
            let printed = reused.unwrap_or_else(|| self.member_text_at(member_type, node.node_id));
            if let Some(id) = property.node_id() {
                checked_members.push((id, PropertySlot::resolved(member_type)));
            } else {
                capture_complete = false;
            }
            if unnamed {
                continue;
            }
            // SS329 corrects SS323's push rule: `symbolProperty1`'s three
            // `[s]` rows come from three DIFFERENT ARMS (property, method,
            // getter), so the plain-property flow upserts for bracketed
            // names exactly as for written ones - duplicate late-bound
            // PROPERTY assignments collapse to one row
            // (`symbolProperty36`'s `{ [Symbol.isConcatSpreadable]: 0,
            // [Symbol.isConcatSpreadable]: 1 }` prints one member).
            let mut replaced_name = None;
            {
                let component = if let tsr_ast::PropertyName::ComputedPropertyName(computed) =
                    name_node
                {
                    self.late_bound_symbol_member_name(computed).is_some_and(|(_, named)| !named)
                } else {
                    false
                };
                let semantic_name = property_node_id
                    .and_then(|id| self.binder.symbol_of(id))
                    .map(|symbol| self.type_literal_member_key(name_node, symbol, &name))
                    .or_else(|| {
                        if let tsr_ast::PropertyName::ComputedPropertyName(computed) = name_node {
                            let key = computed
                                .expression
                                .map(|expression| self.check_expression(expression))?;
                            self.property_name_from_index(key)
                        } else {
                            property_name_text(&name_node).map(str::to_string)
                        }
                    });
                if let Some(semantic_name) = semantic_name.filter(|_| !component) {
                    let mut property = AnonymousProperty {
                        accessor_write: None,
                        method: false,
                        origin: property.node_id().and_then(|id| self.binder.symbol_of(id)),
                        checked_declaration: matches!(
                            property,
                            tsr_ast::ObjectLiteralElementLike::PropertyAssignment(_)
                        )
                        .then(|| property.node_id())
                        .flatten(),
                        name: semantic_name,
                        printed_name: name.clone(),
                        printed_slot: PrintedSlot::printed(printed.clone()),
                        optional: member_optional,
                        readonly: const_context,
                        slot: PropertySlot::resolved(member_type),
                    };
                    if let Some(index) =
                        typed_properties.iter().position(|p| p.name == property.name)
                    {
                        // checkObjectLiteral's `propertiesTable[member.Name] =
                        // member` (`checker.go:13331`) keys by the escaped
                        // name, so `26` and `"26"` are one entry. A computed
                        // entry carries its own `nameType` (`[+1]` prints `1`,
                        // `[-1]` prints `[-1]`); a written name prints from the
                        // binder-merged symbol, whose first spelling wins.
                        let previous = &typed_properties[index];
                        if previous.printed_name != name {
                            if !matches!(name_node, tsr_ast::PropertyName::ComputedPropertyName(_))
                            {
                                property.printed_name.clone_from(&previous.printed_name);
                            }
                            replaced_name = Some((
                                previous.printed_name.clone(),
                                property.printed_name.clone(),
                            ));
                        }
                        typed_properties[index] = property;
                    } else {
                        typed_properties.push(property);
                    }
                } else if !component {
                    capture_complete = false;
                }
            }
            match replaced_name {
                Some((previous, surviving)) => replace_member_named(
                    &mut members,
                    &previous,
                    Member::Property {
                        name: surviving,
                        optional: member_optional,
                        readonly: const_context,
                        printed,
                    },
                ),
                None => upsert_member(
                    &mut members,
                    Member::Property {
                        name,
                        optional: member_optional,
                        readonly: const_context,
                        printed,
                    },
                ),
            }
        }
        let Some(indexes) = self.object_literal_indexes(&checked_members, const_context) else {
            return error;
        };
        let mut index_members = Vec::new();
        for index in &indexes {
            let Some(rendered) = self.index_info_members(index) else {
                return error;
            };
            index_members.extend(rendered);
        }
        // getNamedMembers (checker.go:22047) sorts by the originating
        // declaration, including replaced and merged spread properties.
        if property_only && typed_properties.len() == members.len() {
            typed_properties.sort_by(|left, right| match (left.origin, right.origin) {
                (Some(left), Some(right)) => self.compare_symbols(left, right),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                _ => left.name.cmp(&right.name),
            });
            members = self.property_members(&typed_properties);
        }
        index_members.extend(members);
        let members = index_members;
        let printed = render_object_type(&members);
        // The binder gives an object literal its own `__object` symbol, whose
        // members table is where a property access on this type looks — the same
        // arrangement `get_type_from_type_literal` relies on for `__type`.
        let symbol = node.node_id.and_then(|id| self.binder.symbol_of(id));
        let minted = self.store.new_named(TypeFlags::OBJECT, printed, symbol);
        if capture_complete {
            // Spread properties have no binder symbol on this literal. Use
            // their resolved semantic types for member lookup (getSpreadType).
            let synthetic = node.properties.iter().any(|property| {
                matches!(property, tsr_ast::ObjectLiteralElementLike::SpreadAssignment(_))
            }) || typed_properties.iter().any(|property| {
                property
                    .origin
                    .is_none_or(|origin| self.binder.symbols().get(origin).name != property.name)
            });
            self.anonymous_properties.insert(minted, (typed_properties, synthetic));
        }
        // SS185: upstream's `ObjectFlagsJSLiteral` — an object literal
        // created in a JS FILE is a "JS literal" type, which
        // `getPropertyTypeForIndexType`'s failure path answers `any` for
        // (`checker.go:27130` and `:27189` via `isJSLiteralType`,
        // `utilities.go:1753`). Recorded in a side table per ADR-0003.
        // Only without a contextual type (`contextualType == nil`,
        // `checker.go:13206`): a literal under `@type`/`@satisfies` is checked
        // like a TypeScript one.
        if node
            .node_id
            .is_some_and(|id| self.in_js_file(id) && self.get_contextual_type(id).is_none())
        {
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
        self.object_literal_index_infos.insert(minted, indexes);
        minted
    }

    /// getObjectLiteralIndexInfo (checker.go:19721): filter the complete
    /// propertiesArray independently for each requested primitive key kind.
    fn object_literal_indexes(
        &mut self,
        checked_members: &[(tsr_ast::NodeId, PropertySlot)],
        readonly: bool,
    ) -> Option<Vec<crate::index_signatures::IndexInfo>> {
        let mut needed = [false; 3];
        let mut properties = Vec::with_capacity(checked_members.len());
        for &(declaration, value) in checked_members {
            let name = match self.node_map.get(declaration)? {
                tsr_ast::Node::PropertyAssignment(node) => node.name,
                tsr_ast::Node::ShorthandPropertyAssignment(node) => node.name,
                tsr_ast::Node::MethodDeclaration(node) => node.name,
                tsr_ast::Node::GetAccessorDeclaration(node) => node.name,
                tsr_ast::Node::SetAccessorDeclaration(node) => node.name,
                _ => return None,
            };
            let (symbol, numeric, component) =
                if let tsr_ast::PropertyName::ComputedPropertyName(computed) = name {
                    let key = self.check_expression(computed.expression?);
                    match self.computed_member_index_key(computed) {
                        ComputedNameKey::Index("string") => needed[0] = true,
                        ComputedNameKey::Index("number") => needed[1] = true,
                        ComputedNameKey::Index("symbol") => needed[2] = true,
                        _ => {}
                    }
                    let flags = self.store.get(key).flags;
                    let symbol = flags.intersects(TypeFlags::ES_SYMBOL)
                        || self.is_type_assignable_to(key, self.intrinsics.es_symbol);
                    let numeric = flags.intersects(TypeFlags::NUMBER_LIKE)
                        || self.is_type_assignable_to(key, self.intrinsics.number);
                    (symbol, numeric, Some(declaration))
                } else {
                    let numeric = property_name_text(&name)
                        .is_some_and(crate::index_signatures::is_numeric_literal_name);
                    (false, numeric, None)
                };
            properties.push((symbol, numeric, component, value));
        }
        let mut indexes = Vec::new();
        for (kind, needed) in needed.into_iter().enumerate() {
            if !needed {
                continue;
            }
            let mut values = Vec::new();
            let mut components = Vec::new();
            for &(symbol, numeric, component, value) in &properties {
                if match kind {
                    0 => !symbol,
                    1 => numeric,
                    _ => symbol,
                } {
                    // getObjectLiteralIndexInfo reads getTypeOfSymbol of each
                    // contributing member here, so an accessor slot resolves.
                    values.push(self.property_slot_type(value));
                    components.extend(component);
                }
            }
            let value = if values.is_empty() {
                self.intrinsics.undefined
            } else {
                self.object_literal_index_value(&values)?
            };
            let components = if components.is_empty() {
                None
            } else {
                let id = crate::index_signatures::IndexComponentsId(self.index_components.len());
                self.index_components.push(components);
                Some(id)
            };
            indexes.push(crate::index_signatures::IndexInfo {
                components,
                declaration: None,
                key: [self.intrinsics.string, self.intrinsics.number, self.intrinsics.es_symbol]
                    [kind],
                value,
                readonly,
            });
        }
        Some(indexes)
    }

    /// getObjectLiteralIndexInfo unions component values with subtype reduction.
    fn object_literal_index_value(&mut self, values: &[TypeId]) -> Option<TypeId> {
        self.union_with_subtype_reduction(values)
    }

    /// Retain the already-checked method/accessor value beside ordinary batch
    /// properties. The enclosing binder table also contains other batches and
    /// therefore cannot substitute for this complete semantic table.
    fn capture_checked_object_member(
        &mut self,
        member: &tsr_ast::ObjectLiteralElementLike<'_>,
        readonly: bool,
        properties: &mut Vec<AnonymousProperty>,
        checked_members: &mut Vec<(tsr_ast::NodeId, PropertySlot)>,
    ) -> bool {
        let (name, method) = match member {
            tsr_ast::ObjectLiteralElementLike::MethodDeclaration(node) => (node.name, true),
            tsr_ast::ObjectLiteralElementLike::GetAccessorDeclaration(node) => (node.name, false),
            tsr_ast::ObjectLiteralElementLike::SetAccessorDeclaration(node) => (node.name, false),
            _ => return false,
        };
        if matches!(name, tsr_ast::PropertyName::PrivateIdentifier(_)) {
            return true;
        }
        let Some(symbol) = member.node_id().and_then(|id| self.binder.symbol_of(id)) else {
            return false;
        };
        let Some(id) = member.node_id() else {
            return false;
        };
        // checkObjectLiteralMethod creates a transient property with this
        // declaration's checked type, independently of duplicate binder names.
        let value = if method {
            let Some(signature) = self.get_signature_from_declaration(id) else {
                return false;
            };
            let printed = self.signature_to_string(&signature);
            let value = self.store.new_anonymous(TypeFlags::OBJECT, printed, symbol, true);
            let this_type =
                signature.this_parameter.as_ref().map(|parameter| self.parameter_type(parameter));
            self.contextual_this_parameters.insert(id, this_type);
            self.signature_types.insert(value, vec![signature]);
            Some(value)
        } else if !matches!(name, tsr_ast::PropertyName::ComputedPropertyName(_))
            && self.accessor_type_deferred(symbol)
        {
            // A computed (late-bound) name's binder symbol is this one
            // declaration's, not the merged get/set member native reads
            // (`lateBindMember`), so only a named accessor slot is deferred.
            None
        } else {
            Some(self.get_type_of_symbol(symbol))
        };
        if value == Some(self.intrinsics.error) {
            return false;
        }
        if method
            && let Some(value) = value
            && self.is_context_sensitive_function_like(id)
        {
            self.add_intra_expression_inference_site(id, value);
        }
        let slot = value.map_or(PropertySlot::of_accessor(symbol), PropertySlot::resolved);
        checked_members.push((id, slot));
        let (name, printed_name) =
            if let tsr_ast::PropertyName::ComputedPropertyName(computed) = name {
                match self.computed_member_index_key(computed) {
                    ComputedNameKey::LateBound => {
                        let Some((printed, named)) = self.late_bound_symbol_member_name(computed)
                        else {
                            return false;
                        };
                        if !named {
                            return true;
                        }
                        (self.type_literal_member_key(name, symbol, &printed), printed)
                    }
                    ComputedNameKey::Index(_) | ComputedNameKey::Nothing => return true,
                }
            } else {
                let key = self.binder.symbols().get(symbol).name.to_string();
                let printed = self.callable_property_name(symbol, &key);
                (key, printed)
            };
        let property = AnonymousProperty {
            accessor_write: self.accessor_write_parameter(symbol),
            origin: Some(symbol),
            checked_declaration: None,
            method,
            name,
            printed_name,
            printed_slot: match value {
                Some(value) => PrintedSlot::printed(self.type_to_string(value)),
                None => PrintedSlot::on_demand(),
            },
            slot,
            optional: self.property_is_optional(symbol),
            readonly: readonly || self.is_readonly_symbol(symbol),
        };
        if let Some(index) = properties.iter().position(|held| held.name == property.name) {
            properties[index] = property;
        } else {
            properties.push(property);
        }
        true
    }

    /// Declaration provenance survives synthetic spread and instantiation images.
    /// Native getSpreadSymbol/getSpreadType copy the original Declarations list.
    pub(crate) fn property_origin(&mut self, source: TypeId, name: &str) -> Option<SymbolId> {
        if let Some((properties, _)) = self.anonymous_properties.get(&source)
            && let Some(property) = properties.iter().find(|property| property.name == name)
        {
            return property.origin;
        }
        self.get_property_of_type(source, name)
    }

    /// Ported from `Checker.isValidSpreadType` (`checker.go:13504`).
    /// Filter definitely falsy alternatives only after resolving base constraints;
    /// a primitive that can be truthy still makes the operand invalid.
    pub(crate) fn is_valid_spread_type(&mut self, source: TypeId) -> bool {
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
    /// partial object. Multiple nonempty alternatives are left for getSpreadType
    /// to distribute.
    pub(crate) fn try_merge_union_of_object_type_and_empty_object(
        &mut self,
        source: TypeId,
    ) -> TypeId {
        let TypeData::Union { types, .. } = self.store.get(source).data.clone() else {
            return source;
        };
        if types.iter().all(|&ty| self.is_empty_object_type_or_spreads_into_empty_object(ty)) {
            // Native searches for an ACTUAL empty object separately. A later
            // null/undefined/primitive constituent must not overwrite it; if
            // there is no actual empty object, return emptyObjectType rather
            // than one of the primitive constituents.
            return types
                .into_iter()
                .find(|&ty| self.is_empty_spread_object_type(ty))
                .unwrap_or(self.intrinsics.empty_object);
        }
        let mut nonempty = types
            .into_iter()
            .filter(|&ty| !self.is_empty_object_type_or_spreads_into_empty_object(ty));
        let Some(first) = nonempty.next() else { return source };
        if nonempty.any(|ty| ty != first) {
            return source;
        }
        let semantic = self.base_constraint_of_type(first).unwrap_or(first);
        let properties_source = if self.is_empty_object_type_or_spreads_into_empty_object(semantic)
        {
            semantic
        } else {
            first
        };
        let Some((mut properties, _)) = self.spread_properties(properties_source, false) else {
            return source;
        };
        let Some(resolved) = self.resolved_spread_source(properties_source) else { return source };
        let Some(infos) = self.get_index_infos_of_type(resolved) else { return source };
        let mut partial_members = Vec::new();
        for info in &infos {
            let Some(members) = self.index_info_members(info) else {
                return source;
            };
            partial_members.extend(members);
        }
        for property in &mut properties {
            property.method = false;
            property.accessor_write = None;
            property.optional = true;
            if self.strict_null_checks {
                let r#type = self.property_type(property);
                property.slot = PropertySlot::resolved(self.get_optional_type(r#type, true));
            }
            let r#type = self.property_type(property);
            let displayed = self.remove_missing_type(r#type);
            property.printed_slot = PrintedSlot::printed(self.type_to_string(displayed));
        }
        partial_members.extend(self.property_members(&properties));
        let owner = match self.store.get(first).data {
            TypeData::Named { members, .. } => members,
            _ => None,
        };
        let result =
            self.store.new_named(TypeFlags::OBJECT, render_object_type(&partial_members), owner);
        self.anonymous_properties.insert(result, (properties, true));
        self.object_literal_index_infos.insert(result, infos);
        self.object_literal_members.insert(result, partial_members);
        self.object_literal_spread_flags.insert(result, false);
        result
    }

    /// `isEmptyObjectType` (`checker.go:26485`), restricted at the concrete
    /// object boundary to member tables this port can prove complete.
    /// Native's union/intersection recursion belongs here, not in the broader
    /// spread-empty predicate below.
    pub(crate) fn is_empty_spread_object_type(&mut self, ty: TypeId) -> bool {
        if self.is_empty_anonymous_object_type(ty)
            || self.store.get(ty).flags.intersects(TypeFlags::NON_PRIMITIVE)
        {
            return true;
        }
        if self.store.get(ty).flags.contains(TypeFlags::OBJECT)
            && self.declared_members_are_complete(ty)
            && self.get_property_names_of_type(ty).is_some_and(|names| names.is_empty())
            && self.call_signatures_of_type(ty).is_some_and(|signatures| signatures.is_empty())
            && self
                .signatures_of_type_kind(ty, crate::signatures::SignatureKind::Construct)
                .is_some_and(|signatures| signatures.is_empty())
            && self.get_index_infos_of_type(ty).is_some_and(|indexes| indexes.is_empty())
        {
            return true;
        }
        match self.store.get(ty).data.clone() {
            TypeData::Union { types, .. } => {
                types.into_iter().any(|part| self.is_empty_spread_object_type(part))
            }
            TypeData::Intersection { types, .. } => {
                types.into_iter().all(|part| self.is_empty_spread_object_type(part))
            }
            _ => false,
        }
    }

    /// `isEmptyObjectTypeOrSpreadsIntoEmptyObject` (`checker.go:13604`).
    /// This classification deliberately does not inspect type-parameter
    /// constraints: a `T extends undefined` is the one nonempty constituent
    /// of `object | T`, then native property resolution consults its constraint
    /// and produces the empty partial object.
    fn is_empty_object_type_or_spreads_into_empty_object(&mut self, ty: TypeId) -> bool {
        if self.is_empty_spread_object_type(ty) {
            return true;
        }
        // Unlike isEmptyObjectType, this mask applies only to `ty` itself.
        // Do not recurse through union/intersection constituents here.
        self.store.get(ty).flags.intersects(
            TypeFlags::NULLABLE
                | TypeFlags::BOOLEAN_LIKE
                | TypeFlags::NUMBER_LIKE
                | TypeFlags::BIG_INT_LIKE
                | TypeFlags::STRING_LIKE
                | TypeFlags::ENUM_LIKE
                | TypeFlags::NON_PRIMITIVE
                | TypeFlags::INDEX,
        )
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
/// Named properties and methods replace the previous value in insertion order.
/// Call, construct, accessor and index signatures have no replaceable key here.
fn upsert_member(members: &mut Vec<Member>, member: Member) {
    let name = match &member {
        Member::Property { name, .. } | Member::Method { name, .. } => Some(name),
        _ => None,
    };
    if let Some(name) = name
        && let Some(existing) = members.iter_mut().find(|held| match held {
            Member::Property { name: held, .. } | Member::Method { name: held, .. } => held == name,
            _ => false,
        })
    {
        *existing = member;
        return;
    }
    members.push(member);
}

/// `classifyPropertyName` (`nodebuilderimpl.go:2384`) for a method name this
/// producer has already spelled: a method named `new` is a string literal (it
/// would re-parse as a construct signature), and so is a name that is not
/// identifier text — here only the parser-recovery empty name of `{ *() {} }`,
/// since every other spelling arrives quoted or numeric already.
fn classified_method_name(spelled: &str) -> &str {
    match spelled {
        "new" => "\"new\"",
        "" => "\"\"",
        other => other,
    }
}

/// Replace the named member `previous` (a different spelling of the same
/// escaped name) in place; push when it is absent.
fn replace_member_named(members: &mut Vec<Member>, previous: &str, member: Member) {
    if let Some(existing) = members.iter_mut().find(|held| match held {
        Member::Property { name, .. } | Member::Method { name, .. } => name == previous,
        _ => false,
    }) {
        *existing = member;
        return;
    }
    upsert_member(members, member);
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

#[cfg(test)]
mod display_source_tests {
    use super::*;
    use tsr_ast::{Node, push_children};
    use tsr_core::Arena;

    #[test]
    fn surviving_assignment_keys_copy_without_changing_original_regular_or_widened_identity() {
        for (source, boolean_return) in [
            (
                "const value = { item: 17, item: 'last', callback: () => 23,
                    callback: () => true, nested: { marker: 41 } } as const;",
                true,
            ),
            (
                "const value = { item: 'first', item: 19, callback: () => false,
                    callback: () => 31, nested: { marker: 43 } } as const;",
                false,
            ),
        ] {
            for reverse in [false, true] {
                let arena = Arena::new();
                let parsed = tsr_parser::parse(&arena, source);
                let root = parsed.source_file.node_id.unwrap();
                let bound = tsr_binder::bind(
                    &arena,
                    parsed.source_file,
                    &parsed.nodes,
                    tsr_binder::FileInfo { name: "/survivor.ts", text: source },
                );
                let mut nodes = vec![parsed.node_map.get(root).unwrap()];
                let mut literal = None;
                while let Some(node) = nodes.pop() {
                    if let Node::ObjectLiteralExpression(node) = node
                        && node.properties.len() == 5
                    {
                        literal = Some(node);
                    }
                    push_children(node, &mut nodes);
                }
                let literal = literal.unwrap();
                let mut last = std::collections::HashMap::new();
                for property in literal.properties {
                    let tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) =
                        property
                    else {
                        panic!("property-only native control");
                    };
                    let declaration = assignment.node_id.unwrap();
                    let symbol = bound.symbol_of(declaration).unwrap();
                    last.insert(bound.symbols().get(symbol).name, (symbol, declaration));
                }
                let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
                let fresh =
                    checker.check_expression(tsr_ast::Expression::ObjectLiteralExpression(literal));
                let (regular, widened) = if reverse {
                    let widened = checker.widen_object_literal_freshness(fresh);
                    (checker.get_regular_type_of_object_literal(fresh), widened)
                } else {
                    let regular = checker.get_regular_type_of_object_literal(fresh);
                    (regular, checker.widen_object_literal_freshness(fresh))
                };
                assert_ne!(fresh, regular);
                assert_ne!(fresh, widened);
                assert_ne!(regular, widened);
                let mut images = vec![fresh, regular, widened];
                if reverse {
                    images.reverse();
                }
                let mut snapshots = Vec::new();
                for id in images {
                    let properties = checker.anonymous_properties[&id].0.clone();
                    assert_eq!(properties.len(), 3);
                    for property in &properties {
                        let &(symbol, declaration) = last.get(property.name.as_str()).unwrap();
                        assert_eq!(property.origin, Some(symbol));
                        assert_eq!(property.checked_declaration, Some(declaration));
                        if property.name == "callback" {
                            assert_ne!(
                                bound.symbols().get(symbol).value_declaration,
                                Some(declaration)
                            );
                            let tsr_ast::Node::PropertyAssignment(assignment) =
                                parsed.node_map.get(declaration).unwrap()
                            else {
                                panic!();
                            };
                            let ty = checker.peek_property_type(property).unwrap();
                            let signature = checker.signature_types[&ty][0].clone();
                            assert_eq!(
                                signature.declaration,
                                assignment.initializer.unwrap().node_id().unwrap()
                            );
                            assert_eq!(
                                checker.get_return_type_of_signature(&signature),
                                Some(if boolean_return {
                                    checker.intrinsics.boolean
                                } else {
                                    checker.intrinsics.number
                                })
                            );
                        }
                    }
                    snapshots.push((id, checker.store.get(id).data.clone(), properties));
                }
                let nested = |id| {
                    checker.anonymous_properties[&id]
                        .0
                        .iter()
                        .find(|property| property.name == "nested")
                        .and_then(|property| checker.peek_property_type(property))
                        .unwrap()
                };
                assert_ne!(
                    nested(fresh),
                    nested(regular),
                    "member types transform while source keys copy"
                );
                assert_ne!(nested(fresh), nested(widened));
                for _ in 0..4 {
                    snapshots.reverse();
                    assert_eq!(checker.get_regular_type_of_object_literal(fresh), regular);
                    assert_eq!(checker.widen_object_literal_freshness(fresh), widened);
                    for (id, data, properties) in &snapshots {
                        assert_eq!(&checker.store.get(*id).data, data);
                        for (current, original) in
                            checker.anonymous_properties[id].0.iter().zip(properties)
                        {
                            assert_eq!(current.origin, original.origin);
                            assert_eq!(current.name, original.name);
                            assert_eq!(current.checked_declaration, original.checked_declaration);
                            assert_eq!(
                                checker.peek_property_type(current),
                                checker.peek_property_type(original)
                            );
                        }
                    }
                }
            }
        }
    }
}
