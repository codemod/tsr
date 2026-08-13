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
    /// §140: the dynamic-import call's type. `None` keeps the caller's
    /// errorType — an unresolvable or non-literal specifier, a module file
    /// with no symbol, or a missing Promise global each decline.
    fn check_import_call_expression(&mut self, node: &CallExpression<'_>) -> Option<TypeId> {
        let specifier = node.arguments.first().copied()?;
        let specifier_id = tsr_ast::Node::from(specifier).node_id()?;
        let tsr_ast::Expression::StringLiteral(literal) = specifier else { return None };
        let module = self.resolve_external_module_name(specifier_id, specifier_id)?;
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

    fn check_call_expression_worker(
        &mut self,
        node: &CallExpression<'_>,
        callee: tsr_ast::Expression<'_>,
        callee_type: TypeId,
    ) -> TypeId {
        let error = self.intrinsics.error;
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
        if self.is_untyped_call_target(callee, callee_type) {
            bump(&COUNTERS.untyped_call);
            return self.intrinsics.any;
        }
        let resolved = self.resolve_call_signature_with_type_arguments(
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
            // §33's fired leg: `const` type parameters retain literals (and
            // mint readonly tuples) where this port's inference widens — 70
            // G→W in the first pair — so calls through a const-marked
            // signature DECLINE while the signature itself prints.
            if signature.type_parameters.iter().any(|parameter| parameter.is_const) {
                return error;
            }
            // §288: WRITTEN type arguments need no inference at all — the
            // SS161 recipe at the CALL road. `fn2<string>(4)` instantiates
            // the return with the written list when the arity matches
            // (`typeAssertions`, `thisInInvalidContexts`); a mismatch or an
            // unresolvable argument keeps the inference road below.
            if !node.type_arguments.is_empty()
                && node.type_arguments.len() == signature.type_parameters.len()
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
                    let map: Vec<(TypeId, TypeId)> =
                        parameters.iter().copied().zip(written.iter().copied()).collect();
                    let answer = self.instantiate_type(signature.r#type, &map, &parameters, &names);
                    if answer != error {
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
            let answer = self.check_generic_call(&signature, node.node_id, node.arguments);
            if counters::counting() {
                bump(&COUNTERS.single_candidate_generic);
                if answer == error {
                    bump(&COUNTERS.single_candidate_generic_error);
                }
            }
            return answer;
        }
        // `checkNoTypeArguments` (`checker.go:23157`): type arguments on a
        // signature that takes none is an error, and answering the return type
        // would quietly drop them.
        if !node.type_arguments.is_empty() {
            bump(&COUNTERS.single_candidate_type_arguments);
            return error;
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
        if self.store.get(signature.r#type).flags.intersects(TypeFlags::ES_SYMBOL_LIKE)
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
            if signature.r#type == error {
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
        if self.this_types.values().any(|&minted| minted == signature.r#type)
            && let Expression::PropertyAccessExpression(access) = callee
            && let Some(receiver) = access.expression
        {
            let receiver_type = self.check_expression(receiver);
            if receiver_type != error && receiver_type != signature.r#type {
                return receiver_type;
            }
        }
        signature.r#type
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
        let TypeData::Anonymous { symbol, .. } = self.store.get(callee).data else {
            // A callee that prints as a **name** — an interface with a call
            // signature member, which is where every lib constructor lives.
            // `getSignaturesOfType` (`checker.go:18959`) reads the *type's*
            // resolved members; `get_signatures_of_symbol` reads a symbol's
            // declarations and an interface declaration is not signature-shaped,
            // so this needs its own route (`bd tsr-4sa`,
            // `docs/architecture/checker-notes-namedcallee.md`).
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
                let clean = self.clean_candidate_prefix_len(&candidates);
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
            let signatures = self.signature_types.get(&callee).cloned().unwrap_or_default();
            if let [signature] = signatures.as_slice() {
                if counted {
                    bump(&COUNTERS.single_candidate);
                }
                return Some(signature.clone());
            }
            if counted {
                bump(&COUNTERS.callee_no_signatures);
            }
            return None;
        }
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
                self.choose_overload(candidates, arguments, has_type_arguments)
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
        has_type_arguments: bool,
    ) -> Option<Signature> {
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
                return Some((*survivor).clone());
            }
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
        let clean_len = self.clean_candidate_prefix_len(candidates);
        if clean_len == 0 {
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
            match &pick {
                Some(_) => bump(&COUNTERS.selected),
                // The whole prefix rejected pass one (or a pair was
                // undecidable); upstream now consults the generic/rest tail,
                // and its outcome is undecidable here.
                None => bump(&COUNTERS.generic_candidate),
            }
            return pick;
        }
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
                match self.relate_ternary(argument, parameter.r#type, Relation::Assignable) {
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
    /// `isUntypedFunctionCall` (`checker.go:9931`), reduced to its first
    /// disjunct, `IsTypeAny(funcType)`.
    ///
    /// Upstream's other two disjuncts are **not** ported and each is a gap
    /// rather than a guess: the `TypeFlagsTypeParameter` arm needs an apparent
    /// type this port does not compute for every parameter, and the
    /// `globalFunctionType` assignability arm needs the global `Function`
    /// interface.
    ///
    /// # The positional refusal, and why it is a rule and not a trade
    ///
    /// An **unannotated parameter** types as `any` in this port and is
    /// **contextually typed** upstream, so upstream's answer for a call through
    /// one is the contextual parameter type — never `any`. Answering `any` there
    /// would assert something upstream never computes, which is a wrong rule,
    /// and `docs/conventions.md` says a rule is not priced. It is `STATUS.md`
    /// §5's 2,082-line contextual-typing refusal reached through a new door, and
    /// `examples/calleegap.rs` measured it at **64 of 77 misses removed for 36
    /// conversions**.
    ///
    /// **The refusal needs no test of its own.** It was written as one and the
    /// narrowing below subsumed it: an unannotated parameter has no annotation,
    /// so [`Checker::any_is_written_in_an_annotation`] already excludes it. The
    /// explicit predicate was deleted rather than left as dead code, and this
    /// paragraph is why the family is still refused without one.
    pub(crate) fn is_untyped_call_target(
        &mut self,
        callee: Expression<'_>,
        callee_type: TypeId,
    ) -> bool {
        if !self.store.get(callee_type).flags.intersects(TypeFlags::ANY) {
            return false;
        }
        // `errorType` carries `ANY` too. A gap must stay a gap: answering `any`
        // for it is precisely ADR-0038's forbidden rendering, and this is the
        // one place this arm could commit it.
        if callee_type == self.intrinsics.error {
            return false;
        }
        // NARROWED after the first run measured 248 gap->wrong against a bar of
        // 20. The counterfactual sized a design whose `any` comes from a
        // **written** annotation; this arm had been firing wherever the callee
        // typed as `any` for ANY reason, including the many places this port
        // produces `any` from an unported mechanism where upstream computes a
        // real type. `want string | got any` was 137 of the 248.
        //
        // So the test is not "is the type `any`" but "did the source **say**
        // `any`". That is the only form in which this port's `any` and
        // upstream's are the same claim.
        if self.any_is_written_in_an_annotation(callee) {
            // §30's narrowing: a named class expression's name is in scope
            // inside its own body upstream; this port's resolver reaches the
            // outer binding instead (`classBlockScoping`, 5 G→W in the §30
            // first pair). An identifier callee lexically inside a class
            // bearing its name is that resolver miss — contained here.
            if let Expression::Identifier(identifier) = callee
                && let Some(id) = identifier.node_id
            {
                let mut ancestor = self.nodes.parent(id);
                while let Some(node) = ancestor {
                    let name = match self.node_map.get(node) {
                        Some(tsr_ast::Node::ClassExpression(class)) => class.name,
                        Some(tsr_ast::Node::ClassDeclaration(class)) => class.name,
                        _ => None,
                    };
                    if let Some(name) = name
                        && name.text == identifier.text
                    {
                        return false;
                    }
                    ancestor = self.nodes.parent(node);
                }
            }
            return true;
        }
        // §23 (`checker-notes-callres.md`): a property/element access whose
        // RECEIVER is `any` or a minted unresolved is `any` in both
        // compilers, and a call through it is an untyped call.
        let receiver = match callee {
            Expression::PropertyAccessExpression(access) => access.expression,
            Expression::ElementAccessExpression(access) => access.expression,
            _ => None,
        };
        // §24: an IDENTIFIER callee whose `any` is §31's own answer — the
        // name resolves nowhere and the file carries no import machinery.
        if let Expression::Identifier(identifier) = callee
            && let Some(id) = identifier.node_id
            && self
                .binder
                .resolve_name(
                    self.nodes,
                    self.node_map,
                    id,
                    identifier.text,
                    SymbolFlags::VALUE
                        | SymbolFlags::TYPE
                        | SymbolFlags::NAMESPACE
                        | SymbolFlags::ALIAS,
                )
                .is_none()
            && !self.file_has_import_machinery(id)
        {
            return true;
        }
        let Some(receiver) = receiver else { return false };
        // §30's second narrowing: a parser-minted MISSING receiver (the
        // empty identifier `new.targ` recovery produces) is not a source
        // `any` — its `any` is §31 answering an empty name.
        if matches!(receiver, Expression::Identifier(identifier) if identifier.text.is_empty()) {
            return false;
        }
        let receiver_type = self.check_expression(receiver);
        if self.unresolved_types.contains(&receiver_type) {
            return true;
        }
        // An `any` receiver admits only outside JS files — the AMD/require
        // shapes type through machinery upstream has and this port lacks
        // (`amdLikeInputDeclarationEmit`, the §23 bar's fired leg).
        receiver_type == self.intrinsics.any
            && receiver.node_id().is_some_and(|id| !self.in_js_file(id))
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
        let Some(declaration) = self.binder.symbols().get(symbol).value_declaration else {
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
        // Upstream REORDERS candidates before any pass — `reorderCandidates`
        // (`checker.go:8957`) splices every specialized signature (one with a
        // literal-typed parameter, GH#1133) ahead of the non-specialized
        // ones. This port keeps declaration order, so a first-match walk is
        // only sound when the set — the WHOLE set, tail included, since the
        // splice hoists from anywhere — holds no specialized candidate:
        // `inheritedOverloadedSpecializedSignatures` lost a passing
        // diagnostics case to exactly this before the guard (the pick took a
        // general overload upstream had spliced behind `(x: 'B1')`).
        let specialized = |checker: &Self, id: TypeId| {
            let literal = TypeFlags::STRING_LITERAL
                | TypeFlags::NUMBER_LITERAL
                | TypeFlags::BIG_INT_LITERAL
                | TypeFlags::BOOLEAN_LITERAL;
            checker.type_of(id).flags.intersects(literal)
        };
        if candidates
            .iter()
            .any(|candidate| candidate.parameters.iter().any(|p| specialized(self, p.r#type)))
        {
            return None;
        }
        let prefix = &candidates[..clean_len];
        let mut argument_types = Vec::with_capacity(arguments.len());
        for &argument in arguments {
            if matches!(argument, Expression::SpreadElement(_)) {
                return None;
            }
            argument_types.push(self.check_expression(argument));
        }
        let simple = |checker: &Self, id: TypeId| {
            checker.type_of(id).flags.intersects(
                TypeFlags::ANY
                    | TypeFlags::UNKNOWN
                    | TypeFlags::NEVER
                    | TypeFlags::VOID
                    | TypeFlags::UNDEFINED
                    | TypeFlags::NULL
                    | TypeFlags::STRING_LIKE
                    | TypeFlags::NUMBER_LIKE
                    | TypeFlags::BIG_INT_LIKE
                    | TypeFlags::BOOLEAN_LIKE
                    | TypeFlags::ES_SYMBOL_LIKE,
            ) && !checker
                .type_of(id)
                .flags
                .intersects(TypeFlags::OBJECT | TypeFlags::UNION | TypeFlags::TYPE_PARAMETER)
        };
        for candidate in prefix {
            if !has_correct_arity(candidate, argument_types.len()) {
                continue;
            }
            let mut verdict = Ternary::Related;
            for (&argument, parameter) in argument_types.iter().zip(&candidate.parameters) {
                if !simple(self, argument) || !simple(self, parameter.r#type) {
                    verdict = Ternary::Unknown;
                    continue;
                }
                match self.relate_ternary(argument, parameter.r#type, Relation::Subtype) {
                    Ternary::NotRelated => {
                        verdict = Ternary::NotRelated;
                        break;
                    }
                    Ternary::Unknown => verdict = Ternary::Unknown,
                    Ternary::Related => {}
                }
            }
            match verdict {
                Ternary::Unknown => return None,
                Ternary::NotRelated => {}
                // Upstream's pass one picks the FIRST subtype-related
                // candidate; the tail never gets a turn.
                Ternary::Related => return Some(candidate.clone()),
            }
        }
        None
    }

    /// §273's admission test, shared with the `new`-expression road: the
    /// candidates before the first one whose selection would need machinery
    /// this port does not trust here.
    pub(crate) fn clean_candidate_prefix_len(&self, candidates: &[Signature]) -> usize {
        candidates
            .iter()
            .position(|candidate| {
                !candidate.type_parameters.is_empty()
                    || candidate.this_parameter.is_some()
                    || candidate.parameters.iter().any(|parameter| parameter.rest)
                    || candidate
                        .parameters
                        .iter()
                        .any(|p| self.type_of(p.r#type).flags.intersects(TypeFlags::ANY))
            })
            .unwrap_or(candidates.len())
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
