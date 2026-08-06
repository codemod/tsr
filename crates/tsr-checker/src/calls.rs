//! Calls: `f(x)`, `a.b()`, and what a call expression's type is.
//!
//! Ported from `Checker.checkCallExpression` into `resolveCallExpression` and
//! `getResolvedSignature` (`checker.go:8951`, `:9995`), reduced to the single
//! question this port can answer exactly: *given the callee's type, which
//! signature is called, and what does it return?*
//!
//! # Overload resolution, over the arguments where a `false` can be trusted
//!
//! `resolveCall` (`checker.go:8843`) chooses among candidate signatures by
//! **assignability** — it builds an argument list, runs inference for generic
//! candidates, and picks the first candidate every argument is assignable to,
//! falling back to the one with the fewest failures for error reporting.
//!
//! [`Checker::choose_overload`] ports the selection. What makes it a port rather
//! than a guess is that it is the first thing in this checker to depend on a
//! **negative** answer from the relater — skipping a candidate is what lets the
//! next one win — and the relater's `false` is only sound over some of the type
//! space. So selection runs over [`SELECTABLE`] argument and parameter types and
//! answers `errorType` everywhere else. Taking the first candidate instead would
//! produce a plausible type for every overloaded call in the corpus and be wrong
//! for most of them, which is the exact failure the `errorType`-not-`anyType`
//! discipline exists to prevent.
//!
//! The corpus shape this targets: of the calls to a locally declared overloaded
//! `function` in the `.types` baselines, 523 are to a non-generic set whose
//! candidates differ by **parameter type**, against 59 differing only by arity
//! and 84 with a generic candidate. Assignability, not arity, is what those
//! calls need, and they are spread over 189 files — the largest is 40 sites, and
//! the top ten hold 26% — so this is corpus-wide rather than one file's shape.
//!
//! # Arguments are not *reported on*, and that is visible
//!
//! Upstream checks each argument against the parameter it lands on and reports.
//! Arguments are read here only to select among overloads; nothing is reported,
//! because every diagnostic is `bd tsr-5e7.6` and arity checking without a full
//! assignability relation would report on shapes it cannot judge. The **return
//! type is unaffected** by this for a non-generic signature,
//! which is why it is a sound reduction rather than a shortcut. A generic
//! signature's return type *does* depend on the arguments, and that is
//! [`crate::inference`]'s question — it answers the shapes whose type arguments
//! can be read straight off an argument position and `errorType` for the rest,
//! which is still most of them (`docs/architecture/checker-notes-infer.md`).

use tsr_ast::{CallExpression, Expression, TaggedTemplateExpression};

use tsr_binder::SymbolFlags;

use crate::calls::counters::{COUNTERS, bump};
use crate::{
    checker::Checker,
    flags::TypeFlags,
    signatures::Signature,
    types::{TypeData, TypeId},
};

/// The type domains over which this port's [`Checker::is_type_assignable_to`]
/// answers `false` only when the relation genuinely does not hold.
///
/// Overload selection is the first caller that depends on a **negative** answer
/// from the relater: skipping a candidate is what makes the next one win. Every
/// other caller so far only depends on a positive one. The relater's module docs
/// record that two distinct object types answer `false` because structural
/// comparison is narrow, not because they are unrelated — a false negative there
/// would silently promote the *next* overload, which is precisely the plausible
/// wrong answer this port refuses to produce.
///
/// So selection runs only where a `false` is trustworthy: the domains
/// `isSimpleTypeRelatedTo` decides on flags alone. This is deliberately
/// *narrower* than upstream's `TypeFlagsPrimitive` — `ENUM_LIKE`,
/// `ES_SYMBOL_LIKE`, `TEMPLATE_LITERAL` and `STRING_MAPPING` are left out,
/// because an enum literal's relation runs through its declared type, a unique
/// symbol's through its declaration, and the other two are unported. A candidate
/// or argument outside this set makes the whole call a gap rather than a guess.
const SELECTABLE: TypeFlags = TypeFlags::STRING
    .union(TypeFlags::STRING_LITERAL)
    .union(TypeFlags::NUMBER)
    .union(TypeFlags::NUMBER_LITERAL)
    .union(TypeFlags::BIG_INT)
    .union(TypeFlags::BIG_INT_LITERAL)
    .union(TypeFlags::BOOLEAN)
    .union(TypeFlags::BOOLEAN_LITERAL)
    .union(TypeFlags::VOID)
    .union(TypeFlags::UNDEFINED)
    .union(TypeFlags::NULL)
    .union(TypeFlags::NEVER);

/// Where a call stops, counted rather than reasoned about.
///
/// # Why this exists
///
/// Overload selection was predicted to move "low hundreds" of assertion lines
/// and moved 48, across 8 cases. A source-side classification of the 779
/// overloaded call sites said [`SELECTABLE`] admits ~198 of them on the
/// **parameter** side, so at least three quarters of the attenuation was
/// happening at a step nobody could name. Widening `SELECTABLE` (`bd tsr-6v7`)
/// is only worth doing if the parameter gate is what actually binds, and no
/// count of the *source* can answer that: the guard also runs over the
/// **argument** types, the winning candidate's return type may be unported, and
/// a callee that never resolves never reaches selection at all. Those three are
/// invisible to a regex over signatures and visible to a counter.
///
/// # Reading the funnel
///
/// The counters partition, in the order the code tests them: every
/// [`Checker::check_call_expression`] entry lands in exactly one of the
/// pre-selection buckets, and every [`Checker::choose_overload`] entry lands in
/// exactly one of the selection buckets. `selected_return_error` is the one
/// exception — a **subset** of `selected`, not a sibling — because a call whose
/// selection succeeded and whose return annotation is unported still prints
/// `error`, which is indistinguishable in the gradient from a selection that
/// failed. The example that prints the histogram derives the unattributed
/// remainder by subtraction; it is zero by construction, and a non-zero reading
/// means these buckets no longer partition the code.
///
/// # No behaviour change, and one consequence of that
///
/// Counting is off unless `TSR_OVERLOAD_COUNTERS` is set, and even when on it
/// only adds relaxed atomic increments — no call classifies differently. That
/// discipline costs one thing worth stating: arguments are checked *after* the
/// parameter gate, so `argument_not_selectable` counts only among sites that
/// already passed it. Checking arguments earlier to attribute both would run
/// `check_expression` on expressions the checker does not otherwise visit,
/// which perturbs its caches and the assertion lines they feed. The parameter
/// gate therefore **shadows** the argument gate, and the histogram reads as a
/// funnel rather than as independent causes.
pub mod counters {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicU64, Ordering};

    macro_rules! define_counters {
        ($($(#[$meta:meta])* $field:ident = $label:literal,)*) => {
            /// The counters themselves. Read through [`snapshot`].
            pub struct Counters {
                $($(#[$meta])* pub $field: AtomicU64,)*
            }

            /// A read of [`COUNTERS`] at one instant.
            #[derive(Debug, Default, Clone, Copy)]
            pub struct Snapshot {
                $($(#[$meta])* pub $field: u64,)*
            }

            impl Snapshot {
                /// Every counter as `(label, value)`, in declaration order —
                /// which is the order the code tests them, so a reader can
                /// follow the funnel down the printed rows.
                pub fn rows(&self) -> Vec<(&'static str, u64)> {
                    vec![$(($label, self.$field),)*]
                }
            }

            /// Process-wide, because the corpus runner checks files in
            /// parallel and a per-`Checker` field would have to be threaded
            /// out through every suite.
            pub static COUNTERS: Counters = Counters {
                $($field: AtomicU64::new(0),)*
            };

            /// The counters as they stand now.
            pub fn snapshot() -> Snapshot {
                Snapshot { $($field: COUNTERS.$field.load(Ordering::Relaxed),)* }
            }
        };
    }

    define_counters! {
        /// Every `check_call_expression` entry — the denominator.
        call_expressions = "call expressions checked",
        /// `f?.()`, unported.
        optional_chain = "  optional chain (unported)",
        /// The callee's type is not an anonymous object type, so it carries no
        /// signatures here: an interface with a call signature member, a
        /// function *type node* in annotation position, an unresolved import,
        /// or a gap that already answered `error`.
        callee_not_anonymous = "  callee type is not an object type",
        /// A bare identifier callee that `resolve_name` does not find at all.
        /// With the bundled libs in the program this is a genuinely undeclared
        /// name; without them it is mostly `parseInt`/`Math`-shaped globals.
        callee_name_unresolved = "    identifier: no symbol",
        /// A bare identifier callee whose symbol resolves and whose type is
        /// `error` — the symbol exists and this port cannot type it.
        callee_name_type_error = "    identifier: symbol types as error",
        /// A bare identifier callee whose symbol resolves to a real, non-object
        /// type. Upstream would report "not callable"; so would we.
        callee_name_not_object = "    identifier: symbol types as a non-object",
        /// `a.b()` where `a`'s own type is already `error` — the receiver is the
        /// gap and the call is downstream of it.
        callee_property_receiver_error = "    property access: receiver is error",
        /// `a.b()` where `a` has a real type and the member lookup did not
        /// produce an object type.
        callee_property_receiver_typed = "    property access: receiver is typed",
        /// **Subset of the row above.** `get_property_of_type` found no member
        /// of that name on the receiver. Mixed: upstream reports "property does
        /// not exist" for most of these, but an incomplete member lookup here
        /// lands in the same row.
        callee_member_absent = "      of which: no such member",
        /// **Subset of `no such member`.** The receiver's type HAS a member
        /// table and the name is not in it — the lookup ran and answered no,
        /// which is upstream's answer too.
        member_absent_looked_up = "        receiver has members, name absent",
        /// **Subset.** An intrinsic receiver (`string`, `number`, `boolean`).
        /// `get_property_of_type` returns `None` before reading the name, and
        /// upstream *does* have members here — `charAt`, `toFixed`. Ours.
        member_absent_intrinsic = "        receiver is an intrinsic",
        /// **Subset.** A union or intersection receiver: no member table, and
        /// upstream distributes the lookup. Ours.
        member_absent_composite = "        receiver is a union or intersection",
        /// **Subset.** A `Named` type whose members this port never built. Ours.
        member_absent_named_no_members = "        receiver is Named without members",
        /// **Subset.** A literal type receiver, or any other shape. Ours.
        member_absent_other = "        receiver is another shape",
        /// **Subset.** The member exists and types as `error` — this port found
        /// the symbol and could not type it. The actionable half.
        callee_member_type_error = "      of which: member types as error",
        /// **Subset.** The member exists and types as a real non-object type.
        /// Upstream reports "not callable" too, so this is **not a gap**.
        callee_member_not_object = "      of which: member types as a non-object",
        /// **A second, orthogonal split of `property access: receiver is
        /// typed`** — `bd tsr-fua`. The three rows below partition the same
        /// 2,562 the three `of which` rows above do, by whether the receiver's
        /// type **carries type arguments**: whether it came out of
        /// `create_type_reference` (`crate::declared`) as `C<number>` rather
        /// than as `C`. That is the discriminator for `bd tsr-4qx`,
        /// instantiated members, and nothing measured so far separates it.
        ///
        /// Receiver carries type arguments and the member lookup found
        /// nothing. This is the population instantiated members would act on.
        receiver_generic_member_absent = "      by receiver: type arguments, no member found",
        /// Receiver carries type arguments and `get_property_of_type` found a
        /// member — **the control bucket, and it must read zero today.**
        ///
        /// `create_type_reference` builds every such type with `members: None`
        /// (`crate::declared`), and `get_property_of_type` (`crate::members`)
        /// returns `None` for a `Named` with no member table before it reads
        /// the name. So a receiver that carries type arguments can never reach
        /// `member types as error` or `member types as a non-object`. That
        /// invariant is the load-bearing fact of `bd tsr-fua`; a counter
        /// re-derives it on every corpus run instead of trusting this comment.
        ///
        /// It stops being a control the moment step 4 of `bd tsr-4qx` flips
        /// `create_type_reference` to `Some(symbol)`, at which point this row
        /// becomes the measurement of how many generic receivers newly resolve
        /// a member.
        receiver_generic_member_found = "      by receiver: type arguments, member found (CONTROL: 0)",
        /// Receiver carries no type arguments. Instantiated members cannot
        /// reach these, whatever else is wrong with them.
        receiver_not_generic = "      by receiver: no type arguments",
        /// `a[b]()`.
        callee_element_access = "    element access",
        /// Any other callee form: a call, a parenthesis, `this`, `new`, a
        /// non-null assertion.
        callee_other_form = "    another expression form",
        /// `get_signatures_of_symbol` answered `None` — the symbol's
        /// declarations are a shape it does not build signatures for.
        callee_no_signatures = "  callee symbol has no signature list",
        /// A class, enum or namespace: an object type with an empty list.
        callee_zero_signatures = "  callee has zero call signatures",
        /// Exactly one candidate, so there was nothing to select.
        single_candidate = "  single candidate (no selection needed)",
        /// Entries to `choose_overload` — the denominator for the rows below.
        overload_sets = "overload sets reaching choose_overload",
        /// A generic candidate anywhere in the set. `bd tsr-4sc.8`.
        generic_candidate = "  a generic candidate in the set",
        /// A `this` or rest parameter on any candidate.
        this_or_rest_parameter = "  a this or rest parameter",
        /// The gate `bd tsr-6v7` proposes widening.
        parameter_not_selectable = "  a parameter type outside SELECTABLE",
        /// `f(...xs)`.
        spread_argument = "  a spread argument",
        /// The same gate on the argument side, shadowed by the parameter one.
        argument_not_selectable = "  an argument type outside SELECTABLE",
        /// No candidate accepts this many arguments.
        arity_no_match = "  no candidate with matching arity",
        /// Arity matched somewhere and no candidate's parameters accepted the
        /// arguments. Inside `SELECTABLE` this is a real negative from the
        /// relater, so it is the row that would *not* move on a widening.
        no_assignable_candidate = "  arity matched, nothing assignable",
        /// Several matches with different return types; upstream's subtype
        /// pass would decide, and this port will not guess.
        ambiguous_return = "  ambiguous: matches with different returns",
        /// A candidate was chosen.
        selected = "  SELECTED",
        /// **Subset of `selected`.** The winner's return type is `error`, so
        /// the call still prints a gap: selection worked and bought nothing.
        selected_return_error = "    of which the return type is error",
    }

    static ENABLED: OnceLock<bool> = OnceLock::new();

    /// Turn counting on for the rest of the process. Idempotent, and a no-op
    /// once anything has read the switch — call it before checking anything.
    pub fn enable() {
        let _ = ENABLED.set(true);
    }

    /// Whether counting is on. Public so a classifier that costs more than one
    /// increment — [`crate::Checker::classify_unresolved_callee`] resolves a
    /// name — can be skipped entirely when it is off.
    #[must_use]
    pub fn counting() -> bool {
        enabled()
    }

    /// Whether to count. Off by default so the checker's hot path is
    /// unchanged for every consumer that is not this probe.
    fn enabled() -> bool {
        *ENABLED.get_or_init(|| std::env::var_os("TSR_OVERLOAD_COUNTERS").is_some())
    }

    /// Add one, if counting is on.
    pub fn bump(counter: &AtomicU64) {
        if enabled() {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl Checker<'_, '_> {
    /// The type of a call expression.
    ///
    /// Ported from `Checker.checkCallExpression` (`checker.go:8951`) into
    /// `getReturnTypeOfSignature` on the resolved signature.
    ///
    /// Not ported, each answering `errorType`: an optional chain (`f?.()`), a
    /// `super(…)` call, an `import(…)` call, a call with explicit type arguments,
    /// a call to a callee with no call signature, and an overloaded call outside
    /// what [`Checker::choose_overload`] can decide.
    pub fn check_call_expression(&mut self, node: &CallExpression<'_>) -> TypeId {
        let error = self.intrinsics.error;
        bump(&COUNTERS.call_expressions);
        if node.question_dot_token.is_some() {
            bump(&COUNTERS.optional_chain);
            return error;
        }
        let Some(callee) = node.expression else { return error };
        let callee_type = self.check_expression(callee);
        // Split the largest bucket in the funnel by *why* the callee has no
        // object type. Done here rather than in `resolve_call_signature`
        // because only this path has the callee **node**, and the question is
        // about the expression that produced the type, not the type.
        if counters::counting()
            && !matches!(self.store.get(callee_type).data, TypeData::Anonymous { .. })
        {
            self.classify_unresolved_callee(callee, callee_type);
        }
        let Some(signature) = self.resolve_call_signature(callee_type, Some(node.arguments)) else {
            return error;
        };
        // Upstream would now report on the arguments; see the module docs for
        // why this does not, and why the return type is the same either way.
        if !signature.type_parameters.is_empty() {
            // A generic signature's return type depends on the arguments, so it
            // needs inference (`inferTypeArguments`, `checker.go:9390`). Answering
            // the uninstantiated return type would print `T` where upstream prints
            // what `T` was inferred as, so [`crate::inference`] answers the shapes
            // it can read a candidate off directly and `errorType` for the rest.
            return self.check_generic_call(&signature, node.node_id, node.arguments);
        }
        // `checkNoTypeArguments` (`checker.go:23157`): type arguments on a
        // signature that takes none is an error, and answering the return type
        // would quietly drop them.
        if !node.type_arguments.is_empty() {
            return error;
        }
        signature.r#type
    }

    /// Attribute one `callee type is not an object type` to the shape of the
    /// callee expression. Counting only; no answer depends on it.
    ///
    /// The buckets are a partition of the callee's *syntactic* shape crossed
    /// with what this port produced for it, which is level 3 of
    /// `docs/conventions.md`'s bucketing rule — our failure, not the syntax of
    /// the question and not upstream's answer. The example that prints them
    /// derives the unclassified remainder by subtraction; it is zero by
    /// construction, and non-zero only if a callee form stopped matching here.
    ///
    /// Re-resolving the identifier is a second `resolve_name` for the same
    /// node, which is why the whole call is behind [`counters::counting`]. It
    /// answers the same thing `check_expression` already answered — the point
    /// is that `check_expression` throws away *which* of "no symbol" and
    /// "symbol with no type" produced its `error`, and that distinction is the
    /// whole question.
    /// Whether `id` is an **instantiated type reference** — `C<number>` rather
    /// than `C` — asked of a `TypeId` alone.
    ///
    /// Upstream asks this as `t.objectFlags & ObjectFlagsReference != 0` and
    /// then reads `target` and `resolvedTypeArguments` straight off the type
    /// (`createTypeReference`, `checker.go:25103`). Here the `(symbol,
    /// arguments)` pair is the intern map's *key*, so the only way back from a
    /// `TypeId` is the reverse index
    /// [`Checker::type_reference_targets`](crate::checker::Checker) — see
    /// `docs/architecture/checker-notes-subst.md`.
    ///
    /// The `!is_empty` test is not defensive padding: it states what the bucket
    /// means. `create_type_reference` is only reached with at least one
    /// argument today (`crate::declared`'s `get_type_from_type_reference`
    /// returns the bare declared type when the target takes no parameters), so
    /// the test is currently redundant — and it is the *definition* of "carries
    /// type arguments", which is what the counters below are counting.
    pub(crate) fn receiver_carries_type_arguments(&self, id: TypeId) -> bool {
        self.type_reference_targets.get(&id).is_some_and(|(_, arguments)| !arguments.is_empty())
    }

    fn classify_unresolved_callee(&mut self, callee: Expression<'_>, callee_type: TypeId) {
        let error = self.intrinsics.error;
        match callee {
            Expression::Identifier(identifier) => {
                let symbol = identifier.node_id.and_then(|id| {
                    self.binder.resolve_name(
                        self.nodes,
                        self.node_map,
                        id,
                        identifier.text,
                        SymbolFlags::VALUE,
                    )
                });
                if symbol.is_none() {
                    bump(&COUNTERS.callee_name_unresolved);
                } else if callee_type == error {
                    bump(&COUNTERS.callee_name_type_error);
                } else {
                    bump(&COUNTERS.callee_name_not_object);
                }
            }
            Expression::PropertyAccessExpression(access) => {
                let receiver =
                    access.expression.map_or(error, |receiver| self.check_expression(receiver));
                if receiver == error {
                    bump(&COUNTERS.callee_property_receiver_error);
                    return;
                }
                bump(&COUNTERS.callee_property_receiver_typed);
                // The same three-way split the identifier side already gets,
                // and for the same reason: a member that types as a `number`
                // is a call upstream rejects too, and crediting it would be
                // false credit of the kind ADR-0038 refused.
                let member = match access.name {
                    Some(tsr_ast::MemberName::Identifier(name)) => {
                        self.get_property_of_type(receiver, name.text)
                    }
                    _ => None,
                };
                if member.is_none() {
                    bump(&COUNTERS.callee_member_absent);
                    // Split by the RECEIVER's shape, because
                    // `get_property_of_type` (`members.rs:163`) returns `None`
                    // for every shape but these two **before** it reads the
                    // name. "The lookup ran and said no" and "we never looked"
                    // are the same row otherwise, and they want opposite
                    // conclusions: the first is upstream's answer too.
                    bump(match self.store.get(receiver).data {
                        TypeData::Named { members: Some(_), .. } | TypeData::Anonymous { .. } => {
                            &COUNTERS.member_absent_looked_up
                        }
                        TypeData::Intrinsic { .. } => &COUNTERS.member_absent_intrinsic,
                        TypeData::Union { .. } | TypeData::Intersection { .. } => {
                            &COUNTERS.member_absent_composite
                        }
                        TypeData::Named { members: None, .. } => {
                            &COUNTERS.member_absent_named_no_members
                        }
                        _ => &COUNTERS.member_absent_other,
                    });
                } else if callee_type == error {
                    bump(&COUNTERS.callee_member_type_error);
                } else {
                    bump(&COUNTERS.callee_member_not_object);
                }
                // `bd tsr-fua`: the same 2,562 partitioned a second way, by
                // whether the receiver carries type arguments. Orthogonal to
                // the split above on purpose — the question "is this an
                // instantiated generic" is about the receiver's *provenance*
                // and the split above is about the member lookup's outcome,
                // and only the join of the two sizes `bd tsr-4qx`.
                bump(match (self.receiver_carries_type_arguments(receiver), member.is_some()) {
                    (true, false) => &COUNTERS.receiver_generic_member_absent,
                    (true, true) => &COUNTERS.receiver_generic_member_found,
                    (false, _) => &COUNTERS.receiver_not_generic,
                });
            }
            Expression::ElementAccessExpression(_) => bump(&COUNTERS.callee_element_access),
            _ => bump(&COUNTERS.callee_other_form),
        }
    }

    /// The type of a tagged template: ``tag`a${b}c` ``.
    ///
    /// Ported from `Checker.checkTaggedTemplateExpression` (`checker.go:10034`),
    /// which is three lines once the grammar checks are set aside: resolve the
    /// tag's signature and return its return type.
    ///
    /// # The same reduction as a call, because upstream treats it as one
    ///
    /// `getResolvedSignature` is the *same* entry point a call expression uses —
    /// a tagged template is a call whose arguments are the template strings
    /// array and the substitutions. So this shares
    /// [`Checker::resolve_call_signature`] rather than growing a second
    /// resolution path, but passes no argument list, so it keeps the restriction
    /// to a callee with exactly one call signature: choosing among several needs
    /// the argument types, and a tagged template's arguments are the template
    /// strings array and the substitutions, neither of which this port builds.
    ///
    /// The arguments are not checked, which is sound here for the same reason it
    /// is sound for a call — a non-generic signature's return type does not
    /// depend on them. **The template is still checked**, so its own line is
    /// populated; today that line is usually a gap, because
    /// `TemplateExpression` is unported and correctly stays so
    /// (`checker-notes-arrays.md` records why it is a workstream). A tag applied
    /// to a template with no substitutions gets a real answer for the template,
    /// since that is a `NoSubstitutionTemplateLiteral`.
    ///
    /// Not ported, each answering `errorType`: an optional chain (``tag?.`x` ``,
    /// **unobservable** — the grammar prohibits it and the parser rejects it, so
    /// the guard mirrors `check_call_expression` rather than covering a reachable
    /// case), explicit type arguments, a generic tag signature, and a tag whose type has
    /// anything other than exactly one call signature. The generic case is the
    /// one that matters most — `String.raw` and every typed template helper is
    /// generic — which is why the 289 gap lines this form carries will not all
    /// close here.
    pub(crate) fn check_tagged_template_expression(
        &mut self,
        node: &TaggedTemplateExpression<'_>,
    ) -> TypeId {
        let error = self.intrinsics.error;
        if node.question_dot_token.is_some() || !node.type_arguments.is_empty() {
            return error;
        }
        let Some(tag) = node.tag else { return error };
        let tag_type = self.check_expression(tag);
        // Checked for its own line; the template's type does not reach the
        // answer, exactly as a call's arguments do not.
        if let Some(template) = node.template {
            self.check_expression(template.into());
        }
        // `None` for the argument list: a tagged template's arguments are the
        // template strings array and the substitutions, neither of which this
        // port builds, so an overloaded tag stays a gap.
        let Some(signature) = self.resolve_call_signature(tag_type, None) else {
            return error;
        };
        if !signature.type_parameters.is_empty() {
            // A generic tag's return type depends on the inferred arguments,
            // which for a tagged template means inferring from the template
            // strings array and each substitution.
            return error;
        }
        signature.r#type
    }

    /// The single call signature of a type, or `None`.
    ///
    /// Ported from `getSignaturesOfType(t, SignatureKindCall)`
    /// (`checker.go:18959`) followed by the part of `resolveCall`
    /// (`checker.go:8843`) that is decidable without assignability: when there is
    /// exactly one candidate, resolution has nothing to choose and the answer is
    /// that candidate.
    ///
    /// Only an **anonymous object type** has signatures here — the shape a
    /// function, method, class, enum or value-module symbol has, and the shape a
    /// function expression or arrow is ([`TypeData::Anonymous`]). An interface
    /// with a call signature member, and a function *type node*
    /// (`(x: number) => void` in annotation position), both still resolve to
    /// nothing: the first needs call-signature members and the second is an
    /// unported type node. Both are gaps rather than wrong answers, and both are
    /// named in `docs/architecture/checker.md`.
    fn resolve_call_signature(
        &mut self,
        callee: TypeId,
        arguments: Option<&[Expression<'_>]>,
    ) -> Option<Signature> {
        // Counting is restricted to the call-expression path: a tagged template
        // passes no argument list, and folding its callees into the same buckets
        // would leave the funnel's denominator counting two different questions.
        let counted = arguments.is_some();
        let TypeData::Anonymous { symbol, .. } = self.store.get(callee).data else {
            if counted {
                bump(&COUNTERS.callee_not_anonymous);
            }
            return None;
        };
        let Some(signatures) = self.get_signatures_of_symbol(symbol) else {
            if counted {
                bump(&COUNTERS.callee_no_signatures);
            }
            return None;
        };
        match signatures.as_slice() {
            [signature] => {
                if counted {
                    bump(&COUNTERS.single_candidate);
                }
                Some(signature.clone())
            }
            // Zero: the callee is a class, an enum or a namespace — upstream
            // reports "this expression is not callable" and answers `errorType`.
            //
            // Two or more: an overload set, which needs assignability. **This
            // arm is live, and it is the second-largest blocker on a call in
            // the corpus** — 357 of the 1,496 declarations initialised by a
            // call to a locally declared `function`, 24%, against 884 (59%)
            // stopped one line up by the generic test in
            // [`Checker::check_call_expression`]. (Counted from the corpus
            // source carried in the `.types` baselines; declarations, not
            // assertion lines.)
            //
            // Choosing among candidates is `resolveCall` (`checker.go:8843`),
            // which picks by assignability. That is what
            // [`Checker::choose_overload`] does, over the argument domains
            // where a *negative* assignability answer can be trusted.
            [] => {
                if counted {
                    bump(&COUNTERS.callee_zero_signatures);
                }
                None
            }
            candidates => {
                let arguments = arguments?;
                if counted {
                    bump(&COUNTERS.overload_sets);
                }
                self.choose_overload(candidates, arguments)
            }
        }
    }

    /// The first candidate every argument is assignable to, or `None`.
    ///
    /// Ported from `Checker.chooseOverload` (`checker.go:9025`), which walks the
    /// candidate list in declaration order, keeps the ones `hasCorrectArity`
    /// (`checker.go:9107`) admits, and returns the first whose parameters every
    /// argument satisfies under the assignable relation. Declaration order is
    /// load-bearing — it is the whole tie-break — and
    /// [`Checker::get_signatures_of_symbol`] preserves it.
    ///
    /// # What is reduced away, each answering `None` so the call is `errorType`
    ///
    /// Upstream runs three passes over the candidates (subtype, strict-subtype,
    /// assignable) and, on total failure, reports against the candidate with the
    /// fewest problems. Only the assignable pass is here: the earlier two exist
    /// to prefer a more specific candidate when several match, and this reduction
    /// answers `None` rather than guessing whenever that could bite — see below.
    ///
    /// - **A generic candidate anywhere in the set.** Selecting it needs
    ///   `inferTypeArguments`; its return type would print `T`.
    /// - **A spread argument** (`f(...xs)`), which changes what arity means.
    /// - **A rest or `this` parameter** on any candidate, for the same reason.
    /// - **Any argument or parameter type outside [`SELECTABLE`]** — the domains
    ///   where a `false` from the relater is a real `false`. This is the guard
    ///   that keeps a narrow structural relation from promoting the wrong
    ///   overload, and it is why the object-typed overload sets in the corpus
    ///   stay gaps.
    /// - **More than one arity-and-assignability match with different return
    ///   types.** Upstream's subtype pass would pick among them by specificity;
    ///   without it, taking the first is a guess. Where every match returns the
    ///   *same* type the pass could not have changed the answer, so it is taken.
    fn choose_overload(
        &mut self,
        candidates: &[Signature],
        arguments: &[Expression<'_>],
    ) -> Option<Signature> {
        // The three rejections below were one short-circuiting `any` over the
        // candidates. They are separated so each can be counted, and tested in
        // the order the doc comment lists them — first match wins, which is why
        // a set that is both generic and object-typed reads as generic. The
        // *answer* is unchanged: any one of them still gaps the whole call.
        if candidates.iter().any(|candidate| !candidate.type_parameters.is_empty()) {
            bump(&COUNTERS.generic_candidate);
            return None;
        }
        if candidates.iter().any(|candidate| {
            candidate.this_parameter.is_some()
                || candidate.parameters.iter().any(|parameter| parameter.rest)
        }) {
            bump(&COUNTERS.this_or_rest_parameter);
            return None;
        }
        if candidates
            .iter()
            .any(|candidate| !candidate.parameters.iter().all(|p| self.is_selectable(p.r#type)))
        {
            bump(&COUNTERS.parameter_not_selectable);
            return None;
        }
        let mut argument_types = Vec::with_capacity(arguments.len());
        for &argument in arguments {
            if matches!(argument, Expression::SpreadElement(_)) {
                bump(&COUNTERS.spread_argument);
                return None;
            }
            let argument_type = self.check_expression(argument);
            if !self.is_selectable(argument_type) {
                bump(&COUNTERS.argument_not_selectable);
                return None;
            }
            argument_types.push(argument_type);
        }
        let mut chosen: Option<&Signature> = None;
        // Splits the empty-handed case in two: no candidate takes this many
        // arguments at all, against arity matching and the relater rejecting
        // every one of them. Only the second is a real negative from inside
        // `SELECTABLE`, and it is the row a widening would *not* move.
        let mut arity_matched = false;
        for candidate in candidates {
            if !has_correct_arity(candidate, argument_types.len()) {
                continue;
            }
            arity_matched = true;
            let applicable =
                argument_types.iter().zip(&candidate.parameters).all(|(&argument, parameter)| {
                    self.is_type_assignable_to(argument, parameter.r#type)
                });
            if !applicable {
                continue;
            }
            match chosen {
                // Upstream's subtype pass would decide this; see the doc
                // comment. Same return type either way means it could not have.
                Some(first) if first.r#type != candidate.r#type => {
                    bump(&COUNTERS.ambiguous_return);
                    return None;
                }
                Some(_) => {}
                None => chosen = Some(candidate),
            }
        }
        match chosen {
            Some(signature) => {
                bump(&COUNTERS.selected);
                // Selection succeeding is not the same as the call answering.
                // An unported return annotation prints `error` from here, and
                // in the gradient that is indistinguishable from a gap.
                if signature.r#type == self.intrinsics.error {
                    bump(&COUNTERS.selected_return_error);
                }
            }
            None if arity_matched => bump(&COUNTERS.no_assignable_candidate),
            None => bump(&COUNTERS.arity_no_match),
        }
        chosen.cloned()
    }

    /// Whether a `false` from [`Checker::is_type_assignable_to`] about this type
    /// means the relation does not hold. See [`SELECTABLE`].
    fn is_selectable(&self, id: TypeId) -> bool {
        // A union is selectable when every constituent is: the relation
        // distributes over it, so a `false` is as trustworthy as the worst
        // constituent's. A union carrying a symbol is an enum or a named alias,
        // and falls through to the flag test, which rejects it: a union's own
        // flags are `UNION`, not the union of its constituents' flags.
        if let TypeData::Union { types, symbol: None, .. } = &self.store.get(id).data {
            return types.iter().all(|&t| self.is_selectable(t));
        }
        let flags = self.store.get(id).flags;
        !flags.is_empty() && SELECTABLE.contains(flags)
    }
}

/// Whether a signature accepts exactly this many arguments.
///
/// Ported from `Checker.hasCorrectArity` (`checker.go:9107`), reduced to the two
/// bounds that survive once rest parameters, spread arguments and the
/// signature-help trailing comma are excluded by
/// [`Checker::choose_overload`]: at least the required parameters, at most all
/// of them.
fn has_correct_arity(candidate: &Signature, argument_count: usize) -> bool {
    let required = candidate.parameters.iter().take_while(|p| !p.optional).count();
    argument_count >= required && argument_count <= candidate.parameters.len()
}

#[cfg(test)]
mod tests {
    use tsr_ast::Statement;
    use tsr_core::Arena;

    use crate::{Checker, types::TypeId};

    /// A checker over one file, plus the type of the variable named `name`.
    ///
    /// **No lib files are loaded**, which is the confound recorded in
    /// `docs/architecture/checker-notes-recv.md`: `number[]`, `Array`, `Promise`
    /// and every other lib global answer `error` in this harness for reasons
    /// that have nothing to do with the code under test. Every fixture below
    /// therefore names no lib type, and
    /// [`the_no_lib_control_still_holds`](self::tests::the_no_lib_control_still_holds)
    /// pins that the confound is present rather than assuming it is absent.
    fn with_declared_variable<R>(
        source: &str,
        name: &str,
        body: impl FnOnce(&mut Checker<'_, '_>, TypeId) -> R,
    ) -> R {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(
            parsed.diagnostics.is_empty(),
            "fixture must parse: {:?}",
            parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
        );
        let bound = tsr_binder::bind(
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "test.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let declaration = parsed
            .source_file
            .statements
            .iter()
            .filter_map(|statement| match statement {
                Statement::VariableStatement(statement) => statement.declaration_list,
                _ => None,
            })
            .flat_map(|list| list.declarations.iter())
            .find(|declaration| match declaration.name {
                Some(tsr_ast::BindingName::Identifier(identifier)) => identifier.text == name,
                _ => false,
            })
            .copied()
            .expect("the fixture must declare the variable");
        let symbol = bound
            .symbol_of(declaration.node_id.expect("a registered node"))
            .expect("the variable must be bound");
        let id = checker.get_type_of_symbol(symbol);
        body(&mut checker, id)
    }

    /// A receiver written with type arguments is reachable *as* one from its
    /// `TypeId` alone, which is what the `bd tsr-fua` counters ask.
    ///
    /// Mutation that reddens this and not the sibling below: make
    /// `receiver_carries_type_arguments` return `false` unconditionally.
    #[test]
    fn a_generic_receiver_carries_type_arguments() {
        with_declared_variable(
            "interface P<T> { get(): string; }\ndeclare var p: P<number>;",
            "p",
            |checker, id| {
                assert_eq!(checker.type_to_string(id), "P<number>");
                assert!(checker.receiver_carries_type_arguments(id));
            },
        );
    }

    /// The same declaration with the type parameter removed — one token apart,
    /// as `checker-notes-recv.md`'s A/A' pair is.
    ///
    /// Mutation that reddens this and not the sibling above: make
    /// `receiver_carries_type_arguments` return `true` unconditionally.
    #[test]
    fn a_plain_receiver_does_not() {
        with_declared_variable(
            "interface P { get(): string; }\ndeclare var p: P;",
            "p",
            |checker, id| {
                assert_eq!(checker.type_to_string(id), "P");
                assert!(!checker.receiver_carries_type_arguments(id));
            },
        );
    }

    /// **The load-bearing fact of `bd tsr-fua`**, and the reason the counter
    /// this commit adds is not quite the one the item was written to ask for.
    ///
    /// This test was written as the tripwire for `bd tsr-4qx` step 4, and the
    /// tripwire fired: until then `create_type_reference` built `P<number>`
    /// with `members: None` and the lookup never ran, which was the load-bearing
    /// fact of `bd tsr-fua` ("the 557 cannot contain this shape"). The flip
    /// ended that fact on schedule — `receiver_generic_member_found` stopped
    /// being a control and became the measurement, as its own doc predicted —
    /// and this test now pins the replacement behaviour: the lookup runs, and
    /// the member's type is the **instantiated** one, never the type parameter.
    #[test]
    fn a_generic_receiver_finds_members_and_their_types_are_instantiated() {
        with_declared_variable(
            "interface P<T> { value: T; }\ndeclare var p: P<number>;",
            "p",
            |checker, id| {
                assert!(checker.get_property_of_type(id, "value").is_some());
                // The seam substitutes: `T` with `T := number` is `number`. An
                // implementation that reads the symbol's type directly prints
                // `T`, which is the wrong answer `members: None` existed to
                // prevent.
                let value = checker.get_type_of_property_of_type(id, "value").expect("found");
                assert_eq!(checker.type_to_string(value), "number");
            },
        );
        // The non-generic sibling keeps finding its member, so the assertion
        // above is about instantiation and not about interface members in
        // general. This is the A/A' discrimination.
        with_declared_variable(
            "interface P { get(): string; }\ndeclare var p: P;",
            "p",
            |checker, id| assert!(checker.get_property_of_type(id, "get").is_some()),
        );
    }

    /// The one-line control from `checker-notes-recv.md`. If this ever passes,
    /// the harness has grown lib files and every fixture above must be re-read
    /// for whether it still measures what it claims.
    #[test]
    fn the_no_lib_control_still_holds() {
        with_declared_variable("declare var x: number[];", "x", |checker, id| {
            assert_eq!(checker.type_to_string(id), "error");
        });
    }
}
