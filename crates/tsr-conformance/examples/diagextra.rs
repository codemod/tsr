//! Every **extra** diagnostic this port reports, over the suite's whole judged
//! population — not only the cases one removal from passing.
//!
//! ```text
//! cargo run --release -p tsr-conformance --example diagextra 18048
//! ```
//!
//! # Why this is not `extraonly`
//!
//! `extraonly` lists extras **in cases whose only defect is an extra**, which is
//! the right population for *"what converts a case"* and the wrong one for
//! *"how big is this rule's over-reporting"*. §921 measured TS18048 at
//! `have − want = 129` while §850 had recorded **11** from `extraonly`, and the
//! other 118 sit in cases already failing for other reasons.
//!
//! An optional code argument filters to one diagnostic; without it, every extra
//! is printed and the tail counts them by code.
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{
    CaseEntry, Corpus,
    errors_baseline::{self, BaselineDiagnostic},
    repo_root,
};

fn main() {
    rayon::ThreadPoolBuilder::new()
        .stack_size(8 * 1024 * 1024)
        .build_global()
        .expect("sizing the corpus thread pool");
    let only: Option<u32> = std::env::args().nth(1).and_then(|a| a.parse().ok());
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let rows: Vec<(String, Vec<(u32, String)>)> =
        cases.par_iter().filter_map(|case| measure(case, only)).collect();
    let mut by_case: Vec<(usize, &String)> = rows.iter().map(|(n, e)| (e.len(), n)).collect();
    by_case.sort_unstable_by_key(|&(count, _)| std::cmp::Reverse(count));
    for (count, name) in by_case.iter().take(20) {
        println!("{count:5}  {name}");
    }
    let total: usize = rows.iter().map(|(_, e)| e.len()).sum();
    println!("cases with an extra: {}   extra lines: {total}", rows.len());
}

/// The extras this port reports that the baseline does not, over every judged
/// case — the same population the suite scores.
fn measure(case: &CaseEntry, only: Option<u32>) -> Option<(String, Vec<(u32, String)>)> {
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
    let extras: Vec<(u32, String)> = actual
        .iter()
        .filter(|d| !expected.contains(d))
        .filter(|d| only.is_none_or(|code| d.code == code))
        .map(|d| (d.code, format!("{}({},{})", d.file, d.line, d.column)))
        .collect();
    if extras.is_empty() { None } else { Some((case.name.clone(), extras)) }
}
