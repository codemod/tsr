//! Counterfactual for private-name property access — `this.#x`.
//!
//! `depend.rs` at HEAD reports **551 gap lines** under
//! `property access, the name is not an identifier`. `MemberName` is
//! `Identifier | PrivateIdentifier` (`crates/tsr-ast`), so that reason names
//! exactly one construct: a private name.
//!
//! The binder already declares a `#x` member under the name `#x`
//! (`binder.rs:3995` maps `PropertyName::PrivateIdentifier` to its text, and
//! `PrivateIdentifier.text` carries the `#`), while the *access* side answers
//! `None` (`binder.rs:3800`) and `members.rs` has no arm at all. So the
//! question is whether the lookup already succeeds once the name is passed
//! through.
//!
//! This is a **counterfactual, not a population count**: it performs the
//! lookup the arm would perform and compares the printed result to the
//! baseline, string for string.
//!
//! Controls: C1 every classified line answers `errorType` today (expect 0).
//! C2 buckets sum to classified.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{MemberName, Node};
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
            // Both the access node itself and the private NAME node render a
            // line, so classify either.
            let (receiver, private) = match map.get(id) {
                Some(Node::PropertyAccessExpression(access)) => {
                    match (access.expression, access.name) {
                        (Some(r), Some(MemberName::PrivateIdentifier(p))) => (r, p),
                        _ => continue,
                    }
                }
                Some(Node::PrivateIdentifier(p)) => {
                    let Some(parent) = nodes.parent(id) else { continue };
                    let Some(Node::PropertyAccessExpression(access)) = map.get(parent) else {
                        continue;
                    };
                    match access.expression {
                        Some(r) => (r, p),
                        None => continue,
                    }
                }
                _ => continue,
            };
            let receiver_type = checker.check_expression(receiver);
            if receiver_type == error {
                // The gap is upstream of this arm.
                continue;
            }
            report.classified += 1;
            if types_producer::type_id_at_location(&mut checker, bound, nodes, map, id) != error {
                report.c1_not_gap += 1;
            }
            let wanted = want.text.rsplit_once(" : ").map_or("", |(_, a)| a).to_string();

            // The lookup the arm would perform. `PrivateIdentifier.text`
            // carries its `#`, and the binder files the member under that same
            // spelling, so this is a plain property lookup.
            let form = match checker.get_type_of_property_of_type(receiver_type, private.text) {
                Some(found) if found == error => {
                    "the member is found but its own type GAPS (downstream)".to_string()
                }
                Some(found) => {
                    let forecast = checker.type_to_string(found);
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
                None => {
                    *report
                        .misses
                        .entry(format!(
                            "no such member `{}` on `{}`",
                            private.text,
                            checker.type_to_string(receiver_type)
                        ))
                        .or_default() += 1;
                    "the receiver has no such private member".to_string()
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

    println!("# privname — the `this.#x` counterfactual\n");
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
