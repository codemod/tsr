//! Judging the binder against upstream's `.symbols` baselines.
//!
//! See [`crate::symbols_baseline`] for the format and why it is the right oracle.
//!
//! # What is compared, and what is not
//!
//! For each symbol upstream names, the baseline gives its declaration sites. This
//! suite asks: **did we create a symbol of that name, declared on the same
//! lines?** That tests symbol creation, declaration merging (one symbol with two
//! declarations rather than two symbols), and scope placement, all of which are
//! model-independent — the baseline says nothing about how either side stores a
//! symbol table, which matters because ours is deliberately not upstream's.
//!
//! It does **not** yet test resolution: which *occurrence* binds to which symbol.
//! The baseline has that, but the occurrence's own position is implicit in the
//! layout (the annotation follows the line it describes), so recovering it means
//! reconstructing the interleaving. That is worth doing and is filed.
//!
//! **Positions are full starts, recovered rather than recorded.** Upstream's
//! `Decl(file, line, character)` is the declaration node's *full start* — where
//! its leading trivia begins — not where its first token does. For a member on
//! the line after `class C {`, upstream reports the position of the `{`. We do
//! not store full start on nodes, so [`symbols_baseline::full_start`] walks back
//! over trivia to recover it; that handles whitespace and block comments but not
//! `//` comments, which cannot be recognised scanning backwards. Recording full
//! start properly is filed.
//!
//! Lines only, not columns: the recovery above is exact for the common cases and
//! approximate for the rest, and a column comparison would turn each
//! approximation into a failure attributed to the binder.

use std::collections::BTreeSet;

use crate::{
    corpus::CaseEntry,
    suite::{Outcome, Suite},
    symbols_baseline,
};

/// Does every symbol upstream found exist here, declared on the same lines?
pub struct BinderSymbols;

impl Suite for BinderSymbols {
    fn name(&self) -> &'static str {
        "binder_symbols"
    }

    fn describes(&self) -> &'static str {
        "every symbol in the .symbols baseline exists with the same declaration lines"
    }

    fn run(&self, case: &CaseEntry) -> Outcome {
        let Some(baseline) = case.expected_symbols() else {
            return Outcome::Skipped { reason: "no .symbols baseline".into() };
        };
        if case.has_known_divergence() {
            return Outcome::Skipped {
                reason: "upstream records a known divergence from TypeScript (.diff baseline)"
                    .into(),
            };
        }

        let expected_files = symbols_baseline::parse(&baseline);
        let parsed = match case.load() {
            Ok(parsed) => parsed,
            Err(err) => return Outcome::Failed { reason: format!("{err:#}") },
        };
        // Multi-file cases need per-file attribution that the single-unit path
        // does not have; skipped rather than scored wrongly.
        if parsed.files.len() != 1 || expected_files.len() != 1 {
            return Outcome::Skipped {
                reason: "multi-file case; needs per-file symbol attribution".into(),
            };
        }

        let unit = &parsed.files[0];
        if !crate::scanner_suite::is_typescript_unit(&unit.name) {
            return Outcome::Skipped { reason: "not a TypeScript unit".into() };
        }

        let arena = tsr_core::Arena::new();
        let script_kind = tsr_parser::ScriptKind::from_file_name(&unit.name);
        let result = tsr_parser::parse_with_script_kind(&arena, &unit.content, script_kind);
        // A file we cannot parse tells us nothing about the binder.
        if !result.diagnostics.is_empty() {
            return Outcome::Skipped { reason: "the parser reports errors for this file".into() };
        }
        let bound = tsr_binder::bind(result.source_file, &result.nodes);

        // What we produced, keyed by *qualified* name: the baseline writes a
        // member as `C.foo`, `C[1]`, or `C["bar"]` depending on how it was
        // written, while we store the bare name and a parent link.
        let mut ours: std::collections::HashMap<String, BTreeSet<u32>> =
            std::collections::HashMap::new();
        for (id, symbol) in bound.symbols().iter() {
            let lines = ours.entry(qualified_name(bound.symbols(), id)).or_default();
            for declaration in &symbol.declarations {
                let span = result.nodes.span(*declaration);
                // Upstream reports the *full start*; see `symbols_baseline`.
                let pos = symbols_baseline::full_start(&unit.content, span.start);
                let (line, _) = symbols_baseline::line_and_character(&unit.content, pos);
                lines.insert(line);
            }
            let _ = symbol;
        }

        // What upstream expects, deduplicated: the baseline repeats a symbol once
        // per occurrence, and a symbol is one fact however often it is used.
        // `BTreeMap`, not `HashMap`: the loop below stops after three misses, so
        // hash order would decide *which* three a failing case reports and the
        // committed snapshot would churn on reruns with no code change. A snapshot
        // that moves on its own teaches reviewers to ignore its diff.
        let mut expected: std::collections::BTreeMap<&str, BTreeSet<u32>> =
            std::collections::BTreeMap::new();
        let unit_file = &expected_files[0].file;
        for reference in &expected_files[0].refs {
            if reference.declarations.is_empty() {
                continue;
            }
            // Only symbols declared *in this file*. A reference to `console`
            // resolves to `Decl(lib.dom.d.ts, --, --)`, and we neither load lib
            // files nor could produce those declarations; requiring them would
            // score the absence of a standard library as a binder failure.
            if reference.declarations.iter().any(|d| &d.file != unit_file) {
                continue;
            }
            let lines = expected.entry(reference.symbol.as_str()).or_default();
            for declaration in &reference.declarations {
                lines.insert(declaration.line);
            }
        }
        if expected.is_empty() {
            return Outcome::Skipped { reason: "the baseline names no symbols".into() };
        }

        let mut missing = Vec::new();
        for (name, lines) in &expected {
            match ours.get(*name) {
                None => missing.push(format!("{name}: no symbol")),
                // Ours must *contain* the expected lines rather than equal them:
                // upstream lists only the declarations reachable from a use site,
                // while we hold every declaration of the symbol.
                Some(found) if !lines.is_subset(found) => missing
                    .push(format!("{name}: declared on {found:?}, expected to include {lines:?}")),
                Some(_) => {}
            }
            if missing.len() >= 3 {
                break;
            }
        }

        if missing.is_empty() {
            Outcome::Passed
        } else {
            Outcome::Failed {
                reason: format!("{} symbol(s): {}", missing.len(), missing.join("; ")),
            }
        }
    }
}

/// A symbol's name as the baseline writes it.
///
/// Upstream qualifies a member with its container — `C.foo` — and switches to
/// bracket form when the name is not a plain identifier: `C[1]` for a numeric
/// name, `C["bar"]` for a string one. Reproducing that here rather than stripping
/// it from the baseline keeps the comparison sensitive to *which* container a
/// member ended up in, which is most of what the binder decides.
fn qualified_name(symbols: &tsr_binder::SymbolStore<'_>, id: tsr_binder::SymbolId) -> String {
    let symbol = symbols.get(id);
    let Some(parent) = symbol.parent else {
        return symbol.name.to_string();
    };
    let prefix = qualified_name(symbols, parent);
    let name = symbol.name;
    if is_identifier_like(name) {
        format!("{prefix}.{name}")
    } else if name.chars().all(|c| c.is_ascii_digit()) && !name.is_empty() {
        format!("{prefix}[{name}]")
    } else {
        format!("{prefix}[\"{name}\"]")
    }
}

/// Whether a name can be written after a dot.
fn is_identifier_like(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}
