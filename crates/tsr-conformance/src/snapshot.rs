//! The committed snapshot format.
//!
//! Modelled on oxc's `tasks/coverage/snapshots/*.snap`. Snapshots are **checked
//! in**, which is the entire point: a regression then shows up as a reviewable
//! diff in the pull request that caused it, rather than as a number someone has to
//! remember. This is the ratchet described in PLAN.md §6.
//!
//! ```text
//! commit: 5b1047d10d32e7d5b446be4de56b126ff42f82bb
//!
//! parser_typescript Summary:
//! What a pass means: source parses and produces the expected parse diagnostics
//! Passed         : 0/12445 (0.00%)
//! Unsupported    : 12445
//!   12445  no parser implemented (bd tsr-pum)
//! ```

use std::fmt::Write as _;

#[cfg(test)]
use crate::suite::LineTally;
use crate::suite::SuiteResult;

/// Render a snapshot for one suite.
///
/// `upstream_commit` pins what the numbers were measured against; without it a
/// committed percentage is not reproducible.
#[must_use]
pub fn render(result: &SuiteResult, upstream_commit: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "commit: {upstream_commit}\n");
    let _ = writeln!(out, "{} Summary:", result.name);
    let _ = writeln!(out, "What a pass means: {}", result.describes);
    let _ = writeln!(
        out,
        "Passed         : {}/{} ({:.2}%)",
        result.passed,
        result.total(),
        result.percentage()
    );

    // The gradient, when the suite has one. Printed *under* the pass rate and
    // never instead of it: a case passes only if all of its lines match, so this
    // is always the more forgiving number. The label says "not a pass rate"
    // in the snapshot itself, because a snapshot gets read out of context.
    if let Some(rate) = result.line_percentage() {
        let _ = writeln!(
            out,
            "Assertion lines: {}/{} ({rate:.2}%)  — a gradient, not a pass rate",
            result.lines.matched, result.lines.total
        );
    }

    if result.failed > 0 {
        let _ = writeln!(out, "Failed         : {}", result.failed);
    }
    if result.unsupported > 0 {
        let _ = writeln!(out, "Unsupported    : {}", result.unsupported);
    }
    if result.skipped > 0 {
        let _ =
            writeln!(out, "Skipped        : {} (excluded from the denominator)", result.skipped);
    }

    if !result.unsupported_reasons.is_empty() {
        out.push('\n');
        for (reason, count) in &result.unsupported_reasons {
            let _ = writeln!(out, "  {count}  {reason}");
        }
    }

    if !result.failures.is_empty() {
        out.push('\n');
        let shown = result.failures.len();
        if shown < result.failed {
            let _ = writeln!(out, "First {shown} of {} failures:", result.failed);
        }
        for (name, reason) in &result.failures {
            let _ = writeln!(out, "{name}");
            for line in reason.lines() {
                let _ = writeln!(out, "  {line}");
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_percent_is_rendered_explicitly_not_omitted() {
        let result = SuiteResult {
            name: "parser_typescript".into(),
            describes: "source parses".into(),
            unsupported: 10,
            unsupported_reasons: vec![("no parser implemented".into(), 10)],
            ..SuiteResult::default()
        };
        let rendered = render(&result, "abc1234");
        assert!(rendered.contains("commit: abc1234"));
        assert!(rendered.contains("Passed         : 0/10 (0.00%)"));
        assert!(rendered.contains("10  no parser implemented"));
    }

    #[test]
    fn skips_are_reported_and_excluded_from_the_denominator() {
        let result = SuiteResult {
            name: "s".into(),
            describes: "d".into(),
            passed: 3,
            skipped: 7,
            ..SuiteResult::default()
        };
        assert_eq!(result.total(), 3);
        let rendered = render(&result, "c");
        assert!(rendered.contains("Passed         : 3/3 (100.00%)"));
        assert!(rendered.contains("Skipped        : 7"));
    }

    #[test]
    fn percentage_of_an_empty_suite_does_not_divide_by_zero() {
        let result = SuiteResult::default();
        assert!((result.percentage() - 0.0).abs() < f64::EPSILON);
        assert!(result.line_percentage().is_none());
    }

    #[test]
    fn a_suite_with_no_gradient_prints_no_assertion_line_row() {
        let result = SuiteResult {
            name: "s".into(),
            describes: "d".into(),
            passed: 1,
            ..SuiteResult::default()
        };
        assert!(!render(&result, "c").contains("Assertion lines"));
    }

    #[test]
    fn the_gradient_prints_under_the_pass_rate_and_is_labelled_as_not_one() {
        // 0 cases passed, yet 3 of 4 assertion lines matched. Both numbers have
        // to appear, and the more forgiving one has to say what it is: the
        // failure mode this exists to avoid is a reader quoting 75%.
        let result = SuiteResult {
            name: "checker_types".into(),
            describes: "d".into(),
            failed: 1,
            lines: LineTally { matched: 3, total: 4 },
            ..SuiteResult::default()
        };
        let rendered = render(&result, "c");
        assert!(rendered.contains("Passed         : 0/1 (0.00%)"), "{rendered}");
        assert!(
            rendered.contains("Assertion lines: 3/4 (75.00%)  — a gradient, not a pass rate"),
            "{rendered}"
        );
        let passed_at = rendered.find("Passed  ").expect("pass rate row");
        let lines_at = rendered.find("Assertion lines").expect("gradient row");
        assert!(passed_at < lines_at, "the gate prints first");
    }
}
