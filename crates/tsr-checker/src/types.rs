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

use tsr_binder::SymbolId;

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
    /// A type that prints as a **name**: a class or interface instance type, a
    /// type parameter, or an enum.
    ///
    /// Upstream carries a `symbol` on the type and the node builder renders the
    /// name from it, applying scoping rules this port has no equivalent of. Here
    /// the printed form is computed once, at creation, from the declaration —
    /// `C`, or `C<T, U>` for a generic one. That is a **divergence in the
    /// renderer, not in the data model**: identity is still one type per symbol,
    /// which is what the memo in the checker provides, so two same-named
    /// declarations in different scopes stay distinct types even though they
    /// print alike. It stops being adequate when a name has to be qualified or
    /// shadowed, which is where upstream's node builder earns its complexity.
    Named {
        /// The printed form, computed once at creation.
        text: String,
        /// The symbol whose `members` table this type's properties live in, for
        /// the types that have one: a class, an interface, a type literal.
        ///
        /// `None` where a lookup would be **wrong rather than empty** — an
        /// instantiated `C<number>` would find `C`'s uninstantiated members and
        /// answer `T` for `x` where upstream answers `number`, because nothing
        /// substitutes yet. A gap is the honest answer there.
        members: Option<SymbolId>,
    },
}

/// One type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Type {
    /// What kind of type this is.
    pub flags: TypeFlags,
    /// The kind-specific payload.
    pub data: TypeData,
    /// Whether this is the *fresh* form of a literal type.
    ///
    /// Upstream keeps a `freshType`/`regularType` pair on `LiteralType` and
    /// asks `isFreshLiteralType(t)` — which is `t.freshType == t`, i.e. pointer
    /// identity (`checker.go`). Here the two forms are two interned types
    /// differing only in this bit, so the same question is `TypeId` equality.
    ///
    /// It decides one visible thing: `getWidenedLiteralType`
    /// (`checker.go:25487`) widens a literal **only if it is fresh**. A literal
    /// written as an expression is fresh, so `let x = "a"` widens to `string`;
    /// a literal arriving from a type node is regular, so `let x: "a"` stays
    /// `"a"`. Both print identically, which is why this is a field and not
    /// something recoverable from the printed form.
    pub fresh: bool,
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
    interned: FxHashMap<(TypeFlags, TypeData, bool), TypeId>,
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
        self.push(Type { flags, data: TypeData::Intrinsic { name }, fresh: false })
    }

    /// Create a type that prints as a name, without interning.
    ///
    /// Ported from `Checker.newObjectType` / `newTypeParameter` (`checker.go`),
    /// reduced to what the printed form needs. **Never interned**, for the same
    /// reason intrinsics are not: identity is one type per *symbol*, and two
    /// same-named declarations in different scopes are different types that
    /// print alike. Interning by content would merge them.
    pub fn new_named(
        &mut self,
        flags: TypeFlags,
        name: String,
        members: Option<SymbolId>,
    ) -> TypeId {
        self.push(Type { flags, data: TypeData::Named { text: name, members }, fresh: false })
    }

    /// Create or reuse a literal type.
    ///
    /// Interned: `"a"` written twice is one type, which is what makes literal
    /// identity comparable by [`TypeId`].
    pub fn intern(&mut self, flags: TypeFlags, data: TypeData) -> TypeId {
        self.intern_literal(flags, data, false)
    }

    /// Create or reuse a literal type in its fresh or regular form.
    ///
    /// The two forms are separate interned types, so `TypeId` equality answers
    /// `isFreshLiteralType` the way pointer identity does upstream. They print
    /// the same string, exactly as `anyType` and `errorType` do.
    pub fn intern_literal(&mut self, flags: TypeFlags, data: TypeData, fresh: bool) -> TypeId {
        if let Some(&existing) = self.interned.get(&(flags, data.clone(), fresh)) {
            return existing;
        }
        let id = self.push(Type { flags, data: data.clone(), fresh });
        self.interned.insert((flags, data, fresh), id);
        id
    }

    fn push(&mut self, ty: Type) -> TypeId {
        let id = TypeId(u32::try_from(self.types.len()).expect("type count fits in u32"));
        self.types.push(ty);
        id
    }
}
