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

/// How many of a case's individual assertion lines matched.
///
/// A *gradient*, reported alongside the case verdict and never in place of it.
/// See [`Judgement`] for why this is not an [`Outcome`] variant.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LineTally {
    /// Assertion lines that matched the baseline.
    pub matched: usize,
    /// Assertion lines the baseline carries. The denominator.
    pub total: usize,
}

/// A case verdict, plus an optional finer-grained measure.
///
/// # Why the tally is not an `Outcome` variant
///
/// [`Outcome`] is deliberately binary — a case passes only if *every* assertion
/// of *every* file matches — and that stays the gate. But a binary gate over a
/// component as large as the checker gives one bit of feedback per case and
/// reads 0% for months, which PLAN.md §4 records and rejects. `.types` cases
/// carry 594,122 assertion lines between them, and that is a gradient worth
/// steering by.
///
/// Two shapes were considered:
///
/// - **A new `Outcome` variant** carrying the tally. Rejected because the tally
///   has to attach to [`Outcome::Unsupported`] too — a case the checker cannot
///   run still has a denominator, and dropping it there would zero the
///   denominator *precisely while the number is 0/N*, which is when the gradient
///   is the only signal there is. A variant that only some verdicts can carry
///   cannot express that.
/// - **A second `Suite`.** Rejected because a `Suite` yields one `Outcome` per
///   case and cannot express a ratio at all; "cases where every line matched" is
///   the gate again under another name.
///
/// So the tally rides *alongside* the verdict, orthogonal to it. The cost is one
/// defaulted trait method; no existing suite changes, and a snapshot gains a line
/// only if its suite reports tallies.
#[derive(Debug, Clone)]
pub struct Judgement {
    /// The case verdict. This is the gate.
    pub outcome: Outcome,
    /// The sub-case measure, for suites that have one.
    pub lines: Option<LineTally>,
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

    /// Judge one case, reporting a per-assertion-line tally as well as the
    /// verdict.
    ///
    /// Defaults to [`Suite::run`] with no tally, which is what every suite whose
    /// baseline is not a list of positioned assertions wants. A suite that *does*
    /// have a gradient overrides this and implements `run` in terms of it, so the
    /// work happens once.
    fn judge(&self, case: &CaseEntry) -> Judgement {
        Judgement { outcome: self.run(case), lines: None }
    }

    /// Whether this suite judges each named configuration of a varied case
    /// ([`crate::Corpus::configured`]) rather than the cases as discovered.
    ///
    /// A separate population, so a suite's existing row keeps its
    /// denominator and the per-configuration one is reported beside it
    /// (ADR-0047).
    fn per_configuration(&self) -> bool {
        false
    }
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
    /// Assertion lines matched over assertion lines judged, summed across cases.
    ///
    /// Zero for a suite that reports no gradient, which is how the snapshot knows
    /// not to print the row. **Never** replaces [`SuiteResult::percentage`]: a
    /// case passes only if all of its lines match, so this number is always the
    /// more forgiving of the two and reading it as a pass rate would overstate
    /// the port.
    pub lines: LineTally,
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

    /// Share of assertion lines matched, as a percentage, or `None` for a suite
    /// that reports no gradient.
    ///
    /// This is not a pass rate and must never be presented as one; see
    /// [`SuiteResult::lines`].
    #[must_use]
    pub fn line_percentage(&self) -> Option<f64> {
        if self.lines.total == 0 {
            return None;
        }
        #[allow(clippy::cast_precision_loss)]
        Some(self.lines.matched as f64 / self.lines.total as f64 * 100.0)
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
    let judgements: Vec<Judgement> = cases.par_iter().map(|case| suite.judge(case)).collect();

    let mut result = SuiteResult {
        name: suite.name().to_string(),
        describes: suite.describes().to_string(),
        ..SuiteResult::default()
    };
    let mut reasons: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();

    for (case, judgement) in cases.iter().zip(judgements) {
        if let Some(lines) = judgement.lines {
            result.lines.matched += lines.matched;
            result.lines.total += lines.total;
        }
        match judgement.outcome {
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
