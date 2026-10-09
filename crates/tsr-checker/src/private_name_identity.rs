//! A private name's per-class identity, for the relation's property walk.
//!
//! Upstream names a private identifier's symbol after its declaring class
//! (`binder.GetSymbolNameForPrivateIdentifier`, `binder/binder.go:369`:
//! `__#<class symbol id>@#foo`). Two classes that both declare `#foo` declare
//! two different property names, so:
//!
//! - a derived class that redeclares `#foo` still **inherits** its base's
//!   `#foo` under the base's name (`addInheritedMembers` keeps every base
//!   member whose name the derived class does not declare, and the names
//!   differ), and relating the derived instance type to the base finds the
//!   base's own symbol there (`propertiesRelatedTo`, `relater.go:4100`);
//! - a **static** `#foo` is never inherited, and `getUnmatchedPropertiesWorker`
//!   (`relater.go:984`) skips `isStaticPrivateIdentifierProperty` targets
//!   outright (its own TODO cites `privateNamesAndStaticFields`).
//!
//! This port keys members by their text, so the derived `#foo` shadows the
//! inherited one and `typeof X` sources look up a static `#foo` by spelling.
//! The two questions below answer what the mangled lookup would, from the
//! declarations. They are pure reads with no cache: the base walk is the
//! memoized `base_symbols_of_ex` (`members.rs`).
//!
//! `docs/parity/notes/r6-smallcodes5.md` §2.1.

use tsr_ast::{Node, SyntaxKind};
use tsr_binder::SymbolId;

use crate::{
    check::{has_modifier, modifiers_of},
    checker::Checker,
    types::{TypeData, TypeId},
};

impl Checker<'_, '_> {
    /// `isStaticPrivateIdentifierProperty(prop)` (`checker.go`) for the
    /// property `name` of `target`: its value declaration is a class element
    /// named by a private identifier and carrying `static`.
    pub(crate) fn is_static_private_identifier_property(
        &mut self,
        target: TypeId,
        name: &str,
    ) -> bool {
        if !name.starts_with('#') {
            return false;
        }
        let Some(property) = self.get_property_of_type(target, name) else { return false };
        let Some(declaration) = self.binder.symbols().get(property).value_declaration else {
            return false;
        };
        let Some(node) = self.node_map.get(declaration) else { return false };
        let private_named = self
            .declaration_name_of(declaration)
            .is_some_and(|name| self.nodes.kind(name) == SyntaxKind::PrivateIdentifier);
        private_named
            && modifiers_of(node)
                .is_some_and(|modifiers| has_modifier(modifiers, SyntaxKind::StaticKeyword))
    }

    /// Does `source`, a class or interface instance type, inherit the
    /// private member `target_property` under its declaring class's name?
    ///
    /// That is upstream's `getPropertyOfType(source, "__#X@#foo")` for the
    /// class `X` that declares `target_property`, when the source's own
    /// `#foo` (by text) is another class's: `X` must be the source's class or
    /// one of its bases. `None` where the answer needs more than the
    /// declarations: a source this walk cannot name a class for, a base it
    /// cannot follow, or an inherited member of a generic declaring class,
    /// whose type depends on the base's type arguments.
    pub(crate) fn source_inherits_private_member(
        &mut self,
        source: TypeId,
        target_property: SymbolId,
    ) -> Option<bool> {
        let declaration = self.binder.symbols().get(target_property).value_declaration?;
        let class_node = self.nodes.parent(declaration)?;
        if !matches!(
            self.node_map.get(class_node),
            Some(Node::ClassDeclaration(_) | Node::ClassExpression(_))
        ) {
            return None;
        }
        let declaring = self.binder.merged_symbol(self.binder.symbol_of(class_node)?);
        let owner = match &self.store.get(source).data {
            TypeData::Named { members: Some(owner), .. } => *owner,
            _ => self.type_reference_targets.get(&source).map(|&(owner, _)| owner)?,
        };
        let mut pending = vec![self.binder.merged_symbol(owner)];
        let mut visited: Vec<SymbolId> = Vec::new();
        while let Some(current) = pending.pop() {
            if current == declaring {
                return self.local_type_parameters_of(declaring).is_empty().then_some(true);
            }
            if visited.contains(&current) {
                continue;
            }
            visited.push(current);
            for base in self.base_symbols_of_ex(current, false)? {
                pending.push(self.binder.merged_symbol(base));
            }
        }
        Some(false)
    }
}
