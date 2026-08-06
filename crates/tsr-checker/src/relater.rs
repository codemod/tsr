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
//! - unions on either side, and intersections on either side;
//! - **structural comparison of object types**: every property of the target,
//!   own and inherited, must have a corresponding property of the source whose
//!   type is related to it.
//!
//! # What is deliberately *not* ported, and why it answers `false`
//!
//! **Structural comparison of object types is ported** — see
//! [`Relater::properties_related_to`]. What is *not* ported, and each answers
//! "not related" rather than guessing:
//!
//! - **optionality.** A target property the source lacks always fails, so
//!   `{ x: string } -> { x: string, y?: number }` is a gap. Reading
//!   `SymbolFlags::OPTIONAL` is the fix, and it belongs with the property
//!   modifiers below rather than on its own.
//! - **a base type this port cannot follow.** `base_symbols_of`
//!   (`crate::members`) answers `None` for a base with type arguments, a
//!   non-identifier base, or a base with no members. A target whose inherited
//!   requirements cannot be *enumerated* must not be satisfied by checking only
//!   the ones that can, so the whole comparison fails.
//! - **signatures and index signatures.** A function type or a `[k: string]:`
//!   member contributes nothing, so two anonymous function types still relate
//!   only when they are the same [`TypeId`].
//! - **variance markers on type references**, so `Box<Dog> -> Box<Animal>` is
//!   not decided by variance.
//!
//! One place is too *permissive*, and it is named rather than buried:
//! `readonly`, `private`/`protected` identity and the property-vs-method
//! distinction are not compared, so a source differing only in one of those
//! relates here and would not upstream. The property **names** and **types**
//! are fully checked; the modifiers on them are not.
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
//! # The recursion limits are exercised, and that was measured
//!
//! `bd tsr-el3.2` records that this project has twice *deliberately* skipped an
//! algorithmic limit because the loop it guards did not exist in the ported
//! subset. This module was the third case and is no longer one: while structural
//! comparison was absent the guards were **unexercised**, because the only
//! cycles in a type graph run through an object type's members and the walk
//! stopped at object types. Structural comparison creates that loop, and both
//! guards now bite. Measured, per guard, not asserted:
//!
//! - a **depth cap** ([`MAX_DEPTH`]), from upstream's `isDeeplyNestedType`.
//!   Raising it to `10_000` and running
//!   `tests/relater.rs::a_chain_deeper_than_the_cap_gives_up` — a 110-link chain
//!   of interfaces — **aborts the process with a stack overflow**. The cap is
//!   load-bearing for safety, not only for answers.
//! - a **relation cache** keyed on the type pair, from upstream's `Relation`
//!   results map, which both memoises and — by parking an in-progress pair as
//!   *assumed related* — closes the co-recursive cycle the way
//!   `recursiveTypeRelatedTo` does with `RelationComparisonResultReported`.
//!   Deleting the park makes
//!   `tests/relater.rs::mutually_recursive_interfaces_terminate` (`interface A
//!   { x: B }` / `interface B { x: A }`) answer `false` instead of `true`. It
//!   still *terminates*, in milliseconds, because the depth cap catches what the
//!   cache no longer closes — so the two guards are not interchangeable: the
//!   cache buys the answer, the cap buys termination.
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

/// The answer to a relation question, including *"I could not tell"*.
///
/// **This has no upstream counterpart, and that is the point.** Upstream's
/// `checkTypeRelatedTo` returns a `Ternary` too (`internal/checker/relater.go`),
/// but its third value is `TernaryMaybe`, which means *"assumed related while a
/// cycle is open"* — an internal bookkeeping value, not an admission of
/// ignorance. Upstream never needs one, because every arm this port omits is
/// implemented there.
///
/// Here the omissions are real, and six of them answer "not related" while
/// meaning "not computed" — enumerated in `docs/architecture/checker-notes-assign.md`
/// §2. [`Unknown`](Ternary::Unknown) is what those six answer instead. The
/// public [`Checker::is_type_assignable_to`] maps it back to `false`, so every
/// existing caller is unaffected; a caller that acts on a **negative** can
/// instead ask [`Checker::relate_ternary`] and refuse the pair it cannot decide.
///
/// The composition rules are Kleene's, not Go's: see [`Ternary::all`] and
/// [`Ternary::any`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ternary {
    /// The relation holds.
    Related,
    /// The relation does not hold, and this port is entitled to say so.
    NotRelated,
    /// This port cannot decide the pair. Never a licence to assume either way.
    Unknown,
}

impl Ternary {
    /// Kleene conjunction over a sequence: `Related` only if every element is,
    /// `NotRelated` if any element is, `Unknown` otherwise.
    ///
    /// The short circuit is on `NotRelated` and **not** on `Unknown`: an
    /// `Unknown` early in the sequence must not mask a `NotRelated` later in
    /// it, because a definite negative is a strictly better answer than "could
    /// not tell" and this port would otherwise refuse pairs it can decide.
    fn all(parts: impl IntoIterator<Item = Ternary>) -> Ternary {
        let mut unknown = false;
        for part in parts {
            match part {
                Ternary::NotRelated => return Ternary::NotRelated,
                Ternary::Unknown => unknown = true,
                Ternary::Related => {}
            }
        }
        if unknown { Ternary::Unknown } else { Ternary::Related }
    }

    /// Kleene disjunction over a sequence: `Related` if any element is,
    /// `NotRelated` only if every element is, `Unknown` otherwise. The dual of
    /// [`Ternary::all`], short-circuiting on `Related` for the same reason.
    fn any(parts: impl IntoIterator<Item = Ternary>) -> Ternary {
        let mut unknown = false;
        for part in parts {
            match part {
                Ternary::Related => return Ternary::Related,
                Ternary::Unknown => unknown = true,
                Ternary::NotRelated => {}
            }
        }
        if unknown { Ternary::Unknown } else { Ternary::NotRelated }
    }
}

/// The flag domain on which [`Relater::is_simple_type_related_to`] is a
/// *complete* decision procedure, so that its failure to fire is an answer.
///
/// Pinned to upstream's `isSimpleTypeRelatedTo` (`internal/checker/relater.go`)
/// rather than to a summary of it: a flag belongs here when every upstream arm
/// mentioning it is ported. The four upstream arms this port omits — `EnumLike`
/// source against an enum target, `UniqueESSymbol`, the wildcard type, and the
/// enum-literal pairings — are exactly why [`TypeFlags::ENUM`],
/// [`TypeFlags::ENUM_LITERAL`] and [`TypeFlags::UNIQUE_ES_SYMBOL`] are
/// **absent** from this set despite being primitives: for them a non-firing
/// simple arm means "unported", not "unrelated".
///
/// This is a strict superset of [`crate::calls`]'s `SELECTABLE`, which
/// additionally excludes [`TypeFlags::NON_PRIMITIVE`]. Widening a caller from
/// one to the other is a separate decision from making the relation ternary,
/// and is not taken here.
const FLAG_DECIDABLE: TypeFlags = TypeFlags::ANY
    .union(TypeFlags::UNKNOWN)
    .union(TypeFlags::UNDEFINED)
    .union(TypeFlags::NULL)
    .union(TypeFlags::VOID)
    .union(TypeFlags::STRING)
    .union(TypeFlags::NUMBER)
    .union(TypeFlags::BIG_INT)
    .union(TypeFlags::BOOLEAN)
    .union(TypeFlags::ES_SYMBOL)
    .union(TypeFlags::STRING_LITERAL)
    .union(TypeFlags::NUMBER_LITERAL)
    .union(TypeFlags::BIG_INT_LITERAL)
    .union(TypeFlags::BOOLEAN_LITERAL)
    .union(TypeFlags::NEVER)
    .union(TypeFlags::NON_PRIMITIVE);

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
    /// Upstream's `c.subtypeRelation`. Added for `narrowTypeByTypeFacts`
    /// (`bd tsr-q9g`); the simple arms carry upstream's three relation tests,
    /// and the structural walk is shared with `Assignable` — a stated
    /// divergence (`checker-notes-narrow.md` §6), reachable only for
    /// structured constituents the narrowing shapes do not produce.
    Subtype,
    /// Upstream's `c.strictSubtypeRelation` — the relation
    /// `narrowTypeByTypeFacts` filters constituents with. Differs from
    /// [`Relation::Subtype`] in the simple arms only (an `any` source does not
    /// relate to `unknown`, and an object source's relation to `object` is
    /// freshness-gated upstream).
    StrictSubtype,
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
    /// An entry is written **before** the recursive walk with the value
    /// [`Ternary::Related`], which is what closes a cycle: re-entering the same
    /// pair assumes the relation holds, exactly as upstream's
    /// `recursiveTypeRelatedTo` does when it finds the pair already on the
    /// stack. The assumption is discharged by the surrounding walk failing if
    /// any *other* constituent fails.
    ///
    /// **The park stays `Related` and not `Unknown`.** Parking `Unknown` would
    /// be the conservative-looking choice and it is the wrong one: every
    /// mutually recursive interface pair — `interface A { x: B }` /
    /// `interface B { x: A }`, which `tests/relater.rs` asserts — would then
    /// answer `Unknown` rather than `Related`, turning upstream's termination
    /// device into a mass refusal. The cycle assumption is a *proof technique*
    /// (co-induction), not an inability to compute.
    results: FxHashMap<(TypeId, TypeId), Ternary>,
    depth: usize,
}

impl Checker<'_, '_> {
    /// Whether `source` is assignable to `target`.
    ///
    /// Ported from `Checker.isTypeAssignableTo` (`internal/checker/relater.go`).
    ///
    /// Read the module docs before trusting a `false`. Object types **are**
    /// compared structurally ([`Relater::properties_related_to`]), but the
    /// comparison has no way to say "I could not tell": an unfollowable base, an
    /// absent property whose target counterpart may be optional, a signature or
    /// index signature, and the depth cap all answer `false`. A caller that acts
    /// on a negative must therefore restrict itself to a domain where `false` is
    /// decidable — [`crate::calls`]'s `SELECTABLE` is that restriction, and
    /// `docs/architecture/checker-notes-assign.md` is why it has not been
    /// widened.
    #[must_use]
    pub fn is_type_assignable_to(&mut self, source: TypeId, target: TypeId) -> bool {
        self.is_type_related_to(source, target, Relation::Assignable)
    }

    /// Whether `source` is a subtype of `target`.
    ///
    /// Ported from `Checker.isTypeSubtypeOf` (`internal/checker/relater.go`);
    /// the caller this exists for is `narrowTypeByTypeFacts` (`flow.go:687`).
    #[must_use]
    pub fn is_type_subtype_of(&mut self, source: TypeId, target: TypeId) -> bool {
        self.is_type_related_to(source, target, Relation::Subtype)
    }

    /// Whether `source` is a strict subtype of `target` — the constituent
    /// filter `narrowTypeByTypeFacts` runs.
    #[must_use]
    pub fn is_type_strict_subtype_of(&mut self, source: TypeId, target: TypeId) -> bool {
        self.is_type_related_to(source, target, Relation::StrictSubtype)
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
        self.relate_ternary(source, target, relation) == Ternary::Related
    }

    /// The same walk as [`Checker::is_type_related_to`], without collapsing
    /// *"not related"* and *"could not tell"* into one `false`.
    ///
    /// There is no upstream counterpart — see [`Ternary`] for why this port
    /// needs a distinction upstream does not. `is_type_related_to` is defined in
    /// terms of this function rather than beside it, so the two can never drift:
    /// there is one walk, and the binary entry point is a projection of it.
    ///
    /// The intended caller is one that acts on a **negative** — overload
    /// selection, which promotes the next candidate when a parameter does not
    /// accept an argument. Such a caller must refuse an [`Ternary::Unknown`]
    /// pair rather than treat it as a rejection, because a wrong rejection here
    /// does not degrade to a gap: it silently selects a different overload and
    /// yields a confident wrong answer.
    #[must_use]
    pub fn relate_ternary(
        &mut self,
        source: TypeId,
        target: TypeId,
        relation: Relation,
    ) -> Ternary {
        let mut relater =
            Relater { checker: self, relation, results: FxHashMap::default(), depth: 0 };
        relater.is_related_to(source, target)
    }
}

impl Relater<'_, '_, '_> {
    /// The body of `isTypeRelatedTo`, minus the entry-point bookkeeping.
    ///
    /// Ported from `Checker.isTypeRelatedTo` (`internal/checker/relater.go`).
    fn is_related_to(&mut self, source: TypeId, target: TypeId) -> Ternary {
        // Upstream reduces a fresh literal to its regular form on both sides
        // before comparing identity, so that `"a"` fresh and `"a"` regular are
        // one type here even though they are two interned types.
        let source = self.checker.get_regular_type_of_literal_type(source);
        let target = self.checker.get_regular_type_of_literal_type(target);
        if source == target {
            return Ternary::Related;
        }
        match self.is_simple_type_related_to(source, target) {
            Some(true) => return Ternary::Related,
            Some(false) => return Ternary::NotRelated,
            None => {}
        }
        let composite = TypeFlags::UNION.union(TypeFlags::INTERSECTION);
        let s = self.checker.type_of(source).flags;
        let t = self.checker.type_of(target).flags;
        // Two object types with members reach the structural arm; upstream's
        // gate is `source.flags&TypeFlags::StructuredOrInstantiable != 0 &&
        // target.flags&...`, and `Named { members: Some(_) }` is the whole of
        // this port's "structured".
        if s.intersects(composite)
            || t.intersects(composite)
            || (self.has_members(source) && self.has_members(target))
        {
            return self.recursive_type_related_to(source, target);
        }
        // Nothing fired. That is an **answer** only where the simple arms above
        // are a complete decision procedure for both sides — `string -> number`
        // is genuinely not related. Where either side carries a flag this port
        // has no arm for (an enum, a `unique symbol`, a type parameter, a
        // conditional), or is an object type that never reached the structural
        // arm because it has no members table (a function type, an
        // index-signature-only type), the same fallthrough means *not
        // computed*. Rows 3 and 6 of `checker-notes-assign.md` §2.
        if self.flag_decidable(source) && self.flag_decidable(target) {
            Ternary::NotRelated
        } else {
            Ternary::Unknown
        }
    }

    /// Whether `id` is an object type with a members table to compare.
    fn has_members(&self, id: TypeId) -> bool {
        matches!(&self.checker.type_of(id).data, TypeData::Named { members: Some(_), .. })
    }

    /// Whether a non-firing [`Relater::is_simple_type_related_to`] is an answer
    /// about `id`. See [`FLAG_DECIDABLE`].
    fn flag_decidable(&self, id: TypeId) -> bool {
        let flags = self.checker.type_of(id).flags;
        !flags.is_empty() && FLAG_DECIDABLE.contains(flags)
    }

    /// Whether `id` carries call, construct or index signatures that this
    /// module's structural comparison does not look at.
    ///
    /// This is row 6 of `checker-notes-assign.md` §2, and it is the load-bearing
    /// one: for a signature-bearing pair the comparison is unsound in **both**
    /// directions at once — a missing rejection (the signatures are never
    /// compared, so two differently-callable types can relate) and a missing
    /// acceptance (a bare `{}` target is satisfied without them). Neither
    /// direction is recoverable from the property walk, so the pair is not
    /// decided at all.
    ///
    /// Two sources, because signatures reach a type by two routes in this port:
    /// [`Checker::signatures_of_type`] for a baked function-shaped type, and the
    /// members symbol's own declarations for an interface or type literal that
    /// writes a signature member.
    fn signature_bearing(&self, id: TypeId) -> bool {
        if self.checker.signatures_of_type(id).is_some_and(|signatures| !signatures.is_empty()) {
            return true;
        }
        let TypeData::Named { members: Some(owner), .. } = self.checker.type_of(id).data else {
            return false;
        };
        self.checker.binder.symbols().get(owner).declarations.iter().any(|&declaration| {
            let members = match self.checker.node_map.get(declaration) {
                Some(tsr_ast::Node::InterfaceDeclaration(node)) => node.members,
                Some(tsr_ast::Node::TypeLiteralNode(node)) => node.members,
                _ => return false,
            };
            members.iter().any(|member| {
                matches!(
                    member,
                    tsr_ast::TypeElement::CallSignatureDeclaration(_)
                        | tsr_ast::TypeElement::ConstructSignatureDeclaration(_)
                        | tsr_ast::TypeElement::IndexSignatureDeclaration(_)
                )
            })
        })
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
    ///
    /// # The return is `Option<bool>`, and the `Some(false)` is one arm
    ///
    /// `None` means *no arm fired*, which on its own decides nothing — the
    /// caller then tries the composite arms and, failing those, asks
    /// [`Relater::flag_decidable`] whether the silence was an answer. The single
    /// `Some(false)` is upstream's third arm, `target.flags&TypeFlags::Never`:
    /// nothing but `never` is assignable to `never`, and upstream returns there
    /// rather than falling through, so it is a **decision** and not an absence.
    /// Collapsing it into `None` would make every `X -> never` pair `Unknown`
    /// whenever `X` is an object type.
    fn is_simple_type_related_to(&mut self, source: TypeId, target: TypeId) -> Option<bool> {
        let s = self.checker.type_of(source).flags;
        let t = self.checker.type_of(target).flags;
        // `any` on the right and `never` on the left relate to everything.
        // `errorType` is `ANY` here, which is upstream's behaviour too: an
        // erroneous type must not cascade a second error.
        if t.intersects(TypeFlags::ANY) || s.intersects(TypeFlags::NEVER) {
            return Some(true);
        }
        // Upstream excludes `strictSubtypeRelation` with an `any` source here
        // (`relater.go:212`) — `any` is assignable to `unknown` but not its
        // strict subtype, which is what keeps an `any` constituent from
        // surviving a `typeof` filter it should not survive.
        if t.intersects(TypeFlags::UNKNOWN)
            && !(matches!(self.relation, Relation::StrictSubtype) && s.intersects(TypeFlags::ANY))
        {
            return Some(true);
        }
        if t.intersects(TypeFlags::NEVER) {
            return Some(false);
        }
        if s.intersects(TypeFlags::STRING_LIKE) && t.intersects(TypeFlags::STRING) {
            return Some(true);
        }
        if s.intersects(TypeFlags::NUMBER_LIKE) && t.intersects(TypeFlags::NUMBER) {
            return Some(true);
        }
        if s.intersects(TypeFlags::BIG_INT_LIKE) && t.intersects(TypeFlags::BIG_INT) {
            return Some(true);
        }
        if s.intersects(TypeFlags::BOOLEAN_LIKE) && t.intersects(TypeFlags::BOOLEAN) {
            return Some(true);
        }
        if s.intersects(TypeFlags::ES_SYMBOL_LIKE) && t.intersects(TypeFlags::ES_SYMBOL) {
            return Some(true);
        }
        // The two `strictNullChecks`-off arms are collapsed to their strict
        // reading; see the module docs for why the permissive one is not the
        // safe default to guess.
        if s.intersects(TypeFlags::UNDEFINED)
            && t.intersects(TypeFlags::UNDEFINED.union(TypeFlags::VOID))
        {
            return Some(true);
        }
        if s.intersects(TypeFlags::NULL) && t.intersects(TypeFlags::NULL) {
            return Some(true);
        }
        // Upstream guards this with a `strictSubtypeRelation` exception for a
        // stale empty anonymous object type (`relater.go:258`). This port has
        // no object freshness and no `IsEmptyAnonymousObjectType`, so the
        // exception is a stated divergence rather than an arm: `{}` relates to
        // `object` under every relation here, which upstream denies only for
        // `StrictSubtype` on that one shape.
        if s.intersects(TypeFlags::OBJECT) && t.intersects(TypeFlags::NON_PRIMITIVE) {
            return Some(true);
        }
        // The **assignable-only** arms (`relater.go:261`, `relation ==
        // assignable || relation == comparable`). This gate is the whole
        // reason `Subtype`/`StrictSubtype` exist as distinct variants:
        // `any -> string` is assignable but not a subtype, and
        // `narrowTypeByTypeFacts` counts on the difference to leave an `any`
        // constituent alone.
        //
        // The enum arms in the same upstream block are not ported — there are no
        // enum types in this crate — so `number -> E` is a gap.
        if matches!(self.relation, Relation::Assignable) && s.intersects(TypeFlags::ANY) {
            return Some(true);
        }
        None
    }

    /// The composite arms, guarded by the depth cap and the cycle cache.
    ///
    /// Ported from `Checker.recursiveTypeRelatedTo` (`internal/checker/relater.go`),
    /// reduced to the union and intersection dispatch that
    /// `structuredTypeRelatedTo` performs before it reaches object types.
    fn recursive_type_related_to(&mut self, source: TypeId, target: TypeId) -> Ternary {
        if let Some(&cached) = self.results.get(&(source, target)) {
            return cached;
        }
        if self.depth >= MAX_DEPTH {
            // Upstream reports `Excessive_stack_depth_comparing_types_0_and_1`
            // and records the pair as failed. There are no diagnostics in this
            // crate (`bd tsr-5e7.6`), so the failure is silent — and it was a
            // *failure*, never a permissive `true`. It is now `Unknown`: giving
            // up at a depth cap is the plainest case of "not computed" on this
            // page, and reporting it as a rejection is what row 4 of
            // `checker-notes-assign.md` §2 objects to.
            return Ternary::Unknown;
        }
        // Park the pair as *assumed related* before recursing. This is what
        // terminates a co-recursive cycle; see the field docs on `results`.
        self.results.insert((source, target), Ternary::Related);
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
    fn structured_type_related_to(&mut self, source: TypeId, target: TypeId) -> Ternary {
        if let Some(constituents) = self.union_constituents(source) {
            // Every constituent of a source union must be related.
            // Upstream's `eachTypeRelatedToType`.
            let parts: Vec<_> =
                constituents.iter().map(|&c| self.is_related_to(c, target)).collect();
            return Ternary::all(parts);
        }
        if let Some(constituents) = self.intersection_constituents(target) {
            // Related to every constituent of a target intersection.
            // Upstream's `typeRelatedToEachType`.
            let parts: Vec<_> =
                constituents.iter().map(|&c| self.is_related_to(source, c)).collect();
            return Ternary::all(parts);
        }
        if let Some(constituents) = self.union_constituents(target) {
            // Related to *some* constituent of a target union.
            // Upstream's `typeRelatedToSomeType`.
            let parts: Vec<_> =
                constituents.iter().map(|&c| self.is_related_to(source, c)).collect();
            return Ternary::any(parts);
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
            let parts: Vec<_> =
                constituents.iter().map(|&c| self.is_related_to(c, target)).collect();
            return Ternary::any(parts);
        }
        if self.has_members(source) && self.has_members(target) {
            // Row 6 of `checker-notes-assign.md` §2, checked **before** the
            // property walk rather than inside it: a signature-bearing pair is
            // not decided at all, and letting it reach `properties_related_to`
            // would produce a confident answer from a comparison that ignored
            // the members that distinguish the two types.
            if self.signature_bearing(source) || self.signature_bearing(target) {
                return Ternary::Unknown;
            }
            return self.properties_related_to(source, target);
        }
        // Reached only by a type whose *flags* say union or intersection while
        // its data says otherwise, which `is_related_to`'s gate lets through.
        // Nothing was compared, so nothing was decided.
        Ternary::Unknown
    }

    /// Every property of `target` has a corresponding property of `source`, and
    /// the two are related.
    ///
    /// Ported from `Checker.propertiesRelatedTo` (`internal/checker/relater.go`),
    /// reduced to the arm that walks `getPropertiesOfType(target)` and asks
    /// `getPropertyOfType(source, name)` for each. Property *types* are related
    /// covariantly, which is upstream's rule for the assignable relation on
    /// non-method properties.
    ///
    /// # What is missing answers `false`, never `true`
    ///
    /// - **Optionality is not read.** Upstream skips a target property whose
    ///   source counterpart is absent when the target property is optional
    ///   (`SymbolFlags::Optional`). Here a missing property is always a failure,
    ///   so `{ x: string } -> { x: string, y?: number }` is a gap rather than a
    ///   wrong `true`.
    /// - **`readonly`, variance markers, private/protected identity, index
    ///   signatures and call/construct signatures** are not compared at all.
    ///   Each is a *missing rejection* — a source that differs only in one of
    ///   them relates here and would not upstream. That is the one place this
    ///   function can be too permissive, and it is bounded to those modifiers:
    ///   the property *names* and *types* are fully checked.
    /// - **A base type this port cannot follow** ([`Checker::base_symbols_of`]
    ///   answering `None`) makes the whole comparison `false`, because a target
    ///   whose inherited requirements cannot be enumerated must not be satisfied
    ///   by checking only the ones that can.
    fn properties_related_to(&mut self, source: TypeId, target: TypeId) -> Ternary {
        let Some(names) = self.property_names_of(target) else {
            // Row 1 of `checker-notes-assign.md` §2: the target's inherited
            // requirements could not be *enumerated*, so no verdict about them
            // is available in either direction.
            return Ternary::Unknown;
        };
        let mut parts = Vec::with_capacity(names.len());
        for name in names {
            // Through [`Checker::get_type_of_property_of_type`], not
            // `get_property_of_type` + `get_type_of_symbol`. The symbol is the
            // *uninstantiated* declaration, so on a `C<number>` with a member
            // declared `a: T` this comparison would run against `T`. **This is
            // the site where that matters most**, because it is the one
            // consumer that acts on a `false`: a wrong member type here does
            // not degrade to a gap, it promotes the next overload candidate and
            // yields a confident wrong type (`docs/conventions.md`, "A
            // conservative `false` is safe for one kind of consumer and unsafe
            // for the other"). Answers identically today; `bd tsr-4qx`.
            //
            // `None` still means *no such property* — a property that exists
            // and does not type answers `Some(errorType)` — so the existence
            // test below is unchanged.
            let (Some(target_type), Some(source_type)) = (
                self.checker.get_type_of_property_of_type(target, &name),
                self.checker.get_type_of_property_of_type(source, &name),
            ) else {
                // Row 2 of `checker-notes-assign.md` §2. A target property with
                // no source counterpart is a rejection *only if the target
                // property is required*, and optionality is not read here — so
                // `{ x } -> { x, y?: number }` must not be reported as a
                // rejection. A target property whose own type does not compute
                // is row 2's twin: the requirement itself is unknown.
                parts.push(Ternary::Unknown);
                continue;
            };
            // Row 5 of `checker-notes-assign.md` §2 (`bd tsr-4qx`): the member
            // read may be the *uninstantiated* declaration, so on a `C<number>`
            // with a member declared `a: T` this comparison would run against
            // `T` itself. A type parameter surviving into a property type is the
            // observable signature of that, and it is not something to decide on.
            let unresolved = TypeFlags::TYPE_PARAMETER;
            if self.checker.type_of(target_type).flags.intersects(unresolved)
                || self.checker.type_of(source_type).flags.intersects(unresolved)
            {
                parts.push(Ternary::Unknown);
                continue;
            }
            parts.push(self.is_related_to(source_type, target_type));
        }
        Ternary::all(parts)
    }

    /// The names of every property of `id`, own and inherited, or `None` if any
    /// base type could not be followed.
    ///
    /// Ported from `Checker.getPropertiesOfType` → `getPropertiesOfObjectType`
    /// (`internal/checker/checker.go`). Upstream reads a resolved members table
    /// that already has the base types layered in; there is none here, so this
    /// walks the same base-symbol graph [`Checker::get_property_of_declared_symbol`]
    /// walks and collects names instead of resolving one. Names only: the
    /// *symbol* for a name is then taken from
    /// [`Checker::get_property_of_type`], so shadowing is decided in exactly one
    /// place rather than twice.
    ///
    /// The `None`-on-an-unfollowable-base rule is [`Checker::base_symbols_of`]'s
    /// and is why the walk cannot silently under-report a requirement.
    fn property_names_of(&mut self, id: TypeId) -> Option<Vec<String>> {
        let TypeData::Named { members: Some(owner), .. } = self.checker.type_of(id).data else {
            return None;
        };
        let mut names = Vec::new();
        let mut visiting = Vec::new();
        self.collect_property_names(owner, &mut names, &mut visiting).then_some(names)
    }

    /// One step of [`Relater::property_names_of`]'s walk.
    ///
    /// The `visiting` guard is [`Checker::get_property_of_declared_symbol`]'s,
    /// for the same reason: `class A extends B` with `class B extends A` is a
    /// real cycle in the base-type graph. Re-entry contributes nothing rather
    /// than failing — every name reachable through the cycle has already been
    /// collected by the outer visit.
    fn collect_property_names(
        &mut self,
        owner: tsr_binder::SymbolId,
        names: &mut Vec<String>,
        visiting: &mut Vec<tsr_binder::SymbolId>,
    ) -> bool {
        if visiting.contains(&owner) {
            return true;
        }
        visiting.push(owner);
        // A members table also holds type parameters, so the value gate is the
        // same one `getPropertyOfType` applies; without it `interface I<T>`
        // would demand a property named `T`.
        let own: Vec<String> = self
            .checker
            .binder
            .symbols()
            .get(owner)
            .members
            .iter()
            .filter(|&(_, &symbol)| self.checker.symbol_is_value(symbol))
            .map(|(&name, _)| name.to_owned())
            .collect();
        for name in own {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        let Some(bases) = self.checker.base_symbols_of(owner) else {
            return false;
        };
        bases.into_iter().all(|base| self.collect_property_names(base, names, visiting))
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
