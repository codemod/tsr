//! Where the *invented* parser diagnostics are, by case.
//!
//! ```text
//! cargo run --release -p tsr-conformance --example parserextra
//! ```
//!
//! # The question this answers, and why the existing instruments do not
//!
//! `extragap` counts the extra column by **code** — `TS1005` at 391 invented
//! lines — and `extraonly` lists the cases blocked by an extra **alone**, which
//! after §219 is 25. Neither says *where* the 1,150 invented parser lines are,
//! and §219 recorded that gap as the open question: four list loops were fixed
//! and seven measured as unwanted, so the lines are somewhere else.
//!
//! **Invented, not displaced.** A line counts here when the baseline has *no*
//! diagnostic of that code at that position — the shape a recovery divergence
//! produces, as opposed to a right diagnostic at a wrong column.
//!
//! `docs/architecture/checker-notes-diag2.md` §220.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{
    CaseEntry, Corpus,
    errors_baseline::{self, BaselineDiagnostic},
    repo_root,
};

/// The codes a recovering parser invents. `extragap`'s head, in order.
const PARSER_CODES: &[u32] = &[1005, 1109, 1012, 1003, 1131, 1128, 1125, 1110, 1002, 1136];

/// One invented diagnostic: line, column, code.
type Invented = (u32, u32, u32);

/// A case and the lines it invents.
type Row = (String, Vec<Invented>);

fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let rows: Vec<Row> = cases.par_iter().filter_map(measure).collect();

    let total: usize = rows.iter().map(|(_, lines)| lines.len()).sum();
    println!("cases with an invented parser diagnostic: {}", rows.len());
    println!("invented parser lines: {total}\n");

    let mut ranked: Vec<&Row> = rows.iter().collect();
    ranked.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));
    println!("-- worst cases --");
    for (name, lines) in ranked.iter().take(20) {
        let mut codes: BTreeMap<u32, usize> = BTreeMap::new();
        for &(_, _, code) in lines {
            *codes.entry(code).or_default() += 1;
        }
        let summary: Vec<String> = codes.iter().map(|(code, n)| format!("TS{code}×{n}")).collect();
        println!("{:>4}  {:<62} {}", lines.len(), name, summary.join(" "));
    }

    // **The distribution is the answer, not the head.** A few enormous cases and
    // a long tail are a different problem from an even spread: the first is a
    // handful of recovery paths, the second is the parser's error behaviour in
    // general.
    let mut buckets: BTreeMap<&str, usize> = BTreeMap::new();
    for (_, lines) in &rows {
        let bucket = match lines.len() {
            0 => continue,
            1 => "1 line",
            2..=4 => "2-4",
            5..=9 => "5-9",
            10..=29 => "10-29",
            _ => "30+",
        };
        *buckets.entry(bucket).or_default() += 1;
    }
    println!("\n-- cases by how many lines they invent --");
    for bucket in ["1 line", "2-4", "5-9", "10-29", "30+"] {
        if let Some(n) = buckets.get(bucket) {
            println!("  {bucket:<8} {n:>4} cases");
        }
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

    // A multiset difference, per §152: a case wanting one and getting two is a
    // difference a set-valued diff would hide.
    let mut remaining: BTreeMap<(String, u32, u32, u32), usize> = BTreeMap::new();
    for diagnostic in &expected {
        *remaining
            .entry((diagnostic.file.clone(), diagnostic.line, diagnostic.column, diagnostic.code))
            .or_default() += 1;
    }
    let mut invented = Vec::new();
    for diagnostic in actual.iter().filter(|d| PARSER_CODES.contains(&d.code)) {
        let key = (diagnostic.file.clone(), diagnostic.line, diagnostic.column, diagnostic.code);
        match remaining.get_mut(&key) {
            Some(n) if *n > 0 => *n -= 1,
            _ => invented.push((diagnostic.line, diagnostic.column, diagnostic.code)),
        }
    }
    if invented.is_empty() {
        return None;
    }
    Some((case.name.clone(), invented))
}
