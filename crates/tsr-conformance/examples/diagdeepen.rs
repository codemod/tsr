//! Which codes does this port **already emit** and still block cases with?
//!
//! ```text
//! cargo run --release -p tsr-conformance --example diagdeepen
//! ```
//!
//! # The seam this ranks
//!
//! §321 opened the *deepening* seam by hand: a code whose rule exists but fires
//! in too few positions. Seven builds followed — §321 `+12`, §323 `−50`
//! (reverted), §325 `+4`, §327 `+4`, §330 `+2`, §333 `+1`, §335 `+7`. **Five of
//! the seven turned on *where* a rule is asked rather than on what it decides**,
//! and each row was found by reading a fixture, not by a ranking.
//!
//! The distinction that matters is cheap to compute and was never computed:
//!
//! ```text
//! emits = 0, blocked > 0   the rule is ABSENT   — port it, or refuse with an owner
//! emits > 0, blocked > 0   the rule UNDER-FIRES — widen where it is asked
//! ```
//!
//! Those two need completely different work, and every instrument here has
//! reported them in one column. `diaggap` says "TS2304 blocks 30 cases" whether
//! this port has never heard of TS2304 or emits it 248 times and misses a node
//! kind — and §335 was the latter, worth `+7` for a predicate widening.
//!
//! # Reading the output
//!
//! `emits` counts **right** lines only, corpus-wide: positions where this port
//! and upstream agree on `(file, line, column, code)`. A code with a high
//! `emits` and a high `blocked` is a working rule with a hole in it, which is
//! the cheapest kind of row on the board — the machinery, the message, the
//! error span and the anchor all already exist.
//!
//! `docs/architecture/checker-notes-diag2.md` §321, §335, §339.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{
    CaseEntry, Corpus,
    errors_baseline::{self, BaselineDiagnostic},
    repo_root,
};

/// Per case: right lines by code, and the code blocking it if exactly one does.
type Row = (BTreeMap<u32, usize>, Option<u32>);

fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let rows: Vec<Row> = cases.par_iter().filter_map(measure).collect();

    let mut emits: BTreeMap<u32, usize> = BTreeMap::new();
    let mut blocked: BTreeMap<u32, usize> = BTreeMap::new();
    for (right, blocker) in rows {
        for (code, n) in right {
            *emits.entry(code).or_default() += n;
        }
        if let Some(code) = blocker {
            *blocked.entry(code).or_default() += 1;
        }
    }

    let mut ranked: Vec<(u32, usize, usize)> = blocked
        .iter()
        .map(|(&code, &cases)| (code, cases, emits.get(&code).copied().unwrap_or(0)))
        .collect();
    // Under-firing rules first: the machinery already exists, so the work is a
    // predicate rather than a port.
    ranked.sort_by(|a, b| (b.2 > 0).cmp(&(a.2 > 0)).then(b.1.cmp(&a.1)).then(a.0.cmp(&b.0)));

    println!("{:<8} {:>8} {:>8}   what the row needs", "code", "blocked", "emits");
    for (code, cases, right) in ranked.iter().take(40) {
        let verdict = if *right == 0 {
            "ABSENT — port it, or refuse with an owner"
        } else {
            "UNDER-FIRES — widen where the rule is asked"
        };
        println!("TS{code:<6} {cases:>8} {right:>8}   {verdict}");
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

    let mut right: BTreeMap<u32, usize> = BTreeMap::new();
    for (key, n) in &got {
        let agreed = (*n).min(want.get(key).copied().unwrap_or(0));
        if agreed > 0 {
            *right.entry(key.3).or_default() += agreed;
        }
    }

    // `diaggap`'s population, re-derived so the two cannot drift.
    let mut missing: Vec<u32> = Vec::new();
    for (key, n) in &want {
        for _ in got.get(key).copied().unwrap_or(0)..*n {
            missing.push(key.3);
        }
    }
    let extra: usize =
        got.iter().map(|(key, n)| n.saturating_sub(want.get(key).copied().unwrap_or(0))).sum();
    let blocker = match missing.first() {
        Some(&code) if extra == 0 && missing.iter().all(|&it| it == code) => Some(code),
        _ => None,
    };
    Some((right, blocker))
}
