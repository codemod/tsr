//! `class C implements Alias` where `Alias` names a type alias, not a class or
//! interface.
//!
//! `checkClassLikeDeclaration`'s implemented-type loop (`checker.go:4365`)
//! takes `getReducedType(getTypeFromTypeNode(typeRefNode))` for every entry,
//! whatever declares the name. If the type is not an error and
//! `isValidBaseType` holds (an object type, `object`, `any`, or an
//! intersection of those), it relates the class to it exactly as it relates
//! an interface. The message is the class one when `t.symbol` is a class,
//! else the interface one. So `type Wrapper = Foo & Bar; class Baz implements
//! Wrapper` reports TS2416 on `Baz`'s members against `Foo & Bar`.
//!
//! `check_class_implemented_types` (`heritage_conformance.rs`) resolved only
//! class and interface symbols. This file answers an alias entry's type for
//! that loop:
//!
//! - **ported:** a non-generic alias written without type arguments, whose
//!   type is its declared type;
//! - **declined:** a generic alias (its instantiation goes through the
//!   alias-instantiation road this loop does not call), a gap, and an
//!   invalid base. The invalid base is TS2422, which is not ported.
//!
//! No cache or side table. The declared type is the alias's own memo.
//!
//! `docs/parity/notes/r6-smallcodes5.md` §2.3.

use tsr_ast::TypeNode;
use tsr_binder::{SymbolFlags, SymbolId};

use crate::{
    checker::Checker,
    flags::TypeFlags,
    types::{TypeData, TypeId},
};

impl Checker<'_, '_> {
    /// The implemented type of an `implements` entry that names the type
    /// alias `alias`, and whether `t.symbol` is a class (which picks the
    /// message), or `None` where the loop declines.
    pub(crate) fn implemented_alias_type(
        &mut self,
        alias: SymbolId,
        type_arguments: &[TypeNode<'_>],
    ) -> Option<(TypeId, bool)> {
        if !self.binder.symbols().get(alias).flags.contains(SymbolFlags::TYPE_ALIAS)
            || !type_arguments.is_empty()
            || !self.local_type_parameters_of(alias).is_empty()
        {
            return None;
        }
        let ty = self.get_declared_type_of_symbol(alias);
        if self.is_error(ty) || self.is_gap(ty) || !self.is_valid_implemented_type(ty) {
            return None;
        }
        // `getTypeWithThisArgument(t, thisType, false)`: an intersection is
        // rebuilt from its constituents (`checker.go`), so the alias it was
        // written through is gone and the base prints as `Foo & Bar`. Each
        // constituent's own this-argument is not modelled, as for every
        // heritage target in this port.
        let ty = match &self.store.get(ty).data {
            TypeData::Intersection { types, .. } => {
                let types = types.clone();
                self.get_intersection_type(&types, None)
            }
            _ => ty,
        };
        let symbol = match &self.store.get(ty).data {
            TypeData::Named { members, .. } => *members,
            _ => self.type_reference_targets.get(&ty).map(|&(symbol, _)| symbol),
        };
        let is_class = symbol.is_some_and(|symbol| {
            self.binder.symbols().get(symbol).flags.contains(SymbolFlags::CLASS)
        });
        Some((ty, is_class))
    }

    /// `isValidBaseType` (`checker.go:19537`) for an implemented alias type:
    /// an object type, `object` or `any`, or an intersection of those. The
    /// type-parameter arm cannot arise from a non-generic alias.
    fn is_valid_implemented_type(&mut self, ty: TypeId) -> bool {
        let flags = self.store.get(ty).flags;
        if flags.intersects(TypeFlags::OBJECT | TypeFlags::NON_PRIMITIVE | TypeFlags::ANY) {
            // `!isGenericMappedType(t)`: a mapped type over a generic key set.
            return !self.mapped_types.get(&ty).is_some_and(|info| {
                self.maybe_type_of_kind(info.constraint, TypeFlags::INSTANTIABLE)
            });
        }
        if let TypeData::Intersection { types, .. } = &self.store.get(ty).data {
            let types = types.clone();
            return types.into_iter().all(|part| self.is_valid_implemented_type(part));
        }
        false
    }
}
