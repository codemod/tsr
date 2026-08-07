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
        // `//// [foo.d.ts]` section that upstream never emitted. 127 cases in this
        // corpus are that shape, and reading the echo as output made the emitter
        // look like it was producing files upstream did not. The discriminator is
        // exact rather than heuristic, and lives in [`output_units`].
        let units = output_units(&baseline, &parsed_case);
        if units.is_empty() {
            return Outcome::Skipped {
                reason: "the emit baseline has no emitted .d.ts section".into(),
            };
        }

        // **Emit every unit before judging any of them.** Returning on the first
        // unit that disagrees looks equivalent and is not: a case whose second unit
        // needs inference is a skip, but only if the first unit did not already
        // fail. Short-circuiting therefore made the *denominator* depend on the
        // emitter's output — removing `declare` moved 18 cases out of the skip
        // bucket and into the judged set. Same lesson as the round-trip suite,
        // where the parse-cleanliness check had to be hoisted ahead of all
        // printing (`docs/architecture/printer.md`).
        let mut produced = Vec::with_capacity(units.len());
        for (unit, expected) in &units {
            let kind = ScriptKind::from_file_name(&unit.name);
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
            let references = declaration_references(&parsed.file_references);
            let mut nodes = parsed.nodes;
            let result = tsr_declarations::emit_with_references_and_options(
                &arena,
                &mut nodes,
                parsed.source_file,
                &references,
                declaration_emit_options(&parsed_case, &unit.content),
            );
            if !result.diagnostics.is_empty() {
                return Outcome::Skipped {
                    reason: "a declaration in this case needs inference (see dts_reachable_target)"
                        .into(),
                };
            }
            if let Some(kind) = result.unsupported.first() {
                return Outcome::Unsupported { reason: format!("printer: {kind}") };
            }
            produced.push((result.text, *expected));
        }

        for (text, expected) in &produced {
            if normalise(text) != normalise(&expected.content) {
                return Outcome::Failed { reason: first_difference(&expected.content, text) };
            }
        }

        // Over-emission: a unit upstream produced no declaration file for, that
        // this port emits into anyway. Checked after the comparisons so a genuine
        // text difference is reported in preference to it.
        for unit in unemitted_units(&baseline, &parsed_case) {
            if emits_anything(unit) {
                return Outcome::Failed {
                    reason: format!(
                        "emitted {}, upstream emitted no declaration file",
                        declaration_name(&unit.name)
                    ),
                };
            }
        }
        Outcome::Passed
    }
}

/// The units this case has declaration output for, paired with it.
///
/// **Computed before anything is emitted.** Deciding case-by-case *while* emitting
/// makes the denominator depend on the emitter: a unit whose output happened to be
/// empty fell through to a skip, so improving the transform silently moved cases
/// between judged and skipped. `docs/architecture/printer.md` records the same
/// trap in the round-trip suite, where the parse-cleanliness check had to be
/// hoisted ahead of all printing for exactly this reason. It was found here by a
/// mutation: removing the scope-fix marker moved 44 cases out of the denominator.
pub(crate) fn output_units<'a>(
    baseline: &'a JsBaseline,
    case: &'a crate::TestCase,
) -> Vec<(&'a crate::TestFile, &'a crate::js_baseline::Section)> {
    let inputs: std::collections::HashSet<&str> =
        case.files.iter().map(|file| file.name.as_str()).collect();
    case.files
        .iter()
        .filter(|unit| {
            ScriptKind::from_file_name(&unit.name) != ScriptKind::Json
                && !is_declaration_file_name(&unit.name)
        })
        .filter_map(|unit| {
            let name = declaration_name(&unit.name);
            let section = baseline.sections.iter().find(|section| {
                section.name == name
                    && section.is_declaration()
                    && !inputs.contains(section.name.as_str())
            })?;
            Some((unit, section))
        })
        .collect()
}

/// The declaration file name for a source unit.
///
/// `.mts` and `.cts` emit `.d.mts` and `.d.cts`; the corpus has 495 such sections
/// and reading them all as `.d.ts` would silently miss every ESM/CJS case.
pub(crate) fn declaration_name(unit: &str) -> String {
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
pub(crate) fn is_declaration_file_name(unit: &str) -> bool {
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

/// Units of a judged case that upstream emitted **no** declaration file for.
///
/// Emitting into one of these is a real defect — the `.d.ts` names a file the
/// compiler never produced — so it is a failure. It is deliberately *not* part of
/// the denominator: a case is judged because upstream emitted at least one
/// declaration section for it, and whether we also over-emit into a sibling unit
/// is a property of that already-judged case. Keeping the two apart is what lets
/// this be caught without the denominator depending on the emitter again.
///
/// About 35 of these need the `Program` to decide (a unit excluded from the
/// compilation emits nothing), so the count is not expected to reach zero in
/// Phase 3.5. It is reported rather than hidden.
pub(crate) fn unemitted_units<'a>(
    baseline: &'a JsBaseline,
    case: &'a crate::TestCase,
) -> Vec<&'a crate::TestFile> {
    let inputs: std::collections::HashSet<&str> =
        case.files.iter().map(|file| file.name.as_str()).collect();
    case.files
        .iter()
        .filter(|unit| {
            ScriptKind::from_file_name(&unit.name) != ScriptKind::Json
                && !is_declaration_file_name(&unit.name)
        })
        .filter(|unit| {
            let name = declaration_name(&unit.name);
            !baseline.sections.iter().any(|section| {
                section.name == name
                    && section.is_declaration()
                    && !inputs.contains(section.name.as_str())
            })
        })
        .collect()
}

/// Whether this port emits anything at all for a unit.
///
/// Used only to decide whether an over-emission has happened, so parse failures
/// and unsupported nodes answer "no": neither is evidence of over-emission, and
/// both are already reported by the caller's own checks.
pub(crate) fn emits_anything(unit: &crate::TestFile) -> bool {
    let kind = ScriptKind::from_file_name(&unit.name);
    let arena = Arena::new();
    let parsed = tsr_parser::parse_with_script_kind(&arena, &unit.content, kind);
    if !parsed.diagnostics.is_empty() {
        return false;
    }
    let references = declaration_references(&parsed.file_references);
    let mut nodes = parsed.nodes;
    let result =
        tsr_declarations::emit_with_references(&arena, &mut nodes, parsed.source_file, &references);
    result.unsupported.is_empty() && !result.text.trim().is_empty()
}

pub(crate) fn declaration_references(
    references: &tsr_parser::FileReferences,
) -> Vec<tsr_declarations::DeclarationReference> {
    use tsr_declarations::{
        DeclarationReference, DeclarationReferenceKind, DeclarationResolutionMode,
    };

    let resolution_mode = |mode| match mode {
        tsr_parser::ResolutionMode::None => DeclarationResolutionMode::None,
        tsr_parser::ResolutionMode::CommonJS => DeclarationResolutionMode::Require,
        tsr_parser::ResolutionMode::ESNext => DeclarationResolutionMode::Import,
    };
    let convert = |reference: &tsr_parser::FileReference, kind| DeclarationReference {
        kind,
        file_name: reference.file_name.clone(),
        resolution_mode: resolution_mode(reference.resolution_mode),
        position: reference.span.start,
    };

    references
        .referenced_files
        .iter()
        .filter(|reference| reference.preserve)
        .map(|reference| convert(reference, DeclarationReferenceKind::Path))
        .chain(
            references
                .type_reference_directives
                .iter()
                .filter(|reference| reference.preserve)
                .map(|reference| convert(reference, DeclarationReferenceKind::Types)),
        )
        .chain(
            references
                .lib_reference_directives
                .iter()
                .filter(|reference| reference.preserve)
                .map(|reference| convert(reference, DeclarationReferenceKind::Lib)),
        )
        .collect()
}

pub(crate) fn declaration_emit_options<'a>(
    case: &crate::TestCase,
    source_text: &'a str,
) -> tsr_declarations::DeclarationEmitOptions<'a> {
    tsr_declarations::DeclarationEmitOptions {
        source_text: Some(source_text),
        strip_internal: case
            .options
            .get("stripinternal")
            .is_some_and(|value| value.eq_ignore_ascii_case("true")),
    }
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
