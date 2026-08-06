//! `bd tsr-6v7` — what is actually outside `SELECTABLE`?
//!
//! `examples/callgate.rs` measured overload selection's own population at
//! 1,095 lines, of which **473 stop at `a parameter type outside SELECTABLE`**
//! and 28 at the argument twin. `calls.rs`'s `SELECTABLE` is deliberately
//! narrower than upstream's `TypeFlagsPrimitive` because overload selection is
//! the first caller that depends on a **negative** answer from the relater,
//! and a `false` is only trustworthy where `isSimpleTypeRelatedTo` decides on
//! flags alone.
//!
//! The open question is whether the excluded space is *cheap* (enums, unique
//! symbols — a few flags) or *expensive* (object types, which need structural
//! assignability). This resolves the real parameter types and reports their
//! shapes, rather than reading the annotations' syntax — the distinction
//! `docs/conventions.md` draws between a match test and a shape test.
//!
//! `SELECTABLE` is reconstructed here from the public `TypeFlags` rather than
//! imported, because it is a private constant; the two are asserted to agree
//! on a spot check in `main` (control C3).
//!
//! Controls: C1 every classified line answers `errorType` today (expect 0).
//! C2 buckets sum to classified.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Node, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_checker::TypeFlags;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// `calls.rs`'s `SELECTABLE`, reconstructed from the public flags.
fn selectable() -> TypeFlags {
    TypeFlags::STRING
        .union(TypeFlags::STRING_LITERAL)
        .union(TypeFlags::NUMBER)
        .union(TypeFlags::NUMBER_LITERAL)
        .union(TypeFlags::BIG_INT)
        .union(TypeFlags::BIG_INT_LITERAL)
        .union(TypeFlags::BOOLEAN)
        .union(TypeFlags::BOOLEAN_LITERAL)
        .union(TypeFlags::VOID)
        .union(TypeFlags::UNDEFINED)
        .union(TypeFlags::NULL)
        .union(TypeFlags::NEVER)
}

#[derive(Default)]
struct Report {
    classified: usize,
    forms: BTreeMap<String, usize>,
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
        types_producer::assertions_for_case_with_ids(&arena, &parsed, &expected);
    let nodes = program.nodes();
    let map = program.node_map();
    let bound = program.binder();
    let mut checker = tsr_checker::Checker::with_module_host(bound, nodes, map, Some(&program));
    let error = checker.intrinsics().error;
    let ok = selectable();

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
            let Some(Node::CallExpression(call)) = map.get(id) else { continue };
            let Some(tsr_ast::Expression::Identifier(callee)) = call.expression else { continue };
            let Some(callee_id) = callee.node_id else { continue };
            let Some(symbol) =
                bound.resolve_name(nodes, map, callee_id, callee.text, SymbolFlags::VALUE)
            else {
                continue;
            };
            // An overload SET: two or more function-like declarations.
            let declarations: Vec<_> = bound
                .symbols()
                .get(symbol)
                .declarations
                .iter()
                .copied()
                .filter(|&d| {
                    matches!(
                        nodes.kind(d),
                        SyntaxKind::FunctionDeclaration | SyntaxKind::MethodDeclaration
                    )
                })
                .collect();
            if declarations.len() < 2 {
                continue;
            }
            report.classified += 1;
            if types_producer::type_id_at_location(&mut checker, bound, nodes, map, id) != error {
                report.c1_not_gap += 1;
            }

            // Resolve every candidate's parameter annotations and find the
            // FIRST that is outside SELECTABLE — that is the one the gate
            // stops on.
            let mut offender: Option<String> = None;
            'outer: for declaration in declarations {
                let parameters = match map.get(declaration) {
                    Some(Node::FunctionDeclaration(f)) => f.parameters,
                    Some(Node::MethodDeclaration(m)) => m.parameters,
                    _ => continue,
                };
                for parameter in parameters {
                    let Some(annotation) = parameter.r#type else {
                        offender = Some("a parameter with no annotation".to_string());
                        break 'outer;
                    };
                    let resolved = checker.get_type_from_type_node(annotation);
                    if resolved == error {
                        offender = Some("a parameter annotation that GAPS".to_string());
                        break 'outer;
                    }
                    let flags = checker.type_of(resolved).flags;
                    if !ok.contains(flags) {
                        // Name the excluded space by what it is, so the answer
                        // says which machinery it needs.
                        let what = if flags.intersects(TypeFlags::UNION) {
                            "a UNION parameter"
                        } else if flags.intersects(TypeFlags::INTERSECTION) {
                            "an INTERSECTION parameter"
                        } else if flags.intersects(TypeFlags::TYPE_PARAMETER) {
                            "a TYPE PARAMETER — generic, needs inference"
                        } else if flags.intersects(TypeFlags::ENUM_LIKE) {
                            "an ENUM parameter"
                        } else if flags.intersects(TypeFlags::ES_SYMBOL_LIKE) {
                            "a SYMBOL parameter"
                        } else if flags.intersects(TypeFlags::ANY) {
                            "an `any` parameter"
                        } else if flags.intersects(TypeFlags::OBJECT) {
                            "an OBJECT parameter — needs structural assignability"
                        } else {
                            "another shape"
                        };
                        offender = Some(what.to_string());
                        break 'outer;
                    }
                }
            }
            let form = offender.unwrap_or_else(|| {
                "every parameter IS selectable — the gate is elsewhere".to_string()
            });
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

    println!("# selectable — what blocks overload selection, by parameter shape\n");
    println!(
        "classified (gap line on a call to a 2+-declaration function): {}\n",
        report.classified
    );
    let mut rows: Vec<_> = report.forms.iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    let mut sum = 0;
    for (form, n) in rows {
        sum += n;
        println!("  {n:>6}  {form}");
    }
    println!("\n  C1 classified-but-not-gap: {}  (expect 0)", report.c1_not_gap);
    println!("  C2 buckets sum {sum} vs classified {}", report.classified);

    let mut cases: Vec<_> = report.cases.iter().collect();
    cases.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    println!("\n## Top cases\n");
    for (case, n) in cases.into_iter().take(8) {
        println!("  {n:>6}  {case}");
    }
}
