//! The probe STATUS.md §4.3 names for `bd tsr-q9g`: identifier-reference
//! lines governed by a `typeof x === "..."` guard, which `refmatch.rs`
//! excludes by charter (its rule 1 admits access expressions only).
//!
//! For every assertion line that is not right today and whose node is an
//! `Identifier` used as an expression (not a declaration name, not a member
//! name), walk the enclosing `if`/`while`/conditional branches; count the
//! line when some enclosing condition contains `typeof y` over the same
//! identifier text. The text test is the same approximation `refmatch.rs`
//! documents: it cannot see shadowing, so it is a ceiling, and it cannot see
//! early-return guards, so it is simultaneously a floor of the loose
//! population — both biases are stated rather than corrected.
//!
//! Controls: C1 counted lines gap or are wrong (by construction of the entry
//! test, violations must be 0); C2 verdict buckets sum to the total.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Node, NodeId, NodeMap, NodeTable};
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// Does this condition subtree contain `typeof <name>`?
fn condition_mentions_typeof_of(map: &NodeMap<'_>, condition: NodeId, name: &str) -> bool {
    let mut stack = vec![condition];
    let mut budget = 512usize;
    while let Some(id) = stack.pop() {
        if budget == 0 {
            return false;
        }
        budget -= 1;
        match map.get(id) {
            Some(Node::TypeOfExpression(node)) => {
                if let Some(tsr_ast::Expression::Identifier(identifier)) = node.expression
                    && identifier.text == name
                {
                    return true;
                }
                if let Some(inner) = node.expression.and_then(|e| e.node_id()) {
                    stack.push(inner);
                }
            }
            Some(Node::BinaryExpression(node)) => {
                stack.extend(node.left.and_then(|e| e.node_id()));
                stack.extend(node.right.and_then(|e| e.node_id()));
            }
            Some(Node::PrefixUnaryExpression(node)) => {
                stack.extend(node.operand.and_then(|e| e.node_id()));
            }
            Some(Node::ParenthesizedExpression(node)) => {
                stack.extend(node.expression.and_then(|e| e.node_id()));
            }
            _ => {}
        }
    }
    false
}

/// The nearest enclosing conditions whose guarded branch contains `node`.
fn enclosing_conditions(nodes: &NodeTable, map: &NodeMap<'_>, node: NodeId) -> Vec<NodeId> {
    let mut conditions = Vec::new();
    let mut current = node;
    let mut budget = 256usize;
    while let Some(parent) = nodes.parent(current) {
        if budget == 0 {
            break;
        }
        budget -= 1;
        match map.get(parent) {
            Some(Node::IfStatement(statement)) => {
                let in_branch = statement
                    .then_statement
                    .and_then(|s| s.node_id())
                    .is_some_and(|id| id == current)
                    || statement
                        .else_statement
                        .and_then(|s| s.node_id())
                        .is_some_and(|id| id == current);
                if in_branch && let Some(condition) = statement.expression.and_then(|e| e.node_id())
                {
                    conditions.push(condition);
                }
            }
            Some(Node::ConditionalExpression(expression)) => {
                let in_branch =
                    expression.when_true.and_then(|e| e.node_id()).is_some_and(|id| id == current)
                        || expression
                            .when_false
                            .and_then(|e| e.node_id())
                            .is_some_and(|id| id == current);
                if in_branch && let Some(condition) = expression.condition.and_then(|e| e.node_id())
                {
                    conditions.push(condition);
                }
            }
            Some(Node::WhileStatement(statement)) => {
                if statement.statement.node_id() == Some(current)
                    && let Some(condition) = statement.expression.and_then(|e| e.node_id())
                {
                    conditions.push(condition);
                }
            }
            _ => {}
        }
        current = parent;
    }
    conditions
}

#[derive(Default)]
struct Report {
    verdicts: BTreeMap<&'static str, usize>,
    wants: BTreeMap<String, usize>,
    cases: BTreeMap<String, usize>,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        for (k, n) in &other.verdicts {
            *self.verdicts.entry(k).or_default() += n;
        }
        for (k, n) in &other.wants {
            *self.wants.entry(k.clone()).or_default() += n;
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
            let id = line_ids[position];
            let Some(Node::Identifier(identifier)) = map.get(id) else { continue };
            // A reference, not a declaration name and not a member name — the
            // same two exclusions `depend.rs` makes for its Identifier arm.
            if let Some(parent) = nodes.parent(id)
                && let Some(parent_node) = map.get(parent)
                && parent_node.name_id() == Some(id)
            {
                continue;
            }
            let governed = enclosing_conditions(nodes, map, id)
                .into_iter()
                .any(|condition| condition_mentions_typeof_of(map, condition, identifier.text));
            if !governed {
                continue;
            }
            let verdict = if got.type_string == "error" { "gap" } else { "wrong" };
            *report.verdicts.entry(verdict).or_default() += 1;
            let wanted = want.text.rsplit_once(" : ").map(|(_, w)| w.to_string());
            if let Some(wanted) = wanted {
                *report.wants.entry(wanted).or_default() += 1;
            }
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

    println!("# idtypeof — not-right identifier lines governed by a typeof guard\n");
    let total: usize = report.verdicts.values().sum();
    println!("total: {total}");
    for (verdict, n) in &report.verdicts {
        println!("  {verdict:>6}  {n}");
    }
    println!("\n## Top wanted answers\n");
    let mut wants: Vec<_> = report.wants.iter().collect();
    wants.sort_by(|a, b| b.1.cmp(a.1));
    for (want, n) in wants.into_iter().take(12) {
        println!("  {n:>6}  {want}");
    }
    println!("\n## Top cases\n");
    let mut cases: Vec<_> = report.cases.iter().collect();
    cases.sort_by(|a, b| b.1.cmp(a.1));
    for (case, n) in cases.into_iter().take(10) {
        println!("  {n:>6}  {case}");
    }
}
