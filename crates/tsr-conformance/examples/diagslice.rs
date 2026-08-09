//! For a code, **how many lines does each blocked case need?**
//!
//! ```text
//! cargo run --release -p tsr-conformance --example diagslice
//! ```
//!
//! # The question this answers, and why `diaggap` cannot
//!
//! `diaggap` reports *cases blocked on exactly one missing code*. That prices
//! the **row** — "seven cases need only TS7008" — and it is accurate. It says
//! nothing about whether a partial rule will convert any of them, because a
//! case passes only when it has **every** line of that code.
//!
//! §272 paid for the distinction: TS7008's slice produced **17 right lines and
//! zero wrong**, and converted **one** case. Nine cases went from *blocked on
//! one code* to `STILL SHORT`, which is exactly as far from passing as before.
//! §258 (`+2 of 32`) and §267 (`+6 of 7`) are the same measurement from the two
//! ends: §267's fragment cleared its bar because that code's blocked cases
//! wanted **one line each**, and §258's did not because they wanted several.
//!
//! > A sole-obstacle count prices the row, not the slice.
//!
//! So this reports, per code, the distribution of **lines-per-blocked-case**.
//! A code whose blocked cases mostly want a single line is convertible by a
//! partial rule; one whose cases want four is not, and no amount of
//! *correctness* in a fragment changes that.
//!
//! # The third column, and what §292 paid to learn
//!
//! §292 built seven of `checkGrammarIndexSignature`'s guards and measured
//! **`+0`**: every tree-decidable check in it is *already covered by a different
//! code this port emits*, so porting the missing codes moved nothing. §287's
//! parameter-list family looked identical from the gap and was `+9`.
//!
//! > A cluster of no-producer codes in a syntactic function is not evidence
//! > that the cases are reachable — only that this port does not emit those
//! > particular codes.
//!
//! `occupied` is the pre-check that separates them: of a code's missing lines,
//! how many sit at a `(file, line, column)` where this port **already emits
//! something else**? A high count means the position is taken and porting the
//! code will add a second diagnostic rather than convert a case. A low one
//! means the position is empty and the rule is genuinely absent.
//!
//! `docs/architecture/checker-notes-diag2.md` §272, §273, §293.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{
    CaseEntry, Corpus,
    errors_baseline::{self, BaselineDiagnostic},
    repo_root,
};

/// One blocked case: the code, how many lines it wants, and how many of those
/// sit at a position this port already fills with a different code.
type Row = (u32, usize, usize);

fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let rows: Vec<Row> = cases.par_iter().filter_map(measure).collect();

    let mut by_code: BTreeMap<u32, Vec<(usize, usize)>> = BTreeMap::new();
    for (code, lines, occupied) in rows {
        by_code.entry(code).or_default().push((lines, occupied));
    }

    let mut ranked: Vec<(u32, Vec<(usize, usize)>)> = by_code.into_iter().collect();
    ranked.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));

    println!(
        "{:<8} {:>6} {:>7} {:>7} {:>7} {:>9}   convertible by a partial rule?",
        "code", "cases", "1 line", "2-3", "4+", "occupied"
    );
    for (code, counts) in &ranked {
        let one = counts.iter().filter(|&&(n, _)| n == 1).count();
        let few = counts.iter().filter(|&&(n, _)| (2..=3).contains(&n)).count();
        let many = counts.iter().filter(|&&(n, _)| n >= 4).count();
        let lines: usize = counts.iter().map(|&(n, _)| n).sum();
        let occupied: usize = counts.iter().map(|&(_, o)| o).sum();
        // A partial rule converts a case only if it supplies EVERY line, so a
        // one-line case is the only kind a fragment reliably reaches — and only
        // if the position is not already filled by a different code (§293).
        let verdict = if occupied * 2 >= lines {
            "NO — positions already taken"
        } else if one * 2 >= counts.len() {
            "yes — mostly single-line"
        } else {
            "no — needs the whole rule"
        };
        let share = format!("{occupied}/{lines}");
        println!(
            "TS{code:<6} {:>6} {one:>7} {few:>7} {many:>7} {share:>9}   {verdict}",
            counts.len()
        );
    }
}

fn measure(case: &CaseEntry) -> Option<Row> {
    if case.has_varied_errors() || case.has_known_divergence() || !case.has_any_baseline() {
        return None;
    }
    let baseline = case.expected_errors().ok()?;
    let expected: Vec<BaselineDiagnostic> =
        baseline.as_deref().map(errors_baseline::parse).unwrap_or_default();
    if expected.is_empty() {
        return None;
    }
    let test = case.load().ok()?;
    let actual = tsr_conformance::diagnostics_suite::reported_for(&test);

    // The multiset difference, per §152.
    let mut want: BTreeMap<(String, u32, u32, u32), usize> = BTreeMap::new();
    for d in &expected {
        *want.entry((d.file.clone(), d.line, d.column, d.code)).or_default() += 1;
    }
    let mut got: BTreeMap<(String, u32, u32, u32), usize> = BTreeMap::new();
    for d in &actual {
        *got.entry((d.file.clone(), d.line, d.column, d.code)).or_default() += 1;
    }
    let mut missing: Vec<(String, u32, u32, u32)> = Vec::new();
    for (key, n) in &want {
        let have = got.get(key).copied().unwrap_or(0);
        for _ in have..*n {
            missing.push(key.clone());
        }
    }
    let extra: usize =
        got.iter().map(|(key, n)| n.saturating_sub(want.get(key).copied().unwrap_or(0))).sum();
    if extra != 0 || missing.is_empty() {
        return None;
    }
    // Blocked on exactly one code — `diaggap`'s population, re-derived so the
    // two instruments cannot drift.
    let code = missing[0].3;
    if missing.iter().any(|it| it.3 != code) {
        return None;
    }
    // §293 — how many of those lines sit where this port already emits a
    // *different* code. A taken position means porting the code adds a second
    // diagnostic rather than converting the case.
    let occupied = missing
        .iter()
        .filter(|key| {
            got.keys().any(|other| {
                (&other.0, other.1, other.2) == (&key.0, key.1, key.2) && other.3 != key.3
            })
        })
        .count();
    Some((code, missing.len(), occupied))
}
