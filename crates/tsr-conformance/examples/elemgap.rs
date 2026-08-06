//! Sizing probe for the element-access remainder — STATUS.md §4.2's 374-score
//! row, refused three times historically on a `want-any` share that the
//! `tsr-4qx` collapse reduced to 19.2%.
//!
//! For every gap line whose own node is an `ElementAccessExpression` and whose
//! receiver and index both type today (the "root is here" condition), classify
//! by what `getIndexedAccessType` (`checker.go:21902`) would need:
//!
//! - the index type's shape (numeric literal / string literal / other), and
//! - the receiver type's shape — in particular whether it is a **tuple** this
//!   port minted (`tuple_element_lists`, recorded at `0d56467`), because
//!   `t[0]`'s element type became answerable the moment the reverse index
//!   existed, and whether the receiver is a union.
//!
//! Controls: C1 every classified line's node types to `errorType` (expect 0
//! violations); C2 buckets sum to the classified total.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::Node;
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
        types_producer::assertions_for_case_with_ids(&arena, &parsed, &expected);
    let nodes = program.nodes();
    let map = program.node_map();
    let bound = program.binder();
    let mut checker = tsr_checker::Checker::with_module_host(bound, nodes, map, Some(&program));
    let error = checker.intrinsics().error;
    let any = checker.intrinsics().any;

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
            let Some(Node::ElementAccessExpression(access)) = map.get(id) else { continue };
            let (Some(receiver), Some(argument)) = (access.expression, access.argument_expression)
            else {
                continue;
            };

            let receiver_type = checker.check_expression(receiver);
            let index_type = checker.check_expression(argument);
            if receiver_type == error || index_type == error {
                // The gap is upstream of this arm — not this row's.
                continue;
            }
            report.classified += 1;
            if types_producer::type_id_at_location(&mut checker, bound, nodes, map, id) != error {
                report.c1_not_gap += 1;
            }

            // Classified from the PRINTED text — a probe-only convenience the
            // comparator itself is forbidden (`checker-notes-tuple.md` §7): a
            // tuple's print starts with `[`, a string literal's with `"`, and
            // a numeric literal's parses as a number.
            let index_text = checker.type_to_string(index_type);
            let index_shape = if index_text.starts_with('"') {
                "string literal"
            } else if index_text.parse::<f64>().is_ok() {
                "numeric literal"
            } else if index_text == "number" {
                "number"
            } else if index_text == "string" {
                "string"
            } else {
                "other index"
            };
            let receiver_text = checker.type_to_string(receiver_type);
            let receiver_shape = if access.question_dot_token.is_some() {
                "optional chain"
            } else if receiver_text.starts_with('[') || receiver_text.starts_with("readonly [") {
                "tuple"
            } else if receiver_type == any {
                "any receiver"
            } else if receiver_text.contains(" | ") {
                "union receiver"
            } else {
                "other receiver"
            };
            let form = format!("{receiver_shape} [{index_shape}]");
            let wants_any = want.text.rsplit_once(" : ").is_some_and(|(_, answer)| answer == "any");
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

    println!("# elemgap — element-access gap lines whose receiver and index type\n");
    println!("classified: {}\n", report.classified);
    let mut rows: Vec<_> = report.forms.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1));
    let mut sum = 0;
    for (form, n) in rows {
        let any = report.wants_any.get(form).copied().unwrap_or(0);
        sum += n;
        println!("  {n:>6}  want-any {any:>5}  {form}");
    }
    println!("\n  C1 classified-but-not-gap: {}  (expect 0)", report.c1_not_gap);
    println!("  C2 buckets sum {sum} vs classified {}", report.classified);
    let mut cases: Vec<_> = report.cases.iter().collect();
    cases.sort_by(|a, b| b.1.cmp(a.1));
    println!("\n## Top cases\n");
    for (case, n) in cases.into_iter().take(10) {
        println!("  {n:>6}  {case}");
    }
}
