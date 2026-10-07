//! Counterfactual for `bd tsr-84iz` — the pattern-implied contextual type.
//!
//! `checkDeclarationInitializer` (`checker.go:16797`) threads
//! `getTypeFromBindingPattern` (`checker.go:17904`) as the initializer's
//! contextual type, which is what makes
//!
//! ```ts
//! var [a, b] = [1, "x"];
//! ```
//!
//! infer the **tuple** `[number, string]` rather than the array
//! `(string | number)[]`, so `a` is `number` and `b` is `string`.
//!
//! `crates/tsr-checker/src/destructure.rs` **refuses this construct whole**
//! (its array-literal guard), because approximating it was measured at ~80
//! wrong lines during `bd tsr-o00` — the refusal is recorded in `STATUS.md`
//! §5 with that number. This probe asks what building it properly would be
//! worth, as a match test rather than a population count.
//!
//! The forecast for element *i* of an array pattern over an array-literal
//! initializer is the **widened** type of the literal's element *i* — which
//! is what a tuple contextual type reduces to for this shape. Both halves are
//! reported: the element lines it would convert, and the ones it would get
//! wrong.
//!
//! Controls: C1 every classified line answers `errorType` today (expect 0).
//! C2 buckets sum to classified.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Node, SyntaxKind};
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
            // The name of a binding element inside an ARRAY pattern whose
            // holder is initialised by an array literal — exactly the
            // construct `destructure.rs` refuses.
            let Some(Node::Identifier(_)) = map.get(id) else { continue };
            let Some(element_id) = nodes.parent(id) else { continue };
            if nodes.kind(element_id) != SyntaxKind::BindingElement {
                continue;
            }
            if map.get(element_id).and_then(|n| n.name_id()) != Some(id) {
                continue;
            }
            let Some(pattern_id) = nodes.parent(element_id) else { continue };
            if nodes.kind(pattern_id) != SyntaxKind::ArrayBindingPattern {
                continue;
            }
            let Some(Node::BindingPattern(pattern)) = map.get(pattern_id) else { continue };
            let Some(holder) = nodes.parent(pattern_id) else { continue };
            let Some(Node::VariableDeclaration(declaration)) = map.get(holder) else { continue };
            if declaration.r#type.is_some() {
                continue;
            }
            let Some(tsr_ast::Expression::ArrayLiteralExpression(literal)) =
                declaration.initializer
            else {
                continue;
            };
            let Some(slot) = pattern.elements.iter().position(|e| e.node_id == Some(element_id))
            else {
                continue;
            };
            report.classified += 1;
            if types_producer::type_id_at_location(&mut checker, bound, nodes, map, id) != error {
                report.c1_not_gap += 1;
            }
            let wanted = want.text.rsplit_once(" : ").map_or("", |(_, a)| a).to_string();

            // The forecast: element `slot` of the literal, widened. A tuple
            // contextual type over a plain array literal reduces to exactly
            // this for the element positions.
            let form = match literal.elements.get(slot) {
                None => "the pattern is longer than the literal — out of range".to_string(),
                Some(tsr_ast::Expression::SpreadElement(_)) => {
                    "a spread element — needs sliceTupleType".to_string()
                }
                Some(element) => {
                    let element_type = checker.check_expression(*element);
                    if element_type == error {
                        "the literal's element itself gaps".to_string()
                    } else {
                        let widened = checker.get_widened_literal_type(element_type);
                        let forecast = checker.type_to_string(widened);
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

    println!("# patctx — the pattern-implied contextual type, forecast (bd tsr-84iz)\n");
    println!("classified: {}\n", report.classified);
    let mut rows: Vec<_> = report.forms.iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    let mut sum = 0;
    for (form, n) in rows {
        sum += n;
        println!("  {n:>6}  {form}");
    }
    println!("\n  C1 classified-but-not-gap: {}  (expect 0)", report.c1_not_gap);
    println!("  C2 buckets sum {sum} vs classified {}", report.classified);

    println!("\n## Misses, verbatim\n");
    let mut misses: Vec<_> = report.misses.iter().collect();
    misses.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (miss, n) in misses.into_iter().take(12) {
        println!("  {n:>5}  {miss}");
    }

    let mut cases: Vec<_> = report.cases.iter().collect();
    cases.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    println!("\n## Top cases\n");
    for (case, n) in cases.into_iter().take(8) {
        println!("  {n:>6}  {case}");
    }
}
