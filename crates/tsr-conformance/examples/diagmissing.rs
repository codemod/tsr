//! The **missing** half of one diagnostic code: the lines the baseline records
//! and the port does not, restricted to the cases that code alone is blocking.
//!
//! ```text
//! cargo run --release -p tsr-conformance --example diagmissing -- 2454
//! ```
//!
//! `diaggap.rs` ranks the codes and `extragap.rs` splits the *extra* column;
//! neither says **which lines** are absent, and a rule that under-reports is
//! diagnosed by reading them. Every case printed here is one whose only defect
//! is a missing diagnostic of the given code, so each is exactly one conversion
//! — `checker-notes-diag2.md` §42 was steered by this list.
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{
    CaseEntry, Corpus,
    errors_baseline::{self, BaselineDiagnostic},
    repo_root,
};

fn main() {
    let code: u32 = std::env::args()
        .nth(1)
        .and_then(|argument| argument.trim_start_matches("TS").parse().ok())
        .expect("usage: diagmissing <code>");
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let rows: Vec<(String, Vec<String>)> =
        cases.par_iter().filter_map(|case| measure(case, code)).collect();
    let mut total = 0usize;
    for (name, missing) in &rows {
        for m in missing {
            println!("{name}  {m}");
            total += 1;
        }
    }
    println!("total missing TS{code} lines: {total}");
    println!("cases blocked on TS{code} alone: {}", rows.len());
}

fn measure(case: &CaseEntry, code: u32) -> Option<(String, Vec<String>)> {
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
    let missing: Vec<String> = expected
        .iter()
        .filter(|d| d.code == code && !actual.contains(d))
        .map(|d| format!("{}({},{})", d.file, d.line, d.column))
        .collect();
    if missing.is_empty() {
        return None;
    }
    // Sole obstacle: every other expected diagnostic is present and nothing
    // extra is reported, so the case converts the moment these lines arrive.
    let other_missing = expected.iter().any(|d| d.code != code && !actual.contains(d));
    let extra = actual.iter().any(|d| !expected.contains(d));
    if other_missing || extra {
        return None;
    }
    Some((case.name.clone(), missing))
}
