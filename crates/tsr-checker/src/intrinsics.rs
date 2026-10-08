//! The intrinsic types, created once when the checker is constructed.
//!
//! Ported from `Checker.initializeChecker`'s type creation block
//! (`internal/checker/checker.go:975`–`1015`).
//!
//! # Why several of these look identical
//!
//! Upstream creates **distinct types that print the same string**, and
//! distinguishes them by pointer identity:
//!
//! - `anyType`, `errorType`, `wildcardType`, `blockedStringType`,
//!   `nonInferrableAnyType`, `autoType` and `intrinsicMarkerType` are all
//!   `TypeFlagsAny`; four of them print `any`, and the rest print `error`,
//!   `unresolved` and `intrinsic`.
//! - `neverType`, `silentNeverType`, `implicitNeverType`, `unreachableNeverType`
//!   and `uniqueLiteralType` are all `TypeFlagsNever` printing `never`.
//! - `undefinedType`, `missingType` and `optionalType` all print `undefined`.
//!
//! Merging any pair would be an invisible change that alters inference: `errorType`
//! suppresses cascading errors where `anyType` does not, and `silentNeverType`
//! suppresses them where `neverType` does not. [`TypeStore::new_intrinsic`] never
//! interns, which is what keeps them separate.
//!
//! [`TypeStore::new_intrinsic`]: crate::types::TypeStore::new_intrinsic

use crate::{
    flags::TypeFlags,
    types::{TypeId, TypeStore},
};

/// The well-known types, by the names upstream gives them.
///
/// Only the ones the checker currently reaches are listed. Adding one means
/// adding the corresponding line from `checker.go:975` onwards, not inventing a
/// type.
#[derive(Debug, Clone, Copy)]
pub struct Intrinsics {
    /// `anyType` — `checker.go:975`.
    pub any: TypeId,
    /// **The port's gap**: "this port could not compute the type". Prints
    /// `error` everywhere, so every instrument can tell a gap from a wrong
    /// answer. It is `TypeFlagsAny` and behaves as upstream's `errorType`
    /// semantically (it suppresses follow-on errors that `any` would not),
    /// because until ADR-0048 it was the port's only error type.
    ///
    /// Upstream's own `errorType` — the answer upstream *computes* — is
    /// [`Intrinsics::native_error`]. Answering this one claims nothing about
    /// upstream; answering that one claims upstream answers `errorType` there.
    pub error: TypeId,
    /// `errorType` — `checker.go:979`, the type upstream computes for an
    /// unresolved name or an abandoned computation. `TypeFlagsAny`, so
    /// `IsTypeAny` holds, and [`Checker::is_error`](crate::Checker) answers
    /// true for it as for the gap.
    ///
    /// Upstream's intrinsic name is `"error"`, but only the baseline writer's
    /// intrinsic-name fast path ever prints that name
    /// (`type_symbol_baseline.go:378`); the node builder renders every
    /// `TypeFlagsAny` type as the `any` keyword. This type is therefore created
    /// with the printed name `any`, which is what every rendering inside the
    /// checker (and every nested position of a baseline line) shows, and the
    /// writer's fast path restores `"error"` by identity
    /// (`types_producer::render`). ADR-0048.
    pub native_error: TypeId,
    /// Native unresolvedType: private unresolved aliases link to this distinct intrinsic.
    pub unresolved: TypeId,
    /// `unknownType` — `checker.go:983`.
    pub unknown: TypeId,
    /// The empty anonymous object and unknown's distinct empty constituent
    /// (`checker.go:1021`, `checker.go:1025`).
    pub empty_object: TypeId,
    /// The distinct empty constituent of unknown (`checker.go:1025`).
    pub unknown_empty_object: TypeId,
    /// The strict unknown expansion (`checker.go:25036`).
    pub unknown_union: TypeId,
    /// `undefinedType` — `checker.go:984`.
    pub undefined: TypeId,
    /// `undefinedWideningType` — aliases ordinary undefined in strict mode.
    pub undefined_widening: TypeId,
    /// Retained loose-mode identity, selected without allocating new types.
    loose_undefined_widening: TypeId,
    /// `missingType` — `checker.go:986`; see `exactOptionalPropertyTypes`.
    pub missing: TypeId,
    /// `nullType` — `checker.go:989`.
    pub null: TypeId,
    /// `nullWideningType` — `checker.go:990`, `createWideningType(nullType)`
    /// (`checker.go:25027`): what a `null` *expression* answers. Aliases
    /// ordinary null in strict mode; with `strictNullChecks` off it is a
    /// distinct identity that prints `null` and widens to `any`
    /// ([`Intrinsics::is_widening_nullable`]).
    pub null_widening: TypeId,
    /// Retained loose-mode identity, selected without allocating new types.
    loose_null_widening: TypeId,
    /// `stringType` — `checker.go:991`.
    pub string: TypeId,
    /// `numberType` — `checker.go:992`.
    pub number: TypeId,
    /// `bigint_type` — `checker.go:993`.
    pub bigint: TypeId,
    /// `booleanType` — `checker.go:1002`, and **not an intrinsic**: upstream
    /// builds it as the union `false | true`, and so does this
    /// ([`crate::unions::create_boolean_type`]). It prints `boolean` because a
    /// union of exactly the two boolean literal types carries
    /// [`TypeFlags::BOOLEAN`], which the node builder tests before it ever
    /// reaches its union branch (`nodebuilderimpl.go:3255`).
    ///
    /// That it is a union rather than an intrinsic is observable: `boolean` in a
    /// union flattens into its two constituents, so `boolean | true` reduces to
    /// `boolean` instead of printing as two members.
    pub boolean: TypeId,
    /// `esSymbolType` — `checker.go:1003`.
    pub es_symbol: TypeId,
    /// `voidType` — `checker.go:1004`.
    pub void: TypeId,
    /// `neverType` — `checker.go:1005`.
    pub never: TypeId,
    /// `implicitNeverType` — the empty-array element in strict mode.
    pub implicit_never: TypeId,
    /// `unreachableNeverType` — `checker.go`, the flow walk's SENTINEL for a
    /// path a `never`-returning call (or an unreachable assignment) cuts
    /// off. Flagged `NEVER` so every junction drops it like `never`, and
    /// distinct from [`Intrinsics::never`] so the walk's EXIT
    /// (`flow.go:111`) can recognise it and answer the declared type. §744.
    pub unreachable_never: TypeId,
    /// `nonPrimitiveType` — `checker.go:1009`, spelled `object`.
    pub non_primitive: TypeId,
    /// `trueType` — the **fresh** `true`, which is what a `true` *expression*
    /// has. Upstream creates `regularTrueType` first and `trueType` as its fresh
    /// twin; that pair is what makes `let b = true` widen to `boolean` while
    /// `let b: true` stays `true`.
    pub true_type: TypeId,
    /// `falseType` — the fresh `false`.
    pub false_type: TypeId,
    /// `regularTrueType`, the non-fresh `true` a literal type node yields.
    pub regular_true: TypeId,
    /// `regularFalseType`.
    pub regular_false: TypeId,
}

impl Intrinsics {
    /// Create every intrinsic, in upstream's order.
    ///
    /// The order matters only in that [`TypeId`]s are handed out sequentially, so
    /// keeping it makes the ids stable and comparable against upstream when
    /// debugging.
    pub fn create(store: &mut TypeStore) -> Self {
        let literal = |store: &mut TypeStore, value: bool, fresh: bool| {
            store.intern_literal(
                TypeFlags::BOOLEAN_LITERAL,
                crate::types::TypeData::BooleanLiteral(value),
                fresh,
            )
        };
        // Upstream's order (`checker.go:975`–`1015`), which now has to be
        // followed rather than merely preferred: `booleanType` is the *union* of
        // the two regular boolean literal types, so those must exist first.
        let any = store.new_intrinsic(TypeFlags::ANY, "any");
        let error = store.new_intrinsic(TypeFlags::ANY, "error");
        // ADR-0048: upstream's own `errorType`, beside the port's gap.
        let native_error = store.new_intrinsic(TypeFlags::ANY, "any");
        let unresolved = store.new_intrinsic(TypeFlags::ANY, "unresolved");
        let unknown = store.new_intrinsic(TypeFlags::UNKNOWN, "unknown");
        let undefined = store.new_intrinsic(TypeFlags::UNDEFINED, "undefined");
        let loose_undefined_widening = store.new_intrinsic(TypeFlags::UNDEFINED, "undefined");
        // `missingType` (`checker.go:986`): a DISTINCT undefined used for the
        // optionality a `?` adds under `exactOptionalPropertyTypes`; prints
        // `undefined`, removed at write positions.
        let missing = store.new_intrinsic(TypeFlags::UNDEFINED, "undefined");
        let null = store.new_intrinsic(TypeFlags::NULL, "null");
        let loose_null_widening = store.new_intrinsic(TypeFlags::NULL, "null");
        let string = store.new_intrinsic(TypeFlags::STRING, "string");
        let number = store.new_intrinsic(TypeFlags::NUMBER, "number");
        let bigint = store.new_intrinsic(TypeFlags::BIG_INT, "bigint");
        // Regular first, then fresh, as upstream creates them.
        let regular_false = literal(store, false, false);
        let false_type = literal(store, false, true);
        let regular_true = literal(store, true, false);
        let true_type = literal(store, true, true);
        let boolean = crate::unions::create_boolean_type(store, regular_false, regular_true);
        // `emptyStringType`, `zeroType`, `zeroBigIntType` (`checker.go:1049`):
        // upstream creates the three zero literals at checker construction, so
        // they precede every literal of the program in type-id order (and so
        // in union order). Interned here, regular form, for that order only.
        store.intern_literal(
            TypeFlags::STRING_LITERAL,
            crate::types::TypeData::StringLiteral(String::new()),
            false,
        );
        store.intern_literal(
            TypeFlags::NUMBER_LITERAL,
            crate::types::TypeData::NumberLiteral("0".to_owned()),
            false,
        );
        store.intern_literal(
            TypeFlags::BIG_INT_LITERAL,
            crate::types::TypeData::BigIntLiteral("0".to_owned()),
            false,
        );
        let empty_object = store.new_named(TypeFlags::OBJECT, "{}".to_string(), None);
        let unknown_empty_object = store.new_named(TypeFlags::OBJECT, "{}".to_string(), None);
        let unknown_union = crate::unions::create_union(
            store,
            TypeFlags::empty(),
            vec![undefined, null, unknown_empty_object],
            None,
        );
        Self {
            empty_object,
            unknown_empty_object,
            unknown_union,
            any,
            error,
            native_error,
            unresolved,
            unknown,
            undefined,
            undefined_widening: undefined,
            loose_undefined_widening,
            missing,
            null,
            null_widening: null,
            loose_null_widening,
            string,
            number,
            bigint,
            boolean,
            es_symbol: store.new_intrinsic(TypeFlags::ES_SYMBOL, "symbol"),
            void: store.new_intrinsic(TypeFlags::VOID, "void"),
            never: store.new_intrinsic(TypeFlags::NEVER, "never"),
            implicit_never: store.new_intrinsic(TypeFlags::NEVER, "never"),
            unreachable_never: store.new_intrinsic(TypeFlags::NEVER, "never"),
            non_primitive: store.new_intrinsic(TypeFlags::NON_PRIMITIVE, "object"),
            regular_true,
            regular_false,
            true_type,
            false_type,
        }
    }

    /// Select the active identity before semantic queries (`checker.go:25027`).
    pub(crate) fn select_strict_null_checks(&mut self, on: bool) {
        self.undefined_widening = if on { self.undefined } else { self.loose_undefined_widening };
        self.null_widening = if on { self.null } else { self.loose_null_widening };
    }

    /// Whether `id` is one of `createWideningType`'s products
    /// (`checker.go:25027`), i.e. a nullable intrinsic carrying
    /// `ObjectFlagsContainsWideningType`. Only true with `strictNullChecks`
    /// off: in strict mode the widening twins *are* the plain types, which
    /// carry no widening flag.
    pub(crate) fn is_widening_nullable(&self, id: TypeId) -> bool {
        (id == self.undefined_widening && id != self.undefined)
            || (id == self.null_widening && id != self.null)
    }
}
