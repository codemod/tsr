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
    types::{TypeData, TypeId},
};

/// One index signature, reduced to what a lookup needs.
///
/// Upstream's `IndexInfo` (`types.go`), retaining the key, value and readonly
/// flag. Declaration provenance and general index-write diagnostics are absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexInfo {
    /// `keyType` — a valid primitive, pattern or nongeneric intersection key.
    pub key: TypeId,
    /// `valueType`, what an applicable access yields.
    pub value: TypeId,
    /// `isReadonly`, preserved by instantiation and combined with index values.
    pub readonly: bool,
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
                        } else {
                            infos.push(next);
                        }
                    }
                }
                return Some(infos);
            }
            _ => {}
        }
        // §262. An ENUM's object type is `TypeData::Anonymous`, not `Named`, so
        // it returned empty here before the collector was ever asked — proven
        // by probe: `index_infos_of_symbol` is invoked ZERO times on
        // `compiler/indexIntoEnum`. §261 synthesised the signature in the
        // collector and measured a clean `+0` for exactly that reason; the
        // repair is routing, and the synthesis is its second half.
        //
        // Upstream's enum object carries an implicit numeric index signature
        // returning `string` — the REVERSE MAPPING, where `E[0]` is the member
        // NAME rather than a member.
        //
        //     namespace M { enum E { } var x = E[0]; }
        //     >E[0] : string
        //
        // Handled HERE rather than by widening the `Named`/`Anonymous` split,
        // which the type model keeps apart on purpose: `Named.members` is where
        // `getPropertyOfType` looks, and `Anonymous.symbol` carries the
        // declarations a call reads signatures from. Routing every anonymous
        // type into the members collector would make `typeof C` offer a class's
        // INSTANCE members, which is the wrong answer rather than a missing one
        // (see `TypeData`'s note on why the two fields are separate).
        //
        // `CONST_ENUM` is excluded: it has no runtime object, so upstream mints
        // no reverse mapping for it.
        if let TypeData::Anonymous { symbol, .. } = self.store.get(id).data
            && self
                .binder
                .symbols()
                .get(symbol)
                .flags
                .intersects(tsr_binder::SymbolFlags::REGULAR_ENUM)
        {
            return Some(vec![IndexInfo {
                key: self.intrinsics.number,
                value: self.intrinsics.string,
                readonly: true,
            }]);
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
                    key: info.key,
                    value: self.instantiate_for_reference(id, info.value),
                    readonly: info.readonly,
                })
                .collect(),
        )
    }

    /// `getUnionIndexInfos` (internal/checker/checker.go): only keys present
    /// in every constituent survive; their value types are unioned.
    fn union_index_infos(&mut self, types: &[TypeId]) -> Option<Vec<IndexInfo>> {
        let mut constituents = Vec::with_capacity(types.len());
        for &ty in types {
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
    fn index_infos_of_symbol(
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

    /// getIndexInfosOfIndexSymbol splits union keys and retains each valid
    /// primitive, pattern or nongeneric intersection key (checker.go).
    pub(crate) fn index_infos_of_declaration(
        &mut self,
        signature: &tsr_ast::IndexSignatureDeclaration<'a>,
    ) -> Vec<IndexInfo> {
        let [parameter] = signature.parameters else { return Vec::new() };
        let (Some(key), Some(value)) = (parameter.r#type, signature.r#type) else {
            return Vec::new();
        };
        let key = self.get_type_from_type_node(key);
        let value = self.get_type_from_type_node(value);
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
        let value_node = signature.r#type?;
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
        let readonly = signature.modifiers.iter().any(|modifier| {
            matches!(modifier, tsr_ast::ModifierLike::Token(token)
                if token.kind == tsr_ast::SyntaxKind::ReadonlyKeyword)
        });
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
        (self.binder.merged_symbol(target) == self.binder.merged_symbol(record))
            .then_some(IndexInfo { key, value: arguments[1], readonly: false })
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
                key: self.intrinsics.unknown,
                value: self.get_intersection_type(
                    &applicable.iter().map(|info| info.value).collect::<Vec<_>>(),
                    None,
                ),
                readonly: applicable.iter().all(|info| info.readonly),
            }),
        }
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
                TypeData::StringLiteral(value) => is_numeric_literal_name(value),
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
