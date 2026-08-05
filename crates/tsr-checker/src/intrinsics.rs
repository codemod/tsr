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
    /// `errorType` — `checker.go:979`. Prints `any`, but is not `anyType`:
    /// it marks a type that could not be computed, and suppresses follow-on
    /// errors that `any` would not.
    pub error: TypeId,
    /// `unknownType` — `checker.go:983`.
    pub unknown: TypeId,
    /// `undefinedType` — `checker.go:984`.
    pub undefined: TypeId,
    /// `nullType` — `checker.go:989`.
    pub null: TypeId,
    /// `stringType` — `checker.go:991`.
    pub string: TypeId,
    /// `numberType` — `checker.go:992`.
    pub number: TypeId,
    /// `bigint_type` — `checker.go:993`.
    pub bigint: TypeId,
    /// `booleanType`. Upstream builds it as the union `false | true` rather than
    /// as an intrinsic; until unions exist it is an intrinsic here, which is a
    /// **known divergence** — see the crate docs and `bd tsr-4sc.1`.
    pub boolean: TypeId,
    /// `esSymbolType` — `checker.go:1003`.
    pub es_symbol: TypeId,
    /// `voidType` — `checker.go:1004`.
    pub void: TypeId,
    /// `neverType` — `checker.go:1005`.
    pub never: TypeId,
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
        Self {
            any: store.new_intrinsic(TypeFlags::ANY, "any"),
            error: store.new_intrinsic(TypeFlags::ANY, "error"),
            unknown: store.new_intrinsic(TypeFlags::UNKNOWN, "unknown"),
            undefined: store.new_intrinsic(TypeFlags::UNDEFINED, "undefined"),
            null: store.new_intrinsic(TypeFlags::NULL, "null"),
            string: store.new_intrinsic(TypeFlags::STRING, "string"),
            number: store.new_intrinsic(TypeFlags::NUMBER, "number"),
            bigint: store.new_intrinsic(TypeFlags::BIG_INT, "bigint"),
            boolean: store.new_intrinsic(TypeFlags::BOOLEAN, "boolean"),
            es_symbol: store.new_intrinsic(TypeFlags::ES_SYMBOL, "symbol"),
            void: store.new_intrinsic(TypeFlags::VOID, "void"),
            never: store.new_intrinsic(TypeFlags::NEVER, "never"),
            non_primitive: store.new_intrinsic(TypeFlags::NON_PRIMITIVE, "object"),
            // Regular first, then fresh, matching upstream's creation order so
            // the ids stay comparable when debugging against it.
            regular_true: store.intern_literal(
                TypeFlags::BOOLEAN_LITERAL,
                crate::types::TypeData::BooleanLiteral(true),
                false,
            ),
            regular_false: store.intern_literal(
                TypeFlags::BOOLEAN_LITERAL,
                crate::types::TypeData::BooleanLiteral(false),
                false,
            ),
            true_type: store.intern_literal(
                TypeFlags::BOOLEAN_LITERAL,
                crate::types::TypeData::BooleanLiteral(true),
                true,
            ),
            false_type: store.intern_literal(
                TypeFlags::BOOLEAN_LITERAL,
                crate::types::TypeData::BooleanLiteral(false),
                true,
            ),
        }
    }
}
