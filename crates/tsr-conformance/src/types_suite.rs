//! `checker_types`: the type of every expression, against upstream's `.types`.
//!
//! # This suite reports 0%, on purpose
//!
//! There is no checker (`bd tsr-4sc`), so every case it can judge comes back
//! [`Outcome::Unsupported`]. [conventions.md](../../../docs/conventions.md) is
//! explicit about why that is the right shape rather than a skip:
//!
//! > A conformance suite whose subject does not exist reports **0%**, loudly, with
//! > the reason stated. It does not skip, and it does not omit the row. A missing
//! > number and a zero look identical in a summary table, and only one of them is
//! > honest.
//!
//! # What it is for before it can pass
//!
//! The denominator. `internal/checker` is 60,269 lines — a third of the core port
//! and, by PLAN.md's estimate, 120–200 sessions. Starting that without a gate
//! means months before the first honest number, and the lesson this repository
//! keeps relearning is that the *instrument* is what needs building first:
//! `dts_reachable_target` sized the emitter's target before the emitter existed,
//! and ADR-0021 committed to a falsifier that could fire on the measurement alone.
//!
//! So this row appears in the summary from today, reading `0/N`, and the number
//! moves the first time the checker computes a type.
//!
//! # What a pass will mean
//!
//! Every `>expression : type` line of the baseline reproduced **verbatim, in
//! order, for every file of the case**. Whole-line rather than
//! expression-and-type, because upstream writes the `" : "` separator without
//! escaping either side and both halves can contain it — see
//! [`crate::types_baseline`].
//!
//! # The exclusions, and why each is not a free pass
//!
//! - **Configuration-varied baselines** (2,032 of 12,155) are skipped, the same
//!   way every other suite skips them, until `bd tsr-bb4.1` runs per configuration.
//!   A `case(target=es5).types` records one of several compilations and matching
//!   it against a single default run is not a comparison.
//! - **Known divergences** (`.types.diff`) are skipped: upstream itself records
//!   that its output differs from TypeScript's there, so the baseline is not a
//!   specification.
//!
//! Both counts print, so the shrinkage stays visible rather than implied.

use crate::{
    CaseEntry,
    suite::{Outcome, Suite},
    types_baseline,
};

/// The `checker_types` suite.
pub struct CheckerTypes;

impl Suite for CheckerTypes {
    fn name(&self) -> &'static str {
        "checker_types"
    }

    fn describes(&self) -> &'static str {
        "the type of every expression matches upstream's .types baseline, line for \
         line — 0% until a checker exists (bd tsr-4sc); the denominator is what \
         this measures today"
    }

    fn run(&self, case: &CaseEntry) -> Outcome {
        // **Varied first.** A configuration-varied case has no *plain* `.types`
        // file, only `case(target=es5).types`, so asking for the plain one first
        // sends all 2,032 of them into the "no baseline" bucket — the right
        // outcome under a reason that says something else, which is how a skip
        // count stops meaning anything.
        if case.has_varied_types() {
            return Outcome::Skipped {
                reason: "configuration-varied baseline (bd tsr-bb4.1)".into(),
            };
        }
        if case.has_known_divergence() {
            return Outcome::Skipped {
                reason: "upstream records a known divergence from TypeScript".into(),
            };
        }
        let Some(text) = case.expected_types() else {
            return Outcome::Skipped { reason: "upstream recorded no .types baseline".into() };
        };

        let files = types_baseline::parse(&text);
        let assertions = types_baseline::assertion_count(&files);
        if assertions == 0 {
            // A `.types` baseline with no assertion asserts nothing, and counting
            // it would be a free pass the day the checker lands.
            return Outcome::Skipped { reason: "the .types baseline has no assertions".into() };
        }

        Outcome::Unsupported { reason: "no checker (bd tsr-4sc)".into() }
    }
}
