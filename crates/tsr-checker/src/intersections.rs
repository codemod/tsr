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
//! A **union constituent is parenthesised**: `T & ({} | null)`. Nothing else in
//! the corpus needs parentheses inside an intersection — there is not one
//! baseline line with a function type as an intersection constituent — so only
//! the union case is ported, rather than a general precedence table that would
//! be a guess everywhere it was not exercised.
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
//! What is **not** here is the two-constituent type-variable reduction
//! (`checker.go:26128`), which asks `getBaseConstraintOfType` and
//! `isTypeStrictSubtypeOf` — assignability, which does not exist. `T & string`
//! where `T` is a type parameter is therefore a **gap**, not a guess: upstream
//! may answer `T`, `never`, or the intersection itself depending on `T`'s
//! constraint, and there is no way to tell which without the constraint.

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
    /// (`X & {}` deliberately skips supertype reduction, `{}` is removed beside
    /// a definitely-non-nullable type). None of that is ported, so its presence
    /// makes the whole intersection a gap.
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
                // A union binds less tightly than an intersection, so it is
                // parenthesised: `T & ({} | null)`.
                if constituent.flags.contains(TypeFlags::UNION) {
                    format!("({printed})")
                } else {
                    printed
                }
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

        // `{}` has reduction rules of its own that are not ported — see
        // [`Includes::empty_object`].
        if includes.empty_object {
            return self.intrinsics.error;
        }

        // "An intersection type is considered empty if it contains the type
        // never" (`checker.go:26071`). `silentNeverType` is upstream's other
        // answer here and this port does not create one.
        if includes.flags.contains(TypeFlags::NEVER) {
            return self.intrinsics.never;
        }

        // The disjoint-domain rules, `checker.go:26077`. Pure flag arithmetic:
        // a string-like type beside anything from a *different* domain is the
        // empty set, and so on for each domain. `string & number` is `never`.
        if is_disjoint(includes.flags) {
            return self.intrinsics.never;
        }

        if includes.flags.contains(TypeFlags::ANY) {
            // `checker.go:26092`, and the same "a gap in a constituent is a gap
            // in the whole" that unions get for free: `errorType` carries
            // `ANY`, and `IncludesError` wins.
            return if includes.error { self.intrinsics.error } else { self.intrinsics.any };
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
        // The two-constituent type-variable reduction needs assignability — see
        // the module docs. Answering the plain intersection here would be right
        // *sometimes*, which is worse than a gap.
        if set.len() == 2
            && set.iter().any(|&id| self.store.get(id).flags.contains(TypeFlags::TYPE_PARAMETER))
            && set.iter().any(|&id| {
                self.store.get(id).flags.intersects(TypeFlags::PRIMITIVE | TypeFlags::NON_PRIMITIVE)
            })
        {
            return self.intrinsics.error;
        }

        let named = symbol.map(|id| (id, self.binder.symbols().get(id).name.to_string()));
        create_intersection(&mut self.store, set, named)
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
        if matches!(&ty.data, TypeData::Named { text, .. } if text == "{}") {
            // `IsEmptyAnonymousObjectType`. Recognised by its printed form
            // rather than by an object flag, because this port has no
            // `ObjectFlags` — a divergence that is safe only because the answer
            // is a gap either way.
            includes.empty_object = true;
            return;
        }
        if flags.intersects(TypeFlags::ANY_OR_UNKNOWN) {
            if self.is_error(id) {
                includes.error = true;
            }
        } else if !set.contains(&id) {
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
    /// The `{}`-beside-a-non-nullable clause is not ported; an intersection
    /// containing `{}` is a gap before this is reached.
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
                || flags.contains(TypeFlags::VOID) && includes.contains(TypeFlags::UNDEFINED);
            if remove {
                types.remove(index);
            }
        }
        types
    }
}

/// The disjoint-domain emptiness rules (`checker.go:26077`).
///
/// A type from one domain intersected with a type from any *other* domain is
/// the empty set. `TypeFlagsDisjointDomains` (`types.go:480`) names the domains,
/// and each test asks whether anything outside this one is present.
///
/// The `strictNullChecks && nullable && object-ish` clause is folded in on the
/// assumption the rest of this crate makes — that the option is **on** — see
/// [`crate::unions`] for the measurement behind it.
fn is_disjoint(includes: TypeFlags) -> bool {
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
    includes.intersects(TypeFlags::NULLABLE)
        && includes.intersects(TypeFlags::OBJECT | TypeFlags::NON_PRIMITIVE)
        || other_than(TypeFlags::NON_PRIMITIVE)
        || other_than(TypeFlags::STRING_LIKE)
        || other_than(TypeFlags::NUMBER_LIKE)
        || other_than(TypeFlags::BIG_INT_LIKE)
        || other_than(TypeFlags::ES_SYMBOL_LIKE)
        || other_than(TypeFlags::VOID_LIKE)
}
