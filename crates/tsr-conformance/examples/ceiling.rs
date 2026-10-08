//! How much of `checker_types` is unreachable by construction, and where it sits.
//!
//! # The question
//!
//! [ADR-0038](../../../docs/adr/0038-errortype-prints-error-and-the-corpus-has-a-ceiling.md)
//! records that `checker_types` cannot reach 100%: upstream renders `errorType`
//! as **`any`**, this port renders it as **`error`** so that a gap and a wrong
//! answer stay separable, and every line where upstream's answer *is* an
//! `errorType` is therefore unmatchable however good the checker gets. It sizes
//! that at *"roughly 6,000 lines"* and names the one way to do better:
//!
//! > The one avenue worth trying is a `.errors.txt` cross-reference to
//! > distinguish upstream's two kinds of `any`; nothing in a `.types` file can.
//!
//! This is that cross-reference. It matters now because the target is **80%**,
//! and the distance to it has to be measured against the reachable population
//! rather than against 100% — quoting a target against the wrong denominator is
//! the failure this repo has catalogued more than any other.
//!
//! # Why a `.types` file cannot answer it alone
//!
//! Upstream prints `any` for three different things, and they are the same six
//! characters:
//!
//! 1. a **genuine** `any` — `declare let x: any`, an untyped parameter under
//!    `noImplicitAny: false`, a widened evolving array;
//! 2. an **`errorType`** — `let x: NoSuchType`, a name that does not resolve;
//! 3. an **implicit** `any` from a failed inference upstream reports separately.
//!
//! Only (2) is unreachable here. (1) is ordinary work this port can and does do,
//! and counting it as unreachable would manufacture a ceiling that does not
//! exist — the mirror of ADR-0039's rejected shortcut, which would have *falsely
//! credited* ~6,100 lines by rendering `any` for comparison.
//!
//! # The instrument
//!
//! For every line where **upstream says exactly `any` and this port says
//! `error`**, ask whether the case's `.errors.txt` reports a *name-resolution*
//! failure — `TS2304` (Cannot find name), `TS2552` (Did you mean), `TS2503`
//! (Cannot find namespace), `TS2307` (Cannot find module) — **on the same source
//! line**. Same line, not same case: a case-level test would sweep in every
//! genuine `any` in a file that happens to contain one unresolved name, and
//! `parserRealSource7` alone carries 568 `TS2304`s beside 2,320 `any` lines.
//!
//! That is a **lower bound** on the unreachable set and is reported as one. It
//! misses an `errorType` that flows *downstream* of the reported line (`let a =
//! NoSuchType; let b = a;` reports on the first only) and it misses the
//! non-name-resolution error codes. It cannot over-count in the ways that
//! matter, which is the direction to be wrong in.
//!
//! # Controls
//!
//! - **C1, pinned by construction: a line we answer `error` on can never be
//!   counted as matched.** The two sets are disjoint by the definition of the
//!   bucket, so `matched ∩ unreachable = 0` whatever the corpus looks like.
//! - **C2, pinned by construction: a case with no `.errors.txt` contributes 0
//!   attributed lines**, because there is no diagnostic to match against. The
//!   mirror — how many `any`-vs-`error` lines those cases hold — is printed
//!   beside it, and it is the size of what this method cannot see.
//! - **C3, arithmetic:** the buckets sum to the `any`-vs-`error` population.
//!
//! Reproduce:
//!
//! ```text
//! cargo run --release -p tsr-conformance --example ceiling
//! ```

use std::collections::{BTreeMap, BTreeSet};

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::types_baseline::{self, FileTypes};
use tsr_conformance::{Corpus, repo_root, types_producer, types_suite};

/// Error codes that mean *a name did not resolve*, which is what makes
/// upstream's answer an `errorType` rather than a genuine `any`.
const NAME_RESOLUTION_CODES: [&str; 4] = ["TS2304", "TS2552", "TS2503", "TS2307"];

#[derive(Default)]
struct Tally {
    /// Judged cases.
    cases: usize,
    /// The gradient, so this probe's population is checkable against the suite.
    matched: usize,
    total: usize,
    /// Upstream says exactly `any`, we say exactly `error`.
    any_vs_error: usize,
    /// Of those, on a source line the case's `.errors.txt` reports a
    /// name-resolution failure on. **Unreachable by ADR-0038.**
    attributed: usize,
    /// Of those, in a case that has an `.errors.txt` but no such error on that
    /// line. Either a genuine `any`, or an `errorType` flowing downstream.
    unattributed_with_errors: usize,
    /// C2's mirror: in a case with no `.errors.txt` at all. The blind spot.
    no_errors_file: usize,
    /// C1: lines counted both matched and unreachable. Must be 0.
    c1_overlap: usize,
    /// Cases with an `.errors.txt`.
    cases_with_errors: usize,
    /// ADR-0048: matched lines whose top-level type is the port's GAP — the
    /// writer printed it `any` and upstream printed `any`. Each is credited
    /// although this port computed nothing: the falsely-credited population
    /// ADR-0038 feared, now counted by identity rather than estimated.
    credited_gap: usize,
    /// Of those, with a name-resolution error on the same source line.
    credited_gap_attributed: usize,
    /// ADR-0048: lines whose top-level type is upstream's `errorType`
    /// (`native_error`), by verdict, and of the matched ones how many carry a
    /// name-resolution error on their source line.
    native_total: usize,
    native_matched: usize,
    native_matched_attributed: usize,
    /// Of the matched ones in a case with an `.errors.txt`.
    native_matched_with_errors: usize,
    /// ADR-0048 (d): what narrowing the writer's gap->`any` rewrites to
    /// upstream's `errorType` would do. Every line a rewrite printed `any` for
    /// the port's gap would print `error` instead: a matched one goes
    /// RIGHT->GAP, an unmatched one WRONG->GAP.
    narrow_right_to_gap: usize,
    narrow_wrong_to_gap: usize,
    /// Cases with at least one RIGHT->GAP line.
    narrow_cases: usize,
    /// RIGHT->GAP lines by the rewrite that printed them.
    narrow_by_rewrite: BTreeMap<String, usize>,
    /// RIGHT->GAP lines by producer (`gap_reason`, its leading clause).
    narrow_by_producer: BTreeMap<String, usize>,
}

impl Tally {
    fn merge(&mut self, o: &Self) {
        self.cases += o.cases;
        self.matched += o.matched;
        self.total += o.total;
        self.any_vs_error += o.any_vs_error;
        self.attributed += o.attributed;
        self.unattributed_with_errors += o.unattributed_with_errors;
        self.no_errors_file += o.no_errors_file;
        self.c1_overlap += o.c1_overlap;
        self.cases_with_errors += o.cases_with_errors;
        self.credited_gap += o.credited_gap;
        self.credited_gap_attributed += o.credited_gap_attributed;
        self.native_total += o.native_total;
        self.native_matched += o.native_matched;
        self.native_matched_attributed += o.native_matched_attributed;
        self.native_matched_with_errors += o.native_matched_with_errors;
        self.narrow_right_to_gap += o.narrow_right_to_gap;
        self.narrow_wrong_to_gap += o.narrow_wrong_to_gap;
        self.narrow_cases += o.narrow_cases;
        for (key, n) in &o.narrow_by_rewrite {
            *self.narrow_by_rewrite.entry(key.clone()).or_default() += n;
        }
        for (key, n) in &o.narrow_by_producer {
            *self.narrow_by_producer.entry(key.clone()).or_default() += n;
        }
    }
}

/// The 1-based source lines carrying a name-resolution diagnostic.
///
/// `.errors.txt` reports as `file.ts(LINE,COL): error TSxxxx: ...`, and also
/// inlines the source with `!!!` markers underneath — only the header form is
/// parsed, because the inline form repeats the message without a line number.
fn name_resolution_lines(text: &str) -> BTreeSet<u32> {
    let mut lines = BTreeSet::new();
    for line in text.lines() {
        if !NAME_RESOLUTION_CODES.iter().any(|code| line.contains(&format!("error {code}:"))) {
            continue;
        }
        // `path(LINE,COL): error TS...`
        let Some(open) = line.find('(') else { continue };
        let Some(comma) = line[open..].find(',') else { continue };
        let Ok(number) = line[open + 1..open + comma].parse::<u32>() else { continue };
        lines.insert(number);
    }
    lines
}

/// The source line each baseline assertion sits under.
///
/// A `.types` baseline interleaves source lines with `>` assertion lines, so the
/// n-th assertion of a file belongs to the most recent non-`>` line. This walks
/// the raw section text rather than the parsed assertions, because the parse
/// deliberately drops the source lines.
fn assertion_source_lines(text: &str, file: &str) -> Vec<u32> {
    let mut result = Vec::new();
    let mut in_section = false;
    let mut source_line = 0u32;
    for line in text.lines() {
        if let Some(name) = line.strip_prefix("=== ").and_then(|r| r.strip_suffix(" ===")) {
            in_section = name == file;
            source_line = 0;
            continue;
        }
        if !in_section {
            continue;
        }
        if line.starts_with('>') {
            result.push(source_line);
        } else {
            source_line += 1;
        }
    }
    result
}

fn measure(case: &tsr_conformance::CaseEntry) -> Option<Tally> {
    if case.has_varied_types() || case.has_known_divergence() {
        return None;
    }
    let text = case.expected_types()?;
    let expected = types_baseline::parse(&text);
    let total = types_baseline::assertion_count(&expected);
    if total == 0 {
        return None;
    }
    let mut tally = Tally { cases: 1, total, ..Tally::default() };

    let Ok(parsed) = case.load() else {
        return Some(tally);
    };

    let errors_text = std::fs::read_to_string(case.baseline_path("errors.txt")).ok();
    let error_lines = errors_text.as_deref().map(name_resolution_lines);
    if error_lines.is_some() {
        tally.cases_with_errors = 1;
    }

    let arena = tsr_core::Arena::new();
    let (rendered, kinds) =
        types_producer::assertions_for_case_with_error_kinds(&arena, &parsed, &expected);
    let ours: Vec<FileTypes> = rendered
        .iter()
        .zip(&expected)
        .map(|(r, e)| types_producer::to_file_types(&e.file, r))
        .collect();
    tally.matched = types_suite::compare(&expected, &ours).lines.matched;

    for (index, expected_file) in expected.iter().enumerate() {
        let Some(our_file) = rendered.get(index) else { continue };
        let source_lines = assertion_source_lines(&text, &expected_file.file);
        let attributed_at = |position: usize| {
            error_lines.as_ref().is_some_and(|lines| {
                lines.contains(&source_lines.get(position).copied().unwrap_or(0))
            })
        };

        for (position, assertion) in our_file.iter().enumerate() {
            let identity = kinds.get(index).and_then(|k| k.get(position));
            let kind = identity.map_or(types_producer::TopError::Other, |line| line.top);
            let matched = expected_file
                .assertions
                .get(position)
                .is_some_and(|baseline| baseline.text == assertion.line());
            if let Some(line) = identity
                && let Some(rewrite) = line.rewrite
            {
                if matched {
                    tally.narrow_right_to_gap += 1;
                    *tally.narrow_by_rewrite.entry(format!("{rewrite:?}")).or_default() += 1;
                    *tally
                        .narrow_by_producer
                        .entry(producer_bucket(line.producer.as_deref()))
                        .or_default() += 1;
                } else {
                    tally.narrow_wrong_to_gap += 1;
                }
            }
            match kind {
                types_producer::TopError::Gap if matched => {
                    tally.credited_gap += 1;
                    tally.credited_gap_attributed += usize::from(attributed_at(position));
                }
                types_producer::TopError::Native => {
                    tally.native_total += 1;
                    if matched {
                        tally.native_matched += 1;
                        tally.native_matched_attributed += usize::from(attributed_at(position));
                        tally.native_matched_with_errors += usize::from(error_lines.is_some());
                    }
                }
                _ => {}
            }
            if std::env::var_os("TSR_CEILING_DUMP").is_some()
                && kind != types_producer::TopError::Other
            {
                println!(
                    "LINE\t{}:{index}:{position}\t{kind:?}\t{}\t{}\t{}",
                    case.name,
                    if matched { "RIGHT" } else { "OTHER" },
                    if attributed_at(position) { "attributed" } else { "unattributed" },
                    assertion.line()
                );
            }
            if assertion.type_string != "error" {
                continue;
            }
            let Some(baseline) = expected_file.assertions.get(position) else { continue };
            // Upstream's answer is exactly `any` — not `any[]`, not a union
            // containing it. The narrow test on purpose: a composite answer is
            // reachable work, and only the bare form is the `errorType` render.
            if !baseline.text.ends_with(" : any") {
                continue;
            }
            tally.any_vs_error += 1;

            // C1 is pinned by construction: this line answered `error`, so it
            // cannot have matched a baseline reading `any`.
            if baseline.text == assertion.line() {
                tally.c1_overlap += 1;
            }

            match &error_lines {
                None => tally.no_errors_file += 1,
                Some(lines) => {
                    let source = source_lines.get(position).copied().unwrap_or(0);
                    if lines.contains(&source) {
                        tally.attributed += 1;
                    } else {
                        tally.unattributed_with_errors += 1;
                    }
                }
            }
        }
    }

    tally.narrow_cases = usize::from(tally.narrow_right_to_gap > 0);
    Some(tally)
}

/// A `gap_reason` string's leading clause, so the producer table groups
/// rather than listing every symbol: `symbol has no type: VARIABLE / …`
/// keeps its flags and declaration kind and drops the rest.
fn producer_bucket(reason: Option<&str>) -> String {
    let Some(reason) = reason else { return "(none)".to_string() };
    let head: Vec<&str> = reason.split(" / ").take(2).collect();
    head.join(" / ")
}

#[allow(clippy::cast_precision_loss)]
fn main() {
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    assert!(corpus.is_available(), "corpus missing; run git submodule update --init --recursive");
    let cases = corpus.discover().expect("discovering cases");
    let tallies: Vec<Tally> = cases.par_iter().filter_map(measure).collect();
    let mut t = Tally::default();
    for tally in &tallies {
        t.merge(tally);
    }

    let pct = |n: usize| n as f64 / t.total.max(1) as f64 * 100.0;
    println!("cases judged {} ({} with an .errors.txt)\n", t.cases, t.cases_with_errors);
    println!("gradient          {} / {}  ({:.2}%)", t.matched, t.total, pct(t.matched));
    println!(
        "upstream `any`, we `error`   {:>7}  ({:.2}% of the denominator)\n",
        t.any_vs_error,
        pct(t.any_vs_error)
    );

    println!("  UNREACHABLE (ADR-0038): a name-resolution error on the same source line");
    println!(
        "    attributed                 {:>7}  ({:.2} points)",
        t.attributed,
        pct(t.attributed)
    );
    println!("  not attributed:");
    println!(
        "    case has errors, line does not  {:>7}  ({:.2} points)",
        t.unattributed_with_errors,
        pct(t.unattributed_with_errors)
    );
    println!(
        "    case has no .errors.txt         {:>7}  ({:.2} points)   <- C2 mirror, the blind spot\n",
        t.no_errors_file,
        pct(t.no_errors_file)
    );

    let reachable = t.total - t.attributed;
    println!("REACHABLE CEILING (lower bound on unreachable => upper bound on ceiling)");
    println!("  reachable denominator      {reachable} of {}", t.total);
    println!(
        "  today                      {:.2}% of reachable",
        t.matched as f64 / reachable as f64 * 100.0
    );
    println!(
        "  80% of the FULL denominator is {:.2}% of the reachable one",
        0.80 * t.total as f64 / reachable as f64 * 100.0
    );

    println!("\nADR-0048: THE SPLIT, BY IDENTITY");
    println!(
        "  matched lines whose type is the port's GAP     {:>7}  ({} attributed)   <- falsely credited",
        t.credited_gap, t.credited_gap_attributed
    );
    println!(
        "  lines whose type is upstream's errorType        {:>7}  ({} matched; {} of those in a case with errors, {} attributed)",
        t.native_total, t.native_matched, t.native_matched_with_errors, t.native_matched_attributed
    );

    println!("\nADR-0048 (d): NARROWING THE GAP->`any` REWRITES TO `native_error`");
    println!(
        "  RIGHT->GAP {:>7} lines in {} cases; WRONG->GAP {:>7}",
        t.narrow_right_to_gap, t.narrow_cases, t.narrow_wrong_to_gap
    );
    println!("  by rewrite:");
    let mut rows: Vec<_> = t.narrow_by_rewrite.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1));
    for (key, n) in rows {
        println!("    {n:>7}  {key}");
    }
    println!("  by producer (top 25):");
    let mut rows: Vec<_> = t.narrow_by_producer.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1));
    for (key, n) in rows.into_iter().take(25) {
        println!("    {n:>7}  {key}");
    }

    println!("\nCONTROLS");
    println!(
        "  C1 matched and unreachable at once   {:>7}   (0, by construction: the line answered `error`)",
        t.c1_overlap
    );
    println!(
        "  C3 buckets - population              {:>7}   (0, arithmetic)",
        i128::try_from(t.attributed + t.unattributed_with_errors + t.no_errors_file)
            .unwrap_or_default()
            - i128::try_from(t.any_vs_error).unwrap_or_default()
    );
}
