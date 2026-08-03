//! The suites we can run today.
//!
//! Two of them measure the harness itself; one measures the parser and reports 0%
//! because there is no parser. That zero is the number this whole crate exists to
//! move.

use crate::{
    corpus::CaseEntry,
    suite::{Outcome, Suite},
};

/// Can every case be read and split into units?
///
/// This measures the **harness**, not the compiler. It is worth a suite of its own
/// because the case parser mirrors upstream's directive rules by hand
/// (`crates/tsr-conformance/src/case.rs`), and a bug there silently misattributes
/// source to the wrong file — corrupting every downstream comparison in a way that
/// would look like a compiler bug.
///
/// Expected to sit at 100%. If it drops, fix the harness before trusting any other
/// number in the snapshot.
pub struct CorpusIngest;

impl Suite for CorpusIngest {
    fn name(&self) -> &'static str {
        "corpus_ingest"
    }

    fn describes(&self) -> &'static str {
        "the case file is readable and splits into at least one named unit"
    }

    fn run(&self, case: &CaseEntry) -> Outcome {
        match case.load() {
            Ok(parsed) => {
                if parsed.files.is_empty() {
                    return Outcome::Failed { reason: "parsed into zero units".into() };
                }
                if let Some(unnamed) = parsed.files.iter().position(|f| f.name.is_empty()) {
                    return Outcome::Failed { reason: format!("unit {unnamed} has an empty name") };
                }
                if let Some(err) = parsed.error {
                    return Outcome::Failed { reason: err };
                }
                Outcome::Passed
            }
            Err(err) => Outcome::Failed { reason: format!("{err:#}") },
        }
    }
}

/// Can every case's expected-diagnostics baseline be resolved?
///
/// Also harness-measuring. A missing `.errors.txt` is a *positive* expectation —
/// "this case produces no diagnostics" — so this suite passes either way and
/// exists to prove the case-to-baseline path mapping is right. If the mapping were
/// wrong, every case would resolve to "no errors expected" and a parser emitting
/// nothing would score 100%.
pub struct BaselineResolution;

impl Suite for BaselineResolution {
    fn name(&self) -> &'static str {
        "baseline_resolution"
    }

    fn describes(&self) -> &'static str {
        "the case maps to a baseline directory that exists on disk"
    }

    fn run(&self, case: &CaseEntry) -> Outcome {
        if !case.baselines.is_present() {
            return Outcome::Failed {
                reason: format!("baseline dir missing: {}", case.baselines.dir().display()),
            };
        }
        match case.expected_errors() {
            Ok(_) => Outcome::Passed,
            Err(err) => Outcome::Failed { reason: format!("{err:#}") },
        }
    }
}

/// Does a file TypeScript accepts parse without complaint?
///
/// Restricted to cases with no `.errors.txt`, for the reason given on
/// [`crate::scanner_suite::ScannerCleanFiles`]: for those the expected diagnostic
/// output is exactly nothing, so any parse error is unambiguously our bug.
///
/// Cases that *do* expect errors are skipped. `.errors.txt` mixes syntactic and
/// semantic diagnostics with no marker distinguishing them, and most of its
/// contents belong to the checker, so matching against it is not yet meaningful.
pub struct Parser;

impl Suite for Parser {
    fn name(&self) -> &'static str {
        "parser_typescript"
    }

    fn describes(&self) -> &'static str {
        "a file TypeScript reports no errors for parses with no diagnostics"
    }

    fn run(&self, case: &CaseEntry) -> Outcome {
        if !case.has_any_baseline() {
            return Outcome::Skipped {
                reason: "upstream recorded no output for this case, so there is nothing to judge \
                         against"
                    .into(),
            };
        }
        if case.has_known_divergence() {
            return Outcome::Skipped {
                reason: "upstream records a known divergence from TypeScript (.diff baseline)"
                    .into(),
            };
        }
        if case.has_varied_errors() {
            return Outcome::Skipped {
                reason: "configuration-varied baselines; needs per-configuration runs".into(),
            };
        }
        match case.expected_errors() {
            Err(err) => return Outcome::Failed { reason: format!("{err:#}") },
            Ok(Some(_)) => {
                return Outcome::Skipped {
                    reason: "expects diagnostics; the parser cannot tell which are its own".into(),
                };
            }
            Ok(None) => {}
        }

        let parsed = match case.load() {
            Ok(parsed) => parsed,
            Err(err) => return Outcome::Failed { reason: format!("{err:#}") },
        };

        for file in &parsed.files {
            if !crate::scanner_suite::is_typescript_unit(&file.name) {
                continue;
            }
            // Encoding is the file loader's job, not the parser's.
            if file.content.contains('\u{FFFD}') {
                continue;
            }
            let arena = tsr_core::Arena::new();
            // `.tsx` reads `<` as JSX; `.ts` reads it as a type assertion.
            let script_kind = tsr_parser::ScriptKind::from_file_name(&file.name);
            let result = tsr_parser::parse_with_script_kind(&arena, &file.content, script_kind);
            if let Some(first) = result.diagnostics.first() {
                return Outcome::Failed {
                    reason: format!(
                        "{}: {} diagnostic(s), first is TS{} at {:?}: {}",
                        file.name,
                        result.diagnostics.len(),
                        first.message.code(),
                        first.span,
                        first.text()
                    ),
                };
            }
        }
        Outcome::Passed
    }
}

/// How many cases are judgeable by a parser today, ignoring the checker?
///
/// Reports the size of the reachable target rather than a pass rate: cases with no
/// `.errors.txt` expect zero diagnostics, so a parser alone can be held to them.
/// Cases with a recorded `.diff` are excluded, since upstream already knows its
/// output differs from TypeScript's there.
///
/// This exists so the parser milestone has a denominator before the parser does.
pub struct ParserReachable;

impl Suite for ParserReachable {
    fn name(&self) -> &'static str {
        "parser_reachable_target"
    }

    fn describes(&self) -> &'static str {
        "the case expects zero diagnostics, so a parser alone can be judged against it"
    }

    fn run(&self, case: &CaseEntry) -> Outcome {
        if !case.has_any_baseline() {
            return Outcome::Skipped {
                reason: "upstream recorded no output for this case, so there is nothing to judge \
                         against"
                    .into(),
            };
        }
        if case.has_known_divergence() {
            return Outcome::Skipped {
                reason: "upstream records a known divergence from TypeScript (.diff baseline)"
                    .into(),
            };
        }
        // A configuration-varied case has no single expected output: its
        // diagnostics live under `case(target=es2015).errors.txt`. Treating the
        // absence of a plain baseline as "expects nothing" would silently inflate
        // this number by 793 cases.
        if case.has_varied_errors() {
            return Outcome::Skipped {
                reason: "configuration-varied baselines; needs per-configuration runs".into(),
            };
        }
        match case.expected_errors() {
            Ok(None) => Outcome::Passed,
            Ok(Some(_)) => {
                Outcome::Failed { reason: "expects diagnostics; needs the checker to judge".into() }
            }
            Err(err) => Outcome::Failed { reason: format!("{err:#}") },
        }
    }
}
