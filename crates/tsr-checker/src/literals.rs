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
        let ty = self.store.get(id);
        if !ty.fresh {
            return id;
        }
        let (flags, data) = (ty.flags, ty.data.clone());
        self.store.intern_literal(flags, data, false)
    }

    /// The fresh form of a literal type.
    ///
    /// Ported from `Checker.getFreshTypeOfLiteralType`.
    pub fn get_fresh_type_of_literal_type(&mut self, id: TypeId) -> TypeId {
        let ty = self.store.get(id);
        if ty.fresh || !ty.flags.intersects(TypeFlags::FRESHABLE) {
            return id;
        }
        let (flags, data) = (ty.flags, ty.data.clone());
        self.store.intern_literal(flags, data, true)
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
    /// Upstream's second condition asks whether the type's symbol is an enum
    /// *member*, which distinguishes `E.A` from `E` — both are `EnumLike`, and
    /// answering the enum for the enum would be a no-op at best. Here that
    /// question is the presence of a [`Checker::enum_member_owners`] entry: the
    /// table is written only where a member type is created
    /// (`crate::declared::get_declared_type_of_enum`), so having an entry *is*
    /// being a member type. The flags test is kept anyway rather than relying on
    /// the table alone, because it is upstream's guard and the two could only
    /// disagree if the table were populated wrongly — in which case the flags
    /// test is what catches it.
    pub(crate) fn get_base_type_of_enum_like_type(&mut self, id: TypeId) -> TypeId {
        if !self.store.get(id).flags.intersects(TypeFlags::ENUM_LIKE) {
            return id;
        }
        let Some(&owner) = self.enum_member_owners.get(&id) else {
            return id;
        };
        self.get_declared_type_of_symbol(owner)
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
}
