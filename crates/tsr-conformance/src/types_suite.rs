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
//! # Two numbers, and which one is the gate
//!
//! The **case rate** is the gate: a case passes only if every assertion of every
//! file matches. That is binary, and over a component this size it stays at 0%
//! for months while giving one bit of feedback per case.
//!
//! So [`CheckerTypes::judge`] also reports a **per-assertion-line tally** — the
//! gradient. It prints beside the case rate and never in place of it; it is
//! always the more forgiving of the two, because a case passes only when all of
//! its lines do. See [`crate::suite::Judgement`] for why this rides alongside the
//! verdict rather than inside it.
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
//! - **Configuration-varied baselines** (2,032 of 12,155) are skipped by this
//!   row: a `case(target=es2015).types` records one of several compilations,
//!   and matching it against a single default run is not a comparison.
//!   [`CheckerTypesConfigured`] judges each of those compilations against its
//!   own suffixed baseline (ADR-0047).
//! - **Known divergences** (`.types.diff`) are skipped: upstream itself records
//!   that its output differs from TypeScript's there, so the baseline is not a
//!   specification.
//!
//! Both counts print, so the shrinkage stays visible rather than implied.

use crate::{
    CaseEntry,
    suite::{Judgement, LineTally, Outcome, Suite},
    types_baseline::{self, FileTypes},
    types_producer,
};

/// The result of comparing what we produced against a `.types` baseline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comparison {
    /// How many assertion lines matched, of how many the baseline carries.
    pub lines: LineTally,
    /// The first disagreement, or `None` if the case matches exactly.
    ///
    /// **This is the gate, not [`Comparison::lines`].** A tally can read full
    /// while the case is still wrong — produce every expected line and one extra
    /// and `matched == total` — so the verdict is taken from here.
    pub mismatch: Option<String>,
}

/// Compare produced type assertions against a `.types` baseline.
///
/// Positional and whole-line, which is what the baseline means: upstream writes
/// one `>expression : type` per position, in source order, per file. So the
/// comparison is by index — file *i* against file *i*, assertion *j* against
/// assertion *j* — and equality is string equality on the whole line, because
/// the `" : "` separator is unescaped and appears inside expressions (see
/// [`crate::types_baseline`]).
///
/// Positional rather than set-based on purpose. Upstream's baseline is ordered
/// output; comparing it as a multiset would accept a checker that computed every
/// right type and attached them to the wrong expressions, which is a real defect
/// class and an easy one to write.
///
/// The tally counts a line as matched only where both the *position* and the
/// *text* agree, so a missing assertion in the middle of a file costs every line
/// after it. That makes the gradient a strict lower bound rather than a
/// best-alignment estimate, which is the direction to be wrong in: it can
/// understate progress, never overstate it.
#[must_use]
pub fn compare(expected: &[FileTypes], ours: &[FileTypes]) -> Comparison {
    let total = types_baseline::assertion_count(expected);
    let mut matched = 0usize;
    let mut mismatch: Option<String> = None;

    // Recorded rather than returned immediately, so the tally is complete even
    // for a case that disagreed on its first line. The gradient exists precisely
    // to say something about cases that fail.
    let mut note = |reason: String| {
        if mismatch.is_none() {
            mismatch = Some(reason);
        }
    };

    if ours.len() != expected.len() {
        note(format!("{} file section(s), expected {}", ours.len(), expected.len()));
    }

    for (index, expected_file) in expected.iter().enumerate() {
        let Some(our_file) = ours.get(index) else {
            note(format!("no output for {}", expected_file.file));
            continue;
        };
        if our_file.file != expected_file.file {
            note(format!("file {index} is {}, expected {}", our_file.file, expected_file.file));
        }
        if our_file.assertions.len() != expected_file.assertions.len() {
            note(format!(
                "{}: {} assertion(s), expected {}",
                expected_file.file,
                our_file.assertions.len(),
                expected_file.assertions.len()
            ));
        }
        for (position, expected_assertion) in expected_file.assertions.iter().enumerate() {
            match our_file.assertions.get(position) {
                Some(ours) if ours.text == expected_assertion.text => matched += 1,
                Some(ours) => note(format!(
                    "{}: >{} — expected >{}",
                    expected_file.file, ours.text, expected_assertion.text
                )),
                None => note(format!(
                    "{}: nothing at position {position}, expected >{}",
                    expected_file.file, expected_assertion.text
                )),
            }
        }
    }

    Comparison { lines: LineTally { matched, total }, mismatch }
}

/// The `checker_types` suite.
pub struct CheckerTypes;

impl Suite for CheckerTypes {
    fn name(&self) -> &'static str {
        "checker_types"
    }

    fn describes(&self) -> &'static str {
        "the type of every expression matches upstream's .types baseline, line for \
         line, for every file of the case. The `lines` gradient beside it is the \
         share of individual assertions that match; the walker feeding both agrees \
         with upstream on 97.85% of assertion TEXT, so the residual is the checker \
         rather than the walk"
    }

    fn run(&self, case: &CaseEntry) -> Outcome {
        self.judge(case).outcome
    }

    fn judge(&self, case: &CaseEntry) -> Judgement {
        // A skipped case carries no tally: it is out of the denominator for the
        // gradient exactly as it is for the gate, so the two numbers are always
        // taken over the same population.
        let skip = |reason: &str| Judgement {
            outcome: Outcome::Skipped { reason: reason.into() },
            lines: None,
        };

        // **Varied first.** A configuration-varied case has no *plain* `.types`
        // file, only `case(target=es5).types`, so asking for the plain one first
        // sends all 2,032 of them into the "no baseline" bucket — the right
        // outcome under a reason that says something else, which is how a skip
        // count stops meaning anything.
        if case.configuration.is_none() && case.has_varied_types() {
            return skip(
                "configuration-varied baseline, judged per configuration by \
                 checker_types_configured",
            );
        }
        if case.has_known_divergence() {
            return skip("upstream records a known divergence from TypeScript");
        }
        let Some(text) = case.expected_types() else {
            if case.configuration.is_some() && !case.has_any_baseline() {
                return skip(&crate::diagnostics_suite::no_output_reason(case));
            }
            return skip("upstream recorded no .types baseline");
        };

        let files = types_baseline::parse(&text);
        let assertions = types_baseline::assertion_count(&files);
        if assertions == 0 {
            // A `.types` baseline with no assertion asserts nothing, and counting
            // it would be a free pass the day the checker lands.
            return skip("the .types baseline has no assertions");
        }

        let Ok(parsed) = case.load() else {
            return Judgement {
                outcome: Outcome::Failed { reason: "case did not load".into() },
                lines: Some(LineTally { matched: 0, total: assertions }),
            };
        };
        // Re-read with each section's source, so a code line the writer
        // echoed with a leading `>` is not an assertion
        // (`types_producer::expected_for_case`, `docs/parity/notes/r5-align.md`
        // §2.3). It can only drop lines, so the skip is re-tested.
        let files = types_producer::expected_for_case(&text, &parsed);
        if types_baseline::assertion_count(&files) == 0 {
            return skip("the .types baseline has no assertions");
        }

        // One rendered section per baseline section, in the baseline's order, so
        // position `i` on one side is position `i` on the other.
        let ours: Vec<FileTypes> = types_producer::assertions_for_case(&parsed, &files, false)
            .iter()
            .zip(&files)
            .map(|(rendered, expected_file)| {
                types_producer::to_file_types(&expected_file.file, rendered)
            })
            .collect();

        let comparison = compare(&files, &ours);
        Judgement {
            outcome: match comparison.mismatch {
                None => Outcome::Passed,
                Some(reason) => Outcome::Failed { reason },
            },
            lines: Some(comparison.lines),
        }
    }
}

/// `checker_types`, over each named configuration of a case whose directives
/// vary ([`crate::Corpus::configured`]), against that configuration's own
/// `case(<configuration>).types`.
///
/// [`CheckerTypes::judge`] unchanged; a row of its own so the plain row's
/// denominator does not move (ADR-0047).
pub struct CheckerTypesConfigured;

impl Suite for CheckerTypesConfigured {
    fn name(&self) -> &'static str {
        "checker_types_configured"
    }

    fn describes(&self) -> &'static str {
        "the checker_types judgement for each configuration upstream's runner compiles \
         a configuration-varied case under (`// @target: es2015, esnext`), against that \
         configuration's suffixed .types baseline"
    }

    fn run(&self, case: &CaseEntry) -> Outcome {
        self.judge(case).outcome
    }

    fn judge(&self, case: &CaseEntry) -> Judgement {
        if case.configuration.is_none() {
            return Judgement {
                outcome: Outcome::Skipped { reason: "not a named configuration".into() },
                lines: None,
            };
        }
        CheckerTypes.judge(case)
    }

    fn per_configuration(&self) -> bool {
        true
    }
}

/// Proving the oracle before anything is measured with it.
///
/// This suite had reported 0% since the day it was added, which means its
/// comparison had executed **zero times**. A suite that has only ever read 0% is
/// exactly as unproven as one reading 100% on its first run — the failure
/// `file_loader` was caught by, where 76/76 held until four deliberate mutations
/// were applied to the code under test.
///
/// So each test below feeds the judge output that is wrong in one specific way
/// and asserts it goes red. Between them they cover every way a positional
/// whole-line comparison can be weakened: comparing text but not count, count but
/// not text, either but not order, and any of them in only the first file of a
/// multi-file case.
///
/// These were checked against a weakened judge rather than assumed to bite; the
/// six mutations and what each turned red are recorded in
/// `docs/architecture/checker-oracle.md`.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::types_baseline::TypeAssertion;

    /// `("a.ts", ["x : string", ...])` in the shape [`compare`] takes.
    fn files(sections: &[(&str, &[&str])]) -> Vec<FileTypes> {
        sections
            .iter()
            .map(|(file, assertions)| FileTypes {
                file: (*file).to_string(),
                assertions: assertions
                    .iter()
                    .map(|text| TypeAssertion { text: (*text).to_string() })
                    .collect(),
            })
            .collect()
    }

    const BASELINE: &[(&str, &[&str])] =
        &[("a.ts", &["x : string", "\"a\" : \"a\"", "y : number"])];

    #[test]
    fn identical_output_passes_and_the_tally_is_full() {
        let expected = files(BASELINE);
        let result = compare(&expected, &files(BASELINE));
        assert_eq!(result.mismatch, None);
        assert_eq!(result.lines, LineTally { matched: 3, total: 3 });
    }

    #[test]
    fn a_wrong_type_with_the_right_count_fails() {
        // The commonest real failure and the one a count-only comparison would
        // miss entirely: three lines produced, three expected, one type wrong.
        let ours = files(&[("a.ts", &["x : number", "\"a\" : \"a\"", "y : number"])]);
        let result = compare(&files(BASELINE), &ours);
        assert!(result.mismatch.is_some(), "a wrong type must fail the case");
        assert!(result.mismatch.unwrap().contains("x : number"));
        assert_eq!(result.lines.matched, 2, "only the wrong line stops counting");
    }

    #[test]
    fn the_right_types_in_the_wrong_order_fail() {
        // Every expected line is present; two are at each other's positions. A
        // set- or multiset-based comparison passes this, and it is a defect
        // class — the right types attached to the wrong expressions.
        let ours = files(&[("a.ts", &["\"a\" : \"a\"", "x : string", "y : number"])]);
        let result = compare(&files(BASELINE), &ours);
        assert!(result.mismatch.is_some(), "order is part of the baseline");
        assert_eq!(result.lines.matched, 1, "only `y : number` is still in place");
    }

    #[test]
    fn a_missing_assertion_fails() {
        let ours = files(&[("a.ts", &["x : string", "\"a\" : \"a\""])]);
        let result = compare(&files(BASELINE), &ours);
        let reason = result.mismatch.expect("a short file must fail the case");
        assert!(reason.contains("2 assertion(s), expected 3"), "{reason}");
        assert_eq!(result.lines, LineTally { matched: 2, total: 3 });
    }

    #[test]
    fn an_extra_assertion_fails_even_though_every_expected_line_matched() {
        // The case the tally cannot catch, which is why the gate is `mismatch`
        // and not `matched == total`. Inventing a type for an expression upstream
        // does not type is as wrong as missing one.
        let ours = files(&[("a.ts", &["x : string", "\"a\" : \"a\"", "y : number", "z : any"])]);
        let result = compare(&files(BASELINE), &ours);
        assert_eq!(
            result.lines,
            LineTally { matched: 3, total: 3 },
            "the tally reads full — this is exactly the blind spot"
        );
        let reason = result.mismatch.expect("an extra assertion must still fail the case");
        assert!(reason.contains("4 assertion(s), expected 3"), "{reason}");
    }

    #[test]
    fn a_multi_file_case_fails_when_only_its_second_file_is_wrong() {
        // A comparison that stopped at the first file, or that only checked file
        // count, would pass this.
        let expected = files(&[("a.ts", &["x : string"]), ("b.ts", &["y : number"])]);
        let ours = files(&[("a.ts", &["x : string"]), ("b.ts", &["y : string"])]);
        let result = compare(&expected, &ours);
        let reason = result.mismatch.expect("a wrong second file must fail the case");
        assert!(reason.contains("b.ts"), "the failure has to name the file: {reason}");
        assert_eq!(result.lines, LineTally { matched: 1, total: 2 });
    }

    #[test]
    fn output_for_the_wrong_file_fails_even_with_matching_assertions() {
        // Same assertions, wrong section name. Comparing assertions without
        // comparing headers would accept this.
        let expected = files(&[("a.ts", &["x : string"])]);
        let ours = files(&[("b.ts", &["x : string"])]);
        assert!(compare(&expected, &ours).mismatch.is_some());
    }

    #[test]
    fn producing_nothing_fails_and_the_denominator_survives() {
        // The state the suite is in today, as a comparison rather than as an
        // assumption: no output at all is 0 of N, not 0 of 0. A gradient whose
        // denominator collapsed when the producer was absent would read 100%.
        let result = compare(&files(BASELINE), &[]);
        assert!(result.mismatch.is_some());
        assert_eq!(result.lines, LineTally { matched: 0, total: 3 });
    }
}
