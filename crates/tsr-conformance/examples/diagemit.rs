//! How many diagnostics of each code this port **emits**, beside how many the
//! baselines record.
//!
//! ```text
//! cargo run --release -p tsr-conformance --example diagemit
//! ```
//!
//! `diagmissing.rs` says which lines a rule is short; this says whether the
//! rule speaks at all. A code with a large `want` and a `have` of zero or
//! near-zero is a rule that is **not running** rather than one that is
//! declining — `checker-notes-diag2.md` §64 found one of those by hand
//! (`Binder::symbol_of` answers `None` for a `Constructor`, which returned
//! `checkFunctionOrConstructorSymbol` on its first line and silenced four codes
//! at once), and this is the instrument that would have found it in one run.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{
    CaseEntry, Corpus,
    errors_baseline::{self, BaselineDiagnostic},
    repo_root,
};

fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let rows: Vec<(BTreeMap<u32, usize>, BTreeMap<u32, usize>)> =
        cases.par_iter().filter_map(measure).collect();

    let (mut want, mut have): (BTreeMap<u32, usize>, BTreeMap<u32, usize>) =
        (BTreeMap::new(), BTreeMap::new());
    for (expected, actual) in &rows {
        for (code, count) in expected {
            *want.entry(*code).or_default() += count;
        }
        for (code, count) in actual {
            *have.entry(*code).or_default() += count;
        }
    }

    println!("{:<10} {:>8} {:>8}   rule", "code", "want", "have");
    let mut ranked: Vec<(&u32, &usize)> = want.iter().collect();
    ranked.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
    for (code, wanted) in ranked.iter().take(60) {
        let emitted = have.get(code).copied().unwrap_or(0);
        let note = if !RULE_CODES.contains(code) {
            "unported"
        } else if emitted == 0 {
            "**SILENT**"
        } else if emitted * 4 < **wanted {
            "quiet"
        } else {
            ""
        };
        println!("TS{code:<8} {wanted:>8} {emitted:>8}   {note}");
    }
}

fn measure(case: &CaseEntry) -> Option<(BTreeMap<u32, usize>, BTreeMap<u32, usize>)> {
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
    let mut want = BTreeMap::new();
    let mut have = BTreeMap::new();
    for diagnostic in &expected {
        *want.entry(diagnostic.code).or_default() += 1;
    }
    for diagnostic in &actual {
        *have.entry(diagnostic.code).or_default() += 1;
    }
    Some((want, have))
}

/// The codes this port's rules emit — kept in step with `diag2307.rs`.
const RULE_CODES: &[u32] = &[
    2307, 2882, 2564, 2304, 2454, 2369, 2695, 1104, 1105, 1107, 1115, 1116, 1036, 1183, 2384, 2389,
    2390, 2391, 2392, 2393, 6133, 6138, 6192, 6196, 6198, 6199, 6205, 2322, 7006, 7019, 2314, 2707,
    2554, 2555, 2339, 2741, 2353, 2345, 2411, 2415, 2416, 2420, 2430, 2583, 2301, 2352, 18050,
    2552, 2367, 2872, 2873, 1345, 2365, 18047, 18048, 18049, 2531, 2532, 2533, 2464, 2540, 2362,
    2363, 2356, 2341, 2445, 2374,
];
