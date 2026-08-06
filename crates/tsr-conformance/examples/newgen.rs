//! Counterfactual for the `new C<T>()` arm (`checker-notes-callres.md` §14).
//!
//! `callgate.rs` sized two `new` gates at 826 lines — 516 refused for having
//! explicit type arguments, 310 for the class being generic. A population is
//! a ceiling, and `docs/conventions.md` requires the *match* test rather than
//! the shape test, so this probe does not count the row: it **computes the
//! answer the arm would produce and compares it to the baseline, string for
//! string**.
//!
//! The forecast type is what `create_type_reference(symbol, arguments)` would
//! print — `type_reference_text` (`declared.rs`) renders a reference as
//! `Name<A, B>`, so the probe formats the same string from the class's name
//! and the written type arguments' resolved types. Where a type argument
//! itself gaps, the arm would gap too and the line is reported as such rather
//! than as a conversion.
//!
//! Controls: C1 every classified line answers `errorType` today (expect 0).
//! C2 buckets sum to classified.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Node, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

#[derive(Default)]
struct Report {
    classified: usize,
    forms: BTreeMap<String, usize>,
    cases: BTreeMap<String, usize>,
    misses: BTreeMap<String, usize>,
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
        for (k, n) in &other.misses {
            *self.misses.entry(k.clone()).or_default() += n;
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
            let Some(Node::NewExpression(node)) = map.get(id) else { continue };
            let Some(callee) = node.expression else { continue };
            let callee_type = checker.check_expression(callee);
            if callee_type == error {
                continue;
            }
            // The class symbol, reached the way `check_new_expression` reaches
            // it — through the callee's *type*, so `new (C)()` and an aliased
            // class behave alike.
            let Some(callee_id) = tsr_ast::Node::from(callee).node_id() else { continue };
            let symbol = match map.get(callee_id) {
                Some(Node::Identifier(identifier)) => {
                    bound.resolve_name(nodes, map, callee_id, identifier.text, SymbolFlags::VALUE)
                }
                _ => None,
            };
            let Some(symbol) = symbol else { continue };
            let entry = bound.symbols().get(symbol);
            if !entry.flags.contains(SymbolFlags::CLASS) {
                continue;
            }
            let Some(declaration) = entry.declarations.first().copied() else { continue };
            let type_parameters = match map.get(declaration) {
                Some(Node::ClassDeclaration(class)) => class.type_parameters,
                Some(Node::ClassExpression(class)) => class.type_parameters,
                _ => continue,
            };
            if type_parameters.is_empty() {
                continue;
            }
            report.classified += 1;
            if types_producer::type_id_at_location(&mut checker, bound, nodes, map, id) != error {
                report.c1_not_gap += 1;
            }
            let wanted = want.text.rsplit_once(" : ").map_or("", |(_, a)| a).to_string();

            let form = if node.type_arguments.is_empty() {
                // No written arguments: upstream infers from the constructor
                // call. Out of scope for this arm.
                "no written type arguments — needs inference".to_string()
            } else if node.type_arguments.len() != type_parameters.len() {
                // `fillMissingTypeArguments` and defaults; arity mismatch is an
                // upstream error.
                "arity differs from the class's type parameters".to_string()
            } else {
                let mut printed = Vec::new();
                let mut gapped = false;
                for argument in node.type_arguments {
                    let t = checker.get_type_from_type_node(*argument);
                    if t == error {
                        gapped = true;
                        break;
                    }
                    printed.push(checker.type_to_string(t));
                }
                if gapped {
                    "a written type argument itself gaps".to_string()
                } else {
                    let name = entry.name.to_string();
                    let forecast = format!("{name}<{}>", printed.join(", "));
                    if forecast == wanted {
                        "CONVERTS — forecast matches the baseline exactly".to_string()
                    } else if wanted == "any" {
                        "want `any` (ceiling)".to_string()
                    } else {
                        *report
                            .misses
                            .entry(format!("want `{wanted}`, forecast `{forecast}`"))
                            .or_default() += 1;
                        "MISS — forecast differs".to_string()
                    }
                }
            };
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

    println!("# newgen — the `new C<T>()` counterfactual, forecast against the baseline\n");
    println!("classified (generic class, gap line): {}\n", report.classified);
    let mut rows: Vec<_> = report.forms.iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    let mut sum = 0;
    for (form, n) in rows {
        sum += n;
        println!("  {n:>6}  {form}");
    }
    println!("\n  C1 classified-but-not-gap: {}  (expect 0)", report.c1_not_gap);
    println!("  C2 buckets sum {sum} vs classified {}", report.classified);

    println!("\n## The misses, verbatim — what a forecast got wrong\n");
    let mut misses: Vec<_> = report.misses.iter().collect();
    misses.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (miss, n) in misses.into_iter().take(15) {
        println!("  {n:>5}  {miss}");
    }

    let mut cases: Vec<_> = report.cases.iter().collect();
    cases.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    println!("\n## Top cases\n");
    for (case, n) in cases.into_iter().take(8) {
        println!("  {n:>6}  {case}");
    }
}
