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
//! `isSimpleTypeRelatedTo` reads [`Checker::strict_null_checks`]. In non-strict
//! mode, `undefined` and `null` relate to every non-union/intersection target
//! except `never`; composite targets continue through the structured walk.
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
/// mentioning it is ported. The upstream arms this port omits —
/// `UniqueESSymbol`, the wildcard type — are why [`TypeFlags::UNIQUE_ES_SYMBOL`]
/// is **absent** from this set despite being a primitive: for it a non-firing
/// simple arm means "unported", not "unrelated". [`TypeFlags::ENUM`] — an enum
/// MEMBER in this port's model — joined the set at §751, when every upstream
/// enum arm (`relater.go:219`/`:225`/`:236-243`/`:266-270`) was ported through
/// [`Checker::enum_member_value`]; the one arm left undecided (two same-named
/// enums from different declarations, `isEnumTypeRelatedTo`) answers `None`
/// explicitly. [`TypeFlags::ENUM_LITERAL`] marks the enum's UNION type, which
/// the composite dispatch decomposes before any simple arm is asked of it.
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
    .union(TypeFlags::ENUM)
    .union(TypeFlags::BIG_INT_LITERAL)
    .union(TypeFlags::BOOLEAN_LITERAL)
    .union(TypeFlags::NEVER)
    .union(TypeFlags::NON_PRIMITIVE);

/// **Measurement only** — which of the sites in
/// `docs/architecture/checker-notes-assign.md` §2 produced an
/// [`Ternary::Unknown`].
///
/// `calls.rs`'s `undecidable_pair` counter says *that* a pair was refused; it
/// cannot say *which* of the six sites refused it, and that split is what
/// decides which one is worth porting next. This module is the split. It is
/// modelled on [`crate::calls::counters`]: off unless [`reasons::enable`] has
/// been called, relaxed atomics when on, and **no arm classifies differently**
/// with it enabled.
///
/// # Attributing a mask to a caller
///
/// One top-level [`Checker::relate_ternary`] walk can fire several sites — the
/// Kleene combinators collect parts — so a walk's reason is a **set**, recorded
/// as a bitmask. [`reasons::last_unknown`] holds the mask of the most recent
/// top-level walk that *returned* `Unknown`; a caller that refuses a pair (as
/// `choose_overload` does, returning immediately) reads it straight after.
///
/// Re-entrancy is handled by saving and restoring the in-flight mask around the
/// walk, so a nested `relate_ternary` reached through
/// `get_type_of_property_of_type` cannot erase its caller's accumulated set.
pub mod reasons {
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

    /// A site that answers [`super::Ternary::Unknown`], numbered by the row of
    /// `checker-notes-assign.md` §2 it implements where there is one.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[repr(u8)]
    pub enum Site {
        /// Row 1 — `property_names_of` answered `None`: a base type this port
        /// cannot follow, so the target's inherited requirements are unknown.
        UnfollowableBase = 0,
        /// Row 2 — a target property with no source counterpart (which may be
        /// optional upstream), or whose own type does not compute.
        AbsentProperty = 1,
        /// Row 3 — an object type that never reached the structural arm because
        /// it has no members table: a function type, an index-signature-only
        /// type, `typeof C`.
        NoMembersTable = 2,
        /// Row 4 — [`super::MAX_DEPTH`].
        DepthCap = 3,
        /// Row 5 — a member type that is still a type parameter (`bd tsr-4qx`).
        GenericMember = 4,
        /// Row 6 — either side carries call, construct or index signatures.
        SignatureBearing = 5,
        /// **Not one of the six.** A side carries a flag with no simple arm —
        /// an enum, a `unique symbol`, a type parameter, a conditional. The
        /// flag histogram says which; see [`flag_histogram`].
        UnportedFlag = 6,
        /// **Not one of the six.** A type whose flags say union or intersection
        /// while its data says otherwise; nothing was compared.
        CompositeShape = 7,
    }

    /// How many sites there are.
    pub const SITES: usize = 8;

    static ON: AtomicBool = AtomicBool::new(false);
    #[allow(clippy::declare_interior_mutable_const)]
    const ZERO: AtomicU64 = AtomicU64::new(0);
    static TOTALS: [AtomicU64; SITES] = [ZERO; SITES];
    static FLAGS: [AtomicU64; 32] = [ZERO; 32];
    static CURRENT: AtomicU32 = AtomicU32::new(0);
    static LAST_UNKNOWN: AtomicU32 = AtomicU32::new(0);
    static UNKNOWN_WALKS: AtomicU64 = AtomicU64::new(0);

    /// Turn counting on for this process.
    pub fn enable() {
        ON.store(true, Ordering::Relaxed);
    }

    /// Whether counting is on.
    #[must_use]
    pub fn enabled() -> bool {
        ON.load(Ordering::Relaxed)
    }

    /// Record that `site` fired in the walk in flight.
    pub(crate) fn note(site: Site) {
        if !enabled() {
            return;
        }
        TOTALS[site as usize].fetch_add(1, Ordering::Relaxed);
        CURRENT.fetch_or(1 << (site as u32), Ordering::Relaxed);
    }

    /// Record the raw [`super::TypeFlags`] bits of a side that
    /// [`Site::UnportedFlag`] refused.
    pub(crate) fn note_flags(bits: u32) {
        if !enabled() {
            return;
        }
        for (bit, slot) in FLAGS.iter().enumerate() {
            if bits & (1 << bit) != 0 {
                slot.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Start a top-level walk, returning the mask to restore.
    pub(crate) fn begin() -> u32 {
        if !enabled() {
            return 0;
        }
        CURRENT.swap(0, Ordering::Relaxed)
    }

    /// End a top-level walk that answered `unknown`, restoring `outer`.
    pub(crate) fn finish(outer: u32, unknown: bool) {
        if !enabled() {
            return;
        }
        let mine = CURRENT.swap(outer | CURRENT.load(Ordering::Relaxed), Ordering::Relaxed);
        if unknown {
            LAST_UNKNOWN.store(mine, Ordering::Relaxed);
            UNKNOWN_WALKS.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// The mask of the last top-level walk that returned `Unknown`.
    #[must_use]
    pub fn last_unknown() -> u32 {
        LAST_UNKNOWN.load(Ordering::Relaxed)
    }

    /// How many top-level walks have returned `Unknown` — a control: a caller
    /// attributing [`last_unknown`] to itself wants to know that exactly one
    /// walk went `Unknown` while it ran.
    #[must_use]
    pub fn unknown_walks() -> u64 {
        UNKNOWN_WALKS.load(Ordering::Relaxed)
    }

    /// Every site's firing count, in [`Site`] order.
    #[must_use]
    pub fn totals() -> [u64; SITES] {
        std::array::from_fn(|index| TOTALS[index].load(Ordering::Relaxed))
    }

    /// How often each `TypeFlags` bit appeared on a side refused by
    /// [`Site::UnportedFlag`].
    #[must_use]
    pub fn flag_histogram() -> [u64; 32] {
        std::array::from_fn(|index| FLAGS[index].load(Ordering::Relaxed))
    }

    /// The labels for [`totals`], in the same order.
    #[must_use]
    pub fn labels() -> [&'static str; SITES] {
        [
            "row 1  unfollowable base",
            "row 2  absent / uncomputed property",
            "row 3  no members table",
            "row 4  depth cap",
            "row 5  generic member type",
            "row 6  signature-bearing",
            "  --   unported flag (not one of the six)",
            "  --   composite shape mismatch (not one of the six)",
        ]
    }
}

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
    /// Upstream's `c.comparableRelation` — what `narrowTypeByDiscriminant`
    /// (`flow.go:746`) and `narrowTypeByEquality` (`flow.go:589`) filter
    /// with. §750 (`checker-notes-callres.md`). Comparability is *mostly*
    /// bidirectional: the simple arms are tried in BOTH directions
    /// (`relater.go:181`), a source UNION needs only SOME constituent related
    /// (`relater.go:2870`), and it shares the assignable-only simple arms
    /// (`relater.go:261`). Upstream's further comparable carve-outs — the
    /// type-parameter constraint rule (`:3435`), template-literal
    /// definite-unrelatedness (`:3574`), the intersection-into-primitive
    /// constraint hoist (`:2886`), mapped-type modifier leniency (`:3973`)
    /// — sit on shapes this relater does not decide at all, so they stay
    /// gaps rather than divergences.
    Comparable,
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

    /// Whether `source` is comparable to `target` (`isTypeComparableTo`,
    /// `relater.go:162`). §750.
    #[must_use]
    pub fn is_type_comparable_to(&mut self, source: TypeId, target: TypeId) -> bool {
        self.is_type_related_to(source, target, Relation::Comparable)
    }

    /// `areTypesComparable` (`relater.go:166`): comparable in either
    /// direction. §750.
    #[must_use]
    pub fn are_types_comparable(&mut self, type1: TypeId, type2: TypeId) -> bool {
        self.is_type_comparable_to(type1, type2) || self.is_type_comparable_to(type2, type1)
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
        // Measurement only; a no-op unless `reasons::enable` was called.
        let outer = reasons::begin();
        let answer = relater.is_related_to(source, target);
        reasons::finish(outer, answer == Ternary::Unknown);
        answer
    }

    pub(crate) fn compare_signature_ternary(
        &mut self,
        source: &crate::signatures::Signature,
        target: &crate::signatures::Signature,
    ) -> Option<Ternary> {
        let mut relater = Relater {
            checker: self,
            relation: Relation::Assignable,
            results: FxHashMap::default(),
            depth: 0,
        };
        relater.one_signature_related_to(source, target, false, false)
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
        if let Some([sup, sub, other]) = self.checker.variance_markers
            && [sup, sub, other].contains(&source)
            && [sup, sub, other].contains(&target)
        {
            return if source == target || (source == sub && target == sup) {
                Ternary::Related
            } else {
                Ternary::NotRelated
            };
        }
        let target = self.checker.get_regular_type_of_literal_type(target);
        if source == target {
            return Ternary::Related;
        }
        // `relater.go:181`/`:2661`: under the comparable relation the simple
        // arms are also tried REVERSED (target against source) first, unless
        // the target is `never`. §750.
        if matches!(self.relation, Relation::Comparable)
            && !self.checker.type_of(target).flags.intersects(TypeFlags::NEVER)
            && self.is_simple_type_related_to(target, source) == Some(true)
        {
            return Ternary::Related;
        }
        match self.is_simple_type_related_to(source, target) {
            Some(true) => return Ternary::Related,
            Some(false) => return Ternary::NotRelated,
            None => {}
        }
        // anyFunctionType has no properties, and function expressions have
        // no own property requirements. Their call-signature relation is the
        // wildcard rule even when no symbol member table is attached.
        if self.checker.any_function_type == Some(source)
            && self.is_plain_function_expression_type(target)
        {
            return Ternary::Related;
        }
        if self.checker.any_function_type == Some(target)
            && self.is_plain_function_expression_type(source)
        {
            return Ternary::NotRelated;
        }
        if self.is_plain_function_expression_type(source)
            && self.is_plain_function_expression_type(target)
        {
            return self.recursive_type_related_to(source, target);
        }
        // §17 (`checker-notes-assign.md`): both-own-private class pairs are
        // nominal — NotRelated by the private-identity rule, decided from
        // syntax.
        if let Some(answer) = self.checker.nominal_class_pair_verdict(source, target) {
            return if answer { Ternary::Related } else { Ternary::NotRelated };
        }
        let composite = TypeFlags::UNION.union(TypeFlags::INTERSECTION);
        let s = self.checker.type_of(source).flags;
        let t = self.checker.type_of(target).flags;
        let source_tuple = self.checker.tuple_element_lists.contains_key(&source)
            || self.checker.variadic_tuple_elements.contains_key(&source);
        let target_tuple = self.checker.tuple_element_lists.contains_key(&target)
            || self.checker.variadic_tuple_elements.contains_key(&target);
        let tuple_array_pair = (source_tuple
            && self.checker.tuple_spread_array_element(target).is_some())
            || (target_tuple && self.checker.tuple_spread_array_element(source).is_some());
        // structuredTypeRelatedTo compares primitive sources through their
        // apparent wrapper type (internal/checker/relater.go). Indexed targets
        // still need sourceIsPrimitive rules for an acceptance, but a failed
        // required-property comparison already proves a rejection.
        if s.intersects(TypeFlags::PRIMITIVE)
            && !s.intersects(TypeFlags::NULLABLE | TypeFlags::VOID)
            && t.intersects(TypeFlags::OBJECT)
        {
            let apparent = self.checker.apparent_type(source);
            if apparent != source {
                if self
                    .checker
                    .get_index_infos_of_type(target)
                    .is_some_and(|infos| !infos.is_empty())
                {
                    return if matches!(self.relation, Relation::Subtype | Relation::StrictSubtype)
                        && self.has_members(apparent)
                        && self.has_members(target)
                        && self.properties_related_to(apparent, target) == Ternary::NotRelated
                    {
                        Ternary::NotRelated
                    } else {
                        Ternary::Unknown
                    };
                }
                return self.is_related_to(apparent, target);
            }
        }
        // Two object types with members reach the structural arm; upstream's
        // gate is `source.flags&TypeFlags::StructuredOrInstantiable != 0 &&
        // target.flags&...`, and `Named { members: Some(_) }` is the whole of
        // this port's "structured".
        if s.intersects(composite)
            || t.intersects(composite)
            || (self.has_members(source) && self.has_members(target))
            || (source_tuple && target_tuple)
            || tuple_array_pair
            || s.contains(TypeFlags::TYPE_PARAMETER)
            || t.contains(TypeFlags::STRING_MAPPING)
            || (t.contains(TypeFlags::TEMPLATE_LITERAL)
                && s.intersects(
                    TypeFlags::STRING_LITERAL | TypeFlags::TEMPLATE_LITERAL | TypeFlags::STRING,
                ))
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
        // §369: a NON-NULLABLE primitive source against the EMPTY object type
        // relates — upstream's structural walk finds nothing to require of
        // `{}` (`string | {}` subtype-reduces to `{}`,
        // `nullishCoalescingOperator2`'s `a7 ?? 'whatever'`). `undefined`,
        // `null` and `void` stay out — they do not inhabit `{}` under strict
        // — and the FRESH empty-literal exception (relater.go:3853) cannot
        // fire here because this port carries no object freshness; the
        // recognition is the printed `{}`, the same approximation
        // `intersections.rs` records.
        if s.intersects(TypeFlags::PRIMITIVE)
            && !s.intersects(TypeFlags::NULLABLE | TypeFlags::VOID)
            && matches!(&self.checker.type_of(target).data,
                TypeData::Named { text, .. } if text == "{}")
        {
            return Ternary::Related;
        }
        // §357: an OBJECT source against a decidable primitive target is a
        // decision, not an absence — upstream's `isSimpleTypeRelatedTo` has no
        // arm relating an object to `undefined`/`null`/`void`/`string`/…
        // (the object arms it does have — `any`/`unknown`/`never`/
        // `nonprimitive` targets — all fired above), and
        // `structuredTypeRelatedTo` never relates an object to a
        // non-structured target. `Baz -> undefined` reads NotRelated, which
        // is what lets a class-instance union carry its nullable constituent
        // through subtype reduction (`generatorTypeCheck22`).
        if s.intersects(TypeFlags::OBJECT) && self.flag_decidable(target) {
            return Ternary::NotRelated;
        }
        // A concrete object cannot inhabit an arbitrary target parameter.
        // Generic mapped types have a separate target-parameter relation
        // (relater.go:3423), which remains outside this arm.
        if t.contains(TypeFlags::TYPE_PARAMETER)
            && s.intersects(TypeFlags::OBJECT | TypeFlags::UNKNOWN)
            && matches!(self.relation, Relation::Subtype | Relation::StrictSubtype)
            && !self.checker.mapped_types.contains_key(&source)
        {
            return Ternary::NotRelated;
        }
        if self.checker.strict_null_checks
            && s.intersects(TypeFlags::NULLABLE)
            && t.contains(TypeFlags::OBJECT)
            && matches!(self.relation, Relation::Subtype | Relation::StrictSubtype)
        {
            return Ternary::NotRelated;
        }
        // A template always inhabits the string domain. Generic holes do not
        // make it overlap a decidable non-string primitive.
        if s.intersects(TypeFlags::TEMPLATE_LITERAL | TypeFlags::STRING_MAPPING)
            && self.flag_decidable(target)
            && !t.intersects(TypeFlags::STRING_LIKE)
        {
            return Ternary::NotRelated;
        }
        if self.flag_decidable(source) && self.flag_decidable(target) {
            Ternary::NotRelated
        } else {
            // Measurement only: say which of the two shapes above it was, per
            // undecidable side. An object type here is one with no members
            // table (row 3); anything else carries a flag with no simple arm.
            for side in [source, target] {
                if self.flag_decidable(side) {
                    continue;
                }
                let flags = self.checker.type_of(side).flags;
                if flags.intersects(TypeFlags::OBJECT) {
                    reasons::note(reasons::Site::NoMembersTable);
                } else {
                    reasons::note(reasons::Site::UnportedFlag);
                    reasons::note_flags(flags.bits());
                }
            }
            Ternary::Unknown
        }
    }

    /// Whether `id` is an object type with a members table to compare.
    fn has_members(&self, id: TypeId) -> bool {
        matches!(&self.checker.type_of(id).data, TypeData::Named { members: Some(_), .. })
    }

    fn is_plain_function_expression_type(&self, id: TypeId) -> bool {
        self.checker.signature_types.get(&id).is_some_and(|signatures| {
            !signatures.is_empty()
                && signatures.iter().all(|signature| {
                    matches!(
                        self.checker.nodes.kind(signature.declaration),
                        tsr_ast::SyntaxKind::ArrowFunction
                            | tsr_ast::SyntaxKind::FunctionExpression
                            | tsr_ast::SyntaxKind::FunctionType
                            | tsr_ast::SyntaxKind::MethodSignature
                            | tsr_ast::SyntaxKind::MethodDeclaration
                            | tsr_ast::SyntaxKind::CallSignature
                    )
                })
        })
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
    /// §935: the one-call-signature arm of `signaturesRelatedTo`
    /// (`relater.go:4441`). `None` when the shape is outside what this port can
    /// decide, which keeps row 6's `Unknown`; `Some` is a real verdict.
    fn related_call_signatures(&mut self, source: TypeId, target: TypeId) -> Option<Ternary> {
        if self.checker.any_function_type == Some(source) {
            return Some(Ternary::Related);
        }
        if self.checker.any_function_type == Some(target) {
            return Some(Ternary::NotRelated);
        }
        let source_signatures = self.checker.call_signatures_of_type(source)?;
        let target_signatures = self.checker.call_signatures_of_type(target)?;
        if source_signatures.is_empty() || target_signatures.is_empty() {
            return None;
        }
        // §936.1: §935 required exactly ONE signature per side and named the
        // generalisation as its residue — upstream's *"some source signature
        // relates to each target signature"* (`signaturesRelatedTo`,
        // `relater.go:4441`, whose loop iterates the TARGET's list and searches
        // the source's). That is this walk.
        //
        // An unjudgeable PAIR is skipped rather than failing the set, so a target
        // signature with no judgeable partner leaves the set UNDECIDED (`None`)
        // rather than rejected — a missing verdict, never a wrong one.
        let mut parts = Vec::new();
        for target_signature in &target_signatures {
            let mut best: Option<Ternary> = None;
            for source_signature in &source_signatures {
                let Some(verdict) =
                    self.one_signature_related_to(source_signature, target_signature, false, false)
                else {
                    continue;
                };
                if verdict == Ternary::Related {
                    best = Some(Ternary::Related);
                    break;
                }
                best = Some(match best {
                    None | Some(Ternary::Unknown) => verdict,
                    Some(held) => held,
                });
            }
            parts.push(best?);
        }
        Some(Ternary::all(parts))
    }

    /// One source signature against one target signature — the comparison §935
    /// ported, now the inner step of [`Relater::related_call_signatures`].
    ///
    /// `None` when the pair is outside what this port can judge, which lets the
    /// caller keep looking rather than reading "cannot judge" as "not related".
    fn one_signature_related_to(
        &mut self,
        source_signature: &crate::signatures::Signature,
        target_signature: &crate::signatures::Signature,
        callback: bool,
        bivariant_callback: bool,
    ) -> Option<Ternary> {
        let source_top = self.checker.signature_is_top(source_signature);
        let target_top = self.checker.signature_is_top(target_signature);
        let strict_top = matches!(self.relation, Relation::Subtype | Relation::StrictSubtype);
        if target_top && !(strict_top && source_top) {
            return Some(Ternary::Related);
        }
        if strict_top && source_top && !target_top {
            return Some(Ternary::NotRelated);
        }
        let target_count = self.checker.signature_parameter_count(target_signature);
        let source_minimum = self.checker.signature_min_argument_count(source_signature);
        let source_count = self.checker.signature_parameter_count(source_signature);
        if !self.checker.signature_has_effective_rest(target_signature)
            && if self.relation == Relation::StrictSubtype {
                self.checker.signature_has_effective_rest(source_signature)
                    || source_count > target_count
            } else {
                source_minimum > target_count
            }
        {
            return Some(Ternary::NotRelated);
        }
        let shared_type_parameters = if !source_signature.type_parameters.is_empty()
            && source_signature.type_parameters.len() == target_signature.type_parameters.len()
        {
            match (
                self.checker.type_parameter_types(source_signature),
                self.checker.type_parameter_types(target_signature),
            ) {
                (Some(source), Some(target)) => source == target,
                _ => false,
            }
        } else {
            source_signature.type_parameters.is_empty()
                && target_signature.type_parameters.is_empty()
        };
        let instantiated_source = if !shared_type_parameters
            && !source_signature.type_parameters.is_empty()
        {
            Some(
                self.checker
                    .instantiate_signature_in_context(source_signature.clone(), target_signature)?,
            )
        } else {
            None
        };
        let source_signature = instantiated_source.as_ref().unwrap_or(source_signature);
        let source_count = self.checker.signature_parameter_count(source_signature);
        let source_minimum = self.checker.signature_min_argument_count(source_signature);
        let target_minimum = self.checker.signature_min_argument_count(target_signature);
        let non_array_rest = self.checker.signature_non_array_rest_type(source_signature).is_some()
            || self.checker.signature_non_array_rest_type(target_signature).is_some();
        let parameter_count = if non_array_rest {
            source_count.min(target_count)
        } else {
            source_count.max(target_count)
        };
        let rest_index = non_array_rest.then(|| parameter_count.checked_sub(1)).flatten();
        let strict_variance = !callback
            && self.checker.strict_function_types
            && !matches!(
                self.checker.nodes.kind(target_signature.declaration),
                tsr_ast::SyntaxKind::MethodDeclaration
                    | tsr_ast::SyntaxKind::MethodSignature
                    | tsr_ast::SyntaxKind::Constructor
            );
        let mut parts = Vec::with_capacity(parameter_count + 1);
        if let (Some(source_this), Some(target_this)) =
            (&source_signature.this_parameter, &target_signature.this_parameter)
            && source_this.r#type != self.checker.intrinsics.void
        {
            let reverse = self.is_related_to(target_this.r#type, source_this.r#type);
            parts.push(if strict_variance {
                reverse
            } else {
                Ternary::any([self.is_related_to(source_this.r#type, target_this.r#type), reverse])
            });
        }
        for index in 0..parameter_count {
            let source_type = if rest_index == Some(index) {
                Some(self.checker.signature_rest_or_any_at_position(source_signature, index))
            } else {
                self.checker.signature_type_at_position(source_signature, index)
            };
            let target_type = if rest_index == Some(index) {
                Some(self.checker.signature_rest_or_any_at_position(target_signature, index))
            } else {
                self.checker.signature_type_at_position(target_signature, index)
            };
            let (Some(from), Some(to)) = (source_type, target_type) else { continue };
            if self.checker.is_error(from) || self.checker.is_error(to) {
                return None;
            }
            if from == to && self.relation != Relation::StrictSubtype {
                continue;
            }
            let source_non_nullable = self.checker.get_non_nullable_type(from);
            let target_non_nullable = self.checker.get_non_nullable_type(to);
            let source_callback = if !callback
                && !self
                    .checker
                    .signature_has_instantiated_generic_parameter(source_signature, index)
            {
                self.checker.single_call_signature(source_non_nullable)
            } else {
                None
            };
            let target_callback = if !callback
                && !self
                    .checker
                    .signature_has_instantiated_generic_parameter(target_signature, index)
            {
                self.checker.single_call_signature(target_non_nullable)
            } else {
                None
            };
            let nullable = crate::flow::TypeFacts::IS_UNDEFINED | crate::flow::TypeFacts::IS_NULL;
            if !callback
                && let (Some(source_callback), Some(target_callback)) =
                    (source_callback, target_callback)
                && source_callback.predicate.is_none()
                && target_callback.predicate.is_none()
                && self.checker.get_type_facts(from) & nullable
                    == self.checker.get_type_facts(to) & nullable
            {
                parts.push(self.one_signature_related_to(
                    &target_callback,
                    &source_callback,
                    true,
                    !strict_variance,
                )?);
            } else {
                let reverse = self.is_related_to(to, from);
                parts.push(if callback || strict_variance {
                    reverse
                } else {
                    Ternary::any([self.is_related_to(from, to), reverse])
                });
            }
            if self.relation == Relation::StrictSubtype
                && index >= source_minimum
                && index < target_minimum
                && self.is_related_to(from, to) != Ternary::NotRelated
            {
                return Some(Ternary::NotRelated);
            }
        }
        if target_signature.r#type != self.checker.intrinsics.void
            && target_signature.r#type != self.checker.intrinsics.any
        {
            if target_signature.predicate.is_some() {
                if source_signature.predicate.is_some() {
                    if !source_signature.predicate_kinds_match(target_signature)? {
                        return Some(Ternary::NotRelated);
                    }
                    let source = source_signature.predicate.as_ref()?.r#type;
                    let target = target_signature.predicate.as_ref()?.r#type;
                    parts.push(match (source, target) {
                        (Some(source), Some(target)) => self.is_related_to(source, target),
                        (None, None) => Ternary::Related,
                        _ => Ternary::NotRelated,
                    });
                } else if !target_signature.predicate.as_ref()?.asserts {
                    return Some(Ternary::NotRelated);
                }
            } else {
                let forward = self.is_related_to(source_signature.r#type, target_signature.r#type);
                parts.push(if bivariant_callback {
                    Ternary::any([
                        self.is_related_to(target_signature.r#type, source_signature.r#type),
                        forward,
                    ])
                } else {
                    forward
                });
            }
        }
        Some(Ternary::all(parts))
    }

    /// Whether `id` declares a CALL or CONSTRUCT signature — the half of
    /// [`Relater::signature_bearing`] that §935's arm is about, split out so
    /// §936's index arm cannot run on a type §935 should be judging.
    fn declares_call_or_construct(&self, id: TypeId) -> bool {
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
                )
            })
        })
    }

    /// §936: `indexSignaturesRelatedTo` (`relater.go`), reduced to the two arms
    /// upstream reaches for a plain object target.
    ///
    /// For each of the target's index infos:
    ///
    /// 1. the source declares an index info with **the same key type** and the
    ///    values relate covariantly; or
    /// 2. the source's property enumeration is complete and **every** property
    ///    type relates to the target's value type — upstream's
    ///    `membersRelatedToIndexInfo`.
    ///
    /// `None` when neither applies, which keeps row 6's `Unknown`.
    ///
    /// # Not ported
    ///
    /// - **Key subtyping is ported** (a `string`-keyed source satisfies a
    ///   `number`-keyed target, since every numeric key is a string key; the
    ///   reverse does not hold) and **measured zero change**. Kept because it is
    ///   what `getApplicableIndexInfo` does, with the zero recorded so the next
    ///   reader does not re-derive it — §935's discipline.
    /// - **`symbol` and pattern keys**, which `IndexInfo` does not model.
    /// - **`readonly` on the index signature**, which is a missing rejection and
    ///   shares that status with every other `readonly` in this relater.
    fn related_index_signatures(&mut self, source: TypeId, target: TypeId) -> Option<Ternary> {
        let target_infos = self.checker.get_index_infos_of_type(target)?;
        if target_infos.is_empty() {
            return None;
        }
        let source_infos = self.checker.get_index_infos_of_type(source).unwrap_or_default();
        let mut parts = Vec::with_capacity(target_infos.len());
        for info in &target_infos {
            let applicable =
                source_infos.iter().find(|candidate| candidate.key == info.key).or_else(|| {
                    // Upstream's `getApplicableIndexInfo`: a STRING index
                    // applies to a NUMBER access, because every numeric key is
                    // also a string key. The reverse does not hold.
                    (info.key == self.checker.intrinsics.number)
                        .then(|| {
                            source_infos.iter().find(|c| c.key == self.checker.intrinsics.string)
                        })
                        .flatten()
                });
            if let Some(from) = applicable {
                let (from_value, to_value) = (from.value, info.value);
                parts.push(self.is_related_to(from_value, to_value));
                continue;
            }
            let names = self.property_names_of(source)?;
            for name in &names {
                let member = self.checker.get_type_of_property_of_type(source, name)?;
                parts.push(self.is_related_to(member, info.value));
            }
        }
        Some(Ternary::all(parts))
    }

    fn signature_bearing(&self, id: TypeId) -> bool {
        if self.checker.any_function_type == Some(id) {
            return true;
        }
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
        // §751: the enum arms, on this port's member model — a member is a
        // `Named` type flagged `ENUM` whose VALUE lives in the value-keyed
        // intern map (`enum_value_types`), so "the literal's value" is read
        // back through [`Checker::enum_member_value`] (`n:`/`s:` keys). They
        // sit BEFORE the `NumberLike` arm because that arm's `ENUM` bit would
        // otherwise relate a STRING-valued member to `number`.
        let source_member = self.checker.enum_member_value(source);
        let target_member = self.checker.enum_member_value(target);
        let plain =
            |flags: TypeFlags| !flags.intersects(TypeFlags::UNION | TypeFlags::INTERSECTION);
        if let Some((source_owner, source_key)) = &source_member {
            if let Some((target_owner, target_key)) = &target_member {
                // `relater.go:236-243`: two members of ONE enum relate only
                // by identity (caught above) — otherwise they are two
                // literals and the structured walk finds nothing. Members of
                // two enums relate through `isEnumTypeRelatedTo`, which
                // requires the same NAME and member-by-member equal values;
                // a different name is decided, the same name is not ported.
                if source_owner == target_owner {
                    return Some(source_key == target_key);
                }
                let same_name = self.checker.binder.symbols().get(*source_owner).name
                    == self.checker.binder.symbols().get(*target_owner).name;
                return if same_name { None } else { Some(false) };
            }
            let numeric = source_key.starts_with("n:");
            // `relater.go:219`/`:225`: an enum literal relates to the PLAIN
            // literal of its value.
            if t.intersects(TypeFlags::LITERAL)
                && plain(t)
                && self.checker.plain_literal_key(target).as_deref() == Some(source_key.as_str())
            {
                return Some(true);
            }
            // `NumberLike -> number` / `StringLike -> string`, by the
            // member's actual domain.
            if plain(t) && t.intersects(TypeFlags::NUMBER | TypeFlags::STRING) {
                return Some(numeric == t.intersects(TypeFlags::NUMBER));
            }
        } else if let Some((_, target_key)) = &target_member
            && matches!(self.relation, Relation::Assignable | Relation::Comparable)
            && target_key.starts_with("n:")
        {
            // `relater.go:266-270`, the bit-flag rules: `number` relates to a
            // numeric enum member (and so, through the union dispatch, to a
            // numeric enum), and a non-enum numeric literal to the member
            // holding its value.
            if s.intersects(TypeFlags::NUMBER) && plain(s) {
                return Some(true);
            }
            if s.intersects(TypeFlags::NUMBER_LITERAL)
                && self.checker.plain_literal_key(source).as_deref() == Some(target_key.as_str())
            {
                return Some(true);
            }
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
        // isSimpleTypeRelatedTo: in non-strict null checking, nullable
        // sources relate to every non-union/intersection target except never.
        if s.intersects(TypeFlags::UNDEFINED)
            && ((!self.checker.strict_null_checks
                && !t.intersects(TypeFlags::UNION | TypeFlags::INTERSECTION))
                || t.intersects(TypeFlags::UNDEFINED.union(TypeFlags::VOID)))
        {
            return Some(true);
        }
        if s.intersects(TypeFlags::NULL)
            && ((!self.checker.strict_null_checks
                && !t.intersects(TypeFlags::UNION | TypeFlags::INTERSECTION))
                || t.intersects(TypeFlags::NULL))
        {
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
        // The enum arms of the same upstream block (`relater.go:266-270`)
        // are ported above, at the member level (§751).
        if matches!(self.relation, Relation::Assignable | Relation::Comparable)
            && s.intersects(TypeFlags::ANY)
        {
            return Some(true);
        }
        None
    }

    /// isValidTypeForTemplateLiteralPlaceholder (relater.go:2476).
    fn valid_template_placeholder(&mut self, source: TypeId, target: TypeId) -> bool {
        if let TypeData::Intersection { types, .. } = &self.checker.type_of(target).data {
            let types = types.clone();
            return types.into_iter().all(|target| {
                self.checker.is_empty_anonymous_object_type(target)
                    || self.valid_template_placeholder(source, target)
            });
        }
        let flags = self.checker.type_of(target).flags;
        if flags.contains(TypeFlags::STRING)
            || self.is_related_to(source, target) != Ternary::NotRelated
        {
            return true;
        }
        if let TypeData::StringLiteral(value) = &self.checker.type_of(source).data {
            let value = value.clone();
            if flags.contains(TypeFlags::NUMBER) {
                return crate::template_match::template_number(&value, false).is_some();
            }
            if flags.contains(TypeFlags::BIG_INT) {
                return crate::template_match::template_bigint(&value, false).is_some();
            }
            if flags.intersects(TypeFlags::BOOLEAN_LITERAL | TypeFlags::NULLABLE) {
                return value == self.checker.type_to_string(target);
            }
            if flags.contains(TypeFlags::STRING_MAPPING) {
                return self.checker.is_member_of_string_mapping(source, target);
            }
            if let Some(parts) = self.checker.template_literal_parts.get(&target).cloned() {
                return self.checker.template_literal_inferences(source, &parts).is_some_and(
                    |matches| {
                        matches
                            .into_iter()
                            .zip(parts.types)
                            .all(|(source, target)| self.valid_template_placeholder(source, target))
                    },
                );
            }
        }
        if let Some(parts) = self.checker.template_literal_parts.get(&source).cloned()
            && parts.types.len() == 1
            && parts.texts.iter().all(String::is_empty)
        {
            return self.is_related_to(parts.types[0], target) != Ternary::NotRelated;
        }
        false
    }

    /// The composite arms, guarded by the depth cap and the cycle cache.
    /// Ported from Checker.recursiveTypeRelatedTo (relater.go).
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
            reasons::note(reasons::Site::DepthCap);
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
        // Source type variables explore their constraint under the same cycle
        // guard (relater.go:3664). Synthetic this types keep the existing
        // structural member path; an unreadable written constraint is unknown.
        if self.checker.type_of(source).flags.contains(TypeFlags::TYPE_PARAMETER)
            && matches!(self.relation, Relation::Subtype | Relation::StrictSubtype)
            && let Some(&symbol) = self.checker.type_parameter_symbols.get(&source)
            && let Some(declaration) =
                self.checker.binder.symbols().get(symbol).declarations.iter().find_map(|&id| {
                    match self.checker.node_map.get(id) {
                        Some(tsr_ast::Node::TypeParameterDeclaration(parameter)) => Some(parameter),
                        _ => None,
                    }
                })
        {
            let mut constraint = if declaration.constraint.is_some() {
                let Some(constraint) = self.checker.type_parameter_constraint(source) else {
                    return Ternary::Unknown;
                };
                constraint
            } else {
                self.checker.intrinsics.unknown
            };
            // Constraint cycles do not justify a coinductive object relation.
            // Stop type-parameter-only cycles before entering the pair cache;
            // a constraint equal to the target keeps its direct identity proof.
            let mut seen = vec![source];
            while constraint != target
                && self.checker.type_of(constraint).flags.contains(TypeFlags::TYPE_PARAMETER)
            {
                if seen.contains(&constraint) {
                    return Ternary::Unknown;
                }
                seen.push(constraint);
                let Some(next) = self.checker.type_parameter_constraint(constraint) else { break };
                constraint = next;
            }
            return self.is_related_to(constraint, target);
        }
        if let Some(constituents) = self.union_constituents(source) {
            // Every constituent of a source union must be related.
            // Upstream's `eachTypeRelatedToType` — except under the
            // comparable relation, where SOME constituent suffices
            // (`relater.go:2870`, `someTypeRelatedToType`). §750.
            let parts: Vec<_> =
                constituents.iter().map(|&c| self.is_related_to(c, target)).collect();
            return if matches!(self.relation, Relation::Comparable) {
                Ternary::any(parts)
            } else {
                Ternary::all(parts)
            };
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
        if let Some(parts) = self.checker.template_literal_parts.get(&target).cloned()
            && self.checker.type_of(source).flags.intersects(
                TypeFlags::STRING_LITERAL | TypeFlags::TEMPLATE_LITERAL | TypeFlags::STRING,
            )
        {
            // templateLiteralTypesDefinitelyUnrelated: comparable patterns may
            // overlap even when neither is assignable to the other.
            if matches!(self.relation, Relation::Comparable)
                && let Some(source_parts) = self.checker.template_literal_parts.get(&source)
            {
                let source_start = source_parts.texts[0].as_bytes();
                let target_start = parts.texts[0].as_bytes();
                let source_end = source_parts.texts.last().unwrap().as_bytes();
                let target_end = parts.texts.last().unwrap().as_bytes();
                let start = source_start.len().min(target_start.len());
                let end = source_end.len().min(target_end.len());
                return if source_start[..start] != target_start[..start]
                    || source_end[source_end.len() - end..] != target_end[target_end.len() - end..]
                {
                    Ternary::NotRelated
                } else {
                    Ternary::Related
                };
            }
            let Some(matches) = self.checker.template_literal_inferences(source, &parts) else {
                return Ternary::NotRelated;
            };
            return if matches
                .into_iter()
                .zip(parts.types)
                .all(|(source, target)| self.valid_template_placeholder(source, target))
            {
                Ternary::Related
            } else {
                Ternary::NotRelated
            };
        }
        if let Some((target_symbol, target_inner)) =
            self.checker.string_mapping_types.get(&target).copied()
        {
            if let Some((source_symbol, source_inner)) =
                self.checker.string_mapping_types.get(&source).copied()
            {
                return if source_symbol == target_symbol {
                    self.is_related_to(source_inner, target_inner)
                } else {
                    Ternary::NotRelated
                };
            }
            return if self.checker.is_member_of_string_mapping(source, target) {
                Ternary::Related
            } else {
                Ternary::NotRelated
            };
        }
        if let Some(answer) = self.tuples_related_to(source, target) {
            return answer;
        }
        if let Some(answer) = self.tuple_array_related_to(source, target) {
            return answer;
        }
        // structuredTypeRelatedTo (relater.go:3841): a mutable array relates
        // to a readonly array by its numeric index, without comparing every
        // library method. Same-target arrays still use the variance path.
        if let (Some((source_symbol, _)), Some((target_symbol, _))) = (
            self.checker.type_reference_targets.get(&source).cloned(),
            self.checker.type_reference_targets.get(&target).cloned(),
        ) {
            let source_symbol = self.checker.binder.merged_symbol(source_symbol);
            let target_symbol = self.checker.binder.merged_symbol(target_symbol);
            if self
                .checker
                .global_type_symbol("Array")
                .is_some_and(|symbol| self.checker.binder.merged_symbol(symbol) == source_symbol)
                && self.checker.global_type_symbol("ReadonlyArray").is_some_and(|symbol| {
                    self.checker.binder.merged_symbol(symbol) == target_symbol
                })
                && let (Some(source_element), Some(target_element)) = (
                    self.checker.tuple_spread_array_element(source),
                    self.checker.tuple_spread_array_element(target),
                )
            {
                return self.is_related_to(source_element, target_element);
            }
        }
        if self.is_plain_function_expression_type(source)
            && self.is_plain_function_expression_type(target)
        {
            return self.related_call_signatures(source, target).unwrap_or(Ternary::Unknown);
        }
        // relateVariances (internal/checker/relater.go): shared reference
        // targets compare their arguments in the measured directions. Marker
        // instances and an active recursive measurement compare structurally.
        if !self.checker.variance_marker_types.contains(&source)
            && !self.checker.variance_marker_types.contains(&target)
            && let (
                Some((source_symbol, source_arguments)),
                Some((target_symbol, target_arguments)),
            ) = (
                self.checker.type_reference_targets.get(&source).cloned(),
                self.checker.type_reference_targets.get(&target).cloned(),
            )
            && source_symbol == target_symbol
            && source_arguments.len() == target_arguments.len()
        {
            let measured = self.checker.inference_variances(source_symbol);
            let variances = match measured {
                Some(variances) if variances.len() == source_arguments.len() => variances,
                None if !self.checker.variance_in_progress.is_empty() => return Ternary::Unknown,
                _ if self.checker.variance_in_progress.is_empty() => {
                    // Until Unmeasurable/Unreliable flags are represented, keep
                    // the existing default covariance for unmeasured targets.
                    vec![crate::variances::Variance::Covariant; source_arguments.len()]
                }
                _ => Vec::new(),
            };
            if variances.len() == source_arguments.len() {
                let allows_covariant_void =
                    target_arguments.iter().zip(&variances).any(|(&target, variance)| {
                        *variance == crate::variances::Variance::Covariant
                            && self.checker.type_of(target).flags.intersects(TypeFlags::VOID)
                    });
                let mut parts = Vec::new();
                for ((source, target), variance) in
                    source_arguments.into_iter().zip(target_arguments).zip(variances)
                {
                    use crate::variances::Variance;
                    parts.push(match variance {
                        Variance::Covariant => self.is_related_to(source, target),
                        Variance::Contravariant => self.is_related_to(target, source),
                        Variance::Invariant => {
                            let forward = self.is_related_to(source, target);
                            let reverse = self.is_related_to(target, source);
                            Ternary::all([forward, reverse])
                        }
                        Variance::Bivariant => {
                            let forward = self.is_related_to(source, target);
                            let reverse = self.is_related_to(target, source);
                            Ternary::any([forward, reverse])
                        }
                        Variance::Independent => Ternary::Related,
                    });
                }
                let result = Ternary::all(parts);
                if result != Ternary::NotRelated || !allows_covariant_void {
                    return result;
                }
            }
        }
        if self.has_members(source) && self.has_members(target) {
            // Row 6 of `checker-notes-assign.md` §2, checked **before** the
            // property walk rather than inside it: a signature-bearing pair is
            // not decided at all, and letting it reach `properties_related_to`
            // would produce a confident answer from a comparison that ignored
            // the members that distinguish the two types.
            // §848 narrows row 6 to what upstream's shape actually requires.
            // `signaturesRelatedTo` (`relater.go:4441`) starts at
            // `TernaryTrue` and every one of its branches iterates the
            // **target's** signature list, so a target with no signatures is
            // vacuously related on the signature axis and the pair is decided
            // by `propertiesRelatedTo` alone. A signature-bearing SOURCE
            // against a plain object target is therefore decidable, and
            // refusing it was this port's own over-reach rather than row 6's.
            //
            // A signature-bearing TARGET still refuses: that is the real
            // `signatureRelatedTo` this port does not have.
            // §935: `signaturesRelatedTo` (`relater.go:4441`) for the one
            // shape this port can decide — **both sides carrying exactly one
            // CALL signature**. Row 6's refusal above is right that a
            // signature-bearing target cannot be decided by the property walk
            // alone; it is not right that nothing can decide it.
            //
            // Conservative on purpose, and each restriction is a missing
            // ACCEPTANCE rather than a possible wrong answer:
            //
            // - **One signature each.** An overload set needs upstream's
            //   "some source signature relates to each target signature" walk
            //   with its `Ternary` bookkeeping.
            // - **Equal parameter counts**, no rests, no generics. Upstream
            //   relates shorter-to-longer through `getParameterCount`'s arity
            //   rules; declining is a gap.
            // - **Parameters related in BOTH directions.** Upstream is
            //   contravariant under `strictFunctionTypes` and bivariant for
            //   methods, and this port tracks neither. Requiring both is
            //   stricter than either, so it can only decline where upstream
            //   accepts — never accept where upstream declines. **Relaxing it
            //   to contravariant-only measured ZERO change**, so the strict
            //   form is kept: it costs nothing and cannot answer wrongly.
            //
            // Two of these restrictions were measured and are free. Admitting a
            // SHORTER source parameter list (upstream's arity rule) also
            // measured zero; it is kept because it is what upstream does, and
            // the zero is recorded so the next reader does not re-derive it.
            // - **Returns covariant**, which is upstream's rule outright.
            //
            // The properties walk still runs and both must agree, which is
            // upstream's shape: `signaturesRelatedTo` and `propertiesRelatedTo`
            // are conjuncts.
            if self.signature_bearing(target) {
                if let Some(signatures) = self.related_call_signatures(source, target) {
                    let properties = self.properties_related_to(source, target);
                    return Ternary::all(vec![signatures, properties]);
                }
                // §936: `indexSignaturesRelatedTo` (`relater.go`). §935 left this
                // arm untouched and said so: `signature_bearing` counts INDEX
                // signatures too, so a target declaring only `[k: string]: T`
                // refused even though nothing about it needs
                // `signatureRelatedTo`. The relater used index infos **nowhere**.
                //
                // Only when the target declares no call or construct signature,
                // so §935's population and this one cannot overlap.
                if !self.declares_call_or_construct(target)
                    && let Some(indexes) = self.related_index_signatures(source, target)
                {
                    let properties = self.properties_related_to(source, target);
                    return Ternary::all(vec![indexes, properties]);
                }
                reasons::note(reasons::Site::SignatureBearing);
                return Ternary::Unknown;
            }
            return self.properties_related_to(source, target);
        }
        // Reached only by a type whose *flags* say union or intersection while
        // its data says otherwise, which `is_related_to`'s gate lets through.
        // Nothing was compared, so nothing was decided.
        reasons::note(reasons::Site::CompositeShape);
        Ternary::Unknown
    }

    /// Fixed and concrete-rest tuples in propertiesRelatedTo
    /// (internal/checker/relater.go). Generic variadic operands still require
    /// base-constraint resolution and retain an unknown relation here.
    fn tuples_related_to(&mut self, source: TypeId, target: TypeId) -> Option<Ternary> {
        let (target_elements, target_readonly) = self.tuple_relation_elements(target)?;
        let target_generic = target_elements.iter().any(|element| {
            element.spread && self.checker.tuple_spread_array_element(element.r#type).is_none()
        });
        if !target_generic {
            let constraint = self.checker.tuple_base_constraint(source);
            if constraint != source {
                return Some(self.is_related_to(constraint, target));
            }
        }
        let (source_elements, source_readonly) =
            if let Some(tuple) = self.tuple_relation_elements(source) {
                tuple
            } else {
                self.checker.tuple_spread_array_element(source)?;
                let (symbol, _) = self.checker.type_reference_targets.get(&source)?;
                let symbol = self.checker.binder.merged_symbol(*symbol);
                let readonly = self
                    .checker
                    .global_type_symbol("ReadonlyArray")
                    .is_some_and(|readonly| self.checker.binder.merged_symbol(readonly) == symbol);
                (
                    vec![crate::tuples::TupleElement {
                        r#type: source,
                        spread: true,
                        optional: false,
                        label: None,
                    }],
                    readonly,
                )
            };
        if source_readonly && !target_readonly {
            return Some(Ternary::NotRelated);
        }
        if source_elements.iter().chain(&target_elements).any(|element| {
            element.spread && self.checker.tuple_spread_array_element(element.r#type).is_none()
        }) {
            return Some(Ternary::Unknown);
        }
        let source_rest = source_elements.iter().any(|element| element.spread);
        let target_rest = target_elements.iter().any(|element| element.spread);
        let source_min =
            source_elements.iter().filter(|element| !element.optional && !element.spread).count();
        let target_min =
            target_elements.iter().filter(|element| !element.optional && !element.spread).count();
        let source_arity = source_elements.len();
        let target_arity = target_elements.len();
        if (!source_rest && source_arity < target_min)
            || (!target_rest && target_arity < source_min)
            || (!target_rest && (source_rest || target_arity < source_arity))
        {
            return Some(Ternary::NotRelated);
        }
        let target_start = target_elements.iter().take_while(|element| !element.spread).count();
        let target_end = target_elements.iter().rev().take_while(|element| !element.spread).count();
        let mut parts = Vec::with_capacity(source_arity);
        for (position, element) in source_elements.iter().enumerate() {
            let from_end = source_arity - 1 - position;
            let target_position = if target_rest && position >= target_start {
                target_arity - 1 - from_end.min(target_end)
            } else {
                position
            };
            let Some(target_element) = target_elements.get(target_position) else {
                return Some(Ternary::NotRelated);
            };
            if !target_element.optional
                && !target_element.spread
                && (element.optional || element.spread)
            {
                return Some(Ternary::NotRelated);
            }
            let source_type = if element.spread {
                self.checker.tuple_spread_array_element(element.r#type)?
            } else {
                element.r#type
            };
            let target_type = if target_element.spread {
                self.checker.tuple_spread_array_element(target_element.r#type)?
            } else if target_element.optional
                && self.checker.strict_null_checks
                && !self.checker.exact_optional_property_types
            {
                self.checker
                    .get_union_type(&[target_element.r#type, self.checker.intrinsics.undefined])
            } else {
                target_element.r#type
            };
            parts.push(self.is_related_to(source_type, target_type));
        }
        Some(Ternary::all(parts))
    }

    fn tuple_relation_elements(
        &self,
        id: TypeId,
    ) -> Option<(Vec<crate::tuples::TupleElement>, bool)> {
        if let Some(tuple) = self.checker.variadic_tuple_elements.get(&id) {
            return Some(tuple.clone());
        }
        let (types, readonly) = self.checker.tuple_element_lists.get(&id)?;
        let mask = self.checker.tuple_optional_masks.get(&id);
        Some((
            types
                .iter()
                .enumerate()
                .map(|(index, &t)| crate::tuples::TupleElement {
                    r#type: t,
                    spread: false,
                    optional: mask.and_then(|mask| mask.get(index)).copied().unwrap_or(false),
                    label: None,
                })
                .collect(),
            *readonly,
        ))
    }

    /// structuredTypeRelatedTo's tuple-to-array index comparison
    /// (internal/checker/relater.go).
    fn tuple_array_related_to(&mut self, source: TypeId, target: TypeId) -> Option<Ternary> {
        let fixed = self.checker.tuple_element_lists.get(&source).cloned();
        if fixed.is_none() && !self.checker.variadic_tuple_elements.contains_key(&source) {
            return None;
        }
        let target_element = self.checker.tuple_spread_array_element(target)?;
        let constraint = self.checker.tuple_base_constraint(source);
        if constraint != source {
            return Some(self.is_related_to(constraint, target));
        }
        let (target_symbol, _) = self.checker.type_reference_targets.get(&target)?;
        let target_symbol = self.checker.binder.merged_symbol(*target_symbol);
        let readonly_array = self
            .checker
            .global_type_symbol("ReadonlyArray")
            .map(|symbol| self.checker.binder.merged_symbol(symbol));
        if self.checker.tuple_is_readonly(source) && readonly_array != Some(target_symbol) {
            return Some(Ternary::NotRelated);
        }
        let source_element = if let Some((mut elements, _)) = fixed {
            if self.checker.strict_null_checks
                && self
                    .checker
                    .tuple_optional_masks
                    .get(&source)
                    .is_some_and(|mask| mask.iter().any(|&optional| optional))
            {
                elements.push(self.checker.intrinsics.undefined);
            }
            self.checker.get_union_type(&elements)
        } else {
            let Some(element) = self.checker.variadic_tuple_index_union(source) else {
                return Some(Ternary::Unknown);
            };
            element
        };
        Some(self.is_related_to(source_element, target_element))
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
            reasons::note(reasons::Site::UnfollowableBase);
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
                // Row 2 of `checker-notes-assign.md` §2, half-answered by §15:
                // a target property with no source counterpart is fine when
                // the target property is OPTIONAL — under assignability
                // always, under the subtype relations only for an
                // object-literal source (`requireOptionalProperties`,
                // upstream `propertiesRelatedTo`; interface-backed sources
                // must still match optionals or subtype reduction loses its
                // order). Optionality reads the declaration's postfix `?`,
                // never `SymbolFlags::OPTIONAL`, which this binder does not
                // write (the `acdeed5` trap). Everything else stays row 2's
                // `Unknown`.
                if self.checker.get_type_of_property_of_type(source, &name).is_none()
                    && self
                        .checker
                        .get_property_of_type(target, &name)
                        .is_some_and(|p| self.checker.property_is_optional(p))
                    && (self.relation == Relation::Assignable
                        || self.checker.is_object_literal_type(source))
                {
                    parts.push(Ternary::Related);
                    continue;
                }
                // §387 narrows row 2: when the SOURCE's own member
                // enumeration is COMPLETE — its names walk succeeds, and a
                // signature-bearing source never reached this function — a
                // missing required target member is upstream's plain `false`,
                // not an absence of knowledge. `bar(i)` with `i: I` against
                // `(i: C)` where `C` requires a member `I` lacks now REJECTS
                // the candidate and the any-overload answers
                // (`symbolProperty13`). An unfollowable source keeps the
                // Unknown.
                if self.checker.get_type_of_property_of_type(source, &name).is_none()
                    && self.property_names_of(source).is_some()
                {
                    parts.push(Ternary::NotRelated);
                    continue;
                }
                reasons::note(reasons::Site::AbsentProperty);
                parts.push(Ternary::Unknown);
                continue;
            };
            // The privacy arms (`propertyRelatedTo`'s first switch, §16 of
            // `checker-notes-assign.md`): PRIVATE on either side relates only
            // when both symbols share one value declaration — an identity
            // this port tests exactly; a protected SOURCE against a public
            // target rejects; a protected TARGET needs `isValidOverrideOf`,
            // unported, so that pair is `Unknown` and any reduction touching
            // it declines whole.
            if let (Some(source_property), Some(target_property)) = (
                self.checker.get_property_of_type(source, &name),
                self.checker.get_property_of_type(target, &name),
            ) {
                let private = tsr_ast::SyntaxKind::PrivateKeyword;
                let protected = tsr_ast::SyntaxKind::ProtectedKeyword;
                let source_private = self.checker.property_has_modifier(source_property, private);
                let target_private = self.checker.property_has_modifier(target_property, private);
                if source_private || target_private {
                    let source_declaration =
                        self.checker.binder.symbols().get(source_property).value_declaration;
                    let target_declaration =
                        self.checker.binder.symbols().get(target_property).value_declaration;
                    if source_declaration != target_declaration || source_declaration.is_none() {
                        parts.push(Ternary::NotRelated);
                        continue;
                    }
                } else if self.checker.property_has_modifier(target_property, protected) {
                    parts.push(Ternary::Unknown);
                    continue;
                } else if self.checker.property_has_modifier(source_property, protected) {
                    parts.push(Ternary::NotRelated);
                    continue;
                }
            }
            // A source-OPTIONAL property against a REQUIRED target member
            // rejects in every relation but comparability
            // (`propertyRelatedTo`, the 1.0-spec §3.8.3 clause: "if M is a
            // required property, N is also a required property") —
            // `{ p?: number }` is not related to `{ p: any }`, which is what
            // keeps `Contextual | Ellement` un-reduced
            // (`nonContextuallyTypedLogicalOr`, §15.1's two wrong lines).
            if let (Some(source_property), Some(target_property)) = (
                self.checker.get_property_of_type(source, &name),
                self.checker.get_property_of_type(target, &name),
            ) && self.checker.property_is_optional(source_property)
                && !self.checker.property_is_optional(target_property)
            {
                parts.push(Ternary::NotRelated);
                continue;
            }
            // `readonly` orders the STRICT subtype relation and only that one
            // (`relater.go:4300`–`:4308`): a readonly source property against
            // a mutable target rejects, so `{ a } | { readonly a }` reduces to
            // `{ readonly a }` and never by declaration order — `readonly`
            // deliberately does not affect assignability. §14 of
            // `checker-notes-assign.md`; `readonlyPropertySubtypeRelationDirected`
            // is the pin.
            if self.relation == Relation::StrictSubtype
                && let (Some(source_property), Some(target_property)) = (
                    self.checker.get_property_of_type(source, &name),
                    self.checker.get_property_of_type(target, &name),
                )
                && self.checker.is_readonly_property(source_property)
                && !self.checker.is_readonly_property(target_property)
            {
                parts.push(Ternary::NotRelated);
                continue;
            }
            // Row 5 of `checker-notes-assign.md` §2 (`bd tsr-4qx`): the member
            // read may be the *uninstantiated* declaration, so on a `C<number>`
            // with a member declared `a: T` this comparison would run against
            // `T` itself. A type parameter surviving into a property type is the
            // observable signature of that, and it is not something to decide on.
            let unresolved = TypeFlags::TYPE_PARAMETER;
            if self.checker.type_of(target_type).flags.intersects(unresolved)
                || self.checker.type_of(source_type).flags.intersects(unresolved)
            {
                reasons::note(reasons::Site::GenericMember);
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
        // §381: late-bound members are in NO table; their bracketed
        // spellings join the walk so a target's `[Symbol.iterator]` is
        // REQUIRED of the source (`symbolProperty13`'s `C -> I`).
        for (name, _) in self.checker.late_bound_members_of(owner) {
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
