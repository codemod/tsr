//! Unfiltered diagnostic cases as `case<TAB>verdict<TAB>expected<TAB>actual`.
//!
//! Includes empty expected baselines so new false errors remain visible. Sorted
//! diagnostic lists preserve duplicate occurrences, unlike a code or pass set.
//! Configuration-varied, known-divergence and absent-baseline cases are excluded
//! by the same eligibility checks as the diagnostics suite.
//!
//! ```text
//! cargo run --release -p tsr-conformance --example diagverdictdump > before.tsv
//! ```

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{Corpus, diagnostics_suite, errors_baseline, repo_root};

fn main() {
    rayon::ThreadPoolBuilder::new()
        .stack_size(8 * 1024 * 1024)
        .build_global()
        .expect("sizing the corpus thread pool");
    let cases = Corpus::from_repo_root(&repo_root()).discover().expect("corpus");
    let mut rows: Vec<_> = cases
        .par_iter()
        .filter(|case| {
            !case.has_varied_errors() && !case.has_known_divergence() && case.has_any_baseline()
        })
        .map(|case| {
            let baseline = case.expected_errors().expect("baseline");
            let mut expected = baseline.as_deref().map(errors_baseline::parse).unwrap_or_default();
            let test = case.load().expect("case");
            let mut actual = diagnostics_suite::reported_for(&test);
            expected.sort_unstable();
            actual.sort_unstable();
            let verdict = match (expected.is_empty(), expected == actual) {
                (false, true) => "RIGHT",
                (false, false) => "WRONG",
                (true, true) => "EMPTY_RIGHT",
                (true, false) => "EMPTY_WRONG",
            };
            (case.name.clone(), verdict, expected, actual)
        })
        .collect();
    rows.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    for (case, verdict, expected, actual) in rows {
        println!("{case}\t{verdict}\t{expected:?}\t{actual:?}");
    }
}
