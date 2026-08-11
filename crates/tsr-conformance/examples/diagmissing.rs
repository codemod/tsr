//! **The counts are restricted, and the restriction is the point.** `measure`
//! drops any case that is *also* missing another code or reporting an extra, so
//! both numbers below describe only cases this code alone blocks. The line count
//! is therefore **not** the corpus-wide total for the code — `diagemit`'s
//! `want − have` is nearer that, and §884 records what it is and is not.
//! §927 relabelled the line: it read `total missing`, and every row priced from
//! it this session was priced on the restricted figure.
//!
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
    let mut rows: Vec<(String, Vec<String>)> =
        cases.par_iter().filter_map(|case| measure(case, code)).collect();
    // **The unrestricted count**, over every judged case rather than only the
    // ones this code alone blocks. §927 found that the restricted figure had
    // been printed as `total missing` for seven hundred sections; this is the
    // number that label promised. §928.
    let mut wide: Vec<(usize, String)> =
        cases.par_iter().filter_map(|case| count_all(case, code)).collect();
    wide.sort_unstable_by_key(|&(count, _)| std::cmp::Reverse(count));
    let corpus_wide: usize = wide.iter().map(|(n, _)| n).sum();
    // **Cheapest case first.** A partial fix converts a case only if it supplies
    // *every* line (§273), so the case worth opening is the one wanting fewest.
    // Printing alphabetically cost §339 a build: `diagslice` called TS2454
    // "mostly single-line" *as a row*, and the case this printed first wanted
    // five. A row-level convertibility verdict does not transfer to the case you
    // open, so the line count belongs next to the case. §340, §341.
    rows.sort_by(|a, b| a.1.len().cmp(&b.1.len()).then_with(|| a.0.cmp(&b.0)));
    let mut total = 0usize;
    for (name, missing) in &rows {
        let wants = missing.len();
        for m in missing {
            println!("[{wants} line(s)] {name}  {m}");
            total += 1;
        }
    }
    println!("missing TS{code} lines IN SOLE-OBSTACLE CASES: {total}");
    println!("cases blocked on TS{code} alone: {}", rows.len());
    println!("missing TS{code} lines CORPUS-WIDE: {corpus_wide}");
    // **The distribution decides the owner.** §922 established it on the extras
    // side: a flat count is a rule mis-firing everywhere, a concentrated one is
    // a rule meeting one unported feature, and those have different owners.
    for (count, name) in wide.iter().take(8) {
        println!("  {count:5}  {name}");
    }
}

/// Every missing line for this code, over the suite's whole judged population —
/// the count `measure`'s sole-obstacle filter discards. §928.
fn count_all(case: &CaseEntry, code: u32) -> Option<(usize, String)> {
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
    let count = expected.iter().filter(|d| d.code == code && !actual.contains(d)).count();
    (count > 0).then(|| (count, case.name.clone()))
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
