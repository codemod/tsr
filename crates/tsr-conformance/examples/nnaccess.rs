//! Sizing probe for `checkNonNullExpression` in access positions.
//!
//! Upstream types `x.a` and `x["a"]` on a **nullable** receiver by removing
//! `null`/`undefined` first — `checkPropertyAccessExpression` hands
//! `checkNonNullExpression(expr)` to the lookup (`checker.go:11258`), and the
//! "possibly undefined" report is a *diagnostic*, not the type answer
//! (ADR-0040's two-channel distinction). This port's `members.rs` and
//! `indexed.rs` hand the union itself to the lookup, which misses, so every
//! such access is a gap.
//!
//! Population: gap lines whose node is a property access (or its member name)
//! or an element access, whose receiver types today as a union that PRINTS
//! with a `null` or `undefined` constituent. Text-classified — a probe-only
//! convenience (`checker-notes-tuple.md` §7's rule is about the comparator).
//!
//! Controls: C1 classified lines actually gap (expect 0 violations);
//! C2 buckets sum.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::Node;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

#[derive(Default)]
struct Report {
    classified: usize,
    forms: BTreeMap<String, usize>,
    wants_any: BTreeMap<String, usize>,
    receivers: BTreeMap<String, usize>,
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
        for (k, n) in &other.receivers {
            *self.receivers.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.cases {
            *self.cases.entry(k.clone()).or_default() += n;
        }
    }
}

/// Does this printed union carry a `null` or `undefined` constituent at top
/// level?
fn is_nullable_union(text: &str) -> bool {
    if !text.contains(" | ") {
        return false;
    }
    text.split(" | ").any(|piece| piece == "null" || piece == "undefined")
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
            let node = map.get(id)?;
            // The access node itself, or the member NAME of one (`a.b`'s `b`
            // types as the property, so its gap has the same receiver).
            let (access_kind, receiver, optional) = match node {
                Node::PropertyAccessExpression(access) => {
                    ("property access", access.expression, access.question_dot_token.is_some())
                }
                Node::ElementAccessExpression(access) => {
                    ("element access", access.expression, access.question_dot_token.is_some())
                }
                _ => {
                    let parent = nodes.parent(id).and_then(|p| map.get(p));
                    match parent {
                        Some(Node::PropertyAccessExpression(access))
                            if access.name.and_then(|n| match n {
                                tsr_ast::MemberName::Identifier(i) => i.node_id,
                                tsr_ast::MemberName::PrivateIdentifier(i) => i.node_id,
                            }) == Some(id) =>
                        {
                            ("member name", access.expression, access.question_dot_token.is_some())
                        }
                        _ => continue,
                    }
                }
            };
            let Some(receiver) = receiver else { continue };
            let receiver_type = checker.check_expression(receiver);
            if receiver_type == error {
                continue;
            }
            let receiver_text = checker.type_to_string(receiver_type);
            if !is_nullable_union(&receiver_text) {
                continue;
            }
            report.classified += 1;
            if types_producer::type_id_at_location(&mut checker, bound, nodes, map, id) != error {
                report.c1_not_gap += 1;
            }
            let form = format!("{access_kind}{}", if optional { " (optional chain)" } else { "" });
            let wants_any = want.text.rsplit_once(" : ").is_some_and(|(_, answer)| answer == "any");
            if wants_any {
                *report.wants_any.entry(form.clone()).or_default() += 1;
            }
            *report.forms.entry(form).or_default() += 1;
            *report.receivers.entry(receiver_text.clone()).or_default() += 1;
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

    println!("# nnaccess — access gap lines on a nullable-union receiver\n");
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
    let mut receivers: Vec<_> = report.receivers.iter().collect();
    receivers.sort_by(|a, b| b.1.cmp(a.1));
    println!("\n## Top receiver types\n");
    for (receiver, n) in receivers.into_iter().take(12) {
        println!("  {n:>6}  {receiver}");
    }
    let mut cases: Vec<_> = report.cases.iter().collect();
    cases.sort_by(|a, b| b.1.cmp(a.1));
    println!("\n## Top cases\n");
    for (case, n) in cases.into_iter().take(10) {
        println!("  {n:>6}  {case}");
    }
}
