//! **Which code, if built, converts the most cases?** — for every code at once.
//!
//! ```text
//! cargo run --release -p tsr-conformance --example diagsole
//! ```
//!
//! `diagmissing <code>` answers this for one code and needs the code chosen
//! first, which is the step that has been guessed all session: §941 ranked the
//! tail by **corpus-wide missing lines**, and §946 then found TS1183's sixty
//! lines were fifty-four in a single hopeless fixture and four in the parser's
//! layer — six reachable. §948's row was picked the same way and paid, but only
//! because `diagmissing` was run afterwards and reported *four cases blocked on
//! TS2693 alone*.
//!
//! **Line counts do not rank rules; sole-obstacle case counts do.** This is that
//! number for all 232 codes in one corpus pass instead of 232, and it is the
//! list §941 should have been.
//!
//! A case appears under code `C` when `C` is the *only* thing wrong with it:
//! every other expected diagnostic is present and nothing extra is reported. It
//! is therefore exactly one conversion, and the sum of this column is an upper
//! bound on what single-code work can reach without touching anything else.
use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{
    CaseEntry, Corpus,
    errors_baseline::{self, BaselineDiagnostic},
    repo_root,
};

/// `(code, lines)` when exactly one code is missing and nothing is extra.
fn measure(case: &CaseEntry) -> Option<(u32, usize)> {
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
    if actual.iter().any(|d| !expected.contains(d)) {
        return None;
    }
    let missing: Vec<&BaselineDiagnostic> =
        expected.iter().filter(|d| !actual.contains(d)).collect();
    if missing.is_empty() {
        return None;
    }
    let code = missing[0].code;
    // **One code, not one line.** A case wanting three lines of the same code is
    // still one conversion for one rule; a case wanting two different codes is
    // two rules' work and belongs to neither.
    if missing.iter().any(|d| d.code != code) {
        return None;
    }
    Some((code, missing.len()))
}

fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let found: Vec<(u32, usize)> = cases.par_iter().filter_map(measure).collect();
    let mut by_code: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
    for (code, lines) in found {
        let entry = by_code.entry(code).or_default();
        entry.0 += 1;
        entry.1 += lines;
    }
    let mut rows: Vec<(u32, usize, usize)> =
        by_code.into_iter().map(|(code, (cases, lines))| (code, cases, lines)).collect();
    rows.sort_unstable_by_key(|&(code, cases, _)| (std::cmp::Reverse(cases), code));
    println!("{:>8}  {:>5}  {:>5}", "code", "cases", "lines");
    let mut total = 0usize;
    for (code, cases, lines) in &rows {
        println!("  TS{code:<6}{cases:>5}  {lines:>5}");
        total += cases;
    }
    println!("\n{total} cases are blocked on exactly one code, across {} codes", rows.len());
}
