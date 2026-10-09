//! Literal freshness and widening.
//!
//! Ported from `Checker.getRegularTypeOfLiteralType`,
//! `getFreshTypeOfLiteralType` and `getWidenedLiteralType`
//! (`checker.go:25487`). Three small functions kept together because they are
//! one rule seen from three sides: a literal type has a fresh form and a
//! regular form, and only the fresh one widens.

use crate::{checker::Checker, flags::TypeFlags, types::TypeId};

impl Checker<'_, '_> {
    /// The regular (non-fresh) form of a literal type.
    ///
    /// Ported from `Checker.getRegularTypeOfLiteralType`. A no-op for anything
    /// that is not a fresh literal.
    pub fn get_regular_type_of_literal_type(&mut self, id: TypeId) -> TypeId {
        // An enum member's regular form is the union's own constituent —
        // interning would mint a twin the relater cannot match
        // (`checker-notes-narrow.md` §18).
        if let Some(&regular) = self.enum_member_regular.get(&id) {
            return regular;
        }
        if !self.store.get(id).fresh {
            return id;
        }
        self.store.literal_twin(id, false)
    }

    /// The fresh form of a literal type.
    ///
    /// Ported from `Checker.getFreshTypeOfLiteralType`.
    pub fn get_fresh_type_of_literal_type(&mut self, id: TypeId) -> TypeId {
        let ty = self.store.get(id);
        if ty.fresh || !ty.flags.intersects(TypeFlags::FRESHABLE) {
            return id;
        }
        self.store.literal_twin(id, true)
    }

    /// The enum a member type belongs to, or the type unchanged.
    ///
    /// Ported from `Checker.getBaseTypeOfEnumLikeType` (`checker.go:25470`):
    ///
    /// ```go
    /// if t.flags&TypeFlagsEnumLike != 0 && t.symbol.Flags&ast.SymbolFlagsEnumMember != 0 {
    ///     return c.getDeclaredTypeOfSymbol(c.getParentOfSymbol(t.symbol))
    /// }
    /// return t
    /// ```
    ///
    /// Literal payloads carry the owning enum directly. Computed-enum types
    /// retain the existing owner side table. Fresh and regular forms therefore
    /// resolve the same base without reconstructing identity from display text.
    pub(crate) fn get_base_type_of_enum_like_type(&mut self, id: TypeId) -> TypeId {
        if !self.store.get(id).flags.intersects(TypeFlags::ENUM_LIKE) {
            return id;
        }
        let owner = match self.store.get(id).data {
            crate::types::TypeData::EnumLiteral { owner, .. } => owner,
            _ => match self.enum_member_owners.get(&id) {
                Some(&owner) => owner,
                None => return id,
            },
        };
        self.get_declared_type_of_symbol(owner)
    }

    /// `getWidenedUniqueESSymbolType` (`checker.go:25505`): a `unique symbol`
    /// widens to plain `symbol` at a mutable location.
    ///
    /// ```go
    /// case t.flags&TypeFlagsUniqueESSymbol != 0: return c.esSymbolType
    /// case t.flags&TypeFlagsUnion != 0:          return c.mapType(t, c.getWidenedUniqueESSymbolType)
    /// ```
    ///
    /// Upstream has exactly **one** call site —
    /// `getWidenedLiteralLikeTypeForContextualType` (`checker.go:25517`), where
    /// it pairs with `getWidenedLiteralType`:
    ///
    /// ```go
    /// t = c.getWidenedUniqueESSymbolType(c.getWidenedLiteralType(t))
    /// ```
    ///
    /// §898. Without it `const s: unique symbol = …; const a = [s];` printed
    /// `(unique symbol)[]` where upstream prints `symbol[]` — the element keeps
    /// a type only a `const` declaration position may carry.
    pub(crate) fn get_widened_unique_es_symbol_type(&mut self, id: TypeId) -> TypeId {
        let ty = self.store.get(id);
        if ty.flags.intersects(crate::flags::TypeFlags::UNIQUE_ES_SYMBOL) {
            return self.intrinsics.es_symbol;
        }
        let crate::types::TypeData::Union { types, .. } = &ty.data else {
            return id;
        };
        let constituents = types.clone();
        let widened: Vec<TypeId> = constituents
            .iter()
            .map(|&constituent| self.get_widened_unique_es_symbol_type(constituent))
            .collect();
        // **Return the original when nothing changed.** `mapType`
        // (`checker.go`) hands back its input rather than rebuilding it, and
        // rebuilding is not free here: a union carries its ALIAS NAME, and
        // `get_union_type` on the same constituents mints a fresh one without it
        // — `{ type: IAxisType; }` printed as `{ type: "categorical" | "linear"; }`,
        // measured as 4 `RIGHT→WRONG` in
        // `conformance/assignmentCompatWithDiscriminatedUnion` on the build that
        // rebuilt unconditionally. Nothing to do with `unique symbol`: it was a
        // union being remade for no reason.
        if widened == constituents {
            return id;
        }
        self.get_union_type(&widened)
    }

    /// `getWidenedLiteralType` (`checker.go:25499`) including its union arm,
    /// `mapType(t, getWidenedLiteralType)`: `b ? f() : 0` widens member-wise
    /// to `void | number`. `mapType` hands back its input when no member
    /// changed, which keeps an alias-named union intact.
    ///
    /// [`Checker::get_widened_literal_type`] is the scalar arms only; its
    /// return-type and declaration callers rely on a union passing through
    /// where upstream gates them by `isUnitType` first (see
    /// `docs/parity/notes/contextual.md` §12). No cache: each member read goes
    /// through the scalar function's existing `regular_types` memo.
    pub(crate) fn get_widened_literal_type_with_unions(&mut self, id: TypeId) -> TypeId {
        let crate::types::TypeData::Union { types, .. } = &self.store.get(id).data else {
            return self.get_widened_literal_type(id);
        };
        let constituents = types.clone();
        let widened: Vec<TypeId> = constituents
            .iter()
            .map(|&constituent| self.get_widened_literal_type(constituent))
            .collect();
        if widened == constituents { id } else { self.get_union_type(&widened) }
    }

    /// The widened form of a literal type.
    ///
    /// Ported from `Checker.getWidenedLiteralType` (`checker.go:25487`).
    /// **Widens only a *fresh* literal**, which is the whole reason freshness is
    /// tracked: `let x = "a"` widens because the expression `"a"` is fresh, while
    /// `let x: "a"` does not, because a literal type node is regular.
    ///
    /// An **enum member** widens to its enum — `var e = E.A` is `E` — which is
    /// the same rule and the reason [`Checker::get_base_type_of_enum_like_type`]
    /// sits beside this one rather than in `crate::declared`.
    pub fn get_widened_literal_type(&mut self, id: TypeId) -> TypeId {
        if let Some(&cached) = self.regular_types.get(&id) {
            return cached;
        }
        let ty = self.store.get(id);
        let (flags, fresh) = (ty.flags, ty.fresh);
        let widened = if !fresh {
            id
        // **First, as upstream has it** (`checker.go:25489`), before the four
        // literal arms. Ordering is not observable today — this port's enum
        // member type carries `ENUM` alone, which none of the arms below test —
        // but upstream's enum literal carries `EnumLiteral | NumberLiteral`, so
        // testing `NUMBER_LITERAL` first there would widen `E.A` to `number`
        // instead of `E`. Writing the order upstream's way costs nothing and
        // means this arm stays correct if the member type ever gains the
        // literal flag it has upstream.
        } else if flags.intersects(TypeFlags::ENUM_LIKE) {
            self.get_base_type_of_enum_like_type(id)
        } else if flags.contains(TypeFlags::STRING_LITERAL) {
            self.intrinsics.string
        } else if flags.contains(TypeFlags::NUMBER_LITERAL) {
            self.intrinsics.number
        } else if flags.contains(TypeFlags::BIG_INT_LITERAL) {
            self.intrinsics.bigint
        } else if flags.contains(TypeFlags::BOOLEAN_LITERAL) {
            self.intrinsics.boolean
        } else {
            id
        };
        self.regular_types.insert(id, widened);
        widened
    }

    /// [`Checker::enum_member_value`] without building its key: the owning
    /// enum and the borrowed value payload of an enum member type (through
    /// its `enum_member_regular` twin), or `None` for anything else. The key
    /// that function spells is `n:`/`s:` then the payload, so two keys are
    /// equal exactly when the variants and the texts are, and
    /// [`enum_value_has_key`] answers the comparison against a spelled key.
    /// Native compares the literal's `value` field the same way
    /// (`isSimpleTypeRelatedTo`, `relater.go:236`). `r5-checkperf2.md` §5.
    pub(crate) fn enum_member_payload(
        &self,
        member: TypeId,
    ) -> Option<(tsr_binder::SymbolId, &crate::types::EnumLiteralValue)> {
        let regular = self.enum_member_regular.get(&member).copied().unwrap_or(member);
        match &self.store.get(regular).data {
            crate::types::TypeData::EnumLiteral { owner, value, .. } => Some((*owner, value)),
            _ => None,
        }
    }
}

/// Whether `key` is [`Checker::enum_member_value`]'s key for `value`:
/// `n:` or `s:`, then the payload.
pub(crate) fn enum_value_has_key(value: &crate::types::EnumLiteralValue, key: &str) -> bool {
    let (prefix, text) = match value {
        crate::types::EnumLiteralValue::Number(text) => ("n:", text),
        crate::types::EnumLiteralValue::String(text) => ("s:", text),
    };
    key.strip_prefix(prefix) == Some(text.as_str())
}
