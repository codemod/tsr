//! The type store: what a type *is*, and how one is named.
//!
//! Ported from `internal/checker/types.go`. Upstream's `Type` is a struct with a
//! `flags`, an `objectFlags`, a `symbol`, and a `data` interface holding the
//! kind-specific payload. Here the payload is an enum, because Rust has one and
//! the closed set is known.
//!
//! **Types are handles.** Nothing hands out a `&Type` that outlives the call;
//! see [ADR-0013](../../../docs/adr/0013-checker-memoisation.md). Reaching a
//! type's contents means asking the store again with its [`TypeId`].

use rustc_hash::FxHashMap;

use crate::flags::TypeFlags;

/// A type, as a handle into [`TypeStore`].
///
/// `Copy` and 4 bytes, which is the property ADR-0013 rests on: nothing survives
/// into a recursive call, so the borrow checker never sees a live borrow of the
/// checker across one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TypeId(u32);

impl TypeId {
    /// The index this handle refers to.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// The payload distinguishing one type from another of the same flags.
///
/// Upstream's `Type.data` (`types.go`), as a closed enum.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeData {
    /// A type with no payload beyond its flags and printed name: `string`,
    /// `number`, `any`, and the rest of `TypeFlagsIntrinsic`.
    ///
    /// Upstream's `IntrinsicType`, whose `intrinsicName` is the printed form.
    /// Note that several *distinct* intrinsics share a name — `errorType` and
    /// `wildcardType` both print as `any`, and `unknownType` prints `unknown` —
    /// so the name alone does not identify one.
    Intrinsic {
        /// What upstream calls `intrinsicName`, e.g. `"any"` or `"error"`.
        name: &'static str,
    },
    /// A string literal type. The payload is the literal's *value*, decoded.
    StringLiteral(String),
    /// A numeric literal type.
    ///
    /// Stored as the printed form rather than as `f64`, because the printed form
    /// is what a `.types` baseline compares against and float formatting is
    /// exactly where a port drifts. `1` must not print as `1.0`.
    NumberLiteral(String),
    /// A bigint literal type, e.g. `1n`.
    BigIntLiteral(String),
    /// `true` or `false`.
    BooleanLiteral(bool),
}

/// One type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Type {
    /// What kind of type this is.
    pub flags: TypeFlags,
    /// The kind-specific payload.
    pub data: TypeData,
}

/// Every type the checker has created, interned.
///
/// Upstream interns literal types through `stringLiteralTypes` and friends
/// (`checker.go`), so that two occurrences of `"a"` are the same `*Type` and can
/// be compared by pointer. Here the same job is done by a map to [`TypeId`], and
/// identity is `TypeId` equality.
///
/// **Intrinsics are deliberately *not* interned by content.** `errorType` and
/// `anyType` are both `ANY` with different names, and `wildcardType`,
/// `blockedStringType` and `nonInferrableAnyType` are further distinct types that
/// all print `any`. Upstream keeps them as separate allocations and relies on
/// pointer identity to tell them apart; interning by content would silently merge
/// them and lose distinctions the checker depends on.
#[derive(Debug, Default)]
pub struct TypeStore {
    types: Vec<Type>,
    /// Literal types only. See the note above on why intrinsics are excluded.
    interned: FxHashMap<(TypeFlags, TypeData), TypeId>,
}

impl TypeStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many types exist.
    #[must_use]
    pub fn len(&self) -> usize {
        self.types.len()
    }

    /// Whether no type has been created.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    /// Read a type back. The only way to reach a type's contents.
    ///
    /// # Panics
    ///
    /// If `id` did not come from this store.
    #[must_use]
    pub fn get(&self, id: TypeId) -> &Type {
        &self.types[id.index()]
    }

    /// Create a type without interning, always a fresh identity.
    ///
    /// Ported from `Checker.newIntrinsicType` (`checker.go:25017`).
    pub fn new_intrinsic(&mut self, flags: TypeFlags, name: &'static str) -> TypeId {
        self.push(Type { flags, data: TypeData::Intrinsic { name } })
    }

    /// Create or reuse a literal type.
    ///
    /// Interned: `"a"` written twice is one type, which is what makes literal
    /// identity comparable by [`TypeId`].
    pub fn intern(&mut self, flags: TypeFlags, data: TypeData) -> TypeId {
        if let Some(&existing) = self.interned.get(&(flags, data.clone())) {
            return existing;
        }
        let id = self.push(Type { flags, data: data.clone() });
        self.interned.insert((flags, data), id);
        id
    }

    fn push(&mut self, ty: Type) -> TypeId {
        let id = TypeId(u32::try_from(self.types.len()).expect("type count fits in u32"));
        self.types.push(ty);
        id
    }
}
