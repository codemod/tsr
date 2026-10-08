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
//! - a **raw depth cap** ([`MAX_DEPTH`]), separate from the recursion-identity
//!   guard in upstream's `isDeeplyNestedType`.
//!   Raising it to `10_000` and running
//!   `tests/relater.rs::a_chain_deeper_than_the_cap_gives_up` — a 110-link chain
//!   of interfaces — **aborts the process with a stack overflow**. The cap is
//!   load-bearing for safety, not only for answers.
//! - completed **relation results** keyed on the type pair, separate from
//!   native `maybeKeys`/`maybeKeysSet` recursive assumptions. Re-entering an
//!   assumed pair returns an internal `Maybe`; failed branches discard their
//!   dependent assumptions and successful proofs publish them together.
//!   Deleting the assumption makes
//!   `tests/relater.rs::mutually_recursive_interfaces_terminate` (`interface A
//!   { x: B }` / `interface B { x: A }`) answer `false` instead of `true`. It
//!   still *terminates*, in milliseconds, because the depth cap catches what the
//!   cache no longer closes — so the two guards are not interchangeable: the
//!   cache buys the answer, the cap buys termination.
//!
//! Completed results persist for the checker's lifetime, as upstream's
//! `Relation.results` does on the `Checker` (`tsr-2zk.902`,
//! [`crate::relation_cache`]); active assumptions stay per walk. Until
//! `tsr-2zk.902` they were per top-level call, and TypeScript's own
//! `src/jsTyping` never finished because every narrowing query repeated the
//! same deep walk. The key, lifetime and frame exclusions are recorded in
//! `docs/architecture/checker-relation-publication.md`.

use rustc_hash::{FxHashMap, FxHashSet};
use tsr_binder::{SymbolFlags, SymbolId};

use crate::relation_cache::{CachedRelation, RelationKey};
use crate::{checker::Checker, flags::TypeFlags, types::TypeData, types::TypeId};

/// How deep the structural walk goes before giving up.
///
/// This raw stack bound returns Unknown when neither the pair cache nor the
/// native recursion-identity guard closes the walk. The identity guard counts
/// expanding instantiations separately on the source and target stacks; a long
/// chain of distinct written types still needs this safety bound.
pub const MAX_DEPTH: usize = 100;

/// What [`Relater::indexed_access_write_constraint`] found: native's write
/// constraint, native's nil, or a step this port cannot certify.
enum WriteConstraint {
    Type(TypeId),
    Nil,
    Undecided,
}

/// The answer to a relation question, including *"I could not tell"*.
///
/// **This has no upstream counterpart, and that is the point.** Native
/// uses `TernaryMaybe` for recursive assumptions and `TernaryUnknown`
/// for circular variance checks (`internal/checker/types.go`). Neither is this
/// public port's admission that an arm is unimplemented. Recursive assumptions
/// are tracked separately by the private `RelationResult`.
///
/// Here the omissions are real, and six of them answer "not related" while
/// meaning "not computed" — enumerated in `docs/architecture/checker-notes-assign.md`
/// §2. [`Unknown`](Ternary::Unknown) is what those six answer instead. The
/// public [`Checker::is_type_assignable_to`] maps it back to `false`, so every
/// existing caller is unaffected; a caller that acts on a **negative** can
/// instead ask [`Checker::relate_ternary`] and refuse the pair it cannot decide.
///
/// Unsupported-work composition retains the port's Kleene policy; internal
/// recursive proofs additionally preserve native's assumption-dependent state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ternary {
    /// The relation holds.
    Related,
    /// The relation does not hold, and this port is entitled to say so.
    NotRelated,
    /// This port cannot decide the pair. Never a licence to assume either way.
    Unknown,
}

/// Private proof state. Maybe depends on an open recursive assumption;
/// `CircularVariance` is native's non-false but unpublishable `TernaryUnknown`.
/// Unknown retains the public port's unsupported/depth-refusal meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RelationResult {
    Related,
    Maybe,
    CircularVariance,
    NotRelated,
    Unknown,
}

impl RelationResult {
    fn is_success(self) -> bool {
        matches!(self, Self::Related | Self::Maybe | Self::CircularVariance)
    }

    fn public_answer(self) -> Ternary {
        match self {
            Self::Related | Self::Maybe | Self::CircularVariance => Ternary::Related,
            Self::NotRelated => Ternary::NotRelated,
            Self::Unknown => Ternary::Unknown,
        }
    }

    // A definite failure dominates unsupported work; unsupported work dominates
    // circular variance, which dominates assumptions. Native's circular result
    // is non-false during measurement, but cannot independently publish a proof.
    fn all(parts: impl IntoIterator<Item = Self>) -> Self {
        let mut unknown = false;
        let mut circular = false;
        let mut maybe = false;
        for part in parts {
            match part {
                Self::NotRelated => return Self::NotRelated,
                Self::Unknown => unknown = true,
                Self::CircularVariance => circular = true,
                Self::Maybe => maybe = true,
                Self::Related => {}
            }
        }
        if unknown {
            Self::Unknown
        } else if circular {
            Self::CircularVariance
        } else if maybe {
            Self::Maybe
        } else {
            Self::Related
        }
    }

    // Native some-type walks retain the first successful proof's assumptions.
    // The port also lets a later proof supersede an unsupported branch.
    fn any(parts: impl IntoIterator<Item = Self>) -> Self {
        let mut unknown = false;
        for part in parts {
            match part {
                Self::Related | Self::Maybe | Self::CircularVariance => return part,
                Self::Unknown => unknown = true,
                Self::NotRelated => {}
            }
        }
        if unknown { Self::Unknown } else { Self::NotRelated }
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
/// simple arm means "unported", not "unrelated". Enum literals carry their
/// primitive flags together with `ENUM_LITERAL`; computed members carry ENUM.
/// The value/owner relation runs before primitive and composite relations.
/// Same-named enums from different declarations relate through
/// isEnumTypeRelatedTo's member walk.
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
    .union(TypeFlags::ENUM_LITERAL)
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
        /// Historical row 5; retained histogram slot after semantic generic
        /// member relations replaced the blanket refusal (`bd tsr-6.37`).
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

/// One relation check, carrying the active assumptions and the depth cap.
///
/// Upstream splits this state between the `Checker` (the persistent `Relation`
/// results map) and a per-check `Relater` struct holding `sourceStack`,
/// `targetStack` and `relationCount`. The port splits it the same way:
/// completed results live in [`Checker::relation_results`]
/// ([`crate::relation_cache`]) for the checker's lifetime, except in a walk
/// opened inside a context frame (see [`Relater::new`]), whose results stay
/// in [`Relater::local_results`].
struct Relater<'c, 'a, 'n> {
    checker: &'c mut Checker<'a, 'n>,
    relation: Relation,
    /// Completed results of a walk that may not read or publish the
    /// checker-lifetime store; `None` for an ordinary walk.
    local_results: Option<FxHashMap<RelationKey, CachedRelation>>,
    /// Native maybeKeys/maybeKeysSet: active and assumption-dependent proofs.
    maybe_keys: Vec<RelationKey>,
    maybe_keys_set: FxHashSet<RelationKey>,
    depth: usize,
    source_stack: Vec<TypeId>,
    target_stack: Vec<TypeId>,
    expanding: (bool, bool),
    /// Native `IntersectionStateTarget` (`relater.go:2879`): set while the
    /// walk relates a source to each constituent of a target intersection
    /// (`typeRelatedToEachType`), and inherited by every nested relation the
    /// way native threads `intersectionState` through `isRelatedToEx`. It
    /// disables the excess-property and common-property arms
    /// (`relater.go:2666`/`:2676`) and the intersection property-check pass
    /// (`relater.go:3232`), so it is part of [`RelationKey`] as in native
    /// `getRelationKey`. `IntersectionStateSource` is not represented.
    intersection_target: bool,
    /// Optional direct diagnostic consumer. No storage allocation on verdict-only walks.
    diagnostic_pair: Option<(TypeId, TypeId)>,
    signature_error: Option<(usize, usize)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RecursionIdentity {
    Type(TypeId),
    Symbol(tsr_binder::SymbolId),
    Node(tsr_ast::NodeId),
}

bitflags::bitflags! {
    #[derive(Clone, Copy)]
    struct RecursionFlags: u8 {
        const SOURCE = 1;
        const TARGET = 2;
        const BOTH = Self::SOURCE.bits() | Self::TARGET.bits();
    }
}

/// A type reference's `(target, typeArguments)`, as
/// [`Checker::type_reference_targets`] stores it.
type ReferenceParts = (SymbolId, Vec<TypeId>);

impl Checker<'_, '_> {
    /// `isDiscriminantProperty` (relater.go:1087) over the union `types`: the
    /// synthetic property's member types (createUnionOrIntersectionProperty,
    /// checker.go:21452, over apparent constituents) are non-uniform, one is
    /// a literal or pattern literal, and their union is not generic. A
    /// private or protected member in a union that does not share one
    /// declaration yields no property (checker.go:21556). `None` when a
    /// constituent's member cannot be typed.
    ///
    /// The one computation of the predicate in this port. Native caches the
    /// answer on the synthetic union property (`links.isDiscriminantProperty`);
    /// this port builds no such symbol, so each call recomputes it.
    pub(crate) fn is_discriminant_property_of_types(
        &mut self,
        types: &[TypeId],
        name: &str,
    ) -> Option<bool> {
        let mut first = None;
        let mut non_uniform = false;
        let mut literal = false;
        let mut members = Vec::new();
        let mut declarations = Vec::new();
        let mut non_public = false;
        let mut partial = false;
        for &part in types {
            let part = self.apparent_type(part);
            if self.is_error(part) || self.type_of(part).flags.contains(TypeFlags::NEVER) {
                continue;
            }
            let Some(member) = self.get_type_of_property_of_type(part, name) else {
                partial = true;
                continue;
            };
            if self.is_gap(member) {
                return None;
            }
            if let Some(symbol) = self.get_property_of_type(part, name) {
                non_public |= self
                    .property_has_modifier(symbol, tsr_ast::SyntaxKind::PrivateKeyword)
                    || self.property_has_modifier(symbol, tsr_ast::SyntaxKind::ProtectedKeyword);
                declarations.push(self.binder.symbols().get(symbol).value_declaration);
            }
            match first {
                None => first = Some(member),
                Some(seen) if seen != member => non_uniform = true,
                Some(_) => {}
            }
            literal |= self.is_literal_type(member) || self.is_pattern_template(member);
            members.push(member);
        }
        if first.is_none() || !non_uniform || !literal {
            return Some(false);
        }
        let shared_declaration = declarations
            .first()
            .is_some_and(|first| first.is_some() && declarations.iter().all(|d| d == first));
        if non_public && (partial || declarations.len() > 1) && !shared_declaration {
            return Some(false);
        }
        let union = self.get_union_type(&members);
        let (object, index) = self.spread_generic_flags(union, &mut Vec::new());
        Some(!object && !index)
    }

    /// The `(target, typeArguments)` pairs of two type references, as
    /// `structuredTypeRelatedToWorker`'s same-target arm (`relater.go:3821`)
    /// and `inferFromTypes`' reference arm read them.
    /// [`Checker::type_reference_targets`] interns only instantiations; the
    /// declared type of a generic class or interface is a reference too —
    /// `getDeclaredTypeOfClassOrInterface` (`checker.go:17319`) sets its
    /// `target` to itself and its `resolvedTypeArguments` to its own type
    /// parameters — so a class's `this` type, constrained to that declared
    /// type, meets `Bar<any>` through `getVariances` rather than member by
    /// member. It is recognised from the other side's target symbol,
    /// read-only over `declared_types`; nothing is cached.
    pub(crate) fn same_target_references(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<(ReferenceParts, ReferenceParts)> {
        match (
            self.type_reference_targets.get(&source).cloned(),
            self.type_reference_targets.get(&target).cloned(),
        ) {
            (Some(source), Some(target)) => Some((source, target)),
            (Some(source), None) => {
                let target = self.declared_self_reference(target, source.0)?;
                Some((source, target))
            }
            (None, Some(target)) => {
                let source = self.declared_self_reference(source, target.0)?;
                Some((source, target))
            }
            (None, None) => None,
        }
    }

    /// `ty` as the self-reference of `symbol`'s generic declared type, keyed
    /// by `symbol` as the other side spells it (see
    /// [`Checker::same_target_references`]).
    fn declared_self_reference(&mut self, ty: TypeId, symbol: SymbolId) -> Option<ReferenceParts> {
        let merged = self.binder.merged_symbol(symbol);
        if self.declared_types.get(&merged) != Some(&ty) {
            return None;
        }
        let parameters = self.local_type_parameter_types_of(merged)?;
        if parameters.is_empty() {
            return None;
        }
        Some((symbol, parameters.into_iter().map(|(parameter, _)| parameter).collect()))
    }

    /// getRecursionIdentity (internal/checker/relater.go). Shapes whose native
    /// origin is not represented keep their unique type identity.
    fn relation_recursion_identity(&self, ty: TypeId) -> RecursionIdentity {
        if let Some(symbol) = self.type_parameter_symbols.get(&ty) {
            return RecursionIdentity::Symbol(*symbol);
        }
        if let Some((symbol, _)) = self.type_reference_targets.get(&ty)
            && !self.reference_types_from_nodes.contains(&ty)
            && self
                .binder
                .symbols()
                .get(*symbol)
                .flags
                .intersects(tsr_binder::SymbolFlags::CLASS | tsr_binder::SymbolFlags::INTERFACE)
        {
            return RecursionIdentity::Symbol(*symbol);
        }
        if let Some(info) = self.mapped_conditionals.get(&ty) {
            return RecursionIdentity::Node(info.declaration);
        }
        // getRecursionIdentity tracks a mapped type by its symbol, which every
        // instantiation of one mapped type node shares.
        if let Some(info) = self.mapped_types.get(&ty) {
            return RecursionIdentity::Node(info.declaration);
        }
        if let Some(&(mut object, _, _)) = self.deferred_indexed_access_types.get(&ty) {
            let mut visited = vec![ty];
            while let Some(&(next, _, _)) = self.deferred_indexed_access_types.get(&object) {
                if visited.contains(&object) {
                    break;
                }
                visited.push(object);
                object = next;
            }
            return RecursionIdentity::Type(object);
        }
        RecursionIdentity::Type(ty)
    }

    fn has_relation_recursion_identity(&self, ty: TypeId, identity: RecursionIdentity) -> bool {
        if let TypeData::Intersection { types, .. } = &self.store.get(ty).data {
            return types.iter().any(|&part| self.has_relation_recursion_identity(part, identity));
        }
        self.relation_recursion_identity(ty) == identity
    }

    /// isDeeplyNestedType's increasing-id count and recursiveTypeRelatedTo's
    /// three-occurrence threshold (internal/checker/relater.go).
    pub(crate) fn is_deeply_nested_type(
        &self,
        ty: TypeId,
        stack: &[TypeId],
        threshold: usize,
    ) -> bool {
        if stack.len() < threshold {
            return false;
        }
        if let TypeData::Intersection { types, .. } = &self.store.get(ty).data {
            return types.iter().any(|&part| self.is_deeply_nested_type(part, stack, threshold));
        }
        let identity = self.relation_recursion_identity(ty);
        let mut count = 0;
        let mut last = 0;
        for &previous in stack {
            if self.has_relation_recursion_identity(previous, identity) {
                if previous.index() >= last {
                    count += 1;
                    if count == threshold {
                        return true;
                    }
                }
                last = previous.index();
            }
        }
        false
    }

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
        self.relate_with_signature_diagnostic(source, target, relation, false).0
    }

    /// Native signatureRelatedTo/compareSignaturesRelated arity reporting,
    /// collected during the same relation walk that decides the failure.
    /// Direct signature pairs only; recursive property/signature error contexts
    /// require the complete native error-state contract before expansion.
    pub(crate) fn relate_with_signature_diagnostic(
        &mut self,
        source: TypeId,
        target: TypeId,
        relation: Relation,
        report_errors: bool,
    ) -> (Ternary, Option<tsr_diagnostics::Diagnostic>) {
        if !report_errors
            && let Some(answer) = self.cached_object_relation(source, target, relation)
        {
            return (answer, None);
        }
        let mut relater = Relater::new(self, relation, report_errors.then_some((source, target)));
        // Measurement only; a no-op unless `reasons::enable` was called.
        let outer = reasons::begin();
        let answer = relater.is_related_to(source, target).public_answer();
        reasons::finish(outer, answer == Ternary::Unknown);
        let diagnostic = if answer == Ternary::NotRelated {
            relater.signature_error.map(|(minimum, count)| {
                tsr_diagnostics::Diagnostic::with_args(
                    &tsr_diagnostics::messages::TARGET_SIGNATURE_PROVIDES_TOO_FEW_ARGUMENTS_EXPECTED_0_OR_MORE_BUT_GOT_1,
                    tsr_core::Span::new(0, 0),
                    [minimum.to_string(), count.to_string()],
                )
            })
        } else {
            None
        };
        (answer, diagnostic)
    }

    /// `isTypeRelatedTo`'s completed-result read for two object types
    /// (`relater.go:193`): before opening a walk, an object pair answers from
    /// the relation's results under `IntersectionStateNone`.
    ///
    /// Equivalent to the walk it skips: `recursive_type_related_to` is the
    /// only writer, it is reached only after `is_related_to_with_flags`'
    /// arms decline the pair, and those arms read nothing but the pair, the
    /// relation and the options [`crate::relation_cache::RelationResults`]
    /// validates. For two object types that walk's normalizations reduce to
    /// `get_regular_type_of_literal_type`, applied here; the variance-marker
    /// pairs it answers first never reach the store. A framed walk
    /// ([`Relater::new`]) does not read the store, and neither does this.
    fn cached_object_relation(
        &mut self,
        source: TypeId,
        target: TypeId,
        relation: Relation,
    ) -> Option<Ternary> {
        if !self.alias_evaluation_bindings.is_empty() || self.mapped_template_depth != 0 {
            return None;
        }
        let source = self.get_regular_type_of_literal_type(source);
        let target = self.get_regular_type_of_literal_type(target);
        if source == target
            || !self.type_of(source).flags.contains(TypeFlags::OBJECT)
            || !self.type_of(target).flags.contains(TypeFlags::OBJECT)
        {
            return None;
        }
        self.relation_results.validate(self.relation_options());
        Some(match self.relation_results.get(relation, (source, target, false))? {
            CachedRelation::Succeeded => Ternary::Related,
            CachedRelation::Failed => Ternary::NotRelated,
        })
    }

    /// The options [`crate::relation_cache::RelationOptions`] names.
    fn relation_options(&self) -> crate::relation_cache::RelationOptions {
        [
            self.strict_null_checks,
            self.strict_function_types,
            self.exact_optional_property_types,
            self.no_implicit_any,
        ]
    }

    /// The missing-property message `checkTypeRelatedToEx` would leave at the
    /// head of a failed top-level `source -> target` chain, or `None` where the
    /// head stays the caller's own message (TS2322/TS2345).
    ///
    /// Mirrors, for two plain object types, `propertiesRelatedTo`'s unmatched
    /// arm (`relater.go:4100`), `getUnmatchedProperties` with
    /// `requireOptionalProperties` false (`relater.go:978`; this caller asks the
    /// assignable relation), `shouldReportUnmatchedPropertyError`
    /// (`relater.go:959`) and `reportUnmatchedProperty`'s
    /// `tryElaborateArrayLikeErrors` gate (`relater.go:4345`/`:4379`).
    /// `reportRelationError` (`relater.go:4751`) then suppresses the head
    /// message because the chain's last entry names the same pair.
    ///
    /// A tuple target related to an array/tuple source, and a tuple target
    /// with variable elements, fail before the unmatched arm. Any name or
    /// modifier this port cannot certify answers `None`, so the caller keeps
    /// its own head rather than inventing a missing-property code. The caller
    /// must already hold a `NotRelated` verdict for the pair.
    pub(crate) fn unmatched_property_report(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<Vec<String>> {
        use crate::signatures::SignatureKind;
        let object_only = |flags: TypeFlags| {
            flags.contains(TypeFlags::OBJECT)
                && !flags.intersects(TypeFlags::UNION | TypeFlags::INTERSECTION)
        };
        if !object_only(self.type_of(source).flags) || !object_only(self.type_of(target).flags) {
            return None;
        }
        // A fresh object literal first meets hasExcessProperties (relater.go:
        // 2714), whose unported arms could own the failure; the reporter's
        // written-key guard (`missing_required_property`) decides those.
        if self.fresh_object_literal_types.contains(&source) {
            return None;
        }
        // reportErrorResults (relater.go:4705) appends "The 'Object' type is
        // assignable to very few other types" for the global Object source, so
        // the chain no longer ends in the missing-property message.
        if let TypeData::Named { members: Some(owner), .. } = self.type_of(source).data
            && self.global_type_symbol_with_arity("Object", 0) == Some(owner)
        {
            return None;
        }
        // reportErrorResults displays an aliased or single-base original type,
        // while the missing-property message names the normalized structure;
        // chainArgsMatch then fails and the head message stays. An alias image
        // or a generic class/interface reference that may normalize to its
        // single base (getSingleBaseForNonAugmentingSubtype) is left there.
        for side in [source, target] {
            if let TypeData::Named { members: Some(owner), .. } = self.type_of(side).data
                && self.binder.symbols().get(owner).flags.intersects(SymbolFlags::TYPE_ALIAS)
            {
                return None;
            }
            if let Some(&(owner, _)) = self.type_reference_targets.get(&side) {
                let flags = self.binder.symbols().get(owner).flags;
                if flags.intersects(SymbolFlags::TYPE_ALIAS) {
                    return None;
                }
                if flags.intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE)
                    && self.binder.symbols().get(owner).members.iter().all(|(_, &member)| {
                        !self.binder.symbols().get(member).flags.intersects(SymbolFlags::VALUE)
                    })
                    && self.base_symbols_of_ex(owner, false).is_none_or(|bases| bases.len() == 1)
                {
                    return None;
                }
            }
        }
        let source_tuple = self.tuple_element_lists.contains_key(&source)
            || self.variadic_tuple_elements.contains_key(&source);
        let source_array = self.array_reference_readonly(source);
        if self.variadic_tuple_elements.contains_key(&target) {
            return None;
        }
        let target_tuple = self.tuple_element_lists.contains_key(&target);
        if target_tuple && (source_tuple || source_array.is_some()) {
            return None;
        }
        // A tuple target's properties are its elements, `length` and the
        // inherited array members (`tuple_target_properties`); a non-array
        // source meets them in propertiesRelatedTo's general walk.
        if target_tuple && !source_tuple && source_array.is_none() {
            let properties = self.tuple_target_properties(target)?;
            let source_names = self.get_property_names_of_type(source)?;
            let missing: Vec<String> = properties
                .into_iter()
                .filter(|(name, optional)| {
                    !optional
                        && !source_names.contains(name)
                        && self.get_type_of_property_of_type(source, name).is_none()
                })
                .map(|(name, _)| name)
                .collect();
            // tryElaborateArrayLikeErrors: a tuple target elaborates several
            // missing properties only for an array source.
            return (missing.len() == 1).then_some(missing);
        }
        let names = self.get_property_names_of_type(target)?;
        self.get_property_names_of_type(source)?;
        let mut relater = Relater::new(self, Relation::Assignable, None);
        let mut missing = Vec::new();
        for name in names {
            if relater.checker.get_type_of_property_of_type(source, &name).is_some() {
                continue;
            }
            let (optional, _) = relater.property_flags(target, &name)?;
            if !optional {
                missing.push(name);
            }
        }
        if missing.is_empty() {
            return None;
        }
        // shouldReportUnmatchedPropertyError: a source that is only
        // signatures reports the unmatched property only against a target
        // with a signature kind the source also has.
        let source_calls = self.signatures_of_type_kind(source, SignatureKind::Call)?;
        let source_constructs = self.signatures_of_type_kind(source, SignatureKind::Construct)?;
        if (!source_calls.is_empty() || !source_constructs.is_empty())
            && self.get_property_names_of_type(source)?.is_empty()
        {
            let target_calls = self.signatures_of_type_kind(target, SignatureKind::Call)?;
            let target_constructs =
                self.signatures_of_type_kind(target, SignatureKind::Construct)?;
            if !((!target_calls.is_empty() && !source_calls.is_empty())
                || (!target_constructs.is_empty() && !source_constructs.is_empty()))
            {
                return None;
            }
        }
        if missing.len() == 1 {
            return Some(missing);
        }
        let (elaborates, _) = self.try_elaborate_array_like_errors(source, target);
        elaborates.then_some(missing)
    }

    /// `tryElaborateArrayLikeErrors` (`relater.go:4379`): its answer, and
    /// whether with `reportErrors` it reports TS4104 (`The type '{0}' is
    /// 'readonly' and cannot be assigned to the mutable type '{1}'`) — a
    /// readonly tuple or `ReadonlyArray` source against a mutable array or
    /// tuple target. Pure reads of the tuple/array reference tables.
    pub(crate) fn try_elaborate_array_like_errors(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> (bool, bool) {
        let is_tuple = |id: TypeId| {
            self.tuple_element_lists.contains_key(&id)
                || self.variadic_tuple_elements.contains_key(&id)
        };
        let target_array = self.array_reference_readonly(target);
        let target_tuple = is_tuple(target);
        // isMutableArrayOrTuple (checker.go)
        let target_mutable_array_or_tuple =
            target_array == Some(false) || (target_tuple && !self.tuple_is_readonly(target));
        if is_tuple(source) {
            if self.tuple_is_readonly(source) && target_mutable_array_or_tuple {
                return (false, true);
            }
            return (target_tuple || target_array.is_some(), false);
        }
        let source_array = self.array_reference_readonly(source);
        if source_array == Some(true) && target_mutable_array_or_tuple {
            return (false, true);
        }
        if target_tuple {
            return (source_array.is_some(), false);
        }
        (true, false)
    }

    /// `reportErrorResults`' (`relater.go:4705`) array-like arm: for two
    /// object types it calls `tryElaborateArrayLikeErrors` with
    /// `reportErrors`, and `reportRelationError` (`relater.go:4751`) then
    /// suppresses the head because the chain's TS4104 names the same pair.
    /// The caller must already hold a `NotRelated` verdict for the pair.
    pub(crate) fn readonly_to_mutable_array_like(&self, source: TypeId, target: TypeId) -> bool {
        let object_only = |flags: TypeFlags| {
            flags.contains(TypeFlags::OBJECT)
                && !flags.intersects(TypeFlags::UNION | TypeFlags::INTERSECTION)
        };
        object_only(self.type_of(source).flags)
            && object_only(self.type_of(target).flags)
            && self.try_elaborate_array_like_errors(source, target).1
    }

    /// `getPropertiesOfType` of a tuple target, as `(name, optional)` in
    /// upstream's order: the leading fixed elements (`"0"`, `"1"`, …,
    /// optional where the element is), `length`, then the string-named
    /// members of the global `Array` (or `ReadonlyArray`) its reference
    /// inherits, all required. Symbol-named members are left out; neither
    /// caller reads them. `None` for a non-tuple or a lib without the base.
    pub(crate) fn tuple_target_properties(
        &mut self,
        target: TypeId,
    ) -> Option<Vec<(String, bool)>> {
        let (elements, readonly) = if let Some(tuple) = self.variadic_tuple_elements.get(&target) {
            tuple.clone()
        } else {
            let (types, readonly) = self.tuple_element_lists.get(&target)?.clone();
            let mask = self.tuple_optional_masks.get(&target).cloned();
            (
                types
                    .iter()
                    .enumerate()
                    .map(|(index, &t)| crate::tuples::TupleElement {
                        r#type: t,
                        spread: false,
                        optional: mask
                            .as_ref()
                            .and_then(|mask| mask.get(index))
                            .copied()
                            .unwrap_or(false),
                        label: None,
                    })
                    .collect(),
                readonly,
            )
        };
        let array = self.global_type_symbol(if readonly { "ReadonlyArray" } else { "Array" })?;
        let array = self.binder.merged_symbol(array);
        let mut properties: Vec<(String, bool)> = elements
            .iter()
            .take_while(|element| !element.spread)
            .enumerate()
            .map(|(index, element)| (index.to_string(), element.optional))
            .collect();
        properties.push(("length".to_string(), false));
        for (name, &member) in &self.binder.symbols().get(array).members {
            let name = name.to_string();
            if self
                .binder
                .symbols()
                .get(member)
                .flags
                .intersects(SymbolFlags::PROPERTY | SymbolFlags::METHOD)
                && !name.starts_with("__@")
                && !name.starts_with('[')
                && !properties.iter().any(|(seen, _)| seen == &name)
            {
                properties.push((name, false));
            }
        }
        Some(properties)
    }

    /// `isArrayType` (`checker.go`): `Some(readonly)` for a reference to the
    /// global `Array` (`false`) or `ReadonlyArray` (`true`).
    fn array_reference_readonly(&self, id: TypeId) -> Option<bool> {
        let (target, _) = self.type_reference_targets.get(&id)?;
        let target = self.binder.merged_symbol(*target);
        [("Array", false), ("ReadonlyArray", true)].into_iter().find_map(|(name, readonly)| {
            self.global_type_symbol(name)
                .is_some_and(|symbol| self.binder.merged_symbol(symbol) == target)
                .then_some(readonly)
        })
    }

    pub(crate) fn compare_signature_ternary(
        &mut self,
        source: &crate::signatures::Signature,
        target: &crate::signatures::Signature,
    ) -> Option<Ternary> {
        let mut relater = Relater::new(self, Relation::Assignable, None);
        relater
            .one_signature_related_to(source, target, false, false, false)
            .map(RelationResult::public_answer)
    }
}

impl<'c, 'a, 'n> Relater<'c, 'a, 'n> {
    /// A walk with no active assumptions, as `checkTypeRelatedToEx` starts
    /// one (`relater.go:257`).
    ///
    /// A walk opened while a conditional-alias evaluation frame
    /// (`alias_evaluation_bindings`) or a mapped-template frame
    /// (`mapped_template_depth`) is active reads member and template types
    /// through that frame, so its answers are not the frame-free pair's.
    /// Native has neither frame (it instantiates instead); such a walk keeps
    /// its completed results walk-local, as every walk did before
    /// `tsr-2zk.902`, and neither reads nor publishes the checker store.
    fn new(
        checker: &'c mut Checker<'a, 'n>,
        relation: Relation,
        diagnostic_pair: Option<(TypeId, TypeId)>,
    ) -> Self {
        let framed =
            !checker.alias_evaluation_bindings.is_empty() || checker.mapped_template_depth != 0;
        if !framed {
            let options = checker.relation_options();
            checker.relation_results.validate(options);
        }
        Relater {
            checker,
            relation,
            local_results: framed.then(FxHashMap::default),
            maybe_keys: Vec::new(),
            maybe_keys_set: FxHashSet::default(),
            depth: 0,
            source_stack: Vec::new(),
            target_stack: Vec::new(),
            expanding: (false, false),
            intersection_target: false,
            diagnostic_pair,
            signature_error: None,
        }
    }

    /// `relation.get(id)` (`relater.go:3068`).
    fn cached_result(&self, key: RelationKey) -> Option<CachedRelation> {
        match &self.local_results {
            Some(local) => local.get(&key).copied(),
            None => self.checker.relation_results.get(self.relation, key),
        }
    }

    /// `relation.set(id, ...)` (`relater.go:3162`, `:3173`).
    fn publish_result(&mut self, key: RelationKey, result: CachedRelation) {
        match &mut self.local_results {
            Some(local) => {
                local.insert(key, result);
            }
            None => self.checker.relation_results.set(self.relation, key, result),
        }
    }
}

impl Relater<'_, '_, '_> {
    /// The body of `isTypeRelatedTo`, minus the entry-point bookkeeping.
    ///
    /// Ported from `Checker.isTypeRelatedTo` (`internal/checker/relater.go`).
    fn is_related_to(&mut self, source: TypeId, target: TypeId) -> RelationResult {
        self.is_related_to_with_flags(source, target, RecursionFlags::BOTH)
    }

    fn is_related_to_with_flags(
        &mut self,
        source: TypeId,
        target: TypeId,
        flags: RecursionFlags,
    ) -> RelationResult {
        // isPerformingExcessPropertyChecks / isPerformingCommonPropertyChecks
        // both require intersectionState&IntersectionStateTarget == 0.
        let check_excess = !self.intersection_target;
        // Upstream reduces a fresh literal to its regular form on both sides
        // before comparing identity, so that `"a"` fresh and `"a"` regular are
        // one type here even though they are two interned types.
        let source = self.checker.get_regular_type_of_literal_type(source);
        if let Some([sup, sub, other]) = self.checker.variance_markers
            && [sup, sub, other].contains(&source)
            && [sup, sub, other].contains(&target)
        {
            return if source == target || (source == sub && target == sup) {
                RelationResult::Related
            } else {
                RelationResult::NotRelated
            };
        }
        let target = self.checker.get_regular_type_of_literal_type(target);
        // getNormalizedType's substitution arm (`checker.go:27884`): a
        // `NoInfer<T>` normalizes to its base type on either side
        // (`getSubstitutionIntersection`, `checker.go:26828`).
        let source = self.checker.no_infer_base_type(source).unwrap_or(source);
        let target = self.checker.no_infer_base_type(target).unwrap_or(target);
        if source == target {
            return RelationResult::Related;
        }
        // getNormalizedType's getSimplifiedIndexedAccessType (checker.go:27915),
        // reading on the source side and writing on the target side. The
        // simplified pair is normalized again, as native's loop does.
        // `declared.rs` mints an OBJECT-flagged `Named` image for an alias
        // reference whose symbol is the alias. Native's alias instantiation
        // is its body (getTypeFromTypeAliasReference), so a body that is not
        // an object is related in its place: `Keyof<Registry>` is `"a" | "b"`
        // (`docs/parity/notes/r5-relater6.md` §2). An object body keeps the
        // image, which the structural arms read.
        if let Some(body) = self.non_object_alias_image_body(source) {
            return self.is_related_to_with_flags(body, target, flags);
        }
        if let Some(body) = self.non_object_alias_image_body(target) {
            return self.is_related_to_with_flags(source, body, flags);
        }
        let simplified_source = self.simplified_indexed_access(source, false);
        let simplified_target = self.simplified_indexed_access(target, true);
        if simplified_source != source || simplified_target != target {
            return self.is_related_to_with_flags(simplified_source, simplified_target, flags);
        }
        // getNormalizedType reduces source intersections before the simple
        // relation (relater.go:2625, checker.go:28041). Apparent constituents
        // expose generic constraints to the existing whole-never certification;
        // a never-valued property alone is not such a proof. Keep the written
        // type and property readers unchanged outside this comparison.
        let source = if self.checker.type_of(source).flags.contains(TypeFlags::INTERSECTION) {
            let apparent = self.checker.apparent_type(source);
            if self.checker.intersection_has_never_discriminant(apparent) {
                self.checker.intrinsics.never
            } else {
                source
            }
        } else {
            source
        };
        // isRelatedToWorker fast-paths a parameter's exact constraint before
        // decomposing a target union or considering simple negative verdicts
        // (relater.go:2640). This includes a written `never` constraint.
        if self.checker.type_of(source).flags.contains(TypeFlags::TYPE_PARAMETER)
            && self.checker.type_parameter_constraint(source) == Some(target)
        {
            return RelationResult::Related;
        }
        // A deferred conditional type is not an object, whatever flags its
        // print-only mint carries (`declared.rs` §906 mints an inline one as
        // OBJECT). isSimpleTypeRelatedTo's only arms for it are the any,
        // unknown and never ones; every other pair is structuredTypeRelatedTo's
        // conditional arms (relater.go:3540, :3721).
        // A conditional whose check type is not generic is not native's
        // deferred type: native resolves every such conditional, so one
        // reaching the relater is a conditional this port failed to evaluate
        // (`StepSelection<QuickPickStep<QuickPickItem>>` in
        // `generatorYieldContextualType`). The conditional arms do not apply
        // to it; it keeps the relater's other roads.
        if self.is_deferred_conditional(source) || self.is_deferred_conditional(target) {
            // getNormalizedType's getSimplifiedType (checker.go:28006) on
            // either side: `T extends U ? T : never` and its mirror reduce to
            // `T` or `never` when the check is decided either way.
            let simplified_source = self.simplified_conditional(source);
            let simplified_target = self.simplified_conditional(target);
            let (Some(simplified_source), Some(simplified_target)) =
                (simplified_source, simplified_target)
            else {
                return RelationResult::Unknown;
            };
            if simplified_source != source || simplified_target != target {
                return self.is_related_to_with_flags(simplified_source, simplified_target, flags);
            }
            let s = self.checker.type_of(source).flags;
            let t = self.checker.type_of(target).flags;
            if t.intersects(TypeFlags::ANY)
                || s.intersects(TypeFlags::NEVER)
                || (t.intersects(TypeFlags::UNKNOWN)
                    && !(self.relation == Relation::StrictSubtype && s.intersects(TypeFlags::ANY)))
                || (s.intersects(TypeFlags::ANY)
                    && matches!(self.relation, Relation::Assignable | Relation::Comparable)
                    && !t.intersects(TypeFlags::NEVER))
            {
                return RelationResult::Related;
            }
            // Stated decline: a conditional minted with OBJECT flags. Its
            // consumers test genericity by flags (`declared.rs`'s
            // conditional evaluator defers only an instantiable check;
            // `indexed_access_index_is_generic`), so a decided answer about
            // it is acted on as if it were concrete: `Distributive<[T]
            // extends [never] ? X : never>` would evaluate to `X`. It stays
            // undecided until the mint carries CONDITIONAL
            // (`docs/parity/notes/r5-relater4.md` §2).
            //
            // Likewise a mapped template's conditional (an `as` clause such as
            // `P extends \`_${string}\` ? P : never`): deciding it makes
            // `mapped_indexed_access_constraint` treat the mapped type as
            // filtering, and the base constraint this port then computes for
            // `keyof Mapped<K>` is the whole key domain instead of the
            // filtered keys (`mappedTypeConstraints2`).
            if self.is_object_flagged_conditional(source)
                || self.is_object_flagged_conditional(target)
                || self.checker.mapped_conditionals.contains_key(&source)
                || self.checker.mapped_conditionals.contains_key(&target)
            {
                return RelationResult::Unknown;
            }
            if !self.is_conditional(source)
                && !s.intersects(TypeFlags::UNION | TypeFlags::INTERSECTION)
                && self.is_simple_type_related_to(source, target) == Some(true)
            {
                return RelationResult::Related;
            }
            return self.recursive_type_related_to(source, target, flags);
        }
        // `relater.go:181`/`:2661`: under the comparable relation the simple
        // arms are also tried REVERSED (target against source) first, unless
        // the target is `never`. §750.
        if matches!(self.relation, Relation::Comparable)
            && !self.checker.type_of(target).flags.intersects(TypeFlags::NEVER)
            && self.is_simple_type_related_to(target, source) == Some(true)
        {
            return RelationResult::Related;
        }
        match self.is_simple_type_related_to(source, target) {
            Some(true) => return RelationResult::Related,
            Some(false) => return RelationResult::NotRelated,
            None => {}
        }
        // isRelatedToWorker / hasExcessProperties (relater.go:2667,2714).
        // A fresh source must first satisfy the target's accepted property set.
        if check_excess
            && self.checker.fresh_object_literal_types.contains(&source)
            && (self.checker.no_implicit_any || !self.checker.js_literal_types.contains(&target))
            && !(matches!(self.relation, Relation::Assignable | Relation::Comparable)
                && self.target_exempts_excess_properties(target))
            && self.checker.fresh_literal_has_excess_property(source, target)
        {
            return RelationResult::NotRelated;
        }
        // isPerformingCommonPropertyChecks && !hasCommonProperties
        // (isRelatedToEx, relater.go:2676): a weak target that knows none of
        // the source's properties. Not under IntersectionStateTarget, and
        // under the comparable relation only for a unit source.
        if check_excess
            && (!matches!(self.relation, Relation::Comparable)
                || self.checker.type_of(source).flags.intersects(TypeFlags::UNIT))
            && self.checker.fails_common_property_check(source, target)
        {
            return RelationResult::NotRelated;
        }
        // anyFunctionType has no properties, and function expressions have
        // no own property requirements. Their call-signature relation is the
        // wildcard rule even when no symbol member table is attached.
        if self.checker.any_function_type == Some(source) && self.is_pure_signature_type(target) {
            return RelationResult::Related;
        }
        if self.checker.any_function_type == Some(target) && self.is_pure_signature_type(source) {
            return RelationResult::NotRelated;
        }
        if self.is_pure_signature_type(source) && self.is_pure_signature_type(target) {
            return self.recursive_type_related_to(source, target, flags);
        }
        // §17 (`checker-notes-assign.md`): both-own-private class pairs are
        // nominal — NotRelated by the private-identity rule, decided from
        // syntax. The syntax shortcut only holds for classes without
        // heritage: a derived class inherits its base's private members, whose
        // value declarations are the base's (propertyRelatedTo,
        // relater.go:4270), so `Derived extends Base` relates to `Base` even
        // when both declare privates. Such pairs take the structural walk,
        // whose privacy arm compares declarations.
        if !self.class_declares_heritage(source)
            && !self.class_declares_heritage(target)
            && let Some(answer) = self.checker.nominal_class_pair_verdict(source, target)
        {
            return if answer { RelationResult::Related } else { RelationResult::NotRelated };
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
        // required-property comparison already proves a rejection under every
        // relation: `propertiesRelatedTo` runs before `indexSignaturesRelatedTo`
        // in structuredTypeRelatedToWorker (relater.go), so `number -> any[]`
        // fails on the missing `length` whatever the index infos say.
        // A deferred `keyof` target is not an object type, whatever flags its
        // written mint carries: its arm (relater.go:3489) compares the
        // primitive itself.
        if s.intersects(TypeFlags::PRIMITIVE)
            && !s.intersects(TypeFlags::NULLABLE | TypeFlags::VOID)
            && t.intersects(TypeFlags::OBJECT)
            && !self.checker.deferred_keyof_operands.contains_key(&target)
        {
            let apparent = self.checker.apparent_type(source);
            if apparent != source {
                // A tuple's only index info is the number index its `Array`
                // base contributes (`getTupleBaseType`); its element
                // properties and `length` are judged on the tuple road below.
                if !target_tuple
                    && self
                        .checker
                        .get_index_infos_of_type(target)
                        .is_some_and(|infos| !infos.is_empty())
                {
                    // propertiesRelatedTo needs only the target's names: an
                    // enum object (`typeof E`, which carries a reverse-mapping
                    // index) has no member table here but enumerates them.
                    return if self.has_members(apparent)
                        && (self.has_members(target)
                            || self.checker.get_property_names_of_type(target).is_some())
                        && self.properties_related_to(apparent, target)
                            == RelationResult::NotRelated
                    {
                        RelationResult::NotRelated
                    } else {
                        RelationResult::Unknown
                    };
                }
                return self.is_related_to(apparent, target);
            }
        }
        // isSimpleTypeRelatedTo relates an `any` source to anything only
        // under the assignable and comparable relations (relater.go:261); the
        // `any`/`unknown` targets fired above. Under the subtype relations
        // structuredTypeRelatedToWorker has no arm relating `any` to a type
        // parameter (relater.go:3423) or to an object target: its apparent
        // type is `any`, which is not an object, so the structural arm
        // (relater.go:3864) does not run. A generic mapped target keeps its
        // keyof-based arm (relater.go:3593), and a qualified alias mint's
        // flags are not evidence.
        if s.contains(TypeFlags::ANY)
            && matches!(self.relation, Relation::Subtype | Relation::StrictSubtype)
            && (t.contains(TypeFlags::TYPE_PARAMETER)
                || (t.contains(TypeFlags::OBJECT)
                    && !self.checker.mapped_types.contains_key(&target)
                    && !self.is_generic_mapped_target(target)
                    && !self.is_qualified_alias_mint(target)))
        {
            return RelationResult::NotRelated;
        }
        // structuredTypeRelatedTo compares the non-primitive `object` through
        // its apparent type, the empty object type (getApparentType,
        // checker.go), so a target requiring a property rejects it
        // (`nonPrimitiveAssignError`). Only a decided answer is taken; an
        // index-signature target keeps the existing undecided path.
        // structuredTypeRelatedTo compares the non-primitive `object` through
        // its apparent type, the empty object type (getApparentType,
        // checker.go); propertiesRelatedTo then rejects a target requiring a
        // property the empty object cannot supply, Object's members included
        // (`nonPrimitiveAssignError`). Only that definite negative is taken
        // here; every other `object` pair keeps its existing path.
        if s.contains(TypeFlags::NON_PRIMITIVE)
            && t.contains(TypeFlags::OBJECT)
            && let Some(table) = self.checker.relation_property_table(target)
        {
            let empty = self.checker.intrinsics.empty_object;
            if table.iter().any(|(name, optional)| {
                !optional && self.checker.get_type_of_property_of_type(empty, name).is_none()
            }) {
                return RelationResult::NotRelated;
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
            || (self.is_structural_tuple_source(source)
                && t.contains(TypeFlags::OBJECT)
                && self.has_members(target))
            || ((self.has_members(source) || self.is_structural_tuple_source(source))
                && self.is_memberless_object_intrinsic(target))
            || s.intersects(TypeFlags::TYPE_PARAMETER | TypeFlags::INDEXED_ACCESS)
            || self.checker.deferred_keyof_operands.contains_key(&source)
            || self.checker.deferred_keyof_operands.contains_key(&target)
            || t.contains(TypeFlags::STRING_MAPPING)
            || (t.contains(TypeFlags::TEMPLATE_LITERAL)
                && s.intersects(
                    TypeFlags::STRING_LITERAL | TypeFlags::TEMPLATE_LITERAL | TypeFlags::STRING,
                ))
        {
            return self.recursive_type_related_to(source, target, flags);
        }
        // A generic mapped type captured lazily (`ensure_mapped_type_info`)
        // has no member table, so the structural gate above does not route it
        // to `structured_type_related_to_worker`; its target arms apply here.
        self.checker.ensure_mapped_type_info(target);
        if self.is_generic_mapped_target(target) {
            self.checker.ensure_mapped_type_info(source);
            if let Some(result) = self.generic_mapped_target_related_to(source, target)
                && result != RelationResult::NotRelated
            {
                return result;
            }
            if self.mapped_modifiers_reject(source, target) {
                return RelationResult::NotRelated;
            }
            if let Some(result) = self.generic_mapped_target_default(source, target) {
                return result;
            }
        }
        // propertiesRelatedTo (relater.go:4100) for a tuple target and a
        // source that is neither an array nor a tuple: the tuple's
        // properties are compared one by one, and only a definite failure is
        // taken from that walk here.
        if let Some(result) = self.non_array_source_tuple_target(source, target) {
            return result;
        }
        if let Some(result) = self.tuple_source_non_array_target(source, target) {
            return result;
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
            return RelationResult::Related;
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
        // isRelatedToEx (relater.go:2605) answers an object source against a
        // primitive target by isSimpleTypeRelatedTo alone, whose only object
        // arms are the any/unknown/never/nonprimitive targets above; none
        // relates an object to a `unique symbol`.
        if s.intersects(TypeFlags::OBJECT)
            && (self.flag_decidable(target) || t == TypeFlags::UNIQUE_ES_SYMBOL)
        {
            return RelationResult::NotRelated;
        }
        // In strict mode unknown includes null and undefined, so it cannot
        // inhabit an object type, including the empty anonymous object.
        // This also decides an unconstrained parameter's base-constraint check.
        if self.checker.strict_null_checks
            && s.contains(TypeFlags::UNKNOWN)
            && t.intersects(TypeFlags::OBJECT | TypeFlags::NON_PRIMITIVE)
        {
            return RelationResult::NotRelated;
        }
        // The type-parameter target arm for a mapped source (relater.go:3423).
        if t.contains(TypeFlags::TYPE_PARAMETER)
            && s.contains(TypeFlags::OBJECT)
            && !s.intersects(TypeFlags::INSTANTIABLE | composite)
        {
            self.checker.ensure_mapped_type_info(source);
            if self.checker.mapped_types.contains_key(&source) {
                return self.mapped_source_type_parameter_target(source, target);
            }
        }
        // A concrete object, primitive or unknown cannot inhabit an arbitrary
        // target parameter. The simple relation already handles any/never and
        // loose nullability (structuredTypeRelatedToWorker, relater.go).
        // Generic mapped types have a separate target-parameter relation
        // (relater.go:3423), which remains outside this arm.
        if t.contains(TypeFlags::TYPE_PARAMETER)
            && s.intersects(TypeFlags::OBJECT | TypeFlags::PRIMITIVE | TypeFlags::UNKNOWN)
            && matches!(
                self.relation,
                Relation::Assignable | Relation::Subtype | Relation::StrictSubtype
            )
            && !self.checker.mapped_types.contains_key(&source)
        {
            return RelationResult::NotRelated;
        }
        // The same arm under comparability. isRelatedToEx's reverse simple
        // check (relater.go:2607) relates an `unknown` source to any target
        // (`isSimpleTypeRelatedTo(T, unknown)`), so only object and primitive
        // sources are decided here; the comparable carve-out at
        // relater.go:3435 concerns a type-parameter *source*.
        if t.contains(TypeFlags::TYPE_PARAMETER)
            && s.intersects(TypeFlags::OBJECT | TypeFlags::PRIMITIVE)
            && !s.intersects(TypeFlags::UNKNOWN)
            && self.relation == Relation::Comparable
            && !self.checker.mapped_types.contains_key(&source)
        {
            return RelationResult::NotRelated;
        }
        // structuredTypeRelatedToWorker's indexed-access target arm
        // (relater.go:3443) for a source that is not itself a type variable:
        // under the assignable/comparable relations `S -> T[K]` relates only
        // through the write constraint `getIndexedAccessTypeOrUndefined(
        // baseConstraintOrType(T), baseConstraintOrType(K), Writing |
        // (NoIndexSignatures when T had a constraint))`, and only when neither
        // base is still generic; no later arm relates a concrete object,
        // primitive or `unknown` source to an indexed access (the object arm
        // needs an object target). A generic base is a definite failure, and
        // so is a constrained object read through a key with no property-name
        // constituent (`keyof T`'s base is `string | number | symbol`): with
        // index signatures excluded no member can be selected, so the
        // constraint is nil. Any other write constraint is not built here and
        // the pair stays undecided.
        if t.contains(TypeFlags::INDEXED_ACCESS)
            && s.intersects(TypeFlags::OBJECT | TypeFlags::PRIMITIVE | TypeFlags::UNKNOWN)
            && matches!(
                self.relation,
                Relation::Assignable | Relation::Subtype | Relation::StrictSubtype
            )
            && !self.checker.mapped_types.contains_key(&source)
            && !self.is_qualified_alias_mint(source)
            && let Some(&(object, index, _)) =
                self.checker.deferred_indexed_access_types.get(&target)
        {
            if matches!(self.relation, Relation::Assignable) {
                return self.indexed_access_write_constraint_related_to(source, object, index);
            }
            return RelationResult::NotRelated;
        }
        // structuredTypeRelatedToWorker (relater.go:3261): under strict null
        // checks `null`/`undefined`/`void` fail isSimpleTypeRelatedTo against
        // an object target, their apparent type stays primitive, and no
        // object-target arm accepts a non-object source. A generic mapped
        // target keeps its own keyof-based arm (relater.go:3593), which this
        // port leaves undecided under the assignable/comparable relations.
        //
        // A `Named` image whose symbol is a type alias is not an object type
        // upstream: it is `declared.rs`'s print-only mint for a qualified alias
        // reference (`N.Alias`), which may stand for a primitive such as
        // `undefined`. Its flags are not evidence, so it stays undecided.
        if self.checker.strict_null_checks
            && s.intersects(TypeFlags::NULLABLE | TypeFlags::VOID)
            && t.contains(TypeFlags::OBJECT)
            && (matches!(self.relation, Relation::Subtype | Relation::StrictSubtype)
                || (!self.is_generic_mapped_target(target)
                    && !self.is_qualified_alias_mint(target)))
        {
            return RelationResult::NotRelated;
        }
        // A template always inhabits the string domain. Generic holes do not
        // make it overlap a decidable non-string primitive.
        if s.intersects(TypeFlags::TEMPLATE_LITERAL | TypeFlags::STRING_MAPPING)
            && self.flag_decidable(target)
            && !t.intersects(TypeFlags::STRING_LIKE)
        {
            return RelationResult::NotRelated;
        }
        // structuredTypeRelatedToWorker's template-source arm (relater.go:3772):
        // against a non-object, non-template target only a distinct base
        // constraint can relate; no target-side arm applies to a decidable
        // target, so the failed constraint is the answer.
        if s.contains(TypeFlags::TEMPLATE_LITERAL) && self.flag_decidable(target) {
            if let Some(constraint) = self.checker.base_constraint_of_type(source)
                && constraint != source
            {
                let result = self.is_related_to(constraint, target);
                if result != RelationResult::NotRelated {
                    return result;
                }
            }
            return RelationResult::NotRelated;
        }
        if self.flag_decidable(source) && self.flag_decidable(target) {
            RelationResult::NotRelated
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
            RelationResult::Unknown
        }
    }

    /// `isValidOverrideOf` (`checker.go:11928`) for a non-synthetic source
    /// property against a protected target property: the source must be
    /// declared in a class that has the target's declaring class as a base
    /// (`isPropertyInClassDerivedFrom`, `hasBaseType`). A source declared
    /// outside any class (an object literal, an interface's own member) is
    /// not a valid override. A protected target whose declaring class is not
    /// found stays `Unknown`.
    fn is_valid_override_of(
        &mut self,
        source_property: SymbolId,
        target_property: SymbolId,
    ) -> RelationResult {
        let Some(base) = self.declaring_class(target_property) else {
            return RelationResult::Unknown;
        };
        match self.declaring_class(source_property) {
            Some(derived) if self.class_has_base(derived, base, 0) => RelationResult::Related,
            _ => RelationResult::NotRelated,
        }
    }

    /// `getDeclaringClass` (`checker.go:11920`): the class whose member
    /// table holds `property`, read off its declaration (a class element, or
    /// a constructor parameter property).
    fn declaring_class(&self, property: SymbolId) -> Option<SymbolId> {
        let declaration = self.checker.binder.symbols().get(property).value_declaration?;
        let mut container = self.checker.nodes.parent(declaration)?;
        if matches!(
            self.checker.node_map.get(container),
            Some(tsr_ast::Node::ConstructorDeclaration(_))
        ) {
            container = self.checker.nodes.parent(container)?;
        }
        if !matches!(
            self.checker.node_map.get(container),
            Some(tsr_ast::Node::ClassDeclaration(_) | tsr_ast::Node::ClassExpression(_))
        ) {
            return None;
        }
        let class = self.checker.binder.symbol_of(container)?;
        Some(self.checker.binder.merged_symbol(class))
    }

    /// `hasBaseType` (`checker.go:19551`) between two class symbols, over
    /// `get_base_types`. The depth bound stands in for native's resolution
    /// guard on a circular `extends` chain.
    fn class_has_base(&mut self, class: SymbolId, base: SymbolId, depth: usize) -> bool {
        if class == base {
            return true;
        }
        if depth > 64 {
            return false;
        }
        for base_type in self.checker.get_base_types(class) {
            let symbol = match self.checker.type_reference_targets.get(&base_type) {
                Some(&(symbol, _)) => Some(symbol),
                None => match self.checker.type_of(base_type).data {
                    TypeData::Named { members: Some(symbol), .. } => Some(symbol),
                    _ => None,
                },
            };
            if let Some(symbol) = symbol
                && self.class_has_base(self.checker.binder.merged_symbol(symbol), base, depth + 1)
            {
                return true;
            }
        }
        false
    }

    /// The evaluated bodies of two instantiations of one type alias whose
    /// source body is neither an Object nor a Conditional type
    /// (`structuredTypeRelatedToWorker`'s alias-variance gate,
    /// `relater.go:3392`). `None` for a class or interface reference, an
    /// Object/Conditional body, or a body that does not evaluate; those keep
    /// the variance road. The bodies come from `evaluate_alias_body`'s
    /// existing `(symbol, arguments)` cache; nothing is published here.
    ///
    /// An INTERSECTION body also keeps the variance road, a stated
    /// divergence: native relates it structurally, but this relater answers
    /// `Unknown` for intersection constituents with no members table
    /// (`typeof Class<T>`, reasons row 3), which loses
    /// `aliasInstantiationExpressionGenericIntersectionNoCrash2`'s TS2352
    /// (`docs/parity/notes/r5-relater3.md` §2).
    fn non_object_alias_bodies(
        &mut self,
        symbol: SymbolId,
        source_arguments: &[TypeId],
        target_arguments: &[TypeId],
    ) -> Option<(TypeId, TypeId)> {
        if !self.checker.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_ALIAS) {
            return None;
        }
        let source_body = self.checker.evaluate_alias_body(symbol, source_arguments)?;
        if self
            .checker
            .type_of(source_body)
            .flags
            .intersects(TypeFlags::OBJECT | TypeFlags::CONDITIONAL | TypeFlags::INTERSECTION)
        {
            return None;
        }
        let target_body = self.checker.evaluate_alias_body(symbol, target_arguments)?;
        Some((source_body, target_body))
    }

    /// `structuredTypeRelatedToWorker`'s generic-mapped-target arm
    /// (`relater.go:3593`): is `source` related to `{ [P in Q]: T }` or
    /// `{ [P in Q as R]: T }`?
    ///
    /// - `{ [P in Q]: S[P] }` relates `S` outright;
    /// - otherwise, for a source that is not itself a generic mapped type, `Q`
    ///   (or `R`) must relate to `keyof S` — for a `?` target some key must
    ///   be common — and `S[P]` (or the fast path `S -> Obj` for a template
    ///   `Obj[P]`) must relate to the template.
    ///
    /// `None` when the arm does not apply. A `NotRelated` answer is the arm
    /// failing; the caller continues with the remaining arms, as upstream
    /// restores its error state and falls through. `keyof S` here is
    /// `resolved_keyof_type`, which (unlike `IndexFlagsNoIndexSignatures`)
    /// would include index-signature keys, so a source carrying index
    /// signatures is left to the remaining arms.
    fn generic_mapped_target_related_to(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<RelationResult> {
        if !self.is_generic_mapped_target(target) {
            return None;
        }
        let info = self.checker.mapped_types.get(&target).cloned()?;
        // MappedTypeModifiersExcludeOptional: a `-?` target skips the arm.
        if info.optionality == Some(false) {
            return None;
        }
        let keys_remapped = info.name_type.is_some();
        let template = self.checker.mapped_template_type(&info);
        if !keys_remapped
            && let Some(&(object, index, _)) =
                self.checker.deferred_indexed_access_types.get(&template)
            && object == source
            && index == info.parameter
        {
            return Some(RelationResult::Related);
        }
        // `!isGenericMappedType(source)`: a generic mapped source skips the
        // arm; one this port cannot classify leaves it undecided.
        match self.is_generic_mapped_type(source) {
            Some(false) => {}
            Some(true) => return None,
            None => return Some(RelationResult::Unknown),
        }
        let target_keys = info.name_type.unwrap_or(info.constraint);
        let source_keys = self.keyof_without_index_signatures(source);
        let Some(source_keys) = source_keys else {
            return Some(RelationResult::Unknown);
        };
        let include_optional = info.optionality == Some(true);
        let filtered = if include_optional {
            let filtered = self.checker.get_intersection_type(&[target_keys, source_keys], None);
            if self.checker.type_of(filtered).flags.intersects(TypeFlags::NEVER) {
                return Some(RelationResult::NotRelated);
            }
            Some(filtered)
        } else {
            let keys = self.is_related_to(target_keys, source_keys);
            if keys == RelationResult::NotRelated {
                return Some(RelationResult::NotRelated);
            }
            if keys == RelationResult::Unknown {
                return Some(RelationResult::Unknown);
            }
            None
        };
        // extractTypesOfKind(templateType, ^TypeFlagsNullable).
        let non_null = match &self.checker.type_of(template).data {
            TypeData::Union { types, .. } => {
                let parts: Vec<TypeId> = types
                    .iter()
                    .copied()
                    .filter(|&part| {
                        !self.checker.type_of(part).flags.intersects(TypeFlags::NULLABLE)
                    })
                    .collect();
                self.checker.get_union_type(&parts)
            }
            _ => template,
        };
        if !keys_remapped
            && let Some(&(object, index, _)) =
                self.checker.deferred_indexed_access_types.get(&non_null)
            && index == info.parameter
        {
            return Some(self.is_related_to_with_flags(source, object, RecursionFlags::TARGET));
        }
        let indexing = if keys_remapped {
            filtered.unwrap_or(target_keys)
        } else if let Some(filtered) = filtered {
            self.checker.get_intersection_type(&[filtered, info.parameter], None)
        } else {
            info.parameter
        };
        let Some(indexed) = self.checker.resolved_indexed_access_type(source, indexing, false)
        else {
            return Some(RelationResult::Unknown);
        };
        Some(self.is_related_to(indexed, template))
    }

    /// `getIndexTypeEx(source, IndexFlagsNoIndexSignatures)`
    /// (`checker.go:26684`) for a source the generic-mapped-target arm
    /// compares: the literal keys of its properties. `null`, `undefined` and
    /// `void` have none. The port's `resolved_keyof_type` adds each index
    /// info's key (and `number` beside a `string` key, as
    /// `getIndexType` does), so those constituents are removed again; a
    /// key this cannot account for leaves the keys unknown.
    fn keyof_without_index_signatures(&mut self, source: TypeId) -> Option<TypeId> {
        if self.checker.type_of(source).flags.intersects(TypeFlags::NULLABLE | TypeFlags::VOID) {
            return Some(self.checker.intrinsics.never);
        }
        let infos = self.checker.get_index_infos_of_type(source)?;
        let keys = self.checker.resolved_keyof_type(source)?;
        if infos.is_empty() {
            return Some(keys);
        }
        let string = self.checker.intrinsics.string;
        let has_string = infos.iter().any(|info| info.key == string);
        let parts = self.union_constituents(keys).unwrap_or_else(|| vec![keys]);
        let mut kept = Vec::with_capacity(parts.len());
        for part in parts {
            if infos.iter().any(|info| info.key == part)
                || (has_string && part == self.checker.intrinsics.number)
            {
                continue;
            }
            if !self.checker.type_of(part).flags.intersects(
                TypeFlags::STRING_LITERAL | TypeFlags::NUMBER_LITERAL | TypeFlags::UNIQUE_ES_SYMBOL,
            ) {
                return None;
            }
            kept.push(part);
        }
        Some(self.checker.get_union_type(&kept))
    }

    /// The rest of `structuredTypeRelatedToWorker` for `S[K]` against `T[J]`
    /// once the components have not related (relater.go:3443-3488, :3652).
    /// Under the assignable and comparable relations the target's write
    /// constraint is tried ([`Self::indexed_access_write_constraint_related_to`]).
    /// The source switch's type-variable case skips a pair of indexed
    /// accesses, and the worker ends False (:3900).
    ///
    /// Only a generic **object** base (a type variable or a generic mapped
    /// type) is taken as generic here: the port's base constraint of an
    /// index is not always native's (`keyof T`'s is `string | number |
    /// symbol` there), so an index the port reads as generic is no proof
    /// and stays `Unknown`.
    fn indexed_access_pair_after_components(
        &mut self,
        source: TypeId,
        target_object: TypeId,
        target_index: TypeId,
    ) -> RelationResult {
        if !matches!(self.relation, Relation::Assignable | Relation::Comparable) {
            return RelationResult::NotRelated;
        }
        let base_object = self.checker.base_constraint_or_type(target_object);
        if self.checker.type_of(base_object).flags.intersects(TypeFlags::INSTANTIABLE_NON_PRIMITIVE)
            || self.is_generic_mapped_type(base_object) == Some(true)
        {
            return RelationResult::NotRelated;
        }
        let base_index = self.checker.base_constraint_or_type(target_index);
        if self.checker.indexed_access_index_is_generic(base_index) {
            return RelationResult::Unknown;
        }
        self.indexed_access_write_constraint_related_to(source, target_object, target_index)
    }

    /// The write-constraint step of `structuredTypeRelatedToWorker`'s
    /// indexed-access target arm (relater.go:3459-3487): `S` relates to
    /// `T[K]` through `getIndexedAccessTypeOrUndefined(baseConstraintOrType(T),
    /// baseConstraintOrType(K), Writing | (NoIndexSignatures when T had a
    /// constraint))` when neither base is generic. No later arm relates a
    /// pair that fails it, so the answer is the constraint relation's, and
    /// a nil constraint is False.
    fn indexed_access_write_constraint_related_to(
        &mut self,
        source: TypeId,
        object: TypeId,
        index: TypeId,
    ) -> RelationResult {
        let base_object = self.checker.base_constraint_or_type(object);
        let base_index = self.checker.base_constraint_or_type(index);
        if self.checker.type_of(base_object).flags.intersects(TypeFlags::INSTANTIABLE_NON_PRIMITIVE)
            || self.checker.indexed_access_index_is_generic(base_index)
        {
            return RelationResult::NotRelated;
        }
        let no_index_signatures = base_object != object;
        match self.indexed_access_write_constraint(base_object, base_index, no_index_signatures) {
            WriteConstraint::Type(constraint) => {
                self.is_related_to_with_flags(source, constraint, RecursionFlags::TARGET)
            }
            WriteConstraint::Nil => RelationResult::NotRelated,
            WriteConstraint::Undecided => RelationResult::Unknown,
        }
    }

    /// `getIndexedAccessTypeOrUndefined` with `AccessFlagsWriting` and no
    /// access node (checker.go:26975, `getPropertyTypeForIndexType` :27001)
    /// over a non-generic object and key, as far as this port certifies it:
    /// - a union key gives the intersection of its constituents' write types,
    ///   and nil when any is nil;
    /// - a key naming a property gives that property's type; an accessor's
    ///   write type is not ported (`Undecided`);
    /// - otherwise an applicable index signature's value, without
    ///   `noUncheckedIndexedAccess`'s `undefined` (a write), unless index
    ///   signatures are excluded;
    /// - a non-literal `string`/`number`/`symbol` key with index signatures
    ///   excluded selects no member: nil (no property name can match it);
    /// - anything else (a tuple or array object, a missing property this
    ///   port cannot prove absent, an `any` object) is `Undecided`.
    fn indexed_access_write_constraint(
        &mut self,
        object: TypeId,
        index: TypeId,
        no_index_signatures: bool,
    ) -> WriteConstraint {
        if let Some(parts) = self.union_constituents(index)
            && !self.checker.type_of(index).flags.intersects(TypeFlags::BOOLEAN)
        {
            let mut types = Vec::with_capacity(parts.len());
            let mut undecided = false;
            for part in parts {
                match self.indexed_access_write_constraint(object, part, no_index_signatures) {
                    WriteConstraint::Type(ty) => types.push(ty),
                    WriteConstraint::Nil => return WriteConstraint::Nil,
                    WriteConstraint::Undecided => undecided = true,
                }
            }
            if undecided {
                return WriteConstraint::Undecided;
            }
            return WriteConstraint::Type(self.checker.get_intersection_type(&types, None));
        }
        let object_flags = self.checker.type_of(object).flags;
        let index_flags = self.checker.type_of(index).flags;
        if object_flags.intersects(TypeFlags::ANY | TypeFlags::UNION | TypeFlags::INTERSECTION)
            || self.checker.tuple_element_lists.contains_key(&object)
            || self.checker.variadic_tuple_elements.contains_key(&object)
            || self.checker.tuple_spread_array_element(object).is_some()
        {
            return WriteConstraint::Undecided;
        }
        if no_index_signatures
            && index_flags.intersects(TypeFlags::STRING | TypeFlags::NUMBER | TypeFlags::ES_SYMBOL)
            && !index_flags.intersects(
                TypeFlags::STRING_LITERAL | TypeFlags::NUMBER_LITERAL | TypeFlags::UNIQUE_ES_SYMBOL,
            )
        {
            return WriteConstraint::Nil;
        }
        let apparent = self.checker.apparent_type(object);
        if let Some(name) = self.checker.property_name_from_index(index) {
            // Held decline: a constrained object's property write type is
            // native's constraint, but deciding it turns
            // `mapped.rs`'s `generic_mapped_contextual_property_type` on,
            // which keys a computed symbol property by its display name
            // (`"[A]"`) where native passes the name's type (`typeof A`), and
            // `contextuallyTypedSymbolNamedProperties` loses its EMPTY_RIGHT
            // (`docs/parity/notes/r5-relater6.md` §4). It retires with that key.
            if no_index_signatures {
                return WriteConstraint::Undecided;
            }
            let Some(property) = self.checker.get_property_of_type(apparent, &name) else {
                return WriteConstraint::Undecided;
            };
            if self
                .checker
                .binder
                .symbols()
                .get(property)
                .flags
                .intersects(SymbolFlags::GET_ACCESSOR | SymbolFlags::SET_ACCESSOR)
            {
                return WriteConstraint::Undecided;
            }
            return match self.checker.get_type_of_property_of_type(apparent, &name) {
                Some(ty) if !self.checker.is_gap(ty) => WriteConstraint::Type(ty),
                _ => WriteConstraint::Undecided,
            };
        }
        if no_index_signatures {
            return WriteConstraint::Undecided;
        }
        match self.checker.get_applicable_index_info(apparent, index) {
            Some(info) if !self.checker.is_gap(info.value) => WriteConstraint::Type(info.value),
            _ => WriteConstraint::Undecided,
        }
    }

    /// getNormalizedType's `getSimplifiedIndexedAccessType` (`checker.go:27915`)
    /// for its generic-mapped-object arm (`:27975`), applied to one side of a
    /// relation entry. `{ [P in K]: E }[X]` becomes `E[P := X]`, plus
    /// `undefined` when the mapped type, or its modifiers type, has `?`
    /// (`substituteIndexedMappedType`, `checker.go:29291`).
    ///
    /// Ported with it, because they feed that arm: the object is simplified
    /// first (a nested `{ [P in T]: { [Q in U]: X } }[T][U]`), a union index
    /// distributes over a mapped object (`distributeObjectOverIndexType`:
    /// a union when reading, an intersection when writing), the substituted
    /// result's indexed-access constituents are simplified in turn (`mapType`),
    /// and the input is removed from a union result (`removeType`).
    ///
    /// Stated divergences, each leaving the indexed access as written:
    /// - the other arms are not ported: a union index over a non-mapped
    ///   object, `(T | U)[K]`/`(T & U)[K]`, and a generic tuple read by a
    ///   number. Native's simplification there changes what the pair relates
    ///   through, not whether this arm applies.
    /// - a substituted conditional constituent is not simplified here; the
    ///   gate's `simplified_conditional` meets it when that constituent is
    ///   related.
    /// - an `as` clause leaves the type unsimplified, as native's remapping
    ///   kind does; the filtering kind is the next port (`tsr-2zk.1035`).
    /// - a step this port cannot compute (an unclassifiable mapped type, a
    ///   failed instantiation, unknown modifiers, 32 nested levels) leaves
    ///   the whole side unsimplified, and the indexed-access pair arm keeps
    ///   its decline for a generic mapped object.
    ///
    /// No cache: native memoises in `cachedTypes`; here the substitution is
    /// an `instantiate_type` of a template under one binding, reached only
    /// for an indexed access whose object is a mapped type. Ir is measured
    /// in `docs/parity/notes/r5-relater6.md` §1.
    fn simplified_indexed_access(&mut self, id: TypeId, writing: bool) -> TypeId {
        if !self.checker.type_of(id).flags.contains(TypeFlags::INDEXED_ACCESS) {
            return id;
        }
        self.simplified_indexed_access_worker(id, writing, 0).unwrap_or(id)
    }

    /// The worker of [`Self::simplified_indexed_access`]: `None` when a step
    /// cannot be computed, so the caller keeps the unsimplified type.
    fn simplified_indexed_access_worker(
        &mut self,
        id: TypeId,
        writing: bool,
        depth: u32,
    ) -> Option<TypeId> {
        let Some(&(object, index, _)) = self.checker.deferred_indexed_access_types.get(&id) else {
            return Some(id);
        };
        if depth == 32 {
            return None;
        }
        let object = self.simplified_indexed_access_worker(object, writing, depth + 1)?;
        self.checker.ensure_mapped_type_info(object);
        let Some(info) = self.checker.mapped_types.get(&object).cloned() else {
            return Some(id);
        };
        if info.name_type.is_some() {
            return Some(id);
        }
        match self.is_generic_mapped_type(object) {
            Some(true) => {}
            Some(false) => return Some(id),
            None => return None,
        }
        let simplified_index = self.simplified_indexed_access_worker(index, writing, depth + 1)?;
        let result = match self.union_constituents(simplified_index) {
            Some(parts) => {
                let mut types = Vec::with_capacity(parts.len());
                for part in parts {
                    types.push(self.substitute_indexed_mapped_type(
                        &info,
                        part,
                        writing,
                        depth + 1,
                    )?);
                }
                if writing {
                    self.checker.get_intersection_type(&types, None)
                } else {
                    self.checker.get_union_type(&types)
                }
            }
            None => self.substitute_indexed_mapped_type(&info, index, writing, depth + 1)?,
        };
        // removeType(result, t).
        if result == id {
            return Some(id);
        }
        Some(match self.union_constituents(result) {
            Some(parts) if parts.contains(&id) => {
                let kept: Vec<TypeId> = parts.into_iter().filter(|&part| part != id).collect();
                self.checker.get_union_type(&kept)
            }
            _ => result,
        })
    }

    /// `substituteIndexedMappedType` (`checker.go:29291`) for a generic mapped
    /// type, followed by `mapType(.., getSimplifiedType)` over the result's
    /// indexed-access constituents. `isGenericType(objectType)` holds for a
    /// generic mapped type, so optionality is the type's own `?` or its
    /// modifiers type's combined optionality.
    fn substitute_indexed_mapped_type(
        &mut self,
        info: &crate::mapped::MappedTypeInfo,
        index: TypeId,
        writing: bool,
        depth: u32,
    ) -> Option<TypeId> {
        let template = self.checker.mapped_template_type(info);
        let instantiated = self.checker.instantiate_type(
            template,
            &[(info.parameter, index)],
            &[info.parameter],
            &[],
        );
        if instantiated == self.checker.intrinsics.error {
            return None;
        }
        let optional = info.optionality == Some(true)
            || match info.modifiers_source {
                Some(modifiers) => self.combined_mapped_optionality(modifiers, 0)? > 0,
                None if info.keyof_constraint => return None,
                None => false,
            };
        let result = if optional && self.checker.strict_null_checks {
            self.checker.get_optional_type(instantiated, true)
        } else {
            instantiated
        };
        match self.union_constituents(result) {
            Some(parts) => {
                let mut types = Vec::with_capacity(parts.len());
                let mut changed = false;
                for part in parts {
                    let simplified = self.simplified_indexed_access_worker(part, writing, depth)?;
                    changed |= simplified != part;
                    types.push(simplified);
                }
                Some(if changed { self.checker.get_union_type(&types) } else { result })
            }
            None => self.simplified_indexed_access_worker(result, writing, depth),
        }
    }

    /// Whether `id` is (or has a constituent that is) the iteration
    /// parameter `P` of a mapped type node `{ [P in Q]: X }`.
    fn is_mapped_iteration_parameter(&self, id: TypeId) -> bool {
        if let Some(parts) = self.union_constituents(id) {
            return parts.into_iter().any(|part| self.is_mapped_iteration_parameter(part));
        }
        self.checker.type_parameter_symbols.get(&id).is_some_and(|&symbol| {
            self.checker.binder.symbols().get(symbol).declarations.iter().any(|&declaration| {
                self.checker.nodes.parent(declaration).is_some_and(|parent| {
                    self.checker.nodes.kind(parent) == tsr_ast::SyntaxKind::MappedType
                })
            })
        })
    }

    /// `isGenericMappedType` (`checker.go:24908`): a mapped type whose
    /// constraint is a generic index type, or whose `as` clause, with the
    /// constraint substituted for the iteration parameter, still is. `None`
    /// when that substitution fails. Unlike [`Self::is_generic_mapped_target`],
    /// which over-approximates for the arms that only decline, this is the
    /// classification the generic-mapped default and `mappedTypeRelatedTo`
    /// decide on.
    fn is_generic_mapped_type(&mut self, id: TypeId) -> Option<bool> {
        self.checker.ensure_mapped_type_info(id);
        let Some(info) = self.checker.mapped_types.get(&id).cloned() else {
            return Some(false);
        };
        if self.checker.is_generic_index_type(info.constraint) {
            return Some(true);
        }
        let Some(name_type) = info.name_type else { return Some(false) };
        let names = self.checker.instantiate_type(
            name_type,
            &[(info.parameter, info.constraint)],
            &[info.parameter],
            &[],
        );
        if names == self.checker.intrinsics.error {
            return None;
        }
        Some(self.checker.is_generic_index_type(names))
    }

    /// `isEmptyObjectType` (`checker.go:26485`): `object`, a union with an
    /// empty constituent, an intersection of empty constituents, or an object
    /// type that is not a generic mapped type and resolves no properties,
    /// index infos or signatures. `None` when a table cannot be read.
    fn is_empty_object_type(&mut self, id: TypeId) -> Option<bool> {
        let flags = self.checker.type_of(id).flags;
        if flags.contains(TypeFlags::NON_PRIMITIVE) {
            return Some(true);
        }
        if let Some(parts) = self.union_constituents(id) {
            let mut undecided = false;
            for part in parts {
                match self.is_empty_object_type(part) {
                    Some(true) => return Some(true),
                    Some(false) => {}
                    None => undecided = true,
                }
            }
            return if undecided { None } else { Some(false) };
        }
        if let Some(parts) = self.intersection_constituents(id) {
            let mut undecided = false;
            for part in parts {
                match self.is_empty_object_type(part) {
                    Some(false) => return Some(false),
                    Some(true) => {}
                    None => undecided = true,
                }
            }
            return if undecided { None } else { Some(true) };
        }
        if !flags.contains(TypeFlags::OBJECT) {
            return Some(false);
        }
        if self.is_generic_mapped_type(id)? {
            return Some(false);
        }
        if self.checker.is_empty_anonymous_object_type(id) {
            return Some(true);
        }
        if !self.checker.get_property_names_of_type(id)?.is_empty()
            || !self.checker.get_index_infos_of_type(id)?.is_empty()
        {
            return Some(false);
        }
        for kind in
            [crate::signatures::SignatureKind::Call, crate::signatures::SignatureKind::Construct]
        {
            if !self.checker.signatures_of_type_kind(id, kind)?.is_empty() {
                return Some(false);
            }
        }
        Some(true)
    }

    /// The default case of `structuredTypeRelatedToWorker`'s source switch
    /// (`relater.go:3795-3812`) against a generic mapped target, reached once
    /// the generic-mapped-target arm (`relater.go:3593`) has not related the
    /// pair:
    ///
    /// - outside the subtype relations an empty object type relates to a
    ///   mapped type with a `?` modifier (`isPartialMappedType`);
    /// - a generic mapped source relates through `mappedTypeRelatedTo`;
    /// - every other source is False. Native returns before the apparent
    ///   type, the structural arm and the discriminated-union arm, so a
    ///   generic mapped target is never compared member by member.
    ///
    /// `None` when the target is not a generic mapped type, or when the
    /// source is not one the default case receives: type variables, `keyof`,
    /// template-literal, string-mapping and conditional sources have their
    /// own cases of the switch, and unions were decomposed earlier.
    fn generic_mapped_target_default(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<RelationResult> {
        let flags = self.checker.type_of(source).flags;
        if flags.intersects(TypeFlags::INSTANTIABLE | TypeFlags::UNION | TypeFlags::ANY)
            || self.checker.deferred_keyof_operands.contains_key(&source)
            || self.checker.deferred_indexed_access_types.contains_key(&source)
            || self.is_conditional(source)
        {
            return None;
        }
        match self.is_generic_mapped_type(target) {
            Some(true) => {}
            Some(false) => return None,
            None => return Some(RelationResult::Unknown),
        }
        let info = self.checker.mapped_types.get(&target).cloned()?;
        if !matches!(self.relation, Relation::Subtype | Relation::StrictSubtype)
            && info.optionality == Some(true)
        {
            match self.is_empty_object_type(source) {
                Some(true) => return Some(RelationResult::Related),
                Some(false) => {}
                None => return Some(RelationResult::Unknown),
            }
        }
        match self.is_generic_mapped_type(source) {
            Some(true) => {
                let result = self.mapped_type_related_to(source, target);
                if result != RelationResult::NotRelated {
                    return Some(result);
                }
            }
            Some(false) => {}
            None => return Some(RelationResult::Unknown),
        }
        Some(RelationResult::NotRelated)
    }

    /// `mappedTypeRelatedTo` (`relater.go:3972`): `{ [P in S]: X }` relates to
    /// `{ [Q in T]: Y }` when the modifiers allow it, `T` relates to `S`, the
    /// `as` clauses agree under `P := Q`, and `X[P := Q]` relates to `Y`.
    ///
    /// Stated divergence: native instantiates the source constraint with
    /// `reportUnmeasurableMapper`/`reportUnreliableMapper`, which only marks
    /// variance measurements; this port has no such markers
    /// (`docs/parity/notes/r5-relater5.md` §1).
    fn mapped_type_related_to(&mut self, source: TypeId, target: TypeId) -> RelationResult {
        let (Some(source_info), Some(target_info)) = (
            self.checker.mapped_types.get(&source).cloned(),
            self.checker.mapped_types.get(&target).cloned(),
        ) else {
            return RelationResult::Unknown;
        };
        if self.relation != Relation::Comparable {
            let (Some(source_optionality), Some(target_optionality)) = (
                self.combined_mapped_optionality(source, 0),
                self.combined_mapped_optionality(target, 0),
            ) else {
                return RelationResult::Unknown;
            };
            if source_optionality > target_optionality {
                return RelationResult::NotRelated;
            }
        }
        let constraints = self.is_related_to_with_flags(
            target_info.constraint,
            source_info.constraint,
            RecursionFlags::BOTH,
        );
        if constraints == RelationResult::NotRelated {
            return RelationResult::NotRelated;
        }
        let map = [(source_info.parameter, target_info.parameter)];
        let parameters = [source_info.parameter];
        let error = self.checker.intrinsics.error;
        let source_names = source_info
            .name_type
            .map(|names| self.checker.instantiate_type(names, &map, &parameters, &[]));
        if source_names == Some(error) {
            return RelationResult::Unknown;
        }
        if source_names != target_info.name_type {
            return RelationResult::NotRelated;
        }
        let source_template = self.checker.mapped_template_type(&source_info);
        let source_template =
            self.checker.instantiate_type(source_template, &map, &parameters, &[]);
        if source_template == error {
            return RelationResult::Unknown;
        }
        let target_template = self.checker.mapped_template_type(&target_info);
        let templates =
            self.is_related_to_with_flags(source_template, target_template, RecursionFlags::BOTH);
        RelationResult::all([constraints, templates])
    }

    /// `structuredTypeRelatedToWorker`'s type-parameter target arm
    /// (`relater.go:3423`) for a mapped source: `{ [P in Q]: X }` relates to
    /// `T` when `keyof T` relates to `Q`, the mapped type has no `?` modifier
    /// and no `as` clause, and `X` relates to `T[P]`. Once the arm has not
    /// related it, the source switch's default case is False: the structural
    /// arm needs an object target and the discriminated arm a union one.
    fn mapped_source_type_parameter_target(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> RelationResult {
        let Some(info) = self.checker.mapped_types.get(&source).cloned() else {
            return RelationResult::Unknown;
        };
        if info.name_type.is_some() {
            return RelationResult::NotRelated;
        }
        let Some(keys) = self.checker.resolved_keyof_type(target) else {
            return RelationResult::Unknown;
        };
        let keys = self.is_related_to_with_flags(keys, info.constraint, RecursionFlags::BOTH);
        if keys == RelationResult::NotRelated {
            return RelationResult::NotRelated;
        }
        let undecided_keys = keys == RelationResult::Unknown;
        if info.optionality == Some(true) {
            return if undecided_keys {
                RelationResult::Unknown
            } else {
                RelationResult::NotRelated
            };
        }
        let Some(indexed) =
            self.checker.resolved_indexed_access_type(target, info.parameter, false)
        else {
            return RelationResult::Unknown;
        };
        let template = self.checker.mapped_template_type(&info);
        let result = self.is_related_to_with_flags(template, indexed, RecursionFlags::BOTH);
        match result {
            RelationResult::NotRelated => RelationResult::NotRelated,
            _ if undecided_keys => RelationResult::Unknown,
            _ => result,
        }
    }

    /// The comparable carve-out of the type-parameter target arm
    /// (relater.go:3434): `None` outside it.
    fn comparable_type_parameter_pair(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<RelationResult> {
        if self.relation != Relation::Comparable
            || !self.checker.type_of(target).flags.contains(TypeFlags::TYPE_PARAMETER)
            || !self.checker.type_of(source).flags.contains(TypeFlags::TYPE_PARAMETER)
            || !self.checker.type_parameter_symbols.contains_key(&target)
        {
            return None;
        }
        let &symbol = self.checker.type_parameter_symbols.get(&source)?;
        let declarations = self.checker.binder.symbols().get(symbol).declarations.clone();
        let [declaration] = declarations.as_slice() else { return None };
        let Some(tsr_ast::Node::TypeParameterDeclaration(parameter)) =
            self.checker.node_map.get(*declaration)
        else {
            return None;
        };
        if parameter.constraint.is_none() {
            return Some(RelationResult::NotRelated);
        }
        let Some(constraint) = self.checker.type_parameter_constraint(source) else {
            return Some(RelationResult::Unknown);
        };
        let mentions_parameter = self
            .union_constituents(constraint)
            .unwrap_or_else(|| vec![constraint])
            .into_iter()
            .any(|part| self.checker.type_of(part).flags.contains(TypeFlags::TYPE_PARAMETER));
        if !mentions_parameter {
            return Some(RelationResult::NotRelated);
        }
        Some(self.is_related_to_with_flags(constraint, target, RecursionFlags::SOURCE))
    }

    /// The modifier gate of `mappedTypeRelatedTo` (`relater.go:3972`), reached
    /// from `structuredTypeRelatedToWorker`'s default branch (`relater.go:3805`)
    /// once the generic-mapped-target arm has failed: for two generic mapped
    /// types, `getCombinedMappedTypeOptionality(source) >
    /// getCombinedMappedTypeOptionality(target)` makes the pair unrelated
    /// (`Partial<T> -> Readonly<T>`). The rest of `mappedTypeRelatedTo`
    /// (constraint and template comparison under a parameter mapper) is not
    /// ported, so only this definite negative is taken.
    ///
    /// Narrowed to mapped types this port is sure are generic — a type-variable
    /// constraint and no `as` clause on both sides — because
    /// `is_generic_mapped_target` over-approximates, and a non-generic mapped
    /// type is resolved structurally upstream instead.
    fn mapped_modifiers_reject(&mut self, source: TypeId, target: TypeId) -> bool {
        if matches!(self.relation, Relation::Comparable) {
            return false;
        }
        let (Some(source_info), Some(target_info)) = (
            self.checker.mapped_types.get(&source).cloned(),
            self.checker.mapped_types.get(&target).cloned(),
        ) else {
            return false;
        };
        let surely_generic = |checker: &Checker<'_, '_>, info: &crate::mapped::MappedTypeInfo| {
            info.name_type.is_none()
                && checker.maybe_type_of_kind(info.constraint, TypeFlags::INSTANTIABLE)
        };
        if !surely_generic(self.checker, &source_info)
            || !surely_generic(self.checker, &target_info)
        {
            return false;
        }
        let (Some(source_optionality), Some(target_optionality)) = (
            self.combined_mapped_optionality(source, 0),
            self.combined_mapped_optionality(target, 0),
        ) else {
            return false;
        };
        source_optionality > target_optionality
    }

    /// `getCombinedMappedTypeOptionality` (`checker.go:29040`): a mapped
    /// type's own `+?`/`-?` (1/-1), else its modifiers type's; an
    /// intersection's common value, else 0. `None` where the modifiers type
    /// is not known to this port.
    fn combined_mapped_optionality(&mut self, id: TypeId, depth: u32) -> Option<i32> {
        if depth > 32 {
            return None;
        }
        if let Some(info) = self.checker.mapped_types.get(&id).cloned() {
            return match info.optionality {
                Some(true) => Some(1),
                Some(false) => Some(-1),
                None if info.keyof_constraint || info.modifiers_source.is_some() => {
                    match info.modifiers_source {
                        Some(modifiers) => self.combined_mapped_optionality(modifiers, depth + 1),
                        None => None,
                    }
                }
                // Not homomorphic: the modifiers type is unknown, which
                // contributes 0 upstream.
                None => Some(0),
            };
        }
        if let Some(parts) = self.intersection_constituents(id) {
            let mut iter = parts.into_iter();
            let first = self.combined_mapped_optionality(iter.next()?, depth + 1)?;
            for part in iter {
                if self.combined_mapped_optionality(part, depth + 1)? != first {
                    return Some(0);
                }
            }
            return Some(first);
        }
        Some(0)
    }

    /// isGenericMappedType (checker.go) as far as this port can tell: a mapped
    /// type whose constraint (or key-remapping name type) may still be
    /// instantiable. Over-approximates, which only leaves pairs undecided.
    fn is_generic_mapped_target(&self, id: TypeId) -> bool {
        self.checker.mapped_types.get(&id).is_some_and(|info| {
            self.checker.maybe_type_of_kind(info.constraint, TypeFlags::INSTANTIABLE)
                || info.name_type.is_some()
        })
    }

    /// The evaluated body of an alias-reference image (`is_qualified_alias_mint`)
    /// when no constituent of that body is an object, intersection or
    /// `object`; `None` otherwise, or when the body does not evaluate. The
    /// body comes from `evaluate_alias_body`'s `(symbol, arguments)` cache.
    ///
    /// Stated divergence: native relates every alias instantiation as its
    /// body. A body with object constituents keeps the image here, which the
    /// alias-variance and structural roads read. Relating `Either<L, A> =
    /// Left<L, A> | Right<L, A>` as its body instead ran
    /// `varianceProblingAndZeroOrderIndexSignatureRelationsAlign` past 8 GB
    /// (base: 41 s), the unbounded expansion r5-relater4 §1 met there.
    fn non_object_alias_image_body(&mut self, id: TypeId) -> Option<TypeId> {
        let TypeData::Named { members: Some(symbol), .. } = self.checker.type_of(id).data else {
            return None;
        };
        if !self.checker.binder.symbols().get(symbol).flags.intersects(SymbolFlags::TYPE_ALIAS) {
            return None;
        }
        let (symbol, arguments) = self.checker.type_reference_targets.get(&id).cloned()?;
        let body = self.checker.evaluate_alias_body(symbol, &arguments)?;
        if body == id || body == self.checker.intrinsics.error {
            return None;
        }
        let parts = self.union_constituents(body).unwrap_or_else(|| vec![body]);
        parts
            .iter()
            .all(|&part| {
                !self.checker.type_of(part).flags.intersects(
                    TypeFlags::OBJECT | TypeFlags::INTERSECTION | TypeFlags::NON_PRIMITIVE,
                )
            })
            .then_some(body)
    }

    /// Whether `id` is an indexed access `{ [P in K]: E }[X]` with a generic
    /// `X` whose object is a resolved instance of a mapped alias: native's
    /// `isMappedTypeGenericIndexedAccess` holds and its constraint is the
    /// substitution `E[P := X]` (getConstraintFromIndexedAccess,
    /// checker.go:17227), which this port cannot build because the instance
    /// lost its mapped identity. Its object is captured first if lazy.
    fn mapped_substitution_out_of_reach(&mut self, id: TypeId) -> bool {
        let Some(&(object, index, _)) = self.checker.deferred_indexed_access_types.get(&id) else {
            return false;
        };
        self.checker.ensure_mapped_type_info(object);
        !self.checker.mapped_types.contains_key(&object)
            && self.checker.indexed_access_index_is_generic(index)
            && self.is_resolved_mapped_alias_instance(object)
    }

    /// `isMappedTypeGenericIndexedAccess` (checker.go): an indexed access
    /// with a generic index whose object is a mapped type that is not
    /// generic, either captured as one or resolved from a mapped alias.
    fn is_mapped_type_generic_indexed_access(&mut self, id: TypeId) -> bool {
        let Some(&(object, index, _)) = self.checker.deferred_indexed_access_types.get(&id) else {
            return false;
        };
        if !self.checker.indexed_access_index_is_generic(index) {
            return false;
        }
        self.checker.ensure_mapped_type_info(object);
        if let Some(info) = self.checker.mapped_types.get(&object).cloned() {
            return info.optionality != Some(false)
                && info.name_type.is_none()
                && self.is_generic_mapped_type(object) == Some(false);
        }
        self.resolved_mapped_alias_declaration(object)
            .is_some_and(|(remapped, excludes_optional)| !remapped && !excludes_optional)
    }

    /// Whether `id` is an instantiation of a type alias whose declared body
    /// is a mapped type node (`Partial<Foo1>`), read from its reference
    /// target, or the image of an argument-less such alias (`Funcs`).
    fn is_resolved_mapped_alias_instance(&self, id: TypeId) -> bool {
        self.resolved_mapped_alias_declaration(id).is_some()
    }

    /// For the mapped type node declared as the body of the alias `id`
    /// instantiates (see [`Self::is_resolved_mapped_alias_instance`]):
    /// whether it has an `as` clause and whether it has a `-?` modifier.
    fn resolved_mapped_alias_declaration(&self, id: TypeId) -> Option<(bool, bool)> {
        let symbol = match self.checker.type_reference_targets.get(&id) {
            Some(&(symbol, _)) => symbol,
            None => match self.checker.type_of(id).data {
                TypeData::Named { members: Some(symbol), .. }
                    if self
                        .checker
                        .binder
                        .symbols()
                        .get(symbol)
                        .flags
                        .intersects(SymbolFlags::TYPE_ALIAS) =>
                {
                    symbol
                }
                _ => return None,
            },
        };
        self.checker.binder.symbols().get(symbol).declarations.iter().find_map(|&declaration| {
            match self.checker.node_map.get(declaration) {
                Some(tsr_ast::Node::TypeAliasDeclaration(alias)) => match alias.r#type {
                    Some(tsr_ast::TypeNode::MappedTypeNode(mapped)) => Some((
                        mapped.name_type.is_some(),
                        mapped
                            .question_token
                            .is_some_and(|token| token.kind == tsr_ast::SyntaxKind::MinusToken),
                    )),
                    _ => None,
                },
                _ => None,
            }
        })
    }

    /// `declared.rs`'s `qualified_type_reference` mints an OBJECT-flagged
    /// `Named` image for every argument-less qualified reference, including a
    /// type alias whose declared type is not an object.
    fn is_qualified_alias_mint(&self, id: TypeId) -> bool {
        matches!(self.checker.type_of(id).data, TypeData::Named { members: Some(symbol), .. }
            if self.checker.binder.symbols().get(symbol).flags
                .intersects(tsr_binder::SymbolFlags::TYPE_ALIAS))
    }

    /// Whether a type parameter's declaration writes an `extends` clause, so
    /// an absent `type_parameter_constraint` means unread rather than none.
    fn declares_written_constraint(&self, id: TypeId) -> bool {
        self.checker.type_parameter_symbols.get(&id).is_some_and(|&symbol| {
            self.checker.binder.symbols().get(symbol).declarations.iter().any(|&node| {
                matches!(
                    self.checker.node_map.get(node),
                    Some(tsr_ast::Node::TypeParameterDeclaration(parameter))
                        if parameter.constraint.is_some()
                )
            })
        })
    }

    /// Whether `id` is a class instance whose declaration has an `extends` or
    /// `implements` clause.
    fn class_declares_heritage(&self, id: TypeId) -> bool {
        self.checker
            .class_instance_symbol(id)
            .and_then(|symbol| self.checker.binder.symbols().get(symbol).value_declaration)
            .is_some_and(|declaration| {
                matches!(self.checker.node_map.get(declaration),
                    Some(tsr_ast::Node::ClassDeclaration(class)) if !class.heritage_clauses.is_empty())
            })
    }

    fn has_members(&self, id: TypeId) -> bool {
        if self.checker.type_of(id).flags.contains(TypeFlags::TYPE_PARAMETER) {
            return false;
        }
        // A bounded open homomorphic map can supply a complete member table
        // independently of its declaration owner. Closed maps keep their
        // existing structural path; nominal roots need a separate proof.
        if self.checker.is_generic_homomorphic_mapped_type(id)
            && let Some((properties, true)) = self.checker.anonymous_properties.get(&id)
            && properties.iter().all(|property| {
                self.checker.peek_property_type(property).is_none_or(|ty| !self.checker.is_gap(ty))
                    && property.origin.is_some_and(|origin| {
                        !self
                            .checker
                            .property_has_modifier(origin, tsr_ast::SyntaxKind::PrivateKeyword)
                            && !self.checker.property_has_modifier(
                                origin,
                                tsr_ast::SyntaxKind::ProtectedKeyword,
                            )
                    })
            })
        {
            return true;
        }
        // An anonymous object type is structured whenever it carries call or
        // construct signatures. `signature` records only the printed node kind
        // (a bare FunctionTypeNode): an aliased function type such as
        // `type L = (x: number) => string` prints as `L` and records `false`,
        // yet upstream still compares it structurally — that is how
        // `L -> Function` relates through the global Function members.
        self.checker.class_static_symbol(id).is_some()
            || match &self.checker.type_of(id).data {
                TypeData::Named { members: Some(_), .. }
                | TypeData::Anonymous { signature: true, .. } => true,
                TypeData::Anonymous { symbol, .. } if self.is_namespace_object_symbol(*symbol) => {
                    true
                }
                TypeData::Anonymous { .. } => {
                    self.checker.signatures_of_type(id).is_some_and(|list| !list.is_empty())
                }
                _ => false,
            }
    }

    /// A namespace's object type (`typeof N`, `getTypeOfSymbol` of a value
    /// module, checker.go `createObjectType(ObjectFlagsAnonymous, symbol)`)
    /// whose members are its exports. Native resolves its members like any
    /// anonymous object type; a namespace merged with a function, class or
    /// enum keeps its own arms.
    fn is_namespace_object_symbol(&self, symbol: SymbolId) -> bool {
        let flags = self.checker.binder.symbols().get(symbol).flags;
        flags.intersects(SymbolFlags::VALUE_MODULE)
            && !flags.intersects(SymbolFlags::FUNCTION | SymbolFlags::CLASS | SymbolFlags::ENUM)
    }

    fn is_pure_signature_type(&mut self, id: TypeId) -> bool {
        if self.checker.get_property_names_of_type_shared(id).is_some_and(|names| !names.is_empty())
            || self.checker.get_index_infos_of_type(id).is_none_or(|infos| !infos.is_empty())
        {
            return false;
        }
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
                            | tsr_ast::SyntaxKind::ConstructorType
                            | tsr_ast::SyntaxKind::ConstructSignature
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

    /// signaturesRelatedTo (relater.go:4441) compares call and construct sets
    /// independently. Index-only targets retain their separate relation path.
    /// None means signature resolution is unsupported, not a rejection.
    fn related_signatures(&mut self, source: TypeId, target: TypeId) -> Option<RelationResult> {
        if self.checker.any_function_type == Some(source) {
            return Some(RelationResult::Related);
        }
        if self.checker.any_function_type == Some(target) {
            return Some(RelationResult::NotRelated);
        }
        if !self.call_or_construct_bearing(target) {
            return None;
        }
        let calls =
            self.related_signature_kind(source, target, crate::signatures::SignatureKind::Call)?;
        let constructs = self.related_signature_kind(
            source,
            target,
            crate::signatures::SignatureKind::Construct,
        )?;
        Some(RelationResult::all([calls, constructs]))
    }

    /// signaturesRelatedTo (relater.go:4441), for either signature kind.
    fn related_signature_kind(
        &mut self,
        source: TypeId,
        target: TypeId,
        kind: crate::signatures::SignatureKind,
    ) -> Option<RelationResult> {
        use crate::signatures::SignatureKind;
        let target_signatures = self.checker.signatures_of_type_kind(target, kind)?;
        if target_signatures.is_empty() {
            return Some(RelationResult::Related);
        }
        let source_signatures = self.checker.signature_shapes_of_type_kind(source, kind)?;
        if source_signatures.is_empty() {
            return Some(RelationResult::NotRelated);
        }
        if kind == SignatureKind::Construct {
            if source_signatures[0].kind == SignatureKind::AbstractConstruct
                && target_signatures[0].kind != SignatureKind::AbstractConstruct
            {
                return Some(RelationResult::NotRelated);
            }
            if !self.constructor_visibilities_are_compatible(
                &source_signatures[0],
                &target_signatures[0],
            ) {
                return Some(RelationResult::NotRelated);
            }
        }
        // signaturesRelatedTo (relater.go:4441) erases generic signatures
        // for the overload matrix and comparable single signatures. Keep an
        // uncomputed erasure local to its signature: another source overload
        // can still prove a target signature compatible.
        let same_origin = match (
            &self.checker.store.get(source).data,
            &self.checker.store.get(target).data,
        ) {
            (
                TypeData::Anonymous { symbol: source_symbol, .. },
                TypeData::Anonymous { symbol: target_symbol, .. },
            ) => {
                source_symbol == target_symbol
                    && self.checker.instantiated_signature_mappers.contains_key(&source)
                    && self.checker.instantiated_signature_mappers.contains_key(&target)
            }
            _ => false,
        } || matches!(
            (self.checker.type_reference_targets.get(&source), self.checker.type_reference_targets.get(&target)),
            (Some((source, _)), Some((target, _))) if source == target
        );
        let erase = same_origin
            || source_signatures.len() != 1
            || target_signatures.len() != 1
            || self.relation == Relation::Comparable;
        let source_signatures: Vec<_> = source_signatures
            .into_iter()
            .map(|signature| {
                if erase {
                    self.checker.signature_for_inference(signature, true)
                } else {
                    Some(signature)
                }
            })
            .collect();
        let target_signatures: Vec<_> = target_signatures
            .into_iter()
            .map(|signature| {
                if erase {
                    self.checker.signature_for_inference(signature, true)
                } else {
                    Some(signature)
                }
            })
            .collect();
        // §936.1: §935 required exactly ONE signature per side and named the
        // generalisation as its residue — upstream's *"some source signature
        // relates to each target signature"* (`signaturesRelatedTo`,
        // `relater.go:4441`, whose loop iterates the TARGET's list and searches
        // the source's). That is this walk.
        //
        // An uncomputed erasure or pair remains Unknown unless a different
        // source signature proves this target compatible. Do not turn a
        // missing comparison into a rejection merely because another failed.
        let report_errors = self.diagnostic_pair == Some((source, target));
        let mut parts = Vec::new();
        for target_signature in &target_signatures {
            let saved_error = self.signature_error.take();
            let Some(target_signature) = target_signature else {
                self.signature_error = saved_error;
                parts.push(RelationResult::Unknown);
                continue;
            };
            let mut best = RelationResult::NotRelated;
            for (index, source_signature) in source_signatures.iter().enumerate() {
                let verdict = source_signature
                    .as_ref()
                    .and_then(|source_signature| {
                        self.one_signature_related_to(
                            source_signature,
                            target_signature,
                            false,
                            false,
                            report_errors && index == 0,
                        )
                    })
                    .unwrap_or(RelationResult::Unknown);
                best = RelationResult::any([best, verdict]);
                if best.is_success() {
                    break;
                }
            }
            if best == RelationResult::NotRelated {
                // Native signaturesRelatedTo returns on the first target with
                // no matching source. Later targets must not replace its chain.
                return Some(RelationResult::NotRelated);
            }
            self.signature_error = saved_error;
            parts.push(best);
        }
        Some(RelationResult::all(parts))
    }

    /// constructorVisibilitiesAreCompatible (relater.go:4520).
    fn constructor_visibilities_are_compatible(
        &self,
        source: &crate::signatures::Signature,
        target: &crate::signatures::Signature,
    ) -> bool {
        use tsr_ast::{ModifierLike, Node, SyntaxKind};
        let visibility = |signature: &crate::signatures::Signature| {
            let modifiers = match self.checker.node_map.get(signature.declaration) {
                Some(Node::ConstructorDeclaration(node)) => node.modifiers,
                _ => return None,
            };
            modifiers.iter().find_map(|modifier| match modifier {
                ModifierLike::Token(token)
                    if matches!(
                        token.kind,
                        SyntaxKind::PrivateKeyword | SyntaxKind::ProtectedKeyword
                    ) =>
                {
                    Some(token.kind)
                }
                _ => None,
            })
        };
        let source = visibility(source);
        let target = visibility(target);
        target == Some(SyntaxKind::PrivateKeyword)
            || (target == Some(SyntaxKind::ProtectedKeyword)
                && source != Some(SyntaxKind::PrivateKeyword))
            || (target != Some(SyntaxKind::ProtectedKeyword) && source.is_none())
    }

    /// hasExcessProperties' Object/empty-object exemption for assignability
    /// and comparability (relater.go:2720). Unknown member sets do not exempt.
    fn target_exempts_excess_properties(&mut self, target: TypeId) -> bool {
        if let Some(types) = self.union_constituents(target) {
            return types.into_iter().any(|ty| self.target_exempts_excess_properties(ty));
        }
        if let Some(types) = self.intersection_constituents(target) {
            return types.into_iter().all(|ty| self.target_exempts_excess_properties(ty));
        }
        if self.checker.type_of(target).flags.contains(TypeFlags::NON_PRIMITIVE) {
            return true;
        }
        if !self.checker.type_of(target).flags.contains(TypeFlags::OBJECT) {
            return false;
        }
        if let TypeData::Named { members: Some(owner), .. } = self.checker.type_of(target).data
            && self.checker.global_type_symbol_with_arity("Object", 0) == Some(owner)
        {
            return true;
        }
        self.checker
            .get_property_names_of_type(target)
            .is_some_and(|properties| properties.is_empty())
            && !self.signature_bearing(target)
            && self.checker.get_index_infos_of_type(target).is_some_and(|infos| infos.is_empty())
    }

    /// One source signature against one target signature — the comparison §935
    /// ported, now the inner step of [`Relater::related_signatures`].
    ///
    /// `None` when the pair is outside what this port can judge, which lets the
    /// caller keep looking rather than reading "cannot judge" as "not related".
    fn one_signature_related_to(
        &mut self,
        source_signature: &crate::signatures::Signature,
        target_signature: &crate::signatures::Signature,
        callback: bool,
        bivariant_callback: bool,
        report_errors: bool,
    ) -> Option<RelationResult> {
        let source_top = self.checker.signature_is_top(source_signature);
        let target_top = self.checker.signature_is_top(target_signature);
        let strict_top = matches!(self.relation, Relation::Subtype | Relation::StrictSubtype);
        if target_top && !(strict_top && source_top) {
            return Some(RelationResult::Related);
        }
        if strict_top && source_top && !target_top {
            return Some(RelationResult::NotRelated);
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
            if report_errors && self.relation != Relation::StrictSubtype {
                self.signature_error = Some((source_minimum, target_count));
            }
            return Some(RelationResult::NotRelated);
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
        let canonical_target =
            if !shared_type_parameters && !source_signature.type_parameters.is_empty() {
                Some(self.checker.canonical_signature(target_signature.clone())?)
            } else {
                None
            };
        let target_signature = canonical_target.as_ref().unwrap_or(target_signature);
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
        let uninstantiated_return = source_signature.r#type;
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
            && self.checker.parameter_type(source_this) != self.checker.intrinsics.void
        {
            let source_this = self.checker.parameter_type(source_this);
            let target_this = self.checker.parameter_type(target_this);
            parts.push(if strict_variance {
                self.is_related_to(target_this, source_this)
            } else {
                let forward = self.is_related_to(source_this, target_this);
                if forward.is_success() {
                    forward
                } else {
                    RelationResult::any([forward, self.is_related_to(target_this, source_this)])
                }
            });
            if parts.last() == Some(&RelationResult::NotRelated) {
                return Some(RelationResult::NotRelated);
            }
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
            if self.checker.is_gap(from) || self.checker.is_gap(to) {
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
                    false,
                )?);
            } else {
                parts.push(if callback || strict_variance {
                    self.is_related_to(to, from)
                } else {
                    // compareSignaturesRelated tries the forward bivariant
                    // proof first; the reverse walk is needed only if it fails.
                    let forward = self.is_related_to(from, to);
                    if forward.is_success() {
                        forward
                    } else {
                        RelationResult::any([forward, self.is_related_to(to, from)])
                    }
                });
            }
            // compareSignaturesRelated returns on the first incompatible
            // parameter, before comparing later parameters or return types.
            if parts.last() == Some(&RelationResult::NotRelated) {
                return Some(RelationResult::NotRelated);
            }
            if self.relation == Relation::StrictSubtype
                && index >= source_minimum
                && index < target_minimum
                && self.is_related_to(from, to) != RelationResult::NotRelated
            {
                return Some(RelationResult::NotRelated);
            }
        }
        if target_signature.r#type != self.checker.intrinsics.void
            && target_signature.r#type != self.checker.intrinsics.any
        {
            // compareSignaturesRelated reads target any/void before demanding
            // the source return (relater.go:1595-1603). Parameter-only source
            // metadata is not a completed error, predicate or mapper image.
            let source_signature =
                self.checker.complete_signature_return(source_signature.clone())?;
            if source_signature.r#type == self.checker.intrinsics.error {
                return None;
            }
            if target_signature.predicate.is_some() {
                if source_signature.predicate.is_some() {
                    if !source_signature.predicate_kinds_match(target_signature)? {
                        return Some(RelationResult::NotRelated);
                    }
                    let source = source_signature.predicate.as_ref()?.r#type;
                    let target = target_signature.predicate.as_ref()?.r#type;
                    parts.push(match (source, target) {
                        (Some(source), Some(target)) => self.is_related_to(source, target),
                        (None, None) => RelationResult::Related,
                        _ => RelationResult::NotRelated,
                    });
                } else if !target_signature.predicate.as_ref()?.asserts {
                    return Some(RelationResult::NotRelated);
                }
            } else {
                parts.push(if bivariant_callback {
                    // Callback returns use the opposite native direction order.
                    let reverse =
                        self.is_related_to(target_signature.r#type, source_signature.r#type);
                    if reverse.is_success() {
                        reverse
                    } else {
                        RelationResult::any([
                            reverse,
                            self.is_related_to(source_signature.r#type, target_signature.r#type),
                        ])
                    }
                } else {
                    let result =
                        self.is_related_to(source_signature.r#type, target_signature.r#type);
                    // instantiateSignatureInContextOf infers the source's type
                    // parameters from the target's return type, through
                    // inferFromObjectTypes' mapped-to-mapped arm (constraint to
                    // constraint, template to template; inference.go:699).
                    // `instantiate_signature_in_context` has no such arm, so a
                    // generic mapped return erases to its constraint's image
                    // (`<U>() => { [K in keyof U]: U[K] }` becomes `{}`) and a
                    // failure is the missing inference's, not the relation's
                    // (`higherOrderMappedIndexLookupInference`).
                    if result == RelationResult::NotRelated
                        && instantiated_source.is_some()
                        && self.is_generic_mapped_type(uninstantiated_return) == Some(true)
                        && self.is_generic_mapped_type(target_signature.r#type) == Some(true)
                    {
                        RelationResult::Unknown
                    } else {
                        result
                    }
                });
            }
        }
        Some(RelationResult::all(parts))
    }

    /// Whether `id` declares a CALL or CONSTRUCT signature — the half of
    /// [`Relater::signature_bearing`] that §935's arm is about, split out so
    /// §936's index arm cannot run on a type §935 should be judging.
    fn declares_call_or_construct(&self, id: TypeId) -> bool {
        if self.checker.class_static_symbol(id).is_some() {
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
                )
            })
        })
    }

    /// [`Relater::declares_call_or_construct`] over the resolved members of
    /// a class or interface: `resolveDeclaredMembers` adds every base type's
    /// call and construct signatures (`resolveObjectTypeMembers`,
    /// checker.go), so `interface F extends P {}` with a callable `P` is a
    /// callable target. `signaturesRelatedTo` (relater.go:4441) reads those
    /// resolved signatures; `signatures_of_type_kind` already resolves them.
    fn call_or_construct_bearing(&mut self, id: TypeId) -> bool {
        self.call_or_construct_bearing_at(id, 0)
    }

    fn call_or_construct_bearing_at(&mut self, id: TypeId, depth: usize) -> bool {
        if self.declares_call_or_construct(id) {
            return true;
        }
        // The depth bound stands in for native's resolution guard on a
        // circular base chain (`getBaseTypes` reports and cuts it).
        if depth > 64 {
            return false;
        }
        let owner = match self.checker.type_reference_targets.get(&id) {
            Some(&(symbol, _)) => symbol,
            None => match self.checker.type_of(id).data {
                TypeData::Named { members: Some(symbol), .. } => symbol,
                _ => return false,
            },
        };
        let owner = self.checker.binder.merged_symbol(owner);
        if !self
            .checker
            .binder
            .symbols()
            .get(owner)
            .flags
            .intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE)
        {
            return false;
        }
        for base in self.checker.get_base_types(owner) {
            if self.call_or_construct_bearing_at(base, depth + 1) {
                return true;
            }
        }
        false
    }

    /// indexSignaturesRelatedTo / typeRelatedToIndexInfo (relater.go:4578).
    /// Semantic target infos apply independently of properties and signatures.
    fn related_index_signatures(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<RelationResult> {
        let target_infos = self.checker.get_index_infos_of_type(target)?;
        if target_infos.is_empty() {
            return Some(RelationResult::Related);
        }
        let target_has_string =
            target_infos.iter().any(|info| info.key == self.checker.intrinsics.string);
        let mut parts = Vec::with_capacity(target_infos.len());
        for info in &target_infos {
            if self.relation != Relation::StrictSubtype
                && target_has_string
                && self.checker.type_of(info.value).flags.contains(TypeFlags::ANY)
            {
                continue;
            }
            // indexSignaturesRelatedTo (relater.go:4590): a generic mapped
            // source against a target with a string index relates its
            // template to each target index's value type.
            if target_has_string {
                match self.is_generic_mapped_type(source) {
                    Some(true) => {
                        let mapped = self.checker.mapped_types.get(&source).cloned()?;
                        let template = self.checker.mapped_template_type(&mapped);
                        parts.push(self.is_related_to(template, info.value));
                        continue;
                    }
                    Some(false) => {}
                    None => return None,
                }
            }
            // Unresolved source infos cannot prove that a declared index is absent.
            let source_infos = self.checker.get_index_infos_of_type(source)?;
            if let Some(from) = self.checker.get_applicable_index_info(source, info.key) {
                parts.push(self.is_related_to(from.value, info.value));
                continue;
            }
            if (self.relation == Relation::StrictSubtype
                && !self.checker.fresh_object_literal_types.contains(&source))
                || !self.object_type_has_inferable_index(source)
            {
                return Some(RelationResult::NotRelated);
            }
            let names = self.checker.get_property_names_of_type(source)?;
            for name in &names {
                let key = self.checker.literal_type_of_property(source, name);
                if !self.checker.is_applicable_index_type(key, info.key) {
                    continue;
                }
                let mut member = self.checker.get_type_of_property_of_type(source, name)?;
                member = self.checker.remove_missing_type(member);
                let optional = if let Some(property) = self
                    .checker
                    .anonymous_properties
                    .get(&source)
                    .and_then(|(properties, _)| properties.iter().find(|p| p.name == *name))
                {
                    property.optional
                } else {
                    self.checker
                        .get_property_of_type(source, name)
                        .is_some_and(|symbol| self.checker.property_is_optional(symbol))
                };
                if !self.checker.exact_optional_property_types
                    && !self.checker.type_of(member).flags.contains(TypeFlags::UNDEFINED)
                    && info.key != self.checker.intrinsics.number
                    && optional
                {
                    member = self
                        .checker
                        .get_type_with_facts(member, crate::flow::TypeFacts::NE_UNDEFINED);
                }
                parts.push(self.is_related_to(member, info.value));
            }
            for source_info in &source_infos {
                if self.checker.is_applicable_index_type(source_info.key, info.key) {
                    parts.push(self.is_related_to(source_info.value, info.value));
                }
            }
        }
        Some(RelationResult::all(parts))
    }

    /// isObjectTypeWithInferableIndex (relater.go:4624): interfaces and classes
    /// require a declared index; object/type literals can infer one from members.
    fn object_type_has_inferable_index(&self, id: TypeId) -> bool {
        if self.checker.js_literal_types.contains(&id) {
            return true;
        }
        let (TypeData::Named { members: Some(symbol), .. } | TypeData::Anonymous { symbol, .. }) =
            self.checker.type_of(id).data
        else {
            return false;
        };
        let flags = self.checker.binder.symbols().get(symbol).flags;
        flags.intersects(
            tsr_binder::SymbolFlags::OBJECT_LITERAL
                | tsr_binder::SymbolFlags::TYPE_LITERAL
                | tsr_binder::SymbolFlags::ENUM
                | tsr_binder::SymbolFlags::VALUE_MODULE,
        ) && !flags.contains(tsr_binder::SymbolFlags::CLASS)
            && !self.declares_call_or_construct(id)
    }

    fn signature_bearing(&self, id: TypeId) -> bool {
        if self.checker.class_static_symbol(id).is_some() {
            return true;
        }
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

    /// isEnumTypeRelatedTo (relater.go:282), for two enum symbols (members
    /// already mapped to their parent enum by the caller). Two distinct
    /// enums relate only when they have the same name, both are regular
    /// (non-const) enums, and every member of the source exists in the target
    /// with the same value; a member whose value is unknown on one side is
    /// assumed numeric, so it mismatches a known string. Native memoises the
    /// answer per symbol pair (`c.enumRelation`); this port recomputes it, a
    /// walk over one enum's members, only for same-named enum pairs.
    fn is_enum_type_related_to(&mut self, source: SymbolId, target: SymbolId) -> bool {
        use tsr_binder::SymbolFlags;
        if source == target {
            return true;
        }
        let symbols = self.checker.binder.symbols();
        let (source_data, target_data) = (symbols.get(source), symbols.get(target));
        if source_data.name != target_data.name
            || !source_data.flags.contains(SymbolFlags::REGULAR_ENUM)
            || !target_data.flags.contains(SymbolFlags::REGULAR_ENUM)
        {
            return false;
        }
        let mut source_members: Vec<(String, SymbolId)> = source_data
            .members
            .as_ref()
            .map(|table| table.iter().map(|(name, &id)| (name.to_string(), id)).collect())
            .unwrap_or_default();
        source_members.sort_by_key(|&(_, id)| id);
        for (name, source_member) in source_members {
            let symbols = self.checker.binder.symbols();
            if !symbols.get(source_member).flags.contains(SymbolFlags::ENUM_MEMBER) {
                continue;
            }
            let Some(target_member) = symbols
                .get(target)
                .members
                .as_ref()
                .and_then(|table| table.get(name.as_str()).copied())
                .filter(|&member| symbols.get(member).flags.contains(SymbolFlags::ENUM_MEMBER))
            else {
                return false;
            };
            let source_type = self.checker.get_declared_type_of_symbol(source_member);
            let target_type = self.checker.get_declared_type_of_symbol(target_member);
            let source_value = self.checker.enum_member_value(source_type).map(|(_, key)| key);
            let target_value = self.checker.enum_member_value(target_type).map(|(_, key)| key);
            if source_value != target_value {
                if source_value.is_some() && target_value.is_some() {
                    return false;
                }
                let is_string = |value: &Option<String>| {
                    value.as_deref().is_some_and(|value| value.starts_with("s:"))
                };
                if is_string(&source_value) || is_string(&target_value) {
                    return false;
                }
            }
        }
        true
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
        // Enum literals carry the primitive value and nominal owner. Keep
        // enum-to-enum identity ahead of ordinary primitive relations.
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
                // `relater.go:238`: equal values of two enums related by
                // isEnumTypeRelatedTo. Otherwise two distinct literals.
                return Some(
                    source_key == target_key
                        && self.is_enum_type_related_to(*source_owner, *target_owner),
                );
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
        // The strict-subtype exception for a non-fresh empty anonymous object
        // is essential when narrowing unknown's empty constituent (relater.go:258).
        if s.intersects(TypeFlags::NON_PRIMITIVE)
            && self.checker.is_empty_anonymous_object_type(target)
        {
            return Some(true);
        }
        if s.intersects(TypeFlags::OBJECT) && t.intersects(TypeFlags::NON_PRIMITIVE) {
            return Some(
                !(self.relation == Relation::StrictSubtype
                    && self.checker.is_empty_anonymous_object_type(source)
                    && !self.checker.fresh_object_literal_types.contains(&source)),
            );
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
            || self.is_related_to(source, target) != RelationResult::NotRelated
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
            return self.is_related_to(parts.types[0], target) != RelationResult::NotRelated;
        }
        false
    }

    /// The composite arms, guarded by the depth cap and the cycle cache.
    /// Ported from Checker.recursiveTypeRelatedTo (relater.go).
    fn recursive_type_related_to(
        &mut self,
        source: TypeId,
        target: TypeId,
        flags: RecursionFlags,
    ) -> RelationResult {
        let key = (source, target, self.intersection_target);
        match self.cached_result(key) {
            Some(CachedRelation::Succeeded) => return RelationResult::Related,
            // Native re-runs a cached failure when it elaborates errors
            // (`relater.go:3069`). This port elaborates only the direct pair's
            // signature arity, so only that pair is re-run.
            Some(CachedRelation::Failed) if self.diagnostic_pair != Some((source, target)) => {
                return RelationResult::NotRelated;
            }
            _ => {}
        }
        if self.maybe_keys_set.contains(&key) {
            return RelationResult::Maybe;
        }
        if self.depth >= MAX_DEPTH {
            // Depth refusal is uncomputed in the port, not reusable success.
            reasons::note(reasons::Site::DepthCap);
            return RelationResult::Unknown;
        }
        let maybe_start = self.maybe_keys.len();
        self.maybe_keys.push(key);
        self.maybe_keys_set.insert(key);
        self.depth += 1;
        let previous = self.expanding;
        if flags.contains(RecursionFlags::SOURCE) {
            self.source_stack.push(source);
            self.expanding.0 |= self.checker.is_deeply_nested_type(source, &self.source_stack, 3);
        }
        if flags.contains(RecursionFlags::TARGET) {
            self.target_stack.push(target);
            self.expanding.1 |= self.checker.is_deeply_nested_type(target, &self.target_stack, 3);
        }
        let related = if self.expanding == (true, true) {
            RelationResult::Maybe
        } else {
            self.structured_type_related_to(source, target)
        };
        self.expanding = previous;
        if flags.contains(RecursionFlags::SOURCE) {
            self.source_stack.pop();
        }
        if flags.contains(RecursionFlags::TARGET) {
            self.target_stack.pop();
        }
        self.depth -= 1;
        match related {
            RelationResult::Related => self.reset_maybe_stack(maybe_start, true),
            RelationResult::Maybe => {
                if self.source_stack.is_empty() && self.target_stack.is_empty() {
                    self.reset_maybe_stack(maybe_start, true);
                }
                // Otherwise retain assumptions for the enclosing proof.
            }
            RelationResult::CircularVariance => {
                if self.source_stack.is_empty() && self.target_stack.is_empty() {
                    self.reset_maybe_stack(maybe_start, false);
                }
                // Native retains nested circular keys for an enclosing proof,
                // but never publishes a top-level circular result as true.
            }
            RelationResult::NotRelated => {
                // Failure under assumptions also fails without them.
                self.publish_result(key, CachedRelation::Failed);
                self.reset_maybe_stack(maybe_start, false);
            }
            RelationResult::Unknown => {
                // Unsupported work is not a circular proof: discard its scope
                // even below depth zero. Another branch may supply a proof.
                self.reset_maybe_stack(maybe_start, false);
            }
        }
        related
    }

    /// resetMaybeStack (internal/checker/relater.go). Publish dependent keys
    /// only when the surrounding proof discharges their assumptions.
    fn reset_maybe_stack(&mut self, start: usize, succeeded: bool) {
        for index in start..self.maybe_keys.len() {
            let key = self.maybe_keys[index];
            self.maybe_keys_set.remove(&key);
            if succeeded {
                self.publish_result(key, CachedRelation::Succeeded);
            }
        }
        self.maybe_keys.truncate(start);
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
    fn structured_type_related_to(&mut self, source: TypeId, target: TypeId) -> RelationResult {
        let mut result = self.structured_type_related_to_worker(source, target);
        let target_is_union = self.checker.type_of(target).flags.contains(TypeFlags::UNION);
        if !result.is_success()
            && (self.checker.type_of(source).flags.contains(TypeFlags::INTERSECTION)
                || (self.checker.type_of(source).flags.contains(TypeFlags::TYPE_PARAMETER)
                    && target_is_union))
        {
            let types = self.intersection_constituents(source).unwrap_or_else(|| vec![source]);
            // getConstraintOfType of such a constituent is the mapped
            // substitution this port cannot reach; the combined constraint
            // would be built from a weaker one (`NonNullable<Partial<Config>[T]>`
            // against `Config[T]`, `correlatedUnions`).
            if result == RelationResult::NotRelated
                && types.iter().any(|&part| self.mapped_substitution_out_of_reach(part))
            {
                return RelationResult::Unknown;
            }
            if let Some(constraint) =
                self.checker.effective_constraint_of_intersection(&types, target_is_union)
                && constraint != source
                && self.union_constituents(constraint).is_none_or(|types| !types.contains(&source))
            {
                // structuredTypeRelatedTo (relater.go:3216) retries the combined
                // constraint after the individual constituents fail. An unknown
                // original relation is retained unless this supplies a proof.
                return RelationResult::any([
                    result,
                    self.is_related_to_with_flags(constraint, target, RecursionFlags::SOURCE),
                ]);
            }
        }
        // The intersection property-check pass (relater.go:3232): a target
        // intersection's combined property types meet the source's members
        // with IntersectionStateNone, detecting nested excess properties and
        // nested weak types the per-constituent walk (under
        // IntersectionStateTarget) does not check.
        //
        // Stated divergence: where this port cannot decide the pass (a
        // constituent whose member table it cannot enumerate, such as a
        // mapped type or a primitive's apparent members), the constituent
        // walk's verdict stands. Relating the source to every constituent
        // already relates each member to every contributing property type,
        // so an undecided pass can only be missing a nested excess or
        // weak-type failure, never a member the walk accepted wrongly.
        if result.is_success()
            && !self.intersection_target
            && let Some(constituents) = self.intersection_constituents(target)
            && self
                .checker
                .type_of(source)
                .flags
                .intersects(TypeFlags::OBJECT | TypeFlags::INTERSECTION)
            && !self.intersection_is_generic_object(&constituents)
        {
            let mut pass = self.properties_related_to(source, target);
            if pass.is_success() && self.checker.fresh_object_literal_types.contains(&source) {
                let index = self
                    .related_index_signatures(source, target)
                    .unwrap_or(RelationResult::Unknown);
                pass = RelationResult::all([pass, index]);
            }
            if pass != RelationResult::Unknown {
                result = RelationResult::all([result, pass]);
            }
        }
        result
    }

    /// The source switch's template-literal and string-mapping cases
    /// (`relater.go:3772`, `:3782`), reached after a failed union or
    /// intersection walk: `unionOrIntersectionRelatedTo`'s failure falls
    /// through for an instantiable source (`relater.go:3380`). A template
    /// source relates through its base constraint to a target that is
    /// neither an object nor a template; a string mapping through its base
    /// constraint to a target that is not a string mapping. `` `${T}` ``
    /// with `T extends "a" | "b"` then relates to `"a" | "b"` whole, where
    /// no single constituent accepts it. `None` when neither case applies.
    fn string_like_source_constraint(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<RelationResult> {
        let s = self.checker.type_of(source).flags;
        let t = self.checker.type_of(target).flags;
        let applies = (s.contains(TypeFlags::TEMPLATE_LITERAL)
            && !t.intersects(TypeFlags::OBJECT | TypeFlags::TEMPLATE_LITERAL))
            || (s.contains(TypeFlags::STRING_MAPPING) && !t.contains(TypeFlags::STRING_MAPPING));
        if !applies {
            return None;
        }
        let constraint = self.checker.base_constraint_of_type(source)?;
        if constraint == source || self.checker.is_gap(constraint) {
            return None;
        }
        Some(self.is_related_to_with_flags(constraint, target, RecursionFlags::SOURCE))
    }

    /// `isGenericObjectType` (`checker.go`) of a target intersection: some
    /// constituent is a type variable or other instantiable non-primitive, or
    /// a generic mapped type.
    fn intersection_is_generic_object(&mut self, constituents: &[TypeId]) -> bool {
        constituents.iter().any(|&part| {
            self.checker.type_of(part).flags.intersects(TypeFlags::INSTANTIABLE_NON_PRIMITIVE) || {
                self.checker.ensure_mapped_type_info(part);
                self.is_generic_mapped_target(part)
            }
        })
    }

    /// structuredTypeRelatedToWorker (internal/checker/relater.go).
    fn structured_type_related_to_worker(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> RelationResult {
        // structuredTypeRelatedToWorker (relater.go:3443): S[K] relates to
        // T[J] when both its object and index relate. Keep this inside the
        // recursive pair cache for indexed members of recursive interfaces.
        if let (Some(&(source_object, source_index, _)), Some(&(target_object, target_index, _))) = (
            self.checker.deferred_indexed_access_types.get(&source),
            self.checker.deferred_indexed_access_types.get(&target),
        ) {
            let objects = self.is_related_to(source_object, target_object);
            if objects != RelationResult::NotRelated {
                let indexes = self.is_related_to(source_index, target_index);
                let result = RelationResult::all([objects, indexes]);
                if result != RelationResult::NotRelated {
                    return result;
                }
                // A mapped type's iteration parameter carries its declared
                // constraint here, not the instance's: native's
                // getTypeParameterFromMappedType instantiates it per mapped
                // instance, so `P` of `MyMap<U>` is `keyof U` there and
                // `keyof T` here (`mappedTypeParameterConstraint`). An index
                // relation through one is no proof.
                if indexes == RelationResult::NotRelated
                    && (self.is_mapped_iteration_parameter(source_index)
                        || self.is_mapped_iteration_parameter(target_index))
                {
                    return RelationResult::Unknown;
                }
            }
            // getNormalizedType's getSimplifiedIndexedAccessType
            // (checker.go:27965) substitutes `{ [P in K]: E }[X]` to `E[P :=
            // X]` (plus `undefined` for a `?` mapped type) before any relation,
            // so native never relates such a pair as two indexed accesses.
            // The gate ports that substitution (`simplified_indexed_access`);
            // a generic mapped object still here is one it declined: an `as`
            // clause (whose filtering kind native does substitute), or a step
            // it could not compute. The unsimplified pair is no proof
            // (`mappedTypes5`'s `Partial<T>[P] -> T[P] | undefined`).
            if self.is_generic_mapped_type(source_object) != Some(false)
                || self.is_generic_mapped_type(target_object) != Some(false)
            {
                return RelationResult::Unknown;
            }
            return self.indexed_access_pair_after_components(source, target_object, target_index);
        }
        if let Some(constituents) = self.union_constituents(source) {
            // Every constituent of a source union must be related.
            // Upstream's `eachTypeRelatedToType` — except under the
            // comparable relation, where SOME constituent suffices
            // (`relater.go:2870`, `someTypeRelatedToType`). §750.
            let comparable = matches!(self.relation, Relation::Comparable);
            let parts = constituents
                .iter()
                .map(|&c| self.is_related_to_with_flags(c, target, RecursionFlags::SOURCE));
            return if comparable {
                RelationResult::any(parts)
            } else {
                RelationResult::all(parts)
            };
        }
        if let Some(constituents) = self.intersection_constituents(target) {
            // Related to every constituent of a target intersection.
            // Upstream's `typeRelatedToEachType` with IntersectionStateTarget
            // (relater.go:2879): the whole intersection's accepted names were
            // checked already, and nested comparisons skip their excess and
            // common-property checks; structured_type_related_to's property
            // pass checks them against the combined property types instead.
            let previous = std::mem::replace(&mut self.intersection_target, true);
            let parts = constituents
                .iter()
                .map(|&c| self.is_related_to_with_flags(source, c, RecursionFlags::TARGET));
            let result = RelationResult::all(parts);
            self.intersection_target = previous;
            // unionOrIntersectionRelatedTo's failure falls through for an
            // instantiable source (relater.go:3380): a conditional source
            // still meets the whole target through its constraints.
            if !result.is_success() && self.is_deferred_conditional(source) {
                let conditional = self.conditional_source_related_to(source, target);
                return RelationResult::any([result, conditional]);
            }
            if !result.is_success()
                && let Some(constraint) = self.string_like_source_constraint(source, target)
            {
                return RelationResult::any([result, constraint]);
            }
            // The same fallthrough for a type parameter: `S extends "a" |
            // "b"` relates to `"a" | "b"` through its constraint
            // (relater.go:3652), not through either constituent. Stated
            // divergence: an indexed-access source keeps the walk's answer
            // until flow.rs stops reading an undecided narrowing relation
            // as a decision (`docs/parity/notes/r5-relater5.md` §3; that
            // case loses 6 `quickinfoTypeAtReturnPositionsInaccurate` lines).
            if !result.is_success()
                && self.checker.type_of(source).flags.contains(TypeFlags::TYPE_PARAMETER)
                && let Some(variable) = self.type_variable_source_related_to(source, target)
            {
                return RelationResult::any([result, variable]);
            }
            // The fallthrough is not taken for an indexed-access source
            // (above). For `{ [P in K]: E }[X]` with a generic `X`, native's
            // goes on to the substitution and to `{ [P in K]: E }[constraint
            // of X]` (isMappedTypeGenericIndexedAccess, relater.go:3681):
            // `Funcs[K]` meets `Func<"a"> | Func<"b">` as `Funcs[keyof
            // ArgMap]` (`correlatedUnions`). Without those steps the failed
            // walk is no proof.
            if result == RelationResult::NotRelated
                && self.is_mapped_type_generic_indexed_access(source)
            {
                return RelationResult::Unknown;
            }
            return result;
        }
        if let Some(constituents) = self.union_constituents(target) {
            // Related to *some* constituent of a target union.
            // Upstream's `typeRelatedToSomeType`.
            let regular = self.checker.get_regular_type_of_object_literal(source);
            let parts = constituents
                .iter()
                .map(|&c| self.is_related_to_with_flags(regular, c, RecursionFlags::TARGET));
            let result = RelationResult::any(parts);
            // unionOrIntersectionRelatedTo's failure falls through for an
            // instantiable source (relater.go:3380): `T extends B ? number :
            // string` relates to `string | number` through its default
            // constraint, not through either constituent.
            if !result.is_success() && self.is_deferred_conditional(source) {
                let conditional = self.conditional_source_related_to(source, target);
                return RelationResult::any([result, conditional]);
            }
            if !result.is_success()
                && let Some(constraint) = self.string_like_source_constraint(source, target)
            {
                return RelationResult::any([result, constraint]);
            }
            // The same fallthrough for a type parameter: `S extends "a" |
            // "b"` relates to `"a" | "b"` through its constraint
            // (relater.go:3652), not through either constituent. Stated
            // divergence: an indexed-access source keeps the walk's answer
            // until flow.rs stops reading an undecided narrowing relation
            // as a decision (`docs/parity/notes/r5-relater5.md` §3; that
            // case loses 6 `quickinfoTypeAtReturnPositionsInaccurate` lines).
            if !result.is_success()
                && self.checker.type_of(source).flags.contains(TypeFlags::TYPE_PARAMETER)
                && let Some(variable) = self.type_variable_source_related_to(source, target)
            {
                return RelationResult::any([result, variable]);
            }
            // The fallthrough is not taken for an indexed-access source
            // (above). For `{ [P in K]: E }[X]` with a generic `X`, native's
            // goes on to the substitution and to `{ [P in K]: E }[constraint
            // of X]` (isMappedTypeGenericIndexedAccess, relater.go:3681):
            // `Funcs[K]` meets `Func<"a"> | Func<"b">` as `Funcs[keyof
            // ArgMap]` (`correlatedUnions`). Without those steps the failed
            // walk is no proof.
            if result == RelationResult::NotRelated
                && self.is_mapped_type_generic_indexed_access(source)
            {
                return RelationResult::Unknown;
            }
            // structuredTypeRelatedToWorker (relater.go:3889): an object or
            // intersection source that failed every constituent may still
            // cover a discriminated union, one constituent per combination
            // of its discriminant types. A constituent answer of Unknown
            // keeps the union walk's Unknown.
            if result == RelationResult::NotRelated
                && self
                    .checker
                    .type_of(source)
                    .flags
                    .intersects(TypeFlags::OBJECT | TypeFlags::INTERSECTION)
            {
                let object_only: Vec<TypeId> = constituents
                    .iter()
                    .copied()
                    .filter(|&c| {
                        self.checker.type_of(c).flags.intersects(
                            TypeFlags::OBJECT | TypeFlags::INTERSECTION | TypeFlags::SUBSTITUTION,
                        )
                    })
                    .collect();
                if object_only.len() > 1 {
                    let discriminated =
                        self.type_related_to_discriminated_type(source, &object_only);
                    if discriminated != RelationResult::NotRelated {
                        return discriminated;
                    }
                }
            }
            return result;
        }
        let source_intersection_result = if let Some(constituents) =
            self.intersection_constituents(source)
        {
            // Pinned 5b1047d unionOrIntersectionRelatedTo (relater.go:2884)
            // hoists instantiable constraints before SOME for primitive
            // Comparable targets. Otherwise T alone falsely admits T & null
            // against 42. This is a local projection using the existing
            // Checker/mapper-keyed constraint supplier and intersection interner;
            // no written type, member image or relation publication changes.
            let constituents = if self.relation == Relation::Comparable
                && self.checker.type_of(target).flags.intersects(TypeFlags::PRIMITIVE)
            {
                let mut constraints = Vec::with_capacity(constituents.len());
                for &part in &constituents {
                    let constraint =
                        if self.checker.type_of(part).flags.intersects(TypeFlags::INSTANTIABLE) {
                            match self.checker.base_constraint_of_type(part) {
                                Some(constraint) if !self.checker.is_gap(constraint) => constraint,
                                None if self
                                    .checker
                                    .type_parameter_symbols
                                    .get(&part)
                                    .is_some_and(|&symbol| {
                                        let declarations =
                                            &self.checker.binder.symbols().get(symbol).declarations;
                                        !declarations.is_empty()
                                            && declarations.iter().all(|&node| {
                                                matches!(
                                                    self.checker.node_map.get(node),
                                                    Some(tsr_ast::Node::TypeParameterDeclaration(p))
                                                        if p.constraint.is_none()
                                                ) && !matches!(
                                                    self.checker.nodes.parent(node).and_then(
                                                        |parent| self.checker.node_map.get(parent)
                                                    ),
                                                    Some(tsr_ast::Node::InferTypeNode(_))
                                                )
                                            })
                                    }) =>
                                {
                                    self.checker.intrinsics.unknown
                                }
                                // Active or unsupported constraints are not a
                                // proof of native's absent-constraint unknown.
                                _ => return RelationResult::Unknown,
                            }
                        } else {
                            part
                        };
                    constraints.push(constraint);
                }
                if constraints == constituents {
                    constituents
                } else {
                    let reduced = self.checker.get_intersection_type(&constraints, None);
                    if self.checker.type_of(reduced).flags.intersects(TypeFlags::NEVER) {
                        return RelationResult::NotRelated;
                    }
                    let Some(parts) = self.intersection_constituents(reduced) else {
                        let forward =
                            self.is_related_to_with_flags(reduced, target, RecursionFlags::SOURCE);
                        return if forward == RelationResult::NotRelated {
                            self.is_related_to_with_flags(target, reduced, RecursionFlags::SOURCE)
                        } else {
                            forward
                        };
                    };
                    parts
                }
            } else {
                constituents
            };
            // unionOrIntersectionRelatedTo first tries individual constituents.
            // A failed attempt must still reach the combined object comparison.
            let parts = constituents
                .iter()
                .map(|&c| self.is_related_to_with_flags(c, target, RecursionFlags::SOURCE));
            let result = RelationResult::any(parts);
            if result.is_success() {
                // structuredTypeRelatedTo's extra source-intersection check:
                // another constituent can supply an incompatible OPTIONAL
                // target property even when the first constituent suffices.
                if self.has_members(target)
                    && !constituents.contains(&target)
                    && self.tuple_relation_elements(target).is_none()
                    && self.checker.tuple_spread_array_element(target).is_none()
                    && !self.checker.spread_generic_flags(target, &mut Vec::new()).0
                {
                    let optionals = self.properties_related_to_with_optionals(source, target, true);
                    return RelationResult::all([result, optionals]);
                }
                return result;
            }
            if !self.has_members(target) {
                return result;
            }
            Some(result)
        } else {
            None
        };
        // Native 5b1047d structuredTypeRelatedToWorker (relater.go:3482):
        // keyof S relates to keyof T contravariantly, through T -> S. Written
        // keyof nodes and semantic IndexTypes share the retained operand, not
        // their spelling. Keep the proof inside this Relater's pair/assumption
        // scope; a circular Maybe is not a completed target-constraint proof.
        if let Some(&operand) = self.checker.deferred_keyof_operands.get(&target) {
            let direct = self
                .checker
                .deferred_keyof_operands
                .get(&source)
                .copied()
                .map(|source_operand| self.is_related_to(operand, source_operand));
            if direct.is_some_and(RelationResult::is_success) {
                return direct.unwrap();
            }
            let constraint =
                if self.checker.type_of(operand).flags.contains(TypeFlags::TYPE_PARAMETER) {
                    self.checker.type_parameter_constraint(operand)
                } else {
                    self.checker.base_constraint_of_type(operand)
                };
            let mut constraint_keys = None;
            if let Some(constraint) = constraint.filter(|&constraint| constraint != operand)
                && let Some(keys) = self.checker.resolved_keyof_type(constraint)
            {
                let result = self.is_related_to_with_flags(source, keys, RecursionFlags::TARGET);
                if result == RelationResult::Related {
                    return result;
                }
                constraint_keys = Some(result);
            }
            // An unimplemented operand relation cannot become a definite
            // negative merely by comparing the broad property-key domain.
            if direct == Some(RelationResult::Unknown) {
                return RelationResult::Unknown;
            }
            // A primitive source has no case of the source switch that could
            // still relate it (relater.go:3652-3812): its default case needs
            // an object or generic mapped target, so the worker ends False
            // (:3900). Decided only where this arm is native's: a type
            // parameter operand, whose constraint is getConstraintOfTypeParameter
            // either way, with no constraint or a definite failure against
            // its constraint's keys (native requires TernaryTrue there).
            let source_flags = self.checker.type_of(source).flags;
            if source_flags.intersects(TypeFlags::PRIMITIVE)
                && !source_flags.intersects(
                    TypeFlags::INSTANTIABLE | TypeFlags::UNION | TypeFlags::INTERSECTION,
                )
                && self.checker.type_of(operand).flags.contains(TypeFlags::TYPE_PARAMETER)
                && match constraint_keys {
                    None => constraint.is_none() && !self.declares_written_constraint(operand),
                    Some(result) => result == RelationResult::NotRelated,
                }
            {
                return RelationResult::NotRelated;
            }
        }
        // structuredTypeRelatedToWorker's type-parameter target arm
        // (relater.go:3435): comparability forbids relating two type
        // parameters unless one extends the other, so a type-parameter source
        // relates only through a constraint that itself mentions a type
        // parameter (`someType`), and otherwise is False.
        if self.relation == Relation::Comparable
            && self.checker.type_of(target).flags.contains(TypeFlags::TYPE_PARAMETER)
            && self.checker.type_of(source).flags.contains(TypeFlags::TYPE_PARAMETER)
        {
            let Some(constraint) = self.checker.type_parameter_constraint(source) else {
                // An unreadable written constraint is not native's absent one.
                return if self.declares_written_constraint(source) {
                    RelationResult::Unknown
                } else {
                    RelationResult::NotRelated
                };
            };
            // getConstraintOfTypeParameter answers nil for a circular
            // constraint (hasNonCircularBaseConstraint).
            let mut seen = vec![source];
            let mut link = constraint;
            while self.checker.type_of(link).flags.contains(TypeFlags::TYPE_PARAMETER) {
                if seen.contains(&link) {
                    return RelationResult::NotRelated;
                }
                seen.push(link);
                let Some(next) = self.checker.type_parameter_constraint(link) else { break };
                link = next;
            }
            let mentions_parameter = match self.union_constituents(constraint) {
                Some(types) => types
                    .iter()
                    .any(|&ty| self.checker.type_of(ty).flags.contains(TypeFlags::TYPE_PARAMETER)),
                None => self.checker.type_of(constraint).flags.contains(TypeFlags::TYPE_PARAMETER),
            };
            return if mentions_parameter {
                self.is_related_to_with_flags(constraint, target, RecursionFlags::SOURCE)
            } else {
                RelationResult::NotRelated
            };
        }
        if let Some(result) = self.generic_mapped_target_related_to(source, target)
            && result != RelationResult::NotRelated
        {
            return result;
        }
        if self.mapped_modifiers_reject(source, target) {
            return RelationResult::NotRelated;
        }
        // structuredTypeRelatedToWorker's type-parameter target arm
        // (relater.go:3423): comparability forbids relating two type
        // parameters unless one extends the other — a source parameter whose
        // constraint mentions a type parameter relates through that
        // constraint, any other source parameter is not comparable. Both
        // sides must be declared parameters; an unreadable written
        // constraint keeps the pair undecided.
        if let Some(result) = self.comparable_type_parameter_pair(source, target) {
            return result;
        }
        if let Some(result) = self.type_variable_source_related_to(source, target) {
            return result;
        }
        // A deferred keyof without a target IndexType inhabits the property-key
        // domain (relater.go:3694). The concrete operand/mapper stays intact.
        if self.checker.deferred_keyof_operands.contains_key(&source) {
            let keys = self.checker.get_union_type(&[
                self.checker.intrinsics.string,
                self.checker.intrinsics.number,
                self.checker.intrinsics.es_symbol,
            ]);
            return self.is_related_to_with_flags(keys, target, RecursionFlags::SOURCE);
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
                    RelationResult::NotRelated
                } else {
                    RelationResult::Related
                };
            }
            let Some(matches) = self.checker.template_literal_inferences(source, &parts) else {
                return RelationResult::NotRelated;
            };
            return if matches
                .into_iter()
                .zip(parts.types)
                .all(|(source, target)| self.valid_template_placeholder(source, target))
            {
                RelationResult::Related
            } else {
                RelationResult::NotRelated
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
                    RelationResult::NotRelated
                };
            }
            return if self.checker.is_member_of_string_mapping(source, target) {
                RelationResult::Related
            } else {
                RelationResult::NotRelated
            };
        }
        if let Some(answer) = self.generic_mapped_apparent_source_related_to(source, target) {
            return answer;
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
        if self.is_pure_signature_type(source) && self.is_pure_signature_type(target) {
            return self.related_signatures(source, target).unwrap_or(RelationResult::Unknown);
        }
        // Native's late apparent-source phase (relater.go:3814) combines an
        // intersection's instantiable constraints before comparing members.
        // A union-shaped apparent source does not re-enter union dispatch here;
        // open mapped sources may enter only when each generic constituent has
        // complete bounded members. Other generic shapes retain their verdict.
        let generic_source = source_intersection_result.is_some()
            && self.checker.spread_generic_flags(source, &mut Vec::new()).0;
        let source = if let Some(intersection_result) = source_intersection_result {
            let apparent = self.checker.apparent_type(source);
            let generic_apparent = self.checker.spread_generic_flags(apparent, &mut Vec::new()).0;
            let bounded_members = !generic_apparent
                || self
                    .intersection_constituents(apparent)
                    .unwrap_or_else(|| vec![apparent])
                    .into_iter()
                    .all(|part| {
                        if !self.checker.spread_generic_flags(part, &mut Vec::new()).0 {
                            return true;
                        }
                        self.checker.is_generic_homomorphic_mapped_type(part)
                            && self.checker.get_property_names_of_type(part).is_some()
                            && self.has_members(part)
                    });
            // Newly exposed mapped constituents can prove a missing member,
            // but that is not a proof about the combined source's nominal
            // requirements. Keep unsupported targets at their prior Unknown.
            if generic_apparent
                && bounded_members
                && !self.checker.get_property_names_of_type(target).is_some_and(|names| {
                    names.into_iter().all(|name| {
                        self.checker.get_property_of_type(target, &name).is_some_and(|property| {
                            !self.checker.property_has_modifier(
                                property,
                                tsr_ast::SyntaxKind::PrivateKeyword,
                            ) && !self.checker.property_has_modifier(
                                property,
                                tsr_ast::SyntaxKind::ProtectedKeyword,
                            )
                        })
                    })
                })
            {
                return RelationResult::Unknown;
            }
            if !self
                .checker
                .type_of(apparent)
                .flags
                .intersects(TypeFlags::OBJECT | TypeFlags::INTERSECTION)
                || !bounded_members
            {
                return intersection_result;
            }
            apparent
        } else {
            source
        };
        // relateVariances (internal/checker/relater.go): shared reference
        // targets compare their arguments in the measured directions. Marker
        // instances and an active recursive measurement compare structurally.
        if !self.checker.variance_marker_types.contains(&source)
            && !self.checker.variance_marker_types.contains(&target)
            && let Some(((source_symbol, source_arguments), (target_symbol, target_arguments))) =
                self.checker.same_target_references(source, target)
            && source_symbol == target_symbol
            && source_arguments.len() == target_arguments.len()
        {
            // structuredTypeRelatedToWorker's alias arm (relater.go:3389-3392)
            // probes alias variance only when the SOURCE is an Object or
            // Conditional type: other aliased types are interned and may or
            // may not carry their alias. This port mints every generic alias
            // instantiation as a named reference, so the native flags are the
            // evaluated body's (`compute_base_constraint` reads the same
            // body). A union, intersection or primitive body relates
            // structurally, body to body: `SearchResult<undefined>` to
            // `SearchResult<string>` with `SearchResult<T> = { value: T |
            // undefined } | undefined` is decided member by member
            // (tsr-2zk.927). An unevaluable body keeps the variance road.
            if let Some((source_body, target_body)) =
                self.non_object_alias_bodies(source_symbol, &source_arguments, &target_arguments)
            {
                return self.is_related_to(source_body, target_body);
            }
            let measured = self.checker.inference_variances(source_symbol);
            let variances = match measured {
                Some(variances)
                    if variances.is_empty()
                        && !source_arguments.is_empty()
                        && self.checker.variance_in_progress.contains(&source_symbol) =>
                {
                    // Native getVariances signals this target's active
                    // measurement with an empty slice. Re-entering its members
                    // here would measure the same recursive occurrences again.
                    return RelationResult::CircularVariance;
                }
                Some(variances) if variances.len() == source_arguments.len() => variances,
                None if !self.checker.variance_in_progress.is_empty() => {
                    return RelationResult::Unknown;
                }
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
                            RelationResult::all([forward, reverse])
                        }
                        Variance::Bivariant => {
                            let forward = self.is_related_to(source, target);
                            let reverse = self.is_related_to(target, source);
                            RelationResult::any([forward, reverse])
                        }
                        Variance::Independent => RelationResult::Related,
                    });
                }
                let result = RelationResult::all(parts);
                if result != RelationResult::NotRelated || !allows_covariant_void {
                    return result;
                }
            }
        }
        // The conditional arms (relater.go:3540, :3721), after alias variance
        // as in native. A conditional source is never related structurally:
        // its case of the source switch either relates or the worker answers
        // False. A conditional target that its own arm did not relate admits
        // no other arm for a source that is not a type variable (type
        // variables took their constraint arms above).
        let target_conditional = if self.is_deferred_conditional(target) {
            let result = self.conditional_target_related_to(source, target);
            if result.is_success() {
                return result;
            }
            Some(result)
        } else {
            None
        };
        if self.is_deferred_conditional(source) {
            let result = self.conditional_source_related_to(source, target);
            return match target_conditional {
                Some(RelationResult::Unknown) if result == RelationResult::NotRelated => {
                    RelationResult::Unknown
                }
                _ => result,
            };
        }
        if let Some(result) = target_conditional {
            return result;
        }
        // The source switch's default case against a generic mapped target
        // (relater.go:3805) precedes the apparent type and the structural arm.
        if self.is_generic_mapped_target(target)
            && let Some(result) = self.generic_mapped_target_default(source, target)
        {
            return RelationResult::any(source_intersection_result.into_iter().chain([result]));
        }
        // structuredTypeRelatedToWorker's structural arm (relater.go:3864)
        // against an intrinsic object with no members: propertiesRelatedTo
        // and indexSignaturesRelatedTo have nothing to require, and
        // signaturesRelatedTo (relater.go:4449) rejects every source of the
        // anyFunctionType wildcard but the wildcard itself (identity above).
        if self.is_memberless_object_intrinsic(target)
            && (self.has_members(source)
                || source_intersection_result.is_some()
                || self.is_structural_tuple_source(source))
        {
            let structural = if self.checker.any_function_type == Some(target) {
                RelationResult::NotRelated
            } else {
                RelationResult::Related
            };
            return RelationResult::any(source_intersection_result.into_iter().chain([structural]));
        }
        // structuredTypeRelatedToWorker (relater.go:3853): under the subtype
        // relations a fresh empty object literal `{}` admits only empty
        // sources. Emptiness is certified here only for sources whose members
        // are known to be non-empty: a tuple (it has `length`) or a type with
        // enumerated properties.
        if matches!(self.relation, Relation::Subtype | Relation::StrictSubtype)
            && self.checker.fresh_object_literal_types.contains(&target)
            && self.checker.is_empty_anonymous_object_type(target)
            && (self.is_structural_tuple_source(source)
                || self
                    .checker
                    .get_property_names_of_type(source)
                    .is_some_and(|names| !names.is_empty()))
        {
            return RelationResult::NotRelated;
        }
        // The structural arm runs only against an object target
        // (relater.go:3864); a conditional this port gives a member image is
        // not one.
        if (self.has_members(source)
            || source_intersection_result.is_some()
            || (self.is_structural_tuple_source(source)
                && self.checker.type_of(target).flags.contains(TypeFlags::OBJECT)))
            && self.has_members(target)
        {
            // structuredTypeRelatedToWorker (relater.go:3864): properties,
            // call/construct signatures and indexes are independent conjuncts.
            // Index infos can be synthesized by literals or mapped types, so
            // inspecting only binder declarations misses a target requirement.
            let properties = self.properties_related_to(source, target);
            if properties == RelationResult::NotRelated {
                return RelationResult::any(
                    source_intersection_result.into_iter().chain([properties]),
                );
            }
            let signatures = if self.call_or_construct_bearing(target) {
                self.related_signatures(source, target).unwrap_or_else(|| {
                    reasons::note(reasons::Site::SignatureBearing);
                    RelationResult::Unknown
                })
            } else {
                RelationResult::Related
            };
            if signatures == RelationResult::NotRelated {
                return RelationResult::any(
                    source_intersection_result.into_iter().chain([signatures]),
                );
            }
            let indexes = if !self.has_members(source)
                && source_intersection_result.is_none()
                && self.is_structural_tuple_source(source)
            {
                self.tuple_index_signatures_related(source, target)
            } else {
                self.related_index_signatures(source, target)
            }
            .unwrap_or(RelationResult::Unknown);
            let result = RelationResult::all([properties, signatures, indexes]);
            // Only a completed comparison widens the generic source boundary.
            // Unsupported members (e.g. protected-target checks) must not turn
            // its previous constituent rejection into a non-false inference.
            if generic_source && result == RelationResult::Unknown {
                return source_intersection_result.unwrap();
            }
            return RelationResult::any(source_intersection_result.into_iter().chain([result]));
        }
        // Reached only by a type whose *flags* say union or intersection while
        // its data says otherwise, which `is_related_to`'s gate lets through.
        // Nothing was compared, so nothing was decided.
        reasons::note(reasons::Site::CompositeShape);
        RelationResult::Unknown
    }

    /// The source switch's type-variable case (relater.go:3652): an indexed
    /// access or type parameter source explores its constraint. `None` when
    /// the source is not one this case decides. Shared by the worker and the
    /// union/intersection-target fallthrough (relater.go:3380).
    fn type_variable_source_related_to(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<RelationResult> {
        // The source-variable branch also explores an indexed access's
        // constraint, except when both operands are indexed accesses and the
        // object/index comparison above owns the relation (relater.go:3665).
        if self.checker.type_of(source).flags.contains(TypeFlags::INDEXED_ACCESS)
            && !self.checker.type_of(target).flags.contains(TypeFlags::INDEXED_ACCESS)
        {
            // getConstraintOfType (relater.go:3667): for `{ [P in K]: E }[X]`
            // that is the substitution `E[P := X]`, not the base constraint
            // `{ [P in K]: E }[constraint of X]` (getConstraintFromIndexedAccess,
            // checker.go:17227). The base constraint stays the road where the
            // port's constraint is undecided. A lazily captured mapped
            // object is captured first, as getConstraintTypeFromMappedType
            // resolves on first use.
            if let Some(&(object, index, _)) =
                self.checker.deferred_indexed_access_types.get(&source)
            {
                self.checker.ensure_mapped_type_info(object);
                // isMappedTypeGenericIndexedAccess needs the object's mapped
                // identity. A concrete instance of a mapped alias
                // (`Partial<Foo1>`) is resolved to its members here, so the
                // substitution is out of reach and no constraint this port
                // computes is native's.
                let _ = (object, index);
                if self.mapped_substitution_out_of_reach(source) {
                    return Some(RelationResult::Unknown);
                }
            }
            match self.checker.constraint_of_type(source) {
                crate::constraints::ConstraintOfType::Constraint(constraint)
                    if constraint != source =>
                {
                    return Some(self.is_related_to_with_flags(
                        constraint,
                        target,
                        RecursionFlags::SOURCE,
                    ));
                }
                crate::constraints::ConstraintOfType::Nil => {
                    let unknown = self.checker.intrinsics.unknown;
                    return Some(self.is_related_to_with_flags(
                        unknown,
                        target,
                        RecursionFlags::SOURCE,
                    ));
                }
                _ => {}
            }
            if let Some(constraint) = self.checker.base_constraint_of_type(source) {
                return Some(if constraint == source {
                    RelationResult::Unknown
                } else {
                    self.is_related_to_with_flags(constraint, target, RecursionFlags::SOURCE)
                });
            }
            // No base constraint: native relates getConstraintOfType, or
            // `unknown` for a nil one (relater.go:3668).
            return Some(match self.checker.constraint_of_type(source) {
                crate::constraints::ConstraintOfType::Constraint(constraint)
                    if constraint != source =>
                {
                    self.is_related_to_with_flags(constraint, target, RecursionFlags::SOURCE)
                }
                crate::constraints::ConstraintOfType::Nil => {
                    let unknown = self.checker.intrinsics.unknown;
                    self.is_related_to_with_flags(unknown, target, RecursionFlags::SOURCE)
                }
                _ => RelationResult::Unknown,
            });
        }
        // Synthetic polymorphic this is a source type variable too, not the
        // object member table carried by its representation (relater.go:3665).
        if self.checker.type_of(source).flags.contains(TypeFlags::TYPE_PARAMETER)
            && !self.checker.type_parameter_symbols.contains_key(&source)
            && let Some(constraint) = self.checker.type_parameter_constraint(source)
            && constraint != source
        {
            return Some(self.is_related_to_with_flags(constraint, target, RecursionFlags::SOURCE));
        }
        // Source type variables explore their constraint under the same cycle
        // guard (relater.go:3664). An unreadable written constraint is unknown.
        if self.checker.type_of(source).flags.contains(TypeFlags::TYPE_PARAMETER)
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
                    return Some(RelationResult::Unknown);
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
                    return Some(RelationResult::Unknown);
                }
                seen.push(constraint);
                let Some(next) = self.checker.type_parameter_constraint(constraint) else { break };
                constraint = next;
            }
            return Some(self.is_related_to_with_flags(constraint, target, RecursionFlags::SOURCE));
        }
        None
    }

    /// Whether `id` is a deferred conditional type. The CONDITIONAL flag
    /// marks a conditional alias reference and a mapped template's
    /// conditional; an inline conditional elsewhere is minted with OBJECT
    /// flags (`declared.rs` §906) and is known by its retained root.
    fn is_conditional(&self, id: TypeId) -> bool {
        // Mapped templates' conditionals carry CONDITIONAL; inline mints, the
        // only `conditional_inference_nodes` keys, carry OBJECT. Testing the
        // flags first keeps the hot relation entry to one load per side.
        let flags = self.checker.type_of(id).flags;
        flags.contains(TypeFlags::CONDITIONAL)
            || (flags.contains(TypeFlags::OBJECT)
                && self.checker.conditional_inference_nodes.contains_key(&id))
    }

    /// [`Self::is_conditional`] with a generic check type, the only kind
    /// native defers (`conditional_inference_operands` answers `None`
    /// otherwise).
    fn is_deferred_conditional(&mut self, id: TypeId) -> bool {
        self.is_conditional(id) && self.checker.conditional_inference_operands(id).is_some()
    }

    /// A deferred conditional whose print-only mint carries OBJECT flags
    /// (`declared.rs` §906, an inline conditional outside a mapped template).
    fn is_object_flagged_conditional(&self, id: TypeId) -> bool {
        !self.checker.type_of(id).flags.contains(TypeFlags::CONDITIONAL) && self.is_conditional(id)
    }

    /// The written `ConditionalTypeNode` of a deferred conditional, where this
    /// port retains one the relater can read: a mapped template's root, or
    /// the body of the referenced conditional alias. `None` for an inline
    /// conditional (its root is private to `declared.rs`) and for an alias
    /// whose body is another alias reference. Answers `(node, has_infer,
    /// distributive)`: `root.inferTypeParameters != nil` (the binder's type
    /// parameter locals of the node) and `root.isDistributive` (a naked
    /// type-parameter check type).
    fn conditional_root(&self, id: TypeId) -> Option<(tsr_ast::NodeId, bool, bool)> {
        let node = if let Some(info) = self.checker.mapped_conditionals.get(&id) {
            match self.checker.node_map.get(info.declaration) {
                Some(tsr_ast::Node::ConditionalTypeNode(node)) => node,
                _ => return None,
            }
        } else {
            let (symbol, _) = self.checker.type_reference_targets.get(&id)?;
            let declaration = self.checker.type_alias_declaration_of(*symbol)?;
            let Some(tsr_ast::Node::TypeAliasDeclaration(alias)) =
                self.checker.node_map.get(declaration)
            else {
                return None;
            };
            let tsr_ast::TypeNode::ConditionalTypeNode(node) = alias.r#type? else {
                return None;
            };
            node
        };
        let id = node.node_id?;
        let has_infer = self.checker.binder.locals(id).is_some_and(|locals| {
            locals.values().any(|&symbol| {
                self.checker
                    .binder
                    .symbols()
                    .get(symbol)
                    .flags
                    .contains(SymbolFlags::TYPE_PARAMETER)
            })
        });
        let distributive = node
            .check_type
            .and_then(|check| self.checker.distributive_conditional_parameter(check))
            .is_some();
        Some((id, has_infer, distributive))
    }

    /// getSimplifiedConditionalType (checker.go:28006) for a deferred
    /// conditional `id`; any other type is returned unchanged. `None` when the
    /// restrictive-instantiation test it needs is not decidable here.
    fn simplified_conditional(&mut self, id: TypeId) -> Option<TypeId> {
        if !self.is_conditional(id) {
            return Some(id);
        }
        let Some([check, extends, yes, no]) = self.checker.conditional_inference_operands(id)
        else {
            return Some(id);
        };
        let never = self.checker.intrinsics.never;
        let check_any = self.checker.type_of(check).flags.intersects(TypeFlags::ANY);
        let no_never = self.checker.type_of(no).flags.contains(TypeFlags::NEVER);
        let yes_never = self.checker.type_of(yes).flags.contains(TypeFlags::NEVER);
        if no_never && yes == check {
            if check_any || self.restrictive_assignable(check, extends)? {
                return self.simplified_conditional(yes);
            }
            if self.intersection_is_empty(check, extends) {
                return Some(never);
            }
        } else if yes_never && no == check {
            if !check_any && self.restrictive_assignable(check, extends)? {
                return Some(never);
            }
            if check_any || self.intersection_is_empty(check, extends) {
                return self.simplified_conditional(no);
            }
        }
        Some(id)
    }

    /// `isTypeAssignableTo(getRestrictiveInstantiation(check),
    /// getRestrictiveInstantiation(extends))` where it reduces to flags: the
    /// same type; a naked type parameter (unconstrained once restricted)
    /// against a parameter-free extends type, which only a top type accepts.
    fn restrictive_assignable(&mut self, check: TypeId, extends: TypeId) -> Option<bool> {
        if check == extends {
            return Some(true);
        }
        let check_is_parameter =
            self.checker.type_of(check).flags.contains(TypeFlags::TYPE_PARAMETER);
        if check_is_parameter && !self.checker.mentions_registered_type_parameter(extends) {
            return Some(self.checker.type_of(extends).flags.intersects(TypeFlags::ANY_OR_UNKNOWN));
        }
        // Two distinct restrictive parameters have no constraints to relate.
        if check_is_parameter
            && self.checker.type_of(extends).flags.contains(TypeFlags::TYPE_PARAMETER)
        {
            return Some(false);
        }
        None
    }

    /// isIntersectionEmpty (checker.go:28029).
    fn intersection_is_empty(&mut self, left: TypeId, right: TypeId) -> bool {
        let intersection = self.checker.get_intersection_type(&[left, right], None);
        let union = self.checker.get_union_type(&[intersection, self.checker.intrinsics.never]);
        self.checker.type_of(union).flags.contains(TypeFlags::NEVER)
    }

    /// getConditionalType's `forConstraint` extra (checker.go): instantiated
    /// at the check type's constraint `C`, a check that is not assignable to
    /// the extends type `E` still includes the true branch when some
    /// constituent of `E` is assignable to `C` (`Foo<T extends string>` with
    /// `T extends "abc" | 42 ? true : false` is `boolean`, not `false`).
    /// `declared.rs`'s capture evaluates without it, so it is added here.
    /// `None` where the permissive instantiations are not the written types
    /// (a generic `E`) or the true branch mentions the check type.
    fn for_constraint_extra(
        &mut self,
        captured: TypeId,
        operands: Option<[TypeId; 4]>,
    ) -> Option<TypeId> {
        let [check, extends, yes, _] = operands?;
        let constraint = if self.checker.type_of(check).flags.contains(TypeFlags::TYPE_PARAMETER) {
            self.checker.type_parameter_constraint(check)?
        } else {
            self.checker.base_constraint_of_type(check)?
        };
        if self.checker.mentions_registered_type_parameter(extends)
            || self.checker.mentions_registered_type_parameter(constraint)
        {
            return None;
        }
        let forward = self.is_related_to(constraint, extends);
        if forward.is_success() {
            return Some(captured);
        }
        if forward == RelationResult::Unknown {
            return None;
        }
        if self.checker.type_of(extends).flags.contains(TypeFlags::NEVER) {
            return Some(captured);
        }
        let parts = self.union_constituents(extends).unwrap_or_else(|| vec![extends]);
        let mut overlaps = RelationResult::NotRelated;
        for part in parts {
            overlaps = RelationResult::any([overlaps, self.is_related_to(part, constraint)]);
            if overlaps.is_success() {
                break;
            }
        }
        match overlaps {
            RelationResult::NotRelated => Some(captured),
            RelationResult::Unknown => None,
            _ if self.checker.mentions_type_parameter(yes, &[check], &[]) => None,
            _ => Some(self.checker.get_union_type(&[captured, yes])),
        }
    }

    /// The conditional-target arm of structuredTypeRelatedToWorker
    /// (relater.go:3540). Applies when the root has no `infer` positions, is
    /// not distribution dependent, and the source is not an instantiation of
    /// the same root. `skipTrue`/`skipFalse` ask whether the permissive
    /// (restrictive) instantiations of the check and extends types relate;
    /// this port has neither instantiation, so they are computed only where
    /// they reduce to flags: a naked type-parameter check against a
    /// parameter-free extends type, or a naked type-parameter extends type.
    /// Anything else, and a root the relater cannot read, is `Unknown`.
    fn conditional_target_related_to(&mut self, source: TypeId, target: TypeId) -> RelationResult {
        if self.checker.is_deeply_nested_type(target, &self.target_stack, 10) {
            return RelationResult::Maybe;
        }
        let Some((root, has_infer, distributive)) = self.conditional_root(target) else {
            return RelationResult::Unknown;
        };
        // Native skips the arm for a root with `infer` positions, and the
        // worker then answers False. Stated divergence inside a
        // conditional-alias evaluation frame: Unknown. The evaluator's infer
        // road (`declared.rs` `evaluate_conditional_inference`) relates the
        // raw check and extends types there and takes a False as "definitely
        // false", where native relates their permissive instantiations;
        // answering False collapses deferred recursive conditionals
        // (`ramdaToolsNoInfinite`'s `Tail<Tail<T>>`). Elsewhere (narrowing
        // by `a is A` against `ReturnType<T[M]>`) native's False stands.
        if has_infer {
            let framed = !self.checker.alias_evaluation_bindings.is_empty()
                || self.checker.mapped_template_depth > 0;
            return if framed { RelationResult::Unknown } else { RelationResult::NotRelated };
        }
        let Some([check, extends, yes, no]) = self.checker.conditional_inference_operands(target)
        else {
            return RelationResult::Unknown;
        };
        // isDistributionDependent (checker.go): a distributive root whose
        // branches reference the check type parameter.
        let check_is_parameter =
            self.checker.type_of(check).flags.contains(TypeFlags::TYPE_PARAMETER);
        if distributive {
            if !check_is_parameter {
                return RelationResult::Unknown;
            }
            if self.checker.mentions_type_parameter(yes, &[check], &[])
                || self.checker.mentions_type_parameter(no, &[check], &[])
            {
                return RelationResult::NotRelated;
            }
        }
        if self.is_conditional(source)
            && self.conditional_root(source).is_some_and(|(source_root, ..)| source_root == root)
        {
            return RelationResult::NotRelated;
        }
        let extends_flags = self.checker.type_of(extends).flags;
        let extends_is_parameter = extends_flags.contains(TypeFlags::TYPE_PARAMETER);
        let (skip_true, skip_false) =
            if check_is_parameter && !self.checker.mentions_registered_type_parameter(extends) {
                // permissive(T) is the wildcard, assignable to all but never;
                // restrictive(T) is unconstrained, assignable only to a top type.
                let skip_true = extends_flags.contains(TypeFlags::NEVER);
                let skip_false = !skip_true && extends_flags.intersects(TypeFlags::ANY_OR_UNKNOWN);
                (skip_true, skip_false)
            } else if extends_is_parameter && !check_is_parameter {
                // permissive(U) is the wildcard: the true branch is live.
                let check_flags = self.checker.type_of(check).flags;
                if check_flags.intersects(TypeFlags::ANY | TypeFlags::NEVER) {
                    return RelationResult::Unknown;
                }
                (false, false)
            } else if check_is_parameter && extends == check {
                (false, true)
            } else {
                return RelationResult::Unknown;
            };
        let mut result = if skip_true {
            RelationResult::Related
        } else {
            self.is_related_to_with_flags(source, yes, RecursionFlags::TARGET)
        };
        if result != RelationResult::NotRelated && !skip_false {
            let false_result = self.is_related_to_with_flags(source, no, RecursionFlags::TARGET);
            result = RelationResult::all([result, false_result]);
        }
        result
    }

    /// The conditional-source arm of structuredTypeRelatedToWorker
    /// (relater.go:3721): conditional to conditional, then the default
    /// constraint, then (for a non-conditional target) the distributive
    /// constraint. A step this port cannot take answers `Unknown` rather than
    /// False:
    /// - getConditionalFlowTypeOfType is not applied to the true branch
    ///   (`Extract<T, U>`'s true branch is `T & U` natively), so a failed
    ///   default constraint whose true branch mentions the check type is no
    ///   proof;
    /// - the distributive constraint is known only where `declared.rs`
    ///   captured it for an alias reference;
    /// - a conditional-to-conditional pair whose source declares `infer`
    ///   parameters needs inference this arm does not run.
    fn conditional_source_related_to(&mut self, source: TypeId, target: TypeId) -> RelationResult {
        if self.checker.is_deeply_nested_type(source, &self.source_stack, 10) {
            return RelationResult::Maybe;
        }
        let mut undecided = false;
        let operands = self.checker.conditional_inference_operands(source);
        let target_conditional = self.is_conditional(target);
        if target_conditional {
            let source_infer = self.conditional_root(source).map(|(_, has_infer, _)| has_infer);
            match (source_infer, operands) {
                (Some(false), Some([source_check, source_extends, source_yes, source_no])) => {
                    if let Some([target_check, target_extends, target_yes, target_no]) =
                        self.checker.conditional_inference_operands(target)
                    {
                        // isTypeIdenticalTo(sourceExtends, targetExtends):
                        // interned identity, or mutual relation as the
                        // necessary condition this port can test.
                        let identical = if source_extends == target_extends {
                            RelationResult::Related
                        } else {
                            let forward = self.is_related_to(source_extends, target_extends);
                            let reverse = self.is_related_to(target_extends, source_extends);
                            match RelationResult::all([forward, reverse]) {
                                RelationResult::NotRelated => RelationResult::NotRelated,
                                _ => RelationResult::Unknown,
                            }
                        };
                        let checks = if identical == RelationResult::NotRelated {
                            RelationResult::NotRelated
                        } else {
                            let forward = self.is_related_to(source_check, target_check);
                            if forward.is_success() {
                                forward
                            } else {
                                RelationResult::any([
                                    forward,
                                    self.is_related_to(target_check, source_check),
                                ])
                            }
                        };
                        let gate = RelationResult::all([identical, checks]);
                        if gate != RelationResult::NotRelated {
                            let mut result = self.is_related_to(source_yes, target_yes);
                            if result != RelationResult::NotRelated {
                                result = RelationResult::all([
                                    result,
                                    self.is_related_to(source_no, target_no),
                                ]);
                            }
                            let result = RelationResult::all([gate, result]);
                            if result.is_success() {
                                return result;
                            }
                            undecided |= result == RelationResult::Unknown;
                        }
                    } else {
                        undecided = true;
                    }
                }
                _ => undecided = true,
            }
        }
        // getDefaultConstraintOfConditionalType.
        match self.checker.default_constraint_of_conditional_type(source) {
            Some(constraint) => {
                let result =
                    self.is_related_to_with_flags(constraint, target, RecursionFlags::SOURCE);
                if result.is_success() {
                    return result;
                }
                undecided |= result == RelationResult::Unknown;
                // The true branch native relates is the flow type
                // (getConditionalFlowTypeOfType), narrower than this one.
                // getImpliedConstraint substitutes the check type variable
                // where the true branch references it, and an element of a
                // unary tuple check `[T] extends [U]`.
                match operands {
                    Some([check, _, yes, _]) => {
                        let unary_tuple = self
                            .checker
                            .tuple_element_lists
                            .get(&check)
                            .is_some_and(|(elements, _)| elements.len() == 1);
                        if self.checker.mentions_type_parameter(yes, &[check], &[])
                            || (unary_tuple && self.checker.mentions_registered_type_parameter(yes))
                        {
                            undecided = true;
                        }
                    }
                    None => undecided = true,
                }
            }
            None => undecided = true,
        }
        // getConstraintOfDistributiveConditionalType, only for a
        // non-conditional target.
        if !target_conditional {
            let check = operands.map(|[check, ..]| check);
            let distributive = match self.conditional_root(source) {
                Some((_, _, distributive)) => distributive,
                None => check.is_some_and(|check| {
                    self.checker.type_of(check).flags.contains(TypeFlags::TYPE_PARAMETER)
                }),
            };
            let constrained = check.is_none_or(|check| {
                !self.checker.type_of(check).flags.contains(TypeFlags::TYPE_PARAMETER)
                    || self
                        .checker
                        .type_parameter_constraint(check)
                        .is_some_and(|constraint| constraint != check)
                    || self.declares_written_constraint(check)
            });
            if distributive && constrained {
                // `declared.rs` captures the distributive instantiation of a
                // conditional alias reference as a (constraint, constraint)
                // pair alongside its base constraint.
                let _ = self.checker.base_constraint_of_type(source);
                let captured = match self.checker.conditional_constraint_branches.get(&source) {
                    Some(&(yes, no)) if yes == no => Some(yes),
                    _ => None,
                };
                match captured {
                    Some(captured) => {
                        // The forConstraint extra only adds a constituent, so
                        // a failure without it is already a failure.
                        let result =
                            self.is_related_to_with_flags(captured, target, RecursionFlags::SOURCE);
                        if result.is_success() {
                            let result = match self.for_constraint_extra(captured, operands) {
                                Some(distributive) if distributive == captured => result,
                                Some(distributive) => self.is_related_to_with_flags(
                                    distributive,
                                    target,
                                    RecursionFlags::SOURCE,
                                ),
                                None => RelationResult::Unknown,
                            };
                            if result.is_success() {
                                return result;
                            }
                            undecided |= result == RelationResult::Unknown;
                        } else {
                            undecided |= result == RelationResult::Unknown;
                        }
                    }
                    None => undecided = true,
                }
            } else if operands.is_none() {
                undecided = true;
            }
        }
        if undecided { RelationResult::Unknown } else { RelationResult::NotRelated }
    }

    /// Fixed and concrete-rest tuples in propertiesRelatedTo
    /// (internal/checker/relater.go). Generic variadic operands still require
    /// base-constraint resolution and retain an unknown relation here.
    fn tuples_related_to(&mut self, source: TypeId, target: TypeId) -> Option<RelationResult> {
        let (target_elements, target_readonly) = self.tuple_relation_elements(target)?;
        let target_generic = target_elements.iter().any(|element| {
            element.spread && self.checker.tuple_spread_array_element(element.r#type).is_none()
        });
        if !target_generic {
            let constraint = self.checker.tuple_base_constraint(source);
            if constraint != source {
                return Some(self.is_related_to_with_flags(
                    constraint,
                    target,
                    RecursionFlags::SOURCE,
                ));
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
            return Some(RelationResult::NotRelated);
        }
        if source_elements.iter().chain(&target_elements).any(|element| {
            element.spread && self.checker.tuple_spread_array_element(element.r#type).is_none()
        }) {
            return Some(RelationResult::Unknown);
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
            return Some(RelationResult::NotRelated);
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
                return Some(RelationResult::NotRelated);
            };
            if !target_element.optional
                && !target_element.spread
                && (element.optional || element.spread)
            {
                return Some(RelationResult::NotRelated);
            }
            let mut source_type = if element.spread {
                self.checker.tuple_spread_array_element(element.r#type)?
            } else {
                element.r#type
            };
            // Written tuples store optionality separately from the element
            // TypeId. Native type arguments already include this marker; keep
            // it when an optional source aligns with a non-optional rest.
            if element.optional && self.checker.strict_null_checks {
                let marker = if self.checker.exact_optional_property_types {
                    self.checker.intrinsics.missing
                } else {
                    self.checker.intrinsics.undefined
                };
                source_type = self.checker.get_union_type_unprinted(&[source_type, marker]);
            }
            let mut target_type = if target_element.spread {
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
            // propertiesRelatedTo removes target missing at optional positions,
            // and source missing only when both positions are optional.
            if self.checker.exact_optional_property_types && target_element.optional {
                target_type = self.checker.remove_missing_type(target_type);
                if element.optional {
                    source_type = self.checker.remove_missing_type(source_type);
                }
            }
            parts.push(self.is_related_to(source_type, target_type));
        }
        Some(RelationResult::all(parts))
    }

    /// `propertiesRelatedTo` (relater.go:4100) with a tuple target and a
    /// plain object source (not an array or tuple, so the arity arm does not
    /// apply). The target's properties are its leading fixed elements
    /// (`"0"`, `"1"`, …, required unless optional), `length`, and the global
    /// `Array`/`ReadonlyArray` members, all required. Answers `NotRelated`
    /// when the source's complete name table lacks a required one, or when a
    /// fixed element's property type is not related; `Unknown` otherwise (the
    /// method types are not compared, so no positive answer is given). `None`
    /// outside the shape.
    fn non_array_source_tuple_target(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<RelationResult> {
        let (elements, _) = self.tuple_relation_elements(target)?;
        if !self.checker.type_of(source).flags.contains(TypeFlags::OBJECT)
            || !self.has_members(source)
            || self.tuple_relation_elements(source).is_some()
            || self.checker.tuple_spread_array_element(source).is_some()
            || self.checker.mapped_types.contains_key(&source)
            || self.is_qualified_alias_mint(source)
            || elements.iter().any(|element| {
                element.spread && self.checker.tuple_spread_array_element(element.r#type).is_none()
            })
        {
            return None;
        }
        let names = self.checker.get_property_names_of_type(source)?;
        let target_properties = self.checker.tuple_target_properties(target)?;
        let fixed: Vec<_> =
            elements.iter().take_while(|element| !element.spread).cloned().collect();
        // getPropertyOfType: a name the table lacks can still be supplied by
        // the global Object augmentation (`toString`, …).
        let missing_required = target_properties.iter().any(|(name, optional)| {
            !optional
                && !names.contains(name)
                && self.checker.get_type_of_property_of_type(source, name).is_none()
        });
        if missing_required {
            return Some(RelationResult::NotRelated);
        }
        let mut parts = Vec::with_capacity(fixed.len());
        for (index, element) in fixed.iter().enumerate() {
            let name = index.to_string();
            let Some(source_type) = self.checker.get_type_of_property_of_type(source, &name) else {
                continue;
            };
            let target_type = if element.optional && self.checker.strict_null_checks {
                self.checker.get_union_type(&[element.r#type, self.checker.intrinsics.undefined])
            } else {
                element.r#type
            };
            parts.push(self.is_related_to(source_type, target_type));
        }
        // A plain tuple's `length` is the literal union of its possible
        // lengths (createNormalizedTupleType); a rest makes it `number`.
        if self.checker.tuple_element_lists.contains_key(&target)
            && let Some(target_length) = self.checker.get_type_of_property_of_type(target, "length")
            && let Some(source_length) = self.checker.get_type_of_property_of_type(source, "length")
        {
            parts.push(self.is_related_to(source_length, target_length));
        }
        if parts.contains(&RelationResult::NotRelated) {
            return Some(RelationResult::NotRelated);
        }
        Some(RelationResult::Unknown)
    }

    /// propertiesRelatedTo (relater.go:4100) for a tuple source and an
    /// object target that is neither an array nor a tuple: a required target
    /// property the tuple does not have (`getPropertyOfObjectType(source,
    /// name) == nil`, relater.go:4146) is `false` under every relation.
    /// `[] -> RegExpMatchArray` fails on `0`. Only that definite negative is
    /// taken; every other pair keeps the undecided fallthrough.
    fn tuple_source_non_array_target(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<RelationResult> {
        let source_properties = self.checker.tuple_target_properties(source)?;
        if !self.checker.type_of(target).flags.contains(TypeFlags::OBJECT)
            || self.tuple_relation_elements(target).is_some()
            || self.checker.tuple_spread_array_element(target).is_some()
            || self.checker.mapped_types.contains_key(&target)
        {
            return None;
        }
        // The target's OWN declared members suffice for a negative: an
        // inherited requirement can only add failures, never remove one.
        let TypeData::Named { members: Some(owner), .. } = self.checker.type_of(target).data else {
            return None;
        };
        let owner = self.checker.binder.merged_symbol(owner);
        let members: Vec<_> = self
            .checker
            .binder
            .symbols()
            .get(owner)
            .members
            .iter()
            .map(|(name, &symbol)| ((*name).to_string(), symbol))
            .collect();
        let missing = members.into_iter().any(|(name, symbol)| {
            self.checker
                .binder
                .symbols()
                .get(symbol)
                .flags
                .intersects(tsr_binder::SymbolFlags::PROPERTY | tsr_binder::SymbolFlags::METHOD)
                && !self.checker.property_is_optional(symbol)
                && !source_properties.iter().any(|(property, _)| *property == name)
                && self.checker.get_property_of_type(source, &name).is_none()
        });
        missing.then_some(RelationResult::NotRelated)
    }

    /// The intrinsic object types with no properties, signatures or index
    /// infos: `emptyObjectType`, `unknownEmptyObjectType` and the
    /// `anyFunctionType` wildcard (`checker.go:1029`). This port mints them
    /// as `Named` images without a member table.
    fn is_memberless_object_intrinsic(&self, id: TypeId) -> bool {
        id == self.checker.intrinsics.empty_object
            || id == self.checker.intrinsics.unknown_empty_object
            || self.checker.any_function_type == Some(id)
    }

    /// A tuple reference whose members `propertiesRelatedTo` can read
    /// (`relater.go:4100`): every element is fixed or spreads a concrete
    /// array. A tuple has no member table of its own here; its element
    /// properties, `length` and the inherited `Array`/`ReadonlyArray`
    /// members are read through `get_type_of_property_of_type`, as native
    /// reads them from the tuple target's resolved members. Generic tuples
    /// relate through their base constraint first (`relater.go:3849`) and
    /// are left out.
    fn is_structural_tuple_source(&mut self, id: TypeId) -> bool {
        let Some((elements, _)) = self.tuple_relation_elements(id) else { return false };
        elements.iter().all(|element| {
            !element.spread || self.checker.tuple_spread_array_element(element.r#type).is_some()
        })
    }

    /// `getUnmatchedProperty` (`checker.go`) over a tuple source: the
    /// tuple's properties are complete — its fixed elements, `length` and
    /// the global array members [`Checker::tuple_target_properties`] lists —
    /// so a string-named target property absent from them is unmatched.
    /// Symbol-named members are not in that list and stay undecided.
    fn tuple_source_lacks_property(&mut self, source: TypeId, name: &str) -> bool {
        if name.starts_with("__@")
            || name.starts_with('[')
            || !self.is_structural_tuple_source(source)
        {
            return false;
        }
        self.checker
            .tuple_target_properties(source)
            .is_some_and(|properties| !properties.iter().any(|(property, _)| property == name))
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
    fn tuple_array_related_to(&mut self, source: TypeId, target: TypeId) -> Option<RelationResult> {
        if !self.checker.tuple_element_lists.contains_key(&source)
            && !self.checker.variadic_tuple_elements.contains_key(&source)
        {
            return None;
        }
        let target_element = self.checker.tuple_spread_array_element(target)?;
        let constraint = self.checker.tuple_base_constraint(source);
        if constraint != source {
            return Some(self.is_related_to_with_flags(constraint, target, RecursionFlags::SOURCE));
        }
        let (target_symbol, _) = self.checker.type_reference_targets.get(&target)?;
        let target_symbol = self.checker.binder.merged_symbol(*target_symbol);
        let readonly_array = self
            .checker
            .global_type_symbol("ReadonlyArray")
            .map(|symbol| self.checker.binder.merged_symbol(symbol));
        if self.checker.tuple_is_readonly(source) && readonly_array != Some(target_symbol) {
            return Some(RelationResult::NotRelated);
        }
        let Some(source_element) = self.tuple_element_union(source) else {
            return Some(RelationResult::Unknown);
        };
        Some(self.is_related_to(source_element, target_element))
    }

    /// `structuredTypeRelatedToWorker`'s apparent source (relater.go:3815)
    /// for a generic homomorphic mapped type over an array- or
    /// tuple-constrained `T`: `getResolvedApparentTypeOfMappedType` applies
    /// the mapped type to that constraint (`apparent_mapped_type`), so
    /// `{ [P in keyof T]: X }` with `T extends [number] | [string]` is a
    /// union of mapped tuples. Against an array target the array case
    /// (relater.go:3841) relates the number-index types when the target is
    /// readonly and every constituent is an array or tuple, or when every
    /// constituent is a mutable tuple. Otherwise a union apparent type is
    /// not an object, so neither the structural nor the discriminated arm
    /// applies and the worker ends False; a single apparent type is related
    /// in the source's place. `None` when the source has no distinct
    /// apparent type.
    fn generic_mapped_apparent_source_related_to(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<RelationResult> {
        if !self.checker.is_generic_homomorphic_mapped_type(source) {
            return None;
        }
        let apparent = self.checker.apparent_mapped_type(source);
        if apparent == source {
            return None;
        }
        let parts = self.union_constituents(apparent);
        if let Some(target_element) = self.checker.tuple_spread_array_element(target)
            && !self.checker.type_of(target).flags.contains(TypeFlags::ANY)
            && let Some(&(target_symbol, _)) = self.checker.type_reference_targets.get(&target)
        {
            let target_symbol = self.checker.binder.merged_symbol(target_symbol);
            let readonly_target = self
                .checker
                .global_type_symbol("ReadonlyArray")
                .is_some_and(|symbol| self.checker.binder.merged_symbol(symbol) == target_symbol);
            let constituents = parts.clone().unwrap_or_else(|| vec![apparent]);
            let mut every_array_or_tuple = true;
            let mut every_mutable_tuple = true;
            for &part in &constituents {
                let tuple = self.checker.tuple_element_lists.contains_key(&part)
                    || self.checker.variadic_tuple_elements.contains_key(&part);
                let array = !tuple && self.checker.tuple_spread_array_element(part).is_some();
                every_array_or_tuple &= tuple || array;
                every_mutable_tuple &= tuple && !self.checker.tuple_is_readonly(part);
            }
            if (readonly_target && every_array_or_tuple) || every_mutable_tuple {
                let mut elements = Vec::with_capacity(constituents.len());
                for part in constituents {
                    let element = match self.tuple_element_union(part) {
                        Some(element) => Some(element),
                        None => self.checker.tuple_spread_array_element(part),
                    };
                    let Some(element) = element else {
                        return Some(RelationResult::Unknown);
                    };
                    elements.push(element);
                }
                let source_element = self.checker.get_union_type(&elements);
                return Some(self.is_related_to(source_element, target_element));
            }
        }
        if parts.is_some() {
            return Some(RelationResult::NotRelated);
        }
        Some(self.is_related_to_with_flags(apparent, target, RecursionFlags::SOURCE))
    }

    /// `getIndexTypeOfType(tuple, number)`: the union of a tuple's element
    /// types, with `undefined` for an optional element under strict null
    /// checks. `None` when a variadic tuple's union is not computed.
    fn tuple_element_union(&mut self, source: TypeId) -> Option<TypeId> {
        if let Some((mut elements, _)) = self.checker.tuple_element_lists.get(&source).cloned() {
            if self.checker.strict_null_checks
                && self
                    .checker
                    .tuple_optional_masks
                    .get(&source)
                    .is_some_and(|mask| mask.iter().any(|&optional| optional))
            {
                elements.push(self.checker.intrinsics.undefined);
            }
            return Some(self.checker.get_union_type(&elements));
        }
        self.checker.variadic_tuple_index_union(source)
    }

    /// `indexSignaturesRelatedTo` (`relater.go:4578`) for a tuple source.
    /// A tuple's only index info is the numeric one its `Array` or
    /// `ReadonlyArray` base declares, valued at the element union. A target
    /// info no numeric key applies to has no source counterpart, and a tuple
    /// reference is not an object type with an inferable index
    /// (`typeRelatedToIndexInfo`, `:4603`), so it fails.
    fn tuple_index_signatures_related(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<RelationResult> {
        let target_infos = self.checker.get_index_infos_of_type(target)?;
        let target_has_string =
            target_infos.iter().any(|info| info.key == self.checker.intrinsics.string);
        let mut parts = Vec::with_capacity(target_infos.len());
        for info in &target_infos {
            if self.relation != Relation::StrictSubtype
                && target_has_string
                && self.checker.type_of(info.value).flags.contains(TypeFlags::ANY)
            {
                continue;
            }
            if !self.checker.is_applicable_index_type(info.key, self.checker.intrinsics.number) {
                return Some(RelationResult::NotRelated);
            }
            let element = self.tuple_element_union(source)?;
            parts.push(self.is_related_to(element, info.value));
        }
        Some(RelationResult::all(parts))
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
    /// Missing required properties reject when source members can be enumerated;
    /// unfollowable members and protected-target checks remain Unknown. Privacy,
    /// optionality and strict-subtype readonly checks precede comparison of the
    /// resolved property types. Generic parameters use the ordinary relation.
    fn properties_related_to(&mut self, source: TypeId, target: TypeId) -> RelationResult {
        self.properties_related_to_with_optionals(source, target, false)
    }

    fn properties_related_to_with_optionals(
        &mut self,
        source: TypeId,
        target: TypeId,
        optionals_only: bool,
    ) -> RelationResult {
        self.properties_related_to_excluding(source, target, optionals_only, &[])
    }

    /// propertiesRelatedTo (relater.go:4100) with its `excludedProperties`:
    /// the named target members are skipped. Only
    /// [`Self::type_related_to_discriminated_type`] excludes anything.
    fn properties_related_to_excluding(
        &mut self,
        source: TypeId,
        target: TypeId,
        optionals_only: bool,
        excluded: &[String],
    ) -> RelationResult {
        let Some(names) = self.checker.get_property_names_of_type_shared(target) else {
            // Row 1 of `checker-notes-assign.md` §2: the target's inherited
            // requirements could not be *enumerated*, so no verdict about them
            // is available in either direction.
            reasons::note(reasons::Site::UnfollowableBase);
            return RelationResult::Unknown;
        };
        // Keep the original receiver for member reads: intersection property
        // types are already synthesized by property_type_via_shape. Only names
        // and declaration metadata need the distinct contributing object types.
        let source_parts = self.intersection_constituents(source).map(|types| {
            let mut pending = types;
            let mut visited = Vec::new();
            let mut parts = Vec::new();
            while let Some(part) = pending.pop() {
                let part = self.checker.apparent_type(part);
                if visited.contains(&part) {
                    continue;
                }
                visited.push(part);
                if let Some(types) = self.intersection_constituents(part) {
                    pending.extend(types);
                } else {
                    parts.push(part);
                }
            }
            parts
        });
        let intersection_names = source_parts.as_ref().map(|parts| {
            parts
                .iter()
                .map(|&part| self.checker.get_property_names_of_type_shared(part))
                .collect::<Option<Vec<_>>>()
                .map(|names| names.iter().flat_map(|names| names.iter().cloned()).collect())
        });
        // propertiesRelatedTo (relater.go:4240): an object-literal target
        // requires actual named properties, even when it has an index signature.
        // Regularization retains ObjectLiteral; widening removes it.
        if !optionals_only && self.checker.is_object_literal_type(target) {
            let Some(source_names) =
                self.source_property_names(source, intersection_names.as_ref())
            else {
                return RelationResult::Unknown;
            };
            if source_names.iter().any(|name| !names.contains(name)) {
                return RelationResult::NotRelated;
            }
        }
        let mut parts = Vec::with_capacity(names.len());
        for name in names.iter() {
            // propertiesRelatedTo returns on the first failing property
            // (relater.go:4240). `RelationResult::all` would answer the same
            // NotRelated; stopping skips relating the remaining members,
            // which for a matched discriminated constituent can expand
            // recursive generic signatures without bound (`Either<L, (a: A)
            // => B>` in `varianceProblingAndZeroOrderIndexSignatureRelationsAlign`
            // ran out of memory). Only the discriminated walk (the one caller
            // with exclusions) stops early: on the other roads the skipped
            // pairs' published results feed later relations
            // (`arrayDestructuringInSwitch1`'s subtype reduction of `every`
            // changed), a separate inconsistency recorded in
            // `docs/parity/notes/r5-relater4.md` §1.
            if !excluded.is_empty() && parts.last() == Some(&RelationResult::NotRelated) {
                return RelationResult::NotRelated;
            }
            if excluded.contains(name) {
                continue;
            }
            let target_metadata = self.property_flags(target, name);
            if optionals_only && !target_metadata.is_some_and(|flags| flags.0) {
                continue;
            }
            // Through [`Checker::get_type_of_property_of_type`], not
            // `get_property_of_type` + `get_type_of_symbol`. The symbol is the
            // *uninstantiated* declaration, so on a `C<number>` with a member
            // declared `a: T` this comparison would run against `T`. **This is
            // the site where that matters most**, because it is the one
            // consumer that acts on a `false`: a wrong member type here does
            // not degrade to a gap, it promotes the next overload candidate and
            // yields a confident wrong type (`docs/conventions.md`, "A
            // conservative `false` is safe for one kind of consumer and unsafe
            // for the other"). The member read applies the receiver mapper.
            //
            // `None` still means *no such property* — a property that exists
            // and does not type answers `Some(errorType)` — so the existence
            // test below is unchanged.
            let target_type = self.checker.get_type_of_property_of_type(target, name);
            // isPropertySymbolTypeRelated (relater.go:4334) relates an `any`
            // target property (outside the strict subtype relation also an
            // `unknown` one) before it reads the source property's type, so
            // an existing source member is not resolved for it. Existence is
            // the source's named members, as getPropertyOfObjectType answers.
            let type_related_unread = target_type.is_some_and(|target_type| {
                let target_type = if self.checker.exact_optional_property_types {
                    self.checker.remove_missing_type(target_type)
                } else {
                    target_type
                };
                let top = if self.relation == Relation::StrictSubtype {
                    TypeFlags::ANY
                } else {
                    TypeFlags::ANY_OR_UNKNOWN
                };
                self.checker.store.get(target_type).flags.intersects(top)
            }) && self
                .source_property_names(source, intersection_names.as_ref())
                .is_some_and(|names| names.contains(name));
            // An unread source member stands in as the target's type; the
            // type comparison below answers Related without relating it.
            let source_type = if type_related_unread {
                target_type
            } else {
                self.checker.get_type_of_property_of_type(source, name)
            };
            let (Some(target_type), Some(source_type)) = (target_type, source_type) else {
                // Row 2 of `checker-notes-assign.md` §2, half-answered by §15:
                // a target property with no source counterpart is fine when
                // the target property is OPTIONAL — under the assignable and
                // comparable relations always, under the subtype relations
                // only for an object-literal source
                // (`requireOptionalProperties`, relater.go:4232, which only
                // the two subtype relations set; interface-backed sources
                // must still match optionals or subtype reduction loses its
                // order; tsr-2zk.929). Captured mapped modifiers override declaration
                // optionality; this binder does not write SymbolFlags::OPTIONAL.
                // Everything else stays row 2's Unknown.
                if self.checker.get_type_of_property_of_type(source, name).is_none()
                    && target_metadata.is_some_and(|flags| flags.0)
                    && (!matches!(self.relation, Relation::Subtype | Relation::StrictSubtype)
                        || self.checker.is_object_literal_type(source)
                        || self.is_structural_tuple_source(source))
                {
                    parts.push(RelationResult::Related);
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
                if self.checker.get_type_of_property_of_type(source, name).is_none()
                    && target_metadata.is_some()
                    && (self.source_property_names(source, intersection_names.as_ref()).is_some()
                        || self.tuple_source_lacks_property(source, name))
                {
                    parts.push(RelationResult::NotRelated);
                    continue;
                }
                reasons::note(reasons::Site::AbsentProperty);
                parts.push(RelationResult::Unknown);
                continue;
            };
            // propertiesRelatedTo reads both sides through
            // getNonMissingTypeOfSymbol (relater.go:4334): under
            // exactOptionalPropertyTypes an optional property's missing type is
            // removed, so an explicit `undefined` must relate on its own.
            let (target_type, source_type) = if self.checker.exact_optional_property_types {
                (
                    self.checker.remove_missing_type(target_type),
                    self.checker.remove_missing_type(source_type),
                )
            } else {
                (target_type, source_type)
            };
            // The privacy arms (`propertyRelatedTo`'s first switch, §16 of
            // `checker-notes-assign.md`): PRIVATE on either side relates only
            // when both symbols share one value declaration — an identity
            // this port tests exactly; a protected SOURCE against a public
            // target rejects; a protected TARGET asks `isValidOverrideOf`
            // (an intersection source stays `Unknown`).
            let source_properties = if source_parts.is_some() {
                self.checker.intersection_property_symbols(source, name)
            } else {
                self.checker.get_property_of_type(source, name).into_iter().collect()
            };
            if let Some(target_property) = self.checker.get_property_of_type(target, name) {
                let privacy = self.property_privacy_related(
                    &source_properties,
                    target_property,
                    source_parts.is_some(),
                );
                if privacy != RelationResult::Related {
                    parts.push(privacy);
                    continue;
                }
            }
            // createUnionOrIntersectionProperty uses AND for optional and
            // readonly flags: one required/mutable contribution wins. Captured
            // anonymous members retain mapped optionality independent of origin.
            let source_metadata = if let Some(source_parts) = &source_parts {
                let mut metadata = Vec::new();
                for &part in source_parts {
                    if self.checker.get_type_of_property_of_type(part, name).is_none() {
                        continue;
                    }
                    metadata.push(self.property_flags(part, name));
                }
                metadata.into_iter().collect::<Option<Vec<_>>>().and_then(|flags| {
                    (!flags.is_empty())
                        .then(|| (flags.iter().all(|flag| flag.0), flags.iter().all(|flag| flag.1)))
                })
            } else {
                self.property_flags(source, name)
            };
            // A source-OPTIONAL property against a REQUIRED target member
            // rejects in every relation but comparability
            // (`propertyRelatedTo`, the 1.0-spec §3.8.3 clause: "if M is a
            // required property, N is also a required property") —
            // `{ p?: number }` is not related to `{ p: any }`, which is what
            // keeps `Contextual | Ellement` un-reduced
            // (`nonContextuallyTypedLogicalOr`, §15.1's two wrong lines).
            // Comparability passes `skipOptional` (relater.go:4259, tested at
            // :4318) for every source shape, so `x as { a: T[] }` from
            // `{ a?: T[] }` overlaps (tsr-2zk.929).
            if target_metadata.is_some_and(|flags| !flags.0)
                && self.relation != Relation::Comparable
            {
                if source_metadata.is_some_and(|flags| flags.0) {
                    parts.push(RelationResult::NotRelated);
                    continue;
                }
                if source_parts.is_some() && source_metadata.is_none() {
                    parts.push(RelationResult::Unknown);
                    continue;
                }
            }
            // `readonly` orders the STRICT subtype relation and only that one
            // (`relater.go:4300`–`:4308`): a readonly source property against
            // a mutable target rejects, so `{ a } | { readonly a }` reduces to
            // `{ readonly a }` and never by declaration order — `readonly`
            // deliberately does not affect assignability. §14 of
            // `checker-notes-assign.md`; `readonlyPropertySubtypeRelationDirected`
            // is the pin.
            if self.relation == Relation::StrictSubtype
                && source_metadata.is_some_and(|flags| flags.1)
                && target_metadata.is_some_and(|flags| !flags.1)
            {
                parts.push(RelationResult::NotRelated);
                continue;
            }
            // isPropertySymbolTypeRelated (relater.go:4334) relates the
            // resolved member types, including parameters and their constraints.
            // get_type_of_property_of_type has already applied receiver maps;
            // a surviving parameter can be the intended semantic member type.
            parts.push(if type_related_unread {
                RelationResult::Related
            } else {
                self.is_related_to(source_type, target_type)
            });
        }
        RelationResult::all(parts)
    }

    /// The privacy arms of propertyRelatedTo's first switch
    /// (relater.go:4271, §16 of `checker-notes-assign.md`): PRIVATE on either
    /// side relates only when both symbols share one value declaration, an
    /// identity this port tests exactly; a protected SOURCE against a public
    /// target rejects; a protected TARGET asks `isValidOverrideOf` (an
    /// intersection source stays `Unknown`).
    fn property_privacy_related(
        &mut self,
        source_properties: &[SymbolId],
        target_property: SymbolId,
        intersection_source: bool,
    ) -> RelationResult {
        let mut privacy = Vec::new();
        for &source_property in source_properties {
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
                    privacy.push(RelationResult::NotRelated);
                }
            } else if self.checker.property_has_modifier(target_property, protected) {
                privacy.push(if intersection_source {
                    // isPropertyInClassDerivedFrom walks the synthetic source
                    // property's constituents with ANY; this loop combines
                    // with ALL.
                    RelationResult::Unknown
                } else {
                    self.is_valid_override_of(source_property, target_property)
                });
            } else if self.checker.property_has_modifier(source_property, protected) {
                privacy.push(RelationResult::NotRelated);
            }
        }
        RelationResult::all(privacy)
    }

    /// typeRelatedToDiscriminatedType (relater.go:3989), reached from the
    /// end of structuredTypeRelatedToWorker (relater.go:3889) when an object
    /// or intersection source failed every constituent of a union target.
    /// `target` is already `extractTypesOfKind(target,
    /// Object|Intersection|Substitution)` and still a union.
    ///
    /// Each combination of the source's discriminant property types must
    /// match some target constituent (a single-type `propertyRelatedTo` per
    /// discriminant, with `skipOptional` under strictNullChecks or
    /// comparability), and every matched constituent must relate on its
    /// remaining properties, signatures and (outside tuple pairs) indexes.
    ///
    /// Stated divergences (`docs/parity/notes/r5-relater4.md` §1):
    /// - A source whose properties this port cannot enumerate answers
    ///   `Unknown`.
    /// - A matched constituent with a tuple on either side is related whole:
    ///   native's tuple arm of propertiesRelatedTo excludes discriminant
    ///   *positions*, which `tuples_related_to` does not take. The whole
    ///   relation re-checks the discriminants at their full types, so its
    ///   success is native's answer and its failure is `Unknown`.
    /// - `isDiscriminantProperty` is computed from the constituents' member
    ///   types (this port builds no synthetic union property with
    ///   `CheckFlags`), as in `flow.rs`'s and `assignreport.rs`'s copies.
    fn type_related_to_discriminated_type(
        &mut self,
        source: TypeId,
        target_types: &[TypeId],
    ) -> RelationResult {
        let Some(source_names) = self.checker.get_property_names_of_type_shared(source) else {
            return RelationResult::Unknown;
        };
        // findDiscriminantProperties (relater.go:1077), in source order.
        let mut discriminants: Vec<(String, Vec<TypeId>)> = Vec::new();
        for name in source_names.iter() {
            match self.checker.is_discriminant_property_of_types(target_types, name) {
                Some(true) => {}
                Some(false) => continue,
                None => return RelationResult::Unknown,
            }
            let Some(source_type) = self.checker.get_type_of_property_of_type(source, name) else {
                return RelationResult::Unknown;
            };
            if self.checker.is_gap(source_type) {
                return RelationResult::Unknown;
            }
            // getNonMissingTypeOfSymbol, then Distributed().
            let source_type = if self.checker.exact_optional_property_types {
                self.checker.remove_missing_type(source_type)
            } else {
                source_type
            };
            let types = match &self.checker.type_of(source_type).data {
                TypeData::Union { types, .. } => types.clone(),
                _ if self.checker.type_of(source_type).flags.contains(TypeFlags::NEVER) => {
                    Vec::new()
                }
                _ => vec![source_type],
            };
            discriminants.push((name.clone(), types));
        }
        if discriminants.is_empty() {
            return RelationResult::NotRelated;
        }
        // The fixed combination limit, checked before any allocation.
        let mut combinations = 1usize;
        for (_, types) in &discriminants {
            combinations *= types.len();
            if combinations > 25 || combinations == 0 {
                return RelationResult::NotRelated;
            }
        }
        let excluded: Vec<String> = discriminants.iter().map(|(name, _)| name.clone()).collect();
        let skip_optional =
            self.checker.strict_null_checks || self.relation == Relation::Comparable;
        let source_properties: Vec<Option<SymbolId>> = discriminants
            .iter()
            .map(|(name, _)| self.checker.get_property_of_type(source, name))
            .collect();
        let source_member_types: Vec<Option<TypeId>> = discriminants
            .iter()
            .map(|(name, _)| self.checker.get_type_of_property_of_type(source, name))
            .collect();
        let mut matching: Vec<TypeId> = Vec::new();
        let mut undecided = false;
        for index in 0..combinations {
            let mut combination = vec![self.checker.intrinsics.never; discriminants.len()];
            let mut n = index;
            for j in (0..discriminants.len()).rev() {
                let types = &discriminants[j].1;
                combination[j] = types[n % types.len()];
                n /= types.len();
            }
            let mut has_match = false;
            let mut combination_undecided = false;
            'targets: for &t in target_types {
                let mut constituent_undecided = false;
                for (i, (name, _)) in discriminants.iter().enumerate() {
                    // Existence is the member read, as in propertiesRelatedTo
                    // here: an instantiated member may have no symbol.
                    let Some(target_type) = self.checker.get_type_of_property_of_type(t, name)
                    else {
                        continue 'targets;
                    };
                    // `sourceProperty == targetProperty`: this port's symbols
                    // are uninstantiated declarations, so one symbol on two
                    // instantiations is the same property only when the
                    // member types agree too.
                    let target_property = self.checker.get_property_of_type(t, name);
                    if target_property.is_some()
                        && source_properties[i] == target_property
                        && source_member_types[i] == Some(target_type)
                    {
                        continue;
                    }
                    let related = self.discriminant_property_related(
                        source,
                        t,
                        source_properties[i],
                        target_property,
                        name,
                        combination[i],
                        skip_optional,
                    );
                    match related {
                        RelationResult::NotRelated => continue 'targets,
                        RelationResult::Unknown => constituent_undecided = true,
                        _ => {}
                    }
                }
                if constituent_undecided {
                    combination_undecided = true;
                    continue;
                }
                if !matching.contains(&t) {
                    matching.push(t);
                }
                has_match = true;
            }
            if !has_match {
                if combination_undecided {
                    undecided = true;
                    continue;
                }
                return RelationResult::NotRelated;
            }
        }
        // Compare the remaining non-discriminant members of each match.
        let mut parts = Vec::with_capacity(matching.len() + 1);
        let source_tuple = self.tuple_relation_elements(source).is_some();
        for t in matching {
            if source_tuple || self.tuple_relation_elements(t).is_some() {
                // The whole relation checks the discriminants too, with
                // their full types: its success is native's, its failure is
                // not a proof.
                let whole = self.is_related_to(source, t);
                if whole == RelationResult::NotRelated {
                    parts.push(RelationResult::Unknown);
                } else {
                    parts.push(whole);
                }
                continue;
            }
            let properties = self.properties_related_to_excluding(source, t, false, &excluded);
            if properties == RelationResult::NotRelated {
                return properties;
            }
            let signatures = if self.call_or_construct_bearing(t) {
                self.related_signatures(source, t).unwrap_or(RelationResult::Unknown)
            } else {
                RelationResult::Related
            };
            if signatures == RelationResult::NotRelated {
                return signatures;
            }
            let indexes =
                self.related_index_signatures(source, t).unwrap_or(RelationResult::Unknown);
            if indexes == RelationResult::NotRelated {
                return indexes;
            }
            parts.extend([properties, signatures, indexes]);
        }
        if undecided {
            parts.push(RelationResult::Unknown);
        }
        RelationResult::all(parts)
    }

    /// propertyRelatedTo (relater.go:4270) for one discriminant, with
    /// `getTypeOfSourceProperty` answering the combination's single type.
    #[allow(clippy::too_many_arguments)]
    fn discriminant_property_related(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_property: Option<SymbolId>,
        target_property: Option<SymbolId>,
        name: &str,
        source_type: TypeId,
        skip_optional: bool,
    ) -> RelationResult {
        if let Some(target_property) = target_property {
            let intersection_source = self.intersection_constituents(source).is_some();
            let source_properties = if intersection_source {
                self.checker.intersection_property_symbols(source, name)
            } else {
                source_property.into_iter().collect()
            };
            let privacy = self.property_privacy_related(
                &source_properties,
                target_property,
                intersection_source,
            );
            if privacy != RelationResult::Related {
                return privacy;
            }
        }
        let source_flags = self.property_flags(source, name);
        let target_flags = self.property_flags(target, name);
        if self.relation == Relation::StrictSubtype
            && source_flags.is_some_and(|flags| flags.1)
            && target_flags.is_some_and(|flags| !flags.1)
        {
            return RelationResult::NotRelated;
        }
        // isPropertySymbolTypeRelated (relater.go:4334).
        let Some(target_type) = self.checker.get_type_of_property_of_type(target, name) else {
            return RelationResult::Unknown;
        };
        let target_type = if self.checker.exact_optional_property_types {
            self.checker.remove_missing_type(target_type)
        } else {
            target_type
        };
        let top = if self.relation == Relation::StrictSubtype {
            TypeFlags::ANY
        } else {
            TypeFlags::ANY_OR_UNKNOWN
        };
        let related = if self.checker.type_of(target_type).flags.intersects(top) {
            RelationResult::Related
        } else {
            self.is_related_to(source_type, target_type)
        };
        if related == RelationResult::NotRelated {
            return related;
        }
        if !skip_optional && target_flags.is_some_and(|flags| !flags.0) {
            match source_flags {
                Some((true, _)) => return RelationResult::NotRelated,
                None => return RelationResult::all([related, RelationResult::Unknown]),
                _ => {}
            }
        }
        related
    }

    /// Mapped/spread symbols keep their declaration origins while overriding
    /// Optional/Readonly flags (propertyRelatedTo, internal/checker/relater.go).
    /// The source's property names for [`Self::properties_related_to_with_optionals`]:
    /// the flattened constituent names of an intersection source, else the
    /// source's own (shared, not copied: `r5-perf4.md` §3). Asked afresh at
    /// each use, as before.
    fn source_property_names(
        &mut self,
        source: TypeId,
        intersection_names: Option<&Option<std::rc::Rc<[String]>>>,
    ) -> Option<std::rc::Rc<[String]>> {
        match intersection_names {
            Some(names) => names.clone(),
            None => self.checker.get_property_names_of_type_shared(source),
        }
    }

    fn property_flags(&mut self, receiver: TypeId, name: &str) -> Option<(bool, bool)> {
        // createUnionOrIntersectionProperty (checker.go): an intersection
        // property is optional (readonly) only when every contributing
        // constituent's property is.
        if let Some(parts) = self.intersection_constituents(receiver) {
            let mut combined: Option<(bool, bool)> = None;
            for part in parts {
                if self.checker.get_type_of_property_of_type(part, name).is_none() {
                    continue;
                }
                let (optional, readonly) = self.property_flags(part, name)?;
                combined = Some(match combined {
                    None => (optional, readonly),
                    Some((o, r)) => (o && optional, r && readonly),
                });
            }
            return combined;
        }
        self.checker.resolve_mapped_type_members(receiver);
        if let Some((properties, _)) = self.checker.anonymous_properties.get(&receiver)
            && let Some(property) = properties.iter().find(|property| property.name == name)
        {
            return Some((property.optional, property.readonly));
        }
        let mut modifiers = self.checker.mapped_identity_optionality.get(&receiver).copied();
        let source = modifiers.and_then(|_| {
            self.checker
                .type_reference_targets
                .get(&receiver)
                .and_then(|(_, arguments)| (arguments.len() == 1).then_some(arguments[0]))
        });
        // Object/Function augmentation is not a mapped keyof member.
        if let Some(source) = source
            && self
                .checker
                .get_property_names_of_type(source)
                .is_some_and(|names| !names.iter().any(|key| key == name))
        {
            modifiers = None;
        }
        let flags = if let Some(source) = source {
            self.property_flags(source, name)
        } else {
            // A member inherited through a generic base has no symbol on the
            // ordinary road; its declaring symbol carries the same modifiers.
            self.checker
                .get_property_of_type(receiver, name)
                .or_else(|| {
                    self.checker.generic_heritage_property_symbol(receiver, name, &mut Vec::new())
                })
                .map(|property| {
                    (
                        self.checker.property_is_optional(property),
                        self.checker.is_readonly_property(property),
                    )
                })
        };
        flags.map(|(optional, readonly)| {
            let (mapped_optional, mapped_readonly) = modifiers.unwrap_or_default();
            (mapped_optional.unwrap_or(optional), mapped_readonly.unwrap_or(readonly))
        })
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
#[cfg(test)]
mod variance_recursion_tests {
    use super::{Checker, RecursionFlags, Relater, Relation, RelationResult, Ternary};
    use tsr_ast::Statement;
    use tsr_core::Arena;

    #[test]
    fn circular_variance_is_non_false_unpublished_and_recovers_after_measurement() {
        let source = "type Box<T> = { value: T };
            let text: Box<string>; let numeric: Box<number>; let literal: Box<'a'>;";
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "variance-recursion.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let mut types = Vec::new();
        for statement in parsed.source_file.statements.iter().skip(1) {
            let Statement::VariableStatement(statement) = statement else {
                panic!("variable annotation");
            };
            let annotation = statement
                .declaration_list
                .and_then(|list| list.declarations.first().copied())
                .and_then(|declaration| declaration.r#type)
                .expect("type annotation");
            types.push(checker.get_type_from_type_node(annotation));
        }
        let symbol = checker.type_reference_targets[&types[0]].0;
        checker.variance_in_progress.insert(symbol);
        {
            let mut relater = Relater::new(&mut checker, Relation::Assignable, None);
            let circular =
                relater.recursive_type_related_to(types[0], types[1], RecursionFlags::BOTH);
            assert_eq!(circular, RelationResult::CircularVariance);
            assert_eq!(circular.public_answer(), Ternary::Related);
            assert_eq!(
                relater.checker.relation_results.len(Relation::Assignable),
                0,
                "circular variance must not publish a proof"
            );
            assert!(relater.maybe_keys.is_empty());
            assert!(relater.maybe_keys_set.is_empty());
        }
        checker.variance_in_progress.remove(&symbol);
        assert_eq!(
            checker.relate_ternary(types[0], types[1], Relation::Assignable),
            Ternary::NotRelated
        );
        assert_eq!(
            checker.relate_ternary(types[2], types[0], Relation::Assignable),
            Ternary::Related
        );
        assert_eq!(
            checker.relate_ternary(types[0], types[2], Relation::Assignable),
            Ternary::NotRelated
        );
    }

    #[test]
    fn unsupported_work_is_not_circular_variance() {
        use RelationResult::{CircularVariance, Maybe, NotRelated, Related, Unknown};
        assert_eq!(RelationResult::all([Related, Maybe, CircularVariance]), CircularVariance);
        assert_eq!(RelationResult::all([CircularVariance, Unknown]), Unknown);
        assert_eq!(RelationResult::all([CircularVariance, NotRelated]), NotRelated);
        assert_eq!(RelationResult::any([NotRelated, CircularVariance]), CircularVariance);
        assert_eq!(RelationResult::any([Unknown, NotRelated]), Unknown);
        assert_eq!(Unknown.public_answer(), Ternary::Unknown);
    }
}

#[cfg(test)]
mod relation_cache_tests {
    use super::{Checker, Relation, Ternary};
    use crate::relation_cache::CachedRelation;
    use tsr_ast::Statement;
    use tsr_core::Arena;

    fn with_annotations(source: &str, test: impl FnOnce(&mut Checker<'_, '_>, &[crate::TypeId])) {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "relation-cache.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let mut types = Vec::new();
        for statement in parsed.source_file.statements {
            let Statement::VariableStatement(statement) = statement else { continue };
            let annotation = statement
                .declaration_list
                .and_then(|list| list.declarations.first().copied())
                .and_then(|declaration| declaration.r#type)
                .expect("type annotation");
            types.push(checker.get_type_from_type_node(annotation));
        }
        test(&mut checker, &types);
    }

    const RECURSIVE: &str = "interface A { next: B; value: string }
        interface B { next: A; value: string }
        interface C { next: D; value: string }
        interface D { next: C; value: string }
        interface E { next: E; value: number }
        let a: A; let c: C; let e: E;";

    /// Native `recursiveTypeRelatedTo` publishes a completed proof, with the
    /// assumptions it discharged, to the checker's `Relation.results`; a
    /// failure is published on its own. Both outlive the walk.
    #[test]
    fn completed_results_outlive_the_walk() {
        with_annotations(RECURSIVE, |checker, types| {
            let (a, c, e) = (types[0], types[1], types[2]);
            assert_eq!(checker.relate_ternary(a, c, Relation::Assignable), Ternary::Related);
            let results = &checker.relation_results;
            assert_eq!(
                results.get(Relation::Assignable, (a, c, false)),
                Some(CachedRelation::Succeeded)
            );
            // B -> D was assumed while A -> C was active and published with it.
            assert!(results.len(Relation::Assignable) >= 2);
            assert_eq!(results.get(Relation::Comparable, (a, c, false)), None);
            assert_eq!(checker.relate_ternary(a, e, Relation::Assignable), Ternary::NotRelated);
            assert_eq!(
                checker.relation_results.get(Relation::Assignable, (a, e, false)),
                Some(CachedRelation::Failed)
            );
            // A repeat answers from the store, before a walk opens.
            assert_eq!(
                checker.cached_object_relation(a, c, Relation::Assignable),
                Some(Ternary::Related)
            );
            assert_eq!(
                checker.cached_object_relation(a, e, Relation::Assignable),
                Some(Ternary::NotRelated)
            );
            assert_eq!(checker.cached_object_relation(c, a, Relation::Assignable), None);
            assert_eq!(checker.relate_ternary(a, c, Relation::Assignable), Ternary::Related);
            assert_eq!(checker.relate_ternary(a, e, Relation::Assignable), Ternary::NotRelated);
        });
    }

    /// A walk opened inside a conditional-alias evaluation frame reads
    /// members through that frame: it neither reads nor publishes the store.
    #[test]
    fn framed_walks_stay_walk_local() {
        with_annotations(RECURSIVE, |checker, types| {
            let (a, c) = (types[0], types[1]);
            checker.alias_evaluation_bindings.push(rustc_hash::FxHashMap::default());
            assert_eq!(checker.relate_ternary(a, c, Relation::Assignable), Ternary::Related);
            checker.alias_evaluation_bindings.pop();
            assert_eq!(checker.relation_results.len(Relation::Assignable), 0);
            checker.mapped_template_depth += 1;
            assert_eq!(checker.relate_ternary(a, c, Relation::Assignable), Ternary::Related);
            checker.mapped_template_depth -= 1;
            assert_eq!(checker.relation_results.len(Relation::Assignable), 0);
            // Nor does a framed entry read a frame-free result.
            assert_eq!(checker.relate_ternary(a, c, Relation::Assignable), Ternary::Related);
            checker.mapped_template_depth += 1;
            assert_eq!(checker.cached_object_relation(a, c, Relation::Assignable), None);
            checker.mapped_template_depth -= 1;
        });
    }

    /// Results computed under one set of relation options are discarded when
    /// a walk starts under another.
    #[test]
    fn option_changes_discard_results() {
        with_annotations(RECURSIVE, |checker, types| {
            let (a, c) = (types[0], types[1]);
            assert_eq!(checker.relate_ternary(a, c, Relation::Assignable), Ternary::Related);
            assert!(checker.relation_results.len(Relation::Assignable) > 0);
            checker.strict_function_types = !checker.strict_function_types;
            assert_eq!(checker.cached_object_relation(a, c, Relation::Assignable), None);
            assert_eq!(checker.relate_ternary(c, c, Relation::Assignable), Ternary::Related);
            assert_eq!(checker.relation_results.len(Relation::Assignable), 0);
        });
    }
}

#[cfg(test)]
mod discriminated_target_tests {
    use super::{Relation, Ternary};
    use crate::checker::Checker;
    use tsr_ast::Statement;
    use tsr_core::Arena;

    fn with_annotations(source: &str, test: impl FnOnce(&mut Checker<'_, '_>, &[crate::TypeId])) {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "discriminated.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let mut types = Vec::new();
        for statement in parsed.source_file.statements {
            let Statement::VariableStatement(statement) = statement else { continue };
            let annotation = statement
                .declaration_list
                .and_then(|list| list.declarations.first().copied())
                .and_then(|declaration| declaration.r#type)
                .expect("type annotation");
            types.push(checker.get_type_from_type_node(annotation));
        }
        test(&mut checker, &types);
    }

    /// typeRelatedToDiscriminatedType (relater.go:3989): every combination
    /// of the source's discriminant types must match a constituent, and the
    /// matches must accept the remaining members.
    #[test]
    fn a_union_discriminant_covers_a_discriminated_target() {
        let source = "type Action = { type: 'a'; p: number } | { type: 'b'; p: number };
            let target: Action;
            let covers: { type: 'a' | 'b'; p: number };
            let uncovered: { type: 'a' | 'c'; p: number };
            let rest: { type: 'a' | 'b'; p: string };";
        with_annotations(source, |checker, types| {
            let (target, covers, uncovered, rest) = (types[0], types[1], types[2], types[3]);
            assert_eq!(
                checker.relate_ternary(covers, target, Relation::Assignable),
                Ternary::Related
            );
            assert_eq!(
                checker.relate_ternary(uncovered, target, Relation::Assignable),
                Ternary::NotRelated
            );
            assert_eq!(
                checker.relate_ternary(rest, target, Relation::Assignable),
                Ternary::NotRelated
            );
        });
    }
}

#[cfg(test)]
mod generic_key_tests {
    use super::{Relation, Ternary};
    use crate::checker::Checker;
    use tsr_ast::Statement;
    use tsr_core::Arena;

    /// The declared types of the parameters of the source's one function.
    fn with_parameters(source: &str, test: impl FnOnce(&mut Checker<'_, '_>, &[crate::TypeId])) {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "keys.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let function = parsed
            .source_file
            .statements
            .iter()
            .find_map(|statement| match statement {
                Statement::FunctionDeclaration(function) => Some(*function),
                _ => None,
            })
            .expect("function");
        let types: Vec<_> = function
            .parameters
            .iter()
            .map(|parameter| {
                checker.get_type_from_type_node(parameter.r#type.expect("parameter type"))
            })
            .collect();
        test(&mut checker, &types);
    }

    /// `docs/parity/notes/r5-relater6.md` §2: a type parameter and a
    /// template meet a literal union whole through their constraints
    /// (relater.go:3380, :3652, :3772), and an alias image whose body is a
    /// literal union relates as that body.
    #[test]
    fn generic_keys_meet_a_literal_union_through_their_constraints() {
        let source = r#"type Keyof<T> = keyof T & string;
            function f<V extends "a" | "b", T extends "a" | "b", S extends Keyof<{ a: 1; b: 2 }>, U extends "a" | "c">(
                target: "a" | "b" | "c", v: V, t: `${T}`, s: S, u: U, narrow: "a" | "b") {}"#;
        with_parameters(source, |checker, types| {
            let [target, v, t, s, u, narrow] = types[..] else { panic!("six parameters") };
            for source in [v, t, s] {
                assert_eq!(
                    checker.relate_ternary(source, target, Relation::Assignable),
                    Ternary::Related
                );
            }
            assert_eq!(
                checker.relate_ternary(u, narrow, Relation::Assignable),
                Ternary::NotRelated
            );
        });
    }

    /// `docs/parity/notes/r5-relater6.md` §4: `S -> T[K]` relates through
    /// the write constraint `T[constraint of K]` when neither base is
    /// generic (relater.go:3459), and fails when that constraint does.
    #[test]
    fn an_indexed_access_target_relates_through_its_write_constraint() {
        let source = r"type R = { a: string; b: string; [key: string]: string };
            function f<K extends keyof R>(target: R[K], s: string, n: number) {}";
        with_parameters(source, |checker, types| {
            let [target, s, n] = types[..] else { panic!("three parameters") };
            assert_eq!(checker.relate_ternary(s, target, Relation::Assignable), Ternary::Related);
            assert_eq!(
                checker.relate_ternary(n, target, Relation::Assignable),
                Ternary::NotRelated
            );
        });
    }

    /// `docs/parity/notes/r5-relater6.md` §5: a generic homomorphic mapped
    /// source over a tuple-constrained `T` relates through its apparent
    /// type, the mapped tuples (relater.go:3815, :3841).
    #[test]
    fn a_homomorphic_mapped_source_over_tuples_meets_arrays_as_its_apparent_type() {
        let source = r"type H<T> = { [P in keyof T]: T[P] extends string ? boolean : null };
            interface Array<T> { [n: number]: T; length: number }
            interface ReadonlyArray<T> { readonly [n: number]: T; readonly length: number }
            function f<T extends [number] | [string], U extends [number] | readonly [string]>(
                t: H<T>, u: H<U>, mutable: any[], readonly: readonly any[]) {}";
        with_parameters(source, |checker, types| {
            let [t, u, mutable, readonly] = types[..] else { panic!("four parameters") };
            assert_eq!(checker.relate_ternary(t, mutable, Relation::Assignable), Ternary::Related);
            assert_eq!(checker.relate_ternary(u, readonly, Relation::Assignable), Ternary::Related);
            assert_eq!(
                checker.relate_ternary(u, mutable, Relation::Assignable),
                Ternary::NotRelated
            );
        });
    }
}
