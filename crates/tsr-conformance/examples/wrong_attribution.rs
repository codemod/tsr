//! Where the **wrong** answers come from, attributed per line to the change
//! that could have produced them — `bd tsr-y4u.21` follow-up.
//!
//! `cargo run -p tsr-conformance --example wrong_attribution --release`
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

            for expected_file in &expected {
                let Some(unit) = parsed.files.iter().find(|u| {
                    tsr_conformance::binder_suite::same_unit(&u.name, &expected_file.file)
                }) else {
                    continue;
                };
                if tsr_parser::ScriptKind::from_file_name(&unit.name)
                    == tsr_parser::ScriptKind::Json
                {
                    continue;
                }
                let arena = tsr_core::Arena::new();
                let options = tsr_parser::ParseOptions {
                    jsdoc: false,
                    ..tsr_parser::ParseOptions::for_file(&unit.name)
                };
                let file = tsr_parser::parse_with_options(&arena, &unit.content, options);
                let bound = tsr_binder::bind(
                    file.source_file,
                    &file.nodes,
                    tsr_binder::FileInfo { name: &unit.name, text: &unit.content },
                );
                let mut checker = tsr_checker::Checker::new(&bound, &file.nodes, &file.node_map);
                // The ids come back through the closure the producer already
                // takes, in the walker's own order — so `ids[i]` is the node
                // behind `rendered[i]` without re-deriving the walk.
                let mut ids = Vec::new();
                let rendered = types_producer::assertions_for_file(
                    &Node::SourceFile(file.source_file),
                    &unit.content,
                    &file.nodes,
                    &file.node_map,
                    |id| {
                        ids.push(id);
                        types_producer::type_at_location(
                            &mut checker,
                            &bound,
                            &file.nodes,
                            &file.node_map,
                            id,
                        )
                    },
                );
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
                            &bound,
                            &file.nodes,
                            &file.node_map,
                            ids[position],
                            &got.text,
                            want_type,
                            &got.type_string,
                            &lib_names,
                            &Cause::ORDER,
                        );
                        if symbol_of_identifier(&bound, &file.nodes, &file.node_map, ids[position])
                            .is_none()
                        {
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
                    if want_type == format!("{} | undefined", got.type_string) {
                        tally.missing_undefined += 1;
                        if type_parameters_in_scope(&bound, &file.nodes, ids[position])
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
                        &bound,
                        &file.nodes,
                        &file.node_map,
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
        "  * The `checker_types` producer binds each unit ON ITS OWN and loads no\n\
         \x20   lib files (`assertions_for_case`, no program). So the lib arm can only\n\
         \x20   ever fire on a name that resolves NOWHERE and is spelled like a lib\n\
         \x20   name. If it is near zero that is a property of this entry point, NOT\n\
         \x20   evidence that lib loading is irrelevant to the corpus."
    );
    println!(
        "  * The model is therefore EFFECTIVELY THREE ARMS. The lib arm is kept and\n\
         \x20   printed because a zero reading is the evidence for that; a non-zero one\n\
         \x20   would mean the producer does something other than what its source says."
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
