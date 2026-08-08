//! Bucket the remaining TS2300 over-reports by root cause.
//!
//! Diagnostic tool, not a suite. "We emitted a TS2300 upstream did not" has at
//! least three causes and they need completely different fixes:
//!
//! - **position** — upstream reports TS2300 for the same name in the same file at
//!   a different line/column. Ours to fix, no checker involved.
//! - **code** — upstream reports *something else* at our exact position, so the
//!   conflict is real and the message is wrong.
//! - **genuine** — upstream reports nothing about that name in that file at all.
//!
//! Only the third is a true over-report, and only the third can plausibly be
//! something the checker would resolve on its own.

use std::collections::BTreeMap;

use tsr_conformance::{
    Corpus,
    errors_baseline::{self, BaselineDiagnostic},
    repo_root,
    symbols_baseline::line_and_character,
};
use tsr_parser::{ParsedFile, ScriptKind};

/// Whether a unit is JavaScript, for the `checkJs` split.
///
/// `ends_with` on the name rather than `Path::extension`: these are corpus unit
/// names, and the corpus writes them lowercase.
#[allow(clippy::case_sensitive_file_extension_comparisons)]
fn is_javascript(name: &str) -> bool {
    name.ends_with(".js") || name.ends_with(".jsx")
}

fn main() {
    let cases = Corpus::from_repo_root(&repo_root()).discover().expect("discover");
    let mut buckets: BTreeMap<&str, usize> = BTreeMap::new();
    let mut samples: BTreeMap<&str, Vec<String>> = BTreeMap::new();

    for case in &cases {
        if case.has_varied_errors() || case.has_known_divergence() || !case.has_any_baseline() {
            continue;
        }
        let Ok(baseline) = case.expected_errors() else { continue };
        let expected: Vec<BaselineDiagnostic> =
            baseline.as_deref().map(errors_baseline::parse).unwrap_or_default();
        if expected.is_empty() {
            continue;
        }
        let Ok(test) = case.load() else { continue };

        for unit in &test.files {
            let kind = ScriptKind::from_file_name(&unit.name);
            if kind == ScriptKind::Json {
                continue;
            }
            let parsed = ParsedFile::parse_with_script_kind(unit.content.clone(), kind);
            let mut ours = Vec::new();
            parsed.with_ast_and_arena(|file, arena| {
                let bound = tsr_binder::bind(
                    arena,
                    file,
                    parsed.nodes(),
                    tsr_binder::FileInfo { name: &unit.name, text: parsed.source() },
                );
                for diagnostic in bound.diagnostics() {
                    if diagnostic.message.code() != 2300 {
                        continue;
                    }
                    let (line, character) =
                        line_and_character(&unit.content, diagnostic.span.start);
                    ours.push((line + 1, character + 1));
                }
            });

            for (line, column) in ours {
                let exact = expected.iter().any(|e| {
                    e.file == unit.name && e.line == line && e.column == column && e.code == 2300
                });
                if exact {
                    continue;
                }
                let same_position_other_code = expected
                    .iter()
                    .find(|e| e.file == unit.name && e.line == line && e.column == column);
                let ts2300_elsewhere =
                    expected.iter().any(|e| e.file == unit.name && e.code == 2300);

                let bucket = if let Some(other) = same_position_other_code {
                    let key: &'static str = Box::leak(
                        format!("code: upstream says TS{} here", other.code).into_boxed_str(),
                    );
                    key
                } else if ts2300_elsewhere {
                    "position: upstream has TS2300 in this file, elsewhere"
                } else if is_javascript(&unit.name) {
                    // Upstream reports nothing for a JS unit unless the case sets
                    // `@checkJs`, so this is a property of the program rather than
                    // of the binder.
                    let checked = test.options.get("checkjs").map(String::as_str) == Some("true");
                    if checked {
                        "genuine: JS unit with @checkJs"
                    } else {
                        "program: JS unit, no @checkJs — upstream diagnoses nothing"
                    }
                } else {
                    "genuine: upstream reports no TS2300 in this file"
                };
                *buckets.entry(bucket).or_default() += 1;
                let entry = samples.entry(bucket).or_default();
                if entry.len() < 3 {
                    entry.push(format!("{} {}({line},{column})", case.name, unit.name));
                }
            }
        }
    }

    let mut sorted: Vec<_> = buckets.into_iter().collect();
    sorted.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (bucket, count) in &sorted {
        println!("{count:6}  {bucket}");
        for sample in samples.get(bucket).into_iter().flatten() {
            println!("        {sample}");
        }
    }
}
