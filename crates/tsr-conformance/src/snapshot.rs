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
    }
}
