//! Index signatures: `{ [k: string]: number }`, and what `a[i]` means when `i`
//! is not a literal.
//!
//! Ported from `Checker.getIndexInfosOfType` (`checker.go:18974`),
//! `getApplicableIndexInfo` (`:19008`) and `findApplicableIndexInfo`, with the
//! `indexInfos` themselves read off the declarations that `resolveDeclaredMembers`
//! reads them off upstream.
//!
//! # The asymmetry, taken from upstream rather than guessed
//!
//! `isApplicableIndexType` (`checker.go:19040`) is **not symmetric**, and getting
//! it backwards looks right on half the corpus:
//!
//! - a **string** index signature applies to a `string` key *and to a `number`
//!   key* — `{ [k: string]: T }` answers `a[0]`;
//! - a **number** index signature applies only to a `number` key, and to a
//!   string literal that is a numeric name (`a["0"]`), never to `string`.
//!
//! And `findApplicableIndexInfo` adds a precedence rule on top: *"index
//! signatures for type `string` are considered only when no other index
//! signature applies"*, so a type with both answers a numeric access from the
//! **number** signature.
//!
//! Class instance and static indexes are collected separately. Union types
//! retain keys shared by every constituent; intersection types merge equal
//! keys by intersecting their values. Inherited instance indexes are ported;
//! static indexes do not inherit. Mapped indexes use their resolved side table.
//! `noUncheckedIndexedAccess` is applied by the access consumer, not here.
//! Generic heritage substitutes each base index value along the inheritance
//! chain. Qualified heritage and implicit default arguments
//! on bases remain incomplete.

use tsr_ast::{Node, TypeElement};
use tsr_binder::SymbolId;

use crate::{
    checker::Checker,
    flags::TypeFlags,
    types::{TypeData, TypeId},
};

/// Identity of IndexInfo.components in the checker's declaration-list store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexComponentsId(pub(crate) usize);

/// Native `IndexInfo` key, value, readonly flag and declaration/component provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexInfo {
    /// Computed declarations retained by copies, absent on synthesized indexes.
    pub components: Option<IndexComponentsId>,
    /// Source index declaration, retained by copies and cleared by synthesized merges.
    pub declaration: Option<tsr_ast::NodeId>,
    /// `keyType` — a valid primitive, pattern or nongeneric intersection key.
    pub key: TypeId,
    /// `valueType`, what an applicable access yields.
    pub value: TypeId,
    /// `isReadonly`, preserved by instantiation and combined with index values.
    pub readonly: bool,
}

/// One `propertySymbols` entry of `getIndexInfosOfIndexSymbol`: the member's
/// `getTypeOfSymbol` and the name predicates `getObjectLiteralIndexInfo` asks.
struct LateIndexProperty {
    value: TypeId,
    symbol_named: bool,
    numeric_named: bool,
    /// The declaration, when `isSymbolWithComputedName`.
    component: Option<tsr_ast::NodeId>,
}

impl<'a> Checker<'a, '_> {
    /// The index signatures a type declares.
    ///
    /// Ported from `getIndexInfosOfType` (`checker.go:18974`) into
    /// `resolveStructuredTypeMembers`, reduced to reading the
    /// `IndexSignatureDeclaration` members off the declarations of the symbol the
    /// type is named by.
    ///
    /// Named types use their members owner; class constructor objects use
    /// static declarations. Resolved object/mapped types use stored indexes,
    /// and unions/intersections combine their constituents' indexes.
    ///
    /// **`None` is a gap, `Some(vec![])` is "none declared".** The two are not
    /// the same claim: a type whose base this port cannot follow might have an
    /// index signature we cannot see, and answering "no index signatures" for it
    /// would turn a missing answer into a confident wrong one the moment a
    /// caller acts on the emptiness. Today's only caller collapses both to no
    /// answer, which is why the distinction has to be carried here rather than
    /// discovered later.
    pub(crate) fn get_index_infos_of_type(&mut self, id: TypeId) -> Option<Vec<IndexInfo>> {
        if let Some(body) = self.completed_original_array_alias_body(id) {
            return self.get_index_infos_of_type(body);
        }
        let id = self.apparent_mapped_type(id);
        self.resolve_mapped_type_members(id);
        match self.store.get(id).data.clone() {
            TypeData::Union { types, .. } => {
                return self.union_index_infos(&types);
            }
            // resolveIntersectionTypeMembers / appendIndexInfo (checker.go).
            TypeData::Intersection { types, .. } => {
                let mut infos: Vec<IndexInfo> = Vec::new();
                for ty in types {
                    for next in self.get_index_infos_of_type(ty)? {
                        if let Some(info) = infos.iter_mut().find(|info| info.key == next.key) {
                            info.value =
                                self.get_intersection_type(&[info.value, next.value], None);
                            info.readonly &= next.readonly;
                            info.declaration = None;
                            info.components = None;
                        } else {
                            infos.push(next);
                        }
                    }
                }
                return Some(infos);
            }
            _ => {}
        }
        // resolveAnonymousTypeMembers (checker.go): enum values have a reverse
        // numeric index only for an enum type or a number-like exported member.
        // String-only enums have no reverse mapping. This includes const enums
        // even when a separate diagnostic rejects their use as runtime values.
        if let TypeData::Anonymous { symbol, .. } = self.store.get(id).data
            && self.binder.symbols().get(symbol).flags.intersects(tsr_binder::SymbolFlags::ENUM)
        {
            let declared = self.get_declared_type_of_symbol(symbol);
            let members: Vec<_> =
                self.binder.symbols().get(symbol).exports.values().copied().collect();
            let has_reverse_index = self.type_of(declared).flags.contains(TypeFlags::ENUM)
                || members.into_iter().any(|member| {
                    let value = self.get_type_of_symbol(member);
                    self.type_of(value).flags.intersects(TypeFlags::NUMBER_LIKE)
                });
            return Some(if has_reverse_index {
                vec![IndexInfo {
                    components: None,
                    declaration: None,
                    key: self.intrinsics.number,
                    value: self.intrinsics.string,
                    readonly: true,
                }]
            } else {
                Vec::new()
            });
        }
        // §539: an OBJECT LITERAL's index signature is minted at check time
        // and lives in a side table, because the literal's `__object` symbol
        // has no index-signature declaration for `index_infos_of_symbol` to
        // find. Consulted before the symbol road and never after it: a literal
        // that minted one has no declared signatures to merge with.
        if let Some(infos) = self.object_literal_index_infos.get(&id) {
            return Some(infos.clone());
        }
        // §785: the global `Record<K, V>` with a PRIMITIVE key carries an
        // index signature `[k: K]: V`.
        //
        // `Record<K, V>` is `{ [P in K]: V }`, and upstream reaches this
        // through `resolveMappedTypeMembers` (`checker.go`), which walks
        // `getLowerBoundOfKeyType(constraintType)` and — for a key that is not
        // usable as a property name, i.e. exactly `string`/`number`/`symbol` —
        // creates an INDEX INFO rather than a property. Mapped-type member
        // resolution is not ported (see this module's gap list above), so the
        // one alias the corpus actually leans on is special-cased here, the
        // same scoping decision §45 already made for the PROPERTY road in
        // `Checker::record_string_value` — and this is the half that road
        // could not cover, because `m[i]` never asks for a property.
        //
        // Head case `compiler/temporal`: four sites spelled
        // `monthsByDays[zdt.daysInMonth]` over
        // `Record<number, Temporal.ZonedDateTime[]>`, whose `any` cascades
        // into the `Array<T>` members read off each one — 339 wrong lines in a
        // case that is otherwise 6,258 RIGHT.
        //
        // Restricted to `string` and `number` on purpose. A literal-union key
        // (`Record<"a" | "b", V>`) must produce PROPERTIES, not an index
        // signature, and handing one back here would make `r.c` answer `V`
        // where upstream errors — a confident wrong answer in place of a
        // missing one.
        if let Some(info) = self.record_index_info(id) {
            return Some(vec![info]);
        }
        if let TypeData::Anonymous { symbol, .. } = self.store.get(id).data
            && self.binder.symbols().get(symbol).flags.contains(tsr_binder::SymbolFlags::CLASS)
        {
            return self.index_infos_of_symbol(symbol, true, &mut Vec::new());
        }
        if let Some(tuple) = self.tuple_index_infos(id) {
            return tuple;
        }
        let TypeData::Named { members: Some(owner), .. } = self.store.get(id).data else {
            return Some(Vec::new());
        };
        let mut visiting = Vec::new();
        let infos = self.index_infos_of_symbol(owner, false, &mut visiting)?;
        // On an instantiated reference the declared value types are the
        // target's uninstantiated ones — `Array<string>`'s `[n: number]: T`
        // must answer `string`, not `T`. Same seam rule as
        // `get_type_of_property_of_type` (`crate::members`), for the same
        // reason, and a value that cannot be rebuilt becomes `errorType` — the
        // access stays a gap rather than answering the type parameter.
        // `bd tsr-4qx`.
        Some(
            infos
                .into_iter()
                .map(|info| IndexInfo {
                    components: info.components,
                    declaration: info.declaration,
                    key: info.key,
                    value: self.instantiate_for_reference(id, info.value),
                    readonly: info.readonly,
                })
                .collect(),
        )
    }

    /// A tuple's index infos: the number index it inherits from its base.
    ///
    /// `getTupleBaseType` (`checker.go:19206`) makes a tuple target's base
    /// `Array<E>` (`ReadonlyArray<E>` when readonly), where `E` is the union of
    /// its element type arguments, a variadic element contributing
    /// `T[number]`; `resolveObjectTypeMembers` copies the base's
    /// `[n: number]: T` into the tuple's members, instantiated to `E`. An
    /// optional element's argument already carries `undefined`
    /// (`addOptionality` in `getTypeFromTupleTypeNode`), which this port keeps
    /// in the optional mask instead, so it is added back here as
    /// `variadic_tuple_index_union` does.
    ///
    /// `None` when `id` is not a tuple. `Some(None)` — a gap — for the
    /// print-only shapes whose elements are resolved lazily
    /// (`tuple_rest_tails`, `variadic_tuple_nodes`): answering "no index" for
    /// them would be the confident wrong answer this function used to give
    /// every tuple. No cache or side table: the element lists are the
    /// existing tuple tables, read per call. `docs/parity/notes/r4-index2.md` §3.
    fn tuple_index_infos(&mut self, id: TypeId) -> Option<Option<Vec<IndexInfo>>> {
        let readonly;
        let value = if let Some((elements, is_readonly)) = self.tuple_element_lists.get(&id) {
            if self.tuple_rest_tails.contains_key(&id)
                || self.variadic_tuple_nodes.contains_key(&id)
            {
                return Some(None);
            }
            readonly = *is_readonly;
            let mut types = elements.clone();
            if let Some(mask) = self.tuple_optional_masks.get(&id)
                && mask.iter().any(|&optional| optional)
            {
                types.push(self.intrinsics.undefined);
            }
            self.get_union_type(&types)
        } else if let Some((_, is_readonly)) = self.variadic_tuple_elements.get(&id) {
            readonly = *is_readonly;
            match self.variadic_tuple_index_union(id) {
                Some(value) => value,
                None => return Some(None),
            }
        } else if self.variadic_tuple_nodes.contains_key(&id)
            || self.tuple_rest_tails.contains_key(&id)
        {
            return Some(None);
        } else {
            return None;
        };
        Some(Some(vec![IndexInfo {
            components: None,
            declaration: None,
            key: self.intrinsics.number,
            value,
            readonly,
        }]))
    }

    /// Pinned 5b1047d checker.go:23837/24115/25121 publishes the canonical array
    /// reference before recursive arguments; 19095/19106 resolves its indexes.
    /// This port's completed body is owned by (alias `SymbolId`, ordered `TypeIds`),
    /// not the alias's empty member table. Read only the existing successful
    /// publication, preserving source identity and the array receiver mapper.
    /// No evaluator, completion cache or mapped/captured image is introduced.
    fn completed_original_array_alias_body(&self, id: TypeId) -> Option<TypeId> {
        if !self.alias_evaluation_bindings.is_empty()
            || self.mapped_template_depth != 0
            || self.instantiation_depth != 0
            || self.identity_unmapped_type_parameters
            || !self.render_type_parameter_scope.is_empty()
        {
            return None;
        }
        let key @ (owner, arguments) = self.type_reference_targets.get(&id)?;
        if self.instantiations.get(key) != Some(&id)
            || !matches!(self.store.get(id).data, TypeData::Named { members: Some(member), .. } if member == *owner)
            || self.resolutions.on_stack(*owner, crate::resolution::PropertyName::DeclaredType)
            || !self
                .binder
                .symbols()
                .get(*owner)
                .flags
                .contains(tsr_binder::SymbolFlags::TYPE_ALIAS)
        {
            return None;
        }
        let [declaration] = self.binder.symbols().get(*owner).declarations.as_slice() else {
            return None;
        };
        let Node::TypeAliasDeclaration(alias) = self.node_map.get(*declaration)? else {
            return None;
        };
        let root = self.nodes.parent(*declaration)?;
        if self.nodes.kind(root) != tsr_ast::SyntaxKind::SourceFile
            || self.nodes.flags(root).contains(tsr_ast::NodeFlags::JAVASCRIPT_FILE)
            || alias.type_parameters.len() != arguments.len()
            || arguments.iter().any(|&argument| self.is_error(argument))
        {
            return None;
        }
        let body = *self.alias_body_evaluations.get(key)?;
        if body == id || self.is_error(body) {
            return None;
        }
        let body_key @ (target, elements) = self.type_reference_targets.get(&body)?;
        let [element] = elements.as_slice() else { return None };
        if self.is_error(*element)
            || self.instantiations.get(body_key) != Some(&body)
            || !self
                .binder
                .symbols()
                .get(*target)
                .flags
                .contains(tsr_binder::SymbolFlags::INTERFACE)
            || !matches!(self.store.get(body).data, TypeData::Named { members: Some(member), .. } if member == *target)
        {
            return None;
        }
        let array = self.global_type_symbol_with_arity("Array", 1);
        let readonly = self.global_type_symbol_with_arity("ReadonlyArray", 1);
        let source_target = match alias.r#type? {
            tsr_ast::TypeNode::ArrayTypeNode(_) => array?,
            tsr_ast::TypeNode::TypeOperatorNode(operator)
                if operator.operator.kind == tsr_ast::SyntaxKind::ReadonlyKeyword
                    && matches!(operator.r#type, Some(tsr_ast::TypeNode::ArrayTypeNode(_))) =>
            {
                readonly?
            }
            tsr_ast::TypeNode::TypeReferenceNode(reference) => {
                let tsr_ast::EntityName::Identifier(name) = reference.type_name? else {
                    return None;
                };
                if reference.type_arguments.len() != 1 {
                    return None;
                }
                self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    name.node_id?,
                    name.text,
                    tsr_binder::SymbolFlags::TYPE,
                )?
            }
            _ => return None,
        };
        let target = self.binder.merged_symbol(*target);
        (self.binder.merged_symbol(source_target) == target
            && [array, readonly]
                .into_iter()
                .flatten()
                .any(|array| self.binder.merged_symbol(array) == target))
        .then_some(body)
    }

    /// `getUnionIndexInfos` (internal/checker/checker.go): only keys present
    /// in every constituent survive; their value types are unioned.
    pub(crate) fn union_index_infos(&mut self, types: &[TypeId]) -> Option<Vec<IndexInfo>> {
        let mut constituents = Vec::with_capacity(types.len());
        for &ty in types {
            // `getIndexInfosOfType` reads each constituent through
            // `getReducedApparentType`: a `string` constituent contributes
            // `String`'s `readonly [index: number]: string`
            // (`classDoesNotDependOnBaseTypes`). An object constituent is its
            // own apparent type, so only primitives are mapped here.
            let ty = if self.type_of(ty).flags.intersects(TypeFlags::PRIMITIVE) {
                self.apparent_type(ty)
            } else {
                ty
            };
            constituents.push(self.get_index_infos_of_type(ty)?);
        }
        let Some(first) = constituents.first() else { return Some(Vec::new()) };
        let mut result = Vec::new();
        for info in first {
            let values: Option<Vec<_>> = constituents
                .iter()
                .map(|infos| {
                    infos.iter().find(|candidate| candidate.key == info.key).map(|info| info.value)
                })
                .collect();
            if let Some(values) = values {
                let readonly = constituents.iter().any(|infos| {
                    infos.iter().any(|candidate| candidate.key == info.key && candidate.readonly)
                });
                result.push(IndexInfo {
                    components: None,
                    declaration: None,
                    key: info.key,
                    value: self.get_union_type(&values),
                    readonly,
                });
            }
        }
        Some(result)
    }

    /// A symbol's own index signatures, then its base types', in that order.
    ///
    /// Ported from the base-type loop in `resolveObjectTypeMembers`
    /// (`checker.go:19149`), whose rule is a **shadow by key type**, not a
    /// merge:
    ///
    /// ```go
    /// indexInfos = core.Concatenate(indexInfos, core.Filter(inheritedIndexInfos,
    ///     func(info *IndexInfo) bool { return findIndexInfo(indexInfos, info.keyType) == nil }))
    /// ```
    ///
    /// So a derived `[k: string]: A` hides a base's `[k: string]: B` entirely —
    /// the two are never combined — while a base's `[k: number]` survives beside
    /// it. Same shape as `addInheritedMembers` for properties, and the same walk
    /// [`Checker::get_property_of_declared_symbol`] already does.
    ///
    /// # The gaps, and why each is `None` rather than an empty list
    ///
    /// [`Checker::base_symbols_of`] answers `None` when it cannot follow a base
    /// — a base with type arguments, a qualified name, an expression. That
    /// propagates here for the reason it was written in `members.rs`: an
    /// unfollowable base may declare an index signature, and reporting "none"
    /// would let `a[i]` confidently answer nothing when the real answer exists.
    ///
    /// A **cycle** (`interface A extends B {}` with `interface B extends A {}`)
    /// is likewise `None`. Upstream reports
    /// `Type_0_recursively_references_itself_as_a_base_type` and carries on with
    /// empty bases; this port has no diagnostics (`bd tsr-5e7.6`), so the honest
    /// reduction is a gap. The guard is the **path**, exactly as in
    /// `get_property_of_declared_symbol`, and for the same reason: there is no
    /// `resolvedBaseTypes` memo here to park a sentinel in.
    ///
    /// # This cannot desynchronise the printer from the lookup
    ///
    /// The trap this walk looks like it should spring — the lookup finding an
    /// inherited signature the printer never rendered — **cannot arise**, and
    /// the reason is worth stating because it is not obvious. `render_object_type`
    /// is reached only from [`Checker::get_type_from_type_literal`] and
    /// `check_object_literal`. An interface prints by *name* and never renders
    /// its members, and a type literal has no heritage clause, so no type that
    /// prints structurally can have an inherited index signature at all. If a
    /// structural printer for interfaces is ever added, this note is the one to
    /// re-read.
    ///
    /// # Memoised like native's resolved members
    ///
    /// Native resolves a structured type's index infos once
    /// (`resolveObjectTypeMembers`, `checker.go:19106`, publishing
    /// `indexInfos`; the static side through `resolveAnonymousTypeMembers`);
    /// this port re-scanned every member of every declaration per query —
    /// `String`'s, for each `string` constituent of a union.
    /// [`PerfLinks::symbol_index_infos`](crate::perf_links::PerfLinks) keeps
    /// the decided lists. A published list is exact for any `visiting`: it
    /// met no cycle from an empty path, and every symbol on the path has
    /// `owner` as a base, so none can recur in `owner`'s walk. Key,
    /// publication and context: `docs/parity/notes/r4-perf3.md` §5.
    fn index_infos_of_symbol(
        &mut self,
        owner: SymbolId,
        static_side: bool,
        visiting: &mut Vec<SymbolId>,
    ) -> Option<Vec<IndexInfo>> {
        let binder = self.binder;
        let Some(frames) = self.memo_frames(&binder.symbols().get(owner).declarations, None) else {
            return self.index_infos_of_symbol_worker(owner, static_side, visiting);
        };
        let key = (owner, static_side);
        if let Some(cached) = self.perf_links.symbol_index_infos.get(&key) {
            let cached = cached.clone();
            self.alias_evaluation_bindings = frames;
            return Some(cached);
        }
        let mark = self.publication_mark();
        let computed = self.index_infos_of_symbol_worker(owner, static_side, visiting);
        self.alias_evaluation_bindings = frames;
        let infos = computed?;
        if self.publishable_since(mark) && infos.iter().all(|info| !self.is_error(info.value)) {
            self.perf_links.symbol_index_infos.insert(key, infos.clone());
        }
        Some(infos)
    }

    /// [`Self::index_infos_of_symbol`]'s computation, without the memo.
    fn index_infos_of_symbol_worker(
        &mut self,
        owner: SymbolId,
        static_side: bool,
        visiting: &mut Vec<SymbolId>,
    ) -> Option<Vec<IndexInfo>> {
        if visiting.contains(&owner) {
            return None;
        }
        visiting.push(owner);
        let declarations = self.binder.symbols().get(owner).declarations.clone();
        let mut infos = Vec::new();
        for declaration in declarations {
            // §252. A CLASS declares index signatures too, and this collector
            // read only the two `TypeElement` carriers. `getIndexInfosOfSymbol`
            // makes no such distinction — it walks the symbol's members, and a
            // class's `[x: string]: string` is one of them.
            //
            //     class C { foo!: string; [x: string]: string; }
            //     declare var c: C;
            //     var r2: string = c[''];
            //     >c[''] : string        <- this port answered `any`
            //
            // Witness `conformance/objectTypeWithStringIndexerHidingObjectIndexer`,
            // whose four sub-cases are a class, an interface, a type literal and
            // an `Object` augmentation — the interface and literal ones already
            // passed, which is exactly why the class gap was invisible: three of
            // four carriers worked.
            //
            // A class member is a `ClassElement`, not a `TypeElement`, so the
            // arm is separate rather than another line in the match. The
            // *element* is the only difference; the info is built by the same
            // `index_infos_of_declaration`, which the AST makes possible because both
            // carriers wrap the identical `IndexSignatureDeclaration` node.
            let mut push_from = |checker: &mut Self, signature| {
                for info in checker.index_infos_of_declaration(signature) {
                    if !infos.iter().any(|own: &IndexInfo| own.key == info.key) {
                        infos.push(info);
                    }
                }
            };
            match self.node_map.get(declaration) {
                Some(Node::InterfaceDeclaration(node)) if !static_side => {
                    for member in node.members {
                        if let TypeElement::IndexSignatureDeclaration(signature) = member {
                            push_from(self, signature);
                        }
                    }
                }
                Some(Node::TypeLiteralNode(node)) if !static_side => {
                    for member in node.members {
                        if let TypeElement::IndexSignatureDeclaration(signature) = member {
                            push_from(self, signature);
                        }
                    }
                }
                Some(Node::ClassDeclaration(node)) => {
                    for member in node.members {
                        if let tsr_ast::ClassElement::IndexSignatureDeclaration(signature) = member
                            && tsr_ast::has_syntactic_modifier(
                                signature.modifiers,
                                tsr_ast::SyntaxKind::StaticKeyword,
                            ) == static_side
                        {
                            push_from(self, signature);
                        }
                    }
                }
                Some(Node::ClassExpression(node)) => {
                    for member in node.members {
                        if let tsr_ast::ClassElement::IndexSignatureDeclaration(signature) = member
                            && tsr_ast::has_syntactic_modifier(
                                signature.modifiers,
                                tsr_ast::SyntaxKind::StaticKeyword,
                            ) == static_side
                        {
                            push_from(self, signature);
                        }
                    }
                }
                // Any other declaration kind carries no index signature.
                // (`_ => continue` here is the same thing and clippy calls it
                // redundant, since the loop body ends immediately after.)
                _ => {}
            }
        }
        // The late-bound declarations of `__index` follow the early ones
        // (`lateBindIndexSignature` appends to a clone of the early symbol),
        // so explicit signatures claim their keys first.
        if !self.late_bound_index_infos(owner, static_side, &mut infos) {
            visiting.pop();
            return None;
        }
        // resolveAnonymousTypeMembers inherits named properties from the base
        // constructor, not its __index export. Instance indexes instead
        // inherit independently by key in resolveObjectTypeMembers.
        if static_side {
            visiting.pop();
            return Some(infos);
        }
        // Resolve inherited index values under each heritage mapper before
        // applying the receiver's own arguments in get_index_infos_of_type.
        // Unlike value types, instantiateIndexInfo leaves key types unchanged.
        let declarations = self.binder.symbols().get(owner).declarations.to_vec();
        for declaration in declarations {
            let clauses = match self.node_map.get(declaration) {
                Some(Node::ClassDeclaration(node)) => node.heritage_clauses,
                Some(Node::ClassExpression(node)) => node.heritage_clauses,
                Some(Node::InterfaceDeclaration(node)) => node.heritage_clauses,
                _ => continue,
            };
            for clause in clauses {
                if clause.token.kind != tsr_ast::SyntaxKind::ExtendsKeyword {
                    continue;
                }
                for entry in clause.types {
                    let base = self.base_symbol_of_heritage_entry(entry, false)?;
                    let base_type =
                        self.instantiated_heritage_base(base, entry.type_arguments, entry.node_id)?;
                    for inherited in self.index_infos_of_symbol(base, false, visiting)? {
                        // Own indexes and earlier bases win for the same key.
                        if !infos.iter().any(|own| own.key == inherited.key) {
                            infos.push(IndexInfo {
                                components: inherited.components,
                                declaration: inherited.declaration,
                                key: inherited.key,
                                value: self.instantiate_for_reference(base_type, inherited.value),
                                readonly: inherited.readonly,
                            });
                        }
                    }
                }
            }
        }
        visiting.pop();
        Some(infos)
    }

    /// `getIndexInfosOfIndexSymbol`'s `hasLateBindableIndexSignature` arm
    /// (`checker.go:19662`) and its aggregation (`:19700-19716`).
    ///
    /// A member whose computed name is an entity-name expression
    /// (`isLateBindableAST`) of a type that is not usable as a property name
    /// but is assignable to `string | number | symbol` is late-bound into the
    /// `__index` symbol (`getResolvedMembersOrExportsOfSymbol` ->
    /// `lateBindIndexSignature`, `checker.go:15957`/`:16068`) instead of the
    /// members table: `class C { [k]: number }` with `k: string` declares a
    /// string index signature. Each such key kind gets ONE info whose value is
    /// `getObjectLiteralIndexInfo` over the late-bound declarations followed
    /// by every sibling member of the table (`:19720`), so the value is the
    /// union of all applicable member types, not the annotated one.
    ///
    /// Port convention record (`docs/conventions.md`): no cache, side table or
    /// mapper. The pinned operation is `getIndexInfosOfIndexSymbol` on the
    /// owner's members (instance) or exports (static); the receiver context is
    /// the uninstantiated declaration, exactly like the explicit-signature arm
    /// above, and `get_index_infos_of_type` instantiates values afterwards.
    /// Expensive work (the computed-name `check_expression`, sibling
    /// `get_type_of_symbol`) runs only for a declaration that has a member with
    /// an entity-name computed name; every other declaration costs one
    /// syntactic member scan. `false` declines the whole symbol (a gap), for
    /// the one sibling set this port cannot enumerate: a static side whose
    /// `prototype` or inherited statics would need the base constructor.
    /// `docs/parity/notes/r4-index.md` §1.
    fn late_bound_index_infos(
        &mut self,
        owner: SymbolId,
        static_side: bool,
        infos: &mut Vec<IndexInfo>,
    ) -> bool {
        let declarations = self.binder.symbols().get(owner).declarations.clone();
        let mut candidates: Vec<(tsr_ast::NodeId, tsr_ast::Expression<'a>)> = Vec::new();
        for &declaration in &declarations {
            let members: Vec<tsr_ast::NodeId> = match self.node_map.get(declaration) {
                Some(Node::ClassDeclaration(node)) => {
                    node.members.iter().filter_map(|m| Node::from(*m).node_id()).collect()
                }
                Some(Node::ClassExpression(node)) => {
                    node.members.iter().filter_map(|m| Node::from(*m).node_id()).collect()
                }
                Some(Node::InterfaceDeclaration(node)) => {
                    node.members.iter().filter_map(|m| Node::from(*m).node_id()).collect()
                }
                Some(Node::TypeLiteralNode(node)) => {
                    node.members.iter().filter_map(|m| Node::from(*m).node_id()).collect()
                }
                _ => continue,
            };
            for member in members {
                let Some(name) = self.declaration_name_of(member) else { continue };
                let Some(Node::ComputedPropertyName(computed)) = self.node_map.get(name) else {
                    continue;
                };
                let Some(expression) = computed.expression else { continue };
                // `isStatic == ast.HasStaticModifier(member)`.
                let is_static = self
                    .node_map
                    .get(member)
                    .and_then(crate::check::modifiers_of)
                    .is_some_and(|modifiers| {
                        crate::check::has_modifier(modifiers, tsr_ast::SyntaxKind::StaticKeyword)
                    });
                if is_static != static_side
                    || !expression.node_id().is_some_and(|id| self.is_entity_name_expression(id))
                {
                    continue;
                }
                candidates.push((member, expression));
            }
        }
        if candidates.is_empty() {
            return true;
        }
        let allowed = self.get_union_type(&[
            self.intrinsics.string,
            self.intrinsics.number,
            self.intrinsics.es_symbol,
        ]);
        let mut has = [false; 3];
        let mut readonly = [true; 3];
        let mut late_declarations = Vec::new();
        for (member, expression) in candidates {
            let key = self.check_expression(expression);
            // `hasLateBindableName` is asked first: a literal or unique symbol
            // name is a late-bound MEMBER, not an index signature.
            if self.type_of(key).flags.intersects(
                TypeFlags::STRING_LITERAL | TypeFlags::NUMBER_LITERAL | TypeFlags::UNIQUE_ES_SYMBOL,
            ) || !self.is_type_assignable_to(key, allowed)
            {
                continue;
            }
            // Explicit index for key type takes priority.
            if infos.iter().any(|info| info.key == key) {
                continue;
            }
            let kind = if self.is_type_assignable_to(key, self.intrinsics.number) {
                1
            } else if self.is_type_assignable_to(key, self.intrinsics.es_symbol) {
                2
            } else {
                0
            };
            // A plain `symbol` key may be a `unique symbol` this port never
            // minted: `widenTypeForVariableLikeDeclaration` (`checker.go:18246`,
            // typescript-go#1212) turns a `symbol` member merged into the
            // global `SymbolConstructor` into that member's unique symbol, and
            // this port's widening lacks the arm. Such a key late-binds a
            // MEMBER upstream, never an index (`symbolProperty61`), so it is
            // declined until the producer is ported (r4-index report).
            if kind == 2
                && self.type_of(key).flags.contains(TypeFlags::ES_SYMBOL)
                && self.symbol_constructor_declares_symbol_member()
            {
                continue;
            }
            has[kind] = true;
            let is_readonly =
                self.node_map.get(member).and_then(crate::check::modifiers_of).is_some_and(
                    |modifiers| {
                        crate::check::has_modifier(modifiers, tsr_ast::SyntaxKind::ReadonlyKeyword)
                    },
                );
            if !is_readonly {
                readonly[kind] = false;
            }
            late_declarations.push(member);
        }
        if !has.iter().any(|&kind| kind) {
            return true;
        }
        let Some(properties) = self.late_index_properties(owner, static_side, &late_declarations)
        else {
            return false;
        };
        let keys = [self.intrinsics.string, self.intrinsics.number, self.intrinsics.es_symbol];
        for kind in 0..3 {
            if !has[kind] || infos.iter().any(|info| info.key == keys[kind]) {
                continue;
            }
            // getObjectLiteralIndexInfo (checker.go:19720).
            let mut values = Vec::new();
            let mut components = Vec::new();
            for property in &properties {
                let applies = match kind {
                    0 => !property.symbol_named,
                    1 => property.numeric_named,
                    _ => property.symbol_named,
                };
                if applies {
                    values.push(property.value);
                    components.extend(property.component);
                }
            }
            let value = if values.is_empty() {
                self.intrinsics.undefined
            } else if let Some(&error) = values.iter().find(|&&value| self.is_error(value)) {
                // getUnionType answers errorType when a constituent is one
                // (`IncludesError`). A signature carrier's errorType is the
                // NATIVE intrinsic, which prints `any` and is `IsTypeAny`;
                // this port's `error` means "not computed" and prints as a
                // gap, so the carrier's contribution is spelled `any`. A
                // member whose own type this port could not compute stays
                // `error`.
                error
            } else {
                match self.union_with_subtype_reduction(&values) {
                    Some(value) => value,
                    None => return false,
                }
            };
            let components = if components.is_empty() {
                None
            } else {
                let id = IndexComponentsId(self.index_components.len());
                self.index_components.push(components);
                Some(id)
            };
            infos.push(IndexInfo {
                components,
                declaration: None,
                key: keys[kind],
                value,
                readonly: readonly[kind],
            });
        }
        true
    }

    /// Does any declaration of the global `SymbolConstructor` declare a
    /// property signature written `symbol` — the members whose type
    /// `widenTypeForVariableLikeDeclaration` makes unique upstream?
    fn symbol_constructor_declares_symbol_member(&self) -> bool {
        let Some(global) = self.global_type_symbol_with_arity("SymbolConstructor", 0) else {
            return false;
        };
        let global = self.binder.merged_symbol(global);
        self.binder.symbols().get(global).declarations.iter().any(|&declaration| {
            let Some(Node::InterfaceDeclaration(interface)) = self.node_map.get(declaration) else {
                return false;
            };
            interface.members.iter().any(|member| {
                matches!(member, TypeElement::PropertySignatureDeclaration(property)
                    if matches!(property.r#type, Some(tsr_ast::TypeNode::KeywordTypeNode(keyword))
                        if keyword.kind == tsr_ast::SyntaxKind::SymbolKeyword))
            })
        })
    }

    /// `propertySymbols` of `getIndexInfosOfIndexSymbol`: the late-bound index
    /// declarations' own symbols, then every sibling in
    /// `getMembersOfSymbol(symbol)` (instance) or the static side's members,
    /// each with the three name predicates `getObjectLiteralIndexInfo` asks
    /// (`isSymbolWithSymbolName`, `isSymbolWithNumericName`,
    /// `isSymbolWithComputedName`) and `getTypeOfSymbol`.
    ///
    /// The members table upstream also holds the signature-carrying symbols
    /// this binder does not declare there — `__constructor` on a class's
    /// instance side, `__call`/`__new` on an interface or type literal. Their
    /// `getTypeOfSymbol` falls through every flags arm to `errorType`
    /// (`checker.go:16493`), and a name-less symbol is neither symbol- nor
    /// numeric-named, so each contributes `errorType` to the string index.
    fn late_index_properties(
        &mut self,
        owner: SymbolId,
        static_side: bool,
        late_declarations: &[tsr_ast::NodeId],
    ) -> Option<Vec<LateIndexProperty>> {
        let mut properties = Vec::new();
        for &declaration in late_declarations {
            let symbol = self.binder.symbol_of(declaration)?;
            let property = self.late_index_property(symbol, None, declaration);
            properties.push(property);
        }
        let entry = self.binder.symbols().get(owner);
        let is_class = entry.flags.intersects(tsr_binder::SymbolFlags::CLASS);
        let table = if static_side { &entry.exports } else { &entry.members };
        let siblings: Vec<SymbolId> = table
            .iter()
            .filter(|(name, _)| **name != "__index")
            .map(|(_, &symbol)| symbol)
            .collect();
        for symbol in siblings {
            let Some(&declaration) = self.binder.symbols().get(symbol).declarations.first() else {
                continue;
            };
            let name = self.binder.symbols().get(symbol).name;
            properties.push(self.late_index_property(symbol, Some(name), declaration));
        }
        for (_, member) in self.late_bound_members_of(owner, static_side) {
            let Some(symbol) = self.binder.symbol_of(member) else { continue };
            properties.push(self.late_index_property(symbol, None, member));
        }
        let declarations = self.binder.symbols().get(owner).declarations.clone();
        if static_side {
            // `prototype` (getTypeOfPrototypeProperty) and the base
            // constructor's statics (addInheritedMembers) are siblings too.
            // Only a non-generic class without `extends` is enumerable here.
            for declaration in declarations {
                let (parameters, heritage) = match self.node_map.get(declaration) {
                    Some(Node::ClassDeclaration(node)) => {
                        (node.type_parameters, node.heritage_clauses)
                    }
                    Some(Node::ClassExpression(node)) => {
                        (node.type_parameters, node.heritage_clauses)
                    }
                    _ => continue,
                };
                if !parameters.is_empty()
                    || heritage
                        .iter()
                        .any(|clause| clause.token.kind == tsr_ast::SyntaxKind::ExtendsKeyword)
                {
                    return None;
                }
            }
            if is_class {
                properties.push(LateIndexProperty {
                    value: self.get_declared_type_of_class_or_interface(owner),
                    symbol_named: false,
                    numeric_named: false,
                    component: None,
                });
            }
            return Some(properties);
        }
        let signature_carrier =
            declarations.iter().any(|&declaration| match self.node_map.get(declaration) {
                Some(Node::ClassDeclaration(node)) => node.members.iter().any(|member| {
                    matches!(member, tsr_ast::ClassElement::ConstructorDeclaration(_))
                }),
                Some(Node::ClassExpression(node)) => node.members.iter().any(|member| {
                    matches!(member, tsr_ast::ClassElement::ConstructorDeclaration(_))
                }),
                Some(Node::InterfaceDeclaration(node)) => node.members.iter().any(|member| {
                    matches!(
                        member,
                        TypeElement::CallSignatureDeclaration(_)
                            | TypeElement::ConstructSignatureDeclaration(_)
                    )
                }),
                Some(Node::TypeLiteralNode(node)) => node.members.iter().any(|member| {
                    matches!(
                        member,
                        TypeElement::CallSignatureDeclaration(_)
                            | TypeElement::ConstructSignatureDeclaration(_)
                    )
                }),
                _ => false,
            });
        if signature_carrier {
            properties.push(LateIndexProperty {
                value: self.intrinsics.any,
                symbol_named: false,
                numeric_named: false,
                component: None,
            });
        }
        Some(properties)
    }

    /// One `propertySymbols` entry: `getTypeOfSymbol` and the name predicates
    /// read off `symbol.Declarations[0].Name()` (`checker.go:19740-19786`).
    /// `name` is the table name for an early-bound symbol; a late-bound one is
    /// named by its computed expression's type alone.
    fn late_index_property(
        &mut self,
        symbol: SymbolId,
        name: Option<&str>,
        declaration: tsr_ast::NodeId,
    ) -> LateIndexProperty {
        let value = self.get_type_of_symbol(symbol);
        let name_node = self.declaration_name_of(declaration);
        let computed = name_node.and_then(|node| match self.node_map.get(node) {
            Some(Node::ComputedPropertyName(computed)) => Some(computed.expression),
            _ => None,
        });
        let (symbol_named, numeric_named) = if let Some(expression) = computed {
            let key = match expression {
                Some(expression) => self.check_expression(expression),
                None => self.intrinsics.error,
            };
            let key_flags = self.type_of(key).flags;
            let symbol_named = key_flags.intersects(TypeFlags::ES_SYMBOL_LIKE)
                || self.is_type_assignable_to(key, self.intrinsics.es_symbol);
            let numeric_named = name.is_some_and(is_numeric_literal_name)
                || key_flags.intersects(TypeFlags::NUMBER_LIKE)
                || self.is_type_assignable_to(key, self.intrinsics.number);
            (symbol_named, numeric_named)
        } else {
            let text = name_node.and_then(|node| match self.node_map.get(node) {
                Some(Node::Identifier(identifier)) => Some(identifier.text),
                Some(Node::NumericLiteral(literal)) => Some(literal.text),
                Some(Node::StringLiteral(literal)) => Some(literal.text),
                _ => None,
            });
            let numeric_named = name.is_some_and(is_numeric_literal_name)
                || text.is_some_and(is_numeric_literal_name);
            (false, numeric_named)
        };
        LateIndexProperty {
            value,
            symbol_named,
            numeric_named,
            component: computed.map(|_| declaration),
        }
    }

    /// getIndexInfosOfIndexSymbol splits union keys and retains each valid
    /// primitive, pattern or nongeneric intersection key (checker.go).
    pub(crate) fn index_infos_of_declaration(
        &mut self,
        signature: &tsr_ast::IndexSignatureDeclaration<'a>,
    ) -> Vec<IndexInfo> {
        let [parameter] = signature.parameters else { return Vec::new() };
        let Some(key) = parameter.r#type else { return Vec::new() };
        let key = self.get_type_from_type_node(key);
        // `valueType := c.anyType` unless a type is written (`checker.go:19649`):
        // `[k: string];` is a string index of `any`, not no index.
        let value = match signature.r#type {
            Some(value) => self.get_type_from_type_node(value),
            None => self.intrinsics.any,
        };
        if value == self.intrinsics.error {
            return Vec::new();
        }
        let keys = match &self.store.get(key).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => vec![key],
        };
        keys.into_iter()
            .filter(|&key| self.is_valid_index_key_type(key))
            .map(|key| IndexInfo {
                components: None,
                declaration: signature.node_id,
                key,
                value,
                readonly: signature.modifiers.iter().any(|modifier| {
                    matches!(modifier, tsr_ast::ModifierLike::Token(token)
                        if token.kind == tsr_ast::SyntaxKind::ReadonlyKeyword)
                }),
            })
            .collect()
    }

    /// isValidIndexKeyType (checker.go:19787), using existing pattern metadata.
    pub(crate) fn is_valid_index_key_type(&mut self, key: TypeId) -> bool {
        if self.store.get(key).flags.intersects(
            crate::flags::TypeFlags::STRING
                | crate::flags::TypeFlags::NUMBER
                | crate::flags::TypeFlags::ES_SYMBOL,
        ) || self.is_pattern_template(key)
        {
            return true;
        }
        if let TypeData::Intersection { types, .. } = &self.store.get(key).data {
            let types = types.clone();
            return !self.signature_parameter_type_is_generic(key)
                && types.iter().any(|&key| self.is_valid_index_key_type(key));
        }
        false
    }

    /// indexInfoToIndexSignatureDeclarationHelper (nodebuilderimpl.go:2088):
    /// serializable computed components print individually as property signatures.
    pub(crate) fn index_info_members(
        &mut self,
        info: &IndexInfo,
    ) -> Option<Vec<crate::objects::Member>> {
        if let Some(components) = info.components {
            let declarations = self.index_components[components.0].clone();
            let mut members = Vec::new();
            for declaration in declarations {
                let name = match self.node_map.get(declaration)? {
                    Node::PropertyAssignment(node) => node.name,
                    Node::MethodDeclaration(node) => node.name,
                    Node::GetAccessorDeclaration(node) => node.name,
                    Node::SetAccessorDeclaration(node) => node.name,
                    _ => return None,
                };
                let tsr_ast::PropertyName::ComputedPropertyName(computed) = name else {
                    return None;
                };
                let expression = computed.expression?;
                let Some(name) = crate::objects::entity_name_expression_text(&expression) else {
                    return Some(vec![self.index_info_member(info)]);
                };
                if !self.computed_entity_name_is_visible(expression) {
                    return Some(vec![self.index_info_member(info)]);
                }
                let key = self.check_expression(expression);
                if self.store.get(key).flags.intersects(
                    TypeFlags::STRING_LITERAL
                        | TypeFlags::NUMBER_LITERAL
                        | TypeFlags::UNIQUE_ES_SYMBOL,
                ) {
                    continue;
                }
                let name = format!("[{name}]");
                let symbol = self.binder.symbol_of(declaration)?;
                let value = self.get_type_of_symbol(symbol);
                if value == self.intrinsics.error {
                    return None;
                }
                members.push(crate::objects::Member::Property {
                    name,
                    readonly: info.readonly,
                    optional: false,
                    printed: self.type_to_string(value),
                });
            }
            return Some(members);
        }
        Some(vec![self.index_info_member(info)])
    }

    /// isEntityNameVisible / hasVisibleDeclarations (emitresolver.go:340, 389).
    /// Only the first identifier is resolved; subsequent property names do not
    /// create a separate visibility requirement. Printing does not mark aliases.
    fn computed_entity_name_is_visible(&self, mut expression: tsr_ast::Expression<'_>) -> bool {
        use tsr_ast::Expression;
        loop {
            match expression {
                Expression::PropertyAccessExpression(access) => {
                    let Some(left) = access.expression else { return false };
                    expression = left;
                }
                Expression::Identifier(identifier) => {
                    let Some(location) = identifier.node_id else { return false };
                    let Some(symbol) = self.binder.resolve_name(
                        self.nodes,
                        self.node_map,
                        location,
                        identifier.text,
                        tsr_binder::SymbolFlags::VALUE | tsr_binder::SymbolFlags::NAMESPACE,
                    ) else {
                        return false;
                    };
                    return self.binder.symbols().get(symbol).declarations.iter().all(
                        |&declaration| {
                            self.emit_declaration_is_visible(declaration)
                                || self.emit_declaration_can_be_named(declaration, symbol)
                        },
                    );
                }
                _ => return false,
            }
        }
    }

    fn emit_has_modifier(&self, node: tsr_ast::NodeId, kind: tsr_ast::SyntaxKind) -> bool {
        self.node_map
            .get(node)
            .and_then(crate::check::modifiers_of)
            .is_some_and(|modifiers| crate::check::has_modifier(modifiers, kind))
    }

    /// determineIfDeclarationIsVisible (emitresolver.go:131). A parameter's
    /// visibility follows its declaration; a function-local variable does not.
    fn emit_declaration_is_visible(&self, node: tsr_ast::NodeId) -> bool {
        use tsr_ast::SyntaxKind as K;
        let parent = self.nodes.parent(node);
        let parent_visible =
            || parent.is_some_and(|parent| self.emit_declaration_is_visible(parent));
        match self.nodes.kind(node) {
            K::SourceFile | K::NamespaceExportDeclaration | K::TypeParameter => true,
            K::BindingElement => parent.and_then(|parent| self.nodes.parent(parent))
                .is_some_and(|root| self.emit_declaration_is_visible(root)),
            K::VariableDeclaration | K::ModuleDeclaration | K::ClassDeclaration
            | K::InterfaceDeclaration | K::TypeAliasDeclaration | K::FunctionDeclaration
            | K::EnumDeclaration | K::ImportEqualsDeclaration => {
                let mut statement = node;
                if self.nodes.kind(node) == K::VariableDeclaration {
                    let Some(list) = parent else { return false };
                    let Some(container) = self.nodes.parent(list) else { return false };
                    statement = container;
                }
                let Some(container) = self.nodes.parent(statement) else { return false };
                let exported = self.emit_has_modifier(statement, K::ExportKeyword);
                let ambient = self.nodes.kind(container) != K::SourceFile
                    && self.nodes.kind(node) != K::ImportEqualsDeclaration
                    && std::iter::once(container).chain(self.nodes.ancestors(container)).any(|ancestor| {
                        self.is_ambient_module_declaration(ancestor)
                            || self.emit_has_modifier(ancestor, K::DeclareKeyword)
                    });
                if exported || ambient {
                    self.emit_declaration_is_visible(container)
                } else {
                    matches!(self.node_map.get(container), Some(Node::SourceFile(file))
                        if !tsr_binder::is_external_module(file))
                }
            }
            K::PropertyDeclaration | K::PropertySignature | K::GetAccessor | K::SetAccessor
            | K::MethodDeclaration | K::MethodSignature => {
                !self.emit_has_modifier(node, K::PrivateKeyword)
                    && !self.emit_has_modifier(node, K::ProtectedKeyword) && parent_visible()
            }
            K::Constructor | K::ConstructSignature | K::CallSignature | K::IndexSignature
            | K::Parameter | K::ModuleBlock | K::FunctionType | K::ConstructorType
            | K::TypeLiteral | K::TypeReference | K::ArrayType | K::TupleType
            | K::UnionType | K::IntersectionType | K::ParenthesizedType | K::NamedTupleMember => parent_visible(),
            K::ExportSpecifier => parent.and_then(|parent| self.nodes.parent(parent))
                .is_some_and(|export| matches!(self.node_map.get(export),
                    Some(Node::ExportDeclaration(declaration)) if declaration.module_specifier.is_none())),
            _ => false,
        }
    }

    /// hasVisibleDeclarations' aliases-to-make-visible paths, without mutation.
    fn emit_declaration_can_be_named(&self, node: tsr_ast::NodeId, symbol: SymbolId) -> bool {
        use tsr_ast::SyntaxKind as K;
        let mut declaration = node;
        if self.nodes.kind(declaration) == K::BindingElement {
            if !self.binder.symbols().get(symbol).flags.intersects(
                tsr_binder::SymbolFlags::BLOCK_SCOPED_VARIABLE | tsr_binder::SymbolFlags::ALIAS,
            ) {
                return false;
            }
            while matches!(
                self.nodes.kind(declaration),
                K::BindingElement | K::ObjectBindingPattern | K::ArrayBindingPattern
            ) {
                let Some(parent) = self.nodes.parent(declaration) else { return false };
                declaration = parent;
            }
            if self.nodes.kind(declaration) == K::Parameter {
                return false;
            }
        }
        // getAnyImportSyntax walks from named/default/namespace import bindings.
        while matches!(
            self.nodes.kind(declaration),
            K::ImportSpecifier | K::NamedImports | K::ImportClause | K::NamespaceImport
        ) {
            let Some(parent) = self.nodes.parent(declaration) else { return false };
            declaration = parent;
        }
        if self.nodes.kind(declaration) == K::VariableDeclaration {
            let Some(statement) =
                self.nodes.parent(declaration).and_then(|list| self.nodes.parent(list))
            else {
                return false;
            };
            if self.nodes.kind(statement) != K::VariableStatement {
                return false;
            }
            declaration = statement;
        }
        if !matches!(
            self.nodes.kind(declaration),
            K::ImportDeclaration
                | K::ImportEqualsDeclaration
                | K::VariableStatement
                | K::ClassDeclaration
                | K::FunctionDeclaration
                | K::ModuleDeclaration
                | K::TypeAliasDeclaration
                | K::InterfaceDeclaration
                | K::EnumDeclaration
        ) {
            return false;
        }
        !self.emit_has_modifier(declaration, K::ExportKeyword)
            && self
                .nodes
                .parent(declaration)
                .is_some_and(|parent| self.emit_declaration_is_visible(parent))
    }

    /// indexInfoToIndexSignatureDeclarationHelper: copies retain the original
    /// parameter name; synthesized index infos use the native fallback `x`.
    pub(crate) fn index_info_member(&self, info: &IndexInfo) -> crate::objects::Member {
        let name = info
            .declaration
            .and_then(|id| match self.node_map.get(id) {
                Some(Node::IndexSignatureDeclaration(declaration)) => {
                    match declaration.parameters.first()?.name? {
                        tsr_ast::BindingName::Identifier(name) => Some(name.text),
                        tsr_ast::BindingName::BindingPattern(_) => None,
                    }
                }
                _ => None,
            })
            .unwrap_or("x");
        crate::objects::Member::Index {
            name: name.to_owned(),
            readonly: info.readonly,
            key: self.type_to_string(info.key),
            value: self.type_to_string(info.value),
        }
    }

    /// One `[k: K]: V` member rendered for printing, or `None` when it is a gap.
    ///
    /// Ported from `indexInfoToIndexSignatureDeclarationHelper`
    /// (`nodebuilderimpl.go:2138`), which takes the bracketed parameter's name
    /// from the *declaration* — upstream's `IndexInfo` does not carry one — and
    /// that is why the printed name is whatever was written: baselines record
    /// both `[key: string]: string` and `[x: string]: unknown`.
    ///
    /// The gaps are deliberately the **same** ones [`Checker::index_info_of`]
    /// takes, so a literal cannot print an index signature that a subsequent
    /// `a[i]` lookup then fails to find:
    ///
    /// - a key that is not the `string` or `number` intrinsic. `[k: string | number]`
    ///   is upstream *two* index infos rather than one printed with a union key
    ///   (`getIndexInfosOfIndexSymbol` splits it), so printing it whole would be
    ///   a confident wrong answer;
    /// - a value type this port cannot compute;
    /// - a binding pattern where the parameter name should be, which the grammar
    ///   forbids but the AST permits.
    pub(crate) fn index_signature_member(
        &mut self,
        signature: &tsr_ast::IndexSignatureDeclaration<'a>,
    ) -> Option<crate::objects::Member> {
        let [parameter] = signature.parameters else { return None };
        let Some(tsr_ast::BindingName::Identifier(name)) = parameter.name else { return None };
        let key = self.get_type_from_type_node(parameter.r#type?);
        // §32 (`checker-notes-callres.md`): any computable NON-UNION key
        // prints as written; a union key is upstream TWO infos
        // (`getIndexInfosOfIndexSymbol` splits it), so it still declines.
        // The LOOKUP gate (`index_infos_of_declaration`) deliberately stays narrower —
        // a print the lookup cannot serve gaps the access, never wrongs it.
        if key == self.intrinsics.error
            || self.store.get(key).flags.intersects(crate::flags::TypeFlags::UNION)
        {
            return None;
        }
        // `getIndexInfosOfIndexSymbol` keeps only a valid key
        // (`isValidIndexKeyType`, `checker.go:19655`); an invalid one
        // contributes no member upstream, which the caller cannot yet say
        // (`docs/parity/notes/r4-index.md` §4), so it still declines.
        if !self.is_valid_index_key_type(key) {
            return None;
        }
        let readonly = signature.modifiers.iter().any(|modifier| {
            matches!(modifier, tsr_ast::ModifierLike::Token(token)
                if token.kind == tsr_ast::SyntaxKind::ReadonlyKeyword)
        });
        // `valueType := c.anyType` without a written type (`checker.go:19649`):
        // `{ [x: string]; }` prints `{ [x: string]: any; }`.
        let Some(value_node) = signature.r#type else {
            return Some(crate::objects::Member::Index {
                readonly,
                name: name.text.to_string(),
                key: self.type_to_string(key),
                value: "any".to_string(),
            });
        };
        let value = self.get_type_from_type_node(value_node);
        // §949, leg 1: §929's rule at the INDEX member. A value type this port
        // cannot resolve used to decline the member, and the caller turns a
        // declined index member into a WHOLE-LITERAL `error` — so
        // `{ a: string; [k: number]: Bad }` printed nothing at all, losing the
        // perfectly good `a`. Upstream's member carries `errorType` and the node
        // builder still reuses the written annotation node.
        let written_value = if value == self.intrinsics.error {
            let mut single_quoted = false;
            let mut array_headed = false;
            Some(Self::written_type_text(value_node, &mut single_quoted, &mut array_headed)?)
        } else {
            None
        };
        Some(crate::objects::Member::Index {
            readonly,
            name: name.text.to_string(),
            key: self.type_to_string(key),
            value: match written_value {
                Some(text) => text,
                None => self.type_to_string(value),
            },
        })
    }

    /// The `Record<K, V>` index signature, for `K` exactly `string` or
    /// `number`. §785 — see the call site in
    /// [`Checker::get_index_infos_of_type`] for why this alias alone is
    /// special-cased and why a literal-union key is excluded.
    fn record_index_info(&mut self, receiver: TypeId) -> Option<IndexInfo> {
        let (target, arguments) = self.type_reference_targets.get(&receiver)?.clone();
        if arguments.len() != 2 {
            return None;
        }
        let key = arguments[0];
        if key != self.intrinsics.string && key != self.intrinsics.number {
            return None;
        }
        let record = self.binder.global("Record")?;
        (self.binder.merged_symbol(target) == self.binder.merged_symbol(record)).then_some(
            IndexInfo {
                components: None,
                declaration: None,
                key,
                value: arguments[1],
                readonly: false,
            },
        )
    }

    /// findApplicableIndexInfo: string is the fallback when no other key
    /// applies; overlapping applicable signatures intersect their values.
    pub(crate) fn get_applicable_index_info(
        &mut self,
        id: TypeId,
        key: TypeId,
    ) -> Option<IndexInfo> {
        // getPropertyTypeForIndexType excludes nullable keys before invoking
        // this lookup, even when non-strict assignability admits them.
        if self.store.get(key).flags.intersects(crate::flags::TypeFlags::NULLABLE) {
            return None;
        }
        let infos = self.get_index_infos_of_type(id)?;
        let string = self.intrinsics.string;
        let string_info = infos.iter().find(|info| info.key == string).copied();
        let applicable: Vec<IndexInfo> = infos
            .iter()
            .filter(|info| info.key != string)
            .filter(|info| self.is_applicable_index_type(key, info.key))
            .copied()
            .collect();
        match applicable.as_slice() {
            [] => {
                string_info.filter(|_| self.is_applicable_index_type(key, self.intrinsics.string))
            }
            [info] => Some(*info),
            _ => Some(IndexInfo {
                components: None,
                declaration: None,
                key: self.intrinsics.unknown,
                value: self.get_intersection_type(
                    &applicable.iter().map(|info| info.value).collect::<Vec<_>>(),
                    None,
                ),
                readonly: applicable.iter().all(|info| info.readonly),
            }),
        }
    }

    /// isObjectTypeWithInferableIndex (relater.go:4624), for represented sources.
    pub(crate) fn is_object_type_with_inferable_index(&mut self, id: TypeId) -> bool {
        if let TypeData::Intersection { types, .. } = &self.store.get(id).data {
            let types = types.clone();
            return types.into_iter().all(|ty| self.is_object_type_with_inferable_index(ty));
        }
        if self.js_literal_types.contains(&id) {
            return true;
        }
        let (TypeData::Named { members: Some(symbol), .. } | TypeData::Anonymous { symbol, .. }) =
            self.store.get(id).data
        else {
            return false;
        };
        let flags = self.binder.symbols().get(symbol).flags;
        if !flags.intersects(
            tsr_binder::SymbolFlags::OBJECT_LITERAL
                | tsr_binder::SymbolFlags::TYPE_LITERAL
                | tsr_binder::SymbolFlags::ENUM
                | tsr_binder::SymbolFlags::VALUE_MODULE,
        ) || flags.contains(tsr_binder::SymbolFlags::CLASS)
        {
            return false;
        }
        [crate::signatures::SignatureKind::Call, crate::signatures::SignatureKind::Construct]
            .into_iter()
            .all(|kind| {
                self.signatures_of_type_kind(id, kind)
                    .is_some_and(|signatures| signatures.is_empty())
            })
    }

    /// isApplicableIndexType (checker.go:19054). Unknown structural relations
    /// cannot prove applicability; numeric names retain the upstream exception.
    pub(crate) fn is_applicable_index_type(&mut self, source: TypeId, target: TypeId) -> bool {
        use crate::relater::{Relation, Ternary};
        if self.relate_ternary(source, target, Relation::Assignable) == Ternary::Related {
            return true;
        }
        if target == self.intrinsics.string {
            return self.relate_ternary(source, self.intrinsics.number, Relation::Assignable)
                == Ternary::Related;
        }
        if target == self.intrinsics.number {
            if self.template_literal_parts.get(&source).is_some_and(|parts| {
                parts.types.as_slice() == [self.intrinsics.number]
                    && parts.texts.iter().all(String::is_empty)
            }) {
                return true;
            }
            return match &self.store.get(source).data {
                TypeData::StringLiteral(value)
                | TypeData::EnumLiteral {
                    value: crate::types::EnumLiteralValue::String(value),
                    ..
                } => is_numeric_literal_name(value),
                _ => false,
            };
        }
        false
    }
}

/// Ported from `isNumericLiteralName` (`checker.go`): a name is numeric when
/// converting it to a number and back gives the same string.
///
/// That round-trip is the definition rather than a shortcut — it is what makes
/// `"0"` numeric and `"00"`, `"1.0"` and `" 1"` not, all of which are distinct
/// property names.
pub(crate) fn is_numeric_literal_name(name: &str) -> bool {
    let Ok(value) = name.parse::<f64>() else { return false };
    crate::printing::normalise_number(name) == name && value.is_finite()
}

#[cfg(test)]
mod completed_array_alias_tests {
    use super::*;

    const LIB: &str = "interface Array<T> { [index: number]: T; length: number; }
interface ReadonlyArray<T> { readonly [index: number]: T; length: number; }";

    fn with_checker(source: &str, test: impl FnOnce(&mut Checker<'_, '_>, tsr_ast::NodeId)) {
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let root = Node::SourceFile(parsed.source_file).node_id().unwrap();
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "array-alias.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        checker.set_strict_null_checks(true);
        test(&mut checker, root);
    }

    fn receiver(checker: &mut Checker<'_, '_>, root: tsr_ast::NodeId, name: &str) -> TypeId {
        let symbol = checker.binder.lookup_local(root, name).unwrap();
        checker.get_type_of_symbol(symbol)
    }

    fn publication(checker: &Checker<'_, '_>) -> String {
        format!(
            "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
            checker.store,
            checker.alias_body_evaluations,
            checker.instantiations,
            checker.type_reference_targets,
            checker.declared_types,
            checker.symbol_types,
            checker
                .pending_signature_returns
                .iter()
                .map(|(key, state)| { (key, *state == crate::signatures::LazyReturnState::Active) })
                .collect::<Vec<_>>(),
            checker.signature_returns,
        )
    }

    #[test]
    fn completed_array_alias_indexes_keep_native_element_and_declaration_identities() {
        let source = format!(
            "{LIB} type Rec<T> = Array<T | Rec<T>>;
type ReadRec<T> = ReadonlyArray<T | ReadRec<T>>;
declare const numbers: Rec<number>; declare const strings: ReadRec<string>;"
        );
        for reverse in [false, true] {
            with_checker(&source, |checker, root| {
                let mut controls = [("numbers", false), ("strings", true)];
                if reverse {
                    controls.reverse();
                }
                for (name, readonly) in controls {
                    let original = receiver(checker, root, name);
                    let key = checker.type_reference_targets[&original].clone();
                    // Actual property entry prepares the existing alias owner;
                    // the index reader itself must never prepare it.
                    assert_eq!(
                        checker.get_type_of_property_of_type(original, "length"),
                        Some(checker.intrinsics.number),
                    );
                    let body = checker.alias_body_evaluations[&key];
                    let expected = checker.get_index_infos_of_type(body).unwrap();
                    assert_eq!(expected.len(), 1);
                    assert_eq!(expected[0].key, checker.intrinsics.number);
                    assert_eq!(expected[0].readonly, readonly);
                    assert!(expected[0].declaration.is_some());
                    assert_eq!(expected[0].value, checker.type_reference_targets[&body].1[0]);
                    let TypeData::Union { types, .. } = &checker.store.get(expected[0].value).data
                    else {
                        panic!("native recursive array element is a union")
                    };
                    assert!(types.contains(&original));
                    assert!(types.contains(&if readonly {
                        checker.intrinsics.string
                    } else {
                        checker.intrinsics.number
                    }));
                    let before = publication(checker);
                    for _ in 0..3 {
                        assert_eq!(
                            checker.get_index_infos_of_type(original),
                            Some(expected.clone())
                        );
                        assert_eq!(receiver(checker, root, name), original);
                        assert_eq!(publication(checker), before);
                    }
                }
            });
        }
    }

    #[test]
    fn cold_reverse_and_warm_index_entries_never_evaluate_an_alias() {
        let source = format!(
            "{LIB} type Pair<A, B> = Array<[A, B]>;
declare const left: Pair<string, number>; declare const right: Pair<number, string>;
const leftRead = left[0]; const rightRead = right[0];"
        );
        for first in ["left", "right"] {
            with_checker(&source, |checker, root| {
                let original = receiver(checker, root, first);
                let key = checker.type_reference_targets[&original].clone();
                assert!(!checker.alias_body_evaluations.contains_key(&key));
                let before = checker.alias_body_evaluations.clone();
                assert!(checker.get_index_infos_of_type(original).unwrap().is_empty());
                assert_eq!(checker.alias_body_evaluations, before);
                assert_eq!(
                    checker.get_type_of_property_of_type(original, "length"),
                    Some(checker.intrinsics.number),
                );
                for name in [first, if first == "left" { "right" } else { "left" }, first] {
                    let value = receiver(checker, root, name);
                    checker.get_type_of_property_of_type(value, "length").unwrap();
                    let infos = checker.get_index_infos_of_type(value).unwrap();
                    assert_eq!(infos.len(), 1);
                    let expected = if name == "left" {
                        [checker.intrinsics.string, checker.intrinsics.number]
                    } else {
                        [checker.intrinsics.number, checker.intrinsics.string]
                    };
                    assert_eq!(checker.tuple_element_lists[&infos[0].value].0, expected);
                    let read = receiver(
                        checker,
                        root,
                        if name == "left" { "leftRead" } else { "rightRead" },
                    );
                    assert_eq!(read, infos[0].value);
                    assert_eq!(receiver(checker, root, name), value);
                }
            });
        }
    }

    #[test]
    fn active_foreign_and_captured_array_alias_views_do_not_reuse_completion() {
        let source = format!(
            "{LIB} type Rec<T> = Array<T | Rec<T>>; declare const value: Rec<number>;
function enclosing<X>() {{ type Captured<T> = Array<T | X>; let local!: Captured<number>; return local; }}"
        );
        with_checker(&source, |checker, root| {
            let original = receiver(checker, root, "value");
            checker.get_type_of_property_of_type(original, "length").unwrap();
            let key = checker.type_reference_targets[&original].clone();
            let body = checker.alias_body_evaluations[&key];
            checker.get_index_infos_of_type(body).unwrap();
            for context in 0..5 {
                match context {
                    0 => assert!(
                        checker
                            .resolutions
                            .push(key.0, crate::resolution::PropertyName::DeclaredType)
                    ),
                    1 => checker.alias_evaluation_bindings.push(rustc_hash::FxHashMap::default()),
                    2 => checker.mapped_template_depth = 1,
                    3 => checker.instantiation_depth = 1,
                    _ => checker.identity_unmapped_type_parameters = true,
                }
                let before = publication(checker);
                assert!(checker.get_index_infos_of_type(original).unwrap().is_empty());
                assert_eq!(publication(checker), before);
                match context {
                    0 => assert!(checker.resolutions.pop()),
                    1 => {
                        checker.alias_evaluation_bindings.pop();
                    }
                    2 => checker.mapped_template_depth = 0,
                    3 => checker.instantiation_depth = 0,
                    _ => checker.identity_unmapped_type_parameters = false,
                }
            }
            let foreign = checker.store.new_named(TypeFlags::OBJECT, "foreign".into(), Some(key.0));
            checker.type_reference_targets.insert(foreign, key);
            let before = publication(checker);
            assert!(checker.get_index_infos_of_type(foreign).unwrap().is_empty());
            assert_eq!(publication(checker), before);
            let mut walk = vec![Node::SourceFile(match checker.node_map.get(root).unwrap() {
                Node::SourceFile(file) => file,
                _ => unreachable!(),
            })];
            let local = loop {
                let node = walk.pop().unwrap();
                if let Node::VariableDeclaration(declaration) = node
                    && matches!(declaration.name, Some(tsr_ast::BindingName::Identifier(name)) if name.text == "local")
                {
                    break checker.binder.symbol_of(declaration.node_id.unwrap()).unwrap();
                }
                tsr_ast::push_children(node, &mut walk);
            };
            let captured = checker.get_type_of_symbol(local);
            checker.get_type_of_property_of_type(captured, "length").unwrap();
            let before = publication(checker);
            assert!(checker.get_index_infos_of_type(captured).unwrap().is_empty());
            assert_eq!(publication(checker), before);
        });
    }

    #[test]
    fn missing_failed_indirect_and_noncanonical_bodies_are_not_completed_by_the_view() {
        let source = format!(
            "{LIB} type Direct<T> = Array<T>; type Indirect<T> = Direct<T>;
declare const direct: Direct<string>; declare const indirect: Indirect<string>;"
        );
        with_checker(&source, |checker, root| {
            let direct = receiver(checker, root, "direct");
            checker.get_type_of_property_of_type(direct, "length").unwrap();
            let key = checker.type_reference_targets[&direct].clone();
            let body = checker.alias_body_evaluations[&key];
            checker.get_index_infos_of_type(body).unwrap();
            assert_eq!(checker.completed_original_array_alias_body(direct), Some(body));
            for completed in [None, Some(checker.intrinsics.error)] {
                if let Some(completed) = completed {
                    checker.alias_body_evaluations.insert(key.clone(), completed);
                } else {
                    checker.alias_body_evaluations.remove(&key);
                }
                let before = publication(checker);
                assert!(checker.completed_original_array_alias_body(direct).is_none());
                assert!(checker.get_index_infos_of_type(direct).unwrap().is_empty());
                assert_eq!(publication(checker), before);
            }
            let image = checker.store.new_named(
                TypeFlags::OBJECT,
                "not an original array".into(),
                Some(checker.type_reference_targets[&body].0),
            );
            checker
                .type_reference_targets
                .insert(image, checker.type_reference_targets[&body].clone());
            checker.alias_body_evaluations.insert(key.clone(), image);
            let before = publication(checker);
            assert!(checker.completed_original_array_alias_body(direct).is_none());
            assert_eq!(publication(checker), before);
            checker.alias_body_evaluations.insert(key, body);
            let indirect = receiver(checker, root, "indirect");
            checker.get_type_of_property_of_type(indirect, "length").unwrap();
            let before = publication(checker);
            assert!(checker.completed_original_array_alias_body(indirect).is_none());
            assert!(checker.get_index_infos_of_type(indirect).unwrap().is_empty());
            assert_eq!(publication(checker), before);
        });
    }
}
