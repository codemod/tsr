//! What this port reports that upstream does not — the `diagnostics` suite's
//! **extra** column, which no instrument had ever opened.
//!
//! ```text
//! cargo run --release -p tsr-conformance --example extragap
//! ```
//!
//! `diaggap.rs` ranks the *missing* column and prints one line per code for the
//! extras. That is enough to say the extras are large and not enough to act on:
//! a code appearing in both columns of one case is usually **one diagnostic at
//! the wrong position**, not one invented and one missed, and the two readings
//! call for completely different work.
//!
//! This splits them. For every judged case it classifies each extra as either
//!
//! - **displaced** — the same code is missing elsewhere in the same file, so
//!   the port found the defect and put it in the wrong place; or
//! - **invented** — the code appears nowhere in the baseline for that file.
//!
//! and reports, per code, how many cases would pass **if the extras alone were
//! removed**, which is the only forecastable number on this side.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{
    CaseEntry, Corpus,
    errors_baseline::{self, BaselineDiagnostic},
    repo_root,
};

#[derive(Default)]
struct Row {
    /// Extras whose code is also missing somewhere in the same file.
    displaced: usize,
    /// Extras whose code the baseline never mentions for that file.
    invented: usize,
    /// Cases this code is the *only* obstacle in, extras and missing together.
    sole_obstacle: usize,
}

fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let measured: Vec<BTreeMap<u32, Row>> = cases.par_iter().filter_map(measure).collect();

    let mut totals: BTreeMap<u32, Row> = BTreeMap::new();
    for case in measured {
        for (code, row) in case {
            let entry = totals.entry(code).or_default();
            entry.displaced += row.displaced;
            entry.invented += row.invented;
            entry.sole_obstacle += row.sole_obstacle;
        }
    }

    let mut ranked: Vec<(&u32, &Row)> = totals.iter().collect();
    ranked.sort_by_key(|(_, row)| std::cmp::Reverse(row.displaced + row.invented));
    println!("  code   displaced   invented   cases where it is the SOLE obstacle");
    for (code, row) in ranked.iter().take(40) {
        println!("TS{code:<6} {:>9} {:>10} {:>10}", row.displaced, row.invented, row.sole_obstacle);
    }
}

/// One case's extras, classified.
fn measure(case: &CaseEntry) -> Option<BTreeMap<u32, Row>> {
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
    let mut actual = tsr_conformance::diagnostics_suite::reported_for(&test);
    actual.sort_unstable();
    if actual == expected {
        return None;
    }

    let extra: Vec<&BaselineDiagnostic> = surplus(&actual, &expected);
    let missing: Vec<&BaselineDiagnostic> = surplus(&expected, &actual);
    let mut rows: BTreeMap<u32, Row> = BTreeMap::new();
    for diagnostic in &extra {
        let row = rows.entry(diagnostic.code).or_default();
        if missing.iter().any(|m| m.code == diagnostic.code && m.file == diagnostic.file) {
            row.displaced += 1;
        } else {
            row.invented += 1;
        }
    }
    // Which single code, if this port got it entirely right, would finish the
    // case? Counted over extras *and* missing together, so it prices a fix
    // rather than half of one.
    let codes: Vec<u32> = {
        let mut all: Vec<u32> =
            extra.iter().chain(missing.iter()).map(|diagnostic| diagnostic.code).collect();
        all.sort_unstable();
        all.dedup();
        all
    };
    if codes.len() == 1 {
        rows.entry(codes[0]).or_default().sole_obstacle += 1;
    }
    Some(rows)
}

/// The elements of `left` that `right` does not have as many of. Both sorted.
fn surplus<'a>(
    left: &'a [BaselineDiagnostic],
    right: &[BaselineDiagnostic],
) -> Vec<&'a BaselineDiagnostic> {
    let mut out = Vec::new();
    let mut index = 0usize;
    for item in left {
        while index < right.len() && right[index] < *item {
            index += 1;
        }
        if index < right.len() && right[index] == *item {
            index += 1;
        } else {
            out.push(item);
        }
    }
    out
}
