//! Cases whose **only** defect is a diagnostic the baseline does not record —
//! each is a single false positive away from passing.
//!
//! ```text
//! cargo run --release -p tsr-conformance --example extraonly
//! ```
//!
//! `diagreach.rs` ranks the *missing* side; this is its twin on the *extra*
//! side, and it is the sharpest list in the workstream: every line printed is a
//! diagnostic to remove, and every case is one removal from a conversion.
//! `checker-notes-diag2.md` §58 was found by reading it — TS2454's fifteen
//! lines were two cases and one unported narrowing.
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{
    CaseEntry, Corpus,
    errors_baseline::{self, BaselineDiagnostic},
    repo_root,
};

fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let rows: Vec<(String, Vec<String>)> = cases.par_iter().filter_map(measure).collect();
    for (name, extras) in &rows {
        for extra in extras {
            println!("{name}  {extra}");
        }
    }
    println!("cases blocked by an EXTRA alone: {}", rows.len());
}

fn measure(case: &CaseEntry) -> Option<(String, Vec<String>)> {
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
    if expected.iter().any(|d| !actual.contains(d)) {
        return None;
    }
    let extras: Vec<String> = actual
        .iter()
        .filter(|d| !expected.contains(d))
        .map(|d| format!("{}({},{}) TS{}", d.file, d.line, d.column, d.code))
        .collect();
    if extras.is_empty() { None } else { Some((case.name.clone(), extras)) }
}
