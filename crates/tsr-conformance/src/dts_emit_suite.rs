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
            let references = rebase_path_references(
                declaration_references(&parsed.file_references),
                &unit.name,
                &expected.name,
            );
            let mut nodes = parsed.nodes;
            stamp_javascript_root(&unit.name, parsed.source_file, &mut nodes);
            // The URL is relative to the declaration file, so only the final
            // path component is named.
            let declaration_file = declaration_name(&unit.name);
            let map_base = declaration_file.rsplit('/').next().unwrap_or(&declaration_file);
            let map_url = format!("{map_base}.map");
            let mut options = declaration_emit_options(&parsed_case, &unit.name, &unit.content);
            if parsed_case
                .options
                .get("declarationmap")
                .is_some_and(|value| value.eq_ignore_ascii_case("true"))
            {
                options.source_map_url = Some(&map_url);
            }
            let result = tsr_declarations::emit_with_references_and_options(
                &arena,
                &mut nodes,
                parsed.source_file,
                &references,
                options,
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
    pair_units(baseline, case)
        .into_iter()
        .filter_map(|(unit, section)| Some((unit, section?)))
        .collect()
}

/// Every emittable unit paired with its declaration section, if upstream wrote
/// one.
///
/// Pairing is positional where names collide: two `@filename` directories can
/// flatten to the same section name (`moduleDeclarationExportStarShadowingGlobalIsNameable`
/// has two `index.ts` units and two `index.d.ts` outputs), and upstream writes
/// outputs in program order, so the k-th unit claiming a name pairs with the
/// k-th section bearing it. Exact full-name matches are claimed first so a
/// pathed output never loses its section to a basename collision.
fn pair_units<'a>(
    baseline: &'a JsBaseline,
    case: &'a crate::TestCase,
) -> Vec<(&'a crate::TestFile, Option<&'a crate::js_baseline::Section>)> {
    fn base(path: &str) -> &str {
        path.rsplit('/').next().unwrap_or(path)
    }
    let emitted = emitted_sections(baseline, case);
    let units: Vec<&crate::TestFile> = case
        .files
        .iter()
        .filter(|unit| {
            ScriptKind::from_file_name(&unit.name) != ScriptKind::Json
                && !is_declaration_file_name(&unit.name)
                // Files under node_modules are program inputs, never outputs:
                // upstream writes no .js or .d.ts for them
                // (compositeWithNodeModulesSourceFile).
                && !unit.name.contains("node_modules/")
        })
        .collect();

    // Positional within each basename group: exact-name claiming is actively
    // wrong when two flattened outputs share a name, because a bare-named unit
    // would grab the *first* section regardless of whose output it is
    // (`moduleDeclarationExportStarShadowingGlobalIsNameable`).
    let mut claimed = vec![false; emitted.len()];
    let mut pairs: Vec<Option<usize>> = vec![None; units.len()];
    for (index, unit) in units.iter().enumerate() {
        let wanted = declaration_name(&unit.name);
        if let Some(found) = emitted.iter().enumerate().position(|(position, section)| {
            !claimed[position] && section.is_declaration() && base(&section.name) == base(&wanted)
        }) {
            claimed[found] = true;
            pairs[index] = Some(found);
        }
    }

    units
        .into_iter()
        .zip(pairs)
        .map(|(unit, section)| (unit, section.map(|index| emitted[index])))
        .collect()
}

/// The baseline's emitted sections: everything after the input echoes.
///
/// A baseline echoes every input unit — in case order, flattened to its final
/// path component — before any emitted file, so the echo region is the longest
/// prefix of sections whose basename *and content* match an input unit. Name
/// equality alone cannot discriminate: an input at
/// `node_modules/lib/index.d.ts` echoes as `index.d.ts`, which is exactly the
/// name unit `index.ts` emits to, and matching by name paired emitted output
/// with a stub input (`moduleLocalImportNotIncorrectlyRedirected`).
fn emitted_sections<'a>(
    baseline: &'a JsBaseline,
    case: &crate::TestCase,
) -> Vec<&'a crate::js_baseline::Section> {
    fn base(path: &str) -> &str {
        path.rsplit('/').next().unwrap_or(path)
    }
    let is_echo = |section: &crate::js_baseline::Section| {
        case.files.iter().any(|unit| {
            base(&unit.name) == base(&section.name)
                && normalise(&unit.content) == normalise(&section.content)
        })
    };
    let boundary = baseline
        .sections
        .iter()
        .position(|section| !is_echo(section))
        .unwrap_or(baseline.sections.len());
    baseline.sections[boundary..].iter().collect()
}

/// Stamp the JavaScript-file root flag upstream's parser derives from its
/// `ScriptKind`. This parser never sees the file name (ADR-0016), so the
/// harness supplies the fact; JSDoc accessibility tags act as modifiers only
/// under it.
pub(crate) fn stamp_javascript_root(
    unit_name: &str,
    source_file: &tsr_ast::SourceFile<'_>,
    nodes: &mut tsr_ast::NodeTable,
) {
    let lower = unit_name.to_ascii_lowercase();
    let is_js = [".js", ".jsx", ".mjs", ".cjs"].iter().any(|suffix| lower.ends_with(suffix));
    if is_js && let Some(root) = source_file.node_id {
        nodes.add_flags(root, tsr_ast::NodeFlags::JAVASCRIPT_FILE);
    }
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
    pair_units(baseline, case)
        .into_iter()
        .filter_map(|(unit, section)| section.is_none().then_some(unit))
        .collect()
}

/// Whether this port emits anything at all for a unit.
///
/// Used only to decide whether an over-emission has happened, so parse failures
/// and unsupported nodes answer "no": neither is evidence of over-emission, and
/// both are already reported by the caller's own checks. Analysis diagnostics
/// answer "no" for the same reason: a unit that needs inference is outside the
/// population this suite judges — and when upstream emitted nothing for such a
/// unit, its own errors suppressed the file (`isolatedDeclarationErrorsDefault`
/// emits `f.d.ts` and nothing for the five erroring units), so producing text
/// there is not evidence of over-emission either.
pub(crate) fn emits_anything(unit: &crate::TestFile) -> bool {
    let kind = ScriptKind::from_file_name(&unit.name);
    let arena = Arena::new();
    let parsed = tsr_parser::parse_with_script_kind(&arena, &unit.content, kind);
    if !parsed.diagnostics.is_empty() {
        return false;
    }
    let references = declaration_references(&parsed.file_references);
    let mut nodes = parsed.nodes;
    stamp_javascript_root(&unit.name, parsed.source_file, &mut nodes);
    let result =
        tsr_declarations::emit_with_references(&arena, &mut nodes, parsed.source_file, &references);
    result.diagnostics.is_empty() && result.unsupported.is_empty() && !result.text.trim().is_empty()
}

/// Rewrite preserved `path` references relative to where the declaration file
/// is actually emitted.
///
/// Upstream re-relativizes every kept `path` reference against the output
/// file's directory — `getReferencedFiles(outputFilePath)` with
/// `outputFilePath = GetDirectoryPath(declarationFilePath)` and
/// `GetRelativePathToDirectoryOrUrl` doing the math
/// (`vendor/typescript-go/internal/transformers/declarations/transform.go:464`).
/// The source spelling is only correct when the declaration lands beside its
/// source; under `outDir` the reference gains a step (`commonSourceDirectory`:
/// `../types/bar.d.ts` written in `/app/index.ts` must read
/// `../../types/bar.d.ts` from `/app/bin/index.d.ts`). The harness knows the
/// output location from the baseline section upstream actually wrote, so the
/// rebasing lives here rather than behind a host abstraction the emitter does
/// not have.
///
/// A target that is already a `.d.ts` keeps its own path, matching upstream's
/// `IsDeclarationFile` arm. A `.ts` target should map through its *own* output
/// path (`GetOutputPathsFor`); this rebase leaves its resolved source path for
/// `declaration_reference_name`'s extension swap, which is only right when that
/// target emits beside itself — the same approximation the pass-through made.
fn rebase_path_references(
    mut references: Vec<tsr_declarations::DeclarationReference>,
    source_name: &str,
    output_name: &str,
) -> Vec<tsr_declarations::DeclarationReference> {
    fn dir_of(path: &str) -> &str {
        path.rfind('/').map_or("", |index| &path[..index])
    }
    let source_dir = dir_of(source_name);
    let output_dir = dir_of(output_name);
    if source_dir == output_dir {
        return references;
    }
    for reference in &mut references {
        if reference.kind != tsr_declarations::DeclarationReferenceKind::Path {
            continue;
        }
        let target = resolve_segments(source_dir, &reference.file_name);
        reference.file_name = relative_from(output_dir, &target);
    }
    references
}

/// `dir` joined with `path`, with `.` and `..` segments folded away.
///
/// Root anchors are dropped rather than tracked: source and output names in one
/// case share their rooting style, so rootedness cancels in the relative math.
fn resolve_segments(dir: &str, path: &str) -> Vec<String> {
    let mut segments: Vec<String> = Vec::new();
    for segment in dir.split('/').chain(path.split('/')) {
        match segment {
            "." | "" => {}
            ".." if segments.last().is_some_and(|last| last != "..") => {
                segments.pop();
            }
            other => segments.push(other.to_string()),
        }
    }
    segments
}

/// The relative path from `dir` to `target`, one `..` per unshared segment.
fn relative_from(dir: &str, target: &[String]) -> String {
    let from = resolve_segments(dir, "");
    let shared = from.iter().zip(target).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<&str> = vec![".."; from.len() - shared];
    parts.extend(target[shared..].iter().map(String::as_str));
    parts.join("/")
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
    unit_name: &str,
    source_text: &'a str,
) -> tsr_declarations::DeclarationEmitOptions<'a> {
    // A `.mts`/`.cts` extension (or `moduleDetection: force`) marks the file a
    // module regardless of its syntax; upstream's detection is not purely
    // syntactic and the transform needs the fact.
    let lower = unit_name.to_ascii_lowercase();
    let force_module = [".mts", ".cts", ".mjs", ".cjs"]
        .iter()
        .any(|extension| lower.ends_with(extension) && !is_declaration_file_name(&lower))
        || case
            .options
            .get("moduledetection")
            .is_some_and(|value| value.eq_ignore_ascii_case("force"));
    tsr_declarations::DeclarationEmitOptions {
        source_text: Some(source_text),
        force_module,
        strip_internal: case
            .options
            .get("stripinternal")
            .is_some_and(|value| value.eq_ignore_ascii_case("true")),
        remove_comments: case
            .options
            .get("removecomments")
            .is_some_and(|value| value.eq_ignore_ascii_case("true")),
        strict_null_checks: case
            .options
            .get("strictnullchecks")
            .or_else(|| case.options.get("strict"))
            .is_none_or(|value| value.eq_ignore_ascii_case("true")),
        source_map_url: None,
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
    fn declaration_emit_options_follow_strict_null_directives() {
        let default_case = crate::TestCase::parse("compiler/default", "default.ts", "");
        assert!(declaration_emit_options(&default_case, "default.ts", "").strict_null_checks);

        let non_strict =
            crate::TestCase::parse("compiler/nonStrict", "nonStrict.ts", "// @strict: false\n");
        assert!(!declaration_emit_options(&non_strict, "nonStrict.ts", "").strict_null_checks);

        let override_case = crate::TestCase::parse(
            "compiler/override",
            "override.ts",
            "// @strict: false\n// @strictNullChecks: true\n",
        );
        assert!(declaration_emit_options(&override_case, "override.ts", "").strict_null_checks);
    }

    #[test]
    fn the_first_difference_is_located_by_line() {
        assert_eq!(first_difference("a\nb\n", "a\nc\n"), "line 2: want `b`, got `c`".to_string());
        assert_eq!(first_difference("a\nb\n", "a\n"), "line 2: missing `b`".to_string());
        assert_eq!(first_difference("a\n", "a\nb\n"), "line 2: extra `b`".to_string());
    }
}
