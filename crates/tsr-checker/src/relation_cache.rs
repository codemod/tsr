//! Checker-lifetime relation results: native `Relation.results`.
//!
//! Ported from `Relation` (`internal/checker/relater.go:99`, pinned tsgo
//! `5b1047d`), the per-relation `results map[CacheHashKey]RelationComparisonResult`
//! that `recursiveTypeRelatedTo` (`relater.go:3061`) reads before a structured
//! comparison and that it and `resetMaybeStack` (`relater.go:3169`) write.
//! Native keeps one `Relation` per relation kind on the `Checker`
//! (`assignableRelation`, `subtypeRelation`, `strictSubtypeRelation`,
//! `comparableRelation`, `identityRelation`), so a repeated
//! `(source, target, intersectionState)` pair is a lookup for the rest of
//! the program check rather than a new structural walk.
//!
//! The port convention record (key identity, owner, publication states,
//! receiver/alias context and the expensive work boundary) is
//! `docs/architecture/checker-relation-publication.md`, section
//! "Checker-lifetime results (tsr-2zk.902)".

use rustc_hash::FxHashMap;
use tsr_binder::SymbolId;

use crate::relater::Relation;
use crate::types::TypeId;

/// A relation result key: the ordered pair and whether it was related under
/// `IntersectionStateTarget` (native `getRelationKey`'s intersection-state
/// suffix, `checker.go:17613`). Native's generic-reference `'g'` encoding
/// makes more pairs equal; a `TypeId` pair is a strictly finer key, so it can
/// only miss a hit native takes, never share a result native keeps apart.
pub(crate) type RelationKey = (TypeId, TypeId, bool);

/// The completed states of native `RelationComparisonResult` this port
/// publishes. `Reported` is not represented. The
/// `ReportsUnmeasurable`/`ReportsUnreliable` bits travel beside the state as
/// a [`Reliability`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CachedRelation {
    /// `RelationComparisonResultSucceeded`.
    Succeeded,
    /// `RelationComparisonResultFailed`.
    Failed,
    /// `RelationComparisonResultFailed | RelationComparisonResultComplexityOverflow`:
    /// the top-level pair of a check whose relation-count budget ran out
    /// (`checkTypeRelatedToEx`, relater.go:373), so the overflowing walk is
    /// not attempted again.
    ComplexityOverflow,
    /// `RelationComparisonResultFailed | RelationComparisonResultStackDepthOverflow`:
    /// the top-level pair of a check whose source or target stack reached
    /// 100 entries (`recursiveTypeRelatedTo`, relater.go:3103; recorded by
    /// `checkTypeRelatedToEx`, :375). Its report is TS2321.
    StackDepthOverflow,
}

bitflags::bitflags! {
    /// Native `RelationComparisonResultReportsUnmeasurable` and
    /// `RelationComparisonResultReportsUnreliable` (`relater.go:70-71`): a
    /// variance measurement's comparison touched a marker type through a
    /// construct its variance digest cannot describe. Held as the checker's
    /// current `reliabilityFlags` ([`RelationResults::reliability`]), beside
    /// every published result, and per measured type parameter
    /// ([`RelationResults::variance_reliability`]), where they are native's
    /// `VarianceFlagsUnmeasurable`/`VarianceFlagsUnreliable`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub(crate) struct Reliability: u8 {
        /// `ReportsUnmeasurable` / `VarianceFlagsUnmeasurable`.
        const UNMEASURABLE = 1;
        /// `ReportsUnreliable` / `VarianceFlagsUnreliable`.
        const UNRELIABLE = 2;
    }
}

/// The compiler options a relation answer reads, directly in the relater or
/// through the member and optionality readers it calls:
/// `strictNullChecks`, `strictFunctionTypes`, `exactOptionalPropertyTypes`
/// and `noImplicitAny`.
pub(crate) type RelationOptions = [bool; 4];

/// One results map per relation kind, owned by the private `Checker` for its
/// lifetime.
#[derive(Default)]
pub(crate) struct RelationResults {
    /// The options every stored result was computed under. Native fixes them
    /// at checker construction; the port's `apply_compiler_options` and its
    /// tests can change them on a live checker, so a walk that starts under
    /// different options discards every result first ([`Self::validate`]).
    options: Option<RelationOptions>,
    assignable: FxHashMap<RelationKey, (CachedRelation, Reliability)>,
    subtype: FxHashMap<RelationKey, (CachedRelation, Reliability)>,
    strict_subtype: FxHashMap<RelationKey, (CachedRelation, Reliability)>,
    comparable: FxHashMap<RelationKey, (CachedRelation, Reliability)>,
    /// Native `Checker.reliabilityFlags` (`checker.go:736`): the reports
    /// collected by the comparison in progress. `recursiveTypeRelatedTo`
    /// scopes it per structured pair; `getVariancesWorker` reads it per type
    /// parameter. Not cleared by [`Self::validate`]: it describes the walk
    /// in progress, not stored results.
    pub(crate) reliability: Reliability,
    /// The `Unmeasurable`/`Unreliable` half of native's `VarianceFlags`, per
    /// measured symbol and type parameter, beside the masked
    /// `Checker::variance_cache` (`variances.rs`). Variances do not depend
    /// on the relation options [`Self::validate`] guards, so neither does
    /// this.
    pub(crate) variance_reliability: FxHashMap<SymbolId, Vec<Reliability>>,
    /// getRecursionIdentity's deferred type reference arm (relater.go): an
    /// instantiation of an alias written as a tuple or array type is a type
    /// reference whose identity is the alias's tuple/array node, shared by
    /// every instantiation. The port relates such an alias image as its
    /// evaluated body (an interned tuple or array type with no node), so the
    /// relater records the node here, keyed by that body, when it
    /// normalizes the image (`non_object_alias_image_body`). Read only by
    /// `relation_recursion_identity`. Not cleared by [`Self::validate`]: the
    /// node is a property of the body's construction, not of a relation.
    pub(crate) alias_reference_nodes: FxHashMap<TypeId, tsr_ast::NodeId>,
}

impl RelationResults {
    fn map(&self, relation: Relation) -> &FxHashMap<RelationKey, (CachedRelation, Reliability)> {
        match relation {
            Relation::Assignable => &self.assignable,
            Relation::Subtype => &self.subtype,
            Relation::StrictSubtype => &self.strict_subtype,
            Relation::Comparable => &self.comparable,
        }
    }

    fn map_mut(
        &mut self,
        relation: Relation,
    ) -> &mut FxHashMap<RelationKey, (CachedRelation, Reliability)> {
        match relation {
            Relation::Assignable => &mut self.assignable,
            Relation::Subtype => &mut self.subtype,
            Relation::StrictSubtype => &mut self.strict_subtype,
            Relation::Comparable => &mut self.comparable,
        }
    }

    /// `Relation.get` (`relater.go:103`).
    pub(crate) fn get(&self, relation: Relation, key: RelationKey) -> Option<CachedRelation> {
        self.map(relation).get(&key).map(|&(result, _)| result)
    }

    /// `Relation.get` (`relater.go:103`) with the entry's report bits.
    pub(crate) fn get_with_reliability(
        &self,
        relation: Relation,
        key: RelationKey,
    ) -> Option<(CachedRelation, Reliability)> {
        self.map(relation).get(&key).copied()
    }

    /// `Relation.set` (`relater.go:107`).
    pub(crate) fn set(
        &mut self,
        relation: Relation,
        key: RelationKey,
        result: CachedRelation,
        reliability: Reliability,
    ) {
        self.map_mut(relation).insert(key, (result, reliability));
    }

    /// Keep the stored results only if they were computed under `options`.
    pub(crate) fn validate(&mut self, options: RelationOptions) {
        if self.options != Some(options) {
            self.options = Some(options);
            self.assignable.clear();
            self.subtype.clear();
            self.strict_subtype.clear();
            self.comparable.clear();
        }
    }

    /// `relation.size()`: the completed results stored for `relation`.
    pub(crate) fn len(&self, relation: Relation) -> usize {
        self.map(relation).len()
    }
}
