//! Right code, right line, **wrong column**.
//!
//! ```text
//! cargo run --release -p tsr-conformance --example diagcolumn
//! ```
//!
//! # The question this answers, and why the existing instruments do not
//!
//! `diaggap` counts what is missing by code and `extragap` what is invented by
//! code. A diagnostic reported at the wrong column appears in **both** columns
//! at once — as a miss at the expected position and an extra at the reported
//! one — and neither instrument can tell that pair apart from an unrelated
//! miss plus an unrelated extra.
//!
//! That distinction is worth an instrument because the two have utterly
//! different prices. A missing diagnostic needs a rule ported. A **column** is
//! one call: §232 turned `self.nodes.span(node)` into `self.error_span(node)`
//! and TS2440's wrong column went to zero, because upstream's `c.error(node, …)`
//! narrows a *named* declaration to its name via `getErrorSpanForNode`. Any
//! other rule reporting on a named declaration has the same bug available.
//!
//! **Cases blocked by column alone are the headline.** Those pass the moment
//! the span rule is right, with no new rule ported at all.
//!
//! `docs/architecture/checker-notes-diag2.md` §233.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{
    CaseEntry, Corpus,
    errors_baseline::{self, BaselineDiagnostic},
    repo_root,
};

/// A diagnostic identified without its column.
type Position = (String, u32, u32);

/// One case's column-only mismatches: code, expected column, reported column.
type Row = (String, Vec<(u32, u32, u32)>, bool);

fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let rows: Vec<Row> = cases.par_iter().filter_map(measure).collect();

    let total: usize = rows.iter().map(|(_, lines, _)| lines.len()).sum();
    let blocked = rows.iter().filter(|(_, _, only)| *only).count();
    println!("cases with a column-only mismatch: {}", rows.len());
    println!("column-only mismatched lines:      {total}");
    println!("**cases blocked by column ALONE:   {blocked}**\n");

    let mut by_code: BTreeMap<u32, usize> = BTreeMap::new();
    for (_, lines, _) in &rows {
        for &(code, _, _) in lines {
            *by_code.entry(code).or_default() += 1;
        }
    }
    let mut ranked: Vec<(u32, usize)> = by_code.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    println!("-- by code --");
    for (code, n) in ranked.iter().take(25) {
        println!("  TS{code:<6} {n:>4}");
    }

    println!("\n-- cases blocked by column ALONE --");
    for (name, lines, only) in rows.iter().filter(|(_, _, only)| *only).take(30) {
        let _ = only;
        let detail: Vec<String> =
            lines.iter().map(|(code, want, got)| format!("TS{code} {want}->{got}")).collect();
        println!("  {:<62} {}", name, detail.join(" "));
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

    // The multiset difference in both directions, per §152 — a case wanting one
    // and getting two is a difference a set-valued diff would hide.
    let mut want: BTreeMap<(String, u32, u32, u32), usize> = BTreeMap::new();
    for d in &expected {
        *want.entry((d.file.clone(), d.line, d.column, d.code)).or_default() += 1;
    }
    let mut got: BTreeMap<(String, u32, u32, u32), usize> = BTreeMap::new();
    for d in &actual {
        *got.entry((d.file.clone(), d.line, d.column, d.code)).or_default() += 1;
    }
    for (key, n) in &got {
        if let Some(m) = want.get_mut(key) {
            let matched = (*m).min(*n);
            *m -= matched;
        }
    }
    let mut missing: Vec<(String, u32, u32, u32)> = Vec::new();
    for (key, n) in &want {
        for _ in 0..*n {
            missing.push(key.clone());
        }
    }
    let mut extra: Vec<(String, u32, u32, u32)> = Vec::new();
    for (key, n) in &got {
        let claimed = expected
            .iter()
            .filter(|d| (&d.file, d.line, d.column, d.code) == (&key.0, key.1, key.2, key.3))
            .count();
        for _ in claimed..*n {
            extra.push(key.clone());
        }
    }

    // A column-only mismatch: the same (file, line, code) is missing at one
    // column and invented at another.
    let mut extra_by_position: BTreeMap<Position, Vec<u32>> = BTreeMap::new();
    for (file, line, column, code) in &extra {
        extra_by_position.entry((file.clone(), *line, *code)).or_default().push(*column);
    }
    let mut paired = Vec::new();
    let mut unpaired_missing = 0usize;
    for (file, line, column, code) in &missing {
        let key = (file.clone(), *line, *code);
        match extra_by_position.get_mut(&key).and_then(Vec::pop) {
            Some(reported) => paired.push((*code, *column, reported)),
            None => unpaired_missing += 1,
        }
    }
    if paired.is_empty() {
        return None;
    }
    let leftover_extra: usize = extra_by_position.values().map(Vec::len).sum();
    let blocked_by_column_alone = unpaired_missing == 0 && leftover_extra == 0;
    Some((case.name.clone(), paired, blocked_by_column_alone))
}
