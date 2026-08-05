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

    /// The widened form of a literal type.
    ///
    /// Ported from `Checker.getWidenedLiteralType` (`checker.go:25487`).
    /// **Widens only a *fresh* literal**, which is the whole reason freshness is
    /// tracked: `let x = "a"` widens because the expression `"a"` is fresh, while
    /// `let x: "a"` does not, because a literal type node is regular.
    pub fn get_widened_literal_type(&mut self, id: TypeId) -> TypeId {
        if let Some(&cached) = self.regular_types.get(&id) {
            return cached;
        }
        let ty = self.store.get(id);
        let flags = ty.flags;
        let widened = if !ty.fresh {
            id
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
