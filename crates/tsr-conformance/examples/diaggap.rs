//! What the `diagnostics` suite is actually blocked on, per code.
//!
//! ```text
//! cargo run --release -p tsr-conformance --example diaggap
//! ```
//!
//! # Why this exists
//!
//! `diagnostics` has read **80/5,488 (1.46%)** since it was built, and
//! [ADR-0040](../../../docs/adr/0040-diagnostics-come-from-a-check-traversal-and-assignability-gets-a-reporting-twin.md)
//! explains the flatness structurally: this port has no `checkSourceFile`
//! traversal, so every `TS2xxx` in a baseline is unreachable. That is a correct
//! diagnosis of *why the number is 1.46%* and it is **not** a sizing of what the
//! next build should be, because it does not say which codes the blocked cases
//! are blocked on.
//!
//! `STATUS.md`'s fourth rule — a population is a ceiling, not a conversion —
//! applies to the ADR's own `765 cases reachable with scanner + parser + binder
//! only` figure. That figure classifies **codes**; it does not check that this
//! port *emits* them, at the right position, and nothing else besides. This probe
//! measures the conversion instead of the ceiling.
//!
//! # What it reports
//!
//! For every judged case (the `diagnostics` suite's own skips, copied exactly so
//! the denominator is the suite's 5,488):
//!
//! - `MISSING(code)` / `EXTRA(code)` multisets, after the same sorted-multiset
//!   comparison the suite makes.
//! - **The blocking set**: the distinct codes a case is missing, plus a marker if
//!   it also reports something extra. A case converts only when its whole
//!   blocking set is emitted, so a code's *own* value is the number of cases
//!   whose blocking set is exactly `{that code}`.
//! - **The single-code table**, which is the number that can be built against:
//!   cases blocked on one code and nothing else, ranked.
//! - **The false-positive table**: codes this port emits where upstream does not.
//!   These cost cases *today* and are the cheapest possible conversions, because
//!   removing a wrong diagnostic needs no new machinery.
//!
//! # The trap this is written against
//!
//! A code that appears in 3,000 baselines is not worth 3,000 cases: almost all of
//! them also want a `TS2xxx` nobody can emit. Ranking codes by *appearance* would
//! reproduce exactly the "population identified by the shape of the answer"
//! failure `docs/conventions.md` records. Only the exactly-one-code column is a
//! forecast, and even it is a ceiling — the position must also be right.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{
    CaseEntry, Corpus,
    errors_baseline::{self, BaselineDiagnostic},
    repo_root,
};

/// One judged case's verdict, in the terms the ranking needs.
struct CaseVerdict {
    case: String,
    missing: Vec<BaselineDiagnostic>,
    extra: Vec<BaselineDiagnostic>,
}

fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");

    let verdicts: Vec<CaseVerdict> = cases.par_iter().filter_map(judge).collect();

    let judged = verdicts.len();
    let passed = verdicts.iter().filter(|v| v.missing.is_empty() && v.extra.is_empty()).count();
    println!("judged {judged}  passing {passed}\n");

    // How many cases are blocked purely by a false positive — no missing
    // diagnostic at all. These need no new machinery, only a wrong emission
    // removed, which makes them the cheapest column on the page.
    let extra_only =
        verdicts.iter().filter(|v| v.missing.is_empty() && !v.extra.is_empty()).count();
    println!("blocked by an EXTRA diagnostic alone: {extra_only}");

    let missing_only =
        verdicts.iter().filter(|v| !v.missing.is_empty() && v.extra.is_empty()).count();
    let both = verdicts.iter().filter(|v| !v.missing.is_empty() && !v.extra.is_empty()).count();
    println!("blocked by MISSING alone:             {missing_only}");
    println!("blocked by both:                      {both}\n");

    // The only forecastable column: the case wants one distinct code it does not
    // get, and reports nothing it should not.
    let mut single: BTreeMap<u32, Vec<&str>> = BTreeMap::new();
    for verdict in &verdicts {
        if !verdict.extra.is_empty() {
            continue;
        }
        let mut codes: Vec<u32> = verdict.missing.iter().map(|d| d.code).collect();
        codes.sort_unstable();
        codes.dedup();
        if let [code] = codes[..] {
            single.entry(code).or_default().push(&verdict.case);
        }
    }
    let mut ranked: Vec<(u32, usize, &Vec<&str>)> =
        single.iter().map(|(&code, cases)| (code, cases.len(), cases)).collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    println!("== cases blocked on EXACTLY ONE missing code, nothing extra ==");
    println!("{:>6}  {:>6}  example cases", "code", "cases");
    let mut cumulative = 0usize;
    for (code, count, examples) in &ranked {
        cumulative += count;
        let shown: Vec<&str> = examples.iter().take(3).copied().collect();
        println!("TS{code:<4}  {count:>6}  {}", shown.join(", "));
        if cumulative > 0 && *count < 3 {
            break;
        }
    }
    let single_total: usize = ranked.iter().map(|r| r.1).sum();
    println!("\nsingle-code total: {single_total} cases across {} codes", ranked.len());

    // Codes this port emits where upstream does not, ranked by how many cases
    // they appear in. A false positive is a defect we own today.
    let mut extras: BTreeMap<u32, usize> = BTreeMap::new();
    for verdict in &verdicts {
        let mut codes: Vec<u32> = verdict.extra.iter().map(|d| d.code).collect();
        codes.sort_unstable();
        codes.dedup();
        for code in codes {
            *extras.entry(code).or_default() += 1;
        }
    }
    let mut extras: Vec<(u32, usize)> = extras.into_iter().collect();
    extras.sort_by_key(|&(code, count)| (std::cmp::Reverse(count), code));
    println!("\n== FALSE POSITIVES: codes we emit that the baseline does not ==");
    for (code, count) in extras.iter().take(25) {
        println!("TS{code:<4}  {count:>6} cases");
    }

    // Every missing code by how many cases contain it at all — the ceiling
    // column, printed after the conversion column so it cannot be mistaken for
    // one.
    let mut appears: BTreeMap<u32, usize> = BTreeMap::new();
    for verdict in &verdicts {
        let mut codes: Vec<u32> = verdict.missing.iter().map(|d| d.code).collect();
        codes.sort_unstable();
        codes.dedup();
        for code in codes {
            *appears.entry(code).or_default() += 1;
        }
    }
    let mut appears: Vec<(u32, usize)> = appears.into_iter().collect();
    appears.sort_by_key(|&(code, count)| (std::cmp::Reverse(count), code));
    println!("\n== CEILING ONLY: cases missing a code at all (not a forecast) ==");
    for (code, count) in appears.iter().take(30) {
        let converts = single.get(code).map_or(0, Vec::len);
        println!("TS{code:<4}  {count:>6} cases contain it   {converts:>5} would convert alone");
    }
}

/// The `diagnostics` suite's own judgement, returning the two difference sets.
///
/// The skip conditions are copied from `diagnostics_suite.rs` rather than
/// re-derived, so this probe's denominator is the suite's.
fn judge(case: &CaseEntry) -> Option<CaseVerdict> {
    if case.has_varied_errors() || case.has_known_divergence() || !case.has_any_baseline() {
        return None;
    }
    let baseline = case.expected_errors().ok()?;
    let mut expected: Vec<BaselineDiagnostic> =
        baseline.as_deref().map(errors_baseline::parse).unwrap_or_default();
    if expected.is_empty() {
        return None;
    }
    expected.sort_unstable();

    let test = case.load().ok()?;
    // The suite's own set, called rather than re-derived: a probe that
    // re-implements the harness ranks a different compiler's failures.
    let mut actual = tsr_conformance::diagnostics_suite::reported_for(&test);
    actual.sort_unstable();

    Some(CaseVerdict {
        case: case.name.clone(),
        missing: surplus(&expected, &actual),
        extra: surplus(&actual, &expected),
    })
}

/// The elements of `left` that `right` does not have as many of.
fn surplus(left: &[BaselineDiagnostic], right: &[BaselineDiagnostic]) -> Vec<BaselineDiagnostic> {
    let mut out = Vec::new();
    let mut index = 0usize;
    for item in left {
        while index < right.len() && right[index] < *item {
            index += 1;
        }
        if index < right.len() && right[index] == *item {
            index += 1;
        } else {
            out.push(item.clone());
        }
    }
    out
}
