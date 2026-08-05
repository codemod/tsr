//! The relation checker: is one type assignable to another?
//!
//! Ported from `internal/checker/relater.go`. Upstream parameterises one
//! algorithm over five *relations* — identity, assignable, subtype, strict
//! subtype and comparable — sharing `checkTypeRelatedTo` between them. Only the
//! **assignable** relation is ported here, because it is the only one any caller
//! in this crate needs yet: `getAssignmentReducedType` filters a declared union
//! by it, and `resolveCall` picks among overloads by it.
//!
//! # What is ported
//!
//! [`Checker::is_type_assignable_to`] over the type shapes this port actually
//! has:
//!
//! - the intrinsics and their `*Like` groupings — `any`, `unknown`, `never`,
//!   `string`/`number`/`bigint`/`boolean`/`symbol`, `void`, `undefined`, `null`,
//!   `object`;
//! - literal types, including the fresh/regular pair;
//! - unions on either side, and intersections on either side.
//!
//! # What is deliberately *not* ported, and why it answers `false`
//!
//! **Structural comparison of object types.** Two distinct object types — a
//! `Named` class or interface, or an `Anonymous` function type — relate here
//! only when they are the *same* [`TypeId`]. Anything else answers "not
//! related".
//!
//! That is a gap, and it is stated as one rather than papered over. Upstream's
//! `structuredTypeRelatedTo` needs machinery this crate does not have: property
//! *enumeration* of a target including its inherited members, variance markers
//! on type references, signature relation with bivariant parameter positions,
//! and index-signature matching. `get_property_of_type`
//! (`crate::members`) can look a name **up**, but nothing can list a type's
//! properties transitively through its base types, and a structural check that
//! enumerates only a target's *own* members would silently skip the inherited
//! requirements and answer `true` for a source that fails them. A wrong `true`
//! produces a confident wrong type; a wrong `false` produces a gap. So: `false`,
//! and `bd tsr-4sc` carries the follow-up.
//!
//! For the same reason there is no bare-`{}`/`object` structural arm, no
//! array-to-array covariance, and no signature comparison.
//!
//! # `strictNullChecks` is assumed on
//!
//! `isSimpleTypeRelatedTo` branches on `c.strictNullChecks` in two places: with
//! it **off**, `undefined` and `null` are assignable to everything except
//! `never`. This crate has no compiler options plumbed through to the checker,
//! and the two readings are not symmetric — assuming *off* makes the relation
//! maximally permissive, which is the direction that manufactures wrong answers.
//! Assuming *on* costs correctness only on files that actually disable it, and
//! costs it as a gap. So the strict reading is hard-coded, and this paragraph is
//! the note to delete when options arrive.
//!
//! # The recursion limits are part of the port, not an optimisation
//!
//! `bd tsr-el3.2` records that this project has twice *deliberately* skipped an
//! algorithmic limit because the loop it guards did not exist in the ported
//! subset. **Measured, that argument does apply here too, and the limits are
//! ported anyway.** The claim written first — that `interface I { x: I }` is a
//! cycle reachable through the union arm — is false for *this* walk: the only
//! cycles in a type graph run through an object type's members, and this module
//! stops at object types. Deleting the cycle guard and running
//! `tests/relater.rs::a_recursive_type_terminates` leaves it green in
//! milliseconds.
//!
//! They are ported regardless, because the loop appears in the same commit that
//! adds structural comparison and retrofitting a limit means re-deriving which
//! recursion it guards. What is *not* claimed is that they are tested: they are
//! unexercised, said plainly here and in the test, rather than counted.
//!
//! Both of upstream's guards are ported, in the form the ported subset needs:
//!
//! - a **depth cap** ([`MAX_DEPTH`]), from upstream's `isDeeplyNestedType`,
//!   which gives up at `maxDepth` occurrences on the stack;
//! - a **relation cache** keyed on the type pair, from upstream's `Relation`
//!   results map, which both memoises and — by parking an in-progress pair as
//!   *assumed related* — closes the co-recursive cycle the way
//!   `recursiveTypeRelatedTo` does with `RelationComparisonResultReported`.
//!
//! **Divergence, recorded:** upstream's `Relation` lives on the `Checker` and so
//! persists across every call. Here it is created per top-level
//! `is_type_assignable_to` call, because `checker.rs` is not this module's to add
//! a field to. The *termination* guarantee is identical — a cycle is closed
//! within the one walk that encounters it — and only the cross-call memo is
//! lost. That is a cost in time, not in answers.

use rustc_hash::FxHashMap;

use crate::{checker::Checker, flags::TypeFlags, types::TypeData, types::TypeId};

/// How deep the structural walk goes before giving up.
///
/// Upstream passes a `maxDepth` to `isDeeplyNestedType`
/// (`internal/checker/relater.go`), which reports a type as deeply nested once
/// `maxDepth` occurrences with the same recursion identity are on the stack. The
/// walk here has no recursion *identity* to compare — that needs type references
/// with an origin symbol — so the cap is on raw stack depth instead, which is
/// strictly more conservative: it can give up early where upstream would
/// continue, never the reverse. Giving up answers "not related", never "related".
pub const MAX_DEPTH: usize = 100;

/// The relation being checked.
///
/// Upstream's `*Relation` (`internal/checker/relater.go`), which is both the
/// identity of the relation and its results cache. Only the assignable relation
/// is ported; the enum exists so that the call sites read the way upstream's do
/// and so that adding `Subtype` later is an arm rather than a refactor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    /// Upstream's `c.assignableRelation`.
    Assignable,
}

/// One relation check, carrying the cache and the depth cap.
///
/// Upstream splits this state between the `Checker` (the persistent `Relation`
/// results map) and a per-check `Relater` struct holding `sourceStack`,
/// `targetStack` and `relationCount`. Both halves are per-check here; see the
/// module docs for why, and for what is lost.
struct Relater<'c, 'a, 'n> {
    checker: &'c mut Checker<'a, 'n>,
    relation: Relation,
    /// `(source, target) -> related`, upstream's `Relation.results`.
    ///
    /// An entry is written **before** the recursive walk with the value `true`,
    /// which is what closes a cycle: re-entering the same pair assumes the
    /// relation holds, exactly as upstream's `recursiveTypeRelatedTo` does when
    /// it finds the pair already on the stack. The assumption is discharged by
    /// the surrounding walk failing if any *other* constituent fails.
    results: FxHashMap<(TypeId, TypeId), bool>,
    depth: usize,
}

impl Checker<'_, '_> {
    /// Whether `source` is assignable to `target`.
    ///
    /// Ported from `Checker.isTypeAssignableTo` (`internal/checker/relater.go`).
    ///
    /// Read the module docs before trusting a `false`: object types that are not
    /// the same [`TypeId`] answer `false` because structural comparison is not
    /// ported, not because they are unrelated.
    #[must_use]
    pub fn is_type_assignable_to(&mut self, source: TypeId, target: TypeId) -> bool {
        self.is_type_related_to(source, target, Relation::Assignable)
    }

    /// Whether `source` and `target` stand in `relation`.
    ///
    /// Ported from `Checker.isTypeRelatedTo` (`internal/checker/relater.go`).
    #[must_use]
    pub fn is_type_related_to(
        &mut self,
        source: TypeId,
        target: TypeId,
        relation: Relation,
    ) -> bool {
        let mut relater =
            Relater { checker: self, relation, results: FxHashMap::default(), depth: 0 };
        relater.is_related_to(source, target)
    }
}

impl Relater<'_, '_, '_> {
    /// The body of `isTypeRelatedTo`, minus the entry-point bookkeeping.
    ///
    /// Ported from `Checker.isTypeRelatedTo` (`internal/checker/relater.go`).
    fn is_related_to(&mut self, source: TypeId, target: TypeId) -> bool {
        // Upstream reduces a fresh literal to its regular form on both sides
        // before comparing identity, so that `"a"` fresh and `"a"` regular are
        // one type here even though they are two interned types.
        let source = self.checker.get_regular_type_of_literal_type(source);
        let target = self.checker.get_regular_type_of_literal_type(target);
        if source == target {
            return true;
        }
        if self.is_simple_type_related_to(source, target) {
            return true;
        }
        let composite = TypeFlags::UNION.union(TypeFlags::INTERSECTION);
        let s = self.checker.type_of(source).flags;
        let t = self.checker.type_of(target).flags;
        if s.intersects(composite) || t.intersects(composite) {
            return self.recursive_type_related_to(source, target);
        }
        false
    }

    /// The non-recursive arms: everything decidable from flags alone.
    ///
    /// Ported from `Checker.isSimpleTypeRelatedTo`
    /// (`internal/checker/relater.go`). The arms are in upstream's order, which
    /// is load-bearing — `never` on the source side wins over `never` on the
    /// target side, so `never` is assignable to `never` by the first arm rather
    /// than rejected by the third.
    ///
    /// Four of upstream's arms are **not** ported, all for the same reason: this
    /// crate has no enum types, no `wildcardType` distinct from `errorType` in
    /// any reachable path, and no `uniqueESSymbol`. Each is a missing `true`,
    /// i.e. a gap.
    fn is_simple_type_related_to(&mut self, source: TypeId, target: TypeId) -> bool {
        let s = self.checker.type_of(source).flags;
        let t = self.checker.type_of(target).flags;
        // `any` on the right and `never` on the left relate to everything.
        // `errorType` is `ANY` here, which is upstream's behaviour too: an
        // erroneous type must not cascade a second error.
        if t.intersects(TypeFlags::ANY) || s.intersects(TypeFlags::NEVER) {
            return true;
        }
        // Upstream excludes `strictSubtypeRelation` with an `any` source here.
        // Only the assignable relation is ported, so the exclusion cannot fire —
        // matched rather than assumed, so that adding `Subtype` to [`Relation`]
        // fails to compile here instead of silently taking the wrong branch.
        let Relation::Assignable = self.relation;
        if t.intersects(TypeFlags::UNKNOWN) {
            return true;
        }
        if t.intersects(TypeFlags::NEVER) {
            return false;
        }
        if s.intersects(TypeFlags::STRING_LIKE) && t.intersects(TypeFlags::STRING) {
            return true;
        }
        if s.intersects(TypeFlags::NUMBER_LIKE) && t.intersects(TypeFlags::NUMBER) {
            return true;
        }
        if s.intersects(TypeFlags::BIG_INT_LIKE) && t.intersects(TypeFlags::BIG_INT) {
            return true;
        }
        if s.intersects(TypeFlags::BOOLEAN_LIKE) && t.intersects(TypeFlags::BOOLEAN) {
            return true;
        }
        if s.intersects(TypeFlags::ES_SYMBOL_LIKE) && t.intersects(TypeFlags::ES_SYMBOL) {
            return true;
        }
        // The two `strictNullChecks`-off arms are collapsed to their strict
        // reading; see the module docs for why the permissive one is not the
        // safe default to guess.
        if s.intersects(TypeFlags::UNDEFINED)
            && t.intersects(TypeFlags::UNDEFINED.union(TypeFlags::VOID))
        {
            return true;
        }
        if s.intersects(TypeFlags::NULL) && t.intersects(TypeFlags::NULL) {
            return true;
        }
        // Upstream guards this with a `strictSubtypeRelation` exception for the
        // empty anonymous object type; that relation is not ported.
        if s.intersects(TypeFlags::OBJECT) && t.intersects(TypeFlags::NON_PRIMITIVE) {
            return true;
        }
        // The **assignable-only** arms. Upstream guards this block with
        // `relation == c.assignableRelation || relation == c.comparableRelation`,
        // which is exactly why `any` on the *source* side is not in the block
        // above: `any -> string` is assignable but not a subtype, and folding
        // the two would make the subtype relation wrong the day it is ported.
        //
        // The enum arms in the same upstream block are not ported — there are no
        // enum types in this crate — so `number -> E` is a gap.
        if s.intersects(TypeFlags::ANY) {
            return true;
        }
        false
    }

    /// The composite arms, guarded by the depth cap and the cycle cache.
    ///
    /// Ported from `Checker.recursiveTypeRelatedTo` (`internal/checker/relater.go`),
    /// reduced to the union and intersection dispatch that
    /// `structuredTypeRelatedTo` performs before it reaches object types.
    fn recursive_type_related_to(&mut self, source: TypeId, target: TypeId) -> bool {
        if let Some(&cached) = self.results.get(&(source, target)) {
            return cached;
        }
        if self.depth >= MAX_DEPTH {
            // Upstream reports `Excessive_stack_depth_comparing_types_0_and_1`
            // and records the pair as failed. There are no diagnostics in this
            // crate (`bd tsr-5e7.6`), so the failure is silent — but it is a
            // failure, never a permissive `true`.
            return false;
        }
        // Park the pair as *assumed related* before recursing. This is what
        // terminates a co-recursive cycle; see the field docs on `results`.
        self.results.insert((source, target), true);
        self.depth += 1;
        let related = self.structured_type_related_to(source, target);
        self.depth -= 1;
        self.results.insert((source, target), related);
        related
    }

    /// Union and intersection dispatch.
    ///
    /// Ported from `Checker.structuredTypeRelatedTo`
    /// (`internal/checker/relater.go`), restricted to the four composite arms.
    ///
    /// The **order matters and is asserted by a test**: a source union is
    /// decomposed before a target union, so `("a" | "b") -> ("a" | "b" | "c")`
    /// succeeds by decomposing the source and finding each constituent on the
    /// right, rather than by asking whether the whole source union is one of the
    /// target's constituents — which it is not, since union interning makes
    /// `"a" | "b"` a type the target's list does not contain.
    fn structured_type_related_to(&mut self, source: TypeId, target: TypeId) -> bool {
        if let Some(constituents) = self.union_constituents(source) {
            // Every constituent of a source union must be related.
            // Upstream's `eachTypeRelatedToType`.
            return constituents.iter().all(|&c| self.is_related_to(c, target));
        }
        if let Some(constituents) = self.intersection_constituents(target) {
            // Related to every constituent of a target intersection.
            // Upstream's `typeRelatedToEachType`.
            return constituents.iter().all(|&c| self.is_related_to(source, c));
        }
        if let Some(constituents) = self.union_constituents(target) {
            // Related to *some* constituent of a target union.
            // Upstream's `typeRelatedToSomeType`.
            return constituents.iter().any(|&c| self.is_related_to(source, c));
        }
        if let Some(constituents) = self.intersection_constituents(source) {
            // *Some* constituent of a source intersection suffices.
            //
            // Upstream reaches this via `someTypeRelatedToType` and notes that
            // it is incomplete: `A & B` can be related to `T` through the
            // *combination* of its constituents' members without any single one
            // being related. That case needs the structural comparison this
            // module gaps, so it is a gap here for the same reason and not a
            // second one.
            return constituents.iter().any(|&c| self.is_related_to(c, target));
        }
        // Reached only by a type whose *flags* say union or intersection while
        // its data says otherwise, which `is_related_to`'s gate lets through.
        //
        // A pair of plain object types never arrives here at all: the gate in
        // `is_related_to` requires one side to be composite, so `I -> J` answers
        // `false` there. Measured, not assumed — mutating this `false` to `true`
        // left `two_structurally_identical_interfaces_are_a_gap` green, which is
        // how the real location of the gap was found.
        false
    }

    /// The constituents of `id`, if it is a union.
    ///
    /// The clone is what ADR-0013 buys: nothing may hold a borrow of the store
    /// across the recursive call that follows, and a constituent list is a
    /// handful of 4-byte handles.
    fn union_constituents(&self, id: TypeId) -> Option<Vec<TypeId>> {
        match &self.checker.type_of(id).data {
            TypeData::Union { types, .. } => Some(types.clone()),
            _ => None,
        }
    }

    /// The constituents of `id`, if it is an intersection. See
    /// [`Relater::union_constituents`] for why the list is cloned.
    fn intersection_constituents(&self, id: TypeId) -> Option<Vec<TypeId>> {
        match &self.checker.type_of(id).data {
            TypeData::Intersection { types, .. } => Some(types.clone()),
            _ => None,
        }
    }
}
