//! The counterfactual for TS2307: what the check traversal would do to the
//! `diagnostics` suite, measured before the suite is wired to it.
//!
//! ```text
//! cargo run --release -p tsr-conformance --example diag2307
//! ```
//!
//! # Probe and build are the same function
//!
//! This does **not** re-implement the rule. It calls
//! [`tsr_conformance::diagnostics_suite::reported_for`] — the suite's own set,
//! which runs the shipped `Checker::check_source_file` — and reconstructs the
//! *before* side by removing the two codes the rule emits. `STATUS.md` §7
//! records why the shape is worth the trouble: the composite-print twin landed
//! on a 1,500-line forecast to the line because `sigprint::compose` and
//! `signature_to_string_at` were one function.
//!
//! **It was written before the wiring commit and its numbers are that commit's
//! forecast** — 50 CONVERTS, 0 LOST, 101 RIGHT, 21 WRONG, registered in
//! `docs/architecture/checker-notes-diag2.md` §4. After the wiring it re-derives
//! the same split from the shipped suite, which is what makes it re-runnable
//! rather than a one-shot.
//!
//! # The four columns
//!
//! | column | meaning |
//! |---|---|
//! | `CONVERTS` | a case that fails today and would pass — the forecast |
//! | `LOST` | a case that **passes** today and would fail. Must be 0: the rule can only add diagnostics, so a loss means it added one to a case that was already exact |
//! | `RIGHT` / `WRONG` | per *diagnostic*: an emitted TS2307 the baseline also records at that exact `(file, line, column, code)`, or not |
//! | `STILL SHORT` | a case that gains a correct TS2307 and still fails for something else — the cascade this rule cannot reach alone |
//!
//! `WRONG` is the number that decides whether the bound in
//! `Checker::check_module_specifier` is drawn in the right place. A wrong
//! diagnostic is strictly worse than a missing one: it fails its own case *and*
//! can fail a case that passes.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{
    CaseEntry, Corpus,
    errors_baseline::{self, BaselineDiagnostic},
    repo_root,
};

/// One case's before/after, in the terms the bar is written in.
#[derive(Default)]
struct Row {
    converts: bool,
    lost: bool,
    still_short: bool,
    right: usize,
    wrong: Vec<String>,
}

fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let rows: Vec<(String, Row)> = cases.par_iter().filter_map(measure).collect();

    let converts: Vec<&str> =
        rows.iter().filter(|(_, r)| r.converts).map(|(name, _)| name.as_str()).collect();
    let lost: Vec<&str> =
        rows.iter().filter(|(_, r)| r.lost).map(|(name, _)| name.as_str()).collect();
    let still_short = rows.iter().filter(|(_, r)| r.still_short).count();
    let right: usize = rows.iter().map(|(_, r)| r.right).sum();
    let wrong: Vec<&String> = rows.iter().flat_map(|(_, r)| r.wrong.iter()).collect();

    println!("judged cases          {}", rows.len());
    println!("CONVERTS              {}", converts.len());
    println!("LOST                  {}", lost.len());
    println!("STILL SHORT           {still_short}");
    println!("diagnostics RIGHT     {right}");
    println!("diagnostics WRONG     {}", wrong.len());

    if !lost.is_empty() {
        println!("\n-- LOST (a passing case broken) --");
        for name in lost.iter().take(40) {
            println!("  {name}");
        }
    }

    if !wrong.is_empty() {
        println!("\n-- WRONG, by specifier --");
        let mut by_text: BTreeMap<&str, usize> = BTreeMap::new();
        for entry in &wrong {
            *by_text.entry(entry.as_str()).or_default() += 1;
        }
        let mut ranked: Vec<(&&str, &usize)> = by_text.iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(a.1));
        for (text, count) in ranked.iter().take(400) {
            println!("  {count:>4}  {text}");
        }
    }

    println!("\n-- CONVERTS, first 30 --");
    for name in converts.iter().take(30) {
        println!("  {name}");
    }
}

/// The suite's judgement with the rule's diagnostics and without them.
fn measure(case: &CaseEntry) -> Option<(String, Row)> {
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
    let mut after = tsr_conformance::diagnostics_suite::reported_for(&test);
    after.sort_unstable();

    let mut before: Vec<BaselineDiagnostic> =
        after.iter().filter(|d| !RULE_CODES.contains(&d.code)).cloned().collect();
    before.sort_unstable();

    let mut wrong = Vec::new();
    let mut right = 0usize;
    for diagnostic in after.iter().filter(|d| RULE_CODES.contains(&d.code)) {
        if expected.binary_search(diagnostic).is_ok() {
            right += 1;
        } else {
            wrong.push(format!(
                "{}  {}({},{}) TS{}",
                case.name, diagnostic.file, diagnostic.line, diagnostic.column, diagnostic.code
            ));
        }
    }

    let passed_before = before == expected;
    let passed_after = after == expected;
    Some((
        case.name.clone(),
        Row {
            converts: !passed_before && passed_after,
            lost: passed_before && !passed_after,
            still_short: !passed_after && right > 0,
            right,
            wrong,
        },
    ))
}

/// The codes `Checker::check_module_specifier` can emit.
///
/// Removing them from the suite's set is what reconstructs the *before* side.
/// It is exact rather than approximate because no other producer in this port
/// emits either code — the parser and binder have no notion of module
/// resolution at all.
const RULE_CODES: &[u32] = &[
    2307, 2882, 2564, 2304, 2454, 2369, 2695, 1104, 1105, 1107, 1115, 1116, 1036, 1183, 2384, 2389,
    2390, 2391, 2392, 2393, 6133, 6138, 6192, 6196, 6198, 6199, 6205, 2322, 7006, 7019, 2314, 2707,
    2554, 2555, 2339, 2741, 2353, 2345, 2411, 2415, 2416, 2420, 2430,
];
