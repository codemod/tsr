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
//! `docs/architecture/checker-notes-diag2.md` §272, §273.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{
    CaseEntry, Corpus,
    errors_baseline::{self, BaselineDiagnostic},
    repo_root,
};

/// One blocked case: the code it is blocked on, and how many lines it wants.
type Row = (u32, usize);

fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let rows: Vec<Row> = cases.par_iter().filter_map(measure).collect();

    let mut by_code: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (code, lines) in rows {
        by_code.entry(code).or_default().push(lines);
    }

    let mut ranked: Vec<(u32, Vec<usize>)> = by_code.into_iter().collect();
    ranked.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));

    println!(
        "{:<8} {:>6} {:>7} {:>7} {:>7}   convertible by a partial rule?",
        "code", "cases", "1 line", "2-3", "4+"
    );
    for (code, counts) in &ranked {
        let one = counts.iter().filter(|&&n| n == 1).count();
        let few = counts.iter().filter(|&&n| (2..=3).contains(&n)).count();
        let many = counts.iter().filter(|&&n| n >= 4).count();
        // A partial rule converts a case only if it supplies EVERY line, so a
        // one-line case is the only kind a fragment reliably reaches.
        let verdict = if one * 2 >= counts.len() {
            "yes — mostly single-line"
        } else {
            "no — needs the whole rule"
        };
        println!("TS{code:<6} {:>6} {one:>7} {few:>7} {many:>7}   {verdict}", counts.len());
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
    let mut missing: Vec<u32> = Vec::new();
    for (key, n) in &want {
        let have = got.get(key).copied().unwrap_or(0);
        for _ in have..*n {
            missing.push(key.3);
        }
    }
    let extra: usize =
        got.iter().map(|(key, n)| n.saturating_sub(want.get(key).copied().unwrap_or(0))).sum();
    if extra != 0 || missing.is_empty() {
        return None;
    }
    // Blocked on exactly one code — `diaggap`'s population, re-derived so the
    // two instruments cannot drift.
    let code = missing[0];
    if missing.iter().any(|&it| it != code) {
        return None;
    }
    Some((code, missing.len()))
}
