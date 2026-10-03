//! `diagnostics`: every diagnostic upstream reports, by code and position.
//!
//! # The other half of a checker's conformance
//!
//! [`crate::types_suite`] gates the types a checker computes. This gates the
//! *errors* it reports, and that is the larger half: a `.errors.txt` baseline is
//! what a user sees.
//!
//! Until now the only diagnostic comparison in this repository was
//! `isolated_declarations`, which filters to the `9000..9100` range — 20 codes out
//! of TypeScript's ~1,600. Everything else upstream reports was unmeasured.
//!
//! # What this port can produce today
//!
//! The scanner, the parser and the binder. No checker (`bd tsr-4sc`), so every
//! `TS2xxx` in a baseline is a diagnostic we cannot yet emit — which is the point
//! of the number, not a defect in it.
//!
//! `tsr_dts`'s `TS9xxx` are deliberately **not** included. They are produced under
//! `isolatedDeclarations`, which is a per-case compiler option this suite does not
//! model, and emitting them everywhere would be a false positive on every case
//! that does not set it. `isolated_declarations` gates those separately.
//!
//! # Only cases that expect at least one diagnostic are judged
//!
//! A case whose baseline records **no** diagnostics is a real expectation, and
//! reproducing it means reporting nothing — but "we report nothing" is already
//! measured, thoroughly, by `parser_typescript` (99.36%) and
//! `scanner_clean_files` (100%). Counting those cases here would add several
//! thousand near-automatic passes and bury the number that matters: **of the
//! diagnostics upstream reports, how many do we?**
//!
//! The false-positive direction is not lost. A judged case that expects three
//! diagnostics and gets four fails, because the comparison is set equality.
//!
//! # What a pass means
//!
//! The **exact multiset of (file, line, column, code)**, compared after sorting.
//! Not a subset, not "the codes we know about" — a diagnostic we invent fails the
//! case as surely as one we miss.
//!
//! Positions are 1-based, as the baselines write them, and the column is a UTF-16
//! code unit offset. Getting that wrong shifts every diagnostic on a line
//! containing an astral character, which is the kind of error that looks like a
//! checker bug for a week.
//!
//! # Exclusions
//!
//! Ordered so each reason means what it says — the trap
//! `docs/architecture/checker-oracle.md` records for the two `.types`/`.symbols`
//! suites:
//!
//! - **Configuration-varied** baselines, until `bd tsr-bb4.1` runs per
//!   configuration. There is no single expected output to compare against.
//! - **Known divergences** (`.errors.txt.diff`): upstream records that its own
//!   output differs from TypeScript's, so the baseline is not a specification.
//! # The checker's contribution comes from a *second* traversal
//!
//! `Checker::check_source_file` ([`tsr_checker::check`], ADR-0040 decisions (1)
//! and (2)), run over a program built exactly the way the `checker_types`
//! gradient builds one — `types_producer::program_for_case`, called rather than
//! copied, because `docs/conventions.md`'s *"a probe that re-implements the
//! harness is measuring a different compiler"* applies to suites first of all.
//!
//! **The parser and binder half above is deliberately left as it was**, per unit
//! and with no program, even though the program binds every file too. Two
//! reasons, and the second is the load-bearing one: `BindResult` holds one flat
//! diagnostic list for the whole program with no per-file attribution, so
//! switching would need new machinery; and re-deriving a set that 80 passing
//! cases already depend on, in the same commit that adds a new source of
//! diagnostics, would make any movement unattributable. The cost accepted is
//! that each judged case is bound twice.
//!
//! - **Cases upstream recorded no output for at all.** 617 of them. A missing
//!   `.errors.txt` means "no diagnostics" only when some other baseline proves the
//!   case ran; without that the absence proves nothing, and reading it as a clean
//!   expectation hands out free passes.

use tsr_parser::ParsedFile;

use crate::{
    CaseEntry,
    errors_baseline::{self, BaselineDiagnostic},
    suite::{Outcome, Suite},
    symbols_baseline::line_and_character,
    types_producer::program_for_case,
};

/// The `diagnostics` suite.
pub struct Diagnostics;

impl Suite for Diagnostics {
    fn name(&self) -> &'static str {
        "diagnostics"
    }

    fn describes(&self) -> &'static str {
        "every diagnostic in upstream's .errors.txt reproduced exactly — same file, \
         line, column and code, no more and no fewer — for cases that expect at \
         least one; scanner, parser and binder only, with no checker (bd tsr-4sc)"
    }

    fn run(&self, case: &CaseEntry) -> Outcome {
        if case.has_varied_errors() {
            return Outcome::Skipped {
                reason: "configuration-varied baseline (bd tsr-bb4.1)".into(),
            };
        }
        if case.has_known_divergence() {
            return Outcome::Skipped {
                reason: "upstream records a known divergence from TypeScript".into(),
            };
        }
        // Absence is evidence only when something else proves the case ran.
        if !case.has_any_baseline() {
            return Outcome::Skipped { reason: "upstream recorded no output for this case".into() };
        }
        let Ok(baseline) = case.expected_errors() else {
            return Outcome::Failed { reason: "baseline did not load".into() };
        };
        let mut expected: Vec<BaselineDiagnostic> =
            baseline.as_deref().map(errors_baseline::parse).unwrap_or_default();
        if expected.is_empty() {
            return Outcome::Skipped {
                reason: "the case expects no diagnostics (see parser_typescript)".into(),
            };
        }
        expected.sort_unstable();

        let Ok(test) = case.load() else {
            return Outcome::Failed { reason: "case did not load".into() };
        };

        let mut actual = reported_for(&test);
        actual.sort_unstable();

        if actual == expected {
            return Outcome::Passed;
        }
        Outcome::Failed { reason: summarise(&expected, &actual) }
    }
}

/// Every diagnostic this port reports for a case: parser, binder, and the check
/// traversal.
///
/// **Public, and the suite calls it rather than inlining it**, because
/// `examples/diaggap.rs` ranks the suite's failures by code and a probe that
/// re-derives this set is ranking a different compiler's failures
/// (`docs/conventions.md`). Unsorted: the caller sorts, because the comparison
/// is a sorted-multiset equality and doing it twice hides which side is which.
#[must_use]
pub fn reported_for(test: &crate::TestCase) -> Vec<BaselineDiagnostic> {
    let mut actual: Vec<BaselineDiagnostic> = Vec::new();
    // `getAllowJS()` — `allowJs ?? checkJs ?? false` (`core/compileroptions.go`).
    // Read here rather than from the resolved `CompilerOptions` because this
    // half of the function deliberately builds no program; ADR-0042's rule
    // about reading directives by hand applies to options with *defaulting
    // ladders*, and this one has two explicit keys and no fallback.
    let allow_js = ["allowjs", "checkjs"]
        .iter()
        .any(|key| test.options.get(*key).is_some_and(|value| value != "false"));
    for unit in &test.files {
        let kind = tsr_parser::ScriptKind::from_file_name(&unit.name);
        if kind == tsr_parser::ScriptKind::Json {
            continue;
        }
        // **A JavaScript file is not a program input without `allowJs`.** The
        // check half already agrees with the program — it skips any unit
        // `program.source_file` does not return — and this half did not, so it
        // parsed and diagnosed files upstream never reads.
        //
        // `extendsUntypedModule` is the shape: its two
        // `/node_modules/**/index.js` units contain the literal text
        // *"This file is not read."*, which is not JavaScript and is not meant
        // to be, and this port emitted nine parse errors across them. The rule
        // is already ported in `tsr_tsoptions::file_names` and pinned by
        // `without_allow_js_a_javascript_file_is_not_a_root`; only this loop
        // was missing it. `checker-notes-diag2.md` §197.
        // Asked of the **extension**, because `ScriptKind` does not separate
        // JavaScript from TypeScript — `.js` and `.ts` are both `TypeScript`,
        // `.jsx` and `.tsx` both `Tsx`; the enum is a *dialect* flag.
        let is_javascript =
            [".js", ".jsx", ".mjs", ".cjs"].iter().any(|extension| unit.name.ends_with(extension));
        if !allow_js && is_javascript {
            continue;
        }
        let parsed = ParsedFile::parse_with_script_kind(unit.content.clone(), kind);
        let mut reported = parsed.diagnostics().to_vec();
        // The binder reports strict-mode and grammar diagnostics that the
        // parser does not, and they appear in the same baselines.
        parsed.with_ast_and_arena(|file, arena| {
            let bound = tsr_binder::bind(
                arena,
                file,
                parsed.nodes(),
                tsr_binder::FileInfo { name: &unit.name, text: parsed.source() },
            );
            reported.extend(bound.diagnostics().iter().cloned());
        });
        // `SortAndDeduplicateDiagnostics` again — the harness applies it to the
        // whole list and the binder's half needs it as much as the checker's:
        // `class A { m1: string; m1(a: string): void; m1(a: number): void; … }`
        // makes `declare_into_with_excludes` report on the property **once per
        // collision it takes part in**, and upstream's three identical
        // diagnostics collapse to the one its baseline records. §997.
        let mut seen: std::collections::HashSet<(u32, u32, u32, Vec<String>)> =
            std::collections::HashSet::new();
        reported.retain(|diagnostic| {
            seen.insert((
                diagnostic.span.start,
                diagnostic.span.end,
                diagnostic.message.code(),
                diagnostic.args.clone(),
            ))
        });
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
    actual.extend(from_check_traversal(test));
    // `getBindAndCheckDiagnosticsWithChecker` (`compiler/program.go:1352`)
    // applies the comment-directive filter to the *assembled* set, after every
    // producer has contributed. Doing it anywhere earlier would let a
    // parser diagnostic survive a directive that a checker diagnostic honours.
    apply_comment_directives(test, actual)
}

/// Every diagnostic this port reports, as `((file, line, column, code), rendered
/// message)`.
///
/// [`reported_for`] drops the message because the suite compares only the four
/// printed fields; the baselines carry the text as well, and auditing it is what
/// `diagtext` does. Built by rendering each message template against its
/// arguments — `Message::format`, the same substitution upstream prints.
///
/// `docs/architecture/checker-notes-diag2.md` §998.
#[must_use]
pub fn rendered_for(test: &crate::TestCase) -> Vec<((String, u32, u32, u32), String)> {
    let mut out = Vec::new();
    for unit in &test.files {
        let kind = tsr_parser::ScriptKind::from_file_name(&unit.name);
        if kind == tsr_parser::ScriptKind::Json {
            continue;
        }
        let parsed = ParsedFile::parse_with_script_kind(unit.content.clone(), kind);
        let mut reported = parsed.diagnostics().to_vec();
        parsed.with_ast_and_arena(|file, arena| {
            let bound = tsr_binder::bind(
                arena,
                file,
                parsed.nodes(),
                tsr_binder::FileInfo { name: &unit.name, text: parsed.source() },
            );
            reported.extend(bound.diagnostics().iter().cloned());
        });
        for diagnostic in reported {
            let (line, character) = line_and_character(&unit.content, diagnostic.span.start);
            let args: Vec<&str> = diagnostic.args.iter().map(String::as_str).collect();
            out.push((
                (unit.name.clone(), line + 1, character + 1, diagnostic.message.code()),
                diagnostic.message.format(&args),
            ));
        }
    }
    out
}

/// `@ts-ignore` / `@ts-expect-error`, applied per file over the whole set.
///
/// See [`crate::comment_directives`] for the rule. The split by unit is not an
/// optimisation: a directive suppresses diagnostics **in its own file**, and the
/// line numbers on a `BaselineDiagnostic` are only meaningful against that
/// file's text.
fn apply_comment_directives(
    test: &crate::TestCase,
    reported: Vec<BaselineDiagnostic>,
) -> Vec<BaselineDiagnostic> {
    let mut out = Vec::with_capacity(reported.len());
    let mut remaining = reported;
    for unit in &test.files {
        let directives = crate::comment_directives::directives_in(&unit.content);
        if directives.is_empty() {
            continue;
        }
        let (mine, others): (Vec<_>, Vec<_>) =
            remaining.into_iter().partition(|d| d.file == unit.name);
        remaining = others;
        // The suite's lines are 1-based; the filter's are 0-based, as
        // `ComputeLineOfPosition`'s are.
        let entries: Vec<(u32, BaselineDiagnostic)> =
            mine.into_iter().map(|d| (d.line.saturating_sub(1), d)).collect();
        let (kept, unused) =
            crate::comment_directives::filter(&unit.content, &entries, &directives);
        out.extend(kept);
        for diagnostic in unused {
            let (line, character) = line_and_character(&unit.content, diagnostic.span.start);
            out.push(BaselineDiagnostic {
                file: unit.name.clone(),
                line: line + 1,
                column: character + 1,
                code: diagnostic.message.code(),
            });
        }
    }
    out.extend(remaining);
    out
}

/// One position `report_assignability_failure` was asked about, and the gate
/// that decided it.
///
/// See [`assignability_probe_for`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProbedPosition {
    /// The case-relative file name, as `BaselineDiagnostic` spells it.
    pub file: String,
    /// 1-based, as the baselines are.
    pub line: u32,
    /// 1-based, as the baselines are.
    pub column: u32,
    /// One of `tsr_checker::assignreport::PROBE_*`.
    pub verdict: u8,
}

/// Every position the TS2322 site was reached at, for one case.
///
/// **Runs the shipped checker through the same program `from_check_traversal`
/// builds**, rather than re-deriving one: `docs/conventions.md`'s rule that a
/// probe re-implementing the harness measures a different compiler, and the
/// defect `probefile.rs` exists to prevent (a hand-rolled program silently
/// loses the `/.lib` mount).
///
/// Requires `TSR_ASSIGN_PROBE` in the environment; the checker records nothing
/// otherwise. See `docs/architecture/checker-notes-diag2.md` §172.
#[must_use]
pub fn assignability_probe_for(test: &crate::TestCase) -> Vec<ProbedPosition> {
    let arena = tsr_core::Arena::new();
    let program = program_for_case(&arena, test);
    let mut checker = tsr_checker::Checker::with_module_host(
        program.binder(),
        program.nodes(),
        program.node_map(),
        Some(&program),
    );
    checker.apply_compiler_options(program.compiler_options());

    let mut own_files = Vec::new();
    for unit in &test.files {
        if tsr_parser::ScriptKind::from_file_name(&unit.name) == tsr_parser::ScriptKind::Json {
            continue;
        }
        if let Some(file) = program.source_file(&unit.name)
            && let Some(id) = file.source_file().node_id
        {
            own_files.push(id);
        }
    }
    checker.set_checked_files(own_files);

    let mut units = Vec::new();
    for unit in &test.files {
        if tsr_parser::ScriptKind::from_file_name(&unit.name) == tsr_parser::ScriptKind::Json {
            continue;
        }
        let Some(file) = program.source_file(&unit.name) else { continue };
        let Some(id) = file.source_file().node_id else { continue };
        let declaration_file = unit.name.ends_with(".d.ts")
            || unit.name.ends_with(".d.mts")
            || unit.name.ends_with(".d.cts");
        checker.check_source_file(
            id,
            tsr_checker::check::FileContext {
                ambient: declaration_file,
                has_parse_errors: !file.diagnostics().is_empty(),
            },
        );
        units.push((id, unit.name.clone(), file.text()));
    }

    let mut out = Vec::new();
    for &(file, span, verdict) in &checker.assignability_probe {
        let Some((_, unit_name, source)) = units.iter().find(|(id, _, _)| *id == file) else {
            continue;
        };
        let (line, character) = line_and_character(source, span.start);
        out.push(ProbedPosition {
            file: unit_name.clone(),
            line: line + 1,
            column: character + 1,
            verdict,
        });
    }
    out
}

/// The node kind at every position in the case's own files, keyed the way a
/// baseline line is keyed.
///
/// A diagnostic sits at `error_span(anchor)`, so a baseline line and the node
/// that would carry it join on `(file, line, column)`. Several nodes share one
/// position — `error_span` narrows a declaration to its name, and a name is
/// inside its declaration — so every kind at a position is returned and the
/// caller decides which to believe. §173.
#[must_use]
pub fn node_kinds_by_position_for(
    test: &crate::TestCase,
) -> Vec<(String, u32, u32, tsr_ast::SyntaxKind, Option<tsr_ast::SyntaxKind>)> {
    let arena = tsr_core::Arena::new();
    let program = program_for_case(&arena, test);
    let checker = tsr_checker::Checker::with_module_host(
        program.binder(),
        program.nodes(),
        program.node_map(),
        Some(&program),
    );
    let mut units = Vec::new();
    for unit in &test.files {
        if tsr_parser::ScriptKind::from_file_name(&unit.name) == tsr_parser::ScriptKind::Json {
            continue;
        }
        let Some(file) = program.source_file(&unit.name) else { continue };
        let Some(id) = file.source_file().node_id else { continue };
        units.push((id, unit.name.clone(), file.text()));
    }
    let nodes = program.nodes();
    let mut out = Vec::new();
    for index in 0..nodes.len() {
        let id = <tsr_ast::NodeId as tsr_core::Idx>::from_usize(index);
        let Some(file) = checker.source_file_of_diagnostics(id) else { continue };
        let Some((_, unit_name, source)) = units.iter().find(|(unit, _, _)| *unit == file) else {
            continue;
        };
        let span = checker.error_span_of(id);
        let (line, character) = line_and_character(source, span.start);
        out.push((
            unit_name.clone(),
            line + 1,
            character + 1,
            nodes.kind(id),
            nodes.parent(id).map(|parent| nodes.kind(parent)),
        ));
    }
    out
}

/// Every diagnostic `Checker::check_source_file` reports for the case's own
/// units.
///
/// **The lib files are in the program and are not walked.** Upstream reports
/// nothing in `lib.*.d.ts` under any configuration this corpus uses, and a
/// diagnostic positioned in a lib file can match no baseline line — so walking
/// them could only manufacture a false positive.
fn from_check_traversal(test: &crate::TestCase) -> Vec<BaselineDiagnostic> {
    let arena = tsr_core::Arena::new();
    let program = program_for_case(&arena, test);
    // The **loader's** diagnostics, which are produced before any binder or
    // checker runs and which this port had nowhere to put until §240. Collected
    // here because this is the one half of the suite that builds a program.
    let mut from_loader: Vec<BaselineDiagnostic> = Vec::new();
    for diagnostic in program.loader_diagnostics() {
        // The loader names files by their program path; the baselines name them
        // as the case wrote them.
        let Some(unit) = test.files.iter().find(|unit| {
            diagnostic.file_name.trim_start_matches('/') == unit.name.trim_start_matches('/')
        }) else {
            continue;
        };
        let (line, character) = line_and_character(&unit.content, diagnostic.span.start);
        from_loader.push(BaselineDiagnostic {
            file: unit.name.clone(),
            line: line + 1,
            column: character + 1,
            code: diagnostic.message.code(),
        });
    }
    // Match the type producer's module host, JSDoc tables, and compiler options.
    // Without the tables, annotated JavaScript can silently check as `any` or
    // infer only from its initializer instead of its declared type.
    let mut checker = crate::types_producer::configured_checker(&program);

    // `set_checked_files` before the first `check_source_file`, because the set
    // is a property of the program: a rule that asks "is this declaration in a
    // library" would otherwise get an answer that depends on how far the loop
    // below had got.
    let mut own_files = Vec::new();
    for unit in &test.files {
        if tsr_parser::ScriptKind::from_file_name(&unit.name) == tsr_parser::ScriptKind::Json {
            continue;
        }
        if let Some(file) = program.source_file(&unit.name)
            && let Some(id) = file.source_file().node_id
        {
            own_files.push(id);
        }
    }
    checker.set_checked_files(own_files);

    // `Program.getBindAndCheckDiagnostics`: diagnose canonical source files,
    // not every original input spelling. A package redirect may make an input
    // resolve to another unit's AST; attributing its spans to the redirect's
    // name reports a diagnostic in the wrong file.
    let current_directory = tsr_path::get_normalized_absolute_path(
        test.current_directory.as_deref().unwrap_or("/"),
        "/",
    );
    let case_sensitive = test
        .options
        .get("usecasesensitivefilenames")
        .is_none_or(|value| !value.eq_ignore_ascii_case("false"));
    let names: std::collections::HashMap<_, _> = test
        .files
        .iter()
        .map(|unit| (tsr_path::to_path(&unit.name, &current_directory, case_sensitive), &unit.name))
        .collect();
    let mut visited = std::collections::HashSet::new();
    let mut units = Vec::new();
    for unit in &test.files {
        if tsr_parser::ScriptKind::from_file_name(&unit.name) == tsr_parser::ScriptKind::Json {
            continue;
        }
        let Some(file) = program.source_file(&unit.name) else { continue };
        let Some(id) = file.source_file().node_id else { continue };
        if !visited.insert(id) {
            continue;
        }
        // Upstream's parser sets `NodeFlagsAmbient` on every node of a
        // declaration file; this port's does not, so the bit is supplied here.
        // `.d.ts` / `.d.mts` / `.d.cts`, which is `tspath.IsDeclarationFileName`.
        let declaration_file = unit.name.ends_with(".d.ts")
            || unit.name.ends_with(".d.mts")
            || unit.name.ends_with(".d.cts");
        checker.check_source_file(
            id,
            tsr_checker::check::FileContext {
                ambient: declaration_file,
                has_parse_errors: !file.diagnostics().is_empty(),
            },
        );
        let canonical_name = names.get(file.path()).copied().unwrap_or(&unit.name);
        units.push((id, canonical_name.clone(), file.text()));
    }

    let mut out = Vec::new();
    // `SortAndDeduplicateDiagnostics` (`compiler/program.go:1454`), which the
    // **test harness** applies to both halves of every baseline it writes
    // (`harnessutil.go:645` and `:661`). Every `.errors.txt` in the corpus is a
    // deduplicated list, and this port compared an undeduplicated one against it
    // for the whole workstream.
    //
    // **The key is upstream's `EqualDiagnosticsNoRelatedInfo`: file, the whole
    // span, the code and the arguments.** Deduplicating on the *printed* form —
    // file, line, column, code — is too coarse and cost two cases:
    // `commaOperator1`'s baseline records three TS2695 at `(1,11)` which differ
    // only in span **length**, a field the textual form drops. §995.
    let mut seen: std::collections::HashSet<(tsr_ast::NodeId, u32, u32, u32, Vec<String>)> =
        std::collections::HashSet::new();
    for (file, diagnostic) in checker.diagnostics() {
        if !seen.insert((
            *file,
            diagnostic.span.start,
            diagnostic.span.end,
            diagnostic.message.code(),
            diagnostic.args.clone(),
        )) {
            continue;
        }
        let Some((_, unit_name, source)) = units.iter().find(|(id, _, _)| id == file) else {
            continue;
        };
        let (line, character) = line_and_character(source, diagnostic.span.start);
        out.push(BaselineDiagnostic {
            file: unit_name.clone(),
            line: line + 1,
            column: character + 1,
            code: diagnostic.message.code(),
        });
    }
    out.extend(from_loader);
    out
}

/// A one-line summary of how the two sets differ.
///
/// **Multiset, not set.** The comparison this explains is `Vec == Vec` over sorted
/// diagnostics, so a case expecting the *same* diagnostic twice and getting it
/// once is a difference. An earlier version filtered with `Vec::contains`, which
/// answers "is it present at all": both differences came back empty and the
/// function panicked on its own `expect`. It stayed hidden until the binder began
/// reporting a redeclaration on every declaration involved, which is exactly when
/// duplicate positions started to occur.
fn summarise(expected: &[BaselineDiagnostic], actual: &[BaselineDiagnostic]) -> String {
    let missing = surplus(expected, actual);
    let extra = surplus(actual, expected);

    // An unexpected diagnostic is reported in preference to a missing one: with no
    // checker, missing is the expected state and extra is a defect we own today.
    if let Some(first) = extra.first() {
        return format!(
            "{} unexpected (first TS{} at {}({},{})), {} missing",
            extra.len(),
            first.code,
            first.file,
            first.line,
            first.column,
            missing.len()
        );
    }
    match missing.first() {
        Some(first) => format!(
            "{} missing (first TS{} at {}({},{}))",
            missing.len(),
            first.code,
            first.file,
            first.line,
            first.column
        ),
        // Unreachable while the caller compares sorted vectors, but returning a
        // sentence beats an `expect` in a harness that runs over 12,444 cases.
        None => "sets differ but no diagnostic does".to_string(),
    }
}

/// The elements of `left` that `right` does not have *as many of*.
///
/// Both inputs are sorted, so this is a merge rather than a scan.
fn surplus<'a>(
    left: &'a [BaselineDiagnostic],
    right: &[BaselineDiagnostic],
) -> Vec<&'a BaselineDiagnostic> {
    let mut out = Vec::new();
    let mut index = 0usize;
    for item in left {
        while index < right.len() && right[index] < *item {
            index += 1;
        }
        if index < right.len() && right[index] == *item {
            index += 1;
        } else {
            out.push(item);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostic(code: u32, line: u32) -> BaselineDiagnostic {
        BaselineDiagnostic { file: "a.ts".into(), line, column: 1, code }
    }

    #[test]
    fn an_unexpected_diagnostic_is_reported_before_a_missing_one() {
        // With no checker almost every case is "missing"; a false positive is a
        // defect this port owns today, so it must not be buried behind the count.
        let expected = vec![diagnostic(2304, 1), diagnostic(2322, 2)];
        let actual = vec![diagnostic(1005, 9)];
        let summary = summarise(&expected, &actual);
        assert!(summary.starts_with("1 unexpected (first TS1005 at a.ts(9,1))"), "{summary}");
        assert!(summary.ends_with("2 missing"), "{summary}");
    }

    #[test]
    fn the_same_diagnostic_expected_twice_and_seen_once_is_a_difference() {
        // Multiset, not set. This is the case that made the previous `contains`
        // based version panic on its own `expect`.
        let expected = vec![diagnostic(2300, 3), diagnostic(2300, 3)];
        let actual = vec![diagnostic(2300, 3)];
        assert_eq!(summarise(&expected, &actual), "1 missing (first TS2300 at a.ts(3,1))");
    }

    #[test]
    fn a_purely_missing_set_names_the_first_code() {
        let expected = vec![diagnostic(2304, 3), diagnostic(2322, 7)];
        assert_eq!(summarise(&expected, &[]), "2 missing (first TS2304 at a.ts(3,1))".to_string());
    }

    #[test]
    fn redirected_package_diagnostics_keep_the_canonical_file_name() {
        let root = crate::repo_root();
        if !root.join("vendor/typescript-go/_submodules/TypeScript/tests/cases").is_dir() {
            eprintln!("skipping: TypeScript submodule absent");
            return;
        }
        let cases = crate::Corpus::from_repo_root(&root).discover().expect("corpus");
        let case = cases
            .iter()
            .find(|case| case.name == "compiler/duplicatePackage_globalMerge")
            .expect("duplicate-package control");
        let baseline = case.expected_errors().expect("baseline").expect("errors baseline");
        let test = case.load().expect("case");
        let mut expected = errors_baseline::parse(&baseline);
        let mut actual = reported_for(&test);
        expected.sort_unstable();
        actual.sort_unstable();
        assert_eq!(actual, expected);
    }
}
