//! Looking inside an object type: property access and `getPropertyOfType`.
//!
//! Ported from `checker.go:11244`, `:11258` and `getPropertyOfType`. Split out
//! of [`crate::checker`] because the next work here is **inherited members** —
//! a base class's properties are not in the derived symbol's table — which is a
//! binder-side question and is owned separately from the expression forms.

use tsr_binder::SymbolId;

use crate::{
    checker::Checker,
    types::{TypeData, TypeId},
};

impl Checker<'_, '_> {
    /// Ported from `Checker.checkPropertyAccessExpression` into
    /// `checkPropertyAccessExpressionOrQualifiedName` (`checker.go:11244`,
    /// `:11258`), reduced to the lookup.
    ///
    /// Upstream takes the receiver's **apparent** type first, which is what makes
    /// `"a".length` work: a primitive's apparent type is its wrapper interface
    /// from `lib.d.ts`. There are no lib files (`bd tsr-9or.1`), so a primitive
    /// receiver has no members here and answers `errorType` — a gap the histogram
    /// attributes to lib rather than to this function.
    ///
    /// Not ported: optional chains, private identifiers, `super`, index
    /// signatures, and **inherited members** — a base class's properties are not
    /// in the derived symbol's table, so `class C extends B {}` finds nothing of
    /// `B`'s. All answer `errorType`.
    pub fn check_property_access_expression(
        &mut self,
        node: &tsr_ast::PropertyAccessExpression<'_>,
    ) -> TypeId {
        let error = self.intrinsics.error;
        let (Some(receiver), Some(tsr_ast::MemberName::Identifier(name))) =
            (node.expression, node.name)
        else {
            return error;
        };
        let receiver_type = self.check_expression(receiver);
        // No explicit test for an `errorType` receiver: it is an intrinsic and
        // never carries a members table, so the lookup below misses and answers
        // `errorType` anyway. An earlier draft guarded it and no mutation could
        // make the guard observable, so it was removed rather than kept as
        // decoration.
        match self.get_property_of_type(receiver_type, name.text) {
            Some(property) => self.get_type_of_symbol(property),
            None => error,
        }
    }

    /// Ported from `Checker.getPropertyOfType` (`checker.go`), reduced to a
    /// members-table lookup.
    ///
    /// Upstream resolves the type's structure first — base types, index
    /// signatures, mapped and intersection members. This asks the one table the
    /// binder already built, which is why inherited and index-signature
    /// properties are misses rather than answers.
    #[must_use]
    pub fn get_property_of_type(&mut self, id: TypeId, name: &str) -> Option<SymbolId> {
        let TypeData::Named { members: Some(owner), .. } = self.store.get(id).data else {
            return None;
        };
        self.binder.symbols().get(owner).members.get(name).copied()
    }
}
