//! Intersection types: `A & B`.
//!
//! Ported from `Checker.getIntersectionType` / `getIntersectionTypeEx`
//! (`checker.go:26052`, `:26056`), `addTypesToIntersection` (`:26251`) and
//! `removeRedundantSupertypes` (`:26294`).
//!
//! # Source order, not sorted order — the opposite of a union
//!
//! This is the rule to get from upstream rather than by symmetry with
//! [`crate::unions`]. A union's constituents are sorted by `CompareTypes`;
//! an intersection's are held in an `orderedSet` and **kept in the order they
//! were written**. `var x: M1 & C1` prints `M1 & C1` and `var x: C1 & M1`
//! prints `C1 & M1` — both spellings appear in the baselines, which is what a
//! sort would make impossible.
//!
//! A **union constituent is parenthesised**: `T & ({} | null)`. This paragraph
//! used to continue *"Nothing else in the corpus needs parentheses inside an
//! intersection — there is not one baseline line with a function type as an
//! intersection constituent"*. **That claim was false and is corrected here:**
//! §594 measured **47** such lines (`typeof ErrImpl & (<T>() => T)`,
//! `T & (new (...args: any[]) => { … })`), and a further 10 where this port
//! ADDED parentheses upstream omits, around a union a type alias names.
//!
//! Both are now handled at the constituent, and the rule is stated as what it
//! is rather than as a corpus observation: `emitTypeNode` parenthesises a
//! constituent whose precedence is below `Intersection`
//! (`ast/precedence.go:425`–`:480`). Restricting the port to "only the case the
//! corpus exercises" was the reasonable-sounding move that hid a 57-line defect
//! — the corpus had been consulted for the union case and never for the others.
//!
//! # The reduction that needs no assignability, and the one that does
//!
//! Most of upstream's emptiness rules are **pure flag arithmetic** over
//! `TypeFlagsDisjointDomains` (`types.go:480`) and port directly:
//! `string & number` is `never`, and the baselines record exactly that
//! (`switchCaseWithIntersectionType`). Two distinct *unit* types reduce the same
//! way, by upstream's own trick of adding `NON_PRIMITIVE` to `includes` so the
//! disjoint-domain test fires (`checker.go:26283`).
//!
//! The two-constituent type-variable reduction (`checker.go:26128`) follows
//! the parameter's base constraint and compares primitive or empty-object
//! domains. An unconstrained parameter keeps its intersection; a disjoint
//! primitive constraint reduces it to `never`.
//!
//! Union operands distribute into a union of intersections. The semantic
//! constituents stay expanded, while a shorter denormalized origin retains
//! forms such as `A & (B | C)` for printing, as upstream does.

use tsr_binder::SymbolId;

use crate::{
    checker::Checker,
    flags::TypeFlags,
    printing,
    types::{TypeData, TypeId, TypeStore},
};

/// What `addTypesToIntersection` learned. See [`crate::unions::Includes`] for
/// why this is a struct rather than extra bits in [`TypeFlags`].
#[derive(Debug, Default, Clone, Copy)]
struct Includes {
    flags: TypeFlags,
    /// `TypeFlagsIncludesError`.
    error: bool,
    /// An empty anonymous object type `{}` was a constituent. Upstream tracks
    /// this as `IncludesEmptyObject` and gives it its own reduction rules
    /// (`{}` is removed beside a definitely-non-nullable type).
    empty_object: bool,
}

/// Create an intersection type, interned on its contents.
///
/// A free function over the store for symmetry with
/// [`crate::unions::create_union`], and because the printed form needs the
/// constituents while nothing else does.
fn create_intersection(
    store: &mut TypeStore,
    types: Vec<TypeId>,
    symbol: Option<(SymbolId, String)>,
) -> TypeId {
    let text = match &symbol {
        Some((_, name)) => name.clone(),
        None => types
            .iter()
            .map(|&id| {
                let constituent = store.get(id);
                let printed = printing::type_to_string(constituent);
                // `emitTypeNode(node, TypePrecedenceIntersection)`
                // (`printer.go:2274`) parenthesises every constituent whose own
                // precedence is BELOW `Intersection` on the ladder
                // (`ast/precedence.go:425`–`:480`, ascending: `Conditional`,
                // `JSDoc`, `Function`, `Union`, `Intersection`, …). That is two
                // kinds this port can produce, and only one was here.
                //
                // A union binds less tightly: `T & ({} | null)`. `boolean` is
                // exempt — it carries UNION (it *is* `false | true`) but prints
                // as the keyword, and upstream tests BOOLEAN before its union
                // branch (`nodebuilderimpl.go:3255`), so it never reaches the
                // parenthesiser (§89.1's I4).
                //
                // §594, the two corrections:
                //
                // - a union a type ALIAS names prints as that name, which the
                //   node builder emits as a `TypeReferenceNode` at the HIGHEST
                //   precedence — so `Options & { kind: K; }`, never
                //   `(Options) & { kind: K; }`. This is the identical
                //   distinction `unions.rs`'s `parenthesised` records getting
                //   wrong on its first run at 19 lines; the union road learned
                //   it and the intersection road did not.
                // - a FUNCTION or CONSTRUCTOR type sits below `Intersection`
                //   too, and was never parenthesised here at all:
                //   `typeof ErrImpl & (<T>() => T)` and
                //   `T & (new (...args: any[]) => { … })`.
                let needs = if constituent.flags.contains(TypeFlags::UNION)
                    && !constituent.flags.contains(TypeFlags::BOOLEAN)
                {
                    !printing::prints_as_a_single_token(constituent)
                } else {
                    matches!(&constituent.data, TypeData::Anonymous { signature, .. } if *signature)
                };
                if needs { format!("({printed})") } else { printed }
            })
            .collect::<Vec<_>>()
            .join(" & "),
    };
    let symbol = symbol.map(|(id, _)| id);
    store.intern_intersection(
        TypeFlags::INTERSECTION,
        TypeData::Intersection { text, types, symbol },
    )
}

impl Checker<'_, '_> {
    /// Ported from `Checker.getIntersectionTypeEx` (`checker.go:26056`) at
    /// `IntersectionFlagsNone`.
    pub(crate) fn get_intersection_type(
        &mut self,
        types: &[TypeId],
        symbol: Option<SymbolId>,
    ) -> TypeId {
        let (mut set, includes) = self.add_types_to_intersection(types);

        // "An intersection type is considered empty if it contains the type
        // never" (`checker.go:26071`). `silentNeverType` is upstream's other
        // answer here and this port does not create one.
        if includes.flags.contains(TypeFlags::NEVER) {
            return self.intrinsics.never;
        }

        // The disjoint-domain rules, `checker.go:26077`. Pure flag arithmetic:
        // a string-like type beside anything from a *different* domain is the
        // empty set, and so on for each domain. `string & number` is `never`.
        if is_disjoint(includes.flags, self.strict_null_checks, includes.empty_object) {
            return self.intrinsics.never;
        }

        if includes.flags.contains(TypeFlags::ANY) {
            // `checker.go:26092`, and the same "a gap in a constituent is a gap
            // in the whole" that unions get for free: `errorType` carries
            // `ANY`, and `IncludesError` wins.
            return if includes.error { self.intrinsics.error } else { self.intrinsics.any };
        }

        // getIntersectionTypeEx preserves nullable types in non-strict mode,
        // except beside the empty object type, where the intersection is empty.
        if !self.strict_null_checks && includes.flags.intersects(TypeFlags::NULLABLE) {
            return if includes.empty_object {
                self.intrinsics.never
            } else if includes.flags.contains(TypeFlags::UNDEFINED) {
                self.intrinsics.undefined
            } else {
                self.intrinsics.null
            };
        }

        set = self.remove_redundant_supertypes(set, includes.flags);

        if set.is_empty() {
            // `checker.go:26124`. Not `never`: an intersection of nothing is the
            // type that constrains nothing.
            return self.intrinsics.unknown;
        }
        if set.len() == 1 && symbol.is_none() {
            return set[0];
        }
        // getIntersectionTypeEx reduces a primitive-constrained type variable
        // against a primitive or {}. Unknown constraints keep the intersection.
        if set.len() == 2
            && set.iter().any(|&id| self.store.get(id).flags.contains(TypeFlags::TYPE_PARAMETER))
            && set.iter().any(|&id| {
                self.store.get(id).flags.intersects(TypeFlags::PRIMITIVE | TypeFlags::NON_PRIMITIVE)
                    || includes.empty_object
            })
        {
            let variable_index =
                usize::from(!self.store.get(set[0]).flags.contains(TypeFlags::TYPE_PARAMETER));
            let variable = set[variable_index];
            let other = set[1 - variable_index];
            match self.reduce_constrained_intersection(variable, other) {
                Ok(Some(reduced)) => return reduced,
                Ok(None) => {}
                Err(()) => return self.intrinsics.error,
            }
        }

        let named = symbol.map(|id| (id, self.binder.symbols().get(id).name.to_string()));
        if includes.flags.contains(TypeFlags::UNION) {
            return self.intersect_union_constituents(&set, symbol, types.len());
        }
        create_intersection(&mut self.store, set, named)
    }

    /// The constraint-reduction branch of `getIntersectionTypeEx` (checker.go).
    /// This relation is decidable for primitive constraints and empty objects.
    fn reduce_constrained_intersection(
        &mut self,
        variable: TypeId,
        other: TypeId,
    ) -> Result<Option<TypeId>, ()> {
        let mut parameter = variable;
        let mut seen = Vec::new();
        let constraint = loop {
            if seen.contains(&parameter) {
                return Err(());
            }
            seen.push(parameter);
            let Some(&symbol) = self.type_parameter_symbols.get(&parameter) else { return Err(()) };
            let declaration =
                self.binder.symbols().get(symbol).declarations.first().copied().ok_or(())?;
            let Some(tsr_ast::Node::TypeParameterDeclaration(declaration)) =
                self.node_map.get(declaration)
            else {
                return Ok(None);
            };
            let Some(annotation) = declaration.constraint else { return Ok(None) };
            let constraint = self.get_type_from_type_node(annotation);
            if self.is_error(constraint) {
                return Err(());
            }
            if !self.store.get(constraint).flags.contains(TypeFlags::TYPE_PARAMETER) {
                break constraint;
            }
            parameter = constraint;
        };
        let parts = match &self.store.get(constraint).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => vec![constraint],
        };
        if !parts.iter().all(|&id| {
            self.store.get(id).flags.intersects(TypeFlags::PRIMITIVE | TypeFlags::NON_PRIMITIVE)
                || self.is_empty_anonymous_object_type(id)
        }) {
            return Ok(None);
        }
        if parts.iter().any(|&id| {
            self.store
                .get(id)
                .flags
                .intersects(TypeFlags::TEMPLATE_LITERAL | TypeFlags::STRING_MAPPING)
        }) {
            return Err(());
        }
        if parts.iter().all(|&id| self.primitive_or_empty_subtype(id, other)) {
            return Ok(Some(variable));
        }
        if !parts.iter().any(|&id| self.primitive_or_empty_subtype(id, other))
            && !parts.iter().any(|&id| self.primitive_or_empty_subtype(other, id))
        {
            return Ok(Some(self.intrinsics.never));
        }
        Ok(None)
    }

    fn primitive_or_empty_subtype(&mut self, source: TypeId, target: TypeId) -> bool {
        if self.is_empty_anonymous_object_type(target) {
            return self.is_empty_anonymous_object_type(source)
                || self
                    .store
                    .get(source)
                    .flags
                    .intersects(TypeFlags::DEFINITELY_NON_NULLABLE | TypeFlags::NEVER);
        }
        if self.is_empty_anonymous_object_type(source) {
            return false;
        }
        self.is_type_strict_subtype_of(source, target)
    }

    /// `getIntersectionTypeEx` and `getCrossProductIntersections` (checker.go):
    /// distribute an intersection over its union operands, then preserve a
    /// shorter denormalized origin for printing.
    fn intersect_union_constituents(
        &mut self,
        types: &[TypeId],
        symbol: Option<SymbolId>,
        input_count: usize,
    ) -> TypeId {
        if let Some(reduced) = self.intersect_primitive_unions(types) {
            return self.get_intersection_type(&reduced, symbol);
        }
        // Upstream factors a shared nullable constituent out before expanding.
        for nullable in [self.intrinsics.undefined, self.intrinsics.null] {
            if types.iter().all(|&id| {
                matches!(&self.store.get(id).data, TypeData::Union { types, .. } if types.contains(&nullable))
            }) {
                let mut non_nullable = Vec::with_capacity(types.len());
                for &id in types {
                    let TypeData::Union { types, .. } = &self.store.get(id).data else { unreachable!() };
                    let remaining: Vec<_> = types.iter().copied().filter(|&id| id != nullable).collect();
                    non_nullable.push(self.get_union_type(&remaining));
                }
                let intersected = self.get_intersection_type(&non_nullable, None);
                return self.intersection_result_union(&[intersected, nullable], symbol);
            }
        }
        // Divide before expanding a large input, as getIntersectionTypeEx does.
        if types.len() >= 3 && input_count > 2 {
            let middle = types.len() / 2;
            let left = self.get_intersection_type(&types[..middle], None);
            let right = self.get_intersection_type(&types[middle..], None);
            return self.get_intersection_type(&[left, right], symbol);
        }
        let mut count = 1usize;
        for &id in types {
            if let TypeData::Union { types, .. } = &self.store.get(id).data {
                count = count.saturating_mul(types.len());
            }
        }
        if count >= 100_000 {
            return self.intrinsics.error;
        }
        let mut intersections = Vec::new();
        for index in 0..count {
            let mut constituents = types.to_vec();
            let mut selection = index;
            for (position, &id) in types.iter().enumerate().rev() {
                if let TypeData::Union { types, .. } = &self.store.get(id).data {
                    constituents[position] = types[selection % types.len()];
                    selection /= types.len();
                }
            }
            let intersected = self.get_intersection_type(&constituents, None);
            if !self.store.get(intersected).flags.contains(TypeFlags::NEVER) {
                intersections.push(intersected);
            }
        }
        let needs_origin = symbol.is_none()
            && intersections
                .iter()
                .any(|&id| self.store.get(id).flags.contains(TypeFlags::INTERSECTION))
            && intersections
                .iter()
                .map(|&id| self.intersection_constituent_count(id))
                .sum::<usize>()
                > types.iter().map(|&id| self.intersection_constituent_count(id)).sum::<usize>();
        let result = self.intersection_result_union(&intersections, symbol);
        if needs_origin {
            let origin = create_intersection(&mut self.store, types.to_vec(), None);
            let text = self.type_to_string(origin);
            let result = self.union_with_origin_text(result, text);
            if self.store.get(result).flags.contains(TypeFlags::UNION) {
                self.union_origin.insert(result, vec![origin]);
            }
            result
        } else {
            result
        }
    }

    fn intersection_result_union(&mut self, types: &[TypeId], symbol: Option<SymbolId>) -> TypeId {
        match symbol {
            Some(symbol) => self.get_named_union_type(types, TypeFlags::empty(), symbol),
            None => self.get_union_type(types),
        }
    }

    /// `getConstituentCount` (checker.go), including aliases and union origins.
    fn intersection_constituent_count(&self, id: TypeId) -> usize {
        let types = match &self.store.get(id).data {
            TypeData::Union { symbol: Some(_), .. }
            | TypeData::Intersection { symbol: Some(_), .. } => return 1,
            TypeData::Union { types, .. } => self.union_origin.get(&id).unwrap_or(types),
            TypeData::Intersection { types, .. } => types,
            _ => return 1,
        };
        types.iter().map(|&id| self.intersection_constituent_count(id)).sum()
    }

    /// `intersectUnionsOfPrimitiveTypes` / `unionContainsType` (checker.go).
    /// Primitive union membership includes a literal's corresponding primitive.
    fn intersect_primitive_unions(&mut self, types: &[TypeId]) -> Option<Vec<TypeId>> {
        let unions: Vec<_> = types
            .iter()
            .enumerate()
            .filter_map(|(index, &id)| {
                let TypeData::Union { types, .. } = &self.store.get(id).data else { return None };
                types
                    .iter()
                    .all(|&id| self.store.get(id).flags.intersects(TypeFlags::PRIMITIVE))
                    .then(|| (index, types.clone()))
            })
            .collect();
        if unions.len() < 2 {
            return None;
        }
        let mut candidates = Vec::new();
        for (_, members) in &unions {
            for &id in members {
                if candidates.contains(&id) {
                    continue;
                }
                let flags = self.store.get(id).flags;
                let primitive = if flags.contains(TypeFlags::STRING_LITERAL) {
                    Some(self.intrinsics.string)
                } else if flags.intersects(TypeFlags::ENUM | TypeFlags::NUMBER_LITERAL) {
                    Some(self.intrinsics.number)
                } else if flags.contains(TypeFlags::BIG_INT_LITERAL) {
                    Some(self.intrinsics.bigint)
                } else if flags.contains(TypeFlags::UNIQUE_ES_SYMBOL) {
                    Some(self.intrinsics.es_symbol)
                } else {
                    None
                };
                if unions.iter().all(|(_, members)| {
                    members.contains(&id)
                        || primitive.is_some_and(|primitive| members.contains(&primitive))
                }) {
                    candidates.push(id);
                }
            }
        }
        let reduced = self.get_union_type(&candidates);
        Some(
            types
                .iter()
                .enumerate()
                .filter_map(|(index, &id)| {
                    if index == unions[0].0 {
                        Some(reduced)
                    } else if unions.iter().any(|(union_index, _)| *union_index == index) {
                        None
                    } else {
                        Some(id)
                    }
                })
                .collect(),
        )
    }

    /// `addTypesToIntersection` / `addTypeToIntersection` (`checker.go:26251`).
    ///
    /// Every constituent is made **regular** first (`checker.go:26253`), nested
    /// intersections are flattened, and duplicates are dropped while preserving
    /// the order of first appearance.
    fn add_types_to_intersection(&mut self, source: &[TypeId]) -> (Vec<TypeId>, Includes) {
        let mut set: Vec<TypeId> = Vec::with_capacity(source.len());
        let mut includes = Includes::default();
        for &id in source {
            let regular = self.get_regular_type_of_literal_type(id);
            self.add_type_to_intersection(&mut set, &mut includes, regular);
        }
        (set, includes)
    }

    fn add_type_to_intersection(
        &mut self,
        set: &mut Vec<TypeId>,
        includes: &mut Includes,
        id: TypeId,
    ) {
        let ty = self.store.get(id);
        let flags = ty.flags;
        if flags.contains(TypeFlags::INTERSECTION) {
            let TypeData::Intersection { types, .. } = &ty.data else { return };
            for constituent in types.clone() {
                self.add_type_to_intersection(set, includes, constituent);
            }
            return;
        }
        if self.is_empty_anonymous_object_type(id) {
            if !includes.empty_object {
                set.push(id);
            }
            includes.empty_object = true;
            return;
        }
        if flags.intersects(TypeFlags::ANY_OR_UNKNOWN) {
            if self.is_error(id) {
                includes.error = true;
            }
        } else if (self.strict_null_checks || !flags.intersects(TypeFlags::NULLABLE))
            && !set.contains(&id)
        {
            // "We have seen two distinct unit types which means we should reduce
            // to an empty intersection. Adding TypeFlags.NonPrimitive causes that
            // to happen." (`checker.go:26283`) — upstream's own trick, kept
            // because it is what makes the disjoint test below catch `"a" & "b"`.
            if flags.intersects(TypeFlags::UNIT) && includes.flags.intersects(TypeFlags::UNIT) {
                includes.flags |= TypeFlags::NON_PRIMITIVE;
            }
            set.push(id);
        }
        includes.flags |= flags;
    }

    /// `removeRedundantSupertypes` (`checker.go:26294`).
    ///
    /// A primitive is redundant beside one of its own literals: `string & "a"`
    /// is `"a"`. The mirror image of the union rule, which drops the *literal*
    /// beside its primitive.
    ///
    fn remove_redundant_supertypes(
        &mut self,
        mut types: Vec<TypeId>,
        includes: TypeFlags,
    ) -> Vec<TypeId> {
        let mut index = types.len();
        while index > 0 {
            index -= 1;
            let flags = self.store.get(types[index]).flags;
            let remove = flags.contains(TypeFlags::STRING)
                && includes.contains(TypeFlags::STRING_LITERAL)
                || flags.contains(TypeFlags::NUMBER)
                    && includes.contains(TypeFlags::NUMBER_LITERAL)
                || flags.contains(TypeFlags::BIG_INT)
                    && includes.contains(TypeFlags::BIG_INT_LITERAL)
                || flags.contains(TypeFlags::ES_SYMBOL)
                    && includes.contains(TypeFlags::UNIQUE_ES_SYMBOL)
                || flags.contains(TypeFlags::VOID) && includes.contains(TypeFlags::UNDEFINED)
                || self.is_empty_anonymous_object_type(types[index])
                    && includes.intersects(TypeFlags::DEFINITELY_NON_NULLABLE);
            if remove {
                types.remove(index);
            }
        }
        types
    }

    /// `IsEmptyAnonymousObjectType` (checker.go). Only a complete structural
    /// property table establishes emptiness; missing member data does not.
    fn is_empty_anonymous_object_type(&self, id: TypeId) -> bool {
        self.anonymous_properties.get(&id).is_some_and(|(properties, _)| properties.is_empty())
    }
}

/// The disjoint-domain emptiness rules (`checker.go:26077`).
///
/// A type from one domain intersected with a type from any *other* domain is
/// the empty set. `TypeFlagsDisjointDomains` (`types.go:480`) names the domains,
/// and each test asks whether anything outside this one is present.
///
fn is_disjoint(includes: TypeFlags, strict_null_checks: bool, empty_object: bool) -> bool {
    let disjoint = TypeFlags::NON_PRIMITIVE
        | TypeFlags::STRING_LIKE
        | TypeFlags::NUMBER_LIKE
        | TypeFlags::BIG_INT_LIKE
        | TypeFlags::BOOLEAN_LIKE
        | TypeFlags::ES_SYMBOL_LIKE
        | TypeFlags::VOID_LIKE
        | TypeFlags::NULL;
    let other_than = |domain: TypeFlags| {
        includes.intersects(domain) && includes.intersects(disjoint.difference(domain))
    };
    strict_null_checks
        && includes.intersects(TypeFlags::NULLABLE)
        && (includes.intersects(TypeFlags::OBJECT | TypeFlags::NON_PRIMITIVE) || empty_object)
        || other_than(TypeFlags::NON_PRIMITIVE)
        || other_than(TypeFlags::STRING_LIKE)
        || other_than(TypeFlags::NUMBER_LIKE)
        || other_than(TypeFlags::BIG_INT_LIKE)
        || other_than(TypeFlags::ES_SYMBOL_LIKE)
        || other_than(TypeFlags::VOID_LIKE)
}
