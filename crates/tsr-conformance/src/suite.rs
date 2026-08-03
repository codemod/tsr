//! Suites, outcomes, and the run loop.
//!
//! A *suite* judges every case in the corpus and produces a pass rate. The point
//! of the abstraction is that a stage which does not exist yet still reports a
//! number — `0/12445` — rather than being absent from the summary. A missing row
//! and a zero row look identical at a glance, and only one of them is honest.

use rayon::prelude::*;

use crate::corpus::CaseEntry;

/// What happened when a suite judged one case.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// Matched the expectation.
    Passed,
    /// Ran and disagreed with the expectation.
    Failed {
        /// Human-readable disagreement, shown in the snapshot.
        reason: String,
    },
    /// The stage under test does not exist yet, or cannot handle this case.
    ///
    /// Counted as *not passed*. It exists as a separate variant so the snapshot
    /// can distinguish "we ran and were wrong" from "we cannot run", which are
    /// very different signals when reading a 0%.
    Unsupported {
        /// Why, e.g. `no parser implemented (bd tsr-pum)`.
        reason: String,
    },
    /// Excluded from the denominator, with a reason that gets reported.
    ///
    /// Use sparingly. Silent exclusions make a pass rate meaningless; the snapshot
    /// prints the skip count precisely so nobody can quietly shrink the corpus.
    Skipped {
        /// Why this case is not judged.
        reason: String,
    },
}

/// A judgeable stage of the compiler.
pub trait Suite {
    /// Snapshot name, e.g. `parser_typescript`.
    fn name(&self) -> &'static str;

    /// One line describing what a pass means. Written into the snapshot header so
    /// a reader knows what the percentage is actually claiming.
    fn describes(&self) -> &'static str;

    /// Judge one case.
    fn run(&self, case: &CaseEntry) -> Outcome;
}

/// Tallied results for one suite.
#[derive(Debug, Default)]
pub struct SuiteResult {
    /// Suite name.
    pub name: String,
    /// What a pass means.
    pub describes: String,
    /// Cases that passed.
    pub passed: usize,
    /// Cases that ran and disagreed.
    pub failed: usize,
    /// Cases the stage could not handle.
    pub unsupported: usize,
    /// Cases excluded from the denominator.
    pub skipped: usize,
    /// Failure details, truncated for the snapshot.
    pub failures: Vec<(String, String)>,
    /// The distinct reasons cases were unsupported, with counts.
    pub unsupported_reasons: Vec<(String, usize)>,
}

impl SuiteResult {
    /// Cases judged: everything except skips.
    #[must_use]
    pub fn total(&self) -> usize {
        self.passed + self.failed + self.unsupported
    }

    /// Pass rate over judged cases, as a percentage.
    #[must_use]
    pub fn percentage(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        {
            self.passed as f64 / total as f64 * 100.0
        }
    }
}

/// Maximum failure details recorded in a snapshot.
///
/// Snapshots are committed and reviewed as diffs; an unbounded list would make a
/// one-case regression unreadable. The count is always reported in full, so
/// truncation never hides the magnitude — only the detail.
const MAX_REPORTED_FAILURES: usize = 100;

/// Run a suite over the corpus, in parallel.
///
/// Cases are independent — each parses into its own arena — so this is a plain
/// `par_iter`. Results are collected in case order first and tallied afterwards,
/// so the snapshot is byte-identical regardless of how work was scheduled;
/// a run whose output depends on thread timing would be useless as a ratchet.
#[must_use]
pub fn run_suite(suite: &(dyn Suite + Sync), cases: &[CaseEntry]) -> SuiteResult {
    let outcomes: Vec<Outcome> = cases.par_iter().map(|case| suite.run(case)).collect();

    let mut result = SuiteResult {
        name: suite.name().to_string(),
        describes: suite.describes().to_string(),
        ..SuiteResult::default()
    };
    let mut reasons: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();

    for (case, outcome) in cases.iter().zip(outcomes) {
        match outcome {
            Outcome::Passed => result.passed += 1,
            Outcome::Failed { reason } => {
                result.failed += 1;
                if result.failures.len() < MAX_REPORTED_FAILURES {
                    result.failures.push((case.name.clone(), reason));
                }
            }
            Outcome::Unsupported { reason } => {
                result.unsupported += 1;
                *reasons.entry(reason).or_default() += 1;
            }
            Outcome::Skipped { reason } => {
                result.skipped += 1;
                *reasons.entry(format!("skipped: {reason}")).or_default() += 1;
            }
        }
    }

    result.unsupported_reasons = reasons.into_iter().collect();
    result
}
