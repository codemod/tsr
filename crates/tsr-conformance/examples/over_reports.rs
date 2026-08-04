//! Every diagnostic we emit that upstream does not, bucketed by code.
//!
//! Diagnostic tool, not a suite. This is the "what must be cleaned before the
//! checker lands" list, and it is the *only* question the `diagnostics` suite
//! cannot answer directly: that suite reports a case as failing whether we emitted
//! too much or too little, and with no checker almost every case fails for too
//! little. A binder or parser false positive, though, cannot be removed by adding
//! a checker — it would persist underneath the checker's output and corrupt the
//! suite exactly when it becomes the checker's gate.
//!
//! Splits by component so the two owners are separable: a code the scanner or
//! parser raises is a parser bug (`bd tsr-pum.11`), a code the binder raises is a
//! binder bug (`bd tsr-y4u`).

use std::collections::{BTreeMap, BTreeSet};

use tsr_conformance::{
    Corpus,
    errors_baseline::{self, BaselineDiagnostic},
    repo_root,
    symbols_baseline::line_and_character,
};
use tsr_parser::{ParsedFile, ScriptKind};

/// One over-reported code: how many diagnostics, and how many distinct cases.
#[derive(Default)]
struct Bucket {
    diagnostics: usize,
    cases: BTreeSet<String>,
    /// Whether upstream reports *something* at the same position, which means the
    /// conflict is real and only the code or the owner is wrong.
    shadowed: usize,
}

fn main() {
    let cases = Corpus::from_repo_root(&repo_root()).discover().expect("discover");
    let mut parse_time: BTreeMap<u32, Bucket> = BTreeMap::new();
    let mut bind_time: BTreeMap<u32, Bucket> = BTreeMap::new();
    let mut total_cases_with_any = BTreeSet::new();
    let mut judged_by_diagnostics = BTreeSet::new();
    let mut capped_diagnostics_cases = BTreeSet::new();

    for case in &cases {
        if case.has_varied_errors() || case.has_known_divergence() || !case.has_any_baseline() {
            continue;
        }
        let Ok(baseline) = case.expected_errors() else { continue };
        let expected: Vec<BaselineDiagnostic> =
            baseline.as_deref().map(errors_baseline::parse).unwrap_or_default();
        // The `diagnostics` suite judges only cases that expect at least one
        // diagnostic, so that is the denominator an over-report actually caps.
        let in_diagnostics_suite = !expected.is_empty();
        if in_diagnostics_suite {
            judged_by_diagnostics.insert(case.name.clone());
        }
        let Ok(test) = case.load() else { continue };

        for unit in &test.files {
            let kind = ScriptKind::from_file_name(&unit.name);
            if kind == ScriptKind::Json {
                continue;
            }
            let parsed = ParsedFile::parse_with_script_kind(unit.content.clone(), kind);

            // (code, line, column, from_binder)
            let mut ours: Vec<(u32, u32, u32, bool)> = Vec::new();
            for diagnostic in parsed.diagnostics() {
                let (line, character) = line_and_character(&unit.content, diagnostic.span.start);
                ours.push((diagnostic.message.code(), line + 1, character + 1, false));
            }
            parsed.with_ast(|file| {
                let bound = tsr_binder::bind(
                    file,
                    parsed.nodes(),
                    tsr_binder::FileInfo { name: &unit.name, text: parsed.source() },
                );
                for diagnostic in bound.diagnostics() {
                    let (line, character) =
                        line_and_character(&unit.content, diagnostic.span.start);
                    ours.push((diagnostic.message.code(), line + 1, character + 1, true));
                }
            });

            for (code, line, column, from_binder) in ours {
                let matched = expected.iter().any(|e| {
                    e.file == unit.name && e.line == line && e.column == column && e.code == code
                });
                if matched {
                    continue;
                }
                let shadowed = expected
                    .iter()
                    .any(|e| e.file == unit.name && e.line == line && e.column == column);
                let table = if from_binder { &mut bind_time } else { &mut parse_time };
                let bucket = table.entry(code).or_default();
                bucket.diagnostics += 1;
                bucket.cases.insert(case.name.clone());
                if shadowed {
                    bucket.shadowed += 1;
                }
                total_cases_with_any.insert(case.name.clone());
                if in_diagnostics_suite {
                    capped_diagnostics_cases.insert(case.name.clone());
                }
            }
        }
    }

    for (label, table) in [("PARSER / SCANNER", &parse_time), ("BINDER", &bind_time)] {
        let mut sorted: Vec<_> = table.iter().collect();
        sorted.sort_by_key(|(_, b)| std::cmp::Reverse(b.diagnostics));
        let diagnostics: usize = sorted.iter().map(|(_, b)| b.diagnostics).sum();
        let mut cases = BTreeSet::new();
        for (_, bucket) in &sorted {
            cases.extend(bucket.cases.iter().cloned());
        }
        println!(
            "\n=== {label}: {diagnostics} over-reported diagnostics over {} cases ===",
            cases.len()
        );
        println!("{:>7}  {:>6}  {:>8}  code", "diags", "cases", "shadowed");
        for (code, bucket) in sorted.iter().take(25) {
            println!(
                "{:>7}  {:>6}  {:>8}  TS{code}",
                bucket.diagnostics,
                bucket.cases.len(),
                bucket.shadowed
            );
        }
        if sorted.len() > 25 {
            println!("  … and {} more codes", sorted.len() - 25);
        }
    }
    println!(
        "\n{} cases carry at least one over-report (out of {} judged).",
        total_cases_with_any.len(),
        cases.len()
    );
    println!("'shadowed' = upstream reports something else at our exact position.");
    let judged = judged_by_diagnostics.len();
    let capped = capped_diagnostics_cases.len();
    println!(
        "\nOf the {judged} cases the `diagnostics` suite judges, {capped} carry an over-report \
         we emit and upstream does not ({:.1}%). Those cannot pass however good the checker is, \
         so they cap that suite at {:.1}%.",
        100.0 * capped as f64 / judged as f64,
        100.0 * (judged - capped) as f64 / judged as f64
    );
}
