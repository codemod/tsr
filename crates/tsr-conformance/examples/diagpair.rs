//! What has to be ported **alongside** a code for its cases to convert.
//!
//! ```text
//! cargo run --release -p tsr-conformance --example diagpair
//! ```
//!
//! # The question this answers, and why the others cannot
//!
//! `diaggap` prices a row by *cases blocked on this code alone*. `diagslice`
//! (§273) adds how many lines each of those cases wants. Neither says anything
//! about the much larger population of cases blocked on this code **and
//! something else** — and §299 is where that cost a bar:
//!
//! > §297's TS1108 converted **19** against a sole-obstacle count of 5, because
//! > it also finished cases blocked on two codes whose *other* code this port
//! > already emitted. §298's TS2481 converted **2** against a count of 2,
//! > because its partners are codes nothing here emits.
//!
//! Both were complete arms. Both read against the same instrument. The number
//! that separates them is the one below: of the cases missing this code, which
//! **other** codes are missing with it, and does this port already produce them?
//!
//! A code whose partners are all already emitted is worth more than its
//! sole-obstacle count. A code whose partners are themselves absent is worth
//! exactly its count, and its partners are the next build.
//!
//! `docs/architecture/checker-notes-diag2.md` §299, §300.

use std::collections::{BTreeMap, BTreeSet};

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{
    CaseEntry, Corpus,
    errors_baseline::{self, BaselineDiagnostic},
    repo_root,
};

/// One case's missing codes, as a set.
type Row = BTreeSet<u32>;

fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let rows: Vec<Row> = cases.par_iter().filter_map(measure).collect();

    // code -> (cases missing it, partner code -> how often)
    let mut totals: BTreeMap<u32, usize> = BTreeMap::new();
    let mut partners: BTreeMap<u32, BTreeMap<u32, usize>> = BTreeMap::new();
    let mut alone: BTreeMap<u32, usize> = BTreeMap::new();
    for row in &rows {
        for &code in row {
            *totals.entry(code).or_default() += 1;
            if row.len() == 1 {
                *alone.entry(code).or_default() += 1;
                continue;
            }
            for &other in row {
                if other != code {
                    *partners.entry(code).or_default().entry(other).or_default() += 1;
                }
            }
        }
    }

    let mut ranked: Vec<(u32, usize)> = totals.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    println!("{:<8} {:>6} {:>7}   top partners (cases blocked on both)", "code", "cases", "alone");
    for (code, cases) in ranked.iter().take(40) {
        let solo = alone.get(code).copied().unwrap_or(0);
        let mut top: Vec<(u32, usize)> = partners
            .get(code)
            .map(|m| m.iter().map(|(&k, &v)| (k, v)).collect())
            .unwrap_or_default();
        top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let shown: Vec<String> = top.iter().take(4).map(|(c, n)| format!("TS{c}×{n}")).collect();
        println!("TS{code:<6} {cases:>6} {solo:>7}   {}", shown.join("  "));
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

    let mut want: BTreeMap<(String, u32, u32, u32), usize> = BTreeMap::new();
    for d in &expected {
        *want.entry((d.file.clone(), d.line, d.column, d.code)).or_default() += 1;
    }
    let mut got: BTreeMap<(String, u32, u32, u32), usize> = BTreeMap::new();
    for d in &actual {
        *got.entry((d.file.clone(), d.line, d.column, d.code)).or_default() += 1;
    }
    let mut missing: BTreeSet<u32> = BTreeSet::new();
    for (key, n) in &want {
        if got.get(key).copied().unwrap_or(0) < *n {
            missing.insert(key.3);
        }
    }
    // Only cases blocked on missing diagnostics — an extra line is a different
    // problem and `extraonly` owns it.
    let extra: usize =
        got.iter().map(|(key, n)| n.saturating_sub(want.get(key).copied().unwrap_or(0))).sum();
    if extra != 0 || missing.is_empty() {
        return None;
    }
    Some(missing)
}
