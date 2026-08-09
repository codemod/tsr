//! Ask any suite what it did with a case — one case, or all of them.
//!
//! ```text
//! # one case, one suite
//! cargo run --release -p tsr-conformance --example casequery -- binder_symbols compiler/umd1
//!
//! # one case, every suite that judges it
//! cargo run --release -p tsr-conformance --example casequery -- all compiler/umd1
//!
//! # every case, as a joinable TSV — this is what makes a commit pair diffable
//! cargo run --release -p tsr-conformance --example casequery -- diagnostics --list > before.tsv
//! # ... change something ...
//! cargo run --release -p tsr-conformance --example casequery -- diagnostics --list > after.tsv
//! diff before.tsv after.tsv
//! ```
//!
//! # Why this exists
//!
//! Two questions the harness could not answer, both of which cost real sessions:
//!
//! 1. **"Is this case judged, and what did it produce?"** The committed snapshot
//!    carries totals, a skip breakdown, and the *first 100* failures
//!    (`suite.rs`'s `MAX_REPORTED_FAILURES`). A case outside that hundred is
//!    indistinguishable from a case that passed, and the only way to find out
//!    was to add an `eprintln` to the suite and re-run the corpus.
//!    `examples/diagcase.rs` answers it for `diagnostics` alone by calling that
//!    suite's `reported_for` directly; nothing answered it for `binder_symbols`,
//!    which is the suite this file was written to interrogate.
//!
//! 2. **"Which case regressed?"** A suite that moves by −1 over 5,488 cases
//!    names the case nowhere. `examples/casedelta.rs` solves this for
//!    `checker_types` by emitting per-case tallies; it cannot be pointed at any
//!    other suite because it reads `types_suite::compare` directly. The `--list`
//!    mode here is the same idea taken through the `Suite` trait, so it works
//!    for all sixteen — and finding the one lost `diagnostics` case behind the
//!    `declare global` merge is what it was first used for.
//!
//! # What it does *not* do
//!
//! It reports the verdict and the suite's own reason string, not a diff of the
//! case's contents. For `diagnostics`, `diagcase.rs` still prints expected and
//! actual side by side and is the better tool once this one has named the case;
//! this file deliberately does not duplicate it, because a per-suite renderer
//! cannot go through the `Suite` trait — `run` returns a `String`, and what is
//! in that string is each suite's business.
//!
//! # The verdicts are the suite's, not a re-derivation
//!
//! Every row comes from `Suite::judge`, so the skip predicates, their order, and
//! the reason strings are the ones the snapshot was written from. `--list`
//! prints the same four tallies to stderr as a control: they must equal the
//! committed snapshot's, or the rows are over some other population.

use rayon::prelude::*;
use tsr_conformance::{
    Corpus,
    binder_suite::BinderSymbols,
    diagnostics_suite::Diagnostics,
    dts_emit_suite::DtsEmit,
    dts_shape_suite::DtsShape,
    dts_suite::IsolatedDeclarations,
    dts_target_suite::DtsReachableTarget,
    loader_suite::FileLoaderRequests,
    module_suite::ModuleResolution,
    printer_suite::PrinterRoundTrip,
    repo_root,
    scanner_suite::{ScannerCleanFiles, ScannerTermination},
    suite::{Outcome, Suite},
    suites::{BaselineResolution, CorpusIngest, Parser, ParserReachable},
    types_suite::CheckerTypes,
};

/// The same registry `bin/coverage` runs, in the same order.
///
/// Duplicated rather than shared because `coverage`'s copy is a local of `main`
/// and lifting it into the library would put every suite's construction cost on
/// every consumer of the crate. **The cost of the duplication is real**: a suite
/// added to `bin/coverage` and not here is queryable nowhere, and nothing fails
/// when that happens. If a third caller ever needs the list, that is the point
/// at which it should move into `suites.rs` and both callers read it.
fn registry() -> Vec<Box<dyn Suite + Sync>> {
    vec![
        Box::new(CorpusIngest),
        Box::new(BaselineResolution),
        Box::new(ParserReachable),
        Box::new(ScannerTermination),
        Box::new(ScannerCleanFiles),
        Box::new(Parser),
        Box::new(BinderSymbols),
        Box::new(ModuleResolution),
        Box::new(FileLoaderRequests),
        Box::new(IsolatedDeclarations),
        Box::new(DtsReachableTarget),
        Box::new(DtsEmit),
        Box::new(DtsShape),
        Box::new(PrinterRoundTrip),
        Box::new(CheckerTypes),
        Box::new(Diagnostics),
    ]
}

/// `PASS` / `FAIL` / `UNSUP` / `SKIP`, fixed width so a TSV column sorts.
fn verdict(outcome: &Outcome) -> &'static str {
    match outcome {
        Outcome::Passed => "PASS",
        Outcome::Failed { .. } => "FAIL",
        Outcome::Unsupported { .. } => "UNSUP",
        Outcome::Skipped { .. } => "SKIP",
    }
}

/// The suite's own explanation, flattened onto one line so the TSV stays one
/// row per case. A `Passed` has none.
fn reason(outcome: &Outcome) -> String {
    let text = match outcome {
        Outcome::Passed => return String::new(),
        Outcome::Failed { reason }
        | Outcome::Unsupported { reason }
        | Outcome::Skipped { reason } => reason,
    };
    text.replace(['\n', '\t'], " ")
}

/// The corpus is a compiler test suite and contains inputs written to break
/// compilers; `bin/coverage` documents the measured depths that make rayon's
/// 2 MiB default insufficient. Same budget here, for the same reason.
const WORKER_STACK: usize = 8 * 1024 * 1024;

fn main() {
    rayon::ThreadPoolBuilder::new()
        .stack_size(WORKER_STACK)
        .build_global()
        .expect("sizing the corpus thread pool");

    let mut args = std::env::args().skip(1);
    let suite_name = args.next().unwrap_or_else(|| {
        eprintln!(
            "usage: casequery <suite|all> <case-name>\n       \
             casequery <suite> --list\n\nsuites: {}",
            registry().iter().map(|s| s.name()).collect::<Vec<_>>().join(", ")
        );
        std::process::exit(2);
    });
    let target = args.next();

    let mut suites = registry();
    if suite_name != "all" {
        let known = suites.iter().any(|suite| suite.name() == suite_name);
        assert!(known, "unknown suite {suite_name:?}");
        suites.retain(|suite| suite.name() == suite_name);
    }

    let corpus = Corpus::from_repo_root(&repo_root());
    assert!(corpus.is_available(), "corpus missing; run git submodule update --init --recursive");
    let cases = corpus.discover().expect("discovering cases");

    // `--list`: every case, one row each, sorted by name so `diff` and `join`
    // both work without a pre-sort.
    if target.as_deref() == Some("--list") {
        assert!(suites.len() == 1, "--list takes one suite, not `all`");
        let suite = suites.first().expect("one suite");
        let mut rows: Vec<(String, String)> = cases
            .par_iter()
            .map(|case| {
                let judgement = suite.judge(case);
                let lines = judgement
                    .lines
                    .map_or_else(String::new, |tally| format!("{}/{}", tally.matched, tally.total));
                (
                    case.name.clone(),
                    format!(
                        "{}\t{}\t{}",
                        verdict(&judgement.outcome),
                        lines,
                        reason(&judgement.outcome)
                    ),
                )
            })
            .collect();
        rows.sort();

        let mut tally = [0usize; 4];
        for (name, rest) in &rows {
            println!("{name}\t{rest}");
            let index = match rest.split('\t').next() {
                Some("PASS") => 0,
                Some("FAIL") => 1,
                Some("UNSUP") => 2,
                _ => 3,
            };
            tally[index] += 1;
        }
        // The control. These must equal the committed snapshot's counts.
        eprintln!(
            "{}: passed {} failed {} unsupported {} skipped {}",
            suite.name(),
            tally[0],
            tally[1],
            tally[2],
            tally[3]
        );
        return;
    }

    let name = target.expect("a case name, or --list");
    let case = cases
        .iter()
        .find(|case| case.name == name)
        .unwrap_or_else(|| panic!("case {name:?} not found in the corpus"));

    for suite in &suites {
        let judgement = suite.judge(case);
        let lines = judgement.lines.map_or_else(String::new, |tally| {
            format!("  [{}/{} lines]", tally.matched, tally.total)
        });
        println!("{:<24} {}{lines}", suite.name(), verdict(&judgement.outcome));
        let reason = reason(&judgement.outcome);
        if !reason.is_empty() {
            println!("{:<24}   {reason}", "");
        }
    }
}
