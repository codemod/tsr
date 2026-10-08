//! Unfiltered diagnostic cases as `case<TAB>verdict<TAB>expected<TAB>actual`.
//!
//! Includes empty expected baselines so new false errors remain visible. Sorted
//! diagnostic lists preserve duplicate occurrences, unlike a code or pass set.
//! Known-divergence and absent-baseline cases are excluded by the same
//! eligibility checks as the diagnostics suite. A configuration-varied case is
//! dumped once per named configuration upstream compiles it under, keyed
//! `case(target=es2015)` (ADR-0047), and never as itself: its plain row would
//! judge a compilation upstream does not run.
//!
//! ```text
//! cargo run --release -p tsr-conformance --example diagverdictdump > before.tsv
//! ```
//!
//! Every row ends with two guard columns, `ms=<wall>` and `mib=<peak memory>`
//! for the case (`tsr_conformance::case_guard`, `tsr-2zk.1041`), after the
//! four above, which keep their bytes. A case that panics prints `PANIC` as
//! its verdict; one the watchdog stops prints `OOM` or `TIMEOUT` and ends the
//! run with exit status 3. `examples/slowcases.rs` reads the guard columns;
//! `docs/parity/notes/r5-harness.md` says how the gate uses them.

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::case_guard::{self, RowShape};
use tsr_conformance::{Corpus, diagnostics_suite, errors_baseline, repo_root};

#[path = "support/counting_alloc.rs"]
mod counting_alloc;

fn main() {
    case_guard::size_worker_pool();
    case_guard::install(RowShape::Diagnostics);
    let mut cases = Corpus::from_repo_root(&repo_root()).discover().expect("corpus");
    let configured = Corpus::configured(&cases);
    cases.extend(configured);
    // `TSR_FILTER` (comma-separated case-name substrings), as `verdictdump`
    // takes it, for the inner loop; scoring runs stay unfiltered.
    let filter: Option<Vec<String>> = std::env::var("TSR_FILTER").ok().map(|value| {
        value.split(',').filter(|part| !part.is_empty()).map(str::to_string).collect()
    });
    let mut rows: Vec<_> = cases
        .par_iter()
        .filter(|case| {
            filter
                .as_ref()
                .is_none_or(|parts| parts.iter().any(|part| case.name.contains(part.as_str())))
        })
        .filter(|case| {
            (case.configuration.is_some() || !case.has_varied_errors())
                && !case.has_known_divergence()
                && case.has_any_baseline()
                && !case.is_expanded()
        })
        .map(|case| {
            let measured = case_guard::run_case(&case.name, || {
                let baseline = case.expected_errors().expect("baseline");
                let mut expected =
                    baseline.as_deref().map(errors_baseline::parse).unwrap_or_default();
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
                format!("{}\t{verdict}\t{expected:?}\t{actual:?}", case.name)
            });
            let columns = measured.columns();
            let row = match measured.value {
                Ok(row) => format!("{row}\t{columns}"),
                Err(message) => {
                    RowShape::Diagnostics.marker(&case.name, "PANIC", &message, &columns)
                }
            };
            (case.name.clone(), row)
        })
        .collect();
    rows.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    for (_, row) in rows {
        println!("{row}");
    }
}
