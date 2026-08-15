//! How many non-right lines sit on a reference that is being **written**?
//!
//! ```text
//! cargo run --release -p tsr-conformance --example writetarget
//! ```
//!
//! # The question, and why it needs a probe rather than a join
//!
//! §613 (`checker-notes-enums.md`) found that all eight of
//! `compiler/incrementAndDecrement`'s wrong lines sit on `e++` / `--e` operands
//! — assignment TARGETS — and that the fixture contains no plain read of `e` at
//! all. The hypothesis that follows is not enum-specific: **a reference that is
//! being written may not be narrowed the way this port narrows it.**
//!
//! `target/verdict_baseline.tsv` cannot answer this. It records `case:file:pos`
//! with the wanted and printed text and no node identity, so "is this position a
//! write target" is exactly the question a join cannot reach. Hence a probe.
//!
//! §613 registered the sizing as the step that must come BEFORE any build, on
//! the §609–§612 record: four consecutive wrong diagnoses of one 8-line case,
//! every one of them a mechanism guessed at before its population was counted.
//!
//! # What is counted
//!
//! A node is a write target when it is:
//!
//! - the operand of a prefix or postfix `++` / `--`; or
//! - the left-hand side of an assignment operator — plain `=` and every
//!   compound form.
//!
//! Both are `ast.IsAssignmentTarget`'s cases that can carry a `.types` line for
//! the reference itself. The output splits every non-right aligned line by that
//! predicate, so the two halves sum to the suite's own wrong+gap count.

use rayon::prelude::*;
use tsr_ast::{Node, NodeId, NodeMap, NodeTable, SyntaxKind};
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// `ast.IsAssignmentTarget` reduced to the two forms that put the WRITTEN
/// reference in a position a `.types` line is emitted for.
fn is_write_target(id: NodeId, nodes: &NodeTable, map: &NodeMap<'_>) -> bool {
    let Some(parent) = nodes.parent(id) else { return false };
    match map.get(parent) {
        Some(Node::PrefixUnaryExpression(unary)) => {
            matches!(unary.operator.kind, SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken)
                && unary.operand.and_then(|operand| operand.node_id()) == Some(id)
        }
        Some(Node::PostfixUnaryExpression(unary)) => {
            matches!(unary.operator.kind, SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken)
                && unary.operand.and_then(|operand| operand.node_id()) == Some(id)
        }
        Some(Node::BinaryExpression(binary)) => {
            let is_assignment = binary.operator_token.is_some_and(|token| {
                matches!(
                    token.kind,
                    SyntaxKind::EqualsToken
                        | SyntaxKind::PlusEqualsToken
                        | SyntaxKind::MinusEqualsToken
                        | SyntaxKind::AsteriskEqualsToken
                        | SyntaxKind::SlashEqualsToken
                        | SyntaxKind::PercentEqualsToken
                        | SyntaxKind::AmpersandEqualsToken
                        | SyntaxKind::BarEqualsToken
                        | SyntaxKind::CaretEqualsToken
                        | SyntaxKind::LessThanLessThanEqualsToken
                        | SyntaxKind::GreaterThanGreaterThanEqualsToken
                        | SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
                        | SyntaxKind::AsteriskAsteriskEqualsToken
                )
            });
            is_assignment && binary.left.and_then(|left| Node::from(left).node_id()) == Some(id)
        }
        _ => false,
    }
}

fn main() {
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    assert!(corpus.is_available(), "corpus missing; run git submodule update --init --recursive");
    let cases = corpus.discover().expect("discovering cases");

    let rows: Vec<(bool, String, String, String)> = cases
        .par_iter()
        .filter_map(|case| {
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
            let node_map = program.node_map();

            let mut out = Vec::new();
            for (index, expected_file) in expected.iter().enumerate() {
                let Some(our_file) = ours.get(index) else { continue };
                let our_ids = ids.get(index);
                for (position, want) in expected_file.assertions.iter().enumerate() {
                    let Some(got) = our_file.get(position) else { continue };
                    let got_line = got.line();
                    if want.text == got_line {
                        continue;
                    }
                    // `verdict.rs:60`'s alignment test, reproduced.
                    let (Some((we, wt)), Some((ge, gt))) = (
                        want.split(),
                        types_baseline::TypeAssertion { text: got_line.clone() }
                            .split()
                            .map(|(e, t)| (e.to_string(), t.to_string())),
                    ) else {
                        continue;
                    };
                    if we != ge.as_str() {
                        continue;
                    }
                    let Some(id) = our_ids.and_then(|line_ids| line_ids.get(position).copied())
                    else {
                        continue;
                    };
                    out.push((
                        is_write_target(id, nodes, node_map),
                        case.name.clone(),
                        wt.to_string(),
                        gt,
                    ));
                }
            }
            Some(out)
        })
        .flatten()
        .collect();

    // A write-target line is only EVIDENCE for the narrowing hypothesis when the
    // printed type looks like a NARROWING of the wanted one: a constituent of
    // the wanted union, or a member of the wanted enum. Everything else is a
    // write target that happens to be wrong for an unrelated reason —
    // `parserRealSource12`'s 108 lines want `IAstWalkChildren` and print `any`,
    // which is name resolution, not narrowing. Without this split the headline
    // number is a ceiling that reads like a forecast.
    let looks_narrowed = |want: &str, got: &str| -> bool {
        if want == got {
            return false;
        }
        want.split(" | ").any(|constituent| constituent.trim() == got)
            || got.starts_with(&format!("{want}."))
    };

    let writes = rows.iter().filter(|r| r.0).count();
    println!("non-right aligned lines:        {}", rows.len());
    println!("  on a WRITE TARGET:            {writes}");
    println!("  everywhere else:              {}", rows.len() - writes);

    let mut by_case: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for (is_write, case, _, _) in &rows {
        if *is_write {
            *by_case.entry(case.as_str()).or_default() += 1;
        }
    }
    let mut ranked: Vec<_> = by_case.into_iter().collect();
    ranked.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    println!("\ntop cases by write-target lines:");
    for (case, n) in ranked.iter().take(15) {
        println!("  {n:>4}  {case}");
    }
    println!("\ncases with at least one:        {}", ranked.len());

    let narrowed: Vec<_> =
        rows.iter().filter(|(is_write, _, w, g)| *is_write && looks_narrowed(w, g)).collect();
    let mut narrowed_cases: std::collections::BTreeMap<&str, usize> =
        std::collections::BTreeMap::new();
    for (_, case, _, _) in &narrowed {
        *narrowed_cases.entry(case.as_str()).or_default() += 1;
    }
    println!("\n-- of those, the ones that look OVER-NARROWED (the actual hypothesis) --");
    println!("lines:                          {}", narrowed.len());
    println!("cases:                          {}", narrowed_cases.len());
    let mut ranked_narrowed: Vec<_> = narrowed_cases.into_iter().collect();
    ranked_narrowed.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    for (case, n) in ranked_narrowed.iter().take(10) {
        println!("  {n:>4}  {case}");
    }
}
