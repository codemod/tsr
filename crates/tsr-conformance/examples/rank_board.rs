//! The ranked board: every remaining `checker_types` line, partitioned, with the
//! three statistics a row needs before it can be ranked — `bd tsr-g27`.
//!
//! `cargo run -p tsr-conformance --example rank_board --release`
//!
//! # What this adds over `examples/types_shapes.rs`
//!
//! `types_shapes` buckets by upstream's answer shape and by the node kind we
//! failed on. That is level 3 of `docs/conventions.md`'s four levels and it is
//! the right level, but it leaves three questions a ranking cannot skip:
//!
//! 1. **Is the prerequisite met?** 37% of the gap sits under
//!    `expression answered error: X`, which is a *catch-all*: it fires both when
//!    `X`'s own rule is unported and when `X` is ported and merely propagating an
//!    operand's `error`. Those are opposite work items and the label cannot tell
//!    them apart. This probe splits them by asking whether **any sub-expression
//!    of the same node also gapped** — nested strictly inside its span, in the
//!    same file. Nothing gapped below it means everything it is built from was
//!    typed, so the missing thing is the node's own rule: **TERMINAL**, the
//!    prerequisite is met, kind 1. Something gapped below it means the row's size
//!    is not the row's worth: **PROPAGATED**, kind 2.
//! 2. **Is the row a sum over the corpus or one file?** Every row carries its
//!    case count, its largest case's share and its top-10 share. Three rows on
//!    this board evaporate on that check.
//! 3. **What else in the same case still fails?** Levels 1–3 predict *lines*;
//!    only this predicts *cases*. For each row, over the cases it touches, this
//!    counts how many would have **nothing left** if the row were closed — the
//!    cases the row *finishes*, not the cases it touches.
//!
//! # The control bucket
//!
//! `UNCLASSIFIED` is printed unconditionally and must read zero. It is the only
//! evidence that these rows are a partition rather than a list of true facts.
//! Three totals are re-derived and asserted on every run: right + gap + wrong =
//! aligned, the family roll-up = the gap total, and the per-case residual sum =
//! the corpus residual.
//!
//! # Only aligned lines carry a type comparison; every line carries the case gate
//!
//! A type can only be compared where the walker produced upstream's expression
//! text (`examples/types_shapes.rs` explains the alignment test). But the
//! `checker_types` **case** gate is whole-baseline and positional, so an
//! unaligned line fails its case just as a wrong type does
//! (`crates/tsr-conformance/src/types_suite.rs:150`). The per-case residual used
//! for the level-4 statistic is therefore `upstream lines - exactly matched
//! lines`, over *all* lines, and not over aligned ones.

use std::collections::HashMap;

use rayon::prelude::*;
use tsr_conformance::{Corpus, repo_root, type_shape, types_baseline, types_producer};

/// One case's contribution: its residual under the case gate, and its gap and
/// wrong lines keyed by row.
struct CaseReport {
    name: String,
    /// Upstream lines this case has.
    expected: usize,
    /// Lines we reproduced exactly — text *and* type.
    matched: usize,
    /// Gap lines by `(row, cause)`.
    gaps: HashMap<(String, Cause), usize>,
    /// Wrong lines by node kind and shape substitution — ported and defective,
    /// a different work item from a gap.
    wrong: HashMap<String, usize>,
    /// Wrong lines as exact `upstream -> ours` pairs, bounded per case.
    pairs: HashMap<(String, String), usize>,
    /// Lines we answered `any` on and upstream did too — banked, and only
    /// honest where the checker computed it rather than defaulted to it.
    any_credited: usize,
    /// The same, by case, so the concentration check applies to it too.
    any_by_case: HashMap<String, usize>,
    /// Lines we answered `any` on and upstream did not — a confident wrong
    /// answer, and the direction that proves the rule is being broken.
    any_wrong: usize,
    /// Lines where `error` reached a *printed* type, such as `error[]` — the
    /// gap marker leaking into an answer instead of stopping the line.
    error_leaked: usize,
    /// Gap lines the TERMINAL/PROPAGATED split could not attribute. A second
    /// control bucket, printed unconditionally, and it must read zero.
    unattributed: usize,
    aligned: usize,
}

/// How much of a gap line's *cause* is already known to be another gap.
///
/// The span test alone is not enough, and the way it fails is instructive. It
/// asks whether a gapped line sits **inside this node's span**, which is exactly
/// right for an expression — `a[b]`, `f(x)`, `x = y` all contain their operands.
/// It is wrong for a node whose dependency is its *sibling*:
///
/// - the `b` of `a.b` is a leaf, so no line is inside it, yet its reason says
///   `the receiver is a gap` — 6,749 lines that the span test called TERMINAL
///   and that are nothing of the kind;
/// - a declaration name `x` in `var x = f()` spans only `x`, while the gap it
///   waits on is `f()` beside it.
///
/// So the span test is used where it is valid and the reason string is used
/// where it is not, and the cases the reason string leaves open are reported as
/// their own bucket rather than folded into either answer.
/// What the span test found beneath one rendered line.
///
/// Two fields rather than one boolean, because *"nothing inside this node
/// gapped"* and *"this node has nothing inside it"* are the same `false` and
/// mean opposite things. Collapsing them is what let a leaf claim
/// [`Cause::Terminal`].
#[derive(Clone, Copy)]
struct Below {
    /// A rendered line strictly inside this node's span answered `error`.
    inner_gapped: bool,
    /// There was at least one rendered line strictly inside this node's span.
    has_inner: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
enum Cause {
    /// Nothing this line depends on is known to have gapped. The row's own rule
    /// is the whole of the missing work — kind 1, and the size is its worth.
    Terminal,
    /// A sub-expression inside this node's span already gapped.
    PropagatedSpan,
    /// The reason names a dependency that is itself a gap.
    PropagatedNamed,
    /// The reason names a dependency the span test cannot see — an initialiser
    /// or an annotation beside the node rather than inside it, a symbol declared
    /// elsewhere, or a name that never resolved — and this probe does not measure
    /// whether it gapped. **Not** evidence of either answer.
    Unknown,
    /// **The control.** No arm matched: the reason names no dependency, and the
    /// node has nothing inside its span for the span test to look at, so there is
    /// no evidence either way.
    ///
    /// This bucket exists because the arm below it used to be `else`. A default
    /// arm carrying the load-bearing label absorbs every line no test matched and
    /// reports them as the strongest claim the classifier makes; the control
    /// beside it then reads zero on every run and proves nothing, because
    /// `cause()` is total. A *declaration name* is the worked example — it spans
    /// only itself, so `gapped_below` cannot fire for it, and it used to land in
    /// `Terminal` whatever it actually depended on. One row so labelled, 2,618
    /// lines, measured **68.6% propagated** when someone finally walked it.
    /// See `bd tsr-eyn` and the correction header on
    /// `docs/architecture/checker-notes-rank.md`.
    Unmatched,
}

impl Cause {
    fn label(self) -> &'static str {
        match self {
            Self::Terminal => "TERMINAL",
            Self::PropagatedSpan => "propagated/span",
            Self::PropagatedNamed => "propagated/named",
            Self::Unknown => "DEPENDENT-UNKNOWN",
            Self::Unmatched => "UNMATCHED",
        }
    }
}

/// Which of four kinds of evidence a gap line carries — and **`Terminal` has to
/// earn it**.
///
/// `has_inner` is the arm that makes the difference. `Terminal` is the claim
/// *"everything this node is built from was typed, so the node's own rule is the
/// whole of the missing work"*, and that claim is only available if the node **is**
/// built from something this probe looked at. A leaf has nothing inside its span,
/// so `inner_gapped` is vacuously `false` for it, and reading that as *"nothing
/// below gapped"* is reading the absence of a test as a passing test.
///
/// So a leaf falls to [`Cause::Unmatched`], which is the control, and the control
/// can now be reached — the property `docs/conventions.md` demands of every
/// control and the previous `else` arm made impossible.
fn cause(reason: &str, inner_gapped: bool, has_inner: bool) -> Cause {
    if reason.contains("the receiver is a gap") {
        Cause::PropagatedNamed
    } else if inner_gapped {
        Cause::PropagatedSpan
    } else if reason.contains("/ initialiser ")
        || reason.contains("/ annotation ")
        // Both name a dependency that is not inside this node's span: the symbol
        // is declared somewhere this probe did not look, or no declaration was
        // found at all. Neither is evidence that the work is local to this row,
        // and both used to fall through to `Terminal`.
        || reason.contains("no value declaration")
        || reason.contains("the name does not resolve")
    {
        Cause::Unknown
    } else if has_inner {
        Cause::Terminal
    } else {
        Cause::Unmatched
    }
}

/// The volatile tail of a reason, removed so a row is a row and not a type.
///
/// `... has no such property: Promise<boolean>` and `... : SymbolConstructor`
/// are the same work item and 1,400 distinct rows if the receiver type is left
/// in the key. The tails that are *not* removed are node kinds and symbol
/// flags, which are low-cardinality and are what names the work.
fn row_key(reason: &str) -> String {
    for cut in ["has no such property: ", "unresolved: "] {
        if let Some(at) = reason.find(cut) {
            return reason[..at + cut.len() - 2].to_string();
        }
    }
    reason.to_string()
}

/// The family a row belongs to. Ordered, and the order is load-bearing: several
/// reasons satisfy more than one test.
///
/// Mirrors `examples/types_shapes.rs`'s roll-up so the two can be compared line
/// for line, with four rows added that fell into its `other` bucket (6,943 lines
/// at `0e8e902`). `UNCLASSIFIED` is the control.
fn family(reason: &str) -> &'static str {
    if reason.starts_with("type declaration name") {
        "a type alias whose right-hand side gaps"
    } else if reason.starts_with("neither a declaration name nor an expression") {
        "a node that is neither a declaration name nor an expression"
    } else if reason.contains("symbol has a type (") {
        "the symbol has a type; the line differs for another reason"
    } else if reason.contains("the receiver is a gap") {
        "a property access whose receiver we cannot type"
    } else if reason.starts_with("property access") || reason.starts_with("member name,") {
        "a property access whose property we cannot find or type"
    } else if reason.contains("annotation") {
        "a type node we cannot resolve (getTypeFromTypeNode / declared types)"
    } else if reason.contains("initialiser") {
        "an initialiser expression we do not compute"
    } else if reason.starts_with("expression answered error") {
        "an expression we do not compute (unported form, or an error operand)"
    } else if reason.contains("the name of a") {
        "a member name, resolved as if it were free (bd tsr-tl8)"
    } else if reason.contains("does not resolve") {
        "a free name that does not resolve, in value position"
    } else if reason.contains("no value declaration") {
        // Distinct from the row below and it was inside its `other` bucket:
        // there is no declaration to take an annotation or an initialiser from,
        // so `getTypeOfSymbol` has nothing to dispatch *on* rather than no arm
        // to dispatch *to*. Aliases and export markers, and the work is the
        // binder's, not the checker's.
        "a symbol with no value declaration at all (alias, export marker)"
    } else if reason.contains("neither") {
        "a symbol whose kind getTypeOfSymbol does not handle"
    } else {
        "UNCLASSIFIED"
    }
}

/// The two properties the partition rests on, checked **before** anything is
/// measured and on every run.
///
/// They are assertions in `main` rather than `#[cfg(test)]` tests because
/// Cargo does not run tests inside an example unless the manifest declares
/// `test = true`, and `crates/tsr-conformance/Cargo.toml` is a shared manifest
/// three other agents are editing this cycle (`docs/conventions.md`, "on a
/// shared manifest, commit the index you constructed"). A check that runs on
/// every measurement is in any case the stronger statement.
///
/// Each was verified red under a named mutation, and neither mutation reddens
/// the other — recorded in `docs/architecture/checker-notes-rank.md`.
fn check_classifier() {
    // ORDER. `... the receiver is a gap: Identifier` also starts with
    // "property access", and the two families are opposite work items: one is
    // the receiver's own failure propagating, the other is the lookup. If the
    // property-access arm is tested first, 19,166 lines move family silently.
    assert_eq!(
        family("property access, the receiver is a gap: Identifier"),
        "a property access whose receiver we cannot type",
        "the receiver arm must be tested before the property arm"
    );
    // ORDER, the other one that bites: a symbol with no value declaration has
    // no annotation and no initialiser, so it reaches the bottom of the chain
    // and would land in UNCLASSIFIED — 5,743 lines — without its own arm.
    assert_eq!(
        family("reference, symbol has no type: SymbolFlags(ALIAS) / no value declaration"),
        "a symbol with no value declaration at all (alias, export marker)",
        "a symbol with no value declaration is its own row, not UNCLASSIFIED"
    );

    // COLLAPSE. The receiver's *type* is unbounded — every distinct type in the
    // corpus would be its own row — and it is not what names the work, so it is
    // cut. The receiver's *node kind* is bounded and is exactly what names the
    // work, so it is kept. A key that collapses both, or neither, is not a row.
    assert_eq!(
        row_key("property access, the receiver has no such property: Promise<boolean>"),
        row_key("property access, the receiver has no such property: SymbolConstructor"),
        "the receiver type must be cut from the key"
    );
    assert_ne!(
        row_key("property access, the receiver is a gap: Identifier"),
        row_key("property access, the receiver is a gap: PropertyAccessExpression"),
        "the receiver node kind must be kept in the key"
    );

    // CAUSE. The span test must not be trusted on a leaf whose dependency is
    // its sibling. `member name, the receiver is a gap` has no line inside it
    // by construction, so a span-only classifier calls 6,749 lines TERMINAL.
    assert_eq!(
        cause("member name, the receiver is a gap: Identifier", false, false),
        Cause::PropagatedNamed,
        "a named gapped dependency outranks the span test"
    );
    assert_eq!(
        cause(
            "declaration name, symbol has no type: SymbolFlags(X) / initialiser CallExpression",
            false,
            false
        ),
        Cause::Unknown,
        "a sibling initialiser is not evidence of TERMINAL"
    );
    assert_eq!(
        cause("expression answered error: ElementAccessExpression", false, true),
        Cause::Terminal,
        "an expression with something inside it, none of it gapped, is TERMINAL"
    );
    // **The regression this classifier was corrected for.** A declaration name
    // spans only itself, so it reaches `cause` with `has_inner == false`, and the
    // old `else` arm answered TERMINAL — the strongest claim on the page — on no
    // evidence at all. One row so labelled measured 68.6% propagated.
    assert_eq!(
        cause("declaration name, symbol has no type: SymbolFlags(X) / neither", false, false),
        Cause::Unmatched,
        "a leaf has nothing inside it, so `false` is the absence of a test and not a passing one"
    );
    assert_eq!(
        cause(
            "reference, symbol has no type: SymbolFlags(ALIAS) / no value declaration",
            false,
            true
        ),
        Cause::Unknown,
        "a symbol declared somewhere this probe did not look is not evidence of TERMINAL"
    );
    assert_eq!(
        cause("reference, the name does not resolve", false, true),
        Cause::Unknown,
        "an unresolved name depends on resolution, not on this node"
    );
    // The control has to be reachable, and it has to be the ONLY thing an
    // unmatched line can reach. `cause` is still total, but `Terminal` is no
    // longer what totality falls through to.
    assert_eq!(
        cause("something no arm has ever seen", false, false),
        Cause::Unmatched,
        "an unrecognised reason on a leaf reaches the control, not TERMINAL"
    );
}

fn main() {
    check_classifier();
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    assert!(corpus.is_available(), "corpus missing; run git submodule update --init --recursive");
    let cases = corpus.discover().expect("discovering cases");

    let reports: Vec<CaseReport> = cases
        .par_iter()
        .filter_map(|case| {
            // The suite's own skips, skip for skip, so every share here is a
            // share of the gradient's denominator and not of some other set.
            if case.has_varied_types() || case.has_known_divergence() {
                return None;
            }
            let text = case.expected_types()?;
            let expected = types_baseline::parse(&text);
            if types_baseline::assertion_count(&expected) == 0 {
                return None;
            }
            let parsed = case.load().ok()?;

            // The ids entry point, so a gap node's span can be tested against
            // the spans of the lines nested inside it. It renders without
            // explanations, so `gap_reason` is called here against a checker
            // over the same program — the one `render_case` itself uses.
            let arena = tsr_core::Arena::new();
            let (program, ours, ids) =
                types_producer::assertions_for_case_with_ids(&arena, &parsed, &expected);
            let nodes = program.nodes();
            let node_map = program.node_map();
            let bound = program.binder();
            let mut checker = tsr_checker::Checker::new(bound, nodes, node_map);

            let mut report = CaseReport {
                name: case.name.clone(),
                expected: 0,
                matched: 0,
                gaps: HashMap::new(),
                wrong: HashMap::new(),
                pairs: HashMap::new(),
                any_credited: 0,
                any_by_case: HashMap::new(),
                any_wrong: 0,
                error_leaked: 0,
                unattributed: 0,
                aligned: 0,
            };

            for (index, expected_file) in expected.iter().enumerate() {
                let our_file = ours.get(index);
                let our_ids = ids.get(index);

                // TERMINAL / PROPAGATED. The walker emits in source preorder, so
                // a node's descendants are the contiguous run of later lines
                // whose span lies inside its own. A gap line with no gapped line
                // below it is TERMINAL: everything the node is built from was
                // typed, so the node's own rule is the whole of the missing
                // work. Anything else is PROPAGATED and its row size is an upper
                // bound of unknown depth.
                //
                // The runs are short, so one forward scan per line is cheap
                // enough to pay on every line of the corpus.
                //
                // **Two facts per line, not one**, and the second is what stops
                // a leaf from being read as a passing test. `inner_gapped` says
                // a rendered line strictly inside this node's span answered
                // `error`; `has_inner` says there was any such line to ask about
                // at all. A declaration name has none, so `inner_gapped` is
                // `false` for it for the same reason `false` is the answer for a
                // node whose every child typed — and only `has_inner`
                // distinguishes those.
                let below: Vec<Below> = match (our_file, our_ids) {
                    (Some(file), Some(line_ids)) if file.len() == line_ids.len() => (0..file.len())
                        .map(|i| {
                            if file[i].type_string != "error" {
                                return Below { inner_gapped: false, has_inner: false };
                            }
                            let outer = nodes.span(line_ids[i]);
                            let inside: Vec<usize> = (i + 1..file.len())
                                .take_while(|&j| {
                                    let inner = nodes.span(line_ids[j]);
                                    inner.start >= outer.start && inner.end <= outer.end
                                })
                                .collect();
                            Below {
                                inner_gapped: inside
                                    .iter()
                                    .any(|&j| file[j].type_string == "error"),
                                has_inner: !inside.is_empty(),
                            }
                        })
                        .collect(),
                    _ => Vec::new(),
                };

                for (position, want) in expected_file.assertions.iter().enumerate() {
                    report.expected += 1;
                    let Some(got) = our_file.and_then(|file| file.get(position)) else { continue };
                    let Some(want_type) = want.text.strip_prefix(&format!("{} : ", got.text))
                    else {
                        continue;
                    };
                    report.aligned += 1;
                    // The `any` audit. `docs/conventions.md` and ADR-0039 both
                    // forbid answering `any` where the checker cannot compute a
                    // type, because a wrong answer that looks like an answer is
                    // invisible in the aggregate and falsely credits lines.
                    // Whether the rule is *held* has never been measured, so
                    // both directions are counted here: lines we bank by saying
                    // `any`, and lines we lose by saying it.
                    if got.type_string == "any" {
                        if want_type == "any" {
                            report.any_credited += 1;
                            *report.any_by_case.entry(case.name.clone()).or_default() += 1;
                        } else {
                            report.any_wrong += 1;
                        }
                    }
                    // `error` *is* the gap marker, so the leak is only the
                    // lines where it is embedded in a larger printed type.
                    if got.type_string != "error" && got.type_string.contains("error") {
                        report.error_leaked += 1;
                    }
                    if want_type == got.type_string {
                        report.matched += 1;
                        continue;
                    }
                    if got.type_string != "error" {
                        // Keyed by the *shape substitution* as well as the node
                        // kind. "31,093 wrong Identifiers" is not a work item;
                        // "we answered a literal where upstream widened it" is.
                        let key = format!(
                            "{:?}  {} -> {}",
                            got.kind,
                            type_shape::classify(want_type).label(),
                            type_shape::classify(&got.type_string).label()
                        );
                        *report.wrong.entry(key).or_default() += 1;
                        if report.pairs.len() < 400 {
                            *report
                                .pairs
                                .entry((want_type.to_string(), got.type_string.clone()))
                                .or_default() += 1;
                        }
                        continue;
                    }
                    let id = our_ids.expect("ids beside a rendered file")[position];
                    let reason =
                        types_producer::gap_reason(&mut checker, bound, nodes, node_map, id);
                    // A line with no span data is UNATTRIBUTED rather than
                    // silently TERMINAL; the control below counts it.
                    let Some(&Below { inner_gapped, has_inner }) = below.get(position) else {
                        report.unattributed += 1;
                        continue;
                    };
                    *report
                        .gaps
                        .entry((row_key(&reason), cause(&reason, inner_gapped, has_inner)))
                        .or_default() += 1;
                }
            }
            Some(report)
        })
        .collect();

    report(&reports);
}

/// The signed difference of two counts. Every control bucket is a difference
/// that must read zero, and `usize - usize` cannot express a negative one.
fn delta(left: usize, right: usize) -> i128 {
    i128::try_from(left).unwrap_or_default() - i128::try_from(right).unwrap_or_default()
}

#[allow(clippy::cast_precision_loss)]
fn pct(n: usize, d: usize) -> f64 {
    if d == 0 { 0.0 } else { n as f64 / d as f64 * 100.0 }
}

/// Everything a row needs before it can be ranked.
#[derive(Default)]
struct Row {
    lines: usize,
    /// Lines by case, which is what the concentration check reads.
    by_case: HashMap<String, usize>,
    /// For each case the row touches: what would remain in that case if this
    /// row were closed — the level-4 statistic.
    residual_after: Vec<usize>,
}

impl Row {
    /// Cases, largest case's share, top-10 share.
    fn concentration(&self) -> (usize, f64, f64) {
        let mut counts: Vec<usize> = self.by_case.values().copied().collect();
        counts.sort_unstable_by(|a, b| b.cmp(a));
        let top1 = counts.first().copied().unwrap_or_default();
        let top10: usize = counts.iter().take(10).sum();
        (counts.len(), pct(top1, self.lines), pct(top10, self.lines))
    }

    /// Cases this row would **finish** — nothing else left in them — and the
    /// median of what is left in the rest.
    fn finishes(&self) -> (usize, usize) {
        let mut rest = self.residual_after.clone();
        rest.sort_unstable();
        let finished = rest.iter().take_while(|&&r| r == 0).count();
        let median = rest.get(rest.len() / 2).copied().unwrap_or_default();
        (finished, median)
    }
}

fn report(reports: &[CaseReport]) {
    let cases = reports.len();
    let expected: usize = reports.iter().map(|r| r.expected).sum();
    let aligned: usize = reports.iter().map(|r| r.aligned).sum();
    let matched: usize = reports.iter().map(|r| r.matched).sum();
    let gap_total: usize = reports.iter().flat_map(|r| r.gaps.values()).sum();
    let wrong_total: usize = reports.iter().flat_map(|r| r.wrong.values()).sum();

    println!("cases judged:             {cases}");
    println!("assertion lines upstream: {expected}");
    println!("  aligned:                {aligned} ({:.2}%)", pct(aligned, expected));
    println!(
        "  exactly right:          {matched} ({:.2}% of upstream lines, {:.2}% of aligned)",
        pct(matched, expected),
        pct(matched, aligned)
    );
    println!("  gap  (we said `error`): {gap_total}");
    println!("  wrong (ported, defect): {wrong_total}");

    // CONTROL 1. The three outcomes must exhaust the aligned lines.
    println!(
        "\nCONTROL right+gap+wrong-aligned = {} (must be 0)",
        delta(matched + gap_total + wrong_total, aligned)
    );

    // Roll rows up, and carry each case's residual with them.
    let mut rows: HashMap<(String, Cause), Row> = HashMap::new();
    let mut families: HashMap<&'static str, usize> = HashMap::new();
    let mut residual_total = 0usize;
    for case in reports {
        let residual = case.expected - case.matched;
        residual_total += residual;
        for ((key, kind), count) in &case.gaps {
            *families.entry(family(key)).or_default() += count;
            let row = rows.entry((key.clone(), *kind)).or_default();
            row.lines += count;
            *row.by_case.entry(case.name.clone()).or_default() += count;
            row.residual_after.push(residual - count);
        }
    }

    // CONTROL 2. Families partition the gap, and UNCLASSIFIED is empty.
    let family_total: usize = families.values().sum();
    println!("CONTROL family roll-up - gap total = {} (must be 0)", delta(family_total, gap_total));
    println!(
        "CONTROL UNCLASSIFIED = {} (must be 0)",
        families.get("UNCLASSIFIED").copied().unwrap_or_default()
    );
    // A control that only prints a number cannot be acted on when it is
    // non-zero. Print what fell through, so the next reader repairs the
    // partition instead of discounting it.
    let mut stray: Vec<_> = rows
        .iter()
        .filter(|((k, _), _)| family(k) == "UNCLASSIFIED")
        .map(|(k, r)| (r.lines, k.0.clone()))
        .collect();
    stray.sort_unstable_by(|a, b| b.cmp(a));
    for (count, key) in stray.iter().take(10) {
        println!("    UNCLASSIFIED {count:>7}  {key}");
    }
    // CONTROL 3. The per-case residual is the corpus residual.
    let unattributed: usize = reports.iter().map(|r| r.unattributed).sum();
    println!(
        "CONTROL lines with no span data at all = {unattributed} (must be 0; a line the \
         span test could not be run on)"
    );
    // **This one is a measurement, not a must-be-zero, and that is the fix.**
    // It used to read `CONTROL UNATTRIBUTED by the TERMINAL/PROPAGATED split =
    // 0 (must be 0)` over a classifier whose last arm was `else`. Since `cause`
    // was total, nothing could ever be unattributed: the bucket was structurally
    // incapable of reading non-zero, read zero on every run, and proved nothing
    // on any of them — while the `else` it was supposed to be watching quietly
    // absorbed every unmatched line into TERMINAL, the strongest claim the page
    // makes. `Terminal` now needs a positive test (`has_inner`) and this is
    // where the genuinely unmatched land.
    let unmatched: usize = reports
        .iter()
        .flat_map(|r| r.gaps.iter())
        .filter(|((_, cause), _)| *cause == Cause::Unmatched)
        .map(|(_, lines)| *lines)
        .sum();
    println!(
        "CONTROL UNMATCHED by the TERMINAL/PROPAGATED split = {unmatched} \
         (a measurement, not a zero: these are lines no arm claimed)"
    );
    println!(
        "CONTROL residual sum - (upstream - right) = {} (must be 0)",
        delta(residual_total, expected - matched)
    );

    println!("\nfamilies, over the {gap_total} gap lines:");
    let mut family_rows: Vec<_> = families.iter().map(|(k, v)| (*v, *k)).collect();
    family_rows.sort_unstable_by(|a, b| b.cmp(a));
    for (count, what) in family_rows {
        println!("  {count:>9}  {:>6.2}%  {what}", pct(count, gap_total));
    }

    // The repaired triage question — "is the prerequisite MET?" — asked of
    // every gap line at once instead of row by row and by judgement.
    println!("\nIS THE PREREQUISITE MET? — the repaired triage question, measured:");
    for kind in [Cause::Terminal, Cause::PropagatedSpan, Cause::PropagatedNamed, Cause::Unknown] {
        let n: usize = rows.iter().filter(|((_, c), _)| *c == kind).map(|(_, r)| r.lines).sum();
        println!("  {:>8}  {:>6.2}%  {}", n, pct(n, gap_total), kind.label());
    }

    let mut ranked: Vec<_> = rows.iter().collect();
    ranked.sort_unstable_by_key(|((_, _), row)| std::cmp::Reverse(row.lines));

    println!("\nGRADIENT RANKING A — TERMINAL gap rows, by assertion LINES (not nodes).");
    println!("  Nothing these lines depend on is known to have gapped, so the row's own");
    println!("  rule is the whole of the missing work. Kind 1: the size is the worth.");
    print_rows(
        ranked
            .iter()
            .filter(|((_, c), _)| *c == Cause::Terminal)
            .map(|((k, _), r)| (k.as_str(), *r)),
        30,
        gap_total,
    );

    println!("\nGRADIENT RANKING B — PROPAGATED rows. Kind 2: the size is an UPPER BOUND");
    println!("  of unknown depth. Do not quote a number; ask for a demonstration.");
    print_rows(
        ranked
            .iter()
            .filter(|((_, c), _)| matches!(c, Cause::PropagatedSpan | Cause::PropagatedNamed))
            .map(|((k, _), r)| (k.as_str(), *r)),
        20,
        gap_total,
    );

    println!("\nGRADIENT RANKING C — DEPENDENT-UNKNOWN rows: the reason names an");
    println!("  initialiser or an annotation beside the node, which the span test cannot");
    println!("  see. Neither ranked nor dismissed until something measures them.");
    print_rows(
        ranked
            .iter()
            .filter(|((_, c), _)| *c == Cause::Unknown)
            .map(|((k, _), r)| (k.as_str(), *r)),
        12,
        gap_total,
    );

    // The concentration check, made unskippable: the largest cases are printed
    // beside the row, so a row that is one file says so on its own line rather
    // than in a percentage the reader has to interpret.
    println!("\nWHERE THE TOP ROWS ACTUALLY LIVE (top three cases each):");
    for ((key, kind), row) in ranked.iter().take(18) {
        let mut cases: Vec<_> = row.by_case.iter().map(|(n, c)| (*c, n.clone())).collect();
        cases.sort_unstable_by(|a, b| b.cmp(a));
        let top: Vec<String> =
            cases.iter().take(3).map(|(count, name)| format!("{name} {count}")).collect();
        println!(
            "  {:<52} {:>7} {:<18} {}",
            truncate(key, 50),
            row.lines,
            kind.label(),
            top.join(" | ")
        );
    }

    println!("\nCASE RANKING — the same rows by the cases they FINISH, not the lines.");
    let mut by_finish: Vec<_> = rows
        .iter()
        .map(|(key, row)| (row.finishes().0, key, row))
        .filter(|(finished, _, _)| *finished > 0)
        .collect();
    by_finish.sort_unstable_by_key(|(finished, _, _)| std::cmp::Reverse(*finished));
    println!(
        "{:<52} {:>7} {:>8} {:>7} {:>7}  cause",
        "row", "lines", "finishes", "cases", "median"
    );
    for (finished, (key, kind), row) in by_finish.iter().take(30) {
        let (cases, _, _) = row.concentration();
        let (_, median) = row.finishes();
        println!(
            "{:<52} {:>7} {:>8} {:>7} {:>7}  {}",
            truncate(key, 50),
            row.lines,
            finished,
            cases,
            median,
            kind.label()
        );
    }

    // The residual the case gate sees is not the gap. A third source — a line
    // whose *text* we did not reproduce — fails its case exactly as a wrong
    // type does and no checker work touches it. It is reported here because it
    // is invisible in every histogram that only looks at aligned lines.
    let unaligned = expected - aligned;
    println!("\nWHAT THE CASE GATE'S RESIDUAL IS MADE OF ({} lines):", expected - matched);
    println!(
        "  {gap_total:>8}  {:>6.2}%  gap   — we answered `error`, work not ported",
        pct(gap_total, expected - matched)
    );
    println!(
        "  {wrong_total:>8}  {:>6.2}%  wrong — ported and defective",
        pct(wrong_total, expected - matched)
    );
    println!(
        "  {unaligned:>8}  {:>6.2}%  unaligned — the walker did not reproduce upstream's text",
        pct(unaligned, expected - matched)
    );
    println!(
        "  CONTROL gap+wrong+unaligned - residual = {} (must be 0)",
        delta(gap_total + wrong_total + unaligned, expected - matched)
    );
    let walker_only = reports
        .iter()
        .filter(|r| r.expected > r.matched && r.gaps.is_empty() && r.wrong.is_empty())
        .count();
    println!(
        "  cases whose whole residual is unaligned text: {walker_only} — no checker \
         change can flip these"
    );

    // Both roll-ups, because the sub-rows must not be added. A bucket's
    // `finishes` is *not* the sum of its sub-rows' — closing one substitution
    // leaves a case that needed two of them still failing. So the whole-kind
    // row is re-derived here rather than inferred from the split one below
    // (`docs/conventions.md`, "before summing, state which question the sum
    // answers").
    println!("\nWRONG lines by node KIND — the whole bucket, {wrong_total} lines:");
    let mut by_kind: HashMap<String, Row> = HashMap::new();
    for case in reports {
        let residual = case.expected - case.matched;
        let mut folded: HashMap<&str, usize> = HashMap::new();
        for (key, count) in &case.wrong {
            let kind = key.split_whitespace().next().unwrap_or(key);
            *folded.entry(kind).or_default() += count;
        }
        for (kind, count) in folded {
            let row = by_kind.entry(kind.to_string()).or_default();
            row.lines += count;
            *row.by_case.entry(case.name.clone()).or_default() += count;
            row.residual_after.push(residual - count);
        }
    }
    let mut kind_ranked: Vec<_> = by_kind.iter().collect();
    kind_ranked.sort_unstable_by_key(|(_, row)| std::cmp::Reverse(row.lines));
    print_rows(kind_ranked.iter().map(|(k, r)| (k.as_str(), *r)), 8, wrong_total);

    println!("\nWRONG lines by node kind AND shape substitution — the work items:");
    let mut wrong: HashMap<String, Row> = HashMap::new();
    for case in reports {
        let residual = case.expected - case.matched;
        for (kind, count) in &case.wrong {
            let row = wrong.entry(kind.clone()).or_default();
            row.lines += count;
            *row.by_case.entry(case.name.clone()).or_default() += count;
            row.residual_after.push(residual - count);
        }
    }
    let mut wrong_ranked: Vec<_> = wrong.iter().collect();
    wrong_ranked.sort_unstable_by_key(|(_, row)| std::cmp::Reverse(row.lines));
    print_rows(wrong_ranked.iter().map(|(k, r)| (k.as_str(), *r)), 20, wrong_total);

    println!("\ncommonest exact wrong answers, `upstream -> ours`:");
    let mut pairs: HashMap<(String, String), usize> = HashMap::new();
    for case in reports {
        for (key, count) in &case.pairs {
            *pairs.entry(key.clone()).or_default() += count;
        }
    }
    let mut pair_rows: Vec<_> = pairs.into_iter().map(|((w, g), c)| (c, w, g)).collect();
    pair_rows.sort_unstable_by(|a, b| b.cmp(a));
    for (count, want, got) in pair_rows.iter().take(25) {
        println!("  {:>34}  ->  {:<34} {count:>7}", truncate(want, 32), truncate(got, 32));
    }

    // A case with one line left is one edit from flipping; the shape of this
    // curve is what says whether the case gate is reachable at all.
    let any_credited: usize = reports.iter().map(|r| r.any_credited).sum();
    let any_wrong: usize = reports.iter().map(|r| r.any_wrong).sum();
    let error_leaked: usize = reports.iter().map(|r| r.error_leaked).sum();
    println!("\nTHE `any` AUDIT — is `errorType, never anyType` actually held?");
    println!(
        "  {any_credited:>8}  lines we answered `any` and upstream did too (banked; {:.2}% of the {matched} right)",
        pct(any_credited, matched)
    );
    println!(
        "  {any_wrong:>8}  lines we answered `any` and upstream did NOT — a confident wrong answer"
    );
    println!(
        "  {error_leaked:>8}  lines where `error` reached a printed type (`error[]`) instead of stopping the line"
    );
    let mut any_cases: HashMap<&str, usize> = HashMap::new();
    for case in reports {
        for (name, count) in &case.any_by_case {
            *any_cases.entry(name.as_str()).or_default() += count;
        }
    }
    let mut any_ranked: Vec<_> = any_cases.into_iter().map(|(k, v)| (v, k)).collect();
    any_ranked.sort_unstable_by(|a, b| b.cmp(a));
    let top10: usize = any_ranked.iter().take(10).map(|(c, _)| c).sum();
    println!(
        "  the banked `any` over {} cases, top-10 share {:.1}%:",
        any_ranked.len(),
        pct(top10, any_credited)
    );
    for (count, name) in any_ranked.iter().take(6) {
        println!("      {count:>6}  {name}");
    }

    println!("\nCASES BY WHAT IS LEFT IN THEM (the case gate's own distribution):");
    let mut residuals: Vec<usize> = reports.iter().map(|r| r.expected - r.matched).collect();
    residuals.sort_unstable();
    let buckets = [0usize, 1, 2, 3, 5, 10, 25, 50, 100, 250, 1000];
    for window in buckets.windows(2) {
        let (low, high) = (window[0], window[1]);
        let n = residuals.iter().filter(|&&r| r >= low && r < high).count();
        println!("  {low:>5}..{high:<6} {n:>6} cases  ({:.2}%)", pct(n, reports.len()));
    }
    let big = residuals.iter().filter(|&&r| r >= 1000).count();
    println!("  {:>5}+       {big:>6} cases  ({:.2}%)", 1000, pct(big, reports.len()));

    // The question the case gate actually poses. A cycle target is "get the
    // rate to X%"; this says how deep into the residual curve X% reaches, and
    // how many *lines* live down there — which is what says whether the case
    // goal and the gradient goal are the same work or different work.
    println!(
        "\nWHAT A CASE TARGET COSTS (passing today: {} cases with nothing left):",
        residuals.iter().filter(|&&r| r == 0).count()
    );
    println!(
        "{:>8} {:>9} {:>9} {:>12} {:>9}",
        "residual", "cases<=", "rate", "lines in them", "share"
    );
    let total_residual: usize = residuals.iter().sum();
    for limit in [0usize, 1, 2, 3, 5, 10, 15, 25, 50, 100, 250] {
        let n = residuals.iter().filter(|&&r| r <= limit).count();
        let lines: usize = residuals.iter().filter(|&&r| r <= limit).sum();
        println!(
            "{limit:>8} {n:>9} {:>8.2}% {lines:>12} {:>8.2}%",
            pct(n, reports.len()),
            pct(lines, total_residual)
        );
    }
    println!(
        "  total residual over all cases: {total_residual} lines \
         (= upstream {expected} - right {matched})"
    );

    // Near-misses are where the case gate is won or lost. How many distinct
    // work items does one of them need? If it is one or two, targeted rows
    // flip cases; if it is five, only a broad sweep does.
    println!("\nNEAR-MISS CASES (1..=10 lines left), by how many distinct rows they need:");
    let mut spread: HashMap<usize, usize> = HashMap::new();
    let mut near_rows: HashMap<String, usize> = HashMap::new();
    let mut near = 0usize;
    for case in reports {
        let residual = case.expected - case.matched;
        if !(1..=10).contains(&residual) {
            continue;
        }
        near += 1;
        let distinct = case.gaps.len() + case.wrong.len();
        *spread.entry(distinct).or_default() += 1;
        for (key, _) in case.gaps.keys() {
            *near_rows.entry(key.clone()).or_default() += 1;
        }
    }
    let mut spread_rows: Vec<_> = spread.into_iter().collect();
    spread_rows.sort_unstable();
    for (distinct, n) in spread_rows.iter().take(10) {
        println!("  {distinct:>2} row(s): {n:>5} cases ({:.2}% of the {near})", pct(*n, near));
    }
    println!("\nrows present in the most near-miss cases (the case-gate ranking's raw input):");
    let mut near_ranked: Vec<_> = near_rows.into_iter().map(|(k, v)| (v, k)).collect();
    near_ranked.sort_unstable_by(|a, b| b.cmp(a));
    for (n, key) in near_ranked.iter().take(20) {
        println!("  {n:>5}  {}", truncate(key, 70));
    }
}

fn truncate(text: &str, width: usize) -> String {
    if text.len() <= width { text.to_string() } else { format!("{}…", &text[..width - 1]) }
}

fn print_rows<'a>(rows: impl Iterator<Item = (&'a str, &'a Row)>, take: usize, gap_total: usize) {
    println!(
        "{:<58} {:>7} {:>6} {:>6} {:>7} {:>7} {:>8} {:>7}",
        "row", "lines", "share", "cases", "top-1", "top-10", "finishes", "median"
    );
    for (key, row) in rows.take(take) {
        let (cases, top1, top10) = row.concentration();
        let (finished, median) = row.finishes();
        println!(
            "{:<58} {:>7} {:>5.2}% {:>6} {:>6.1}% {:>6.1}% {:>8} {:>7}",
            truncate(key, 56),
            row.lines,
            pct(row.lines, gap_total),
            cases,
            top1,
            top10,
            finished,
            median
        );
    }
}
