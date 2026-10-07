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
use tsr_diagnostics::{Diagnostic, messages};

use crate::calls::counters::{COUNTERS, bump};
use crate::{
    checker::Checker,
    flags::TypeFlags,
    relater::{Relation, Ternary},
    signatures::{Signature, SignatureKind},
    types::{TypeData, TypeId},
};

///
/// Overload selection is the first caller that depends on a **negative** answer
/// from the relater: skipping a candidate is what makes the next one win. Every
/// other caller so far only depends on a positive one.
///
/// # `SELECTABLE` is gone, and the number that removed it
///
/// Until `bd tsr-kmzf` this was a **flag set** — the domains
/// `isSimpleTypeRelatedTo` decides on flags alone — because the relater had no
/// way to say *"I could not tell"* and a false negative here silently promotes
/// the next overload. `examples/ternary.rs` measured what that cost and what
/// replacing it buys (`docs/architecture/checker-notes-assign.md` §5–§6):
///
/// - the flag set is the thing that binds, not the relation. 33 of the 492
///   classified gap lines are decided **correctly by the relation as it already
///   was**; `SELECTABLE` simply never asked, because it contains
///   `STRING_LITERAL` but not `UNION`;
/// - decidability is a property of the **pair**, not of either type, so no
///   widening of a flag set could ever have been the fix. That is now
///   [`Checker::relate_ternary`]'s [`Ternary::Unknown`], and this module asks it
///   about the pair instead of asking a flag set about each side.
///
/// # The one positional refusal that survives, and why it is not a ratio
///
/// An `any` **parameter** refuses the whole set. `any` relates to everything in
/// both directions (`isSimpleTypeRelatedTo`, `internal/checker/relater.go`), so
/// a candidate carrying one is *trivially* applicable and the declaration-order
/// scan always stops on the first such candidate. Upstream selects correctly
/// there because it has the argument-order and inference machinery this port
/// does not — so stopping on the first is a **wrong rule, not a bad trade**, and
/// `docs/conventions.md` says a rule is not priced. Measured, the refusal costs
/// 7 conversions and removes 24 would-be-wrong lines: 40/26 becomes **33/2**.
///
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
        /// A resolved single candidate is **not** the end of the call: the
        /// return type still has to be produced. `bd tsr-klm` found this bucket
        /// to be the largest single reason an admitted call line gaps, and
        /// undivided it reads as "resolution succeeded" — which is true and is
        /// not the question. These four split it by what happened next.
        /// The candidate is generic, so the answer comes from inference.
        single_candidate_generic = "    of which generic, to inference",
        /// …and inference could not read a candidate off the arguments.
        /// **The largest single reason an admitted call line gaps**
        /// (`bd tsr-klm`): resolution succeeded and inference is what stops.
        single_candidate_generic_error = "      of which inference gapped",
        /// `checkNoTypeArguments` — type arguments on a signature with no
        /// type parameters.
        single_candidate_type_arguments = "    of which type arguments on a non-generic signature",
        /// Resolution succeeded, the signature is not generic, and its return
        /// type is itself a gap — the call bought nothing.
        single_candidate_return_error = "    of which the return type is itself error",
        /// Resolution succeeded and produced a real type. A gap line landing
        /// here is blocked by something other than its call.
        single_candidate_answered = "    of which ANSWERED (CONTROL: not a gap)",
        /// Entries to `choose_overload` — the denominator for the rows below.
        overload_sets = "overload sets reaching choose_overload",
        /// A generic candidate anywhere in the set. `bd tsr-4sc.8`.
        generic_candidate = "  a generic candidate in the set",
        /// A `this` or rest parameter on any candidate.
        this_or_rest_parameter = "  a this or rest parameter",
        /// An untyped call — the callee types as `any`, so the call answers
        /// `any`. `resolveUntypedCall` (`checker.go:9902`).
        untyped_call = "  UNTYPED CALL — callee is `any`, answered `any`",
        /// An `any` parameter on any candidate — the positional refusal that
        /// replaced the `SELECTABLE` parameter gate. `bd tsr-kmzf`.
        parameter_any = "  an `any` parameter (refused positionally)",
        /// `f(...xs)`.
        spread_argument = "  a spread argument",
        /// A pair [`Checker::relate_ternary`] cannot decide, anywhere in the
        /// scan. This is the row `SELECTABLE`'s two gates became, and unlike
        /// them it is a property of the **pair**. `bd tsr-kmzf`.
        undecidable_pair = "  a pair the relation cannot decide",
        /// No candidate accepts this many arguments.
        arity_no_match = "  no candidate with matching arity",
        /// Arity matched somewhere and every candidate was **decidably**
        /// rejected — no `Unknown` anywhere in the scan, so this is a real
        /// negative from the relater rather than an absence.
        no_assignable_candidate = "  arity matched, nothing assignable",
        /// Several matches with different return types; upstream's subtype
        /// pass would decide, and this port will not guess.
        ambiguous_return = "  ambiguous: matches with different returns",
        /// A candidate was chosen.
        selected = "  SELECTED",
        /// **Subset of `selected`.** The winner's return type is `error`, so
        /// the call still prints a gap: selection worked and bought nothing.
        selected_return_error = "    of which the return type is error",
        /// The `new` funnel. `check_new_expression` lives in
        /// [`crate::expressions`] and carried **no counters at all** until
        /// `bd tsr-klm`, which is why that probe's C3 control fired on 2,569
        /// lines: `new` is the *more* admitted half of the call row
        /// (`checker-notes-callres.md` §4 measures its callee as typed 66.2%
        /// of the time against a call's 21–29%) and it was the invisible half.
        new_expressions = "new expressions checked",
        /// ~~`new C<T>()` — needs `inferTypeArguments`, same mechanism as a
        /// generic call.~~ **Stale, corrected 2026-08-11.** WRITTEN type
        /// arguments need substitution, not inference, and both roads have
        /// them: the class path at `expressions.rs` (matching arity) and
        /// §161's constructor-interface path. What remains unported is
        /// INFERENCE for `new C(args)` with no written list, and the
        /// short-list-with-defaults window (§191, measured −17 when the
        /// gate was widened without `fillMissingTypeArguments`).
        new_type_arguments = "  explicit type arguments (inference half unported)",
        /// The callee is not an anonymous object type: a lib constructor
        /// *interface* (`DateConstructor`, `MapConstructor`) — `bd tsr-4sa`.
        new_callee_not_anonymous = "  callee type is not an object type",
        /// An anonymous callee whose symbol carries no `CLASS` flag.
        new_callee_not_class = "  callee symbol is not a class",
        /// A class symbol with no declaration recorded.
        new_no_declaration = "  callee symbol has no declaration",
        /// The first declaration is neither a class declaration nor a class
        /// expression.
        new_declaration_not_class_like = "  declaration is not class-like",
        /// `class C<T>` with no written type arguments (inference, unported),
        /// a written list whose arity differs, or an argument that itself gaps.
        new_type_parameters = "  the class is generic, and not instantiable here",
        /// `new C<string>()` — written type arguments, matching arity, every
        /// argument typed. `bd tsr-tgov`.
        new_instantiated = "  INSTANTIATED from written type arguments",
        /// Upstream reports and answers `errorType` for an abstract class.
        new_abstract = "  the class is abstract",
        /// The instance type was produced.
        new_resolved = "  RESOLVED to the declared instance type",
        /// …and it is itself a gap, so the `new` bought nothing.
        new_resolved_error = "    of which the declared type is itself error",
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

/// Native instantiation-expression signature filtering/checking outcome.
/// Unsupported work is the enclosing Option's None, not an applicable image.
pub enum InstantiationExpressionSignature {
    /// Nongeneric signature or incorrect written type-argument arity.
    Inapplicable,
    /// Native checkTypeArguments rejected and reported; retain the original.
    ConstraintRejected,
    /// Fully substituted call/construct signature with no own parameters.
    Instantiated(Signature),
}

/// What `resolveCallExpression`'s head decided before `resolveCall` runs.
pub(crate) enum CallHead {
    /// The head reported (or another error was already reported); `resolveCall`
    /// is not reached — `resolveErrorCall`/`resolveUntypedCall` upstream.
    Done,
    /// The callee's apparent type has call signatures; `resolveCall` runs
    /// over them.
    Resolve(TypeId),
    /// A list this port cannot certify complete: nothing is reported.
    Unknown,
}

/// What the arity half of `resolveCall` decided for one call.
enum CallArity {
    /// The signature list is not certified; the declaration rules run.
    Undecided,
    /// A type-argument or argument arity error was reported; upstream
    /// reports nothing else for the call.
    Reported,
    /// Some candidate has a correct arity; the arguments decide. Carries
    /// the candidate when it is the single non-generic one.
    Applicable(Option<Box<Signature>>),
    /// The single candidate is generic, written without type arguments:
    /// `chooseOverload` infers its type arguments and checks the
    /// instantiation (`checker.go:9055`).
    ApplicableGeneric(Box<Signature>),
    /// Several candidates, none generic, in `reorderCandidates` order; those
    /// whose arity matched. `chooseOverload` checks each against the
    /// arguments (`checker.go:9025`).
    ApplicableOverloads(Vec<Signature>),
}

/// One entry of `getEffectiveCallArguments`: the written argument (or the
/// spread a synthetic element came from) and whether it is spread-like
/// (`isSpreadArgument`).
struct EffectiveArgument {
    node: tsr_ast::NodeId,
    spread: bool,
}

/// `checkNonNullTypeWithReporter`'s result for a callee.
enum NonNullCallee {
    Type(TypeId),
    /// Upstream's `errorType`.
    Error,
    /// The callee's type is not one upstream can hold; nothing is reported.
    Unknown,
}

impl<'a> Checker<'a, '_> {
    /// The diagnostic half of `resolveCallExpression` (`checker.go:8471`) up
    /// to `resolveCall`: the non-null check on the callee
    /// (`checkNonNullTypeWithReporter` with
    /// `reportCannotInvokePossiblyNullOrUndefinedError`), the untyped-call arm
    /// (`isUntypedFunctionCall`, TS2347 for written type arguments), and the
    /// no-call-signature arm (TS2348 when construct signatures exist, else
    /// `invocationError`'s TS2349 head).
    ///
    /// Runs once per call node from the diagnostic walk. It reads the callee's
    /// cached expression type and the shared kind-specific signature resolver
    /// (`signatures_of_type_kind`); it adds no cache. A signature list that
    /// resolver cannot certify (`None`) answers [`CallHead::Unknown`] and the
    /// call reports nothing, which keeps an incomplete list from reading as
    /// "not callable".
    pub(crate) fn check_call_expression_head(&mut self, node: tsr_ast::NodeId) -> CallHead {
        let Some(tsr_ast::Node::CallExpression(call)) = self.node_map.get(node) else {
            return CallHead::Unknown;
        };
        let Some(callee) = call.expression else { return CallHead::Unknown };
        let Some(callee_id) = callee.node_id() else { return CallHead::Unknown };
        if matches!(
            self.nodes.kind(callee_id),
            tsr_ast::SyntaxKind::SuperKeyword | tsr_ast::SyntaxKind::ImportKeyword
        ) {
            return CallHead::Unknown;
        }
        if self.callee_reads_object_literal_this(callee) {
            return CallHead::Unknown;
        }
        let mut func_type = self.check_expression(callee);
        // `isCallChain(node)`: the call is a link of an optional chain.
        if call.question_dot_token.is_some() || self.expression_is_optional_chain(callee_id) {
            func_type = self.get_optional_expression_type(
                func_type,
                Some(callee_id),
                call.question_dot_token.is_some(),
            );
        }
        let func_type = match self.check_non_null_callee(func_type, callee_id) {
            NonNullCallee::Type(t) => t,
            NonNullCallee::Error => return CallHead::Done,
            NonNullCallee::Unknown => return CallHead::Unknown,
        };
        if Some(func_type) == self.silent_never_type {
            return CallHead::Done;
        }
        let apparent = self.apparent_type(func_type);
        if self.is_error(apparent) {
            return CallHead::Done;
        }
        // `isUntypedFunctionCall`'s first two arms need no signature list.
        let untyped = if self.is_untyped_any_callee(func_type, apparent) {
            // An `any` this port produced for an unresolved name, a missing
            // property or an unresolved module is upstream's `errorType`,
            // which reports nothing; only a written `any` is the same claim.
            if self.store.get(func_type).flags.intersects(TypeFlags::ANY)
                && !self.is_error(func_type)
                && !self.untyped_any_is_written(callee)
            {
                return CallHead::Unknown;
            }
            true
        } else if self.is_function_or_method_type(apparent) {
            // A function or method's own type has the declaration's call
            // signatures, so neither the untyped arm (no call signatures) nor
            // the not-callable arm can fire. This port's signature list
            // resolves return types eagerly, which upstream's
            // `getSignaturesOfType` does not, and building it from inside the
            // callee's own body re-enters its return inference.
            return CallHead::Resolve(apparent);
        } else {
            let Some(call_count) = self.head_signature_count(apparent, SignatureKind::Call) else {
                return CallHead::Unknown;
            };
            let Some(construct_count) =
                self.head_signature_count(apparent, SignatureKind::Construct)
            else {
                return CallHead::Unknown;
            };
            let Some(untyped) = self.is_untyped_signatureless_call(
                func_type,
                apparent,
                call_count,
                construct_count,
            ) else {
                return CallHead::Unknown;
            };
            if !untyped && call_count == 0 {
                if self.head_could_contain_type_variables(func_type, 3) {
                    return CallHead::Unknown;
                }
                if construct_count != 0 {
                    let printed = self.type_to_string(func_type);
                    self.report_at_node(
                        node,
                        Diagnostic::with_args(
                            &messages::VALUE_OF_TYPE_0_IS_NOT_CALLABLE_DID_YOU_MEAN_TO_INCLUDE_NEW,
                            self.error_span(node),
                            [printed],
                        ),
                    );
                } else {
                    self.invocation_error(
                        callee_id,
                        call.arguments.is_empty(),
                        SignatureKind::Call,
                    );
                }
                return CallHead::Done;
            }
            untyped
        };
        if !untyped {
            return CallHead::Resolve(apparent);
        }
        if !self.is_error(func_type) && !call.type_arguments.is_empty() {
            self.report_at_node(
                node,
                Diagnostic::new(
                    &messages::UNTYPED_FUNCTION_CALLS_MAY_NOT_ACCEPT_TYPE_ARGUMENTS,
                    self.error_span(node),
                ),
            );
        }
        CallHead::Done
    }

    /// Every diagnostic `resolveCallExpression` issues for one call node.
    pub(crate) fn check_call_expression_diagnostics(&mut self, node: tsr_ast::NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        match self.check_call_expression_head(node) {
            CallHead::Done => {}
            CallHead::Resolve(apparent) => {
                match self.check_resolve_call_arity(node, apparent, SignatureKind::Call) {
                    CallArity::Reported => {}
                    CallArity::Applicable(Some(signature)) => {
                        self.check_single_candidate_arguments(node, &signature);
                    }
                    CallArity::ApplicableGeneric(candidate) => {
                        if !self.check_single_generic_candidate_arguments(node, &candidate) {
                            self.check_call_arity(node, false);
                        }
                    }
                    CallArity::ApplicableOverloads(candidates) => {
                        if !self.check_overload_candidates_arguments(node, &candidates)
                            && !self.report_overload_argument_failure(node)
                        {
                            self.check_call_arity(node, false);
                        }
                    }
                    CallArity::Applicable(None) => {
                        if !self.report_overload_argument_failure(node) {
                            self.check_call_arity(node, false);
                        }
                    }
                    CallArity::Undecided => {
                        self.check_call_arity(node, true);
                        self.check_call_type_argument_arity(node);
                    }
                }
            }
            CallHead::Unknown => {
                self.check_call_arity(node, true);
                self.check_call_type_argument_arity(node);
            }
        }
    }

    /// The arity half of `resolveCall` (`checker.go:8843`) and its
    /// `reportCallResolutionErrors` (`checker.go:9649`).
    ///
    /// `chooseOverload` skips a candidate failing `hasCorrectTypeArgumentArity`
    /// (`checker.go:9214`) or `hasCorrectArity` (`checker.go:9107`) before any
    /// argument or type-argument check. When every candidate is skipped, no
    /// candidate reaches the argument-error, generic-rest or constraint arms,
    /// so the report is the last arm alone: `getTypeArgumentArityError`
    /// (`checker.go:9853`) when no signature has a correct type-argument
    /// count, else `getArgumentArityError` (`checker.go:9705`) over those that
    /// do. When some candidate passes both, the arguments decide and
    /// [`CallArity::Applicable`] hands over to the argument-type rules.
    ///
    /// Reads the callee type's signature list (no new cache); an uncertified
    /// list or a JS file is [`CallArity::Undecided`].
    fn check_resolve_call_arity(
        &mut self,
        node: tsr_ast::NodeId,
        apparent: TypeId,
        kind: SignatureKind,
    ) -> CallArity {
        let (type_arguments, arguments, callee, is_call) = match self.node_map.get(node) {
            Some(tsr_ast::Node::CallExpression(call)) => {
                (call.type_arguments, call.arguments, call.expression, true)
            }
            Some(tsr_ast::Node::NewExpression(new)) => {
                (new.type_arguments, new.arguments, new.expression, false)
            }
            _ => return CallArity::Undecided,
        };
        if self.in_js_file(node) {
            return CallArity::Undecided;
        }
        let Some(signatures) = self.head_signatures(apparent, kind) else {
            return CallArity::Undecided;
        };
        if signatures.is_empty() {
            return CallArity::Undecided;
        }
        // A union's composite signatures take the parameters of whichever
        // member list matched first, and their return types subtype-reduce
        // (`getReturnTypeOfSignature`, `checker.go:20013`); this port has no
        // subtype reduction, so a receiver typed by such a return can reach a
        // different member list than upstream's. The argument count decides
        // nothing there. A sole composite signature of the right arity whose
        // `this` arm fails (`isSignatureApplicable`, `checker.go:9260`) is
        // `candidatesForArgumentError`'s only entry, reported by
        // [`Checker::check_this_argument`].
        if self.store.get(apparent).flags.intersects(TypeFlags::UNION) {
            if !type_arguments.is_empty() {
                return self.check_type_argument_arity_only(node, type_arguments, &signatures);
            }
            if let [signature] = signatures.as_slice()
                && signature.type_parameters.is_empty()
                && signature.this_parameter.is_some()
                && let Some(effective) = self.effective_call_arguments(arguments)
                && self.has_correct_arity(signature, &effective, false) == Some(true)
                && self.check_this_argument(node, signature, true) == Ternary::NotRelated
            {
                return CallArity::Reported;
            }
            return CallArity::Undecided;
        }
        // An immediately invoked function's minimum reads the written
        // argument count while its printed optionality reads the expanded
        // one; with a spread argument the two differ and this port's
        // signature carries neither (`immediately_invoked_argument_count`),
        // so its minimum is not upstream's.
        if arguments.iter().any(|argument| matches!(argument, Expression::SpreadElement(_)))
            && callee.is_some_and(|callee| {
                let mut callee = callee;
                while let Expression::ParenthesizedExpression(inner) = callee {
                    match inner.expression {
                        Some(expression) => callee = expression,
                        None => return false,
                    }
                }
                matches!(callee, Expression::FunctionExpression(_) | Expression::ArrowFunction(_))
            })
        {
            return CallArity::Undecided;
        }
        let count = type_arguments.len();
        let arities: Vec<(usize, usize)> = signatures
            .iter()
            .map(|signature| {
                (
                    Self::min_type_argument_count(&signature.type_parameters),
                    signature.type_parameters.len(),
                )
            })
            .collect();
        let type_argument_arity_ok =
            |&(min, max): &(usize, usize)| count == 0 || count >= min && count <= max;
        if !arities.iter().any(type_argument_arity_ok) {
            self.report_type_argument_arity_error(node, type_arguments, &arities);
            return CallArity::Reported;
        }
        let candidates: Vec<Signature> = signatures
            .into_iter()
            .zip(&arities)
            .filter(|(_, arity)| type_argument_arity_ok(arity))
            .map(|(signature, _)| signature)
            .collect();
        let Some(effective) = self.effective_call_arguments(arguments) else {
            return CallArity::Undecided;
        };
        let no_argument_list =
            !is_call && self.new_has_no_argument_list(node, callee, type_arguments);
        let mut applicable = false;
        for candidate in &candidates {
            match self.has_correct_arity(candidate, &effective, no_argument_list) {
                Some(true) => {
                    applicable = true;
                    break;
                }
                Some(false) => {}
                None => return CallArity::Undecided,
            }
        }
        if applicable {
            // `isSingleNonGenericCandidate`: the sole candidate is checked
            // against the arguments as is, no inference or type arguments.
            if let [candidate] = candidates.as_slice()
                && candidate.type_parameters.is_empty()
                && type_arguments.is_empty()
                && !effective.iter().any(|argument| argument.spread)
                && effective.len() == arguments.len()
            {
                return CallArity::Applicable(Some(Box::new(candidate.clone())));
            }
            // The single generic candidate of a call: `chooseOverload`'s
            // loop body (`checker.go:9046`) checks its written type arguments
            // or infers them, and instantiates it before the applicability
            // check. Candidates failing `hasCorrectTypeArgumentArity` are
            // already filtered out above.
            if let [candidate] = candidates.as_slice()
                && !candidate.type_parameters.is_empty()
                && is_call
                && !effective.iter().any(|argument| argument.spread)
                && effective.len() == arguments.len()
            {
                return CallArity::ApplicableGeneric(Box::new(candidate.clone()));
            }
            if is_call
                && type_arguments.is_empty()
                && let Some(overloads) = self.non_generic_overload_candidates(
                    &candidates,
                    &effective,
                    arguments.len(),
                    no_argument_list,
                )
            {
                return CallArity::ApplicableOverloads(overloads);
            }
            return CallArity::Applicable(None);
        }
        let error_node = match callee.and_then(|callee| callee.node_id()) {
            Some(callee) if is_call => self.call_error_node(callee),
            _ => node,
        };
        self.report_argument_arity_error(node, error_node, &candidates, &effective);
        CallArity::Reported
    }

    /// `getSignatureApplicabilityError` (`checker.go`) for a single
    /// non-generic candidate whose arity matched: the `this` argument
    /// ([`Checker::check_this_argument`]), then each argument against
    /// `getTypeAtPosition`, stopping at the first failure (TS2345, or the
    /// object literal's excess-property elaboration). An unsupported
    /// parameter type stops the walk.
    fn check_single_candidate_arguments(&mut self, node: tsr_ast::NodeId, signature: &Signature) {
        let arguments = match self.node_map.get(node) {
            Some(tsr_ast::Node::CallExpression(call)) => call.arguments,
            Some(tsr_ast::Node::NewExpression(new)) => new.arguments,
            _ => return,
        };
        if self.check_this_argument(node, signature, true) != Ternary::Related {
            return;
        }
        for (position, argument) in arguments.iter().enumerate() {
            let Some(argument_id) = argument.node_id() else { return };
            let Some(target) = self.signature_type_at_position(signature, position) else {
                return;
            };
            if self.is_error(target) {
                return;
            }
            if self.argument_type_is_not_upstreams(*argument) {
                return;
            }
            let before = self.diagnostics.len();
            self.check_excess_properties(target, argument_id);
            if self.diagnostics.len() != before {
                return;
            }
            let source = self.check_expression(*argument);
            if self.mapped_types.get(&source).is_some_and(|info| info.name_type.is_some()) {
                // A mapped type with an `as` clause: this port's member
                // resolution of it over an array source is not upstream's
                // (`mappedTypeWithNameClauseAppliedToArrayType`).
                return;
            }
            if self.report_argument_failure(argument_id, source, target) {
                return;
            }
        }
    }

    /// `isSignatureApplicable`'s `this`-argument arm (`checker.go:9260`): a
    /// signature whose `this` type (`getThisTypeOfSignature`) is present and
    /// not `void` applies only when the call's `this` argument
    /// (`getThisArgumentOfCall`/`getThisArgumentType`, `checker.go:9345`;
    /// `void` for a bare call) is related to it. A `new` call and a call of
    /// a `super` property skip the arm. With `report`, a failure is
    /// `checkTypeRelatedToEx` at the `this` argument node (the call node
    /// when there is none) under
    /// `The_this_context_of_type_0_is_not_assignable_to_method_s_this_of_type_1`
    /// (TS2684); no elaboration. `Unknown` when the relation is undecided:
    /// the caller declines, as for an undecided argument.
    fn check_this_argument(
        &mut self,
        node: tsr_ast::NodeId,
        signature: &Signature,
        report: bool,
    ) -> Ternary {
        let Some(this_type) =
            signature.this_parameter.as_ref().map(|parameter| self.parameter_type(parameter))
        else {
            return Ternary::Related;
        };
        if this_type == self.intrinsics.void {
            return Ternary::Related;
        }
        match self.node_map.get(node) {
            Some(tsr_ast::Node::CallExpression(call)) => {
                let super_property =
                    call.expression.and_then(|callee| callee.node_id()).is_some_and(|callee| {
                        match self.node_map.get(callee) {
                            Some(tsr_ast::Node::PropertyAccessExpression(access)) => {
                                access.expression
                            }
                            Some(tsr_ast::Node::ElementAccessExpression(access)) => {
                                access.expression
                            }
                            _ => None,
                        }
                        .and_then(|receiver| receiver.node_id())
                        .is_some_and(|receiver| {
                            self.nodes.kind(receiver) == tsr_ast::SyntaxKind::SuperKeyword
                        })
                    });
                if super_property {
                    return Ternary::Related;
                }
            }
            Some(tsr_ast::Node::NewExpression(_)) => return Ternary::Related,
            _ => {}
        }
        let source = self.this_argument_type_of_call(Some(node));
        let verdict = self.relate_ternary(source, this_type, Relation::Assignable);
        if verdict == Ternary::NotRelated && report {
            let at = self
                .this_argument_of_call(node)
                .and_then(|(receiver, _)| receiver.node_id())
                .unwrap_or(node);
            let span = self.error_span(at);
            self.report_relation_failure(
                at,
                span,
                None,
                source,
                this_type,
                Some(&messages::THE_THIS_CONTEXT_OF_TYPE_0_IS_NOT_ASSIGNABLE_TO_METHOD_S_THIS_OF_TYPE_1),
            );
        }
        verdict
    }

    /// The candidates `chooseOverload` (`checker.go:9025`) would check when
    /// a call written without type arguments has several signatures, every
    /// one non-generic: those passing `hasCorrectArity`, in
    /// `reorderCandidates` (`checker.go:8958`) order.
    ///
    /// `reorderCandidates` keeps declaration order when every signature
    /// shares one declaration parent and none is specialized
    /// (`SignatureFlagsHasLiteralTypes`, set for a `LiteralType` parameter
    /// annotation in `getSignatureFromDeclaration`); anything else is
    /// declined, as is a spread argument.
    /// The literal test reads the parameter's type, a superset of the written
    /// node test: it can only decline more.
    fn non_generic_overload_candidates(
        &mut self,
        candidates: &[Signature],
        effective: &[EffectiveArgument],
        argument_count: usize,
        no_argument_list: bool,
    ) -> Option<Vec<Signature>> {
        if candidates.len() < 2
            || effective.len() != argument_count
            || effective.iter().any(|argument| argument.spread)
        {
            return None;
        }
        let parent = self.nodes.parent(candidates[0].declaration);
        for candidate in candidates {
            if !candidate.type_parameters.is_empty()
                || self.nodes.parent(candidate.declaration) != parent
                || candidate.parameters.iter().any(|parameter| {
                    let parameter_type = self.parameter_type(parameter);
                    self.store
                        .get(parameter_type)
                        .flags
                        .intersects(TypeFlags::LITERAL | TypeFlags::NULL)
                })
            {
                return None;
            }
        }
        let mut matched = Vec::new();
        for candidate in candidates {
            if self.has_correct_arity(candidate, effective, no_argument_list)? {
                matched.push(candidate.clone());
            }
        }
        Some(matched)
    }

    /// `chooseOverload` (`checker.go:9025`) over several non-generic
    /// candidates with the assignable relation, then
    /// `reportCallResolutionErrors` (`checker.go:9649`): when no candidate is
    /// applicable, the last failing one (`candidatesForArgumentError`'s last
    /// entry) is re-checked with `reportErrors`. With one failing candidate
    /// its diagnostic is reported as is (TS2345); with several, upstream
    /// chains it under `The_last_overload_gave_the_following_error` and
    /// `No_overload_matches_this_call` at the same location, so its head is
    /// TS2769. This port's `Diagnostic` has no message chain or related
    /// information yet (`tsr-2zk.22`): only the head is emitted, and the
    /// chain and `The_last_overload_is_declared_here` are dropped.
    ///
    /// Arguments are read from their checked types (no new cache). Upstream
    /// checks each argument under the candidate's parameter as contextual
    /// type, so an argument whose type depends on it is declined: an object,
    /// array or class literal, a context-sensitive function, and the shapes
    /// of [`Checker::argument_type_is_not_upstreams`]. A relation this port
    /// cannot decide also declines. Answers `false` on decline.
    fn check_overload_candidates_arguments(
        &mut self,
        node: tsr_ast::NodeId,
        candidates: &[Signature],
    ) -> bool {
        let Some(tsr_ast::Node::CallExpression(call)) = self.node_map.get(node) else {
            return false;
        };
        let arguments = call.arguments;
        for argument in arguments {
            let mut inner = *argument;
            while let Expression::ParenthesizedExpression(parenthesized) = inner {
                let Some(expression) = parenthesized.expression else { return false };
                inner = expression;
            }
            if matches!(
                inner,
                Expression::ObjectLiteralExpression(_)
                    | Expression::ArrayLiteralExpression(_)
                    | Expression::ClassExpression(_)
            ) || self.is_context_sensitive_argument(argument)
                || self.argument_type_is_not_upstreams(*argument)
            {
                return false;
            }
        }
        for candidate in candidates {
            let mut applicable = match self.check_this_argument(node, candidate, false) {
                Ternary::Related => true,
                Ternary::NotRelated => false,
                Ternary::Unknown => return false,
            };
            for (position, argument) in arguments.iter().enumerate() {
                if !applicable {
                    break;
                }
                let Some(target) = self.signature_type_at_position(candidate, position) else {
                    return false;
                };
                if self.is_error(target) {
                    return false;
                }
                let source = self.check_expression(*argument);
                // This port's relation over a generic source (a homomorphic
                // mapped type over a type parameter, `Boxified<T>` against
                // `concat`'s overloads) is not upstream's; declined, the same
                // refusal as the signature-less callee's.
                if self.mapped_types.get(&source).is_some_and(|info| info.name_type.is_some())
                    || self.head_could_contain_type_variables(source, 3)
                {
                    return false;
                }
                match self.relate_ternary(source, target, Relation::Assignable) {
                    Ternary::Related => {}
                    Ternary::NotRelated => {
                        applicable = false;
                        break;
                    }
                    Ternary::Unknown => return false,
                }
            }
            if applicable {
                return true;
            }
        }
        let Some(last) = candidates.last() else { return false };
        let before = self.diagnostics.len();
        self.check_single_candidate_arguments(node, last);
        if candidates.len() > 1 {
            for (_, diagnostic) in &mut self.diagnostics[before..] {
                diagnostic.message = &messages::NO_OVERLOAD_MATCHES_THIS_CALL;
                diagnostic.args.clear();
            }
        }
        true
    }

    /// `reportCallResolutionErrors`' `candidatesForArgumentError` arm
    /// (`checker.go:9649`) over the verdicts the type road's overload walk
    /// ([`Checker::transcribed_generic_set_walk`]) published for this call:
    /// `isSignatureApplicable` (`checker.go:9256`) re-run with `reportErrors`
    /// against the last rejected candidate, as checked (instantiated when
    /// generic). Its first failing argument is reported; with several
    /// rejected candidates the head is TS2769 (the chain and related
    /// information await `tsr-2zk.22`, as in
    /// [`Checker::check_overload_candidates_arguments`]).
    ///
    /// Each argument's type is the one the walk checked under that
    /// candidate's parameter as contextual type where it re-checked it,
    /// else its cached type; an argument whose type follows the contextual
    /// type but was not re-checked (an object, array or class literal, a
    /// context-sensitive function) stops the walk without a report, as does
    /// an undecidable pair; a failing receiver is reported first
    /// ([`Checker::check_this_argument`]). Answers whether the walk
    /// published a verdict to read, so the
    /// caller's declaration-only fallback does not run on an overload set.
    fn report_overload_argument_failure(&mut self, node: tsr_ast::NodeId) -> bool {
        // `resolveCall` reports from the resolution itself; here the type
        // road's resolution publishes the verdict, so it is demanded first
        // (cached in `node_types`; a no-op when the call was already typed).
        // Inside a declaration with type parameters the demand is declined:
        // this port's eager signature return types re-enter recursive
        // return inference there (TS7024 in
        // `declarationsWithRecursiveInternalTypesProduceUniqueTypeParams`,
        // `tests/original_callable_entry.rs`), the decline
        // `docs/parity/notes/calls-inference.md` §4 records for TS2349.
        if !self.node_types.contains_key(&node) && self.in_generic_declaration(node) {
            return false;
        }
        match self.node_map.get(node) {
            Some(tsr_ast::Node::CallExpression(call)) => {
                self.check_expression(Expression::CallExpression(call));
            }
            Some(tsr_ast::Node::NewExpression(new)) => {
                self.check_expression(Expression::NewExpression(new));
            }
            _ => return false,
        }
        let Some(failure) = self.overload_argument_failures.get(&node).cloned() else {
            return false;
        };
        let Some(last) = failure.last else { return true };
        let arguments = match self.node_map.get(node) {
            Some(tsr_ast::Node::CallExpression(call)) => call.arguments,
            Some(tsr_ast::Node::NewExpression(new)) => new.arguments,
            _ => return true,
        };
        let before = self.diagnostics.len();
        // The `this` arm precedes the arguments; when it fails it is the
        // report and no argument is related.
        let arguments = match self.check_this_argument(node, &last, true) {
            Ternary::Related => arguments,
            Ternary::NotRelated => &[],
            Ternary::Unknown => return true,
        };
        for (position, argument) in arguments.iter().enumerate() {
            let Some(argument_id) = argument.node_id() else { break };
            let Some(target) = self.signature_type_at_position(&last, position) else { break };
            if self.is_error(target) {
                break;
            }
            let checked = failure.checked.get(position).copied().flatten();
            if checked.is_none() && self.argument_type_is_not_upstreams(*argument) {
                break;
            }
            let source = checked.unwrap_or_else(|| self.check_expression(*argument));
            match self.relate_ternary(source, target, Relation::Assignable) {
                Ternary::Related => continue,
                Ternary::Unknown => break,
                Ternary::NotRelated => {}
            }
            let mut inner = *argument;
            while let Expression::ParenthesizedExpression(parenthesized) = inner {
                let Some(expression) = parenthesized.expression else { break };
                inner = expression;
            }
            let literal = matches!(
                inner,
                Expression::ObjectLiteralExpression(_)
                    | Expression::ArrayLiteralExpression(_)
                    | Expression::ClassExpression(_)
            ) || self.is_context_sensitive_argument(argument);
            // This port's relation over a generic source or target (a type
            // parameter, a homomorphic mapped type over one) is not
            // upstream's, nor are the types it assigns inside the generic
            // context (a rest parameter contextually typed by `...args:
            // Args` reads `any[]`): the refusal of
            // [`Checker::check_overload_candidates_arguments`], both sides.
            if (checked.is_none()
                && (literal
                    || self.mapped_types.get(&source).is_some_and(|info| info.name_type.is_some())))
                || self.head_could_contain_type_variables(source, 3)
                || self.head_could_contain_type_variables(target, 3)
                || (literal && self.absent_member_flags_unreadable(source, target))
            {
                break;
            }
            if literal {
                // `checkTypeRelatedToAndOptionallyElaborate` with the
                // argument as expression: `elaborateError` speaks first.
                self.check_excess_properties(target, argument_id);
                if self.diagnostics.len() == before {
                    let span = self.error_span(argument_id);
                    self.report_relation_failure(
                        argument_id,
                        span,
                        Some(argument_id),
                        source,
                        target,
                        Some(
                            &messages::ARGUMENT_OF_TYPE_0_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE_1,
                        ),
                    );
                }
            } else {
                self.report_argument_failure(argument_id, source, target);
            }
            break;
        }
        if failure.count > 1 {
            for (_, diagnostic) in &mut self.diagnostics[before..] {
                diagnostic.message = &messages::NO_OVERLOAD_MATCHES_THIS_CALL;
                diagnostic.args.clear();
            }
        }
        true
    }

    /// Whether a function-like or class declaration enclosing `node` declares
    /// type parameters.
    fn in_generic_declaration(&self, node: tsr_ast::NodeId) -> bool {
        use tsr_ast::Node;
        let mut current = self.nodes.parent(node);
        while let Some(ancestor) = current {
            let generic = match self.node_map.get(ancestor) {
                Some(Node::FunctionDeclaration(n)) => !n.type_parameters.is_empty(),
                Some(Node::FunctionExpression(n)) => !n.type_parameters.is_empty(),
                Some(Node::ArrowFunction(n)) => !n.type_parameters.is_empty(),
                Some(Node::MethodDeclaration(n)) => !n.type_parameters.is_empty(),
                Some(Node::ConstructorDeclaration(n)) => !n.type_parameters.is_empty(),
                Some(Node::GetAccessorDeclaration(n)) => !n.type_parameters.is_empty(),
                Some(Node::SetAccessorDeclaration(n)) => !n.type_parameters.is_empty(),
                Some(Node::ClassDeclaration(n)) => !n.type_parameters.is_empty(),
                Some(Node::ClassExpression(n)) => !n.type_parameters.is_empty(),
                _ => false,
            };
            if generic {
                return true;
            }
            current = self.nodes.parent(ancestor);
        }
        false
    }

    /// Whether `source` lacks a member of `target` whose symbol this port
    /// cannot resolve although it types it (a member inherited through an
    /// instantiated generic base, `interface Q extends P<string>`). The
    /// relater reads such a member's optionality from its symbol
    /// (`property_flags`), so its rejection of the absent member is not
    /// upstream's `propertiesRelatedTo` verdict and is not reported.
    fn absent_member_flags_unreadable(&mut self, source: TypeId, target: TypeId) -> bool {
        let Some(names) = self.get_property_names_of_type(target) else { return false };
        names.iter().any(|name| {
            self.get_type_of_property_of_type(source, name).is_none()
                && self.get_property_of_type(target, name).is_none()
        })
    }

    /// Whether `ty` mentions a literal type within `depth` member levels: a
    /// literal itself, a union/intersection constituent, or a property or
    /// index-signature value. The contextual types under which
    /// `isLiteralOfContextualType` (`checker.go`) can keep an object
    /// literal member's literal type are among these.
    fn type_mentions_literal(&mut self, ty: TypeId, depth: u32) -> bool {
        let flags = self.store.get(ty).flags;
        if flags.intersects(TypeFlags::LITERAL) {
            return true;
        }
        if let TypeData::Union { types, .. } | TypeData::Intersection { types, .. } =
            &self.store.get(ty).data
        {
            let types = types.clone();
            return types.into_iter().any(|part| self.type_mentions_literal(part, depth));
        }
        if depth == 0 || !flags.intersects(TypeFlags::OBJECT) {
            return false;
        }
        if let Some(names) = self.get_property_names_of_type(ty) {
            for name in &names {
                if let Some(member) = self.get_type_of_property_of_type(ty, name)
                    && self.type_mentions_literal(member, depth - 1)
                {
                    return true;
                }
            }
        }
        self.get_index_infos_of_type(ty).is_some_and(|infos| {
            infos.iter().any(|info| self.type_mentions_literal(info.value, depth - 1))
        })
    }

    /// `chooseOverload` (`checker.go:9025`) for a call whose single candidate
    /// is generic: with written type arguments `checkTypeArguments`
    /// (`checker.go:9222`), else `inferTypeArguments`; then
    /// `getSignatureInstantiation` and `isSignatureApplicable`. On failure
    /// `reportCallResolutionErrors` (`checker.go:9649`) reports the
    /// constraint failure (`candidateForTypeArgumentError`, see
    /// [`Checker::check_call_type_argument_constraints`]), the arity of an
    /// instantiated generic rest (`candidateForArgumentArityError`), or
    /// re-runs the applicability check with `reportErrors` against that same
    /// instantiation (`candidatesForArgumentError`'s only entry).
    ///
    /// A call with a context-sensitive argument and no written type
    /// arguments reuses the instantiation the type road published in
    /// `resolved_call_signatures` (`signatureLinks.resolvedSignature`):
    /// re-running inference there would re-assign the callback's contextual
    /// parameter types after its body was checked, which upstream never does
    /// (`assignContextualParameterTypes` is once). Otherwise the
    /// instantiation comes from the shared inference worker
    /// ([`Checker::check_generic_call_with`]'s out-slot, whose written-type-
    /// argument arm substitutes without touching the arguments); no cache is
    /// added. Answers `false` when no instantiation is decided, so the caller
    /// keeps its declaration-only rule.
    fn check_single_generic_candidate_arguments(
        &mut self,
        node: tsr_ast::NodeId,
        candidate: &Signature,
    ) -> bool {
        let Some(tsr_ast::Node::CallExpression(call)) = self.node_map.get(node) else {
            return false;
        };
        if !call.type_arguments.is_empty() {
            match self.check_call_type_argument_constraints(candidate, node) {
                Some(true) => {}
                Some(false) => return true,
                None => return false,
            }
        }
        let instantiated = if call.type_arguments.is_empty()
            && call.arguments.iter().any(|argument| self.is_context_sensitive_argument(argument))
        {
            // checkCallExpression resolves the call (`resolveCall`,
            // assigning the callbacks' contextual parameter types once)
            // before any callback body is checked. The diagnostic walk
            // reaches this rule before the call's children; resolving
            // here keeps a body from typing its parameters through the
            // stateless contextual road, which fills `unknown` for type
            // parameters an earlier argument would have fixed.
            if !self.resolved_call_signatures.contains_key(&node) {
                self.check_call_expression(call);
            }
            match self.resolved_call_signatures.get(&node) {
                Some(resolved) if resolved.type_parameters.is_empty() => resolved.clone(),
                _ => return false,
            }
        } else {
            let mut instantiated = None;
            let answer = self.check_generic_call_with(
                candidate,
                Some(node),
                call.arguments,
                Some(&mut instantiated),
            );
            match instantiated {
                Some(instantiated) if answer != self.intrinsics.error => instantiated,
                _ => return false,
            }
        };
        // A generic rest type can instantiate to another arity
        // (`checker.go:9068`): the instantiation is then
        // `candidateForArgumentArityError`, reported by `getArgumentArityError`
        // over it alone (`reportCallResolutionErrors`, `checker.go:9670`).
        if self.signature_non_array_rest_type(candidate).is_some() {
            let Some(effective) = self.effective_call_arguments(call.arguments) else {
                return false;
            };
            match self.has_correct_arity(&instantiated, &effective, false) {
                Some(true) => {}
                Some(false) => {
                    let error_node = call
                        .expression
                        .and_then(|callee| callee.node_id())
                        .map_or(node, |callee| self.call_error_node(callee));
                    self.report_argument_arity_error(node, error_node, &[instantiated], &effective);
                    return true;
                }
                None => return false,
            }
        }
        if self.signature_non_array_rest_type(&instantiated).is_some() {
            return false;
        }
        match self.check_this_argument(node, &instantiated, true) {
            Ternary::Related => {
                self.check_instantiated_candidate_arguments(call.arguments, &instantiated);
            }
            Ternary::NotRelated => {}
            Ternary::Unknown => return false,
        }
        true
    }

    /// `checkTypeArguments` (`checker.go:9222`) with `reportErrors`, as
    /// `reportCallResolutionErrors`' `candidateForTypeArgumentError` arm
    /// runs it: the written type arguments, `fillMissingTypeArguments`
    /// (defaults instantiated over the prefix, else `unknown`), then each
    /// written argument against its parameter's constraint instantiated by
    /// that mapper; the first failure is reported at the type-argument node
    /// under `Type_0_does_not_satisfy_the_constraint_1` and ends the check.
    ///
    /// `Some(true)`: every constraint holds. `Some(false)`: a failure was
    /// found (and reported where the relation reporter speaks). `None`: a
    /// side this port cannot decide — an unresolved argument, a constraint
    /// or argument mentioning type variables (the refusal
    /// `constraints.rs`' type-reference check makes), an unknown relation.
    /// `getTypeWithThisArgument` is not applied (no `this`-typed
    /// constraint reaches here decided).
    fn check_call_type_argument_constraints(
        &mut self,
        candidate: &Signature,
        call: tsr_ast::NodeId,
    ) -> Option<bool> {
        let Some(tsr_ast::Node::CallExpression(call)) = self.node_map.get(call) else {
            return None;
        };
        self.check_signature_type_arguments(candidate, call.type_arguments, true)
            .map(|arguments| arguments.is_some())
    }

    /// Native checkTypeArguments; arbitrary instantiation/call/query node list.
    pub fn check_signature_type_arguments(
        &mut self,
        candidate: &Signature,
        nodes: &[tsr_ast::TypeNode<'a>],
        report_errors: bool,
    ) -> Option<Option<Vec<TypeId>>> {
        let parameters = self.type_parameter_types(candidate)?;
        if !Self::signature_accepts_type_argument_count(candidate, nodes.len()) {
            return None;
        }
        let names: Vec<&str> =
            candidate.type_parameters.iter().map(|parameter| parameter.name.as_str()).collect();
        // fillMissingTypeArguments preloads unresolved trailing slots with
        // errorType before evaluating defaults against the complete mapper.
        let mut map: Vec<_> =
            parameters.iter().map(|&parameter| (parameter, self.intrinsics.error)).collect();
        for (position, &node) in nodes.iter().enumerate() {
            let argument = self.get_type_from_type_node(node);
            if self.is_error(argument) {
                return None;
            }
            map[position].1 = argument;
        }
        let is_javascript = self.in_js_file(candidate.declaration);
        for position in nodes.len()..parameters.len() {
            let argument = match candidate.type_parameters[position].default {
                Some(default)
                    if is_javascript
                        && (default == self.intrinsics.unknown
                            || default == self.intrinsics.empty_object) =>
                {
                    self.intrinsics.any
                }
                Some(default) => self.instantiate_type(default, &map, &parameters, &names),
                None if is_javascript => self.intrinsics.any,
                None => self.intrinsics.unknown,
            };
            if self.is_error(argument) {
                return None;
            }
            map[position].1 = argument;
        }
        for (position, &node) in nodes.iter().enumerate() {
            let Some(constraint) = self.type_parameter_constraint(parameters[position]) else {
                continue;
            };
            let constraint = self.instantiate_type(constraint, &map, &parameters, &names);
            let source = map[position].1;
            let target = self.get_type_with_this_argument(constraint, source, false);
            if self.is_error(target)
                || self.head_could_contain_type_variables(source, 3)
                || self.head_could_contain_type_variables(target, 3)
            {
                return None;
            }
            match self.relate_ternary(source, target, Relation::Assignable) {
                Ternary::Related => {}
                Ternary::Unknown => return None,
                Ternary::NotRelated => {
                    if report_errors {
                        let at = node.node_id()?;
                        let span = self.error_span(at);
                        self.report_relation_failure(
                            at,
                            span,
                            None,
                            source,
                            target,
                            Some(&messages::TYPE_0_DOES_NOT_SATISFY_THE_CONSTRAINT_1),
                        );
                    }
                    return Some(None);
                }
            }
        }
        Some(Some(map.into_iter().map(|(_, argument)| argument).collect()))
    }

    /// hasCorrectTypeArgumentArity (5b1047d): required prefix through the last
    /// parameter without a default. The instantiation-expression consumer also
    /// requires a nonempty generic parameter list before calling this helper.
    #[must_use]
    pub fn signature_accepts_type_argument_count(signature: &Signature, count: usize) -> bool {
        count == 0
            || (count >= Self::min_type_argument_count(&signature.type_parameters)
                && count <= signature.type_parameters.len())
    }

    /// getInstantiationExpressionType's signature worker (5b1047d:10671).
    /// Filter first, report constraints, retain the original on rejection.
    /// The expression node owns the caller's result cache; argument nodes own
    /// diagnostic spans. No source members/indexes or image cache are copied.
    pub fn get_instantiation_expression_signature(
        &mut self,
        signature: &Signature,
        arguments: &[tsr_ast::TypeNode<'a>],
    ) -> Option<InstantiationExpressionSignature> {
        if signature.type_parameters.is_empty()
            || !Self::signature_accepts_type_argument_count(signature, arguments.len())
        {
            return Some(InstantiationExpressionSignature::Inapplicable);
        }
        let Some(type_arguments) =
            self.check_signature_type_arguments(signature, arguments, true)?
        else {
            return Some(InstantiationExpressionSignature::ConstraintRejected);
        };
        self.get_signature_instantiation(signature, &type_arguments)
            .map(InstantiationExpressionSignature::Instantiated)
    }

    /// checkTypeArguments / getSignatureInstantiation, without object/cache
    /// publication. None is unsupported; Some(None) is a reported rejection,
    /// whose consumer must retain the original signature as native does.
    pub fn instantiate_signature_with_type_arguments(
        &mut self,
        signature: &Signature,
        nodes: &[tsr_ast::TypeNode<'a>],
    ) -> Option<Option<Signature>> {
        let Some(arguments) = self.check_signature_type_arguments(signature, nodes, true)? else {
            return Some(None);
        };
        self.get_signature_instantiation(signature, &arguments).map(Some)
    }

    /// getSignatureInstantiation for the complete ordered vector returned by
    /// `check_signature_type_arguments`. No members, indexes or wrapper identity
    /// are instantiated here; the expression consumer owns those source links.
    pub fn get_signature_instantiation(
        &mut self,
        signature: &Signature,
        type_arguments: &[TypeId],
    ) -> Option<Signature> {
        if let Some(image) =
            self.cached_signatures.get(&signature.id).and_then(|images| images.get(type_arguments))
        {
            return Some(image.clone());
        }
        let parameters = self.type_parameter_types(signature)?;
        if parameters.len() != type_arguments.len() {
            return None;
        }
        let names: Vec<_> =
            signature.type_parameters.iter().map(|parameter| parameter.name.as_str()).collect();
        let map: Vec<_> = parameters.iter().copied().zip(type_arguments.iter().copied()).collect();
        let image =
            self.instantiate_signature_lazily(signature, &map, &parameters, &names, true)?;
        self.cached_signatures
            .entry(signature.id)
            .or_default()
            .insert(type_arguments.to_vec(), image.clone());
        Some(image)
    }

    /// `isSignatureApplicable` (`checker.go:9256`) with `reportErrors` for an
    /// instantiated generic candidate. Each argument is
    /// `checkExpressionWithContextualType(arg, paramType)` (`checker.go:7484`):
    /// its type under the INSTANTIATED parameter as contextual type, which is
    /// not necessarily the type this port cached while the call's signature
    /// was still being inferred.
    ///
    /// An argument whose cached type relates needs nothing more, and one
    /// whose type cannot depend on its contextual type reports from the cached
    /// type. An object literal reports from its cached type unless the target
    /// mentions a literal type (literal preservation follows the instantiated
    /// context). The rest decline: an array literal (tuple-ness follows the
    /// instantiated context), a context-sensitive function
    /// (`assignContextualParameterTypes`, generic contextual signatures) and a
    /// class expression (its class identity is re-created by a re-check).
    fn check_instantiated_candidate_arguments(
        &mut self,
        arguments: &[Expression<'_>],
        signature: &Signature,
    ) {
        for (position, argument) in arguments.iter().enumerate() {
            let Some(argument_id) = argument.node_id() else { return };
            let Some(target) = self.signature_type_at_position(signature, position) else {
                return;
            };
            if self.is_error(target) || self.argument_type_is_not_upstreams(*argument) {
                return;
            }
            let source = self.check_expression(*argument);
            if self.relate_ternary(source, target, Relation::Assignable) == Ternary::Related {
                continue;
            }
            let mut inner = *argument;
            while let Expression::ParenthesizedExpression(parenthesized) = inner {
                let Some(expression) = parenthesized.expression else { return };
                inner = expression;
            }
            // A non-context-sensitive object literal: its cached type is
            // reported through `checkTypeRelatedToAndOptionallyElaborate`
            // (excess property, then `elaborateError` at the member), as the
            // overload reporter does for a literal the walk re-checked.
            // Upstream checks it under the instantiated parameter, whose
            // literal members keep the literal's own literal types
            // (`getWidenedLiteralLikeTypeForContextualType`); the cached type
            // was widened under another context, so a target mentioning a
            // literal type declines (a superset of the members that differ).
            if matches!(inner, Expression::ObjectLiteralExpression(_))
                && !self.is_context_sensitive_argument(argument)
            {
                if self.relate_ternary(source, target, Relation::Assignable) != Ternary::NotRelated
                    || self.head_could_contain_type_variables(source, 3)
                    || self.head_could_contain_type_variables(target, 3)
                    || self.type_mentions_literal(target, 3)
                    || self.absent_member_flags_unreadable(source, target)
                {
                    return;
                }
                let before = self.diagnostics.len();
                self.check_excess_properties(target, argument_id);
                if self.diagnostics.len() == before {
                    let span = self.error_span(argument_id);
                    self.report_relation_failure(
                        argument_id,
                        span,
                        Some(argument_id),
                        source,
                        target,
                        Some(
                            &messages::ARGUMENT_OF_TYPE_0_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE_1,
                        ),
                    );
                }
                return;
            }
            if matches!(
                inner,
                Expression::ObjectLiteralExpression(_)
                    | Expression::ArrayLiteralExpression(_)
                    | Expression::ClassExpression(_)
            ) || self.is_context_sensitive_argument(argument)
                || self.mapped_types.get(&source).is_some_and(|info| info.name_type.is_some())
            {
                return;
            }
            if self.report_argument_failure(argument_id, source, target) {
                return;
            }
        }
    }

    /// Argument shapes whose type this port computes without a mechanism
    /// upstream applies, so a failed relation is not upstream's answer:
    ///
    /// - `a ?? b`, `a || b` and `c ? a : b` union their operands with
    ///   `UnionReductionSubtype`, which this port does not have;
    /// - an identifier naming an auto-typed `let x = []` array, whose
    ///   evolved element types upstream regularizes
    ///   (`getRegularTypeOfObjectLiteral` in `addEvolvingArrayElementType`)
    ///   and this port leaves fresh.
    fn argument_type_is_not_upstreams(&self, argument: Expression<'_>) -> bool {
        let mut argument = argument;
        while let Expression::ParenthesizedExpression(inner) = argument {
            let Some(expression) = inner.expression else { return true };
            argument = expression;
        }
        match argument {
            Expression::ConditionalExpression(_) => true,
            Expression::BinaryExpression(binary) => binary.operator_token.is_some_and(|token| {
                matches!(
                    token.kind,
                    tsr_ast::SyntaxKind::QuestionQuestionToken | tsr_ast::SyntaxKind::BarBarToken
                )
            }),
            Expression::Identifier(identifier) => {
                let Some(id) = identifier.node_id else { return false };
                let Some(symbol) = self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    id,
                    identifier.text,
                    SymbolFlags::VALUE,
                ) else {
                    return false;
                };
                let Some(declaration) = self.binder.symbols().get(symbol).value_declaration else {
                    return false;
                };
                matches!(self.node_map.get(declaration),
                    Some(tsr_ast::Node::VariableDeclaration(variable))
                        if variable.r#type.is_none()
                            && matches!(variable.initializer,
                                Some(Expression::ArrayLiteralExpression(array)) if array.elements.is_empty()))
            }
            _ => false,
        }
    }

    /// The type-argument half of [`Checker::check_resolve_call_arity`] alone.
    fn check_type_argument_arity_only(
        &mut self,
        node: tsr_ast::NodeId,
        type_arguments: &[tsr_ast::TypeNode<'_>],
        signatures: &[Signature],
    ) -> CallArity {
        let count = type_arguments.len();
        let arities: Vec<(usize, usize)> = signatures
            .iter()
            .map(|signature| {
                (
                    Self::min_type_argument_count(&signature.type_parameters),
                    signature.type_parameters.len(),
                )
            })
            .collect();
        if arities.iter().any(|&(min, max)| count >= min && count <= max) {
            return CallArity::Undecided;
        }
        self.report_type_argument_arity_error(node, type_arguments, &arities);
        CallArity::Reported
    }

    /// `getEffectiveCallArguments` (`checker.go:30042`) for a call or `new`:
    /// a spread of a tuple type becomes one synthetic argument per element,
    /// a rest/variadic element a spread one. `None` when a spread's type is
    /// not settled.
    fn effective_call_arguments(
        &mut self,
        arguments: &[Expression<'_>],
    ) -> Option<Vec<EffectiveArgument>> {
        let mut effective = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let node = argument.node_id()?;
            let Expression::SpreadElement(spread) = argument else {
                effective.push(EffectiveArgument { node, spread: false });
                continue;
            };
            let spread_type = self.check_expression(spread.expression?);
            if self.is_error(spread_type) {
                return None;
            }
            if let Some((elements, _)) = self.tuple_element_lists.get(&spread_type) {
                let n = elements.len();
                effective.extend((0..n).map(|_| EffectiveArgument { node, spread: false }));
            } else if let Some((elements, _)) = self.variadic_tuple_elements.get(&spread_type) {
                let flags: Vec<bool> = elements.iter().map(|element| element.spread).collect();
                effective
                    .extend(flags.into_iter().map(|spread| EffectiveArgument { node, spread }));
            } else {
                effective.push(EffectiveArgument { node, spread: true });
            }
        }
        Some(effective)
    }

    /// `new C` without an argument list (`node.ArgumentList() == nil`): the
    /// node ends where its callee, or its type-argument list's `>`, ends.
    fn new_has_no_argument_list(
        &self,
        node: tsr_ast::NodeId,
        callee: Option<Expression<'_>>,
        type_arguments: &[tsr_ast::TypeNode<'_>],
    ) -> bool {
        let end = self.nodes.span(node).end;
        if let Some(last) = type_arguments.last().and_then(tsr_ast::TypeNode::node_id) {
            return end <= self.nodes.span(last).end + 1;
        }
        callee
            .and_then(|callee| callee.node_id())
            .is_some_and(|callee| end == self.nodes.span(callee).end)
    }

    /// `hasCorrectArity` (`checker.go:9107`) for a call or `new` with a
    /// complete argument list. `None` when a parameter type is unsupported.
    fn has_correct_arity(
        &mut self,
        signature: &Signature,
        arguments: &[EffectiveArgument],
        no_argument_list: bool,
    ) -> Option<bool> {
        let minimum = self.signature_min_argument_count(signature);
        if no_argument_list {
            return Some(minimum == 0);
        }
        let parameter_count = self.signature_parameter_count(signature);
        let has_rest = self.signature_has_effective_rest(signature);
        if let Some(spread_index) = arguments.iter().position(|argument| argument.spread) {
            return Some(spread_index >= minimum && (has_rest || spread_index < parameter_count));
        }
        let count = arguments.len();
        if !has_rest && count > parameter_count {
            return Some(false);
        }
        if count >= minimum {
            return Some(true);
        }
        for position in count..minimum {
            // `getTypeAtPosition` answers `any` for a missing position.
            let Some(t) = self.signature_type_at_position(signature, position) else {
                return Some(false);
            };
            if self.is_error(t) {
                return None;
            }
            // `filterType(t, acceptsVoid)` is `never` unless a constituent is
            // `void`.
            let accepts_void = match &self.store.get(t).data {
                TypeData::Union { types, .. } => {
                    types.iter().any(|&part| self.store.get(part).flags.contains(TypeFlags::VOID))
                }
                _ => self.store.get(t).flags.contains(TypeFlags::VOID),
            };
            if !accepts_void {
                return Some(false);
            }
        }
        Some(true)
    }

    /// `getArgumentArityError` (`checker.go:9705`), without the related
    /// information and the decorator messages.
    fn report_argument_arity_error(
        &mut self,
        node: tsr_ast::NodeId,
        error_node: tsr_ast::NodeId,
        signatures: &[Signature],
        arguments: &[EffectiveArgument],
    ) {
        if let Some(spread) = arguments.iter().find(|argument| argument.spread) {
            let span = self.error_span(spread.node);
            self.report_at_node(
                node,
                Diagnostic::new(
                    &messages::A_SPREAD_ARGUMENT_MUST_EITHER_HAVE_A_TUPLE_TYPE_OR_BE_PASSED_TO_A_REST_PARAMETER,
                    span,
                ),
            );
            return;
        }
        let count = arguments.len();
        let mut min_count = usize::MAX;
        let mut max_count = 0usize;
        let mut max_below: Option<usize> = None;
        let mut min_above: Option<usize> = None;
        for signature in signatures {
            let min_parameter = self.signature_min_argument_count(signature);
            let max_parameter = self.signature_parameter_count(signature);
            min_count = min_count.min(min_parameter);
            max_count = max_count.max(max_parameter);
            if min_parameter < count && max_below.is_none_or(|below| min_parameter > below) {
                max_below = Some(min_parameter);
            }
            if count < max_parameter && min_above.is_none_or(|above| max_parameter < above) {
                min_above = Some(max_parameter);
            }
        }
        let has_rest =
            signatures.iter().any(|signature| self.signature_has_effective_rest(signature));
        let range = if !has_rest && min_count < max_count {
            format!("{min_count}-{max_count}")
        } else {
            min_count.to_string()
        };
        let void_promise =
            !has_rest && range == "1" && count == 0 && self.is_promise_resolve_arity_error(node);
        let message = if has_rest {
            &messages::EXPECTED_AT_LEAST_0_ARGUMENTS_BUT_GOT_1
        } else if void_promise {
            &messages::EXPECTED_0_ARGUMENTS_BUT_GOT_1_DID_YOU_FORGET_TO_INCLUDE_VOID_IN_YOUR_TYPE_ARGUMENT_TO_PROMISE
        } else {
            &messages::EXPECTED_0_ARGUMENTS_BUT_GOT_1
        };
        let diagnostic = if min_count < count && count < max_count {
            Diagnostic::with_args(
                &messages::NO_OVERLOAD_EXPECTS_0_ARGUMENTS_BUT_OVERLOADS_DO_EXIST_THAT_EXPECT_EITHER_1_OR_2_ARGUMENTS,
                self.error_span(error_node),
                [
                    count.to_string(),
                    max_below.map_or_else(String::new, |n| n.to_string()),
                    min_above.map_or_else(String::new, |n| n.to_string()),
                ],
            )
        } else if count < min_count || max_count >= count {
            Diagnostic::with_args(message, self.error_span(error_node), [range, count.to_string()])
        } else {
            let start = self.nodes.span(arguments[max_count].node).start;
            let end = self.nodes.span(arguments[count - 1].node).end.max(start);
            Diagnostic::with_args(
                message,
                tsr_core::Span { start, end },
                [range, count.to_string()],
            )
        };
        self.report_at_node(node, diagnostic);
    }

    /// `isPromiseResolveArityError` (`checker.go`): the callee is a parameter
    /// of a function passed to `new Promise(...)`.
    fn is_promise_resolve_arity_error(&mut self, node: tsr_ast::NodeId) -> bool {
        let Some(tsr_ast::Node::CallExpression(call)) = self.node_map.get(node) else {
            return false;
        };
        let Some(Expression::Identifier(callee)) = call.expression else { return false };
        let Some(callee_id) = callee.node_id else { return false };
        let Some(symbol) = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            callee_id,
            callee.text,
            SymbolFlags::VALUE,
        ) else {
            return false;
        };
        let Some(declaration) = self.binder.symbols().get(symbol).value_declaration else {
            return false;
        };
        if self.nodes.kind(declaration) != tsr_ast::SyntaxKind::Parameter {
            return false;
        }
        let Some(function) = self.nodes.parent(declaration) else { return false };
        if !matches!(
            self.nodes.kind(function),
            tsr_ast::SyntaxKind::FunctionExpression | tsr_ast::SyntaxKind::ArrowFunction
        ) {
            return false;
        }
        let Some(new) = self.nodes.parent(function) else { return false };
        let Some(tsr_ast::Node::NewExpression(new)) = self.node_map.get(new) else { return false };
        let Some(Expression::Identifier(constructor)) = new.expression else { return false };
        let Some(constructor_id) = constructor.node_id else { return false };
        let Some(global) = self.binder.globals().get("Promise").copied() else { return false };
        self.binder
            .resolve_name(
                self.nodes,
                self.node_map,
                constructor_id,
                constructor.text,
                SymbolFlags::VALUE,
            )
            .is_some_and(|resolved| {
                self.binder.merged_symbol(resolved) == self.binder.merged_symbol(global)
            })
    }

    /// `getTypeArgumentArityError` (`checker.go:9853`), the span over the
    /// type-argument list.
    fn report_type_argument_arity_error(
        &mut self,
        node: tsr_ast::NodeId,
        type_arguments: &[tsr_ast::TypeNode<'_>],
        arities: &[(usize, usize)],
    ) {
        let (Some(first), Some(last)) = (
            type_arguments.first().and_then(tsr_ast::TypeNode::node_id),
            type_arguments.last().and_then(tsr_ast::TypeNode::node_id),
        ) else {
            return;
        };
        let count = type_arguments.len();
        let span =
            tsr_core::Span { start: self.nodes.span(first).start, end: self.nodes.span(last).end };
        let diagnostic = if let [(min, max)] = arities {
            let expected = if min < max { format!("{min}-{max}") } else { min.to_string() };
            Diagnostic::with_args(
                &messages::EXPECTED_0_TYPE_ARGUMENTS_BUT_GOT_1,
                span,
                [expected, count.to_string()],
            )
        } else {
            let mut below: Option<usize> = None;
            let mut above: Option<usize> = None;
            for &(min, max) in arities {
                if min > count {
                    above = Some(above.map_or(min, |above| above.min(min)));
                } else if max < count {
                    below = Some(below.map_or(max, |below| below.max(max)));
                }
            }
            match (below, above) {
                (Some(below), Some(above)) => Diagnostic::with_args(
                    &messages::NO_OVERLOAD_EXPECTS_0_TYPE_ARGUMENTS_BUT_OVERLOADS_DO_EXIST_THAT_EXPECT_EITHER_1_OR_2_TYPE_ARGUMENTS,
                    span,
                    [count.to_string(), below.to_string(), above.to_string()],
                ),
                (Some(expected), None) | (None, Some(expected)) => Diagnostic::with_args(
                    &messages::EXPECTED_0_TYPE_ARGUMENTS_BUT_GOT_1,
                    span,
                    [expected.to_string(), count.to_string()],
                ),
                (None, None) => return,
            }
        };
        self.report_at_node(node, diagnostic);
    }

    /// `getMinTypeArgumentCount` (`checker.go`): one past the last type
    /// parameter without a default.
    fn min_type_argument_count(type_parameters: &[crate::signatures::TypeParameter]) -> usize {
        type_parameters
            .iter()
            .rposition(|parameter| parameter.default.is_none())
            .map_or(0, |i| i + 1)
    }

    /// Every diagnostic `resolveNewExpression` (`checker.go:8575`) issues for
    /// one `new` node, except constructor accessibility and abstractness,
    /// which [`Checker::check_new_on_abstract_class`] owns.
    ///
    /// The callee goes through `checkNonNullExpression` (the possibly-null
    /// reporter is not emitted, for the narrowing reason
    /// [`Checker::check_non_null_callee`] records); an `any` apparent type is
    /// an untyped call (TS2347 for written type arguments); construct
    /// signatures resolve, else call signatures resolve and, without
    /// `noImplicitAny`, a non-`void` return is TS2350; else
    /// `invocationError(Construct)` is TS2351 on the callee.
    pub(crate) fn check_new_expression_diagnostics(&mut self, node: tsr_ast::NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        match self.check_new_expression_head(node) {
            CallHead::Done => {}
            CallHead::Resolve(apparent) => {
                let kind = if self
                    .head_signature_count(apparent, SignatureKind::Construct)
                    .is_some_and(|count| count != 0)
                {
                    SignatureKind::Construct
                } else {
                    SignatureKind::Call
                };
                match self.check_resolve_call_arity(node, apparent, kind) {
                    CallArity::Reported => {}
                    CallArity::Applicable(Some(signature)) => {
                        self.check_single_candidate_arguments(node, &signature);
                    }
                    CallArity::Applicable(None)
                    | CallArity::ApplicableGeneric(_)
                    | CallArity::ApplicableOverloads(_) => {
                        if !self.report_overload_argument_failure(node) {
                            self.check_new_arity(node, false);
                        }
                    }
                    CallArity::Undecided => {
                        self.check_new_arity(node, true);
                        self.check_call_type_argument_arity(node);
                    }
                }
            }
            CallHead::Unknown => {
                self.check_new_arity(node, true);
                self.check_call_type_argument_arity(node);
            }
        }
    }

    /// The head of `resolveTaggedTemplateExpression` (`checker.go:8719`): an
    /// untyped tag reports nothing; a tag with no call signatures is
    /// `invocationError` (TS2349) on the tag, or TS2796 when the tagged
    /// template is an array element (a likely missing comma).
    pub(crate) fn check_tagged_template_diagnostics(&mut self, node: tsr_ast::NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(tsr_ast::Node::TaggedTemplateExpression(tagged)) = self.node_map.get(node) else {
            return;
        };
        let Some(tag) = tagged.tag else { return };
        let Some(tag_id) = tag.node_id() else { return };
        let tag_type = self.check_expression(tag);
        let apparent = self.apparent_type(tag_type);
        if self.is_error(apparent)
            || self.is_untyped_any_callee(tag_type, apparent)
            || self.is_function_or_method_type(apparent)
        {
            return;
        }
        let (Some(call_count), Some(construct_count)) = (
            self.head_signature_count(apparent, SignatureKind::Call),
            self.head_signature_count(apparent, SignatureKind::Construct),
        ) else {
            return;
        };
        if call_count != 0
            || self.is_untyped_signatureless_call(tag_type, apparent, call_count, construct_count)
                != Some(false)
            || self.head_could_contain_type_variables(tag_type, 3)
        {
            return;
        }
        if self.nodes.parent(node).is_some_and(|parent| {
            self.nodes.kind(parent) == tsr_ast::SyntaxKind::ArrayLiteralExpression
        }) {
            self.report_at_node(
                tag_id,
                Diagnostic::new(
                    &messages::IT_IS_LIKELY_THAT_YOU_ARE_MISSING_A_COMMA_TO_SEPARATE_THESE_TWO_TEMPLATE_EXPRESSIONS_THEY_FORM_A_TAGGED_TEMPLATE_EXPRESSION_WHICH_CANNOT_BE_INVOKED,
                    self.error_span(tag_id),
                ),
            );
            return;
        }
        self.invocation_error(tag_id, false, SignatureKind::Call);
    }

    fn check_new_expression_head(&mut self, node: tsr_ast::NodeId) -> CallHead {
        let Some(tsr_ast::Node::NewExpression(new)) = self.node_map.get(node) else {
            return CallHead::Unknown;
        };
        let Some(callee) = new.expression else { return CallHead::Unknown };
        let Some(callee_id) = callee.node_id() else { return CallHead::Unknown };
        let expression_type = self.check_expression(callee);
        let expression_type = match self.check_non_null_callee(expression_type, callee_id) {
            NonNullCallee::Type(t) => t,
            NonNullCallee::Error => return CallHead::Done,
            NonNullCallee::Unknown => return CallHead::Unknown,
        };
        if Some(expression_type) == self.silent_never_type {
            return CallHead::Done;
        }
        let apparent = self.apparent_type(expression_type);
        if self.is_error(apparent) {
            return CallHead::Done;
        }
        if self.store.get(apparent).flags.intersects(TypeFlags::ANY) {
            if !self.untyped_any_is_written(callee) {
                return CallHead::Unknown;
            }
            if !new.type_arguments.is_empty() {
                self.report_at_node(
                    node,
                    Diagnostic::new(
                        &messages::UNTYPED_FUNCTION_CALLS_MAY_NOT_ACCEPT_TYPE_ARGUMENTS,
                        self.error_span(node),
                    ),
                );
            }
            return CallHead::Done;
        }
        let Some(construct_count) = self.head_signature_count(apparent, SignatureKind::Construct)
        else {
            return CallHead::Unknown;
        };
        if construct_count != 0 {
            return CallHead::Resolve(apparent);
        }
        let Some(call_signatures) = self.head_signatures(apparent, SignatureKind::Call) else {
            return CallHead::Unknown;
        };
        if self.head_could_contain_type_variables(expression_type, 3) {
            return CallHead::Unknown;
        }
        match call_signatures.as_slice() {
            [] => {
                self.invocation_error(
                    callee_id,
                    new.arguments.is_empty(),
                    SignatureKind::Construct,
                );
                CallHead::Done
            }
            // `resolveCall` picks the sole non-generic candidate whatever the
            // arguments; an overload set or a generic one needs the chosen,
            // instantiated signature, which this head does not compute.
            [signature] if signature.type_parameters.is_empty() => {
                if !self.no_implicit_any
                    && let Some(signature) = self.complete_signature_return(signature.clone())
                    && signature.r#type != self.intrinsics.void
                    && !self.is_error(signature.r#type)
                {
                    self.report_at_node(
                        node,
                        Diagnostic::new(
                            &messages::ONLY_A_VOID_FUNCTION_CAN_BE_CALLED_WITH_THE_NEW_KEYWORD,
                            self.error_span(node),
                        ),
                    );
                }
                CallHead::Resolve(apparent)
            }
            _ => CallHead::Resolve(apparent),
        }
    }

    /// `checkNonNullTypeWithReporter` (`checker.go:7413`) with
    /// `reportCannotInvokePossiblyNullOrUndefinedError` as the reporter.
    fn check_non_null_callee(&mut self, t: TypeId, callee: tsr_ast::NodeId) -> NonNullCallee {
        if self.strict_null_checks && self.store.get(t).flags.intersects(TypeFlags::UNKNOWN) {
            let text = self.entity_name_expression_text(callee).filter(|text| text.len() < 100);
            let span = self.error_span(callee);
            let diagnostic = match text {
                Some(text) => Diagnostic::with_args(&messages::_0_IS_OF_TYPE_UNKNOWN, span, [text]),
                None => Diagnostic::new(&messages::OBJECT_IS_OF_TYPE_UNKNOWN, span),
            };
            self.report_at_node(callee, diagnostic);
            return NonNullCallee::Error;
        }
        // `IsUndefined`/`IsNull` facts come only from `undefined`, `null` and
        // `void` themselves, or through a union, intersection or constraint;
        // an object, function or other primitive answers neither. Skipping
        // `getTypeFacts` there avoids resolving a function type's signatures
        // (with their return types) from inside its own body.
        if !self.store.get(t).flags.intersects(
            TypeFlags::NULLABLE
                | TypeFlags::VOID
                | TypeFlags::UNION
                | TypeFlags::INTERSECTION
                | TypeFlags::INSTANTIABLE,
        ) {
            return NonNullCallee::Type(t);
        }
        let facts = self.get_type_facts(t)
            & (crate::flow::TypeFacts::IS_UNDEFINED | crate::flow::TypeFacts::IS_NULL);
        if facts.is_empty() {
            return NonNullCallee::Type(t);
        }
        let non_nullable = self.get_non_nullable_type(t);
        let remainder_nullable =
            self.store.get(non_nullable).flags.intersects(TypeFlags::NULLABLE | TypeFlags::NEVER);
        // Without `strictNullChecks` `addTypeToUnion` never adds a nullable
        // member beside a non-nullable one, so such a union is not upstream's
        // type and the head declines rather than report on it.
        if !self.strict_null_checks && !remainder_nullable {
            return NonNullCallee::Unknown;
        }
        // `reportCannotInvokePossiblyNullOrUndefinedError` (TS2721-TS2723) is
        // not emitted yet: it trusts the callee's narrowed type, and this
        // port's narrowing of `super.m && super.m()` and of a discriminated
        // `opts.a || opts.f()` still answers the declared nullable type.
        if remainder_nullable { NonNullCallee::Error } else { NonNullCallee::Type(non_nullable) }
    }

    /// `this.m(...)` inside an object-literal method or function-valued
    /// property. A refusal: upstream's `getExplicitThisType` (`flow.go:2215`)
    /// answers nil there, so a call in the body never reaches the method's
    /// own type through `getEffectsSignature`; this port's dotted-name walk
    /// reads `this` through `check_this_expression` instead, so typing the
    /// callee from the diagnostic walk re-enters the method's return
    /// inference and reports TS7023 (`thisTypeInObjectLiterals2`).
    fn callee_reads_object_literal_this(&self, callee: Expression<'_>) -> bool {
        let Expression::PropertyAccessExpression(access) = callee else { return false };
        let Some(Expression::KeywordExpression(receiver)) = access.expression else {
            return false;
        };
        if receiver.kind != tsr_ast::SyntaxKind::ThisKeyword {
            return false;
        }
        let Some(receiver) = receiver.node_id else { return false };
        let Some(container) = self.get_this_container(receiver, false) else { return false };
        let Some(parent) = self.nodes.parent(container) else { return false };
        match self.nodes.kind(parent) {
            tsr_ast::SyntaxKind::ObjectLiteralExpression => true,
            tsr_ast::SyntaxKind::PropertyAssignment => self.nodes.parent(parent).is_some_and(|p| {
                self.nodes.kind(p) == tsr_ast::SyntaxKind::ObjectLiteralExpression
            }),
            _ => false,
        }
    }

    /// Whether `t` is a function or method declaration's own type.
    fn is_function_or_method_type(&self, t: TypeId) -> bool {
        let TypeData::Anonymous { symbol, .. } = self.store.get(t).data else { return false };
        let flags = self.binder.symbols().get(self.binder.merged_symbol(symbol)).flags;
        flags.intersects(SymbolFlags::FUNCTION | SymbolFlags::METHOD)
            && !flags.intersects(SymbolFlags::CLASS)
    }

    /// `getSignaturesOfType` (`checker.go`) as `resolveCallExpression` reads
    /// it: the list only. Upstream resolves no return type here, so a baked
    /// list is read without `complete_signature_return` — completing it from
    /// the diagnostic walk re-enters a method's return inference from a call
    /// in its own body. Every other shape goes through the shared resolver.
    pub(crate) fn head_signatures(
        &mut self,
        t: TypeId,
        kind: SignatureKind,
    ) -> Option<Vec<Signature>> {
        let t = self.apparent_type(t);
        let is_call = kind == SignatureKind::Call;
        if let Some(signatures) = self.signature_types.get(&t) {
            return Some(
                signatures
                    .iter()
                    .filter(|signature| (signature.kind == SignatureKind::Call) == is_call)
                    .cloned()
                    .collect(),
            );
        }
        // A class merged with a function declares call signatures through the
        // function; the shared resolver's class arm answers an empty call list
        // for it, which is not upstream's list, so the head declines.
        if is_call
            && let TypeData::Anonymous { symbol, .. } = self.store.get(t).data
            && self
                .binder
                .symbols()
                .get(self.binder.merged_symbol(symbol))
                .flags
                .contains(SymbolFlags::CLASS | SymbolFlags::FUNCTION)
        {
            return None;
        }
        self.signatures_of_type_kind(t, kind)
    }

    /// getSignaturesOfType for instantiation-expression filtering. Never uses
    /// the semantic resolver that completes all returns before arity filtering.
    /// Parent resolves structured members before entering this list consumer.
    pub(crate) fn get_instantiation_expression_signatures(
        &mut self,
        t: TypeId,
        kind: SignatureKind,
    ) -> Option<Vec<Signature>> {
        let t = self.apparent_type(t);
        let is_call = kind == SignatureKind::Call;
        if let Some(signatures) = self.signature_types.get(&t) {
            return Some(
                signatures
                    .iter()
                    .filter(|signature| (signature.kind == SignatureKind::Call) == is_call)
                    .cloned()
                    .collect(),
            );
        }
        if t == self.intrinsics.empty_object
            || t == self.intrinsics.unknown_empty_object
            || self.store.get(t).flags.contains(TypeFlags::NON_PRIMITIVE)
        {
            return Some(Vec::new());
        }
        let TypeData::Anonymous { symbol, .. } = self.store.get(t).data else {
            // Named inheritance/composite construction needs the same pending
            // shape contract, not an eager resolver or invented empty list.
            return None;
        };
        let symbol = self.binder.merged_symbol(symbol);
        let flags = self.binder.symbols().get(symbol).flags;
        if flags.contains(SymbolFlags::CLASS) {
            if is_call {
                return (!flags.contains(SymbolFlags::FUNCTION)).then(Vec::new);
            }
            return self.get_class_construct_signatures(symbol);
        }
        let declarations = self.binder.symbols().get(symbol).declarations.clone();
        if declarations.iter().any(|&declaration| {
            matches!(
                self.nodes.kind(declaration),
                tsr_ast::SyntaxKind::TypeLiteral | tsr_ast::SyntaxKind::InterfaceDeclaration
            )
        }) {
            return None;
        }
        for declaration in declarations {
            if !matches!(
                self.nodes.kind(declaration),
                tsr_ast::SyntaxKind::FunctionDeclaration
                    | tsr_ast::SyntaxKind::FunctionExpression
                    | tsr_ast::SyntaxKind::ArrowFunction
                    | tsr_ast::SyntaxKind::MethodDeclaration
                    | tsr_ast::SyntaxKind::FunctionType
                    | tsr_ast::SyntaxKind::ConstructorType
                    | tsr_ast::SyntaxKind::CallSignature
                    | tsr_ast::SyntaxKind::ConstructSignature
                    | tsr_ast::SyntaxKind::MethodSignature
            ) {
                continue;
            }
            let key = self.type_literal_key(declaration);
            if !self.signature_returns.contains_key(&key) {
                self.pending_signature_returns
                    .entry(key)
                    .or_insert(crate::signatures::LazyReturnState::Pending);
            }
        }
        let signatures = self.get_signatures_of_symbol_for_type(symbol)?;
        Some(
            signatures
                .into_iter()
                .filter(|signature| (signature.kind == SignatureKind::Call) == is_call)
                .collect(),
        )
    }

    fn head_signature_count(&mut self, t: TypeId, kind: SignatureKind) -> Option<usize> {
        self.head_signatures(t, kind).map(|signatures| signatures.len())
    }

    /// The type-variable half of `couldContainTypeVariables` (`checker.go`),
    /// bounded to `depth` levels of reference arguments and signature
    /// parameters/returns.
    ///
    /// A refusal, not an upstream branch: upstream reports a signature-less
    /// generic callee like any other, but this port's generic machinery
    /// (non-nullable filtering of a deferred conditional, homomorphic mapped
    /// apparent types) answers a type upstream does not hold for several such
    /// callees, and an empty list read off one of those is not "not callable".
    fn head_could_contain_type_variables(&mut self, t: TypeId, depth: u8) -> bool {
        let ty = self.store.get(t);
        if ty.flags.intersects(TypeFlags::INSTANTIABLE) {
            return true;
        }
        if let TypeData::Union { types, .. } | TypeData::Intersection { types, .. } = &ty.data {
            let types = types.clone();
            return types
                .into_iter()
                .any(|member| self.head_could_contain_type_variables(member, depth));
        }
        if depth == 0 {
            return false;
        }
        if let Some((_, arguments)) = self.type_reference_targets.get(&t).cloned()
            && arguments
                .into_iter()
                .any(|argument| self.head_could_contain_type_variables(argument, depth - 1))
        {
            return true;
        }
        if let Some(signatures) = self.signature_types.get(&t).cloned() {
            return signatures.iter().any(|signature| {
                !signature.type_parameters.is_empty()
                    || signature.parameters.iter().any(|parameter| {
                        let parameter_type = self.parameter_type(parameter);
                        self.head_could_contain_type_variables(parameter_type, depth - 1)
                    })
                    || self.head_could_contain_type_variables(signature.r#type, depth - 1)
            });
        }
        false
    }

    /// `isUntypedFunctionCall` (`checker.go:9933`), the arms that read no
    /// signature list: an `any` callee, or a type parameter whose apparent
    /// type is `any`.
    fn is_untyped_any_callee(&self, func_type: TypeId, apparent: TypeId) -> bool {
        let any = |t: TypeId| self.store.get(t).flags.intersects(TypeFlags::ANY);
        any(func_type)
            || any(apparent)
                && self.store.get(func_type).flags.intersects(TypeFlags::TYPE_PARAMETER)
    }

    /// `isUntypedFunctionCall` (`checker.go:9933`), the signature-less arm: no
    /// call or construct signatures, not a union, not reducing to `never`, and assignable
    /// to the global `Function`. `None` when that relation is undecidable.
    fn is_untyped_signatureless_call(
        &mut self,
        func_type: TypeId,
        apparent: TypeId,
        call_count: usize,
        construct_count: usize,
    ) -> Option<bool> {
        // `getReducedType(apparentFuncType).flags&TypeFlagsNever`: an
        // intersection with a never-reduced discriminant is `never` here.
        if call_count != 0
            || construct_count != 0
            || self.store.get(apparent).flags.intersects(TypeFlags::UNION | TypeFlags::NEVER)
            || self.intersection_has_never_discriminant(apparent)
        {
            return Some(false);
        }
        let function = self.global_type_symbol_with_arity("Function", 0)?;
        let function = self.get_declared_type_of_symbol(function);
        if self.is_error(function) {
            return None;
        }
        match self.relate_ternary(func_type, function, Relation::Assignable) {
            Ternary::Related => Some(true),
            Ternary::NotRelated => Some(false),
            Ternary::Unknown => None,
        }
    }

    /// `invocationError` (`checker.go:9996`) → `invocationErrorDetails`: the
    /// head message on `target` (a property access's name). Only the head is
    /// emitted; the detail chain is not modelled by this port's `Diagnostic`.
    pub(crate) fn invocation_error(
        &mut self,
        error_target: tsr_ast::NodeId,
        zero_arguments: bool,
        kind: SignatureKind,
    ) {
        let is_call = kind == SignatureKind::Call;
        let parent_is_call = self
            .nodes
            .parent(error_target)
            .is_some_and(|parent| self.nodes.kind(parent) == tsr_ast::SyntaxKind::CallExpression);
        let target = match self.node_map.get(error_target) {
            Some(tsr_ast::Node::PropertyAccessExpression(access)) if parent_is_call => {
                access.name.and_then(|name| name.node_id()).unwrap_or(error_target)
            }
            _ => error_target,
        };
        let mut head = if is_call {
            &messages::THIS_EXPRESSION_IS_NOT_CALLABLE
        } else {
            &messages::THIS_EXPRESSION_IS_NOT_CONSTRUCTABLE
        };
        // Diagnose get accessors incorrectly called as functions.
        if parent_is_call && zero_arguments {
            match self.resolved_symbol_is_get_accessor(error_target) {
                Some(true) => {
                    head = &messages::THIS_EXPRESSION_IS_NOT_CALLABLE_BECAUSE_IT_IS_A_GET_ACCESSOR_DID_YOU_MEAN_TO_USE_IT_WITHOUT;
                }
                Some(false) => {}
                None => return,
            }
        }
        self.report_at_node(target, Diagnostic::new(head, self.error_span(target)));
    }

    /// `getResolvedSymbolOrNil(errorTarget).Flags & SymbolFlagsGetAccessor`
    /// for the callee shapes whose resolved symbol is a member: a property
    /// access resolves its name on the receiver's apparent type. `None` when
    /// this port cannot find the member upstream resolved (an accessor
    /// inherited through a generic base is one), so the head cannot choose
    /// between TS2349 and TS6234.
    fn resolved_symbol_is_get_accessor(&mut self, callee: tsr_ast::NodeId) -> Option<bool> {
        let Some(tsr_ast::Node::PropertyAccessExpression(access)) = self.node_map.get(callee)
        else {
            return Some(false);
        };
        let (Some(receiver), Some(tsr_ast::MemberName::Identifier(name))) =
            (access.expression, access.name)
        else {
            return Some(false);
        };
        let receiver = self.check_expression(receiver);
        let receiver = self.apparent_type(receiver);
        let symbol = self.get_property_of_type(receiver, name.text)?;
        Some(self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::GET_ACCESSOR))
    }

    /// Whether a callee's `any` was written: an annotation or cast
    /// ([`Checker::any_is_written_in_an_annotation`]), a property declared
    /// `: any`, or a member read off such a receiver.
    fn untyped_any_is_written(&mut self, callee: Expression<'_>) -> bool {
        if self.any_is_written_in_an_annotation(callee) {
            return true;
        }
        let Expression::PropertyAccessExpression(access) = callee else { return false };
        let (Some(receiver), Some(tsr_ast::MemberName::Identifier(name))) =
            (access.expression, access.name)
        else {
            return false;
        };
        let receiver_type = self.check_expression(receiver);
        if receiver_type == self.intrinsics.any {
            return self.untyped_any_is_written(receiver);
        }
        let receiver_type = self.apparent_type(receiver_type);
        let Some(property) = self.get_property_of_type(receiver_type, name.text) else {
            return false;
        };
        let Some(declaration) = self.binder.symbols().get(property).value_declaration else {
            return false;
        };
        let annotation = match self.node_map.get(declaration) {
            Some(tsr_ast::Node::PropertyDeclaration(node)) => node.r#type,
            Some(tsr_ast::Node::PropertySignatureDeclaration(node)) => node.r#type,
            Some(tsr_ast::Node::ParameterDeclaration(node)) => node.r#type,
            _ => None,
        };
        matches!(annotation, Some(tsr_ast::TypeNode::KeywordTypeNode(keyword))
            if keyword.kind == tsr_ast::SyntaxKind::AnyKeyword)
    }

    fn report_at_node(&mut self, node: tsr_ast::NodeId, diagnostic: Diagnostic) {
        if let Some(file) = self.source_file_of_for_diagnostics(node) {
            self.report(file, diagnostic);
        }
    }

    /// The type of a call expression.
    ///
    /// Ported from `Checker.checkCallExpression` (`checker.go:8951`) into
    /// `getReturnTypeOfSignature` on the resolved signature.
    ///
    /// Not ported, each answering `errorType`: an optional chain (`f?.()`), a
    /// `super(…)` call, an `import(…)` call, a call with explicit type arguments,
    /// a call to a callee with no call signature, and an overloaded call outside
    /// what [`Checker::choose_overload`] can decide.
    /// §818: `Promise<any>`, which is what `createPromiseType`
    /// (`checker.go:20348`) answers for `anyType` — `getAwaitedTypeNoAlias(any)`
    /// is `any`, so no unwrapping is observable. `None` when the lib has no
    /// `Promise`, which is upstream's `errorType` return at `:20378`.
    fn promise_of_any(&mut self) -> Option<TypeId> {
        let promise = self.global_type_symbol_with_arity("Promise", 1)?;
        let any = self.intrinsics.any;
        Some(self.create_type_reference(promise, vec![any]))
    }

    /// §140 + §818: the dynamic-import call's type.
    ///
    /// `checkImportCallExpression` (`checker.go:8267`) has three returns and
    /// **all three are a promise**: no arguments answers `Promise<any>`
    /// (`:8273`), a resolvable module answers `Promise<typeof import("m")>`
    /// (`:8311`), and everything else falls through to `Promise<any>` (`:8314`)
    /// — upstream's own comment at `:8302` says why that catches a non-literal
    /// specifier, *"resolveExternalModuleName will return undefined if the
    /// moduleReferenceExpression is not a string literal"*, and it catches an
    /// unresolvable module with it.
    ///
    /// §140 built the middle return and declined the other two; §818 supplies
    /// them. `None` — the caller's `errorType` — is now reached for exactly one
    /// reason, and it is the faithful one: **no `Promise` global**, where
    /// `createPromiseReturnType` (`:20374`) reports
    /// `A_dynamic_import_call_returns_a_Promise…` and answers `errorType`.
    fn check_import_call_expression(&mut self, node: &CallExpression<'_>) -> Option<TypeId> {
        // `:8273`: no arguments, so there is no specifier to resolve.
        let Some(specifier) = node.arguments.first().copied() else {
            return self.promise_of_any();
        };
        // `:8314`'s fall-through. A non-literal specifier and an unresolvable
        // module are the same answer upstream, and neither is an error here.
        let resolved =
            tsr_ast::Node::from(specifier).node_id().and_then(|specifier_id| match specifier {
                tsr_ast::Expression::StringLiteral(literal) => self
                    .resolve_external_module_name(specifier_id, specifier_id)
                    .map(|module| (literal, module)),
                _ => None,
            });
        let Some((literal, module)) = resolved else {
            return self.promise_of_any();
        };
        // Interned per (module, written spelling): duplicate mints would
        // churn prints (the bar's falsifier c).
        // SS196: the specifier is a STRING and prints escaped —
        // `printing::quote` already implements `escapedCharsMap`
        // (`printer/utilities.go:41`) and was simply not used here, so a
        // Windows-style relative specifier printed one backslash where the
        // baseline records two
        // (`ambientExternalModuleWithRelativeModuleName`).
        let text = format!("typeof import({})", crate::printing::quote(literal.text));
        let key = (text.clone(), module);
        let namespace = if let Some(&existing) = self.qualified_reference_types.get(&key) {
            existing
        } else {
            let minted = self.store.new_anonymous(
                crate::flags::TypeFlags::OBJECT,
                text.clone(),
                module,
                false,
            );
            self.qualified_reference_types.insert(key, minted);
            minted
        };
        let promise = self.global_type_symbol_with_arity("Promise", 1)?;
        Some(self.create_type_reference(promise, vec![namespace]))
    }

    /// The type of a call expression — `checkCallExpression`.
    pub fn check_call_expression(&mut self, node: &CallExpression<'_>) -> TypeId {
        let error = self.intrinsics.error;
        bump(&COUNTERS.call_expressions);
        let Some(callee) = node.expression else { return error };
        // `checkCallExpression` (`checker.go:8331`): a `super(...)` call is
        // `void` (`checker-notes-callres.md` §24).
        if matches!(
            callee,
            Expression::KeywordExpression(keyword)
                if keyword.kind == tsr_ast::SyntaxKind::SuperKeyword
        ) {
            return self.intrinsics.void;
        }
        // §140 (`checker-notes-narrow.md`): `import("./m")` types as
        // `Promise<typeof import("./m")>` — `checkImportCallExpression`. The
        // namespace type is minted HERE, the one place holding the specifier
        // verbatim, so the import-spelling needs no per-site machinery. An
        // unresolvable specifier keeps today's error (the §31-family
        // boundary owns those).
        if matches!(
            callee,
            Expression::KeywordExpression(keyword)
                if keyword.kind == tsr_ast::SyntaxKind::ImportKeyword
        ) {
            if let Some(result) = self.check_import_call_expression(node) {
                return result;
            }
            return error;
        }
        let raw_callee_type = self.check_expression(callee);
        // `checkCallChain` (`checker.go:8300` family): the callee strips its
        // nullable half through the same three chain functions property
        // access uses, and the result re-unions `undefined` when anything
        // was stripped. See `checker-notes-callres.md` §22.
        let optional = node.question_dot_token.is_some();
        if optional {
            bump(&COUNTERS.optional_chain);
        }
        let (callee_type, chain_stripped) = if optional
            || callee.node_id().is_some_and(|id| self.expression_is_optional_chain(id))
        {
            let non_optional =
                self.get_optional_expression_type(raw_callee_type, callee.node_id(), optional);
            let stripped = self.check_non_null_type(non_optional);
            if stripped == error {
                return error;
            }
            (stripped, non_optional != raw_callee_type)
        } else {
            (raw_callee_type, false)
        };
        let result = self.check_call_expression_worker(node, callee, callee_type);
        // checkCallExpression (native 5b1047d1): a CommonJS require in JS
        // returns the resolved module value, not the ambient call signature's
        // any. TS calls and a locally shadowing function keep ordinary calls.
        if self.is_commonjs_require(node) {
            return self
                .commonjs_require_target(node)
                .map_or(self.intrinsics.any, |target| self.get_type_of_symbol(target));
        }
        // A generic call's final object-argument context follows inference.
        // Non-generic calls already supplied their concrete context on the
        // initial check; repeating it can re-enter a flow-dependent initializer.
        // Rebuild the outer
        // literal only: clearing checked initializer symbols would discard
        // their established contextual types and reopen resolution cycles.
        if result != error
            && node.node_id.and_then(|id| self.resolved_call_signatures.get(&id)).is_some_and(
                |signature| self.signature_declares_type_parameters(signature.declaration),
            )
        {
            for &argument in node.arguments {
                if matches!(argument, Expression::ObjectLiteralExpression(_))
                    && !self.is_context_sensitive_argument(&argument)
                {
                    if let Some(id) = argument.node_id() {
                        self.node_types.remove(&id);
                    }
                    self.check_expression(argument);
                }
            }
        }
        // §29 (`checker-notes-callres.md`): a `this`-minted result
        // instantiates to the RECEIVER — `[a, b].sort()` is the array's own
        // type.
        let result = if self.this_type_nodes.values().any(|&t| t == result)
            && let Expression::PropertyAccessExpression(access) = callee
            && let Some(receiver) = access.expression
        {
            self.check_expression(receiver)
        } else {
            result
        };
        if result == error || !chain_stripped {
            return result;
        }
        self.propagate_optional_type_marker_at(node.node_id, result, true)
    }

    /// isCommonJSRequire, native checker.go:15674. Resolve the binding before
    /// treating the spelling as the implicit JS require symbol. Ambient
    /// function/variable declarations qualify; local implementations and aliases
    /// do not. No result is cached independently of the ordinary node worker.
    pub(crate) fn is_commonjs_require(&self, node: &CallExpression<'_>) -> bool {
        let Some(id) = node.node_id else { return false };
        if !self.in_js_file(id)
            || node.arguments.len() != 1
            || !matches!(
                node.arguments[0],
                Expression::StringLiteral(_) | Expression::NoSubstitutionTemplateLiteral(_)
            )
        {
            return false;
        }
        let Some(Expression::Identifier(callee)) = node.expression else { return false };
        if callee.text != "require" {
            return false;
        }
        let Some(reference) = callee.node_id else { return false };
        let Some(symbol) = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            reference,
            callee.text,
            SymbolFlags::VALUE,
        ) else {
            // The native name resolver supplies its implicit requireSymbol in
            // JS when no value binding shadows it. TSR leaves it unresolved.
            return true;
        };
        let record = self.binder.symbols().get(symbol);
        let kind = if record.flags.intersects(SymbolFlags::FUNCTION) {
            tsr_ast::SyntaxKind::FunctionDeclaration
        } else if record.flags.intersects(SymbolFlags::VARIABLE) {
            tsr_ast::SyntaxKind::VariableDeclaration
        } else {
            return false;
        };
        !record.flags.intersects(SymbolFlags::ALIAS)
            && record.declarations.iter().any(|&declaration| {
                self.nodes.kind(declaration) == kind
                    && self.combined_node_flags(declaration).contains(tsr_ast::NodeFlags::AMBIENT)
            })
    }

    /// resolveExternalModuleTypeByLiteral's symbol half. Keep the original
    /// module identity or its resolved export= value; consumers retain their
    /// own written alias context. Existing module/alias workers own cycles and
    /// completion, and this adds neither a module image nor a second cache.
    pub(crate) fn commonjs_require_target(
        &mut self,
        node: &CallExpression<'_>,
    ) -> Option<tsr_binder::SymbolId> {
        let specifier = node.arguments.first()?.node_id()?;
        let module = self.resolve_external_module_name(specifier, specifier)?;
        Some(self.resolve_external_module_symbol(module))
    }

    fn check_call_expression_worker(
        &mut self,
        node: &CallExpression<'_>,
        callee: tsr_ast::Expression<'_>,
        callee_type: TypeId,
    ) -> TypeId {
        let error = self.intrinsics.error;
        // resolveCallExpression/isUntypedFunctionCall: a module copy has no
        // signatures, but may still be an untyped call through global Function.
        // An independently complete empty target demands nothing of the copy.
        // Other targets use the existing relation answer; unsupported
        // applicability never recovers the original function's signature.
        if self.module_value_clones.contains_key(&callee_type) {
            return if self.is_assignable_to_global_function(callee_type) {
                self.intrinsics.any
            } else {
                error
            };
        }
        // Split the largest bucket in the funnel by *why* the callee has no
        // object type. Done here rather than in `resolve_call_signature`
        // because only this path has the callee **node**, and the question is
        // about the expression that produced the type, not the type.
        if counters::counting()
            && !matches!(self.store.get(callee_type).data, TypeData::Anonymous { .. })
        {
            self.classify_unresolved_callee(callee, callee_type);
        }
        // `resolveUntypedCall` (`checker.go:9902`): a call through an `any`
        // callee is an **untyped call**, and its type is `any`. TS 1.0 spec
        // §4.12, quoted in upstream's comment above `isUntypedFunctionCall`
        // (`checker.go:9931`).
        //
        // This is NOT ADR-0038's forbidden rendering. Upstream keeps
        // `anySignature` (`checker.go:1042`, returning `anyType`) distinct from
        // `unknownSignature` (`checker.go:1043`, returning `errorType`), and its
        // error path is the separate `resolveErrorCall` (`checker.go:9923`). The
        // `any` answered here is an honest computation, not a failed one wearing
        // `any`'s name — `docs/architecture/checker-notes-calleegap.md` argues it
        // from those two adjacent upstream lines.
        if self.is_untyped_call_target(callee_type) {
            bump(&COUNTERS.untyped_call);
            return self.intrinsics.any;
        }
        // isUntypedFunctionCall's third disjunct: a `Function`-typed callee.
        if self.is_untyped_function_typed_callee(callee_type) {
            return self.intrinsics.any;
        }
        let resolved = self.resolve_call_signature_at(
            node.node_id,
            callee_type,
            Some(node.arguments),
            !node.type_arguments.is_empty(),
        );
        let Some(signature) = resolved else {
            // §250. Overload resolution FAILING does not make the call's type
            // unknown. Upstream reports on the arguments and then takes a
            // candidate anyway — `getCandidateForOverloadFailure`
            // (`checker.go:10285`) — so the call still prints a return type.
            //
            //     function f<T, U>() { }
            //     f<number, string, number>();
            //     >f<number, string, number>() : void
            //
            // Witnesses `compiler/callWithWrongNumberOfTypeArguments` (too many
            // type arguments, one signature) and
            // `compiler/signatureLengthMismatchInOverload` (no matching
            // overload, two signatures).
            //
            // Upstream's choice among candidates is a ranking this port does
            // not have, so instead of guessing it this answers ONLY where the
            // choice cannot matter: every candidate agrees on the return type,
            // and that type mentions no type parameter, so inference could not
            // have changed it either. Where candidates disagree the gap stays —
            // picking one would be inventing upstream's ranking, and
            // `getCandidateForOverloadFailure` ranks by argument-count
            // closeness, which is not derivable from the return types.
            //
            // The discriminating input is a failing call whose candidates have
            // DIFFERENT return types (conventions corollary 25). This declines
            // there by construction rather than by measurement, which is why no
            // fixture is needed to defend it.
            if let TypeData::Anonymous { symbol, .. } = self.store.get(callee_type).data
                && let Some(candidates) = self.get_signatures_of_symbol(symbol)
                && let Some(first) = candidates.first().map(|candidate| candidate.r#type)
                && candidates.iter().all(|candidate| candidate.r#type == first)
                && !self.mentions_any_type_parameter(first, 2)
            {
                return first;
            }
            return error;
        };
        // Upstream would now report on the arguments; see the module docs for
        // why this does not, and why the return type is the same either way.
        if !signature.type_parameters.is_empty() {
            // Const callback return contexts now preserve their literal source
            // before inference (getReturnTypeFromBody, checker.go:20141).
            // §288: WRITTEN type arguments need no inference at all — the
            // SS161 recipe at the CALL road. `fn2<string>(4)` instantiates
            // the return with the written list when the arity matches
            // (`typeAssertions`, `thisInInvalidContexts`); a mismatch or an
            // unresolvable argument keeps the inference road below.
            // §690: upstream reports the ARITY error and instantiates anyway —
            // `f<number>(1, '')` on `f<T, U>` still records `number`
            // (`callGenericFunctionWithIncorrectNumberOfTypeArguments`), because
            // `checkTypeArguments` diagnoses while `getSignatureInstantiation`
            // proceeds through `fillMissingTypeArguments`. Requiring an exact
            // match made every mis-arity call a gap. Extra arguments are
            // dropped and missing ones filled below.
            // Only the SURPLUS half: `f<number, string, number>(…)` on `f<T, U>`
            // drops the third and instantiates. The missing half
            // (`f<number>(…)`) needs `fillMissingTypeArguments`' DEFAULTS, and
            // this port's `TypeParameter` carries no default — filling with
            // `any` instead measured **62 RIGHT→WRONG in `genericDefaults`
            // alone**. Reopens when type-parameter defaults are modelled.
            if node.type_arguments.len() >= signature.type_parameters.len()
                && !signature.type_parameters.is_empty()
                && let Some(written_nodes) =
                    node.node_id.and_then(|id| match self.node_map.get(id) {
                        Some(tsr_ast::Node::CallExpression(fetched)) => {
                            Some(fetched.type_arguments)
                        }
                        _ => None,
                    })
            {
                let written: Vec<TypeId> = written_nodes
                    .iter()
                    .map(|&argument| self.get_type_from_type_node(argument))
                    .collect();
                if !written.contains(&error)
                    && let Some(parameters) = self.type_parameter_types(&signature)
                {
                    let names: Vec<&str> = signature
                        .type_parameters
                        .iter()
                        .map(|parameter| parameter.name.as_str())
                        .collect();
                    // `fillMissingTypeArguments`: pair what was written with the
                    // parameters in order, drop any surplus, and fill a missing
                    // slot with `any` — upstream fills from the parameter's
                    // default or constraint, and neither is observable on the
                    // lines this converts (the return slot uses the WRITTEN
                    // arguments), so `any` is the honest stand-in until
                    // defaults land.
                    let map: Vec<(TypeId, TypeId)> =
                        parameters.iter().copied().zip(written.iter().copied()).collect();
                    let returned = self.get_return_type_of_signature(&signature).unwrap_or(error);
                    let answer = self.instantiate_type(returned, &map, &parameters, &names);
                    if answer != error {
                        let contextual = node.arguments.iter().any(|argument| {
                            self.is_context_sensitive_argument(argument)
                                || matches!(argument, Expression::ObjectLiteralExpression(_))
                        });
                        // getResolvedSignature/getSignatureInstantiation, pinned
                        // tsgo 5b1047d1 checker.go:8407/19293. Only the original
                        // single-generic call with written arguments owns this
                        // extension: its Checker, AST node, ordered argument slice
                        // and declaration must match, without a foreign fixing
                        // frame, active/provisional owner or temporary flow.
                        // The existing whole-signature worker publishes Some;
                        // return-only success and unsupported work publish nothing.
                        // Ordinary inferred callable sources remain excluded:
                        // recovered returns do not certify contextual completion
                        // (coordinator tsr-6.47.4.2.1 follow-up).
                        let publish_original = !contextual
                            && signature.target.is_none()
                            && self.alias_evaluation_bindings.is_empty()
                            && self.mapped_template_depth == 0
                            && self.flow_loop_stack.is_empty()
                            && !self.contextual_prefers_uninstantiated
                            && self.uninstantiated_context_node.is_none()
                            && node.node_id.is_some_and(|call| {
                                matches!(self.node_map.get(call), Some(tsr_ast::Node::CallExpression(original))
                                    if std::ptr::eq(original, node) && std::ptr::eq(original.arguments, node.arguments))
                                    && !self.active_inference_contexts.contains_key(&call)
                                    && self.live_inference_context(call).is_none()
                                    && !self.call_inference_signatures.contains_key(&call)
                                    && !self.resolving_signature_calls.contains(&call)
                            })
                            && self.call_signatures_of_type(callee_type).is_some_and(|originals| {
                                matches!(originals.as_slice(), [original]
                                    if original.declaration == signature.declaration && original.target.is_none())
                            });
                        let mut instance = signature.clone();
                        if publish_original {
                            instance.type_parameters.clear();
                        }
                        if (contextual || publish_original)
                            && let Some(call_id) = node.node_id
                            && let Some(mut concrete) =
                                self.instantiate_signature(instance, &map, &parameters, &names)
                        {
                            if publish_original {
                                // instantiateSignatureEx erases only the result's
                                // parameters and retains sig as target (20619).
                                // Keep target-only constraints/defaults out of
                                // the worker's eager substitution, but restore
                                // the original generic vector and carried slots.
                                concrete.target = Some(std::sync::Arc::new(signature.clone()));
                                self.resolved_call_signatures.entry(call_id).or_insert(concrete);
                            } else {
                                self.resolved_call_signatures.insert(call_id, concrete);
                            }
                        }
                        bump(&COUNTERS.new_instantiated);
                        return answer;
                    }
                }
            }
            // A generic signature's return type depends on the arguments, so it
            // needs inference (`inferTypeArguments`, `checker.go:9390`). Answering
            // the uninstantiated return type would print `T` where upstream prints
            // what `T` was inferred as, so [`crate::inference`] answers the shapes
            // it can read a candidate off directly and `errorType` for the rest.
            let mut instantiated = None;
            let contextual = node.arguments.iter().any(|argument| {
                self.is_context_sensitive_argument(argument)
                    || matches!(argument, Expression::ObjectLiteralExpression(_))
            });
            let answer = self.check_generic_call_with(
                &signature,
                node.node_id,
                node.arguments,
                contextual.then_some(&mut instantiated),
            );
            if answer != error
                && let Some(call_id) = node.node_id
                && let Some(concrete) = instantiated
            {
                self.resolved_call_signatures.insert(call_id, concrete);
            }
            if counters::counting() {
                bump(&COUNTERS.single_candidate_generic);
                if answer == error {
                    bump(&COUNTERS.single_candidate_generic_error);
                }
            }
            return answer;
        }
        if node.arguments.iter().any(|argument| {
            self.is_context_sensitive_argument(argument)
                || matches!(argument, Expression::ObjectLiteralExpression(_))
        }) && let Some(call_id) = node.node_id
        {
            self.resolved_call_signatures.insert(call_id, signature.clone());
        }
        // `checkNoTypeArguments` (`checker.go:23157`): type arguments on a
        // signature that takes none is an ERROR — and the call still answers
        // the signature's return. §375 corrects the old reading ("answering
        // the return type would quietly drop them"): upstream reports TS2558
        // and resolves the error call through the candidate anyway
        // (`typeAssertions` records `fn2<string>(4) : void`). The report is
        // the diagnostics lane's; the type is this one's.
        let Some(returned) = self.get_return_type_of_signature(&signature) else {
            return error;
        };
        if !node.type_arguments.is_empty() {
            bump(&COUNTERS.single_candidate_type_arguments);
            return returned;
        }
        // `resolveCallExpression` (`checker.go:8348`): *"treat any call to the
        // global `Symbol` function that is part of a const variable or readonly
        // property as a fresh unique symbol literal type"*. This port has
        // `TypeFlags::UNIQUE_ES_SYMBOL` and no type that carries it, so the
        // position **gaps** rather than answering `symbol` — which is what
        // upstream prints only *outside* those positions
        // (`getESSymbolLikeTypeForNode`, `checker.go:22982`, falls through to
        // `esSymbolType`). 291 corpus lines want `unique symbol` here and this
        // is the difference between them being gaps and being wrong answers;
        // the 150 lines in other positions still answer, which is why the test
        // is on the *position* and not on the return type
        // (`docs/architecture/checker-notes-namedcallee.md` §4.1).
        if self.store.get(returned).flags.intersects(TypeFlags::ES_SYMBOL_LIKE)
            && Self::is_symbol_or_symbol_for_call(node)
            && self.is_valid_es_symbol_declaration(node.node_id)
        {
            // §26 (`checker-notes-callres.md`): the position the gate
            // detected now MINTS — one distinct `unique symbol` per site
            // (`getESSymbolLikeTypeForNode`, `checker.go:22982`).
            return self.store.new_named(
                TypeFlags::UNIQUE_ES_SYMBOL,
                "unique symbol".to_string(),
                None,
            );
        }
        if counters::counting() {
            if returned == error {
                bump(&COUNTERS.single_candidate_return_error);
            } else {
                bump(&COUNTERS.single_candidate_answered);
            }
        }
        // §164 (`checker-notes-narrow.md`) at the CALL site: a signature
        // that RETURNS the this-type answers the receiver, which is the same
        // `getTypeWithThisArgument` reading the member road takes — the
        // receiver here is the property access's own left operand
        // (`c.fn()` is `C`, not `this`).
        if self.this_types.values().any(|&minted| minted == returned)
            && let Expression::PropertyAccessExpression(access) = callee
            && let Some(receiver) = access.expression
        {
            let receiver_type = self.check_expression(receiver);
            if receiver_type != error && receiver_type != returned {
                return receiver_type;
            }
        }
        returned
    }

    /// Upstream's `isSymbolOrSymbolForCall` (`checker.go:8381`), without the
    /// global-symbol identity check.
    ///
    /// Upstream compares the resolved symbol against
    /// `getGlobalESSymbolConstructorSymbolOrNil`. This port has no global-type
    /// table, so the name test stands alone. It is sound in the direction that
    /// matters: a locally shadowed `Symbol` would be **refused** where upstream
    /// answers, which costs a gap rather than a wrong line, and no corpus case
    /// in the measured population shadows it.
    fn is_symbol_or_symbol_for_call(node: &CallExpression<'_>) -> bool {
        let Some(mut left) = node.expression else { return false };
        if let Expression::PropertyAccessExpression(access) = left {
            let is_for = access.name.is_some_and(|name| {
                matches!(tsr_ast::Node::from(name), tsr_ast::Node::Identifier(id) if id.text == "for")
            });
            if is_for {
                let Some(inner) = access.expression else { return false };
                left = inner;
            }
        }
        matches!(left, Expression::Identifier(id) if id.text == "Symbol")
    }

    /// Upstream's `isValidESSymbolDeclaration` (`utilities.go:961`) asked of the
    /// position a call sits in, after `WalkUpParenthesizedExpressions`
    /// (`checker.go:8351`).
    fn is_valid_es_symbol_declaration(&self, call: Option<tsr_ast::NodeId>) -> bool {
        let Some(call) = call else { return false };
        let mut parent = self.nodes.parent(call);
        while let Some(node) = parent {
            if self.nodes.kind(node) != tsr_ast::SyntaxKind::ParenthesizedExpression {
                break;
            }
            parent = self.nodes.parent(node);
        }
        let Some(parent) = parent else { return false };
        let has = |modifiers: &[tsr_ast::ModifierLike<'_>], kind: tsr_ast::SyntaxKind| {
            modifiers.iter().any(
                |modifier| matches!(modifier, tsr_ast::ModifierLike::Token(t) if t.kind == kind),
            )
        };
        match self.node_map.get(parent) {
            Some(tsr_ast::Node::VariableDeclaration(declaration)) => {
                matches!(declaration.name, Some(tsr_ast::BindingName::Identifier(_)))
                    && self.nodes.parent(parent).is_some_and(|list| {
                        self.nodes.flags(list).intersects(tsr_ast::NodeFlags::CONST)
                    })
            }
            Some(tsr_ast::Node::PropertyDeclaration(property)) => {
                has(property.modifiers, tsr_ast::SyntaxKind::ReadonlyKeyword)
                    && has(property.modifiers, tsr_ast::SyntaxKind::StaticKeyword)
            }
            Some(tsr_ast::Node::PropertySignatureDeclaration(property)) => {
                has(property.modifiers, tsr_ast::SyntaxKind::ReadonlyKeyword)
            }
            _ => false,
        }
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
    /// populated.
    ///
    /// ~~today that line is usually a gap, because `TemplateExpression` is
    /// unported and correctly stays so~~ — **stale, corrected §203's audit.**
    /// `check_template_expression` has answered a tagged template `string`
    /// since §147 (upstream never folds a tagged template's substitution form),
    /// and since §198 an untyped substitution no longer gaps a template at all.
    /// The sentence survived two changes to the thing it describes, which is
    /// `docs/conventions.md` corollary 8's shape in a doc comment that gates
    /// nothing — harmless to the compiler and misleading to the next reader,
    /// who would have priced this form's residue against the wrong baseline.
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
        if node.question_dot_token.is_some() {
            return error;
        }
        let Some(tag) = node.tag else { return error };
        let tag_type = self.check_expression(tag);
        // Checked for its own line; the template's type does not reach the
        // answer, exactly as a call's arguments do not.
        if let Some(template) = node.template {
            self.check_expression(template.into());
        }
        // §901: `resolveUntypedCall` (`checker.go:9899`) — a tag of type `any`
        // resolves to `anySignature`, so the tagged template is `any`.
        //
        // ```go
        // case ast.KindTaggedTemplateExpression:
        //     c.checkExpression(node.AsTaggedTemplateExpression().Template)
        // …
        // return c.anySignature
        // ```
        //
        // The CALL road has had this since `checker-notes-calleegap.md`
        // (`is_untyped_call_target`, the same predicate); the tagged-template
        // road went straight to `resolve_call_signature`, which answers `None`
        // for `any` and gapped. `var f: any; f`abc`` wants `any`.
        //
        // Placed after the template check, which is the order upstream's
        // `resolveUntypedCall` uses — the template is checked for its own lines
        // whether or not the tag is typed.
        if self.is_untyped_call_target(tag_type) {
            return self.intrinsics.any;
        }
        if self.is_untyped_function_typed_callee(tag_type) {
            return self.intrinsics.any;
        }
        // §914: an OVERLOADED tag is selected by ARITY, which a tagged template
        // has even though this port cannot build its argument *expressions*.
        //
        // `getEffectiveCallArguments` (`checker.go`) for a tagged template is
        //
        // ```go
        // args := []*ast.Node{createSyntheticExpression(template, c.getGlobalTemplateStringsArrayType())}
        // for _, span := range template.AsTemplateExpression().TemplateSpans.Nodes {
        //     args = append(args, span.Expression())
        // }
        // ```
        //
        // — a synthetic `TemplateStringsArray` followed by one argument per
        // substitution. **The COUNT of that list needs no synthetic expression**:
        // it is `1 + spans`, and `hasCorrectArity` is upstream's first pass in
        // `chooseOverload`. A single arity survivor is the answer, exactly as it
        // is for a call (`callres2` slice 1).
        //
        // Selection only. Argument *checking* and inference still need the
        // expressions, so a generic survivor falls through to the type-argument
        // road below or declines, and two survivors keep the gap.
        let argument_count = 1 + match node.template {
            Some(tsr_ast::TemplateLiteral::TemplateExpression(template)) => {
                template.template_spans.len()
            }
            _ => 0,
        };
        let candidates = self.call_signatures_of_type(tag_type).unwrap_or_default();
        // `chooseOverload` skips a candidate failing
        // `hasCorrectTypeArgumentArity` (`checker.go:9214`) before arity, so
        // `fooFn<number>``…`` cannot pick a non-generic overload.
        let type_argument_count = node.type_arguments.len();
        let arity_pick = if candidates.len() > 1 {
            let survivors: Vec<&Signature> = candidates
                .iter()
                .filter(|candidate| {
                    candidate.this_parameter.is_none()
                        && (type_argument_count == 0
                            || type_argument_count
                                >= Self::min_type_argument_count(&candidate.type_parameters)
                                && type_argument_count <= candidate.type_parameters.len())
                        && has_correct_arity(candidate, argument_count)
                })
                .collect();
            if let [survivor] = survivors.as_slice() {
                Some((*survivor).clone())
            } else {
                // §917: two or more arity survivors go to `chooseOverload`'s
                // ARGUMENT pass, which §916's shift makes reachable — each
                // candidate is shifted past its strings-array parameter so the
                // rest pair with the substitutions by index, exactly as a call's
                // do. The pick is mapped back to the UNSHIFTED candidate by
                // declaration, so the signature that leaves here is the real one
                // and only the selection used the shifted view.
                let substitutions: Vec<Expression<'_>> = match node.template {
                    Some(tsr_ast::TemplateLiteral::TemplateExpression(expression)) => {
                        expression.template_spans.iter().filter_map(|s| s.expression).collect()
                    }
                    _ => Vec::new(),
                };
                let shifted: Vec<Signature> = candidates
                    .iter()
                    .filter(|candidate| !candidate.parameters.is_empty())
                    .map(|candidate| {
                        let mut shifted = candidate.clone();
                        shifted.parameters.remove(0);
                        shifted
                    })
                    .collect();
                if shifted.len() == candidates.len() {
                    self.choose_overload(&shifted, &substitutions, false, None).and_then(|picked| {
                        candidates
                            .iter()
                            .find(|candidate| candidate.declaration == picked.declaration)
                            .cloned()
                    })
                } else {
                    None
                }
            }
        } else {
            None
        };
        // `None` for the argument list: a tagged template's argument
        // EXPRESSIONS are the strings array and the substitutions, neither of
        // which this port builds, so an overloaded tag with no single arity
        // survivor stays a gap.
        let Some(signature) = arity_pick.or_else(|| self.resolve_call_signature(tag_type, None))
        else {
            return error;
        };
        if !signature.type_parameters.is_empty() {
            // §914: WRITTEN type arguments instantiate the return, exactly as a
            // call's do (`checker.go`'s `fillMissingTypeArguments` half, ported
            // for calls at `check_call_expression_worker`). A tagged template
            // may carry them — `` f<number>`x` `` — and this road used to reject
            // the whole expression the moment it saw any
            // (`!node.type_arguments.is_empty() → error`), before resolving
            // anything.
            //
            // Only the SURPLUS half, for the same reason calls take only that
            // half: the missing half needs type-parameter DEFAULTS, which this
            // port does not model, and filling with `any` measured 62
            // `RIGHT→WRONG` in `genericDefaults` when the call road tried it.
            // Re-fetched from `node_map` for its `'a` lifetime, exactly as the
            // call road does at `check_call_expression_worker` — a `&node`
            // borrowed here cannot outlive the `&mut self` the resolution needs.
            if node.type_arguments.len() >= signature.type_parameters.len()
                && let Some(parameters) = self.type_parameter_types(&signature)
                && let Some(written_nodes) =
                    node.node_id.and_then(|id| match self.node_map.get(id) {
                        Some(tsr_ast::Node::TaggedTemplateExpression(fetched)) => {
                            Some(fetched.type_arguments)
                        }
                        _ => None,
                    })
            {
                let written: Vec<TypeId> = written_nodes
                    .iter()
                    .map(|&argument| self.get_type_from_type_node(argument))
                    .collect();
                if !written.contains(&error) {
                    let names: Vec<&str> = signature
                        .type_parameters
                        .iter()
                        .map(|parameter| parameter.name.as_str())
                        .collect();
                    let map: Vec<(TypeId, TypeId)> =
                        parameters.iter().copied().zip(written.iter().copied()).collect();
                    let returned = self.get_return_type_of_signature(&signature).unwrap_or(error);
                    let answer = self.instantiate_type(returned, &map, &parameters, &names);
                    if answer != error {
                        return answer;
                    }
                }
            }
            // §916: inference from the SUBSTITUTIONS, without building a
            // synthetic argument for the strings array.
            //
            // Upstream's argument list is `[TemplateStringsArray, ...spans]`, so
            // substitution *i* pairs with parameter *i + 1*. Every index pairing
            // in `check_generic_call` is `parameters[i] ↔ arguments[i]`, and
            // threading an offset through all of them is one way to get the `+1`.
            // **Dropping the first parameter from the signature is the same
            // thing and touches nothing**: the remaining parameters line up with
            // the substitutions by construction, and the return type and type
            // parameters are untouched, so the inference that runs is exactly
            // upstream's over the same pairs.
            //
            // What this gives up, stated: a tag whose FIRST parameter is generic
            // (`tag<T>(s: T, …)`) loses that inference site. Upstream infers
            // `TemplateStringsArray` into it; this port would need the synthetic
            // argument to do the same, and no corpus tag is written that way.
            if let Some(template) = node.template
                && let tsr_ast::TemplateLiteral::TemplateExpression(expression) = template
                && !signature.parameters.is_empty()
            {
                let substitutions: Vec<Expression<'_>> =
                    expression.template_spans.iter().filter_map(|span| span.expression).collect();
                if substitutions.len() == expression.template_spans.len() {
                    let mut shifted = signature.clone();
                    shifted.parameters.remove(0);
                    let answer = self.check_generic_call(&shifted, node.node_id, &substitutions);
                    if answer != error {
                        return answer;
                    }
                }
            }
            return error;
        }
        self.get_return_type_of_signature(&signature).unwrap_or(error)
    }

    /// Whether `signature` accepts exactly `count` arguments. §788.
    ///
    /// `hasCorrectArity` (`checker.go`) reduced to what decides an overload
    /// pick here: a rest parameter makes the maximum unbounded, and the
    /// minimum is the count of leading parameters that are neither optional
    /// nor rest.
    pub(crate) fn arity_accepts(signature: &crate::signatures::Signature, count: usize) -> bool {
        let required = signature
            .parameters
            .iter()
            .take_while(|parameter| !parameter.optional && !parameter.rest)
            .count();
        let unbounded = signature.parameters.iter().any(|parameter| parameter.rest);
        count >= required && (unbounded || count <= signature.parameters.len())
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
    pub fn resolve_call_signature(
        &mut self,
        callee: TypeId,
        arguments: Option<&[Expression<'_>]>,
    ) -> Option<Signature> {
        self.resolve_call_signature_with_type_arguments(callee, arguments, false)
    }

    /// [`Checker::resolve_call_signature`], carrying whether the CALL writes
    /// type arguments — §273's truncated prefix must not pick a non-generic
    /// candidate for `f<string>(…)`, since upstream's
    /// `hasCorrectTypeArgumentArity` skips every candidate that cannot take
    /// them and only the (undecidable) generic tail remains.
    pub fn resolve_call_signature_with_type_arguments(
        &mut self,
        callee: TypeId,
        arguments: Option<&[Expression<'_>]>,
        has_type_arguments: bool,
    ) -> Option<Signature> {
        self.resolve_call_signature_at(None, callee, arguments, has_type_arguments)
    }

    fn resolve_call_signature_at(
        &mut self,
        call: Option<tsr_ast::NodeId>,
        callee: TypeId,
        arguments: Option<&[Expression<'_>]>,
        has_type_arguments: bool,
    ) -> Option<Signature> {
        // cloneTypeAsModuleType's empty signature lists are authoritative even
        // though the retained symbol provenance is a function declaration.
        if self.module_value_clones.contains_key(&callee) {
            return None;
        }
        // Counting is restricted to the call-expression path: a tagged template
        // passes no argument list, and folding its callees into the same buckets
        // would leave the funnel's denominator counting two different questions.
        let counted = arguments.is_some();
        // SS176 (`resolveCallExpression`, checker.go:8515, transcribed):
        // the callee is read through its APPARENT type, so a call whose
        // callee is a TYPE PARAMETER resolves against its constraint's
        // signatures - `f<T extends () => number>(g: T) { return g() }`.
        // The apparent type of everything else is itself, so this is a
        // no-op elsewhere by construction.
        let callee = if self.store.get(callee).flags.contains(TypeFlags::TYPE_PARAMETER) {
            match self.type_parameter_constraint(callee) {
                Some(constraint) => constraint,
                None => callee,
            }
        } else {
            callee
        };
        // getUnionSignatures builds the union's own list before overload
        // resolution; selecting a constituent independently loses its domains.
        if matches!(self.store.get(callee).data, TypeData::Union { .. }) {
            if let Some(union_signatures) =
                self.signatures_of_type_kind(callee, SignatureKind::Call)
            {
                if let [single] = union_signatures.as_slice() {
                    if counted {
                        bump(&COUNTERS.single_candidate);
                    }
                    return Some(single.clone());
                }
                if let Some(arguments) = arguments
                    && let Some(chosen) =
                        self.choose_overload(&union_signatures, arguments, has_type_arguments, call)
                {
                    if counted {
                        bump(&COUNTERS.single_candidate);
                    }
                    return Some(chosen);
                }
            }
            return None;
        }
        let TypeData::Anonymous { symbol, .. } = self.store.get(callee).data else {
            // A callee that prints as a **name** — an interface with a call
            // signature member, which is where every lib constructor lives.
            // `getSignaturesOfType` (`checker.go:18959`) reads the *type's*
            // resolved members; `get_signatures_of_symbol` reads a symbol's
            // declarations and an interface declaration is not signature-shaped,
            // so this needs its own route (`bd tsr-4sa`,
            // `docs/architecture/checker-notes-namedcallee.md`).
            // §947.3: the type's OWN signatures first — `getSignaturesOfType`
            // (`checker.go:18959`) reads them off the type, and a NAMED type can
            // carry them. §947.2 registers a generic function alias's signatures
            // on its reference, which is a `TypeData::Named`, so `fc(1)` with
            // `fc: F<number>` reached this branch and found nothing.
            //
            // **Third instance of §932's split** — two roads reach the same pair
            // of types and only one reads the table. §932 wired the anonymous
            // branch and §932.1 the contextual one; this is the named branch.
            let from_type =
                self.signatures_of_type_kind(callee, SignatureKind::Call).unwrap_or_default();
            if let [single] = from_type.as_slice() {
                if counted {
                    bump(&COUNTERS.single_candidate);
                }
                return Some(single.clone());
            }
            if !from_type.is_empty()
                && let Some(arguments) = arguments
                && let Some(chosen) =
                    self.choose_overload(&from_type, arguments, has_type_arguments, call)
            {
                return Some(chosen);
            }
            let named = self.get_signature_of_named_type(callee, SignatureKind::Call);
            if named.is_some() {
                return named;
            }
            // §273 at the named-callee road: the subtype pass over the clean
            // candidate prefix — `Array(3)` through ArrayConstructor's call
            // signatures, the same shape as the `new` road's hook.
            if !has_type_arguments
                && let Some(arguments) = arguments
                && let Some(candidates) =
                    self.signature_candidates_of_named_type(callee, SignatureKind::Call)
                && !candidates.is_empty()
            {
                let candidates = self.reorder_candidates(candidates);
                let clean = Self::clean_candidate_prefix_len(&candidates);
                if clean > 0
                    && let Some(signature) =
                        self.subtype_pass_prefix_pick(&candidates, clean, arguments)
                {
                    return Some(signature);
                }
            }
            if counted {
                bump(&COUNTERS.callee_not_anonymous);
            }
            return None;
        };
        // An **instantiated** signature type carries the *uninstantiated*
        // symbol (`Checker::instantiate_signature_type`, `bd tsr-0hc`), so
        // reading the symbol's declarations back here would resolve the
        // uninstantiated signature and answer `Promise<TResult1 | TResult2>`
        // where upstream substitutes — a wrong line rather than a gap. The
        // signatures are read **off the type** instead (`bd tsr-1uz`), which
        // is upstream's shape everywhere: `getSignaturesOfType`
        // (`checker.go:18959`) resolves the type's signatures and never the
        // symbol's declarations. One candidate resolves exactly as the
        // symbol path's one candidate does; an overload set stays a gap for
        // the same reason the symbol path's does.
        if self.is_instantiated_signature_type(callee) {
            let signatures: Vec<_> = self
                .signature_types
                .get(&callee)?
                .iter()
                .filter(|signature| signature.kind == SignatureKind::Call)
                .cloned()
                .collect();
            if let [signature] = signatures.as_slice() {
                if counted {
                    bump(&COUNTERS.single_candidate);
                }
                let signature = self.complete_signature_return(signature.clone())?;
                return (signature.r#type != self.intrinsics.error).then_some(signature);
            }
            // §579: an OVERLOAD SET on an instantiated signature type is
            // selected here rather than gapped. The comment above says it
            // *"stays a gap for the same reason the symbol path's does"* — but
            // the symbol path does NOT gap a non-generic overload set:
            // `interface W { q(a: number): number; q(a: string): string }`
            // resolves `w.q(1)` to `number` today. So the two are not the same
            // reason, and what was left unhandled is exactly
            // **overloaded AND generic**:
            //
            //     interface X<T> { m(a: T): T; m(a: T, b: T): T }
            //     x.m(1)   // gapped, and the member printed the UNINSTANTIATED `T`
            //
            // `Array.prototype.concat` is that shape (two signatures, `T` from
            // `Array<T>`), which is why `[1,2].concat([3])` gapped while
            // `.slice`, `.indexOf`, `.push` and `.map` all answered. §37.1
            // carries the four-probe table that isolated it.
            //
            // The signatures are already correctly instantiated — reading them
            // OFF THE TYPE is what the branch above does and what
            // `getSignaturesOfType` (`checker.go:18959`) does — so the only
            // thing missing was running the SAME `choose_overload` the symbol
            // path runs. Every guard inside it (arity first, the clean prefix,
            // the same-return reduction) applies unchanged; a set it cannot
            // decide still declines.
            if let Some(arguments) = arguments
                && let Some(signature) =
                    self.choose_overload(&signatures, arguments, has_type_arguments, call)
            {
                let signature = self.complete_signature_return(signature)?;
                return (signature.r#type != self.intrinsics.error).then_some(signature);
            }
            if counted {
                bump(&COUNTERS.callee_no_signatures);
            }
            return None;
        }
        // §932: the SINGLE-SIGNATURE COLLAPSE's type was unreadable here.
        // `get_type_from_type_literal`'s §10.15 arm mints `{ (a: number): number }`
        // as an anonymous type printed in arrow form and records its signature in
        // `signature_types` — but **not** in `minted_signature_types`, so
        // `is_instantiated_signature_type` above answers `false` and never looks.
        // Control then falls to `get_signatures_of_symbol`, which reads the
        // `__type` symbol's declarations; a `TypeLiteralNode` is not
        // signature-shaped, so it answers `None` and the call gapped.
        //
        // Measured by the neighbour that worked: `{ (a: number): number; x: string }`
        // is callable (two members, no collapse) and `{ new (a: number): number }`
        // is constructible, while `{ (a: number): number }` alone answered
        // `error` — **the same signature as §921/§922/§923, a road correct for
        // one shape and silently incomplete for the neighbouring one, with the
        // working half hiding the broken one.**
        //
        // `getSignaturesOfType` (`checker.go:18959`) reads the *type's*
        // signatures, which is what `signature_types` holds, so consulting it
        // here is upstream's own order rather than a fallback.
        // Filtered by KIND. §10.15's collapse stores whichever signature the
        // literal declared, call or construct, and reading it unfiltered
        // answered a CONSTRUCT signature for a plain call — 3 `RIGHT->WRONG`,
        // one of them in a case named for the confusion
        // (`objectTypeWithConstructSignatureAppearsToBeFunctionType`, where
        // upstream reports and answers `any`).
        let from_type: Vec<Signature> = self
            .signature_types
            .get(&callee)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|signature| signature.kind == SignatureKind::Call)
            .collect();
        let signatures = if from_type.is_empty() && !self.signature_types.contains_key(&callee) {
            let Some(signatures) = self.get_signatures_of_symbol(symbol) else {
                if counted {
                    bump(&COUNTERS.callee_no_signatures);
                }
                return None;
            };
            signatures
        } else {
            from_type
        };
        let selected = match signatures.as_slice() {
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
                self.choose_overload(candidates, arguments, has_type_arguments, call)
            }
        }?;
        // A raw type vector can carry a declaration-owned pending return.
        // Complete only the selected semantic return, preserving its target and
        // mapper, and decline unsupported/active error without caching an image.
        let selected = self.complete_signature_return(selected)?;
        (selected.r#type != self.intrinsics.error).then_some(selected)
    }

    /// resolveNewExpression / resolveCall (checker.go:8575): construct
    /// candidates are already in native reorderCandidates order, so the full
    /// applicability walk can select specialized literal signatures directly.
    pub(crate) fn choose_construct_overload(
        &mut self,
        candidates: &[Signature],
        arguments: &[Expression<'_>],
        has_type_arguments: bool,
        call: Option<tsr_ast::NodeId>,
    ) -> Option<Signature> {
        if !has_type_arguments
            && let Some(picked) = self.transcribed_generic_set_walk(candidates, arguments, call)
        {
            return Some(picked);
        }
        self.choose_ordered_overload(candidates, arguments, has_type_arguments, call)
    }

    /// The first candidate every argument is assignable to, or `None`.
    ///
    /// Ported from `Checker.chooseOverload` (`checker.go:9025`), which walks the
    /// candidate list in `reorderCandidates` order, keeps the ones `hasCorrectArity`
    /// (`checker.go:9107`) admits, and returns the first whose parameters every
    /// argument satisfies under the assignable relation. Order is
    /// load-bearing — it is the whole tie-break — so this entry applies
    /// [`Checker::reorder_candidates`] to the declaration-order list
    /// [`Checker::get_signatures_of_symbol`] produces.
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
    pub(crate) fn choose_overload(
        &mut self,
        candidates: &[Signature],
        arguments: &[Expression<'_>],
        has_type_arguments: bool,
        call: Option<tsr_ast::NodeId>,
    ) -> Option<Signature> {
        // `resolveCall` reorders once before any pass (`checker.go:8843` ->
        // `reorderCandidates`, `:8957`); every first-match walk below assumes
        // that order. Construct candidates arrive reordered and enter at
        // `choose_ordered_overload` directly — the reorder is not idempotent
        // (a second pass splices merged groups back).
        if candidates.len() > 1 {
            let reordered = self.reorder_candidates(candidates.to_vec());
            return self.choose_ordered_overload(&reordered, arguments, has_type_arguments, call);
        }
        self.choose_ordered_overload(candidates, arguments, has_type_arguments, call)
    }

    fn choose_ordered_overload(
        &mut self,
        candidates: &[Signature],
        arguments: &[Expression<'_>],
        has_type_arguments: bool,
        call: Option<tsr_ast::NodeId>,
    ) -> Option<Signature> {
        if candidates.is_empty() {
            return None;
        }
        // Non-generic callbacks and object literals use the candidate walk.
        // Literal-only objects also need isSignatureApplicable's parameter
        // context; a context-free widened member can falsely reject its slot.
        // Callback parameter types keep their existing first-check retention.
        if !has_type_arguments
            && candidates.iter().all(|candidate| candidate.type_parameters.is_empty())
            && arguments.iter().any(|argument| {
                self.is_context_sensitive_argument(argument)
                    || matches!(
                        argument,
                        Expression::ObjectLiteralExpression(_)
                            | Expression::ArrayLiteralExpression(_)
                    )
            })
            && let Some(picked) = self.transcribed_generic_set_walk(candidates, arguments, call)
        {
            return Some(picked);
        }
        // callres2 slice 1: `hasCorrectArity` is upstream's FIRST pass
        // (checker.go:9107, inside chooseOverload's loop) and it runs here
        // BEFORE every reduction below - a SINGLE arity-survivor needs no
        // selection at all, so it returns exactly as a born-single candidate
        // does (that path imposes none of the guards below; a generic
        // survivor flows to the caller's check_generic_call like today's
        // single generics). Spread calls skip the pass - arity is
        // meaningless there and the spread decline below still fires.
        // Rest-bearing candidates keep the old declines this slice: their
        // arity floor differs and the survivor test would lie.
        if !arguments.iter().any(|a| matches!(a, Expression::SpreadElement(_)))
            && !candidates
                .iter()
                .any(|c| c.this_parameter.is_some() || c.parameters.iter().any(|p| p.rest))
        {
            let survivors: Vec<&Signature> =
                candidates.iter().filter(|c| has_correct_arity(c, arguments.len())).collect();
            if let [survivor] = survivors.as_slice() {
                let survivor = (*survivor).clone();
                // §359: a single ARITY survivor of an OVERLOADED set is not
                // yet the answer — upstream still argument-checks it, and a
                // rejected survivor is an overload FAILURE, which answers
                // §199's intersection of every candidate's return
                // (`foo(): string; foo(bar: string): number;` called
                // `foo(5)` is `string & number = never`,
                // `compiler/functionOverloads`). A born-single candidate
                // keeps the unguarded return; an UNDECIDABLE pair keeps the
                // survivor, which is this path's pre-§359 behaviour.
                // A GENERIC survivor is not argument-checked here: upstream
                // infers before `isSignatureApplicable` (`chooseOverload`,
                // `checker.go:9040-9080`), and relating arguments to its
                // uninstantiated parameters answered NotRelated for
                // `proxy<T, U>(fn: (options: T) => U)` given `oneArg`, which
                // then fell to the order-sensitive longest-candidate pick.
                // It flows to the caller's `check_generic_call` like a single
                // generic does. `docs/parity/notes/calls-inference.md` §3.
                if candidates.len() > 1 && survivor.type_parameters.is_empty() {
                    let mut verdict = Ternary::Related;
                    let call = self.call_for_overload_arguments(arguments);
                    for (index, &argument) in arguments.iter().enumerate() {
                        let Some(parameter) = survivor.parameters.get(index) else { break };
                        let argument_type = self.check_argument_in_candidate_context(
                            call,
                            Some(&survivor),
                            argument,
                        );
                        if !self.strict_null_checks
                            && self
                                .type_of(argument_type)
                                .flags
                                .intersects(TypeFlags::UNDEFINED | TypeFlags::NULL)
                            && !self
                                .type_of(argument_type)
                                .flags
                                .intersects(!(TypeFlags::UNDEFINED | TypeFlags::NULL))
                        {
                            continue;
                        }
                        let parameter_type = self.parameter_type(parameter);
                        match self.relate_ternary(
                            argument_type,
                            parameter_type,
                            Relation::Assignable,
                        ) {
                            Ternary::NotRelated => {
                                verdict = Ternary::NotRelated;
                                break;
                            }
                            Ternary::Unknown => {
                                if verdict == Ternary::Related {
                                    verdict = Ternary::Unknown;
                                }
                            }
                            Ternary::Related => {}
                        }
                    }
                    if verdict == Ternary::NotRelated {
                        // getCandidateForOverloadFailure does not combine a
                        // set containing generics. Return the longest original
                        // candidate so the caller can infer its instantiation.
                        if candidates.iter().any(|candidate| !candidate.type_parameters.is_empty())
                        {
                            let best = self.longest_candidate_index(candidates, arguments.len());
                            return Some(candidates[best].clone());
                        }
                        let returns: Vec<TypeId> =
                            candidates.iter().map(|candidate| candidate.r#type).collect();
                        let mut failure = candidates[0].clone();
                        failure.r#type = self.get_intersection_type(&returns, None);
                        return Some(failure);
                    }
                }
                // A generic survivor of an overloaded set still runs
                // `resolveCall`'s two passes (`checker.go:8922-8928`): with a
                // context-sensitive argument, the subtype pass can context-check
                // it and reject the candidate, and the assignable pass then
                // infers from the retained check, not a fresh one.
                if candidates.len() > 1
                    && !survivor.type_parameters.is_empty()
                    && !has_type_arguments
                    && arguments.iter().any(|argument| self.is_context_sensitive_argument(argument))
                    && let Some(picked) =
                        self.transcribed_generic_set_walk(candidates, arguments, call)
                {
                    return Some(picked);
                }
                return Some(survivor);
            }
            // §463: two or more arity survivors, EVERY one generic, their
            // return types spelled identically — the `Promise.resolve` shape
            // (`(): Promise<void>` arity-rejected; `<T>(value: T)` and
            // `<T>(value: T | PromiseLike<T>)` both return
            // `Promise<Awaited<T>>`). Upstream's pass walk tries them in
            // order and the first success wins; with agreeing return
            // spellings the pick cannot change the printed answer, which is
            // the §44 skip-with-agreement recipe at the call road. The first
            // survivor flows to the caller's `check_generic_call` exactly as
            // a single generic does — inference decides or gaps there.
            if survivors.len() >= 2
                && survivors.iter().all(|survivor| !survivor.type_parameters.is_empty())
            {
                let prints: Vec<String> =
                    survivors.iter().map(|survivor| self.type_to_string(survivor.r#type)).collect();
                if prints.windows(2).all(|pair| pair[0] == pair[1]) {
                    // Equal return spellings imply neither equal callback
                    // contexts nor equal inferences: `then<U>(s: (v: T) =>
                    // Promise<U>)` and `then<U>(s: (v: T) => U)` both print
                    // `Promise<U>` but infer different U. chooseOverload
                    // (checker.go:9040) infers and checks applicability per
                    // candidate, so the walk decides first whether or not an
                    // argument is context-sensitive; undecidable sets retain
                    // the return-agreement recovery
                    // (docs/architecture/checker-99-agreeing-overloads.md).
                    if !has_type_arguments
                        && let Some(picked) =
                            self.transcribed_generic_set_walk(candidates, arguments, call)
                    {
                        return Some(picked);
                    }
                    return Some(survivors[0].clone());
                }
            }
        }
        // §391: WRITTEN type arguments skip every candidate that cannot take
        // them (`hasCorrectTypeArgumentArity`), so a set with exactly ONE
        // generic candidate has its survivor decided by the write alone —
        // `foo<string>("hi")` on a non-generic + generic pair answers the
        // generic's instantiated return, `number`
        // (`typeArgumentsShouldDisallowNonGenericOverloads`). The §288
        // instantiation downstream does the rest; two or more generics keep
        // the existing roads.
        if has_type_arguments {
            let generics: Vec<&Signature> = candidates
                .iter()
                .filter(|candidate| !candidate.type_parameters.is_empty())
                .collect();
            if let [single] = generics.as_slice() {
                return Some((*single).clone());
            }
        }
        if !has_type_arguments
            && candidates.iter().any(|candidate| candidate.parameters.iter().any(|p| p.rest))
        {
            return self.transcribed_generic_set_walk(candidates, arguments, call);
        }
        // §273: these rejections used to judge the SET; upstream judges
        // candidates in ORDER — `chooseOverload` (`checker.go:9425`) walks and
        // the first success wins — so a CLEAN PREFIX, every candidate before
        // the first generic / this-parameter / rest / any-parameter one, is
        // decidable exactly as an all-clean set was. If a prefix candidate
        // matches, upstream picks it (or an earlier one, which rejected the
        // same way here) without ever consulting the problematic tail; only
        // when the whole prefix rejects does the tail's undecidability gap
        // the call. Witness `new Array(3)`: ArrayConstructor's FIRST construct
        // signature is the non-generic `new (arrayLength?: number): any[]`,
        // and upstream never reaches the generic overloads behind it.
        let clean_len = Self::clean_candidate_prefix_len(candidates);
        if clean_len == 0 {
            // §487: before conceding the whole-set gap, run upstream's own
            // loop over the set — it decides exactly where inference and the
            // relater can, and refuses everywhere else, so a former gap either
            // converts or stays a gap.
            if !has_type_arguments
                && let Some(picked) = self.transcribed_generic_set_walk(candidates, arguments, call)
            {
                bump(&COUNTERS.selected);
                return Some(picked);
            }
            // Nothing decidable before the first problematic candidate: the
            // old whole-set gap, counted by the first candidate's own reason.
            let first = &candidates[0];
            if !first.type_parameters.is_empty() {
                bump(&COUNTERS.generic_candidate);
            } else if first.this_parameter.is_some()
                || first.parameters.iter().any(|parameter| parameter.rest)
            {
                bump(&COUNTERS.this_or_rest_parameter);
            } else {
                bump(&COUNTERS.parameter_any);
            }
            return None;
        }
        let truncated = clean_len < candidates.len();
        let full_set = candidates;
        let candidates = &candidates[..clean_len];
        if truncated && has_type_arguments {
            // Written type arguments skip every non-generic candidate
            // upstream (`hasCorrectTypeArgumentArity`); only the generic tail
            // could answer, and it is undecidable here
            // (`overloadsAndTypeArgumentArity`, the third draft's R→W pair).
            bump(&COUNTERS.generic_candidate);
            return None;
        }
        // §273's second draft — the first ran the ASSIGNABLE pass over the
        // prefix and turned 34 gaps in `anyAssignabilityInInheritance` into
        // wrong answers, because upstream runs a SUBTYPE pass over ALL
        // candidates before assignability consults any (`checker.go:8924`):
        // `foo3(a)` with `a: any` against `(x: string)` + `(x: any)` picks
        // the `any` overload in pass one — `any` is not a subtype of
        // `string` — so an assignable-pass prefix winner is unsound whenever
        // a tail exists. With a tail, ONLY a subtype-pass winner inside the
        // prefix is upstream's certain answer; everything else gaps.
        if truncated {
            let pick = self.subtype_pass_prefix_pick(full_set, clean_len, arguments);
            if pick.is_some() {
                bump(&COUNTERS.selected);
                return pick;
            }
            // §487: the whole prefix rejected pass one (or a pair was
            // undecidable); upstream now consults the generic/rest tail — run
            // the transcribed loop over the FULL set before conceding.
            if let Some(picked) = self.transcribed_generic_set_walk(full_set, arguments, call) {
                bump(&COUNTERS.selected);
                return Some(picked);
            }
            bump(&COUNTERS.generic_candidate);
            return None;
        }
        // SS339: the untruncated set runs upstream's pass ORDER too - the
        // subtype pass first (`checker.go:8924`), and only then
        // assignability. A pass that DECIDABLY rejected every candidate
        // licenses pass three's first-wins walk (`ambiguousOverloadResolution`:
        // `f(x, x)` with `x: any` fails both subtype tests and picks the
        // FIRST assignable candidate, `number` - upstream never asks whether
        // the returns agree); anything undecidable keeps the ambiguity guard.
        let first_wins = match self.subtype_pass_outcome(candidates, candidates.len(), arguments) {
            SubtypePassOutcome::Picked(signature) => {
                bump(&COUNTERS.selected);
                return Some(*signature);
            }
            SubtypePassOutcome::AllRejected => true,
            SubtypePassOutcome::Undecidable => false,
        };
        let mut argument_types = Vec::with_capacity(arguments.len());
        for &argument in arguments {
            if matches!(argument, Expression::SpreadElement(_)) {
                bump(&COUNTERS.spread_argument);
                return None;
            }
            argument_types.push(self.check_expression(argument));
        }
        let mut chosen: Option<&Signature> = None;
        // Splits the empty-handed case in three: no candidate takes this many
        // arguments at all; arity matched and every candidate was *decidably*
        // rejected; and arity matched but the relation could not decide. Only
        // the second is a real negative.
        let mut arity_matched = false;
        for candidate in candidates {
            if !has_correct_arity(candidate, argument_types.len()) {
                continue;
            }
            arity_matched = true;
            // Kleene conjunction over the pairs, evaluated in full rather than
            // short-circuiting on the first `Unknown`: a definite `NotRelated`
            // later in the list is a strictly better answer than "could not
            // tell", and short-circuiting would refuse calls this port can
            // decide.
            let mut verdict = Ternary::Related;
            for (&argument, parameter) in argument_types.iter().zip(&candidate.parameters) {
                // With `strictNullChecks` off, `undefined`/`null` inhabit every
                // type's domain, so such an argument rejects NO candidate —
                // without this, `fn1(undefined)` reads as an overload failure
                // and §21's arm answers an intersection upstream never computes
                // (`overloadResolution`, the bar's fired falsifier (a)).
                if !self.strict_null_checks
                    && self
                        .type_of(argument)
                        .flags
                        .intersects(TypeFlags::UNDEFINED | TypeFlags::NULL)
                    && !self
                        .type_of(argument)
                        .flags
                        .intersects(!(TypeFlags::UNDEFINED | TypeFlags::NULL))
                {
                    continue;
                }
                let parameter_type = self.parameter_type(parameter);
                match self.relate_ternary(argument, parameter_type, Relation::Assignable) {
                    Ternary::NotRelated => {
                        verdict = Ternary::NotRelated;
                        break;
                    }
                    Ternary::Unknown => verdict = Ternary::Unknown,
                    Ternary::Related => {}
                }
            }
            match verdict {
                // The scan cannot step over this candidate: it does not know
                // whether this one would have won, so no later candidate may be
                // selected and no earlier selection may be trusted against it.
                // The whole call gaps — a gap beats a wrong answer.
                Ternary::Unknown => {
                    bump(&COUNTERS.undecidable_pair);
                    return None;
                }
                Ternary::NotRelated => continue,
                Ternary::Related => {}
            }
            match chosen {
                Some(_) if first_wins => break,
                // Upstream's subtype pass would decide this; see the doc
                // comment. Same return type either way means it could not have.
                // (When the pass RAN and rejected all - `first_wins` - the
                // first assignable candidate is upstream's own answer and the
                // guard retires for this call.)
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
        if chosen.is_none() {
            // Overload resolution FAILED, and upstream does not gap the call
            // however it failed. §21 gated this on `arity_matched` and gave a
            // reason about upstream that was wrong: `getCandidateForOverloadFailure`
            // (`checker.go:9498`) never asks why `chooseOverload` failed. Its
            // branch is on the CANDIDATE SET — single, or generic, or has a
            // `candidatesOutArray` — and arity does not enter it. `foo()`
            // against two one-parameter overloads is `string & number = never`
            // in `compiler/functionOverloads29.types`. §199.
            //
            // `createUnionOfSignaturesForOverloadFailure`
            // (`checker.go:9581`) answers with a synthetic signature whose
            // return is the INTERSECTION of every candidate's return
            // (`checker.go:9620`) — `foo(x)` on a union no overload takes is
            // `number & string = never`. Only the return type is consumed
            // downstream, so the failure signature is candidates[0] with the
            // intersected return. See `checker-notes-callres.md` §21.
            let returns: Vec<TypeId> =
                candidates.iter().map(|candidate| candidate.r#type).collect();
            let mut failure = candidates[0].clone();
            failure.r#type = self.get_intersection_type(&returns, None);
            return Some(failure);
        }
        chosen.cloned()
    }

    /// Whether a call through this callee is an **untyped call** —
    /// `isUntypedFunctionCall` (`checker.go:9933`), its first disjunct
    /// `IsTypeAny(funcType)`; `resolveNewExpression` asks the same of the
    /// apparent type (`checker.go:8593`), which for `any` is `any`.
    ///
    /// `errorType` carries `ANY` too, but every caller upstream has already
    /// answered it through `resolveErrorCall` (`checker.go:9923`) before
    /// asking, so it is excluded here. The type-parameter and
    /// `globalFunctionType` disjuncts are not ported on this road. The
    /// diagnostic heads keep [`Checker::any_is_written_in_an_annotation`]'s
    /// provenance test: there this port's `any` for an unresolved name or a
    /// missing member stands for upstream's `errorType`, which reports nothing.
    pub(crate) fn is_untyped_call_target(&self, callee_type: TypeId) -> bool {
        callee_type != self.intrinsics.error
            && self.store.get(callee_type).flags.intersects(TypeFlags::ANY)
    }

    /// `isUntypedFunctionCall`'s third disjunct (`checker.go:9936`): a callee
    /// whose apparent type is not a union, does not reduce to `never`, has
    /// **no** call and **no** construct signatures, and is assignable to the
    /// global `Function` interface — a value typed `Function` — is an untyped
    /// call answering `any`.
    ///
    /// Both signature lists must come from a complete query: an unresolved
    /// list (`None`) is not "zero", so the arm declines rather than reading an
    /// incomplete list as empty.
    pub(crate) fn is_untyped_function_typed_callee(&mut self, callee_type: TypeId) -> bool {
        let apparent = self.apparent_type(callee_type);
        let flags = self.store.get(apparent).flags;
        if flags.intersects(TypeFlags::UNION | TypeFlags::NEVER | TypeFlags::ANY)
            || self.intersection_has_never_discriminant(apparent)
        {
            return false;
        }
        if !self
            .signatures_of_type_kind(apparent, SignatureKind::Call)
            .is_some_and(|s| s.is_empty())
            || !self
                .signatures_of_type_kind(apparent, SignatureKind::Construct)
                .is_some_and(|s| s.is_empty())
        {
            return false;
        }
        self.is_assignable_to_global_function(callee_type)
    }

    /// `isTypeAssignableTo(t, globalFunctionType)`, declining (false) when the
    /// global `Function` interface is missing or unreadable.
    fn is_assignable_to_global_function(&mut self, ty: TypeId) -> bool {
        let Some(function) = self.global_type_symbol_with_arity("Function", 0) else {
            return false;
        };
        if !self
            .binder
            .symbols()
            .get(function)
            .flags
            .intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE)
        {
            return false;
        }
        let function = self.get_declared_type_of_symbol(function);
        function != self.intrinsics.error
            && (self.is_empty_spread_object_type(function)
                || self.relate_ternary(ty, function, crate::relater::Relation::Assignable)
                    == crate::relater::Ternary::Related)
    }

    /// Whether the callee's `any` was **written** in the source, rather than
    /// produced by an unported mechanism. See [`Checker::is_untyped_call_target`].
    ///
    /// This is the whole safety property of the arm. `docs/conventions.md`'s
    /// *"a gap beats a wrong answer"*: where this port answers `any` because it
    /// could not compute something, upstream computes a real type and answering
    /// `any` manufactures a wrong line. Only an `any` the programmer wrote is a
    /// claim both compilers make.
    fn any_is_written_in_an_annotation(&mut self, callee: Expression<'_>) -> bool {
        // §297: an `as any` CAST is as written as an annotation — explicit in
        // both compilers, so the untyped-call rationale (an inferred any that
        // upstream would contextually type) cannot apply to it. The callee is
        // read through parentheses, exactly the shape `(a2 as any)()` and
        // `castFunctionExpressionShouldBeParenthesized` write; a variable
        // whose UNANNOTATED declaration is initialised by such a cast carries
        // the same written any one hop later (`var u = (a2 as any); u()`).
        let mut stripped = callee;
        while let Expression::ParenthesizedExpression(paren) = stripped {
            let Some(inner) = paren.expression else { return false };
            stripped = inner;
        }
        if Self::is_written_any_cast(stripped) {
            return true;
        }
        // §860: a call or `new` whose OWN callee was an untyped call target
        // answers upstream's `any` transitively, so the provenance is still
        // *written* — one hop further out, exactly as the cast comment above
        // treats `var u = (a2 as any); u()`.
        //
        // This keeps the narrowing's guarantee rather than weakening it: the
        // base case is unchanged (a written annotation or an `as any`), and a
        // chain only qualifies if its root does. Without it,
        // `declare const f: any; f()()` gapped while `f()` answered `any`.
        match stripped {
            Expression::CallExpression(call) => {
                return call
                    .expression
                    .is_some_and(|inner| self.any_is_written_in_an_annotation(inner));
            }
            Expression::NewExpression(new) => {
                let Some(inner) = new.expression else { return false };
                // §862: `new f(...)` where `f` lacks a construct signature is
                // upstream's own `anyType` (§861, `checker.go:8334-8342`), not
                // an unported gap — so calling that result is `any` too. This
                // is the same provenance argument as the cast hop above, with
                // a second base case.
                let inner_type = self.check_expression(inner);
                if self.new_target_lacks_a_construct_signature(inner_type) {
                    return true;
                }
                return self.any_is_written_in_an_annotation(inner);
            }
            _ => {}
        }
        let callee = stripped;
        let Expression::Identifier(identifier) = callee else { return false };
        let Some(id) = identifier.node_id else { return false };
        let Some(symbol) = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            id,
            identifier.text,
            SymbolFlags::VALUE,
        ) else {
            return false;
        };
        // §505: an alias symbol may carry no value declaration; its
        // declarations list still names the import node the shorthand test
        // below needs.
        let Some(declaration) = self
            .binder
            .symbols()
            .get(symbol)
            .value_declaration
            .or_else(|| self.binder.symbols().get(symbol).declarations.first().copied())
        else {
            return false;
        };
        let annotation = match self.node_map.get(declaration) {
            Some(tsr_ast::Node::VariableDeclaration(node)) => node.r#type,
            Some(tsr_ast::Node::ParameterDeclaration(node)) => node.r#type,
            Some(tsr_ast::Node::PropertyDeclaration(node)) => node.r#type,
            Some(tsr_ast::Node::PropertySignatureDeclaration(node)) => node.r#type,
            _ => None,
        };
        if matches!(annotation, Some(tsr_ast::TypeNode::KeywordTypeNode(k))
            if k.kind == tsr_ast::SyntaxKind::AnyKeyword)
        {
            return true;
        }
        // §299: an UNANNOTATED, UNINITIALISED variable — `declare var x;`,
        // `var x;` — is the implicit any in BOTH compilers: contextual typing
        // never reaches a bare variable declaration, so the 248-G→W
        // population this gate was narrowed against (unannotated PARAMETERS)
        // does not contain it. A call through one is upstream's untyped call
        // (`asOpEmitParens`, `typeAliasExport`).
        if let Some(tsr_ast::Node::VariableDeclaration(node)) = self.node_map.get(declaration)
            && node.r#type.is_none()
            && node.initializer.is_none()
        {
            return true;
        }
        // §505: an import from a SHORTHAND ambient module
        // (`declare module "jquery"` — no body) is `any` in BOTH compilers:
        // `isShorthandAmbientModuleSymbol` (`internal/checker/utilities.go:198`)
        // makes upstream answer the module symbol for every member, and each
        // use reads `any`. This is upstream's own computed any, not the
        // positional refusal's manufactured one, so a call through it is an
        // untyped call (`conformance/ambientShorthand`'s
        // `foo(bar, baz, boom) : any`).
        {
            let import = match self.node_map.get(declaration) {
                Some(tsr_ast::Node::ImportSpecifier(_)) => self
                    .nodes
                    .parent(declaration)
                    .and_then(|named| self.nodes.parent(named))
                    .and_then(|clause| self.nodes.parent(clause)),
                Some(tsr_ast::Node::ImportClause(_)) => self.nodes.parent(declaration),
                Some(tsr_ast::Node::NamespaceImport(_)) => {
                    self.nodes.parent(declaration).and_then(|clause| self.nodes.parent(clause))
                }
                Some(tsr_ast::Node::ImportEqualsDeclaration(_)) => Some(declaration),
                _ => None,
            };
            let specifier = import.and_then(|node| match self.node_map.get(node) {
                Some(tsr_ast::Node::ImportDeclaration(import)) => {
                    import.module_specifier.and_then(|s| tsr_ast::Node::from(s).node_id())
                }
                Some(tsr_ast::Node::ImportEqualsDeclaration(import)) => {
                    match import.module_reference {
                        Some(tsr_ast::ModuleReference::ExternalModuleReference(external)) => {
                            external.expression.and_then(|e| e.node_id())
                        }
                        _ => None,
                    }
                }
                _ => None,
            });
            if let Some(specifier) = specifier
                && let Some(module) = self.resolve_external_module_name(declaration, specifier)
                && self.is_shorthand_ambient_module(module)
            {
                return true;
            }
        }
        // §297's one-hop half: `var u = (a2 as any);` — unannotated, the
        // initialiser IS the written cast.
        if annotation.is_none()
            && let Some(tsr_ast::Node::VariableDeclaration(node)) = self.node_map.get(declaration)
            && let Some(mut initializer) = node.initializer
        {
            while let Expression::ParenthesizedExpression(paren) = initializer {
                let Some(inner) = paren.expression else { return false };
                initializer = inner;
            }
            return Self::is_written_any_cast(initializer);
        }
        false
    }

    /// §297: `expr as any` or `<any>expr`, the two written-cast spellings.
    fn is_written_any_cast(expression: Expression<'_>) -> bool {
        let cast_type = match expression {
            Expression::AsExpression(node) => node.r#type,
            Expression::TypeAssertion(node) => node.r#type,
            _ => return false,
        };
        matches!(cast_type, Some(tsr_ast::TypeNode::KeywordTypeNode(k))
            if k.kind == tsr_ast::SyntaxKind::AnyKeyword)
    }
}

impl Checker<'_, '_> {
    /// §273: upstream's SUBTYPE pass (`chooseOverload`'s first run,
    /// `checker.go:8924`) over a CLEAN candidate prefix — the candidates
    /// before the first generic / this-parameter / rest / any-parameter one.
    /// A winner here is upstream's certain answer whatever follows: pass one
    /// walks candidates in order and never reaches the tail once a prefix
    /// candidate relates, and every earlier candidate rejected the same way.
    ///
    /// The subtype WALK was never hardened the way the assignable one was
    /// when `SELECTABLE` retired: its structural half is shared with
    /// `Assignable` and answers a confident Related for object pairs whose
    /// members it cannot see — the third draft picked `(i: C): C` for an `I`
    /// argument that way (`symbolProperty13`; upstream says I is NOT a
    /// subtype of C and takes the `any` overload). So this pass trusts only
    /// the domains where the simple arms decide: pairs whose two sides are
    /// both primitive-like, literal, nullish or any-like. Anything wider is
    /// Unknown by fiat and the pick declines.
    pub(crate) fn subtype_pass_prefix_pick(
        &mut self,
        candidates: &[Signature],
        clean_len: usize,
        arguments: &[Expression<'_>],
    ) -> Option<Signature> {
        match self.subtype_pass_outcome(candidates, clean_len, arguments) {
            SubtypePassOutcome::Picked(signature) => Some(*signature),
            _ => None,
        }
    }

    /// SS339: the tri-state the untruncated road needs - a pass that
    /// DECIDABLY rejected every candidate licenses upstream's pass-three
    /// first-wins walk; anything undecidable keeps the conservative
    /// ambiguity guard.
    fn subtype_pass_outcome(
        &mut self,
        candidates: &[Signature],
        clean_len: usize,
        arguments: &[Expression<'_>],
    ) -> SubtypePassOutcome {
        // Callers pass candidates in `reorderCandidates` order
        // (`checker.go:8957`, [`Checker::reorder_candidates`]): specialized
        // (literal-typed) signatures first, later merged declaration groups
        // ahead of earlier ones. So the first-match walk here is upstream's,
        // and the old declaration-order guard that declined every set with a
        // specialized candidate is gone (`docs/parity/notes/calls-inference.md`
        // §3).
        let prefix = &candidates[..clean_len];
        // `isSignatureApplicable` checks each argument under the candidate's
        // parameter type; the first arity-matching candidate's check is the
        // argument's first, so it is made under that context.
        let first = candidates.iter().find(|c| has_correct_arity(c, arguments.len()));
        let call = self.call_for_overload_arguments(arguments);
        let mut argument_types = Vec::with_capacity(arguments.len());
        for &argument in arguments {
            if matches!(argument, Expression::SpreadElement(_)) {
                return SubtypePassOutcome::Undecidable;
            }
            argument_types.push(self.check_argument_in_candidate_context(call, first, argument));
        }
        let all_decidable = clean_len == candidates.len();
        for candidate in prefix {
            if !has_correct_arity(candidate, argument_types.len()) {
                continue;
            }
            let mut verdict = Ternary::Related;
            for (&argument, parameter) in argument_types.iter().zip(&candidate.parameters) {
                let parameter_type = self.parameter_type(parameter);
                match self.relate_ternary(argument, parameter_type, Relation::Subtype) {
                    Ternary::NotRelated => {
                        verdict = Ternary::NotRelated;
                        break;
                    }
                    Ternary::Unknown => verdict = Ternary::Unknown,
                    Ternary::Related => {}
                }
            }
            match verdict {
                Ternary::Unknown => return SubtypePassOutcome::Undecidable,
                Ternary::NotRelated => {}
                // Upstream's pass one picks the FIRST subtype-related
                // candidate; the tail never gets a turn.
                Ternary::Related => return SubtypePassOutcome::Picked(Box::new(candidate.clone())),
            }
        }
        if all_decidable {
            SubtypePassOutcome::AllRejected
        } else {
            SubtypePassOutcome::Undecidable
        }
    }

    /// `checkExpressionWithContextualType(arg, paramType, nil, checkMode)`
    /// as `isSignatureApplicable` (`checker.go:9256`) calls it for a
    /// non-generic candidate, when that call is the argument's first check.
    ///
    /// Upstream checks every argument under each candidate's parameter type;
    /// what an argument resolves inside that check (its own call's
    /// `resolvedSignature`, a variance measured by contextual return
    /// inference) is cached by the first candidate's check and reused by every
    /// later one. This port caches the argument's type in `node_types`, so
    /// only the first check is made under a context: it publishes `candidate`
    /// as the call's memo ([`Checker::call_inference_signatures`], read by
    /// `contextual_type_for_argument`) for the duration of one
    /// `check_expression`. An already-checked argument, a context-sensitive
    /// one (the walk's retention owns those), a generic candidate (its
    /// context is inference's), a synthesized argument list, and a call
    /// whose memo is already set keep the context-free `check_expression`.
    fn check_argument_in_candidate_context(
        &mut self,
        call: Option<tsr_ast::NodeId>,
        candidate: Option<&Signature>,
        argument: Expression<'_>,
    ) -> TypeId {
        if let Some(candidate) = candidate
            && candidate.type_parameters.is_empty()
            && argument.node_id().is_some_and(|id| !self.node_types.contains_key(&id))
            && !self.is_context_sensitive_argument(&argument)
            && let Some(call) = call
            && !self.call_inference_signatures.contains_key(&call)
        {
            self.call_inference_signatures.insert(call, candidate.clone());
            let checked = self.check_expression(argument);
            self.call_inference_signatures.remove(&call);
            return checked;
        }
        self.check_expression(argument)
    }

    /// §273's admission test, shared with the `new`-expression road: the
    /// candidates before the first one whose selection would need machinery
    /// this port does not trust here.
    pub(crate) fn clean_candidate_prefix_len(candidates: &[Signature]) -> usize {
        candidates
            .iter()
            .position(|candidate| {
                // §335 removed the fourth disjunct — an `any`-PARAMETER
                // candidate: the SUBTYPE pass decides it (everything is a
                // subtype of `any`), which is exactly how upstream's pass one
                // picks `(bar: any): number` for `foo(5)` behind a failing
                // `(bar: string)` (`functionOverloads33`). The exclusion
                // dated from §273's FIRST draft, whose damage came from
                // running the assignable pass first, not from the candidate.
                !candidate.type_parameters.is_empty() || candidate.this_parameter.is_some()
            })
            .unwrap_or(candidates.len())
    }
}

/// SS339: what the subtype pass concluded about a candidate set.
enum SubtypePassOutcome {
    /// A candidate matched pass one; upstream's certain answer.
    ///
    /// Boxed: a `Signature` is the only variant carrying data and it dwarfs the
    /// other two, which `clippy::large_enum_variant` refuses once §926 widened
    /// `TypePredicate`.
    Picked(Box<Signature>),
    /// Every arity-matching candidate was DECIDABLY rejected - pass three's
    /// first-wins walk is licensed.
    AllRejected,
    /// A pair was undecidable, a candidate shape was outside the domain, or
    /// the set held a specialized signature - nothing below may trust it.
    Undecidable,
}

/// Whether a signature accepts exactly this many arguments.
///
/// Ported from `Checker.hasCorrectArity` (`checker.go:9107`), reduced to the two
/// bounds that survive once rest parameters, spread arguments and the
/// signature-help trailing comma are excluded by
/// [`Checker::choose_overload`]: at least the required parameters, at most all
/// of them.
/// One pass of the §487 walk's verdict.
enum OverloadPass {
    /// Boxed for the same reason as [`SubtypePassOutcome::Picked`].
    Picked(Box<Signature>),
    /// Every candidate rejected; carries the pass's
    /// `candidatesForArgumentError`.
    AllRejected(OverloadArgumentFailure),
    Undecidable,
}

/// `CallState.candidatesForArgumentError` (`checker.go:8838`) after the
/// §487 walk's final pass: `chooseOverload` (`checker.go:9025`) resets it at
/// each pass entry and appends every arity-matching candidate (instantiated
/// when generic) that `isSignatureApplicable` rejected. Only what
/// `reportCallResolutionErrors` (`checker.go:9649`) reads is kept: the last
/// entry and the length.
///
/// Published per call node in `Checker::overload_argument_failures` by
/// [`Checker::transcribed_generic_set_walk`] when both passes reject every
/// candidate; the walk removes the entry when it picks or declines. No work
/// happens on publication: the signature and argument types are the walk's
/// own verdict data.
#[derive(Clone, Default)]
pub(crate) struct OverloadArgumentFailure {
    /// The last rejected candidate, as checked. `None` when the last
    /// rejection did not go through `isSignatureApplicable` here (the
    /// missing-array-member pre-inference skip), so no report is upstream's.
    pub(crate) last: Option<Box<Signature>>,
    /// `len(candidatesForArgumentError)`.
    pub(crate) count: usize,
    /// Each argument's type as checked under `last`'s parameter as
    /// contextual type (`checkExpressionWithContextualType`), where the walk
    /// re-checked it; `None` where the walk read the context-free type.
    pub(crate) checked: Vec<Option<TypeId>>,
}

impl OverloadArgumentFailure {
    fn push(&mut self, last: Option<Signature>, checked: Vec<Option<TypeId>>) {
        self.last = last.map(Box::new);
        self.count += 1;
        self.checked = checked;
    }
}

impl Checker<'_, '_> {
    /// §487 — upstream's `chooseOverload` loop (`checker.go:9040-9104`)
    /// transcribed for the candidate sets the ladder above DECLINES (a
    /// generic candidate in the set): per candidate in order — arity, infer
    /// type arguments, instantiate (`check_generic_call_with`'s out-slot),
    /// applicability under the pass's relation — first success wins, run as
    /// upstream's two `resolveCall` passes (subtype, then assignable,
    /// `checker.go:8922-8928`). Both passes rejecting everything DECIDABLY is
    /// upstream's overload failure, which still answers a candidate:
    /// `pickLongestCandidateSignature` (`checker.go:9510`).
    ///
    /// Nongeneric candidates retain the first checked callback context across
    /// later candidates and both relations. Generic inference still uses fresh
    /// speculative caches. An unsupported walk restores previous caches so the
    /// recovery path cannot observe a rejected candidate.
    /// Receiver types and effective array/tuple rest positions participate in
    /// applicability. Spreads, written type arguments, unresolved non-array
    /// rests, undecidable inference and unknown relations remain unsupported.
    fn transcribed_generic_set_walk(
        &mut self,
        candidates: &[Signature],
        arguments: &[Expression<'_>],
        call: Option<tsr_ast::NodeId>,
    ) -> Option<Signature> {
        let mut stack: Vec<_> = arguments
            .iter()
            .filter(|argument| {
                self.is_context_sensitive_argument(argument)
                    || matches!(argument, Expression::ArrayLiteralExpression(_))
                    || (candidates.iter().all(|candidate| candidate.type_parameters.is_empty())
                        && matches!(argument, Expression::ObjectLiteralExpression(_)))
            })
            .filter_map(tsr_ast::Expression::node_id)
            .collect();
        let mut cached = Vec::new();
        while let Some(id) = stack.pop() {
            let symbol = self.binder.symbol_of(id);
            cached.push((
                id,
                self.node_types.get(&id).copied(),
                self.resolved_call_signatures.get(&id).cloned(),
                symbol.map(|symbol| (symbol, self.symbol_types.get(&symbol).copied())),
            ));
            if let Some(node) = self.node_map.get(id) {
                tsr_ast::for_each_child_id(node, |child| stack.push(child));
            }
        }
        let result = self.transcribed_generic_set_walk_worker(candidates, arguments, call);
        for argument in arguments {
            if let Some(id) = argument.node_id() {
                self.context_checked_arguments.remove(&id);
            }
        }
        if result.is_none() {
            for (id, ty, signature, symbol) in cached {
                self.node_types.remove(&id);
                if let Some(ty) = ty {
                    self.node_types.insert(id, ty);
                }
                self.resolved_call_signatures.remove(&id);
                if let Some(signature) = signature {
                    self.resolved_call_signatures.insert(id, signature);
                }
                if let Some((symbol, ty)) = symbol {
                    self.symbol_types.remove(&symbol);
                    if let Some(ty) = ty {
                        self.symbol_types.insert(symbol, ty);
                    }
                }
            }
        }
        result
    }

    fn transcribed_generic_set_walk_worker(
        &mut self,
        candidates: &[Signature],
        arguments: &[Expression<'_>],
        call: Option<tsr_ast::NodeId>,
    ) -> Option<Signature> {
        if arguments.iter().any(|a| matches!(a, Expression::SpreadElement(_))) {
            return None;
        }
        let call = call.or_else(|| self.call_for_overload_arguments(arguments));
        if let Some(call) = call {
            self.overload_argument_failures.remove(&call);
        }
        let contextual = arguments.iter().any(|a| self.is_context_sensitive_argument(a));
        if contextual && call.is_none() {
            return None;
        }
        if call.is_none() && candidates.iter().any(|c| c.this_parameter.is_some()) {
            return None;
        }
        // Native checker.go:19836/10166 publishes original callable shape
        // before body demand. No overload is selected here: every candidate's
        // argument position must certify a completed absent contextual call
        // signature. Unsupported/unfinished lookups and mapper frames decline.
        if self.alias_evaluation_bindings.is_empty() && self.mapped_template_depth == 0 {
            for (index, argument) in arguments.iter().enumerate() {
                if !matches!(
                    argument,
                    Expression::ArrowFunction(_) | Expression::FunctionExpression(_)
                ) {
                    continue;
                }
                let Some(declaration) = argument.node_id() else { continue };
                let absent = !candidates.is_empty()
                    && candidates.iter().all(|candidate| {
                        let Some(contextual) =
                            self.contextual_argument_type(candidate, index, arguments.len())
                        else {
                            return false;
                        };
                        let contextual =
                            if self.store.get(contextual).flags.contains(TypeFlags::TYPE_PARAMETER)
                            {
                                let Some(constraint) = self.base_constraint_of_type(contextual)
                                else {
                                    return false;
                                };
                                constraint
                            } else {
                                contextual
                            };
                        let contextual = self.apparent_type(contextual);
                        matches!(
                            self.contextual_call_signature(contextual, Some(declaration)),
                            Some(crate::contextual::ContextualSignature::Absent)
                        )
                    });
                if absent {
                    self.prepare_uncontextual_callable(declaration);
                }
            }
        }
        let argument_types: Vec<TypeId> = arguments
            .iter()
            .map(|&argument| {
                if self.is_context_sensitive_argument(&argument) {
                    self.intrinsics.error
                } else {
                    self.check_expression(argument)
                }
            })
            .collect();
        if argument_types.iter().zip(arguments).any(|(&ty, argument)| {
            ty == self.intrinsics.error && !self.is_context_sensitive_argument(argument)
        }) {
            return None;
        }
        let mut checked_contexts = vec![None; arguments.len()];
        let mut failures = OverloadArgumentFailure::default();
        for relation in [Relation::Subtype, Relation::Assignable] {
            match self.overload_pass(
                candidates,
                arguments,
                &argument_types,
                relation,
                call,
                &mut checked_contexts,
            ) {
                OverloadPass::Picked(signature) => return Some(*signature),
                // The final pass's list is the one reported.
                OverloadPass::AllRejected(rejected) => failures = rejected,
                OverloadPass::Undecidable => return None,
            }
        }
        if let Some(call) = call
            && failures.count != 0
        {
            self.overload_argument_failures.insert(call, failures);
        }
        // `getCandidateForOverloadFailure` (`checker.go:9498`) with a generic
        // in the set → `pickLongestCandidateSignature` (`:9510`):
        // `getLongestCandidateIndex` (`:9545`) is the first candidate whose
        // parameter count covers the arguments or has a rest, else the longest.
        if candidates.len() > 1
            && candidates.iter().all(|candidate| candidate.type_parameters.is_empty())
        {
            return self.union_signature_for_overload_failure(candidates);
        }
        let best_index = self.longest_candidate_index(candidates, arguments.len());
        let best = &candidates[best_index];
        if best.type_parameters.is_empty() {
            return Some(best.clone());
        }
        let mut instantiated = None;
        let _ = self.check_generic_call_with(best, call, arguments, Some(&mut instantiated));
        instantiated
    }

    fn longest_candidate_index(
        &mut self,
        candidates: &[Signature],
        argument_count: usize,
    ) -> usize {
        candidates
            .iter()
            .position(|c| {
                self.signature_has_effective_rest(c)
                    || self.signature_parameter_count(c) >= argument_count
            })
            .unwrap_or_else(|| {
                let mut best = 0;
                for (index, candidate) in candidates.iter().enumerate() {
                    if self.signature_parameter_count(candidate)
                        > self.signature_parameter_count(&candidates[best])
                    {
                        best = index;
                    }
                }
                best
            })
    }

    /// `getCandidateForOverloadFailure` (`checker.go:9498`) for the failure
    /// this port can decide before choosing: the call's written type
    /// arguments fit no candidate (`hasCorrectTypeArgumentArity`,
    /// `checker.go:9214`), so `chooseOverload` rejects every one. A
    /// non-generic overload set is combined
    /// (`createUnionOfSignaturesForOverloadFailure`); otherwise
    /// `pickLongestCandidateSignature` (`:9510`) instantiates the longest
    /// candidate with `getTypeArgumentsFromNodes` (`:9557`): the written
    /// arguments truncated to its type parameters, the tail filled with each
    /// parameter's default, else its constraint, else `unknown`
    /// (`new C<Date, Date>()` on `class C<T>` is `C<Date>`).
    ///
    /// `call` is the `new` or call node, read through `node_map` for the
    /// written argument nodes. `None` when some candidate admits the written
    /// count (the ordinary resolution owns that call) or the candidate cannot
    /// be instantiated.
    pub(crate) fn written_type_argument_arity_failure(
        &mut self,
        candidates: &[Signature],
        call: tsr_ast::NodeId,
        argument_count: usize,
    ) -> Option<Signature> {
        let written = match self.node_map.get(call)? {
            tsr_ast::Node::NewExpression(node) => node.type_arguments,
            tsr_ast::Node::CallExpression(node) => node.type_arguments,
            _ => return None,
        };
        if written.is_empty()
            || candidates.is_empty()
            || candidates.iter().any(|candidate| {
                written.len() >= Self::min_type_argument_count(&candidate.type_parameters)
                    && written.len() <= candidate.type_parameters.len()
            })
        {
            return None;
        }
        if candidates.len() > 1
            && candidates.iter().all(|candidate| candidate.type_parameters.is_empty())
        {
            return self.union_signature_for_overload_failure(candidates);
        }
        let best = candidates[self.longest_candidate_index(candidates, argument_count)].clone();
        if best.type_parameters.is_empty() {
            return Some(best);
        }
        let parameters = self.type_parameter_types(&best)?;
        let mut arguments: Vec<TypeId> = written
            .iter()
            .take(parameters.len())
            .map(|&argument| self.get_type_from_type_node(argument))
            .collect();
        while let Some(&parameter) = parameters.get(arguments.len()) {
            let filled = match self.get_default_from_type_parameter(parameter) {
                Some(default) => default,
                None => {
                    self.type_parameter_constraint(parameter).unwrap_or(self.intrinsics.unknown)
                }
            };
            arguments.push(filled);
        }
        let names: Vec<String> =
            best.type_parameters.iter().map(|parameter| parameter.name.clone()).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let map: Vec<(TypeId, TypeId)> = parameters.iter().copied().zip(arguments).collect();
        let mut instance = best;
        instance.type_parameters.clear();
        self.instantiate_signature(instance, &map, &parameters, &names)
    }

    /// The containing call for an actual argument list. Synthesized argument
    /// lists (constructors or template substitutions) have no such context.
    fn call_for_overload_arguments(&self, arguments: &[Expression<'_>]) -> Option<tsr_ast::NodeId> {
        let parent = self.nodes.parent(arguments.first()?.node_id()?)?;
        let Some(tsr_ast::Node::CallExpression(call)) = self.node_map.get(parent) else {
            return None;
        };
        (call.arguments.len() == arguments.len()
            && call.arguments.iter().zip(arguments).all(|(a, b)| a.node_id() == b.node_id()))
        .then_some(parent)
    }

    /// hasCorrectArity (checker.go:9110) for complete calls without spreads.
    /// Tuple rests contribute their effective fixed positions and minimum.
    fn overload_has_correct_arity(&mut self, signature: &Signature, count: usize) -> bool {
        (self.signature_has_effective_rest(signature)
            || count <= self.signature_parameter_count(signature))
            && count >= self.signature_min_argument_count(signature)
    }

    /// One candidate walk under one relation — `chooseOverload`'s loop body
    /// with `isSignatureApplicable` (`checker.go:9256`) reduced to the
    /// argument relation this port can ask, Kleene-honest.
    fn overload_pass(
        &mut self,
        candidates: &[Signature],
        arguments: &[Expression<'_>],
        argument_types: &[TypeId],
        relation: Relation,
        call: Option<tsr_ast::NodeId>,
        checked_contexts: &mut [Option<TypeId>],
    ) -> OverloadPass {
        // `chooseOverload` resets `candidatesForArgumentError` at entry.
        let mut failures = OverloadArgumentFailure::default();
        let retain_context =
            candidates.iter().all(|candidate| candidate.type_parameters.is_empty());
        // A call's array-literal argument checked under a candidate's
        // context (`checkExpressionWithContextualType`, uncached upstream)
        // gets its published type back unless that candidate is picked:
        // upstream's printed type is the literal's check outside a rejected
        // candidate's context. A `new` keeps the candidate's answer (its
        // overloads publish it). Restored before each next candidate.
        let is_new =
            call.is_some_and(|call| self.nodes.kind(call) == tsr_ast::SyntaxKind::NewExpression);
        let mut published: Vec<(tsr_ast::NodeId, TypeId)> = Vec::new();
        'candidate: for candidate in candidates {
            for &(id, ty) in &published {
                self.node_types.insert(id, ty);
            }
            if !self.overload_has_correct_arity(candidate, argument_types.len()) {
                continue;
            }
            let call = call.or_else(|| self.call_for_overload_arguments(arguments));
            let contextual =
                arguments.iter().any(|argument| self.is_context_sensitive_argument(argument));
            if contextual {
                // A context-sensitive argument is checked under a candidate
                // once (`NodeCheckFlagsContextChecked`, `checker.go:10155`);
                // only an argument no candidate has checked yet loses its
                // context-free answer.
                for (index, argument) in arguments.iter().enumerate() {
                    if self.is_context_sensitive_argument(argument)
                        && checked_contexts[index].is_none()
                        && let Some(id) = argument.node_id()
                    {
                        self.evict_subtree(id);
                    }
                }
            }
            let concrete: Signature = if candidate.type_parameters.is_empty() {
                candidate.clone()
            } else {
                // A missing required array member rejects every element-type
                // instantiation before inference. Identity alone is not proof:
                // an empty global Array<T> can accept primitives under noLib.
                if argument_types.iter().enumerate().any(|(index, &argument)| {
                    let flags = self.store.get(argument).flags;
                    if self.is_context_sensitive_argument(&arguments[index])
                        || !flags.intersects(TypeFlags::PRIMITIVE)
                        || flags.intersects(
                            TypeFlags::NULLABLE | TypeFlags::UNION | TypeFlags::ANY_OR_UNKNOWN,
                        )
                    {
                        return false;
                    }
                    let Some(parameter) = self.signature_type_at_position(candidate, index) else {
                        return false;
                    };
                    if self.signature_array_element(parameter).is_none() {
                        return false;
                    }
                    let apparent = self.apparent_type(argument);
                    if self.get_property_names_of_type(apparent).is_none() {
                        return false;
                    }
                    self.get_property_names_of_type(parameter).is_some_and(|names| {
                        names.iter().any(|name| {
                            self.get_property_of_type(parameter, name)
                                .is_some_and(|property| !self.property_is_optional(property))
                                && self.get_property_of_type(apparent, name).is_none()
                        })
                    })
                }) {
                    // Upstream instantiates and rejects it in
                    // `isSignatureApplicable`; that instantiation is not
                    // computed here, so it is no reportable last entry.
                    failures.push(None, Vec::new());
                    continue;
                }
                // inferTypeArguments re-checks every argument with this
                // candidate's parameter type as context; an array literal's
                // tuple-ness depends on it, so a previous candidate's (or the
                // context-free) answer must not be reused. On a call this is
                // done for an empty literal only: re-checking elements under a
                // candidate's context re-enters their own calls' resolution,
                // whose published answers this walk does not undo
                // (`tupleTypeInference`'s `[$q.when<string>(), ...]`).
                for argument in arguments {
                    if let Expression::ArrayLiteralExpression(literal) = argument
                        && (is_new || literal.elements.is_empty())
                        && let Some(id) = argument.node_id()
                    {
                        self.publish_array_argument(is_new, id, &mut published);
                        self.evict_subtree(id);
                    }
                }
                let mut instantiated = None;
                let _ = self.check_generic_call_with(
                    candidate,
                    call,
                    arguments,
                    Some(&mut instantiated),
                );
                let Some(signature) = instantiated else {
                    // Inference could not decide this candidate, so no later
                    // candidate may be trusted against it.
                    return OverloadPass::Undecidable;
                };
                signature
            };
            // chooseOverload rechecks arity after instantiating a non-array
            // rest parameter (checker.go:9067).
            if !self.overload_has_correct_arity(&concrete, arguments.len()) {
                continue;
            }
            // A still-generic non-array rest requires getSpreadArgumentType.
            // Do not silently compare only its fixed prefix.
            if self.signature_non_array_rest_type(&concrete).is_some() {
                return OverloadPass::Undecidable;
            }
            // isSignatureApplicable checks the call receiver before arguments.
            // A definite argument mismatch still rejects a candidate when the
            // receiver comparison is unsupported. Unknown must prevent choosing
            // the candidate, but must not hide an independently known failure.
            let mut receiver_relation_unknown = false;
            if let Some(parameter) = &concrete.this_parameter
                && self.parameter_type(parameter) != self.intrinsics.void
            {
                let Some(call) = call else { return OverloadPass::Undecidable };
                let receiver = self.this_argument_type_of_call(Some(call));
                let parameter_type = self.parameter_type(parameter);
                match self.relate_ternary(receiver, parameter_type, relation) {
                    Ternary::NotRelated => {
                        failures.push(Some(concrete.clone()), vec![None; arguments.len()]);
                        continue;
                    }
                    Ternary::Unknown => receiver_relation_unknown = true,
                    Ternary::Related => {}
                }
            }
            let mut parameter_types = Vec::with_capacity(arguments.len());
            for index in 0..arguments.len() {
                let Some(parameter) = self.signature_type_at_position(&concrete, index) else {
                    return OverloadPass::Undecidable;
                };
                if self.is_error(parameter) {
                    return OverloadPass::Undecidable;
                }
                parameter_types.push(parameter);
            }
            // isSignatureApplicable checks each argument with the instantiated
            // parameter's context. Even an array with no context-sensitive
            // elements can acquire a tuple type at this point. Ordinary objects
            // enter only a wholly nongeneric set: generic inference owns their
            // contextual checks, and a preceding nongeneric candidate must not
            // reopen callable returns before that inference.
            let contextual_arguments: Vec<_> = arguments
                .iter()
                .enumerate()
                .map(|(index, argument)| {
                    self.is_context_sensitive_argument(argument)
                        || (retain_context
                            && matches!(argument, Expression::ObjectLiteralExpression(_)))
                        || (matches!(argument, Expression::ArrayLiteralExpression(_))
                            && (retain_context
                                || self.tuple_element_lists.contains_key(&parameter_types[index])
                                || self
                                    .variadic_tuple_elements
                                    .contains_key(&parameter_types[index])))
                })
                .collect();
            let contextual = contextual_arguments.iter().any(|&needed| needed);
            // SkipContextSensitive (`chooseOverload`'s `argCheckMode`,
            // `checker.go:9025`): while no context-sensitive argument has been
            // checked under a candidate, a candidate is rejected from its
            // ordinary arguments before any callback parameter types are
            // assigned; the first candidate that passes switches the walk to
            // the normal mode and its context check is retained
            // (NodeCheckFlagsContextChecked, checker.go:10155). A generic
            // candidate's skipped arguments are `anyFunctionType`, which
            // relates; its instantiation is the full inference's.
            let skip_mode = checked_contexts.iter().all(Option::is_none)
                && if candidate.type_parameters.is_empty() {
                    contextual
                } else {
                    arguments.iter().any(|argument| self.is_context_sensitive_argument(argument))
                };
            if skip_mode {
                for (index, &argument) in argument_types.iter().enumerate() {
                    let argument = if self.is_context_sensitive_argument(&arguments[index]) {
                        if !candidate.type_parameters.is_empty() {
                            continue;
                        }
                        let Some(skipped) =
                            self.context_free_object_inference_type(arguments[index])
                        else {
                            return OverloadPass::Undecidable;
                        };
                        skipped
                    } else if contextual_arguments[index] {
                        // An object or tuple-context array is checked under
                        // its candidate, not its context-free widened type.
                        continue;
                    } else {
                        argument
                    };
                    match self.relate_ternary(argument, parameter_types[index], relation) {
                        Ternary::NotRelated => {
                            failures.push(Some(concrete), vec![None; arguments.len()]);
                            continue 'candidate;
                        }
                        Ternary::Unknown => return OverloadPass::Undecidable,
                        Ternary::Related => {}
                    }
                }
            }
            if skip_mode && !candidate.type_parameters.is_empty() {
                // The candidate passed the skipped check, so inference's
                // context check of its context-sensitive arguments is the
                // one later candidates and the next relation reuse rather
                // than re-assigning parameter types
                // (`contextuallyCheckFunctionExpressionOrObjectLiteralMethod`).
                for (index, argument) in arguments.iter().enumerate() {
                    if self.is_context_sensitive_argument(argument)
                        && let Some(id) = argument.node_id()
                        && let Some(&checked) = self.node_types.get(&id)
                    {
                        checked_contexts[index] = Some(checked);
                        self.context_checked_arguments.insert(id);
                    }
                }
            }
            let mut checked_arguments = argument_types.to_vec();
            if contextual {
                let Some(call) = call else { return OverloadPass::Undecidable };
                if self.call_inference_signatures.contains_key(&call) {
                    return OverloadPass::Undecidable;
                }
                self.call_inference_signatures.insert(call, concrete.clone());
                for (index, argument) in arguments.iter().enumerate() {
                    if contextual_arguments[index] {
                        // Literal-only objects lack NodeCheckFlagsContextChecked
                        // (5b1047d checker.go:10155). Their literal context can
                        // differ between candidates, so only the existing
                        // context-sensitive/array retention may reuse a result;
                        // a context-sensitive argument keeps its first check
                        // under every candidate, generic or not.
                        let retain_argument_context = self.is_context_sensitive_argument(argument)
                            || (retain_context
                                && !matches!(argument, Expression::ObjectLiteralExpression(_)));
                        if retain_argument_context && let Some(checked) = checked_contexts[index] {
                            checked_arguments[index] = checked;
                            continue;
                        }
                        if let Some(id) = argument.node_id() {
                            if !retain_context
                                && matches!(argument, Expression::ArrayLiteralExpression(literal)
                                    if literal.elements.is_empty())
                            {
                                self.publish_array_argument(is_new, id, &mut published);
                            }
                            self.evict_subtree(id);
                        }
                        let checked = self.check_expression(*argument);
                        checked_arguments[index] = checked;
                        if retain_argument_context {
                            checked_contexts[index] = Some(checked);
                            if self.is_context_sensitive_argument(argument)
                                && let Some(id) = argument.node_id()
                            {
                                self.context_checked_arguments.insert(id);
                            }
                        }
                    }
                }
                self.call_inference_signatures.remove(&call);
                if checked_arguments.contains(&self.intrinsics.error) {
                    return OverloadPass::Undecidable;
                }
            }
            let mut verdict =
                if receiver_relation_unknown { Ternary::Unknown } else { Ternary::Related };
            for (&argument, &parameter) in checked_arguments.iter().zip(&parameter_types) {
                // Non-strict `undefined`/`null` inhabit every domain — the
                // same skip the ladder's loops carry.
                if !self.strict_null_checks
                    && self
                        .type_of(argument)
                        .flags
                        .intersects(TypeFlags::UNDEFINED | TypeFlags::NULL)
                    && !self
                        .type_of(argument)
                        .flags
                        .intersects(!(TypeFlags::UNDEFINED | TypeFlags::NULL))
                {
                    continue;
                }
                match self.relate_ternary(argument, parameter, relation) {
                    Ternary::NotRelated => {
                        verdict = Ternary::NotRelated;
                        break;
                    }
                    Ternary::Unknown => verdict = Ternary::Unknown,
                    Ternary::Related => {}
                }
            }
            match verdict {
                Ternary::Unknown => return OverloadPass::Undecidable,
                Ternary::NotRelated => {
                    let checked = checked_arguments
                        .iter()
                        .zip(&contextual_arguments)
                        .map(|(&ty, &needed)| needed.then_some(ty))
                        .collect();
                    failures.push(Some(concrete), checked);
                }
                Ternary::Related => {
                    // A literal the picked instantiation did not re-check
                    // under its own context keeps its published type, not
                    // inference's context.
                    for (index, argument) in arguments.iter().enumerate() {
                        if !contextual_arguments[index]
                            && let Some(id) = argument.node_id()
                            && let Some(&(_, ty)) = published.iter().find(|&&(seen, _)| seen == id)
                        {
                            self.node_types.insert(id, ty);
                        }
                    }
                    return OverloadPass::Picked(Box::new(concrete));
                }
            }
        }
        for (id, ty) in published {
            self.node_types.insert(id, ty);
        }
        OverloadPass::AllRejected(failures)
    }

    /// Records an array-literal argument's published type, once per walk,
    /// for [`Checker::overload_pass`]'s restore.
    fn publish_array_argument(
        &self,
        is_new: bool,
        id: tsr_ast::NodeId,
        published: &mut Vec<(tsr_ast::NodeId, TypeId)>,
    ) {
        if !is_new
            && !published.iter().any(|&(seen, _)| seen == id)
            && let Some(&ty) = self.node_types.get(&id)
        {
            published.push((id, ty));
        }
    }
}

fn has_correct_arity(candidate: &Signature, argument_count: usize) -> bool {
    // §588 EXPERIMENT: upstream's rest arm (`checker.go:9162`) is exactly
    // *"no upper bound when the signature has an effective rest parameter"*,
    // and a rest parameter is never itself required.
    let has_rest = candidate.parameters.iter().any(|p| p.rest);
    let required = candidate.parameters.iter().take_while(|p| !p.optional && !p.rest).count();
    if argument_count < required {
        return false;
    }
    has_rest || argument_count <= candidate.parameters.len()
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
            &arena,
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

    #[test]
    fn explicit_original_calls_publish_distinct_signatures_without_inferred_admission() {
        let source = "declare function known<T>(value: T): T;
            known<number>(17); known<string>('east'); known(23 + 1); const probe = 0;";
        for reverse in [false, true] {
            with_declared_variable(source, "probe", |checker, _| {
                let mut calls: Vec<_> = (0..checker.nodes.len())
                    .filter_map(|index| {
                        let id = tsr_ast::NodeId::new(u32::try_from(index).unwrap());
                        match checker.node_map.get(id) {
                            Some(tsr_ast::Node::CallExpression(call)) => Some((id, call)),
                            _ => None,
                        }
                    })
                    .collect();
                assert_eq!(calls.len(), 3);
                if reverse {
                    calls.reverse();
                }
                let mut completed = Vec::new();
                for stage in ["worker", "repeat"] {
                    for (id, call) in &calls {
                        let expected =
                            if matches!(call.arguments[0], tsr_ast::Expression::StringLiteral(_)) {
                                checker.intrinsics.string
                            } else {
                                checker.intrinsics.number
                            };
                        let callee = checker.check_expression(call.expression.unwrap());
                        let original = checker.call_signatures_of_type(callee).unwrap().remove(0);
                        let original_parameters = checker.type_parameter_types(&original).unwrap();
                        assert_eq!(checker.check_call_expression(call), expected);
                        let signature = checker.resolved_call_signatures.get(id).cloned();
                        if call.type_arguments.is_empty() {
                            assert!(signature.is_none(), "ordinary inferred {stage}");
                        } else {
                            let signature = signature.expect("explicit worker completion");
                            assert!(signature.type_parameters.is_empty());
                            assert_eq!(checker.parameter_type(&signature.parameters[0]), expected);
                            assert_eq!(signature.r#type, expected);
                            let target = signature.target.as_ref().unwrap();
                            assert_eq!(target.declaration, original.declaration);
                            assert_eq!(target.type_parameters.len(), 1);
                            assert_eq!(target.type_parameters[0].name, "T");
                            assert_eq!(
                                checker.type_parameter_types(target).unwrap(),
                                original_parameters
                            );
                            assert_eq!(
                                checker.parameter_type(&target.parameters[0]),
                                original_parameters[0]
                            );
                            assert_eq!(target.r#type, original_parameters[0]);
                            assert_eq!(format!("{target:?}"), format!("{original:?}"));
                            if stage == "worker" {
                                completed.push((*id, target.clone()));
                            } else {
                                let (_, first) =
                                    completed.iter().find(|(call_id, _)| call_id == id).unwrap();
                                assert!(std::sync::Arc::ptr_eq(first, target));
                            }
                            eprintln!("{stage} original={id:?} signature={signature:?}");
                        }
                        assert!(!checker.contextual_signature_mappers.contains_key(id));
                    }
                }
                assert!(checker.active_inference_contexts.is_empty());
                assert!(checker.call_inference_signatures.is_empty());
                assert!(checker.resolving_signature_calls.is_empty());
            });
        }
    }

    #[test]
    fn explicit_targets_keep_ordered_constraint_and_default_metadata_without_substitution() {
        for (name, source) in [
            ("ordered", "declare function ordered<T, U>(left: T, right: U): U;
             ordered<number, string>(17, 'east'); const probe = 0;"),
            ("constrained", "declare function constrained<T extends Record<string, unknown>, U extends T['absent']>(value: T): T;
             declare const input: { present: number };
             constrained<{ present: number }, never>(input); const probe = 0;"),
            ("defaulted", "declare function defaulted<T extends Record<string, unknown>, U = T['absent']>(value: T): T;
             declare const input: { present: number };
             defaulted<{ present: number }, string>(input); const probe = 0;"),
        ] {
            with_declared_variable(source, "probe", |checker, _| {
                let (id, call) = (0..checker.nodes.len())
                    .find_map(|index| {
                        let id = tsr_ast::NodeId::new(u32::try_from(index).unwrap());
                        match checker.node_map.get(id) {
                            Some(tsr_ast::Node::CallExpression(call)) => Some((id, call)),
                            _ => None,
                        }
                    })
                    .unwrap();
                let callee = checker.check_expression(call.expression.unwrap());
                let original = checker.call_signatures_of_type(callee).unwrap().remove(0);
                let parameters = checker.type_parameter_types(&original).unwrap();
                assert_eq!(original.type_parameters.len(), 2, "{name}");
                assert_eq!(original.type_parameters[0].name, "T");
                assert_eq!(original.type_parameters[1].name, "U");
                assert_ne!(parameters[0], parameters[1]);
                let written_u = checker.get_type_from_type_node(call.type_arguments[1]);
                for stage in ["worker", "repeat"] {
                    let answer = checker.check_call_expression(call);
                    assert_ne!(answer, checker.intrinsics.error, "{name} {stage}");
                    let concrete = checker.resolved_call_signatures.get(&id).cloned().unwrap();
                    assert!(concrete.type_parameters.is_empty());
                    assert_eq!(concrete.r#type, answer);
                    let target = concrete.target.as_ref().unwrap();
                    assert_eq!(format!("{target:?}"), format!("{original:?}"), "{name} {stage}");
                    assert_eq!(checker.type_parameter_types(target).unwrap(), parameters);
                    if name != "ordered" {
                        // Native eraseTypeParameters does not instantiate a
                        // constraint/default which belongs only to the target.
                        // The present-only object has no 'absent' property,
                        // so substituting that metadata would rewrite it to
                        // getIndexedAccessTypeEx's nil-node answer, `unknown`
                        // (checker.go:26930) — a different type from the
                        // target's preserved `T['absent']` checked above.
                        let out = checker
                            .instantiate_signature(
                                original.clone(),
                                &[(parameters[0], answer), (parameters[1], written_u)],
                                &parameters,
                                &["T", "U"],
                            )
                            .unwrap();
                        let metadata = out.type_parameters[1]
                            .constraint
                            .or(out.type_parameters[1].default)
                            .unwrap();
                        assert_eq!(metadata, checker.intrinsics.unknown, "{name}");
                    }
                    eprintln!("{name} {stage} call={id:?} concrete={concrete:?}");
                }
            });
        }
    }

    #[test]
    fn explicit_return_only_success_does_not_publish_unsupported_parameter_image() {
        with_declared_variable(
            "declare function missing<T>(consume: () => T['absent']): number;
             missing<unknown>(17); const probe = 0;",
            "probe",
            |checker, _| {
                let (id, call) = (0..checker.nodes.len())
                    .find_map(|index| {
                        let id = tsr_ast::NodeId::new(u32::try_from(index).unwrap());
                        match checker.node_map.get(id) {
                            Some(tsr_ast::Node::CallExpression(call)) => Some((id, call)),
                            _ => None,
                        }
                    })
                    .unwrap();
                for _ in 0..2 {
                    assert_eq!(checker.check_call_expression(call), checker.intrinsics.number);
                    assert!(!checker.resolved_call_signatures.contains_key(&id));
                    assert!(!checker.contextual_signature_mappers.contains_key(&id));
                }
            },
        );
    }

    #[test]
    fn explicit_publication_refuses_foreign_active_and_flow_owners() {
        let source = "declare function known<T>(value: T): T;
            known<number>(17); const probe = 0;";
        for mode in [
            "flow",
            "alias",
            "mapped",
            "freshness",
            "uninstantiated",
            "provisional",
            "active",
            "resolving",
            "foreign-node",
        ] {
            with_declared_variable(source, "probe", |checker, _| {
                let (id, call) = (0..checker.nodes.len())
                    .find_map(|index| {
                        let id = tsr_ast::NodeId::new(u32::try_from(index).unwrap());
                        match checker.node_map.get(id) {
                            Some(tsr_ast::Node::CallExpression(call)) => Some((id, call)),
                            _ => None,
                        }
                    })
                    .unwrap();
                let callee = checker.check_expression(call.expression.unwrap());
                let signature = checker.call_signatures_of_type(callee).unwrap().remove(0);
                let parameter = checker.type_parameter_types(&signature).unwrap()[0];
                match mode {
                    "flow" => checker
                        .flow_loop_stack
                        .push(((0, 0, checker.intrinsics.number), Vec::new())),
                    "alias" => checker.alias_evaluation_bindings.push(
                        [(checker.type_parameter_symbols[&parameter], checker.intrinsics.string)]
                            .into_iter()
                            .collect(),
                    ),
                    "mapped" => checker.mapped_template_depth = 1,
                    "freshness" => checker.contextual_prefers_uninstantiated = true,
                    "uninstantiated" => checker.uninstantiated_context_node = Some(id),
                    "provisional" => {
                        checker.call_inference_signatures.insert(id, signature.clone());
                    }
                    "active" => {
                        checker.active_inference_contexts.insert(
                            id,
                            crate::inference::InferenceContextSnapshot {
                                signature: signature.clone(),
                                inferences: Vec::new(),
                                return_inferences: Vec::new(),
                                flags: crate::inference::InferenceFlags::NONE,
                                inferential: false,
                                intra_expression_sites: Vec::new(),
                                outer_return_map: None,
                            },
                        );
                    }
                    "resolving" => {
                        checker.resolving_signature_calls.insert(id);
                    }
                    "foreign-node" => {}
                    _ => unreachable!(),
                }
                let foreign = tsr_ast::CallExpression {
                    node_id: call.node_id,
                    expression: call.expression,
                    question_dot_token: call.question_dot_token,
                    type_arguments: call.type_arguments,
                    arguments: call.arguments,
                };
                checker.check_call_expression(if mode == "foreign-node" { &foreign } else { call });
                assert!(!checker.resolved_call_signatures.contains_key(&id), "{mode}");
                assert!(!checker.contextual_signature_mappers.contains_key(&id), "{mode}");
                eprintln!("refused {mode}: call={id:?} completed=None mapper=None");
                checker.flow_loop_stack.clear();
                checker.alias_evaluation_bindings.clear();
                checker.mapped_template_depth = 0;
                checker.contextual_prefers_uninstantiated = false;
                checker.uninstantiated_context_node = None;
                checker.call_inference_signatures.clear();
                checker.active_inference_contexts.clear();
                checker.resolving_signature_calls.clear();
                assert_eq!(checker.check_call_expression(call), checker.intrinsics.number);
                assert!(checker.resolved_call_signatures.contains_key(&id), "released {mode}");
            });
        }
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
