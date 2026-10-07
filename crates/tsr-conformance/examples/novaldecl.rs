//! What is behind `depend.rs`'s `resolves, but the symbol has no value
//! declaration` — **2,404 lines, want-any 687 (28.6%), 447 cases**, the
//! largest gap root nobody has opened.
//!
//! `get_type_of_symbol`'s flag dispatch is complete (accessor, variable /
//! property, function / class / enum / module, enum member, alias, export
//! marker), so these lines are not a missing arm in that switch. The reason
//! is emitted purely on `value_declaration.is_none()`, so the question is
//! **which symbol shapes reach it** — and, for each, whether the type is
//! computable by some route the checker is not taking.
//!
//! Reports each line by the symbol's `SymbolFlags`, how many declarations it
//! has, and the kind of the first one. A shape with declarations but no
//! *value* declaration is a binder question; a shape with none at all is a
//! resolution question. Splitting them is the whole point.
//!
//! Controls: C1 every classified line answers `errorType` today (expect 0).
//! C2 buckets sum to classified.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::Node;
use tsr_binder::SymbolFlags;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

#[derive(Default)]
struct Report {
    classified: usize,
    forms: BTreeMap<String, usize>,
    wants_any: BTreeMap<String, usize>,
    cases: BTreeMap<String, usize>,
    c1_not_gap: usize,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        self.classified += other.classified;
        self.c1_not_gap += other.c1_not_gap;
        for (k, n) in &other.forms {
            *self.forms.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.wants_any {
            *self.wants_any.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.cases {
            *self.cases.entry(k.clone()).or_default() += n;
        }
    }
}

fn measure(case: &tsr_conformance::CaseEntry) -> Option<Report> {
    if case.has_varied_types() || case.has_known_divergence() {
        return None;
    }
    let text = case.expected_types()?;
    let expected = types_baseline::parse(&text);
    if types_baseline::assertion_count(&expected) == 0 {
        return None;
    }
    let parsed = case.load().ok()?;
    let arena = tsr_core::Arena::new();
    let (program, ours, ids) =
        types_producer::assertions_for_case_with_ids(&arena, &parsed, &parsed.files.as_slice());
    let nodes = program.nodes();
    let map = program.node_map();
    let bound = program.binder();
    let mut checker = tsr_checker::Checker::with_module_host(bound, nodes, map, Some(&program));
    let error = checker.intrinsics().error;

    let mut report = Report::default();
    for (index, expected_file) in expected.iter().enumerate() {
        let (Some(our_file), Some(line_ids)) = (ours.get(index), ids.get(index)) else { continue };
        if our_file.len() != line_ids.len() {
            continue;
        }
        for (position, want) in expected_file.assertions.iter().enumerate() {
            let Some(got) = our_file.get(position) else { continue };
            if want.text == got.line() {
                continue;
            }
            if want.text.strip_prefix(&format!("{} : ", got.text)).is_none() {
                continue;
            }
            if got.type_string != "error" {
                continue;
            }
            let id = line_ids[position];
            let Some(Node::Identifier(identifier)) = map.get(id) else { continue };
            // A reference, not a declaration name — the same shape `depend.rs`
            // classifies.
            if nodes.parent(id).and_then(|p| map.get(p)).and_then(|p| p.name_id()) == Some(id) {
                continue;
            }
            let Some(symbol) =
                bound.resolve_name(nodes, map, id, identifier.text, SymbolFlags::VALUE)
            else {
                continue;
            };
            let entry = bound.symbols().get(symbol);
            if entry.value_declaration.is_some() {
                continue;
            }
            report.classified += 1;
            if types_producer::type_id_at_location(&mut checker, bound, nodes, map, id) != error {
                report.c1_not_gap += 1;
            }

            let first_kind = entry.declarations.first().map_or_else(
                || "NO DECLARATIONS AT ALL".to_string(),
                |&d| format!("{:?}", nodes.kind(d)),
            );
            // Flags as a readable set — the shape is the finding.
            let mut names = Vec::new();
            for (bit, label) in [
                (SymbolFlags::ALIAS, "ALIAS"),
                (SymbolFlags::INTERFACE, "INTERFACE"),
                (SymbolFlags::TYPE_ALIAS, "TYPE_ALIAS"),
                (SymbolFlags::CLASS, "CLASS"),
                (SymbolFlags::FUNCTION, "FUNCTION"),
                (SymbolFlags::PROPERTY, "PROPERTY"),
                (SymbolFlags::VARIABLE, "VARIABLE"),
                (SymbolFlags::VALUE_MODULE, "VALUE_MODULE"),
                (SymbolFlags::NAMESPACE_MODULE, "NAMESPACE_MODULE"),
                (SymbolFlags::ENUM_MEMBER, "ENUM_MEMBER"),
                (SymbolFlags::EXPORT_VALUE, "EXPORT_VALUE"),
                (SymbolFlags::TYPE_PARAMETER, "TYPE_PARAMETER"),
            ] {
                if entry.flags.intersects(bit) {
                    names.push(label);
                }
            }
            if names.is_empty() {
                names.push("(none of the listed bits)");
            }
            let form = format!(
                "{} | {} decl(s), first {first_kind}",
                names.join("+"),
                entry.declarations.len()
            );
            let wants_any = want.text.rsplit_once(" : ").is_some_and(|(_, a)| a == "any");
            if wants_any {
                *report.wants_any.entry(form.clone()).or_default() += 1;
            }
            *report.forms.entry(form).or_default() += 1;
            *report.cases.entry(case.name.clone()).or_default() += 1;
        }
    }
    Some(report)
}

fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    assert!(corpus.is_available(), "corpus missing");
    let cases = corpus.discover().expect("cases");
    let mut report = Report::default();
    for partial in cases.par_iter().filter_map(measure).collect::<Vec<_>>() {
        report.merge(&partial);
    }

    println!("# novaldecl — symbols that resolve as VALUE with no value declaration\n");
    println!("classified: {}\n", report.classified);
    let mut rows: Vec<_> = report.forms.iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    let mut sum = 0;
    for (form, n) in rows.into_iter().take(18) {
        let any = report.wants_any.get(form).copied().unwrap_or(0);
        sum += n;
        println!("  {n:>6}  want-any {any:>5}  {form}");
    }
    let total: usize = report.forms.values().sum();
    println!("\n  (top 18 shown: {sum} of {total})");
    println!("  C1 classified-but-not-gap: {}  (expect 0)", report.c1_not_gap);
    println!("  C2 buckets sum {total} vs classified {}", report.classified);

    let mut cases: Vec<_> = report.cases.iter().collect();
    cases.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    println!("\n## Top cases\n");
    for (case, n) in cases.into_iter().take(8) {
        println!("  {n:>6}  {case}");
    }
}
