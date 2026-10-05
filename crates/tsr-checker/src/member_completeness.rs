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
use tsr_binder::{SymbolFlags, SymbolId};

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
        // §41's audit: the instantiated-reference decline is **deleted**. It
        // was drawn because a reference's members need substitution this port
        // performs one property at a time — true of the member *types*, and this
        // predicate promises only the member **names**, which instantiation
        // never changes. +4 cases.
        let owner = match &self.store.get(id).data {
            TypeData::Named { members: Some(owner), .. } => *owner,
            // `Anonymous` is `typeof X` — the symbol's `exports`, which
            // `get_property_of_anonymous_symbol` reads without following
            // anything. Its one documented gap is **inherited statics**
            // (`getBaseConstructorTypeOfClass`), which only a *class* can have.
            // A namespace or an enum has no base, so its exports table is the
            // whole table and a `None` from it is an absent member.
            TypeData::Anonymous { symbol, .. } => {
                let symbol = *symbol;
                let flags = self.binder.symbols().get(symbol).flags;
                if flags.intersects(SymbolFlags::MODULE.union(SymbolFlags::ENUM))
                    && !flags.intersects(SymbolFlags::CLASS)
                {
                    return true;
                }
                // A **class** static side is decidable too when the class has no
                // base — `getBaseConstructorTypeOfClass` is the only gap
                // `get_property_of_anonymous_symbol` names for it, and a class
                // that extends nothing cannot reach it.
                if !flags.intersects(SymbolFlags::CLASS) {
                    return false;
                }
                let declarations = self.binder.symbols().get(symbol).declarations.to_vec();
                // A **namespace merged onto the class** — a clodule — is not a
                // reason the static table is incomplete. The gap this arm
                // guards is `getBaseConstructorTypeOfClass`, inherited statics,
                // and a `ModuleDeclaration` is not a base: it contributes more
                // members to the same exports table. The arm directly above
                // already trusts a pure namespace's table, so both halves are
                // independently complete and only their conjunction was not.
                // §349.
                return declarations.iter().all(|declaration| {
                    match self.node_map.get(*declaration) {
                        Some(Node::ClassDeclaration(class)) => {
                            class.type_parameters.is_empty() && class.heritage_clauses.is_empty()
                        }
                        Some(Node::ModuleDeclaration(_)) => true,
                        _ => false,
                    }
                });
            }
            _ => return false,
        };
        let mut visiting = Vec::new();
        self.symbol_members_are_complete(owner, &mut visiting, 0)
    }

    /// Every declared property of `id`, with whether it is optional — or `None`
    /// where the walk is not complete.
    ///
    /// The same traversal as [`Checker::declared_members_are_complete`] with the
    /// **index-signature condition removed**, and that difference is the whole
    /// reason this is a second entry point rather than a flag.
    ///
    /// An index signature changes what a *property access* means — `a.anything`
    /// is legal — so [`crate::nonexistent_property`] must decline it. It changes
    /// nothing about what a *required property* is: `getPropertyOfType` does not
    /// answer from an index signature, so `{ [k: string]: any }` is still
    /// missing `one` when assigned to `{ one: number }`. That is
    /// `assignmentCompat1`, and it is the case that would be silently lost if
    /// the two questions shared one predicate.
    pub(crate) fn declared_property_table(&mut self, id: TypeId) -> Option<Vec<(String, bool)>> {
        self.declared_property_table_worker(id, 0, false)
    }

    /// The property table the relation reporters (TS2741/TS2739/TS2353) read:
    /// [`Checker::declared_property_table`], or for a *widened, regular*
    /// object-literal type — `var a = { x: 1, y: 2 }`'s declared type — the
    /// literal's own written member list.
    ///
    /// Deliberately a separate entry from the shared predicates: TS2339
    /// (`crate::nonexistent_property`) must not read an object-literal
    /// constituent as complete, because upstream's union property lookup
    /// (`createUnionOrIntersectionProperty`) treats a member an object literal
    /// lacks as `undefined` rather than absent (`nonPrimitiveAndEmptyObject`).
    pub(crate) fn relation_property_table(&mut self, id: TypeId) -> Option<Vec<(String, bool)>> {
        self.declared_property_table(id).or_else(|| self.object_literal_property_table(id))
    }

    /// [`Checker::declared_members_are_complete`] widened the same way, for
    /// the excess-property check's "every name is known" question. An object
    /// literal declares no index signature.
    pub(crate) fn relation_members_are_complete(&mut self, id: TypeId) -> bool {
        self.declared_members_are_complete(id) || self.object_literal_property_table(id).is_some()
    }

    /// A regular object-literal type's written member list, in source order.
    /// Complete exactly when every element has a literal name: a spread
    /// contributes properties that live only in the checked type
    /// (`getSpreadType`), and a computed name is late-bound or becomes an index
    /// signature (`checkObjectLiteral`). JavaScript literals are declined:
    /// assignment declarations can add expando members to the literal's symbol.
    /// A fresh literal is read from its captured list by
    /// `declared_property_table` and never reaches here.
    fn object_literal_property_table(&mut self, id: TypeId) -> Option<Vec<(String, bool)>> {
        let TypeData::Named { members: Some(owner), .. } = self.store.get(id).data else {
            return None;
        };
        let &[declaration] = self.binder.symbols().get(owner).declarations.as_slice() else {
            return None;
        };
        let Some(Node::ObjectLiteralExpression(literal)) = self.node_map.get(declaration) else {
            return None;
        };
        if self.in_js_file(declaration)
            || !literal.properties.iter().all(|property| match property {
                tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                    self.name_is_written(assignment.name)
                }
                tsr_ast::ObjectLiteralElementLike::ShorthandPropertyAssignment(_) => true,
                tsr_ast::ObjectLiteralElementLike::MethodDeclaration(method) => {
                    self.name_is_written(method.name)
                }
                tsr_ast::ObjectLiteralElementLike::GetAccessorDeclaration(accessor) => {
                    self.name_is_written(accessor.name)
                }
                tsr_ast::ObjectLiteralElementLike::SetAccessorDeclaration(accessor) => {
                    self.name_is_written(accessor.name)
                }
                tsr_ast::ObjectLiteralElementLike::SpreadAssignment(_) => false,
            })
        {
            return None;
        }
        let mut members: Vec<(u32, String)> = self
            .binder
            .symbols()
            .get(owner)
            .members
            .iter()
            .filter(|(_, id)| self.binder.symbols().get(**id).flags.intersects(SymbolFlags::VALUE))
            .map(|(name, id)| {
                let start = self
                    .binder
                    .symbols()
                    .get(*id)
                    .declarations
                    .iter()
                    .map(|&member| self.nodes.span(member).start)
                    .min()
                    .unwrap_or(u32::MAX);
                (start, (*name).to_string())
            })
            .collect();
        members.sort_unstable();
        Some(members.into_iter().map(|(_, name)| (name, false)).collect())
    }

    fn declared_property_table_worker(
        &mut self,
        id: TypeId,
        depth: u32,
        mapped_container: bool,
    ) -> Option<Vec<(String, bool)>> {
        if depth > MAX_BASE_DEPTH {
            return None;
        }
        // check_object_literal publishes this list only when capture_complete
        // holds. Its bool marks synthetic lookup, not completeness. Keep
        // incomplete literals and regularization's fallback tables declined.
        if self.fresh_object_literal_types.contains(&id) {
            return self.anonymous_properties.get(&id).map(|(properties, _)| {
                properties
                    .iter()
                    .map(|property| (property.name.clone(), property.optional))
                    .collect()
            });
        }
        // An identity map preserves its source's own keys, but replaces the
        // declaration's optionality. Compose modifiers only after certifying
        // the source table; an ordinary generic reference still declines.
        let optionality = self.mapped_identity_optionality.get(&id).map(|&(optional, _)| optional);
        let mut out = if let Some((_, arguments)) = self.type_reference_targets.get(&id) {
            optionality?;
            let [source] = arguments.as_slice() else { return None };
            self.declared_property_table_worker(*source, depth + 1, true)?
        } else {
            let owner = match &self.store.get(id).data {
                TypeData::Named { members: Some(owner), .. } => *owner,
                _ => return None,
            };
            let mut visiting = Vec::new();
            let mut out = Vec::new();
            if !self.collect_declared_properties(
                owner,
                &mut out,
                &mut visiting,
                0,
                (mapped_container || optionality.is_some()).then_some(id),
            ) {
                return None;
            }
            out
        };
        if let Some(Some(optional)) = optionality {
            for (_, member_optional) in &mut out {
                *member_optional = optional;
            }
        }
        Some(out)
    }

    /// One step of the property enumeration. Own members shadow inherited ones,
    /// exactly as `get_property_of_declared_symbol`'s first-hit-wins walk does.
    fn collect_declared_properties(
        &mut self,
        owner: SymbolId,
        out: &mut Vec<(String, bool)>,
        visiting: &mut Vec<SymbolId>,
        depth: u32,
        mapped_container: Option<TypeId>,
    ) -> bool {
        if depth > MAX_BASE_DEPTH || visiting.contains(&owner) {
            return false;
        }
        visiting.push(owner);
        let declarations = self.binder.symbols().get(owner).declarations.to_vec();
        if declarations.is_empty() {
            return false;
        }
        for &declaration in &declarations {
            if !self.declaration_property_names_are_readable(declaration) {
                return false;
            }
        }
        let members: Option<Vec<_>> = self
            .binder
            .symbols()
            .get(owner)
            .members
            .iter()
            .filter(|(_, id)| self.binder.symbols().get(**id).flags.intersects(SymbolFlags::VALUE))
            .map(|(name, id)| {
                // Owner declarations are in binder/source order, including
                // merges. Within each, use written position, not SymbolId:
                // merging can allocate or reuse symbols in a different order.
                let order = self
                    .binder
                    .symbols()
                    .get(*id)
                    .declarations
                    .iter()
                    .filter_map(|&member| {
                        declarations
                            .iter()
                            .position(|&owner| {
                                self.nodes.ancestors(member).any(|node| node == owner)
                            })
                            .map(|owner| (owner, self.nodes.span(member).start))
                    })
                    .min()?;
                Some((order, (*name).to_string(), *id))
            })
            .collect();
        let Some(mut members) = members else { return false };
        members.sort_unstable_by_key(|(order, _, _)| *order);
        for (_, name, symbol) in members {
            let entry = self.binder.symbols().get(symbol);
            if out.iter().any(|(seen, _)| *seen == name) {
                continue;
            }
            // `SymbolFlagsOptional` is set by upstream's binder and by nothing
            // in this one — the same class of trap `NodeFlags::AMBIENT` was, and
            // caught the same way: the first measurement reported TS2741 for
            // every *optional* property of every target
            // (`assignmentCompatWithObjectMembersOptionality2`, 3 lines on one
            // case). Optionality is read off the declaration's `?` instead.
            let declarations = entry.declarations.to_vec();
            let optional = declarations.iter().any(|d| self.declaration_is_optional_member(*d));
            out.push((name, optional));
        }
        let Some(bases) = self.base_symbols_of(owner) else { return false };
        for base in bases {
            if !self.collect_declared_properties(base, out, visiting, depth + 1, None) {
                return false;
            }
        }
        if let Some(source) = mapped_container {
            // getNamedMembers sorts a mapped table by linked declarations,
            // without the class/interface own-before-inherited partition.
            // Only sort after the entire source walk proved complete. File
            // roots in the shared NodeTable are registered in program order.
            let mut ordered = Vec::with_capacity(out.len());
            for (name, optional) in std::mem::take(out) {
                let Some(symbol) = self.get_property_of_type(source, &name) else {
                    return false;
                };
                // getLiteralTypeFromProperty excludes non-public keyof keys.
                if self.is_non_public_member(symbol) {
                    continue;
                }
                let Some(&declaration) = self.binder.symbols().get(symbol).declarations.first()
                else {
                    return false;
                };
                if self.declaration_names_a_private(declaration) {
                    continue;
                }
                let Some(file) = self.source_file_of_for_diagnostics(declaration) else {
                    return false;
                };
                ordered.push(((file, self.nodes.span(declaration).start), name, optional));
            }
            ordered.sort_unstable_by_key(|(order, _, _)| *order);
            out.extend(ordered.into_iter().map(|(_, name, optional)| (name, optional)));
        }
        true
    }

    /// Does this member declaration carry a `?`?
    fn declaration_is_optional_member(&self, declaration: NodeId) -> bool {
        match self.node_map.get(declaration) {
            Some(Node::PropertySignatureDeclaration(property)) => property.postfix_token.is_some(),
            Some(Node::PropertyDeclaration(property)) => property.postfix_token.is_some(),
            Some(Node::MethodSignatureDeclaration(method)) => method.postfix_token.is_some(),
            Some(Node::MethodDeclaration(method)) => method.postfix_token.is_some(),
            Some(Node::ParameterDeclaration(_)) => self.is_optional_declaration(declaration),
            _ => false,
        }
    }

    /// The completeness condition minus the index-signature one — see
    /// [`Checker::declared_property_table`].
    fn declaration_property_names_are_readable(&mut self, declaration: NodeId) -> bool {
        match self.node_map.get(declaration) {
            Some(Node::ClassDeclaration(class)) => {
                class.type_parameters.is_empty()
                    && class.members.iter().all(|member| self.class_member_name_is_written(*member))
            }
            Some(Node::ClassExpression(class)) => {
                class.type_parameters.is_empty()
                    && class.members.iter().all(|member| self.class_member_name_is_written(*member))
            }
            Some(Node::InterfaceDeclaration(interface)) => {
                interface.type_parameters.is_empty()
                    && interface
                        .members
                        .iter()
                        .all(|member| self.type_member_name_is_written(*member))
            }
            Some(Node::TypeLiteralNode(literal)) => {
                literal.members.iter().all(|member| self.type_member_name_is_written(*member))
            }
            _ => false,
        }
    }

    fn class_member_name_is_written(&self, member: ClassElement<'_>) -> bool {
        match member {
            ClassElement::PropertyDeclaration(property) => self.name_is_written(property.name),
            ClassElement::MethodDeclaration(method) => self.name_is_written(method.name),
            ClassElement::GetAccessorDeclaration(accessor) => self.name_is_written(accessor.name),
            ClassElement::SetAccessorDeclaration(accessor) => self.name_is_written(accessor.name),
            _ => true,
        }
    }

    fn type_member_name_is_written(&self, member: TypeElement<'_>) -> bool {
        match member {
            TypeElement::PropertySignatureDeclaration(property) => {
                self.name_is_written(property.name)
            }
            TypeElement::MethodSignatureDeclaration(method) => self.name_is_written(method.name),
            TypeElement::GetAccessorDeclaration(accessor) => self.name_is_written(accessor.name),
            TypeElement::SetAccessorDeclaration(accessor) => self.name_is_written(accessor.name),
            _ => true,
        }
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
                class.members.iter().all(|member| self.class_member_is_plain(*member))
            }
            Some(Node::ClassExpression(class)) => {
                class.members.iter().all(|member| self.class_member_is_plain(*member))
            }
            Some(Node::InterfaceDeclaration(interface)) => {
                interface.members.iter().all(|member| self.type_member_is_plain(*member))
            }
            // A **type literal** — `{ id: number }` — is an interface's member
            // list without the interface: no type parameters to instantiate and
            // no `extends` to follow, so the same member test settles it. It is
            // the single most common target shape in the corpus's assignability
            // cases, and leaving it out made `declared_property_table` answer
            // `None` for most of them.
            Some(Node::TypeLiteralNode(literal)) => {
                literal.members.iter().all(|member| self.type_member_is_plain(*member))
            }
            Some(Node::JSDocTypedefTag(_)) => {
                // bind_jsdoc_declarations exposes sibling members only for an
                // Object/object body. Prove the entire document uses that
                // table, not a merged alias or a partially gathered body.
                let Some(owner) = self.binder.symbol_of(declaration) else { return false };
                let entry = self.binder.symbols().get(owner);
                if entry.declarations.as_slice() != [declaration] || entry.members.is_empty() {
                    return false;
                }
                let Some(doc) =
                    self.jsdoc_entries.values().flat_map(|docs| docs.iter().copied()).find(|doc| {
                        doc.tags.iter().any(|tag| {
                            matches!(tag, tsr_ast::JSDocTag::JSDocTypedefTag(tag)
                                if tag.node_id == Some(declaration))
                        })
                    })
                else {
                    return false;
                };
                doc.tags.iter().all(|tag| match tag {
                    tsr_ast::JSDocTag::JSDocTypedefTag(tag) => tag.node_id == Some(declaration),
                    tsr_ast::JSDocTag::JSDocTypeTag(_) => false,
                    tsr_ast::JSDocTag::JSDocParameterOrPropertyTag(property)
                        if property.kind.kind == SyntaxKind::JSDocPropertyTag =>
                    {
                        let Some(tsr_ast::EntityName::Identifier(name)) = property.name else {
                            return false;
                        };
                        property.node_id.and_then(|id| self.binder.symbol_of(id)).is_some_and(
                            |member| {
                                self.binder.symbols().get(member).parent == Some(owner)
                                    && entry.members.contains_key(name.text)
                            },
                        )
                    }
                    _ => true,
                })
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

#[cfg(test)]
mod tests {
    use super::Checker;

    #[test]
    fn parameter_properties_need_certified_optionality_for_a_complete_table() {
        let source = "class Optional { constructor(public z?: number) {} }\n\
                      class Required { constructor(public z: number) {} }\n\
                      class Defaulted { constructor(public z = 0) {} }\n\
                      class Field { z?: number; }\n\
                      class Plain { constructor(z?: number) {} }\n\
                      class Generic<T> { constructor(public z?: T) {} }";
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let root = parsed.source_file.node_id.expect("registered file");
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "parameters.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        for (name, expected) in [
            ("Optional", Some(vec![("z".to_owned(), true)])),
            ("Required", Some(vec![("z".to_owned(), false)])),
            ("Defaulted", Some(vec![("z".to_owned(), false)])),
            ("Field", Some(vec![("z".to_owned(), true)])),
            ("Plain", Some(vec![])),
            ("Generic", None),
        ] {
            let symbol = bound.lookup_local(root, name).expect("class bound");
            let ty = checker.get_declared_type_of_symbol(symbol);
            assert_eq!(checker.declared_property_table(ty), expected, "{name}");
        }
    }

    const BOUNDARY_SOURCE: &str = r"/** @typedef {Object} Plain @property {string} present */
;
/** @template T @typedef {Object} Generic @property {T} present */
;
/** @typedef {object} Lower @property {number} present */
;
/** @typedef {Object} Qualified @property {Object} child @property {string} child.present */
;
/** @typedef {Object} Replaced @property {string} ignored @type {{ kept: number }} */
;
function first() {
    /** @typedef {Object} Merged @property {number} left */
    ;
    /** @param {Merged} item */
    function read(item) { item.left; }
}
function second() {
    /** @typedef {Object} Merged @property {string} right */
    ;
    /** @param {Merged} item */
    function read(item) { item.right; }
}
/** @typedef {Object} Multiple @property {string} left @typedef {Object} Other @property {number} right */
;
/** @param {Qualified} qualified @param {Replaced} replaced */
function read(qualified, replaced) { qualified.child.present; replaced.kept; }
";

    #[test]
    fn only_unambiguous_fully_bound_sibling_typedefs_are_complete() {
        let arena = tsr_core::Arena::new();
        let mut parsed = tsr_parser::parse_with_options(
            &arena,
            BOUNDARY_SOURCE,
            tsr_parser::ParseOptions::for_file("boundary.js"),
        );
        assert!(parsed.diagnostics.is_empty());
        let root = parsed.source_file.node_id.expect("registered file");
        parsed.nodes.add_flags(root, tsr_ast::NodeFlags::JAVASCRIPT_FILE);
        let docs: Vec<_> = parsed.jsdoc.iter().collect();
        let bound = tsr_binder::bind_into_with_jsdoc(
            tsr_binder::BindResult::empty(),
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "boundary.js", text: BOUNDARY_SOURCE },
            &docs,
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        checker.set_jsdoc(parsed.jsdoc.iter());
        for (name, expected) in [
            ("Plain", vec![true]),
            ("Generic", vec![true]),
            ("Lower", vec![true]),
            ("Qualified", vec![false]),
            ("Replaced", vec![false]),
            ("Merged", vec![false, false]),
            ("Multiple", vec![false]),
            ("Other", vec![false]),
        ] {
            let symbol = bound.lookup_local(root, name).expect("typedef bound");
            let actual: Vec<_> = bound
                .symbols()
                .get(symbol)
                .declarations
                .iter()
                .map(|&declaration| checker.declaration_members_are_complete(declaration))
                .collect();
            assert_eq!(actual, expected, "{name}");
        }
    }
}
