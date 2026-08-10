//! The **set of passing case names** for the `diagnostics` suite, one per line.
//!
//! ```text
//! cargo run --release -p tsr-conformance --example diagpass > /tmp/before.txt
//! …edit…
//! cargo run --release -p tsr-conformance --example diagpass > /tmp/after.txt
//! LC_ALL=C comm -23 /tmp/before.txt /tmp/after.txt    # LOST
//! LC_ALL=C comm -13 /tmp/before.txt /tmp/after.txt    # GAINED
//! ```
//!
//! **`LC_ALL=C` is not optional.** This sorts with Rust's `sort_unstable`,
//! which is byte order; `comm` under a UTF-8 locale collates differently, warns
//! *"file 1 is not in sorted order"* on stderr — and **still prints a result**.
//! A wrong answer with a warning is worse than an error, and the first run of
//! this instrument produced one.
//!
//! # Why this exists
//!
//! Every build note in `checker-notes-diag2.md` asserts `0 LOST`, and until
//! §850 that assertion was checked by eye: a `+1` with one obvious new case is
//! self-evidently `+1/−0`. §850 measured a net `+3` alongside eleven new wrong
//! lines and **could not distinguish `+3/−0` from `+4/−1`**, because
//! `snapshots/diagnostics.snap` records aggregate counts and the first hundred
//! failures — not the pass set.
//!
//! `casedelta.rs` has done exactly this for `checker_types` since it was
//! written; this is its `diagnostics` twin, and it exists so that a net gain
//! can be decomposed rather than assumed. `bd tsr-5wv0`.
//!
//! Judged cases only — the same population the suite scores, so the line count
//! equals the suite's `Passed`.
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{
    CaseEntry, Corpus,
    errors_baseline::{self, BaselineDiagnostic},
    repo_root,
};

fn main() {
    // The same pool size the harness itself uses (`src/main.rs:83`); the
    // checker recurses deeply enough on the corpus that rayon's default stack
    // overflows, which is why every long-running example sets this.
    rayon::ThreadPoolBuilder::new()
        .stack_size(8 * 1024 * 1024)
        .build_global()
        .expect("sizing the corpus thread pool");
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let mut passing: Vec<String> = cases.par_iter().filter_map(measure).collect();
    passing.sort_unstable();
    for name in &passing {
        println!("{name}");
    }
    eprintln!("passing: {}", passing.len());
}

/// The case's name when this port reports **exactly** the baseline's
/// diagnostics, as sorted multisets of `(file, line, column, code)` — which is
/// the suite's own comparison.
fn measure(case: &CaseEntry) -> Option<String> {
    if case.has_varied_errors() || case.has_known_divergence() || !case.has_any_baseline() {
        return None;
    }
    let baseline = case.expected_errors().ok()?;
    let mut expected: Vec<BaselineDiagnostic> =
        baseline.as_deref().map(errors_baseline::parse).unwrap_or_default();
    // `Outcome::Skipped` for an empty baseline (`diagnostics_suite.rs:123`) —
    // *"the case expects no diagnostics"*. Without this the population is 7,069
    // rather than the suite's 5,488, and a pass-set diff over the wrong
    // population is worse than none.
    if expected.is_empty() {
        return None;
    }
    let test = case.load().ok()?;
    let mut actual = tsr_conformance::diagnostics_suite::reported_for(&test);
    expected.sort_unstable();
    actual.sort_unstable();
    (expected == actual).then(|| case.name.clone())
}
