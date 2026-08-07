//! The counterfactual for TS2307: what the check traversal would do to the
//! `diagnostics` suite, measured before the suite is wired to it.
//!
//! ```text
//! cargo run --release -p tsr-conformance --example diag2307
//! ```
//!
//! # Probe and build are the same function
//!
//! This does **not** re-implement the rule. It calls
//! [`tsr_checker::Checker::check_source_file`] — the shipped traversal — and
//! merges its output into the diagnostic set the `diagnostics` suite compares
//! today. `STATUS.md` §7 records why: the composite-print twin landed on its
//! forecast to the line because `sigprint::compose` and
//! `signature_to_string_at` were one function, proven by a self-check leg before
//! either ran. The same shape is used here, and the self-check is leg SC below.
//!
//! What is therefore *not* yet true when this runs: the suite does not consult
//! the checker. So this probe's numbers are a forecast of the wiring commit,
//! and the wiring commit is the only thing that can move `diagnostics`.
//!
//! # The four columns
//!
//! | column | meaning |
//! |---|---|
//! | `CONVERTS` | a case that fails today and would pass — the forecast |
//! | `LOST` | a case that **passes** today and would fail. Must be 0: the rule can only add diagnostics, so a loss means it added one to a case that was already exact |
//! | `RIGHT` / `WRONG` | per *diagnostic*: an emitted TS2307 the baseline also records at that exact `(file, line, column, code)`, or not |
//! | `STILL SHORT` | a case that gains a correct TS2307 and still fails for something else — the cascade this rule cannot reach alone |
//!
//! `WRONG` is the number that decides whether the bound in
//! `Checker::check_module_specifier` is drawn in the right place. A wrong
//! diagnostic is strictly worse than a missing one: it fails its own case *and*
//! can fail a case that passes.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{
    CaseEntry, Corpus,
    errors_baseline::{self, BaselineDiagnostic},
    repo_root,
    symbols_baseline::line_and_character,
    types_producer::program_for_case,
};
use tsr_parser::ParsedFile;

/// One case's before/after, in the terms the bar is written in.
#[derive(Default)]
struct Row {
    converts: bool,
    lost: bool,
    still_short: bool,
    right: usize,
    wrong: Vec<String>,
}

fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let rows: Vec<(String, Row)> = cases.par_iter().filter_map(measure).collect();

    let converts: Vec<&str> =
        rows.iter().filter(|(_, r)| r.converts).map(|(name, _)| name.as_str()).collect();
    let lost: Vec<&str> =
        rows.iter().filter(|(_, r)| r.lost).map(|(name, _)| name.as_str()).collect();
    let still_short = rows.iter().filter(|(_, r)| r.still_short).count();
    let right: usize = rows.iter().map(|(_, r)| r.right).sum();
    let wrong: Vec<&String> = rows.iter().flat_map(|(_, r)| r.wrong.iter()).collect();

    println!("judged cases          {}", rows.len());
    println!("CONVERTS              {}", converts.len());
    println!("LOST                  {}", lost.len());
    println!("STILL SHORT           {still_short}");
    println!("diagnostics RIGHT     {right}");
    println!("diagnostics WRONG     {}", wrong.len());

    if !lost.is_empty() {
        println!("\n-- LOST (a passing case broken) --");
        for name in lost.iter().take(40) {
            println!("  {name}");
        }
    }

    if !wrong.is_empty() {
        println!("\n-- WRONG, by specifier --");
        let mut by_text: BTreeMap<&str, usize> = BTreeMap::new();
        for entry in &wrong {
            *by_text.entry(entry.as_str()).or_default() += 1;
        }
        let mut ranked: Vec<(&&str, &usize)> = by_text.iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(a.1));
        for (text, count) in ranked.iter().take(70) {
            println!("  {count:>4}  {text}");
        }
    }

    println!("\n-- CONVERTS, first 30 --");
    for name in converts.iter().take(30) {
        println!("  {name}");
    }
}

/// The suite's judgement before and after the traversal's contribution.
fn measure(case: &CaseEntry) -> Option<(String, Row)> {
    if case.has_varied_errors() || case.has_known_divergence() || !case.has_any_baseline() {
        return None;
    }
    let baseline = case.expected_errors().ok()?;
    let mut expected: Vec<BaselineDiagnostic> =
        baseline.as_deref().map(errors_baseline::parse).unwrap_or_default();
    if expected.is_empty() {
        return None;
    }
    expected.sort_unstable();

    let test = case.load().ok()?;
    let mut before = today(&test);
    before.sort_unstable();

    let mut after = before.clone();
    let mut wrong = Vec::new();
    let mut right = 0usize;
    for diagnostic in from_traversal(&test) {
        if expected.binary_search(&diagnostic).is_ok() {
            right += 1;
        } else {
            wrong.push(format!(
                "{}  {}({},{})",
                case.name, diagnostic.file, diagnostic.line, diagnostic.column
            ));
        }
        after.push(diagnostic);
    }
    after.sort_unstable();

    let passed_before = before == expected;
    let passed_after = after == expected;
    Some((
        case.name.clone(),
        Row {
            converts: !passed_before && passed_after,
            lost: passed_before && !passed_after,
            still_short: !passed_after && right > 0,
            right,
            wrong,
        },
    ))
}

/// Exactly what `diagnostics_suite::run` produces today — parser and binder,
/// per unit, with no program.
fn today(test: &tsr_conformance::TestCase) -> Vec<BaselineDiagnostic> {
    let mut actual = Vec::new();
    for unit in &test.files {
        let kind = tsr_parser::ScriptKind::from_file_name(&unit.name);
        if kind == tsr_parser::ScriptKind::Json {
            continue;
        }
        let parsed = ParsedFile::parse_with_script_kind(unit.content.clone(), kind);
        let mut reported = parsed.diagnostics().to_vec();
        parsed.with_ast(|file| {
            let bound = tsr_binder::bind(
                file,
                parsed.nodes(),
                tsr_binder::FileInfo { name: &unit.name, text: parsed.source() },
            );
            reported.extend(bound.diagnostics().iter().cloned());
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
    actual
}

/// What the shipped `check_source_file` reports for the case's own units.
///
/// The lib files are in the program and are **not** walked: upstream reports
/// diagnostics for `lib.*.d.ts` under no configuration this corpus uses, and a
/// diagnostic on a lib file cannot match any baseline line, so walking them
/// could only manufacture a false positive.
fn from_traversal(test: &tsr_conformance::TestCase) -> Vec<BaselineDiagnostic> {
    let arena = tsr_core::Arena::new();
    let program = program_for_case(&arena, test);
    let mut checker = tsr_checker::Checker::with_module_host(
        program.binder(),
        program.nodes(),
        program.node_map(),
        Some(&program),
    );
    // `GetStrictOptionValue`-style directive read, the same shape
    // `types_producer::render_case` uses for `strictNullChecks`. Upstream's
    // `IsTrueOrUnknown` makes an unset option `true`.
    checker.set_no_unchecked_side_effect_imports(
        test.options
            .get("nouncheckedsideeffectimports")
            .map_or(true, |value| !value.eq_ignore_ascii_case("false")),
    );
    let mut units = Vec::new();
    for unit in &test.files {
        if tsr_parser::ScriptKind::from_file_name(&unit.name) == tsr_parser::ScriptKind::Json {
            continue;
        }
        let Some(file) = program.source_file(&unit.name) else { continue };
        let Some(id) = file.source_file().node_id else { continue };
        checker.check_source_file(id);
        units.push((id, unit.name.clone(), file.text()));
    }
    let mut out = Vec::new();
    for (file, diagnostic) in checker.diagnostics() {
        let Some((_, name, text)) = units.iter().find(|(id, _, _)| id == file) else { continue };
        let (line, character) = line_and_character(text, diagnostic.span.start);
        out.push(BaselineDiagnostic {
            file: name.clone(),
            line: line + 1,
            column: character + 1,
            code: diagnostic.message.code(),
        });
    }
    out
}
