//! `isolated_declarations`: the checker-free half of declaration emit.
//!
//! # What this suite measures, and what it does not
//!
//! [`tsr_dts`] reports where a declaration would have needed type inference. This
//! suite compares those reports — code *and* position — against the `TS9xxx`
//! diagnostics upstream recorded in its `.errors.txt` baselines.
//!
//! At the pin (`5b1047d10`) that oracle is **16 cases carrying 172 positioned
//! diagnostics across 20 distinct codes**, out of 22 cases that set
//! `@isolatedDeclarations: true`. The other six set the flag and produce no
//! `TS9xxx`, which is an assertion too — it is where a false positive shows up.
//!
//! It is deliberately *not* the `.d.ts` output oracle. Only six cases have
//! declaration output in their baselines, and none of it can be judged before a
//! printer exists (`bd tsr-49v.4`). Errors can be judged now, they carry positions,
//! and they measure exactly the half that is checker-free — which is why
//! [ADR-0021](../../../docs/adr/0021-isolated-declarations-is-not-a-port.md) makes
//! this the gate for the analysis slice.
//!
//! # Why the denominator is 22 and not 12,444
//!
//! `isolatedDeclarations` changes what a file is *allowed* to contain. Running the
//! analysis over a case that never opted in would compare our diagnostics against a
//! baseline produced without the flag, and every disagreement would be the harness
//! rather than the compiler. Cases that do not set the flag are therefore skipped
//! with that reason recorded, not silently dropped.
//!
//! Three further exclusions, each of which would otherwise be a free pass:
//!
//! - **Configuration-varied baselines.** A case whose diagnostics live under
//!   `case(target=es5).errors.txt` has no single expected output; see
//!   [`crate::corpus`].
//! - **Cases upstream never ran.** No baseline of any kind means the absence of
//!   `.errors.txt` proves nothing.
//! - **Known divergences.** A `.errors.txt.diff` says typescript-go and TypeScript
//!   already disagree here, so neither is a clean expectation.

use tsr_dts::analyze;

use crate::{
    CaseEntry,
    errors_baseline::{self, BaselineDiagnostic},
    suite::{Outcome, Suite},
    symbols_baseline::line_and_character,
};

/// The `isolated_declarations` suite.
pub struct IsolatedDeclarations;

/// The `TS9xxx` range: everything `isolatedDeclarations` reports, and nothing else.
///
/// A baseline may mix these with ordinary checker diagnostics — three of the
/// sixteen do, 12 of them in total — and those are Phase 4's problem. Comparing the
/// whole baseline would make this suite unpassable for reasons that have nothing to
/// do with what it measures.
const IS_ISOLATED_DECLARATIONS_CODE: std::ops::Range<u32> = 9000..9100;

impl Suite for IsolatedDeclarations {
    fn name(&self) -> &'static str {
        "isolated_declarations"
    }

    fn describes(&self) -> &'static str {
        "the TS9xxx diagnostics of a case that sets @isolatedDeclarations, \
         compared by code and position against upstream's .errors.txt"
    }

    fn run(&self, case: &CaseEntry) -> Outcome {
        let Ok(test) = case.load() else {
            return Outcome::Failed { reason: "case did not load".to_string() };
        };

        // `TestCase::parse` lowercases directive names, so the key is not the
        // camel-cased spelling the corpus writes.
        if test.options.get("isolateddeclarations").map(String::as_str) != Some("true") {
            return Outcome::Skipped { reason: "not an @isolatedDeclarations case".to_string() };
        }
        if !case.has_any_baseline() {
            return Outcome::Skipped { reason: "upstream recorded no baseline".to_string() };
        }
        if case.has_known_divergence() {
            return Outcome::Skipped {
                reason: "upstream records a known divergence from TypeScript".to_string(),
            };
        }
        if case.has_varied_errors() {
            return Outcome::Skipped { reason: "diagnostics vary by configuration".to_string() };
        }

        let Ok(baseline) = case.expected_errors() else {
            return Outcome::Failed { reason: "baseline did not load".to_string() };
        };
        let mut expected: Vec<BaselineDiagnostic> = baseline
            .as_deref()
            .map(errors_baseline::parse)
            .unwrap_or_default()
            .into_iter()
            .filter(|diagnostic| IS_ISOLATED_DECLARATIONS_CODE.contains(&diagnostic.code))
            .collect();
        expected.sort_unstable();

        let mut actual = Vec::new();
        for unit in &test.files {
            let parsed = tsr_parser::ParsedFile::parse(unit.content.clone());
            let reported = parsed.with_ast(|source_file| analyze(source_file, parsed.nodes()));
            for diagnostic in reported {
                let (line, character) = line_and_character(&unit.content, diagnostic.span.start);
                actual.push(BaselineDiagnostic {
                    file: unit.name.clone(),
                    line: line + 1,
                    column: character + 1,
                    code: diagnostic.message.code(),
                });
            }
        }
        actual.sort_unstable();

        if actual == expected {
            return Outcome::Passed;
        }
        Outcome::Failed { reason: difference(&expected, &actual) }
    }
}

/// Describe the disagreement in a form that names the first few concrete sites.
///
/// A bare count ("3 missing, 1 extra") is enough to ratchet on and useless to debug
/// with, and these snapshots are read as diffs by whoever broke them.
fn difference(expected: &[BaselineDiagnostic], actual: &[BaselineDiagnostic]) -> String {
    let missing: Vec<_> = expected.iter().filter(|d| !actual.contains(d)).collect();
    let extra: Vec<_> = actual.iter().filter(|d| !expected.contains(d)).collect();

    let mut parts = Vec::new();
    if !missing.is_empty() {
        parts.push(format!("{} missing [{}]", missing.len(), sample(&missing)));
    }
    if !extra.is_empty() {
        parts.push(format!("{} unexpected [{}]", extra.len(), sample(&extra)));
    }
    format!(
        "{} of {} expected: {}",
        expected.len() - missing.len(),
        expected.len(),
        parts.join(", ")
    )
}

/// The first three sites, so one long list does not dominate a snapshot.
fn sample(diagnostics: &[&BaselineDiagnostic]) -> String {
    let shown: Vec<String> = diagnostics.iter().take(3).map(ToString::to_string).collect();
    let mut text = shown.join(" ");
    if diagnostics.len() > 3 {
        use std::fmt::Write as _;
        let _ = write!(text, " +{}", diagnostics.len() - 3);
    }
    text
}
