//! Bucket the remaining TS2300 over-reports by the **construct** that declared
//! the colliding names, not by where the diagnostic landed.
//!
//! Diagnostic tool, not a suite. `ts2300_classes` answers "does upstream say
//! something else here?", which is the right first question and the wrong second
//! one: once the answer is "upstream says nothing at all" for 4,345 of them, the
//! position is no longer informative. What is informative is the pair of
//! declaration kinds that collided — that names the block of `declareSymbol`, or
//! the caller passing the wrong flags, that is responsible.
//!
//! Attribution is by containment: our redeclaration diagnostics are anchored on a
//! declaration's *name*, so the responsible declaration is the smallest one whose
//! span contains the diagnostic's span.

use std::collections::BTreeMap;

use tsr_conformance::{
    Corpus,
    errors_baseline::{self, BaselineDiagnostic},
    repo_root,
    symbols_baseline::line_and_character,
};
use tsr_parser::{ParsedFile, ScriptKind};

fn main() {
    let cases = Corpus::from_repo_root(&repo_root()).discover().expect("discover");
    let mut buckets: BTreeMap<String, usize> = BTreeMap::new();
    let mut samples: BTreeMap<String, Vec<String>> = BTreeMap::new();

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
            let nodes = parsed.nodes();
            let mut found: Vec<(String, u32, u32)> = Vec::new();
            parsed.with_ast(|file| {
                let bound = tsr_binder::bind(
                    file,
                    nodes,
                    tsr_binder::FileInfo { name: &unit.name, text: parsed.source() },
                );

                // Every declaration the binder filed, so a diagnostic can be
                // attributed to the construct that produced it.
                let mut declarations: Vec<(tsr_core::Span, tsr_ast::SyntaxKind)> = Vec::new();
                for (_, symbol) in bound.symbols().iter() {
                    for declaration in &symbol.declarations {
                        declarations.push((nodes.span(*declaration), nodes.kind(*declaration)));
                    }
                }

                for diagnostic in bound.diagnostics() {
                    if diagnostic.message.code() != 2300 {
                        continue;
                    }
                    let (line, character) =
                        line_and_character(&unit.content, diagnostic.span.start);
                    let (line, column) = (line + 1, character + 1);

                    // Only the over-reports: upstream says nothing at this
                    // position and no TS2300 anywhere in the unit.
                    let accounted = expected.iter().any(|e| {
                        e.file == unit.name
                            && ((e.line == line && e.column == column) || e.code == 2300)
                    });
                    if accounted {
                        continue;
                    }

                    let owner = declarations
                        .iter()
                        .filter(|(span, _)| {
                            span.start <= diagnostic.span.start && diagnostic.span.end <= span.end
                        })
                        .min_by_key(|(span, _)| span.end - span.start)
                        .map(|(_, kind)| *kind);
                    let label = match owner {
                        Some(kind) => format!("{kind:?}"),
                        None => "«no enclosing declaration»".to_string(),
                    };
                    found.push((label, line, column));
                }
            });

            for (label, line, column) in found {
                *buckets.entry(label.clone()).or_default() += 1;
                let entry = samples.entry(label).or_default();
                if entry.len() < 2 {
                    entry.push(format!("{} {}({line},{column})", case.name, unit.name));
                }
            }
        }
    }

    let mut sorted: Vec<_> = buckets.into_iter().collect();
    sorted.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    let total: usize = sorted.iter().map(|(_, n)| *n).sum();
    println!("{total} TS2300 over-reports, by declaring construct:");
    for (bucket, count) in &sorted {
        println!("{count:6}  {bucket}");
        for sample in samples.get(bucket).into_iter().flatten() {
            println!("        {sample}");
        }
    }
}
