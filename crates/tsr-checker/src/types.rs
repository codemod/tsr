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

/// The constant payload of tsgo's enum `LiteralType` (types.go).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EnumLiteralValue {
    /// A decoded string value.
    String(String),
    /// A canonical numeric value, sharing ordinary numeric literal spelling.
    Number(String),
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
    /// An enum literal retains its value and nominal declaration identity.
    /// The text is display metadata and does not determine its value or owner.
    EnumLiteral {
        /// Folded enum-member value.
        value: EnumLiteralValue,
        /// Containing enum, part of the native literal interning key.
        owner: SymbolId,
        /// First member declaring this value.
        member: SymbolId,
        /// Native member/enum display spelling.
        text: String,
    },
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
        /// `None` where a lookup would be **wrong rather than empty**. An
        /// instantiated `C<number>` used to be the standing example — it would
        /// find `C`'s uninstantiated members and answer `T` where upstream
        /// answers `number` — until `bd tsr-4qx` made every type-yielding
        /// consumer substitute through the seam in `crate::members`
        /// (`get_type_of_property_of_type`), at which point the reference
        /// started carrying its target symbol here.
        members: Option<SymbolId>,
    },
    /// An **anonymous object type**, carrying the symbol whose declarations are
    /// its signatures.
    ///
    /// Upstream's `newObjectType(ObjectFlagsAnonymous, symbol)`
    /// (`checker.go:16925`) — what a function, method, class, enum or
    /// value-module symbol *has*, and what a function expression or arrow *is*.
    ///
    /// **Why this is separate from [`TypeData::Named`] rather than a field on
    /// it.** The two answer different questions about a symbol. `Named.members`
    /// is where `getPropertyOfType` looks, and for `typeof C` that table must
    /// stay unreachable — a class's *instance* members are not `typeof C`'s
    /// properties, so pointing at them would answer `C.x` with the wrong symbol.
    /// What a call needs is the opposite direction: the symbol's *declarations*,
    /// which is where `getSignaturesOfSymbol` (`checker.go:19806`) reads call
    /// signatures from. Merging the two fields would make one of the two lookups
    /// wrong, and a wrong answer is worse here than a gap.
    ///
    /// The statics half is still missing and still a gap: a class's statics and a
    /// namespace's exports live in the symbol's `exports` table and nothing reads
    /// it (`docs/architecture/checker.md`).
    Anonymous {
        /// The printed form, computed once at creation — `typeof C`, or the
        /// signature `(x: string) => void`. Same divergence as
        /// [`TypeData::Named`], same cause.
        text: String,
        /// The symbol whose declarations carry this type's call signatures.
        symbol: SymbolId,
        /// Whether the node builder would emit a bare **`FunctionTypeNode`**
        /// for this type rather than a `TypeQueryNode` or a `TypeLiteralNode`.
        ///
        /// This is upstream's *node kind*, recorded where the text is built
        /// because that is the only place that still knows it. It exists for
        /// one caller — [`crate::unions`]'s union formatter, which must
        /// parenthesise a function-typed constituent (`(() => void) | undefined`)
        /// and must not parenthesise `typeof C` or `{ a: string; }`. Upstream
        /// gets the same answer from `GetTypeNodePrecedence`
        /// (`ast/precedence.go:655`) because it still has the node.
        ///
        /// A flag rather than a test on `text`: `{ f: () => void; }` contains
        /// `=>` and is a type literal, so a string test would parenthesise it,
        /// and the three creation sites each know the answer for free.
        signature: bool,
    },
    /// An intersection type: `A & B`.
    ///
    /// Upstream's `IntersectionType` (`types.go`). Deliberately a **separate
    /// variant** from [`TypeData::Union`] despite the identical shape, because
    /// the two differ in the one respect that is printed: a union's constituents
    /// are sorted by `CompareTypes`, an intersection's are kept in **source
    /// order** (`orderedSet`, `checker.go:26057`). Merging them behind a flag
    /// would put that distinction one indirection away from the code that has to
    /// respect it.
    Intersection {
        /// The printed form, computed once at creation — see
        /// [`TypeData::Union::text`] for why the form is computed rather than
        /// rendered on demand.
        text: String,
        /// The constituents, flattened and deduplicated, in source order.
        types: Vec<TypeId>,
        /// The symbol this intersection prints as, when a type alias names it.
        symbol: Option<SymbolId>,
    },
    /// A union type: `A | B`.
    ///
    /// Upstream's `UnionType` (`types.go`), carrying the constituent list. The
    /// list is **sorted by `CompareTypes` and deduplicated** before it reaches
    /// here — see [`crate::unions`] — and that order is printed verbatim in
    /// every `.types` baseline, so it is part of the type's identity rather
    /// than a rendering detail.
    Union {
        /// The printed form, computed once at creation.
        ///
        /// Same divergence as [`TypeData::Named`], for a different reason:
        /// [`crate::printing::type_to_string`] takes a single [`Type`] and has
        /// no way back to the store, so a form that depends on the
        /// *constituents* has to be rendered while they are still reachable.
        /// It is exact rather than approximate because every constituent
        /// already exists when the union is built.
        text: String,
        /// The constituents, sorted and deduplicated.
        types: Vec<TypeId>,
        /// The symbol this union prints as, where it has one.
        ///
        /// Upstream splits this over two fields — `Type.symbol` for an enum's
        /// declared type (`checker.go:23902`) and `Type.alias.symbol` for the
        /// body of a named type alias (`checker.go:23711`) — and the node
        /// builder reaches them through two different branches. They are one
        /// field here because in every case this port reaches, both branches do
        /// the same thing: print the symbol's name instead of the constituents.
        /// A *generic* alias is where the two would part company, since its
        /// printed form carries type arguments, and that case is a gap.
        symbol: Option<SymbolId>,
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
    /// Pinned tsgo 5b1047d typeToTypeNode / formatUnionTypes display plans.
    /// Private store-owned publication keyed by EXISTING interned `TypeId`, never
    /// part of its semantic key. `create_union_with_text` certifies the ordinary
    /// constituent plan only after mint completion; arbitrary/opaque writers
    /// publish false. An opaque publication cannot be promoted by a later hit.
    /// This records display provenance, not alias accessibility or a receiver
    /// mapper; written origins still have priority in the site renderer. No
    /// traversal/forcing or serialized text is cached here, and no speed claim
    /// follows from admitting an existing constituent walk.
    union_display_plans: FxHashMap<TypeId, bool>,
    /// `(literal, fresh) -> its fresh or regular twin`: native keeps the pair
    /// as each literal type's `freshType`/`regularType` links
    /// (`getFreshTypeOfLiteralType`/`getRegularTypeOfLiteralType`,
    /// `checker.go`, pinned `5b1047d`). The twin is a pure function of the
    /// literal's interned key, so every entry is complete; it is dropped when
    /// `complete_object` rewrites an identity (`r5-checkperf.md` §5).
    literal_twins: FxHashMap<(TypeId, bool), TypeId>,
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

    /// Complete a reserved, non-interned object identity after its members resolve.
    pub(crate) fn complete_object(&mut self, reserved: TypeId, resolved: TypeId) {
        self.types[reserved.index()] = self.types[resolved.index()].clone();
        self.literal_twins.remove(&(reserved, false));
        self.literal_twins.remove(&(reserved, true));
    }

    /// The interned literal with `id`'s flags and payload and the given
    /// freshness — what `intern_literal(flags, data.clone(), fresh)` answers —
    /// remembered per `(id, fresh)` so the payload is cloned and hashed once
    /// per literal rather than once per request (`r5-checkperf.md` §5).
    pub(crate) fn literal_twin(&mut self, id: TypeId, fresh: bool) -> TypeId {
        if let Some(&twin) = self.literal_twins.get(&(id, fresh)) {
            return twin;
        }
        let ty = &self.types[id.index()];
        let (flags, data) = (ty.flags, ty.data.clone());
        let twin = self.intern_literal(flags, data, fresh);
        self.literal_twins.insert((id, fresh), twin);
        twin
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

    /// Create an anonymous object type carrying its symbol.
    ///
    /// Ported from `Checker.newObjectType` with `ObjectFlagsAnonymous`
    /// (`checker.go:16925`). **Never interned**, for the same reason
    /// [`TypeStore::new_named`] is not: identity is one type per symbol, and the
    /// memo that provides it lives in the checker.
    pub fn new_anonymous(
        &mut self,
        flags: TypeFlags,
        text: String,
        symbol: SymbolId,
        signature: bool,
    ) -> TypeId {
        self.push(Type {
            flags,
            data: TypeData::Anonymous { text, symbol, signature },
            fresh: false,
        })
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
        let types = &mut self.types;
        *self.interned.entry((flags, data, fresh)).or_insert_with_key(|(flags, data, fresh)| {
            let id = TypeId(u32::try_from(types.len()).expect("type count fits in u32"));
            types.push(Type { flags: *flags, data: data.clone(), fresh: *fresh });
            id
        })
    }

    /// Create or reuse a union type.
    ///
    /// Ported from `Checker.getUnionTypeFromSortedList`'s cache
    /// (`checker.go:25736`), which keys `c.unionTypes` on the sorted constituent
    /// ids together with the alias — upstream's `getUnionKey`
    /// (`checker.go:17508`). Here the whole [`TypeData::Union`] payload is the
    /// key, which is the same key plus two fields derived from it: the printed
    /// text is a function of the constituents and the symbol, and the flags are
    /// a function of the constituents.
    ///
    /// **Interning a union is not an optimisation.** `A | B` written twice must
    /// be one type, or the first relation check written compares two handles
    /// that should have been equal.
    ///
    /// # Why it shares the literal table deliberately
    ///
    /// Upstream keeps `unionTypes` and `stringLiteralTypes` in separate maps
    /// because its keys are hand-built strings and a shared space really could
    /// collide. Here the key is `(TypeFlags, TypeData, bool)` and [`TypeData`]
    /// is an enum, so a `Union` payload can never compare equal to a literal
    /// one whatever their contents: the discriminant is part of the derived
    /// `PartialEq` and `Hash`. One table is therefore safe, and the reason is
    /// recorded rather than left as a coincidence to be re-derived. The `fresh`
    /// component is always `false` for a union — freshness is a property of
    /// literal types (`TypeFlagsFreshable`), and a union is not one.
    pub fn intern_union(&mut self, flags: TypeFlags, data: TypeData) -> TypeId {
        self.intern_union_with_display_plan(flags, data, false)
    }

    pub(crate) fn intern_union_with_display_plan(
        &mut self,
        flags: TypeFlags,
        data: TypeData,
        from_constituents: bool,
    ) -> TypeId {
        let id = self.intern_literal(flags, data, false);
        self.union_display_plans
            .entry(id)
            .and_modify(|plan| *plan &= from_constituents)
            .or_insert(from_constituents);
        id
    }

    pub(crate) fn has_union_display_plan(&self, id: TypeId) -> bool {
        self.union_display_plans.get(&id) == Some(&true)
    }

    /// Create or reuse an intersection type.
    ///
    /// Ported from `getIntersectionTypeEx`'s cache keyed by `getIntersectionKey`
    /// (`checker.go:17532`). The same table and the same argument as
    /// [`TypeStore::intern_union`]: `TypeData` is an enum, so an `Intersection`
    /// payload cannot compare equal to a `Union` one however alike they look.
    pub fn intern_intersection(&mut self, flags: TypeFlags, data: TypeData) -> TypeId {
        self.intern_literal(flags, data, false)
    }

    fn push(&mut self, ty: Type) -> TypeId {
        let id = TypeId(u32::try_from(self.types.len()).expect("type count fits in u32"));
        self.types.push(ty);
        id
    }
}

#[cfg(test)]
mod tests {
    use super::{TypeData, TypeStore};
    use crate::flags::TypeFlags;

    #[test]
    fn opaque_union_publication_never_splits_identity_or_promotes_on_warm_hits() {
        for opaque_first in [false, true] {
            let mut store = TypeStore::new();
            let a = store.new_intrinsic(TypeFlags::STRING, "string");
            let b = store.new_intrinsic(TypeFlags::NUMBER, "number");
            let data =
                TypeData::Union { text: "string | number".into(), types: vec![a, b], symbol: None };
            let flags = TypeFlags::UNION;
            let first = store.intern_union_with_display_plan(flags, data.clone(), !opaque_first);
            assert_eq!(store.has_union_display_plan(first), !opaque_first);
            let second = store.intern_union_with_display_plan(flags, data.clone(), opaque_first);
            assert_eq!(first, second);
            assert!(!store.has_union_display_plan(first));
            for _ in 0..4 {
                assert_eq!(store.intern_union_with_display_plan(flags, data.clone(), true), first);
                assert_eq!(store.intern_union(flags, data.clone()), first);
                assert!(!store.has_union_display_plan(first));
                assert_eq!(store.get(first).data, data);
                assert_eq!(crate::printing::type_to_string(store.get(first)), "string | number");
            }
            assert_eq!(store.len(), 3);
        }
    }
}
