//! `dts_emit`: the emitted `.d.ts` text, compared byte for byte.
//!
//! # What a pass means, and what the denominator is
//!
//! A pass is **byte-identical declaration output for every unit of the case**.
//! Nothing weaker: the point of an emit gate is that the text is the product, and
//! a comparison that normalises whitespace or ignores comments would be measuring
//! something nobody ships.
//!
//! The denominator is deliberately the *reachable* set rather than every case with
//! `.d.ts` output, and this is the one judgement call in the suite. A case that
//! draws a `TS9xxx` is one where upstream inferred a type and this port cannot
//! ([ADR-0021](../../../docs/adr/0021-isolated-declarations-is-not-a-port.md)); its
//! `.d.ts` names types that are not written anywhere in the source. Counting those
//! as failures would bury the emitter's actual rate under 714 cases that were
//! never in reach, and — worse — the number would then move whenever the
//! *analysis* changed, which is a different component.
//!
//! So the two suites are stacked on purpose:
//!
//! | | |
//! |---|---|
//! | `dts_reachable_target` | how many cases a checker-free emitter *could* be held to — 575 |
//! | `dts_emit` (this) | how many of those it actually reproduces |
//!
//! The skip counts are printed, so the shrinking is visible rather than implied.
//!
//! # Why the rate cannot reach 100% as things stand
//!
//! Three known gaps, all recorded in `docs/architecture/declaration-emit.md` and
//! all of them *text* rather than *rules*:
//!
//! - The printer drops comments, and upstream's `.d.ts` preserves JSDoc.
//! - Visibility is approximated by reachability from the exports.
//! - Formatting — quote style, blank lines between statements — was free under the
//!   round-trip gate and is not free here.
//!
//! Each shows up as a failure with its own shape, which is the point of comparing
//! bytes rather than trees: a gate that could not tell these apart would report a
//! single undifferentiated number.

use tsr_core::Arena;
use tsr_parser::ScriptKind;

use crate::{
    CaseEntry,
    js_baseline::JsBaseline,
    suite::{Outcome, Suite},
};

/// The `dts_emit` suite.
pub struct DtsEmit;

impl Suite for DtsEmit {
    fn name(&self) -> &'static str {
        "dts_emit"
    }

    fn describes(&self) -> &'static str {
        "the .d.ts text this port emits is byte-identical to upstream's, for every \
         unit of a case that needs no inference — the emitter's own rate, over the \
         population dts_reachable_target measures"
    }

    fn run(&self, case: &CaseEntry) -> Outcome {
        if !case.has_any_baseline() {
            return Outcome::Skipped { reason: "upstream recorded no output for this case".into() };
        }
        let Ok(text) = std::fs::read_to_string(case.baseline_path("js")) else {
            return Outcome::Skipped { reason: "upstream recorded no .js emit baseline".into() };
        };
        let baseline = JsBaseline::parse(&text);
        let Ok(parsed_case) = case.load() else {
            return Outcome::Failed { reason: "case did not load".into() };
        };

        // **A `.d.ts` section is not necessarily declaration output.** A baseline
        // echoes every *input* unit before the emitted files, so a case with a
        // `foo.d.ts` input — an ambient library, a `node_modules` stub — carries a
        // `//// [foo.d.ts]` section that upstream never emitted. 215 cases in this
        // corpus are that shape, and reading the echo as output made the emitter
        // look like it was producing files upstream did not.
        //
        // The discriminator is exact rather than heuristic: a declaration section
        // is output iff no input unit has that name.
        let inputs: std::collections::HashSet<&str> =
            parsed_case.files.iter().map(|file| file.name.as_str()).collect();
        let has_output = baseline
            .sections
            .iter()
            .any(|section| section.is_declaration() && !inputs.contains(section.name.as_str()));
        if !has_output {
            return Outcome::Skipped {
                reason: "the emit baseline's .d.ts sections are all echoed inputs".into(),
            };
        }

        // Emit every unit first, then judge. Doing it lazily is what let the
        // printer's round-trip denominator move as the printer improved
        // (`docs/architecture/printer.md`), and the same trap is here: a case whose
        // first unit is unreachable and whose second is not must be one skip, not a
        // skip that becomes a judgement when the analysis changes.
        let mut emitted: Vec<(String, String)> = Vec::new();
        for unit in &parsed_case.files {
            let kind = ScriptKind::from_file_name(&unit.name);
            if kind == ScriptKind::Json {
                continue;
            }
            // A `.d.ts` input **is** a declaration file. Upstream's transform
            // returns it untouched (`visitSourceFile`, `transform.go:281`) and its
            // emitter writes no declaration output for it at all, so there is
            // nothing to compare. 201 cases in this corpus carry one — ambient
            // library units, `node_modules` stubs — and asking the emitter for
            // `foo.d.d.ts` was a question upstream never answers.
            if is_declaration_file_name(&unit.name) {
                continue;
            }
            let arena = Arena::new();
            let parsed = tsr_parser::parse_with_script_kind(&arena, &unit.content, kind);
            if !parsed.diagnostics.is_empty() {
                // The corpus is a compiler test suite and contains deliberately
                // malformed syntax. Emitting from a tree built by error recovery is
                // not a property this port owes anyone.
                return Outcome::Skipped {
                    reason: "a unit of this case does not parse cleanly".into(),
                };
            }
            let mut nodes = parsed.nodes;
            let result = tsr_declarations::emit(&arena, &mut nodes, parsed.source_file);
            if !result.diagnostics.is_empty() {
                return Outcome::Skipped {
                    reason: "a declaration in this case needs inference (see dts_reachable_target)"
                        .into(),
                };
            }
            if let Some(kind) = result.unsupported.first() {
                return Outcome::Unsupported { reason: format!("printer: {kind}") };
            }
            emitted.push((declaration_name(&unit.name), result.text));
        }

        if emitted.is_empty() {
            return Outcome::Skipped { reason: "the case has no unit to emit".into() };
        }

        for (name, produced) in &emitted {
            let Some(expected) = baseline.sections.iter().find(|section| &section.name == name)
            else {
                // Upstream emitted no declaration file for this unit — usually
                // because the unit is a `.js` input echo, or is not part of the
                // program. Producing text where upstream produced none is a real
                // disagreement only if we produced something.
                if produced.trim().is_empty() {
                    continue;
                }
                return Outcome::Failed { reason: format!("emitted {name}, upstream did not") };
            };
            if normalise(produced) != normalise(&expected.content) {
                return Outcome::Failed { reason: first_difference(&expected.content, produced) };
            }
        }
        Outcome::Passed
    }
}

/// The declaration file name for a source unit.
///
/// `.mts` and `.cts` emit `.d.mts` and `.d.cts`; the corpus has 495 such sections
/// and reading them all as `.d.ts` would silently miss every ESM/CJS case.
fn declaration_name(unit: &str) -> String {
    for (source, declaration) in [
        (".mts", ".d.mts"),
        (".cts", ".d.cts"),
        (".mjs", ".d.mts"),
        (".cjs", ".d.cts"),
        (".tsx", ".d.ts"),
        (".jsx", ".d.ts"),
        (".ts", ".d.ts"),
        (".js", ".d.ts"),
    ] {
        if let Some(stem) = unit.strip_suffix(source) {
            return format!("{stem}{declaration}");
        }
    }
    format!("{unit}.d.ts")
}

/// Ported from `tspath.IsDeclarationFileName`.
fn is_declaration_file_name(unit: &str) -> bool {
    let lower = unit.to_ascii_lowercase();
    lower.ends_with(".d.ts") || lower.ends_with(".d.mts") || lower.ends_with(".d.cts")
}

/// Trailing whitespace and the final newline are not compared.
///
/// Baseline sections are split on blank lines by [`JsBaseline`], so a trailing
/// newline is an artefact of the splitter rather than of upstream's emit. Nothing
/// *inside* a line is normalised — indentation and blank lines between statements
/// are real output and are compared.
fn normalise(text: &str) -> String {
    text.trim_end().replace("\r\n", "\n")
}

/// The first line that differs, which is what a snapshot reader needs.
///
/// A whole-text diff in a committed snapshot makes a one-case regression
/// unreadable; the line number and both sides are enough to classify a failure
/// without opening the baseline.
fn first_difference(expected: &str, produced: &str) -> String {
    let expected = normalise(expected);
    let produced = normalise(produced);
    for (index, (want, got)) in expected.lines().zip(produced.lines()).enumerate() {
        if want != got {
            return format!("line {}: want `{want}`, got `{got}`", index + 1);
        }
    }
    let want_lines = expected.lines().count();
    let got_lines = produced.lines().count();
    if want_lines > got_lines {
        let missing = expected.lines().nth(got_lines).unwrap_or_default();
        return format!("line {}: missing `{missing}`", got_lines + 1);
    }
    let extra = produced.lines().nth(want_lines).unwrap_or_default();
    format!("line {}: extra `{extra}`", want_lines + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declaration_names_follow_the_module_suffix() {
        assert_eq!(declaration_name("a.ts"), "a.d.ts");
        assert_eq!(declaration_name("a.tsx"), "a.d.ts");
        assert_eq!(declaration_name("a.mts"), "a.d.mts");
        assert_eq!(declaration_name("a.cts"), "a.d.cts");
        // A JavaScript unit emits `a.d.ts`, not `a.js.d.ts`. The wrong mapping
        // made 25 cases report "emitted a.js.d.ts, upstream did not", which reads
        // as an over-emission bug and was a naming bug.
        assert_eq!(declaration_name("a.js"), "a.d.ts");
        assert_eq!(declaration_name("a.mjs"), "a.d.mts");
    }

    #[test]
    fn declaration_file_inputs_are_recognised() {
        assert!(is_declaration_file_name("lib.d.ts"));
        assert!(is_declaration_file_name("a.d.mts"));
        assert!(!is_declaration_file_name("a.ts"));
    }

    #[test]
    fn the_first_difference_is_located_by_line() {
        assert_eq!(first_difference("a\nb\n", "a\nc\n"), "line 2: want `b`, got `c`".to_string());
        assert_eq!(first_difference("a\nb\n", "a\n"), "line 2: missing `b`".to_string());
        assert_eq!(first_difference("a\n", "a\nb\n"), "line 2: extra `b`".to_string());
    }
}
