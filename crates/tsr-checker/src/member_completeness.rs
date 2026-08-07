//! Does this port know a type's member table *completely*?
//!
//! # The question §9 refused for want of
//!
//! `docs/architecture/checker-notes-diag2.md` §9 built TS2339, measured **2
//! conversions against 254 wrong lines**, and refused it with a diagnosis rather
//! than a number:
//!
//! > **An absent property and an unbuilt members table are the same `None`, and
//! > no predicate over the *type* separates them.**
//!
//! That sentence is exactly right about a predicate over the type, and it names
//! its own retirement condition: *"a members table that knows whether it is
//! complete — `resolveStructuredTypeMembers` ported with an explicit
//! resolved/unresolved state per type, rather than an `Option` that conflates
//! 'no members' with 'not yet'."*
//!
//! This module is that state, and the reason it can exist without porting
//! `resolveStructuredTypeMembers` is that it asks the question **of the walk
//! instead of the type**. `Named { members: Some(_) }` is a flag and says only
//! that *a* table was built. [`Checker::declared_members_are_complete`] re-walks
//! the same graph [`crate::members::Checker::get_property_of_type`] walks and
//! answers `false` the moment the walk reaches anything this port resolves
//! lazily, partially, or not at all. A complete answer is therefore a *property
//! of the traversal that produced it*, which is what §9 found no flag could be.
//!
//! # Every `false` is a named unported mechanism
//!
//! The list is not a heuristic; it is §9's own wrong column read back as
//! conditions. Each entry names the case that put it there.
//!
//! | answers `false` | the case in §9's residual |
//! |---|---|
//! | the type is an instantiated reference, or its owner declares type parameters | `longObjectInstantiationChain1`/`3` (13 lines each), `genericDefaults` (9) |
//! | any base cannot be followed ([`crate::members`]'s `base_symbols_of` answers `None`) | `mixinAccessModifiers` (9), `classExtendingClassLikeType` |
//! | a declaration is not a class or interface — a mapped type, a conditional, a type literal reached through an alias | `conditionalTypes1` (8), `mappedTypes6` |
//! | the declaration carries an **index signature** | any receiver where every name is legal |
//! | a member is declared with a **computed name** | late binding is unported (`crate::lib`'s header) |
//! | the walk revisits a symbol | `recursiveIntersectionTypes` |
//!
//! `discriminatedUnionTypes2` needed no entry: a union receiver never reaches
//! here, because only `TypeData::Named` does.

use tsr_ast::{ClassElement, Node, NodeId, SyntaxKind, TypeElement};
use tsr_binder::SymbolId;

use crate::{checker::Checker, types::TypeData, types::TypeId};

/// How deep the completeness walk will follow base types.
///
/// A bound rather than a correctness limit: past it the answer is `false`, which
/// is a *missing* diagnostic. The corpus contains inheritance chains written to
/// break compilers.
const MAX_BASE_DEPTH: u32 = 32;

impl Checker<'_, '_> {
    /// Is every property of `id` reachable by this port's own member walk?
    ///
    /// `true` licenses a consumer to treat a `None` from
    /// [`crate::members::Checker::get_property_of_type`] as **"the property does
    /// not exist"** rather than as "this port did not find it". Nothing else may
    /// be read into it — in particular it says nothing about whether the member
    /// *types* are right, only about whether the member *names* are all present.
    /// TS2339 needs exactly the second and none of the first, which is why this
    /// is worth having before the members subsystem is finished.
    pub(crate) fn declared_members_are_complete(&mut self, id: TypeId) -> bool {
        // An instantiated reference's members need substitution this port only
        // performs at `instantiate_for_reference`, one property at a time; the
        // *table* is the uninstantiated one.
        if self.type_reference_targets.contains_key(&id) {
            return false;
        }
        let owner = match &self.store.get(id).data {
            TypeData::Named { members: Some(owner), .. } => *owner,
            // `Anonymous` is `typeof X` — a namespace's or class's `exports`,
            // which `get_property_of_anonymous_symbol` reads without following
            // anything. Its inherited statics are a documented gap there, so the
            // table is not complete either.
            _ => return false,
        };
        let mut visiting = Vec::new();
        self.symbol_members_are_complete(owner, &mut visiting, 0)
    }

    /// One step of the completeness walk, mirroring
    /// `get_property_of_declared_symbol`'s.
    fn symbol_members_are_complete(
        &mut self,
        owner: SymbolId,
        visiting: &mut Vec<SymbolId>,
        depth: u32,
    ) -> bool {
        if depth > MAX_BASE_DEPTH || visiting.contains(&owner) {
            return false;
        }
        visiting.push(owner);
        let declarations = self.binder.symbols().get(owner).declarations.to_vec();
        if declarations.is_empty() {
            return false;
        }
        for declaration in declarations {
            if !self.declaration_members_are_complete(declaration) {
                return false;
            }
        }
        // `base_symbols_of` already answers `None` for every base it cannot
        // follow — type arguments, a non-identifier `extends`, a name that is
        // not a class or interface. Its contract is the load-bearing half of
        // this predicate and is documented at its own definition.
        let Some(bases) = self.base_symbols_of(owner) else { return false };
        for base in bases {
            if !self.symbol_members_are_complete(base, visiting, depth + 1) {
                return false;
            }
        }
        true
    }

    /// Is one declaration's member list fully readable?
    fn declaration_members_are_complete(&mut self, declaration: NodeId) -> bool {
        match self.node_map.get(declaration) {
            Some(Node::ClassDeclaration(class)) => {
                class.type_parameters.is_empty()
                    && class.members.iter().all(|member| self.class_member_is_plain(*member))
            }
            Some(Node::ClassExpression(class)) => {
                class.type_parameters.is_empty()
                    && class.members.iter().all(|member| self.class_member_is_plain(*member))
            }
            Some(Node::InterfaceDeclaration(interface)) => {
                interface.type_parameters.is_empty()
                    && interface.members.iter().all(|member| self.type_member_is_plain(*member))
            }
            // A class merged with a namespace, an enum, a variable — the symbol's
            // member table is then assembled from somewhere this walk does not
            // read.
            _ => false,
        }
    }

    /// A class member whose *name* the binder recorded verbatim.
    ///
    /// An index signature makes every name legal, so a receiver carrying one can
    /// never produce TS2339. A computed name is late-bound upstream and is not
    /// bound at all here (`tsr-binder`'s module header), so a member declared
    /// `[k]: T` is present upstream and absent in this port's table — the exact
    /// shape §9's refusal is about.
    fn class_member_is_plain(&self, member: ClassElement<'_>) -> bool {
        match member {
            ClassElement::IndexSignatureDeclaration(_) => false,
            ClassElement::PropertyDeclaration(property) => self.name_is_written(property.name),
            ClassElement::MethodDeclaration(method) => self.name_is_written(method.name),
            ClassElement::GetAccessorDeclaration(accessor) => self.name_is_written(accessor.name),
            ClassElement::SetAccessorDeclaration(accessor) => self.name_is_written(accessor.name),
            _ => true,
        }
    }

    /// The same question for an interface's members.
    fn type_member_is_plain(&self, member: TypeElement<'_>) -> bool {
        match member {
            TypeElement::IndexSignatureDeclaration(_) => false,
            TypeElement::PropertySignatureDeclaration(property) => {
                self.name_is_written(property.name)
            }
            TypeElement::MethodSignatureDeclaration(method) => self.name_is_written(method.name),
            TypeElement::GetAccessorDeclaration(accessor) => self.name_is_written(accessor.name),
            TypeElement::SetAccessorDeclaration(accessor) => self.name_is_written(accessor.name),
            _ => true,
        }
    }

    /// Is this member name a literal the binder could record?
    fn name_is_written(&self, name: tsr_ast::PropertyName<'_>) -> bool {
        name.node_id().is_some_and(|id| self.nodes.kind(id) != SyntaxKind::ComputedPropertyName)
    }
}
