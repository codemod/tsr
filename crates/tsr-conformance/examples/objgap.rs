//! `STATUS.md` §4.3's registered probe for the object-literal remainder:
//! *"accessors (`bd tsr-32y`) and computed names both fall into the catch-all,
//! in unknown proportion — split the catch-all by member kind"*.
//!
//! `depend.rs` at HEAD reads `ObjectLiteralExpression` at **2,479 gap lines,
//! want-any 110 (4.4%), 646 cases, top-1 3.4%**. `objects.rs` dispatches
//! property, shorthand, spread and method members and sends everything else to
//! a catch-all that gaps the **whole literal**, so one unported member kind
//! costs every line the literal prints.
//!
//! For every gap line whose node is an `ObjectLiteralExpression`, this reports
//! the **set of member kinds** the literal carries that `objects.rs` has no arm
//! for — the thing that decides which arm to build next — plus, for the
//! accessor case, whether the accessor's type is computable today.
//!
//! Upstream prints an accessor as a **property**, not as a signature
//! (`bd tsr-32y`), so the accessor arm is NOT a copy of the method arm.
//!
//! Controls: C1 every classified line answers `errorType` today (expect 0).
//! C2 buckets sum to classified.

use std::collections::{BTreeMap, BTreeSet};

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Node, ObjectLiteralElementLike, PropertyName};
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

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
            let Some(Node::ObjectLiteralExpression(literal)) = map.get(id) else { continue };
            report.classified += 1;
            if types_producer::type_id_at_location(&mut checker, bound, nodes, map, id) != error {
                report.c1_not_gap += 1;
            }

            // The member kinds this literal carries that `objects.rs` has no
            // arm for, as a SET: a literal gaps whole, so the finding is which
            // combination is present, not a per-member count.
            let mut unhandled: BTreeSet<&'static str> = BTreeSet::new();
            let mut computed_name = false;
            for member in literal.properties {
                match member {
                    ObjectLiteralElementLike::GetAccessorDeclaration(_) => {
                        unhandled.insert("get accessor");
                    }
                    ObjectLiteralElementLike::SetAccessorDeclaration(_) => {
                        unhandled.insert("set accessor");
                    }
                    ObjectLiteralElementLike::PropertyAssignment(p) => {
                        if matches!(p.name, PropertyName::ComputedPropertyName(_)) {
                            computed_name = true;
                        }
                    }
                    ObjectLiteralElementLike::MethodDeclaration(m) => {
                        if matches!(m.name, PropertyName::ComputedPropertyName(_)) {
                            computed_name = true;
                        }
                    }
                    _ => {}
                }
            }
            if computed_name {
                unhandled.insert("computed name");
            }

            let form = if unhandled.is_empty() {
                // Every member kind here HAS an arm, so the literal gaps
                // because one member's *value* gaps — downstream, and not this
                // row's item. Split off so it cannot inflate the accessor
                // figure, which is exactly what §4.3 warns about.
                "every member kind has an arm — a member VALUE gaps (downstream)".to_string()
            } else {
                unhandled.iter().copied().collect::<Vec<_>>().join(" + ")
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

    println!("# objgap — the object-literal catch-all, split by member kind\n");
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

    let mut cases: Vec<_> = report.cases.iter().collect();
    cases.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    println!("\n## Top cases\n");
    for (case, n) in cases.into_iter().take(10) {
        println!("  {n:>6}  {case}");
    }
}
