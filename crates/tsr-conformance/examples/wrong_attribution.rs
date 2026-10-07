//! Where the **wrong** answers come from, attributed per line to the change
//! that could have produced them — `bd tsr-y4u.21` follow-up.
//!
//! `cargo run -p tsr-conformance --example wrong_attribution --release`
//!
//! > **CORRECTED 2026-08-05 (`bd tsr-qj4`) — every number in the sections below
//! > that predates this header was measured WITHOUT the bundled lib files.**
//! > This probe built one `Checker` per *file*, with no `Program`, so every
//! > `Math.trunc`, `Object.assign`, `Promise` member and array method in the
//! > corpus answered `error` where upstream computes a real type. It now routes
//! > through `types_producer::assertions_for_case_with_ids`, the entry point
//! > `types_suite.rs` scores the gradient through. Measured at `9bbb36f`, one
//! > pinned binary per side, the same tip both times:
//! >
//! > ```text
//! >                              lib-less (WRONG)   with libs (correct)
//! > aligned lines                        468,921            468,900
//! > gaps (we said error)         173,537 (37.01%)   139,612 (29.77%)
//! > wrong (a claim)               22,067 ( 4.71%)    37,489 ( 8.00%)
//! >   of which Identifier                 17,828             31,067
//! > lib arm, positive control              4,224                397
//! > `typeof {upstream}` defect             1,086                 88
//! > ```
//! >
//! > The corrected `wrong` total, **37,489**, is exactly what
//! > `examples/rank_board.rs` reports over the same population by an
//! > independently written path — the cross-check that says the two are now
//! > measuring one compiler. The third row is the one that changes conclusions:
//! > a third of what this probe called a **gap** was a lib file it never loaded,
//! > and those lines are wrong answers, not missing ones.
//! >
//! > The claim below that "**lib loading cannot move `checker_types` at all**"
//! > is **false** and was false when written, for the same reason the identical
//! > caveat in `checker-notes-calls.md` was: it reads `binder_suite.rs`, which
//! > deliberately has no libs, as though it were `types_producer.rs`, which has
//! > them. `docs/architecture/checker-notes-any.md` records the correction.
//!
//! # The question the histogram structurally cannot answer
//!
//! `examples/types_shapes.rs` buckets a mismatch by upstream's *answer shape* and
//! by the node kind we failed on. For a **gap** that is enough to rank work,
//! because `gap_reason` re-derives why the checker stopped. For a **wrong**
//! answer there is no equivalent: the histogram sees the substitution
//! (`number -> any`) and never the cause. That is the same blind spot that made
//! `gap_reason` necessary, one level up.
//!
//! Cycle 2 made it urgent. The gap pool shrank and the wrong pool nearly doubled
//! — `Identifier, wrong` went 12,558 -> 22,360 across four changes landing in one
//! run — and no arithmetic over four simultaneous changes can split that. But it
//! does not need arithmetic: every wrong line is attributable *individually*, by
//! asking what its name resolved to.
//!
//! # The model, and how it can be wrong
//!
//! Four arms, one per change that landed in the cycle:
//!
//! | the identifier resolves to | the change that could have produced it |
//! |---|---|
//! | a **type parameter** | the class/interface members arm in `resolve_name` |
//! | a function, class, enum or module symbol | `getTypeOfFuncClassEnumModule` |
//! | either side answering a **union** | union types |
//! | a name the bundled libs declare | lib loading |
//!
//! **A four-arm model produces a tidy table whether or not it is right**, so the
//! fifth bucket — *unattributed* — is printed unconditionally, including when it
//! is zero. If it is large the model is wrong, and that is the finding rather
//! than a footnote. The arms are also reported *non-exclusively* alongside the
//! first-match-wins table, so the effect of the precedence order is visible
//! instead of baked in.
//!
//! # What it found, measured at `02b5c27` in a pinned worktree
//!
//! ```text
//! wrong (a claim)        23,673   5.05% of aligned
//!   of which Identifier  22,360  94.45% of wrong
//!
//! type parameter (resolve_name members arm)       0    0.00%
//! func/class/enum/module symbol               6,785   30.34%
//! union on one side                           4,073   18.22%
//! name declared by a bundled lib                  0    0.00%
//! UNATTRIBUTED (control)                     11,502   51.44%
//! ```
//!
//! **The control is the majority, and that is the result.** The four arms explain
//! 48.6% of the wrong identifiers. The rest is dominated by `number -> any`
//! (5,137) and `string -> any` (1,248) — the implicit any, where upstream infers
//! and this port answers `anyType` from
//! `getWidenedTypeForVariableLikeDeclaration`. That predates cycle 2 entirely, so
//! the model was never going to explain it: the arms were built to explain an
//! *increase*, and the control bucket is largely the pre-existing population.
//!
//! **The two live arms are newly-wrong by construction**, not by arithmetic. A
//! line attributed to `FuncClassEnumModule` or `Union` is wrong *because* the code
//! that produces its answer did not exist before cycle 2 — before it both answered
//! `errorType`, so the line was a gap. 6,785 + 4,073 = 10,858 against an observed
//! increase of 9,802 (12,558 -> 22,360), the difference being old wrong lines that
//! became right.
//!
//! **1,086 of them are one defect**: we answer exactly `typeof {upstream}`.
//! `getTypeOfNode` must take a *type declaration's own name* through
//! `getDeclaredTypeOfSymbol` and any other declaration name through
//! `getTypeOfSymbol`; answering `typeof C` where upstream says `C` is those two
//! collapsed.
//!
//! # The model is effectively THREE arms: lib can never fire here
//!
//! **FALSE — superseded by the header at the top of this file.** The producer
//! does build a program and does load every bundled lib; this section read
//! `binder_suite.rs` for `types_producer.rs`. It is left in place rather than
//! deleted because the wrong turn is the useful part of the record.
//!
//! `checker_types` produces through `types_producer::assertions_for_case`, and
//! that function binds **each unit on its own** — its own arena, its own
//! `BindResult`, its own `Checker`, no `Program`, no lib files, nothing
//! cross-unit. So the lib arm can only fire on a name that resolves *nowhere* and
//! happens to be spelled like a lib name, and it reads 0 wrong (4,224 lines
//! overall).
//!
//! The consequence is larger than this probe and was verified independently
//! against the producer's source: **lib loading cannot move `checker_types` at
//! all** until the producer is rewired to build a program. That is a property of
//! the *measurement path*, not of any particular commit — no amount of lib work
//! pays a measurable gradient until it changes. The arm is kept and printed
//! because "it reads zero" is the evidence for that claim and asserting it is
//! not; a non-zero reading would mean the reading of the producer is wrong.
//!
//! # The blind spot, measured: 52 versus 9,735
//!
//! The section below was written when the only type-parameter arm asked what the
//! *identifier on the line* resolves to. `Cause::AnswerIsTypeParameter` asks
//! instead whether the *answer* is a type parameter in scope, and the difference
//! between them is the size of the blind spot:
//!
//! ```text
//! over all 261,042 aligned Identifier lines (at d27db6f):
//!   ANSWER is a type parameter in scope        9,735   3.73%
//!   name resolves to a type parameter             52   0.02%
//! ```
//!
//! 187 times as many. So the members arm's real footprint is **9,735 identifier
//! lines, of which 622 are wrong** — a 93.6% hit rate within its own population,
//! and 2.90% of all wrong identifiers.
//!
//! **And most of those 622 are not this arm's fault.** 256 of them are exactly
//! `{ours} | undefined`: the resolution is right and a *different* rule is
//! missing on top — `strictNullChecks` defaults on, so an optional property or
//! unmatched parameter is `T | undefined`. Corpus-wide that shape is 1,383 wrong
//! lines, one rule rather than a population.
//!
//! **This generalises past this one change.** The walker emits a line for a
//! *declaration*, so any slice whose effect lands on a declaration rather than on
//! a reference is invisible to a name-side arm. Attribute by the answer, not by
//! the name, whenever the change being measured alters what a declaration's type
//! *is*.
//!
//! # The old arm reads zero, and that is NOT "no effect"
//!
//! The positive control says the arm fires 52 times over all 261,042 aligned
//! identifier lines, so it works — but 52 is far too few for a change that made
//! thousands of `T` references resolve. The reason is a limitation of the model,
//! not a property of the change: **a `.types` line is emitted for the declaration,
//! not for the type reference inside its annotation.** `class C<T> { p: T }`
//! produces a line for `p`, whose type string is now `T` instead of a gap — and
//! `p` resolves to a `PROPERTY` symbol, so it lands in the control bucket. The arm
//! asks what the *identifier on the line* resolves to, which for this change is
//! the wrong question.
//!
//! So this probe **does not measure the members arm's contribution**, in either
//! direction. Reporting it as zero would be reading a null out of an instrument
//! that cannot see. Measuring it needs a different question — whether the line's
//! *answer* mentions a type parameter — and that is not built here.
//!
//! # Run it in a pinned worktree, not in the working tree
//!
//! Measured in the shared tree while three teammates were editing `tsr-checker`,
//! this probe reported 22,360 wrong identifiers in one run and 21,485 twenty
//! minutes later. Neither was wrong; the *tree* moved. A number taken from a
//! working tree holding other people's half-finished edits is not attributable to
//! a commit, and the first symptom here was a mutation that appeared to move lines
//! into a bucket it cannot reach — which is what sent this back to a worktree.
//!
//! ```sh
//! git worktree add /tmp/wt <commit>
//! rm -rf /tmp/wt/vendor/typescript-go   # worktrees do not populate submodules
//! ln -s "$PWD/vendor/typescript-go" /tmp/wt/vendor/typescript-go
//! cp crates/tsr-conformance/examples/wrong_attribution.rs \
//!    /tmp/wt/crates/tsr-conformance/examples/
//! ```
//!
//! # What this shares with the instrument, and what it does not
//!
//! It calls `assertions_for_file` and `type_at_location` **verbatim**, so the
//! walker, the selection predicates and the answers are the suite's own — the
//! `NodeId`s come back through the `type_of` closure, which is why no change to
//! `types_producer.rs` was needed. Re-deriving the walk here instead is exactly
//! the drift that produced the 22,768-line phantom finding. What is duplicated
//! is the outer per-file loop (find the unit, skip JSON, parse, bind), which is
//! bookkeeping rather than judgement, and `lib_declared_names`, which is copied
//! from `examples/types_shapes.rs` with its caveats intact — one example cannot
//! import another.

use std::collections::{HashMap, HashSet};

use rayon::prelude::*;
use tsr_ast::{Node, NodeId, NodeMap, NodeTable, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_conformance::{
    Corpus, repo_root,
    type_shape::{self, Shape},
    types_baseline, types_producer,
};

/// Worked examples kept per bucket.
const SAMPLES: usize = 8;

/// One arm of the causal model. `ORDER` is the first-match-wins precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Cause {
    /// **Asks about the answer, not the name.** Our answer *is* the name of a
    /// type parameter in scope at this node — so the line's type came through the
    /// class/interface members arm in `resolve_name`, whatever the identifier on
    /// the line happens to be.
    ///
    /// This is the arm that can see a change whose effect lands on a
    /// *declaration* line rather than on a reference: `class C<T> { p: T }`
    /// emits a line for `p`, not for `T`. [`Cause::TypeParameter`] below asks
    /// what `p` resolves to (a `PROPERTY`) and therefore cannot see it at all.
    AnswerIsTypeParameter,
    /// Resolves to a type parameter — the members arm in `resolve_name`.
    ///
    /// Kept beside [`Cause::AnswerIsTypeParameter`] rather than replaced,
    /// because the two ask different questions and the difference between their
    /// counts is the size of the blind spot.
    TypeParameter,
    /// Resolves to a function, class, enum or module symbol.
    FuncClassEnumModule,
    /// Either side of the comparison is a union.
    Union,
    /// A name the bundled lib files declare at top level.
    LibDeclared,
    /// **The control.** None of the four matched.
    Unattributed,
}

impl Cause {
    const ORDER: [Cause; 5] = [
        Cause::AnswerIsTypeParameter,
        Cause::TypeParameter,
        Cause::FuncClassEnumModule,
        Cause::Union,
        Cause::LibDeclared,
    ];

    fn label(self) -> &'static str {
        match self {
            Cause::AnswerIsTypeParameter => "ANSWER is a type parameter in scope",
            Cause::TypeParameter => "name resolves to a type parameter",
            Cause::FuncClassEnumModule => "func/class/enum/module symbol",
            Cause::Union => "union on one side",
            Cause::LibDeclared => "name declared by a bundled lib",
            Cause::Unattributed => "UNATTRIBUTED (control)",
        }
    }
}

#[derive(Default)]
struct Tally {
    cases: usize,
    aligned: usize,
    /// Aligned mismatches where we answered `error` — a gap, not a claim.
    gaps: usize,
    /// Aligned mismatches where we answered something else — a wrong claim.
    wrong: usize,
    /// Wrong lines by the node kind we answered on.
    wrong_kinds: HashMap<String, usize>,
    /// Wrong `Identifier` lines: the population this probe exists to attribute.
    identifiers: usize,
    /// First-match-wins attribution of those.
    first_match: HashMap<Cause, usize>,
    /// Non-exclusive: how many wrong identifiers each arm matches at all.
    any_match: HashMap<Cause, usize>,
    /// How many matched more than one arm, which is what the precedence hides.
    multi_arm: usize,
    /// `(case, expression, upstream, ours)` per bucket.
    samples: HashMap<Cause, Vec<(String, String, String, String)>>,
    /// Commonest `upstream -> ours` substitutions among wrong identifiers.
    pairs: HashMap<(String, String), usize>,
    /// **Positive control for the arms.** Over *every* aligned `Identifier`
    /// line, right or wrong, how many resolve to each arm's symbol shape. An arm
    /// reading zero in the attribution table is only meaningful if it is
    /// non-zero here: otherwise the arm is broken and its zero says nothing.
    control_population: HashMap<Cause, usize>,
    /// Aligned `Identifier` lines whose name resolves to no symbol at all. These
    /// can never match a symbol-shaped arm, so they are the floor under the
    /// unattributed bucket.
    control_unresolved: usize,
    /// Aligned `Identifier` lines in total, the denominator for both.
    control_total: usize,
    /// Of the wrong lines, those where we answered exactly `typeof {upstream}`.
    /// `getTypeOfNode` must answer a *type declaration's own name* with
    /// `getDeclaredTypeOfSymbol` and any other declaration name with
    /// `getTypeOfSymbol`; collapsing the two answers every class name with
    /// `typeof C` where upstream says `C`. Counted separately because it is one
    /// defect rather than a population.
    typeof_of_want: usize,
    /// Wrong lines where upstream's answer is exactly ours plus `| undefined`.
    /// Under `strictNullChecks` — which defaults **on** — an optional property
    /// or an unmatched parameter is `T | undefined`, and answering the bare `T`
    /// is one missing rule rather than a wrong resolution.
    missing_undefined: usize,
    /// The same shape restricted to lines whose answer is a type parameter, so
    /// that "my arm resolved it wrongly" can be told apart from "my arm resolved
    /// it correctly and a different rule is missing on top".
    missing_undefined_on_type_parameter: usize,
    /// **The mirror shape**: *ours* carries `| undefined` and upstream's does
    /// not. This is the direct signature of the optionality rule
    /// over-applying, and it is counted by how the case sets
    /// `strictNullChecks`, because that is the assumption most likely to cause
    /// it — `addOptionalityEx` is gated on the option, and with it off upstream
    /// adds nothing at all.
    over_application: HashMap<&'static str, usize>,
    /// Every wrong `Identifier` line by the same classification, so the
    /// over-application rate can be compared against the base rate rather than
    /// read on its own. A bucket that is large because that *kind of case* is
    /// large is not evidence.
    wrong_by_strictness: HashMap<&'static str, usize>,
    /// Lines where we answered `any` and upstream answered something else: the
    /// 6,385-line population, bucketed by cause. Non-exclusive.
    implicit_any: HashMap<&'static str, usize>,
    /// How many such lines matched no cause at all — the control.
    implicit_any_unattributed: usize,
    /// The population being bucketed, as the denominator.
    implicit_any_total: usize,
    /// Every wrong `Identifier` line by the **parent kind** of the node that
    /// produced it.
    ///
    /// The cheapest question the probe was not asking, and it has already found
    /// one defect: every one of the 1,086 `X -> typeof X` lines shared the
    /// parent `ExpressionWithTypeArguments`, which is a class `extends` clause.
    /// Upstream's checker answers `typeof A` there too and its **baseline
    /// writer** compensates (`type_symbol_baseline.go:371`, labelled a
    /// workaround in its own comment) -- so the defect was in neither of the two
    /// candidate places, and one parent kind was the whole of it.
    wrong_by_parent: HashMap<String, usize>,
    /// The 3,901 contextual-typing parameters, by **what contextually types
    /// them**: the parent of the function they belong to.
    ///
    /// `getContextuallyTypedParameterType` reaches the answer through
    /// `getContextualSignature` -> `getApparentTypeOfContextualType` ->
    /// `getContextualType` (`checker.go:29458`), and that last is a dispatch over
    /// dozens of syntactic positions of wildly different cost. Which arms to port
    /// is a question about where the lines actually are, not about which arm is
    /// most interesting.
    contextual_parameter_context: HashMap<String, usize>,
    /// Wrong lines by the **symbol flags** of what the identifier resolves to.
    ///
    /// Parent kind names the *construct*; flags name the *dispatch*. Symbols'
    /// accessor defect lived entirely in the second and was invisible in the
    /// first: `interface I { get x(): number; x(): number; set x(v: number) }`
    /// printed `() => number` where upstream prints `number`, because the merged
    /// symbol carries `METHOD | GET_ACCESSOR | SET_ACCESSOR` and
    /// `getTypeOfSymbol`'s accessor branch — upstream's *first* flags test,
    /// `checker.go:16506` — was absent. By parent kind those lines are ordinary
    /// `GetAccessor`/`SetAccessor` nodes, indistinguishable from the ones that
    /// gap correctly. By flags the combination is obviously wrong: it has no
    /// business reaching a signature printer.
    ///
    /// It took four attempts to find because a *lone* accessor already gaps and
    /// only the **merge** reaches the arm, so three hand-built fixtures came back
    /// clean. That is the argument for this column: a dispatch bug is a fact
    /// about the symbol, and no syntactic bucketing can see one.
    wrong_by_flags: HashMap<String, usize>,
    /// **Gap** lines owned by `FunctionTypeNode`, directly or transitively.
    ///
    /// `get_type_from_type_node` handles keyword, literal, parenthesised, type
    /// reference, type literal and union nodes; a `FunctionTypeNode` falls to
    /// `errorType`. That blocks contextual typing outright, and it also gaps
    /// every annotation that *is* a function type, and every type literal that
    /// contains a method or call signature -- `get_type_from_type_literal`
    /// rejects the whole literal rather than printing members it does understand.
    ///
    /// Two questions, kept apart because they are different sizes of unblocking:
    /// what the node owns **directly**, and what is behind it **transitively**.
    function_type_gaps: HashMap<&'static str, usize>,
    /// Every gap line, as the denominator.
    gap_lines: usize,
    /// Wrong lines whose parent is a `QualifiedName`, by **grandparent** kind.
    ///
    /// 4,577 lines, the second-largest parent bucket and in no ranking. Two
    /// competing readings, which the grandparent separates because the parent is
    /// `QualifiedName` for both:
    ///
    /// - the left-hand `M` of a type reference `M.I` being resolved as a value
    ///   where upstream resolves the whole entity name;
    /// - `import X = M.I`, where upstream answers `>M : typeof M` -- a
    ///   `QualifiedName` is type-position-only syntax, so identifiers inside one
    ///   should be dropped by `IsPartOfTypeNode` and emit no line at all. That
    ///   these are *aligned* means upstream emitted them too, which points at the
    ///   qualified names that are **not** inside a type node.
    qualified_name_grandparent: HashMap<String, usize>,
    /// Over-application restricted to cases where `strictNullChecks` is
    /// *genuinely on*, by parent kind.
    ///
    /// These are the 125 lines the option cannot explain: upstream adds
    /// `| undefined` in these cases too, so something else about the rule is
    /// wrong. Bucketed by parent because that is what worked for the 1,086.
    over_application_strict_on: HashMap<String, usize>,
}

impl Tally {
    fn merge(&mut self, other: Tally) {
        self.cases += other.cases;
        self.aligned += other.aligned;
        self.gaps += other.gaps;
        self.wrong += other.wrong;
        self.identifiers += other.identifiers;
        self.multi_arm += other.multi_arm;
        for (k, v) in other.wrong_kinds {
            *self.wrong_kinds.entry(k).or_default() += v;
        }
        for (k, v) in other.first_match {
            *self.first_match.entry(k).or_default() += v;
        }
        for (k, v) in other.any_match {
            *self.any_match.entry(k).or_default() += v;
        }
        for (k, v) in other.pairs {
            *self.pairs.entry(k).or_default() += v;
        }
        for (k, v) in other.control_population {
            *self.control_population.entry(k).or_default() += v;
        }
        self.control_unresolved += other.control_unresolved;
        self.control_total += other.control_total;
        self.typeof_of_want += other.typeof_of_want;
        self.missing_undefined += other.missing_undefined;
        self.missing_undefined_on_type_parameter += other.missing_undefined_on_type_parameter;
        for (k, v) in other.over_application {
            *self.over_application.entry(k).or_default() += v;
        }
        for (k, v) in other.wrong_by_strictness {
            *self.wrong_by_strictness.entry(k).or_default() += v;
        }
        for (k, v) in other.implicit_any {
            *self.implicit_any.entry(k).or_default() += v;
        }
        self.implicit_any_unattributed += other.implicit_any_unattributed;
        self.implicit_any_total += other.implicit_any_total;
        for (k, v) in other.wrong_by_parent {
            *self.wrong_by_parent.entry(k).or_default() += v;
        }
        for (k, v) in other.wrong_by_flags {
            *self.wrong_by_flags.entry(k).or_default() += v;
        }
        for (k, v) in other.function_type_gaps {
            *self.function_type_gaps.entry(k).or_default() += v;
        }
        self.gap_lines += other.gap_lines;
        for (k, v) in other.qualified_name_grandparent {
            *self.qualified_name_grandparent.entry(k).or_default() += v;
        }
        for (k, v) in other.contextual_parameter_context {
            *self.contextual_parameter_context.entry(k).or_default() += v;
        }
        for (k, v) in other.over_application_strict_on {
            *self.over_application_strict_on.entry(k).or_default() += v;
        }
        for (bucket, examples) in other.samples {
            let slot = self.samples.entry(bucket).or_default();
            for example in examples {
                if slot.len() < SAMPLES {
                    slot.push(example);
                }
            }
        }
    }
}

/// The symbol an identifier denotes, by the same dispatch `type_at_location`
/// uses to *type* it.
///
/// Mirrors `types_producer::type_at_location`'s order deliberately: a declaration
/// name goes through its parent's symbol, and anything else is resolved as a
/// name. Asking a different question here than the answer was computed from
/// would attribute the wrong line.
fn symbol_of_identifier(
    binder: &tsr_binder::BindResult<'_>,
    nodes: &NodeTable,
    map: &NodeMap<'_>,
    id: NodeId,
) -> Option<tsr_binder::SymbolId> {
    if let Some(parent) = nodes.parent(id)
        && map.get(parent).and_then(|p| p.name_id()) == Some(id)
        && let Some(symbol) = binder.symbol_of(parent)
    {
        return Some(symbol);
    }
    let Some(Node::Identifier(name)) = map.get(id) else { return None };
    // Value meaning first, then type: an identifier line is usually an
    // expression, and `resolve_name` is meaning-filtered, so asking with the
    // wrong one would silently answer `None` and inflate the control bucket.
    binder
        .resolve_name(nodes, map, id, name.text, SymbolFlags::VALUE)
        .or_else(|| binder.resolve_name(nodes, map, id, name.text, SymbolFlags::TYPE))
}

/// Whether this gap line is owned by `FunctionTypeNode`, and how.
///
/// Walks the *annotation* of the declaration behind the line. Direct means the
/// annotation is a function or constructor type; transitive means the gap is a
/// type literal carrying a signature member, or a function type nested inside
/// the annotation. `None` means the gap is something else entirely, which is
/// most of them and is why the denominator is printed beside the buckets.
fn function_type_gap(
    binder: &tsr_binder::BindResult<'_>,
    nodes: &NodeTable,
    map: &NodeMap<'_>,
    id: NodeId,
) -> Option<&'static str> {
    let symbol = symbol_of_identifier(binder, nodes, map, id)?;
    let declaration = binder.symbols().get(symbol).value_declaration?;
    let annotation = map.get(declaration)?.type_id()?;
    let node = map.get(annotation)?;
    if matches!(node, Node::FunctionTypeNode(_) | Node::ConstructorTypeNode(_)) {
        return Some("DIRECT: the annotation is a function type");
    }
    // A type literal is rejected whole if any member is one this port cannot
    // render, and a method or call signature is the commonest such member.
    if let Node::TypeLiteralNode(literal) = node
        && literal.members.iter().any(|m| {
            matches!(
                m,
                tsr_ast::TypeElement::MethodSignatureDeclaration(_)
                    | tsr_ast::TypeElement::CallSignatureDeclaration(_)
                    | tsr_ast::TypeElement::ConstructSignatureDeclaration(_)
            )
        })
    {
        return Some("TRANSITIVE: type literal with a signature member");
    }
    // Anywhere deeper: `Array<(x: number) => void>`, `A | ((n: number) => void)`.
    let mut stack = vec![node];
    let mut children = Vec::new();
    let mut depth = 0usize;
    while let Some(current) = stack.pop() {
        depth += 1;
        // A cheap bound rather than a correctness guard: an annotation is not
        // deep, and an unbounded walk over a hostile one is not worth the risk.
        if depth > 512 {
            break;
        }
        if matches!(current, Node::FunctionTypeNode(_) | Node::ConstructorTypeNode(_)) {
            return Some("TRANSITIVE: a function type nested in the annotation");
        }
        children.clear();
        tsr_ast::push_children(current, &mut children);
        stack.extend(children.iter().copied());
    }
    None
}

/// Why a line we answered `any` should have been something else.
///
/// The 6,385-line `number/string -> any` population is the largest defect
/// population in the corpus and **its cause is not established**. The obvious
/// reading — the implicit any from a declaration with neither annotation nor
/// initialiser — cannot be the whole story, because upstream returns the
/// implicit any there too. So these are lines where upstream *infers* something
/// and this port does not, and the candidates are very different sizes of work.
///
/// Non-exclusive and with a control, for the same reason the cause arms are:
/// a classifier that sorts every line into one of six buckets produces a tidy
/// table whether or not it is right.
fn implicit_any_causes(
    binder: &tsr_binder::BindResult<'_>,
    nodes: &NodeTable,
    map: &NodeMap<'_>,
    id: NodeId,
) -> Vec<&'static str> {
    let mut out = Vec::new();
    let Some(symbol) = symbol_of_identifier(binder, nodes, map, id) else {
        // Cannot be an inference failure: there is nothing to infer *from*.
        // Almost certainly a lib global (`bd tsr-9or.1`).
        out.push("the name resolves to no symbol");
        return out;
    };
    let flags = binder.symbols().get(symbol).flags;
    if flags.intersects(
        SymbolFlags::FUNCTION | SymbolFlags::CLASS | SymbolFlags::ENUM | SymbolFlags::MODULE,
    ) {
        // Reported rather than deduplicated away: this population may overlap
        // `getTypeOfFuncClassEnumModule`'s undiagnosed wrong lines entirely.
        out.push("func/class/enum/module symbol (overlaps symbols' item)");
    }
    let Some(declaration) = binder.symbols().get(symbol).value_declaration else {
        out.push("symbol has no value declaration");
        return out;
    };
    let node = map.get(declaration);
    let annotated = node.and_then(|n| n.type_id()).is_some();
    let initialised = node.and_then(|n| n.initializer_id()).is_some();
    match nodes.kind(declaration) {
        SyntaxKind::Parameter if !annotated && !initialised => {
            // Same split as the variable case, and needed for the same reason.
            // A parameter of a *top-level* function has no contextual type at
            // all -- upstream gives it the implicit any too -- so a wrong line
            // there cannot be a contextual-typing gap. It is a **reference** to
            // the parameter inside the body, carrying upstream's narrowed type.
            //
            // Not splitting this the first time put 269 `SourceFile` and 184
            // `ClassDeclaration` contexts into a contextual-typing bucket, which
            // is impossible: neither position contextually types anything.
            let is_own_name = map.get(declaration).and_then(|d| d.name_id()) == Some(id);
            if is_own_name {
                out.push("parameter, no annotation and no initialiser");
            } else {
                out.push("REFERENCE to an implicit-any parameter (narrowing)");
            }
        }
        SyntaxKind::Parameter if !annotated => {
            out.push("parameter, inferred from its initialiser");
        }
        SyntaxKind::BindingElement => out.push("binding element (destructuring)"),
        SyntaxKind::VariableDeclaration if !annotated && initialised => {
            // We *have* an initialiser and still said `any`, so the initialiser
            // expression is itself a gap. That is expression work, not inference.
            out.push("variable, initialiser we cannot type");
        }
        SyntaxKind::VariableDeclaration if !annotated => {
            // A `for (const x of xs)` variable has neither an annotation nor an
            // initialiser and is **not** an implicit any: upstream types it from
            // the iterable (`checkRightHandSideOfForOf`). Splitting it out
            // matters because it is entirely different work from contextual
            // typing, and the undivided bucket reads as "upstream would say
            // `any` here too", which is what made it look impossible.
            let grandparent = nodes
                .parent(declaration)
                .and_then(|list| nodes.parent(list))
                .map(|stmt| nodes.kind(stmt));
            match grandparent {
                Some(SyntaxKind::ForOfStatement) => out.push("for-of variable (from the iterable)"),
                Some(SyntaxKind::ForInStatement) => out.push("for-in variable (always string)"),
                Some(SyntaxKind::CatchClause) => out.push("catch variable"),
                _ => {
                    // The decisive split. If this line is the declaration's own
                    // *name*, upstream answers the implicit `any` too and the
                    // line is genuinely puzzling. If it is a **reference**
                    // elsewhere, upstream is answering the *narrowed* type —
                    // `let x; x = 1; x` is `number` at the use site through
                    // control-flow `any` evolution — and this is not an
                    // inference gap at all but a narrowing one (`bd tsr-4sc.11`),
                    // which is a different epic and blocked on flow analysis.
                    let is_own_name = map.get(declaration).and_then(|d| d.name_id()) == Some(id);
                    if is_own_name {
                        out.push("variable's own name, no annotation or initialiser");
                    } else {
                        out.push("REFERENCE to an implicit-any variable (narrowing)");
                    }
                }
            }
        }
        SyntaxKind::PropertyDeclaration | SyntaxKind::PropertySignature if !annotated => {
            out.push("property, no annotation");
        }
        _ if annotated => {
            // Annotated and still `any`: the annotation is a type node we do not
            // understand. Different work again.
            out.push("annotated, but the annotation is a gap");
        }
        _ => {}
    }
    out
}

/// How a case sets `strictNullChecks`, in upstream's resolution order.
///
/// `GetStrictOptionValue` falls back to `strict` when the specific option is
/// unset, and this port assumes **on** when neither is written — which is the
/// assumption under test here. Reported as three buckets rather than two so
/// that "explicitly off" is distinguishable from "not stated": only the first
/// is a case where upstream certainly adds nothing, and conflating them would
/// make the result look tidier than the evidence is.
fn strictness_of(options: &std::collections::BTreeMap<String, String>) -> &'static str {
    let truthy = |v: &String| v.eq_ignore_ascii_case("true");
    if let Some(value) = options.get("strictnullchecks") {
        return if truthy(value) { "strictNullChecks: true" } else { "strictNullChecks: false" };
    }
    if let Some(value) = options.get("strict") {
        return if truthy(value) { "strict: true" } else { "strict: false" };
    }
    "neither stated (we assume on)"
}

/// Every type-parameter name in scope at `id`.
///
/// Walks outward exactly as `BindResult::resolve_name` does, reading the two
/// tables it reads: a container's `locals`, and — for a class, class expression
/// or interface — its symbol's `members`, which is where a class's or
/// interface's type parameters are filed
/// (`declareSymbolAndAddToSymbolTable` -> `declareClassMember`,
/// `internal/binder/binder.go:429-441`).
///
/// **Names, not symbols, and that is a deliberate approximation.** The question
/// this answers is "could our answer string have come from a type parameter",
/// and the answer is rendered as a bare name. Two different `T`s in scope at
/// different depths are indistinguishable here, which can only ever *over*-count
/// — so treat this arm as an upper bound on the members arm's footprint.
fn type_parameters_in_scope(
    binder: &tsr_binder::BindResult<'_>,
    nodes: &NodeTable,
    id: NodeId,
) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut current = Some(id);
    while let Some(node) = current {
        if let Some(locals) = binder.locals(node) {
            for (name, symbol) in locals {
                if binder.symbols().get(*symbol).flags.contains(SymbolFlags::TYPE_PARAMETER) {
                    names.insert((*name).to_string());
                }
            }
        }
        if matches!(
            nodes.kind(node),
            SyntaxKind::ClassDeclaration
                | SyntaxKind::ClassExpression
                | SyntaxKind::InterfaceDeclaration
        ) && let Some(owner) = binder.symbol_of(node)
        {
            for (name, symbol) in &binder.symbols().get(owner).members {
                if binder.symbols().get(*symbol).flags.contains(SymbolFlags::TYPE_PARAMETER) {
                    names.insert((*name).to_string());
                }
            }
        }
        current = nodes.parent(node);
    }
    names
}

/// Which arms of the model this wrong line matches. Possibly several.
#[allow(
    clippy::too_many_arguments,
    reason = "Every argument is one of the three things an attribution needs: the \
              bound file (binder/nodes/map), the line (id/text/want/got), and the \
              model (lib_names/arms). Bundling them into a struct would hide which \
              of the three an arm actually reads, and that is the property a reader \
              checks an arm against."
)]
fn causes_of(
    binder: &tsr_binder::BindResult<'_>,
    nodes: &NodeTable,
    map: &NodeMap<'_>,
    id: NodeId,
    text: &str,
    want: &str,
    got: &str,
    lib_names: &HashSet<String>,
    arms: &[Cause],
) -> Vec<Cause> {
    let mut matched = Vec::new();
    let flags = symbol_of_identifier(binder, nodes, map, id)
        .map(|symbol| binder.symbols().get(symbol).flags);
    // Computed lazily: the scope walk is per-line and most lines never need it.
    let mut in_scope: Option<HashSet<String>> = None;
    for arm in arms {
        let hit = match arm {
            Cause::AnswerIsTypeParameter => {
                let names =
                    in_scope.get_or_insert_with(|| type_parameters_in_scope(binder, nodes, id));
                // Exact equality, not containment: a type parameter renders as
                // its bare name, and `T` appearing inside `Array<T>` is a
                // different (and unported) shape that this must not claim.
                names.contains(got) || names.contains(want)
            }
            Cause::TypeParameter => flags.is_some_and(|f| f.contains(SymbolFlags::TYPE_PARAMETER)),
            Cause::FuncClassEnumModule => flags.is_some_and(|f| {
                f.intersects(
                    SymbolFlags::FUNCTION
                        | SymbolFlags::CLASS
                        | SymbolFlags::ENUM
                        | SymbolFlags::MODULE,
                )
            }),
            Cause::Union => {
                type_shape::classify(want) == Shape::Union
                    || type_shape::classify(got) == Shape::Union
            }
            Cause::LibDeclared => flags.is_none() && lib_names.contains(text),
            Cause::Unattributed => false,
        };
        if hit {
            matched.push(*arm);
        }
    }
    matched
}

fn main() {
    // `WRONG_ATTRIBUTION_DISABLE=TypeParameter` drops one arm from the model, so
    // that its lines can be watched moving into the control bucket rather than
    // being silently redistributed among the others. That is the mutation this
    // probe is verified with; without it the table is unfalsifiable.
    let disabled = std::env::var("WRONG_ATTRIBUTION_DISABLE").unwrap_or_default();
    let arms: Vec<Cause> =
        Cause::ORDER.into_iter().filter(|c| format!("{c:?}") != disabled).collect();

    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    assert!(corpus.is_available(), "corpus missing; run git submodule update --init --recursive");
    let cases = corpus.discover().expect("discovering cases");
    let lib_names = lib_declared_names();

    let tallies: Vec<Tally> = cases
        .par_iter()
        .map(|case| {
            let mut tally = Tally::default();
            // The same skips the suite applies, so the population is the
            // gradient's own denominator.
            if case.has_varied_types() || case.has_known_divergence() {
                return tally;
            }
            let Some(text) = case.expected_types() else { return tally };
            let expected = types_baseline::parse(&text);
            if types_baseline::assertion_count(&expected) == 0 {
                return tally;
            }
            let Ok(parsed) = case.load() else { return tally };
            tally.cases = 1;

            // **One program per case, with the bundled libs in it.** This is the
            // entry point `types_suite.rs` scores the gradient through, and
            // routing through it is not optional: the per-unit shape this probe
            // used until `bd tsr-qj4` built a `Checker` per *file* with no libs
            // and no program, so every `Math.trunc`, `Object.assign`, `Promise`
            // member and array method in the corpus answered `error` where
            // upstream computes a real type. See `docs/conventions.md`,
            // "A probe that re-implements the harness is measuring a different
            // compiler" — this is the fourth instance and the third file.
            let arena = tsr_core::Arena::new();
            let (program, ours, ids_by_file) =
                types_producer::assertions_for_case_with_ids(&arena, &parsed, &parsed.files.as_slice());
            let nodes = program.nodes();
            let node_map = program.node_map();
            let bound = program.binder();

            for (index, expected_file) in expected.iter().enumerate() {
                let Some(rendered) = ours.get(index) else { continue };
                let Some(ids) = ids_by_file.get(index) else { continue };
                assert_eq!(ids.len(), rendered.len(), "one recorded id per rendered line");

                for (position, want) in expected_file.assertions.iter().enumerate() {
                    let Some(got) = rendered.get(position) else { continue };
                    let Some(want_type) = want.text.strip_prefix(&format!("{} : ", got.text))
                    else {
                        continue;
                    };
                    tally.aligned += 1;
                    // The positive control runs over EVERY aligned identifier
                    // line, before the right/wrong split, so that an arm reading
                    // zero in the attribution table can be told apart from an arm
                    // that never fires at all.
                    if got.kind == SyntaxKind::Identifier {
                        tally.control_total += 1;
                        let matched = causes_of(
                            bound,
                            nodes,
                            node_map,
                            ids[position],
                            &got.text,
                            want_type,
                            &got.type_string,
                            &lib_names,
                            &Cause::ORDER,
                        );
                        if symbol_of_identifier(bound, nodes, node_map, ids[position]).is_none() {
                            tally.control_unresolved += 1;
                        }
                        for arm in matched {
                            *tally.control_population.entry(arm).or_default() += 1;
                        }
                    }
                    if want_type == got.type_string {
                        continue;
                    }
                    if got.type_string == "error" {
                        tally.gaps += 1;
                        tally.gap_lines += 1;
                        if let Some(bucket) =
                            function_type_gap(bound, nodes, node_map, ids[position])
                        {
                            *tally.function_type_gaps.entry(bucket).or_default() += 1;
                        }
                        continue;
                    }
                    tally.wrong += 1;
                    *tally.wrong_kinds.entry(format!("{:?}", got.kind)).or_default() += 1;
                    if got.kind != SyntaxKind::Identifier {
                        continue;
                    }
                    tally.identifiers += 1;
                    if got.type_string == format!("typeof {want_type}") {
                        tally.typeof_of_want += 1;
                    }
                    // The implicit-any population: we claimed `any`, upstream
                    // claimed something else. `any -> any` cannot appear here
                    // (it would have matched), so the test is one-sided.
                    if got.type_string == "any" {
                        tally.implicit_any_total += 1;
                        let why = implicit_any_causes(bound, nodes, node_map, ids[position]);
                        if why.is_empty() {
                            tally.implicit_any_unattributed += 1;
                        }
                        for cause in &why {
                            *tally.implicit_any.entry(*cause).or_default() += 1;
                        }
                        // For the contextual-typing slice: what contextually
                        // types this parameter is the *function's* position, two
                        // levels up from the parameter.
                        if why.contains(&"parameter, no annotation and no initialiser") {
                            let context =
                                symbol_of_identifier(bound, nodes, node_map, ids[position])
                                    .and_then(|s| bound.symbols().get(s).value_declaration)
                                    .and_then(|parameter| nodes.parent(parameter))
                                    .and_then(|function| nodes.parent(function))
                                    .map_or_else(
                                        || "<no enclosing context>".to_string(),
                                        |ctx| format!("{:?}", nodes.kind(ctx)),
                                    );
                            *tally.contextual_parameter_context.entry(context).or_default() += 1;
                        }
                    }
                    let strictness = strictness_of(&parsed.options);
                    *tally.wrong_by_strictness.entry(strictness).or_default() += 1;
                    // The mirror of the target shape: we added `| undefined`
                    // and upstream did not.
                    let parent = nodes
                        .parent(ids[position])
                        .map_or_else(|| "<root>".to_string(), |p| format!("{:?}", nodes.kind(p)));
                    *tally.wrong_by_parent.entry(parent.clone()).or_default() += 1;
                    let flags = symbol_of_identifier(bound, nodes, node_map, ids[position])
                        .map_or_else(
                            || "<unresolved>".to_string(),
                            |symbol| format!("{:?}", bound.symbols().get(symbol).flags),
                        );
                    *tally.wrong_by_flags.entry(flags).or_default() += 1;
                    if parent == "QualifiedName" {
                        let grandparent =
                            nodes.parent(ids[position]).and_then(|p| nodes.parent(p)).map_or_else(
                                || "<root>".to_string(),
                                |g| format!("{:?}", nodes.kind(g)),
                            );
                        *tally.qualified_name_grandparent.entry(grandparent).or_default() += 1;
                    }
                    if got.type_string == format!("{want_type} | undefined") {
                        *tally.over_application.entry(strictness).or_default() += 1;
                        // The 125 the option cannot explain.
                        if strictness == "strict: true" || strictness == "strictNullChecks: true" {
                            *tally.over_application_strict_on.entry(parent).or_default() += 1;
                        }
                    }
                    if want_type == format!("{} | undefined", got.type_string) {
                        tally.missing_undefined += 1;
                        if type_parameters_in_scope(bound, nodes, ids[position])
                            .contains(&got.type_string)
                        {
                            tally.missing_undefined_on_type_parameter += 1;
                        }
                    }
                    *tally
                        .pairs
                        .entry((want_type.to_string(), got.type_string.clone()))
                        .or_default() += 1;

                    let matched = causes_of(
                        bound,
                        nodes,
                        node_map,
                        ids[position],
                        &got.text,
                        want_type,
                        &got.type_string,
                        &lib_names,
                        &arms,
                    );
                    if matched.len() > 1 {
                        tally.multi_arm += 1;
                    }
                    for arm in &matched {
                        *tally.any_match.entry(*arm).or_default() += 1;
                    }
                    let first = matched.first().copied().unwrap_or(Cause::Unattributed);
                    *tally.first_match.entry(first).or_default() += 1;
                    let slot = tally.samples.entry(first).or_default();
                    if slot.len() < SAMPLES {
                        slot.push((
                            case.name.clone(),
                            got.text.clone(),
                            want_type.to_string(),
                            got.type_string.clone(),
                        ));
                    }
                }
            }
            tally
        })
        .collect();

    let mut total = Tally::default();
    for tally in tallies {
        total.merge(tally);
    }
    report(&total, &arms);
}

#[allow(clippy::cast_precision_loss)]
fn pct(n: usize, d: usize) -> f64 {
    if d == 0 { 0.0 } else { n as f64 / d as f64 * 100.0 }
}

#[allow(clippy::too_many_lines)]
fn report(total: &Tally, arms: &[Cause]) {
    println!("\n=== Where the wrong answers come from ===\n");

    // The caveat travels with the number, not beside it in a message. A figure
    // that needs its qualification carried separately is one that will be quoted
    // without it.
    println!("READ THIS BEFORE QUOTING ANY NUMBER BELOW");
    println!(
        "  * These are ALL currently-wrong lines, not the lines that became wrong\n\
         \x20   this cycle. A true before/after diff needs the probe run at the earlier\n\
         \x20   commit too. The arms below all landed in cycle 2, so a large arm bounds\n\
         \x20   how much of the increase it can explain -- it does not measure it."
    );
    println!(
        "  * CORRECTED 2026-08-05 (`bd tsr-qj4`). This probe used to build one\n\
         \x20   Checker per FILE with no program and no lib files. It now routes\n\
         \x20   through `types_producer::assertions_for_case_with_ids`, which is what\n\
         \x20   the gradient is scored through: one program per case, every bundled\n\
         \x20   lib in it. Every number this probe printed before that is measured on\n\
         \x20   a different compiler -- see docs/architecture/checker-notes-any.md."
    );
    println!(
        "  * The lib arm reads 0 among the WRONG lines. That is now a reading about\n\
         \x20   the corpus and not about the entry point: the positive control below\n\
         \x20   shows the arm firing, and its population fell 4224 -> 397 when the\n\
         \x20   libs were loaded, because names it used to see as unresolved now\n\
         \x20   resolve to a real lib symbol."
    );
    println!(
        "  * The type-parameter arm CANNOT see its own change: a `.types` line is\n\
         \x20   emitted for the declaration, not for the type reference inside its\n\
         \x20   annotation, so `class C<T> {{ p: T }}` yields a line for `p` -- a PROPERTY\n\
         \x20   symbol, which lands in the control. Its zero is unreadable, not a zero."
    );
    println!(
        "  * Buckets are first-match-wins in the order printed. The non-exclusive\n\
         \x20   table below shows what the precedence hides."
    );
    if arms.len() != Cause::ORDER.len() {
        println!("  * MUTATED RUN: arms disabled -> {:?}", {
            let mut off: Vec<_> = Cause::ORDER.into_iter().filter(|c| !arms.contains(c)).collect();
            off.sort_unstable();
            off
        });
    }
    println!();

    println!("cases judged        {:>9}", total.cases);
    println!("aligned lines       {:>9}", total.aligned);
    println!(
        "  gaps (we said error){:>8}  {:>6.2}% of aligned",
        total.gaps,
        pct(total.gaps, total.aligned)
    );
    println!(
        "  wrong (a claim)   {:>9}  {:>6.2}% of aligned",
        total.wrong,
        pct(total.wrong, total.aligned)
    );
    println!(
        "    of which Identifier {:>6}  {:>6.2}% of wrong",
        total.identifiers,
        pct(total.identifiers, total.wrong)
    );

    println!("\n--- wrong lines by node kind ---");
    let mut kinds: Vec<_> = total.wrong_kinds.iter().map(|(k, v)| (*v, k.as_str())).collect();
    kinds.sort_unstable_by(|a, b| b.cmp(a));
    for (count, kind) in kinds.iter().take(12) {
        println!("  {kind:<34} {count:>9}  {:>6.2}%", pct(*count, total.wrong));
    }

    println!("\n--- wrong Identifier lines, attributed (first match wins) ---");
    let mut ordered: Vec<Cause> = arms.to_vec();
    ordered.push(Cause::Unattributed);
    for cause in ordered {
        let count = total.first_match.get(&cause).copied().unwrap_or(0);
        // The control prints even at zero: a bucket that vanishes when empty
        // cannot tell you the model is complete.
        println!("  {:<44} {count:>9}  {:>6.2}%", cause.label(), pct(count, total.identifiers));
    }
    println!(
        "\n  matched more than one arm {:>9}  {:>6.2}% of attributed identifiers",
        total.multi_arm,
        pct(total.multi_arm, total.identifiers)
    );

    println!("\n--- the same arms, non-exclusively ---");
    for cause in arms {
        let count = total.any_match.get(cause).copied().unwrap_or(0);
        println!("  {:<44} {count:>9}  {:>6.2}%", cause.label(), pct(count, total.identifiers));
    }

    println!(
        "\n  of the wrong identifiers, `typeof {{upstream}}`  {:>7}  {:>6.2}%  <- one defect",
        total.typeof_of_want,
        pct(total.typeof_of_want, total.identifiers)
    );

    println!(
        "  of the wrong identifiers, `{{ours}} | undefined`  {:>7}  {:>6.2}%  <- one rule",
        total.missing_undefined,
        pct(total.missing_undefined, total.identifiers)
    );
    println!(
        "    of those, where ours is a type parameter    {:>7}  {:>6.2}% of that rule",
        total.missing_undefined_on_type_parameter,
        pct(total.missing_undefined_on_type_parameter, total.missing_undefined)
    );

    println!("\n--- WE SAID `any`, UPSTREAM DID NOT: the largest defect population ---");
    println!(
        "  (non-exclusive: a line may match several. UNATTRIBUTED prints even at\n\
        \x20  zero -- a six-bucket classifier produces a tidy table either way.)"
    );
    println!("  population                                  {:>9}", total.implicit_any_total);
    let mut rows: Vec<_> =
        total.implicit_any.iter().map(|(cause, count)| (*count, *cause)).collect();
    rows.sort_unstable_by(|a, b| b.cmp(a));
    for (count, cause) in rows {
        println!("  {cause:<48} {count:>8}  {:>6.2}%", pct(count, total.implicit_any_total));
    }
    println!(
        "  {:<48} {:>8}  {:>6.2}%",
        "UNATTRIBUTED (control)",
        total.implicit_any_unattributed,
        pct(total.implicit_any_unattributed, total.implicit_any_total)
    );

    println!("\n--- OVER-APPLICATION: ours has `| undefined`, upstream does not ---");
    println!(
        "  (the optionality rule's own defect. Compared against the base rate of\n\
        \x20  wrong lines in the same kind of case, because a bucket that is large\n\
        \x20  because that kind of case is large is not evidence.)"
    );
    let over_total: usize = total.over_application.values().sum();
    println!("  total                                       {over_total:>9}");
    let mut rows: Vec<_> = total.wrong_by_strictness.iter().collect();
    rows.sort_by_key(|(k, _)| *k);
    for (bucket, wrong_here) in rows {
        let over = total.over_application.get(*bucket).copied().unwrap_or(0);
        println!(
            "  {bucket:<32} {over:>7} of {wrong_here:>7} wrong  {:>6.2}% of that bucket, {:>6.2}% of all over-application",
            pct(over, *wrong_here),
            pct(over, over_total)
        );
    }

    println!("\n--- CONTEXTUAL-TYPING PARAMETERS, BY WHAT WOULD TYPE THEM ---");
    println!("  (the parent of the function the parameter belongs to)");
    let ctx_total: usize = total.contextual_parameter_context.values().sum();
    let mut rows: Vec<_> = total
        .contextual_parameter_context
        .iter()
        .map(|(kind, count)| (*count, kind.as_str()))
        .collect();
    rows.sort_unstable_by(|a, b| b.cmp(a));
    for (count, kind) in rows.iter().take(12) {
        println!("  {kind:<42} {count:>8}  {:>6.2}%", pct(*count, ctx_total));
    }

    println!("\n--- WRONG LINES BY SYMBOL FLAGS ---");
    println!(
        "  (parent kind names the construct; flags name the dispatch. A merged\n\
        \x20  accessor carrying METHOD | GET_ACCESSOR | SET_ACCESSOR is a dispatch\n\
        \x20  bug that no syntactic bucketing can see.)"
    );
    let mut rows: Vec<_> =
        total.wrong_by_flags.iter().map(|(f, count)| (*count, f.as_str())).collect();
    rows.sort_unstable_by(|a, b| b.cmp(a));
    for (count, flags) in rows.iter().take(12) {
        println!("  {flags:<58} {count:>7}  {:>6.2}%", pct(*count, total.identifiers));
    }

    println!("\n--- GAP LINES OWNED BY `FunctionTypeNode` ---");
    println!("  all gap lines                               {:>9}", total.gap_lines);
    let ft_total: usize = total.function_type_gaps.values().sum();
    let mut rows: Vec<_> = total.function_type_gaps.iter().map(|(k, v)| (*v, *k)).collect();
    rows.sort_unstable_by(|a, b| b.cmp(a));
    for (count, bucket) in &rows {
        println!("  {bucket:<46} {count:>8}  {:>6.2}% of gaps", pct(*count, total.gap_lines));
    }
    println!(
        "  {:<46} {ft_total:>8}  {:>6.2}% of gaps",
        "TOTAL owned by FunctionTypeNode",
        pct(ft_total, total.gap_lines)
    );

    println!("\n--- THE QualifiedName POPULATION, BY GRANDPARENT ---");
    println!(
        "  (parent is QualifiedName for every reading; the grandparent is what\n\
        \x20  separates `import X = M.I` from the left of a type reference `M.I`.)"
    );
    let qn_total: usize = total.qualified_name_grandparent.values().sum();
    let mut rows: Vec<_> = total
        .qualified_name_grandparent
        .iter()
        .map(|(kind, count)| (*count, kind.as_str()))
        .collect();
    rows.sort_unstable_by(|a, b| b.cmp(a));
    for (count, kind) in rows.iter().take(10) {
        println!("  {kind:<42} {count:>8}  {:>6.2}%", pct(*count, qn_total));
    }

    println!("\n--- WRONG LINES BY PARENT KIND ---");
    println!(
        "  (the question that cracked the 1,086: every one of them had parent\n\
        \x20  ExpressionWithTypeArguments. A defect concentrated in one parent kind\n\
        \x20  is one rule; one spread evenly is a population.)"
    );
    let mut rows: Vec<_> =
        total.wrong_by_parent.iter().map(|(kind, count)| (*count, kind.as_str())).collect();
    rows.sort_unstable_by(|a, b| b.cmp(a));
    for (count, kind) in rows.iter().take(12) {
        println!("  {kind:<42} {count:>8}  {:>6.2}%", pct(*count, total.identifiers));
    }

    println!("\n--- THE {} OVER-APPLICATIONS `strictNullChecks` CANNOT EXPLAIN ---", {
        let n: usize = total.over_application_strict_on.values().sum();
        n
    });
    let strict_on_total: usize = total.over_application_strict_on.values().sum();
    let mut rows: Vec<_> = total
        .over_application_strict_on
        .iter()
        .map(|(kind, count)| (*count, kind.as_str()))
        .collect();
    rows.sort_unstable_by(|a, b| b.cmp(a));
    for (count, kind) in rows.iter().take(10) {
        println!("  {kind:<42} {count:>8}  {:>6.2}%", pct(*count, strict_on_total));
    }

    println!("\n--- POSITIVE CONTROL: the same arms over ALL aligned Identifier lines ---");
    println!(
        "  (an arm reading 0 above is only evidence if it is non-zero here;\n\
        \x20  otherwise the arm is broken and its zero means nothing)"
    );
    println!("  aligned Identifier lines, total           {:>9}", total.control_total);
    println!(
        "  of which the name resolves to NO symbol   {:>9}  {:>6.2}%",
        total.control_unresolved,
        pct(total.control_unresolved, total.control_total)
    );
    for cause in Cause::ORDER {
        let count = total.control_population.get(&cause).copied().unwrap_or(0);
        println!("  {:<44} {count:>9}  {:>6.2}%", cause.label(), pct(count, total.control_total));
    }

    println!("\n--- commonest substitutions on a wrong Identifier ---");
    let mut pairs: Vec<_> = total.pairs.iter().map(|((w, g), c)| (*c, w, g)).collect();
    pairs.sort_unstable_by(|a, b| b.cmp(a));
    for (count, want, got) in pairs.iter().take(15) {
        println!("  {count:>8}  {want} -> {got}");
    }

    for cause in ordered_causes(arms) {
        let Some(examples) = total.samples.get(&cause) else { continue };
        if examples.is_empty() {
            continue;
        }
        println!("\n--- examples: {} ---", cause.label());
        for (case, text, want, got) in examples {
            let text: String = text.chars().take(48).collect();
            println!("  {case}\n    {text}\n      want {want}\n      got  {got}");
        }
    }
}

fn ordered_causes(arms: &[Cause]) -> Vec<Cause> {
    let mut out = arms.to_vec();
    out.push(Cause::Unattributed);
    out
}

/// Every name the bundled lib files declare at top level.
///
/// **Copied from `examples/types_shapes.rs`** — one example cannot import
/// another — with its caveats intact: it takes a declaration to be top-level
/// when its keyword starts at column 0, and it does not parse, so it can
/// over-report and cannot under-report a genuinely top-level name. Treat it as
/// an upper bound.
fn lib_declared_names() -> HashSet<String> {
    const KEYWORDS: [&str; 8] = [
        "declare var ",
        "declare const ",
        "declare let ",
        "declare function ",
        "declare namespace ",
        "declare type ",
        "interface ",
        "declare class ",
    ];
    let dir = repo_root().join("vendor/typescript-go/internal/bundled/libs");
    let mut names = HashSet::new();
    let Ok(entries) = std::fs::read_dir(&dir) else { return names };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "ts") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        for line in text.lines() {
            for keyword in KEYWORDS {
                let Some(rest) = line.strip_prefix(keyword) else { continue };
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                    .collect();
                if !name.is_empty() {
                    names.insert(name);
                }
            }
        }
    }
    names
}
