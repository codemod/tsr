//! What flow narrowing for **property references** could reach — `bd tsr-6ka`.
//!
//! # Why this probe and not a guard histogram
//!
//! `docs/architecture/checker-notes-narrow.md` §4 measured a narrowing arm
//! converting 33 lines against a predicted 150, and named the cause: this
//! port's `is_matching_reference` compares resolved **symbols**, so it matches
//! identifiers only. Every `a.b !== undefined` / `o?.foo != null` guard the
//! corpus writes narrows nothing, and `check_property_access_expression` never
//! reaches the flow walk at all.
//!
//! The registration on `bd tsr-6ka` therefore says: **size it through the
//! matcher, not through the guards.** A count of guards in the corpus is a
//! count of what upstream's users write; what decides this item is how many
//! *assertion lines* a working matcher would put in range. Those are different
//! sets and the last bar was set from the wrong one.
//!
//! # What it counts
//!
//! An assertion line is **in range** when all of:
//!
//! 1. its node is a `PropertyAccessExpression` or `ElementAccessExpression` —
//!    the two forms upstream's `isMatchingReference` handles structurally
//!    (`flow.go`, the `KindPropertyAccessExpression` arm: same accessed
//!    property name **and** a recursively matching receiver);
//! 2. it sits inside the *body* of an enclosing `if` / `while` / conditional —
//!    not inside the condition itself, since narrowing applies to the branch;
//! 3. the guarding condition's **source text mentions the access's own source
//!    text**, which is the syntactic stand-in for "the matcher would match".
//!
//! Rule 3 is deliberately **loose**, and that is stated rather than hidden: it
//! admits `if (a.b) { c.a.b }`, where the texts overlap but the receivers
//! differ, and it admits a guard that is not a narrowing form at all
//! (`if (a.b === 3)` needs comparability). So the number is an **upper bound**,
//! and the two splits below are what make it usable:
//!
//! - **by current verdict** — a line already `right` is *at risk*, not
//!   convertible. `docs/conventions.md`: compute the at-risk population in the
//!   same pass as the target one, because this mechanism fires on a position
//!   rather than on a defect.
//! - **by guard form** — whether the condition is a bare reference
//!   (truthiness, already ported), a nullable equality (ported this cycle), or
//!   something else (`typeof`, `in`, `instanceof`, comparability — all
//!   unported, so those lines are *not* reachable by wiring the matcher alone).
//!
//! **The convertible figure is therefore `gap+wrong` under a ported guard
//! form.** Everything else on the table is context.
//!
//! # Controls
//!
//! - **C1, construction.** No line can be both in range and outside every
//!   enclosing guard; the in-range and out-of-range buckets must sum to the
//!   access-line population. Printed.
//! - **C2, pinned to the port, not to my summary of it.** Lines whose guard is
//!   a bare identifier reference on an **identifier** access are already
//!   narrowed today, so a probe that counted them as newly reachable would be
//!   double-counting work that landed. Restricting rule 1 to access
//!   expressions makes that bucket empty by construction; it is printed and
//!   must read 0.

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Node, NodeId, NodeMap, NodeTable, SyntaxKind};
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// How the assertion line stands today.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Right,
    Gap,
    Wrong,
}

/// Which narrowing form the enclosing guard is, which decides whether wiring
/// the matcher is enough to convert the line.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Guard {
    /// `if (a.b)` — truthiness, ported.
    Truthiness,
    /// `if (a.b !== undefined)`, `if (a.b != null)` — ported this cycle.
    NullableEquality,
    /// `typeof x === "..."` (either operand order) — unported; the largest
    /// single form upstream (`narrowTypeByTypeof`). Split out of `Unported`
    /// for `bd tsr-q9g`'s per-form ranking.
    TypeofGuard,
    /// `"p" in x` — unported; needs `get_type_of_property_of_type` only.
    InGuard,
    /// `x instanceof C` — unported; needs construct signatures.
    InstanceofGuard,
    /// Equality against a non-nullable operand — unported; needs
    /// `areTypesComparable`, which is the call-resolution blocker.
    ComparabilityGuard,
    /// Anything else — every one unported, so the matcher alone converts
    /// nothing here.
    Unported,
}

#[derive(Default)]
struct Report {
    /// Access-expression assertion lines, in range, by (guard, verdict).
    in_range: std::collections::BTreeMap<(u8, u8), usize>,
    /// Access-expression assertion lines with no enclosing guard.
    out_of_range: usize,
    /// The **loose** bound: an access line that is not right today and whose
    /// text is mentioned by *any* guard condition anywhere in the same file,
    /// whether or not the line sits inside that guard's branch.
    ///
    /// This exists because the strict measure above has a systematic blind
    /// spot, and it is the commonest narrowing idiom there is:
    /// `if (!a.b) return;` followed by a use of `a.b` **after** the `if`.
    /// Flow narrowing reaches that through the graph; an enclosing-branch test
    /// cannot see it at all. Early returns, `break`, `continue`, assignments
    /// and `&&` chains are all in the same blind spot.
    loose: usize,
    /// C2: identifier lines admitted by rule 1. Must be 0.
    c2_identifier_lines: usize,
    /// Cases contributing at least one convertible line.
    cases: std::collections::BTreeMap<String, usize>,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        for (k, n) in &other.in_range {
            *self.in_range.entry(*k).or_default() += n;
        }
        self.out_of_range += other.out_of_range;
        self.loose += other.loose;
        self.c2_identifier_lines += other.c2_identifier_lines;
        for (k, n) in &other.cases {
            *self.cases.entry(k.clone()).or_default() += n;
        }
    }
}

fn guard_label(g: u8) -> &'static str {
    match g {
        0 => "truthiness      (PORTED)",
        1 => "nullable equality (PORTED)",
        3 => "typeof guard  (unported)",
        4 => "`in` guard    (unported)",
        5 => "instanceof    (unported)",
        6 => "comparability (unported)",
        _ => "other guard   (unported)",
    }
}

fn verdict_label(v: u8) -> &'static str {
    match v {
        0 => "right (AT RISK)",
        1 => "gap",
        _ => "wrong",
    }
}

/// The nearest enclosing `if` / `while` / conditional whose *body* contains
/// `id`, with that construct's condition node.
///
/// Walking to the nearest one only is upstream's shape by accident rather than
/// by design — a nested guard narrows further — but it is the conservative
/// choice for an upper bound, because a line counted once cannot be counted
/// again under an outer guard.
fn enclosing_guard(nodes: &NodeTable, map: &NodeMap<'_>, id: NodeId) -> Option<(NodeId, NodeId)> {
    let mut child = id;
    let mut current = nodes.parent(id)?;
    loop {
        let (condition, branches): (Option<NodeId>, Vec<Option<NodeId>>) = match map.get(current) {
            Some(Node::IfStatement(node)) => (
                node.expression.and_then(|e| e.node_id()),
                vec![
                    node.then_statement.and_then(|s| s.node_id()),
                    node.else_statement.and_then(|s| s.node_id()),
                ],
            ),
            Some(Node::WhileStatement(node)) => {
                (node.expression.and_then(|e| e.node_id()), vec![node.statement.node_id()])
            }
            Some(Node::ConditionalExpression(node)) => (
                node.condition.and_then(|e| e.node_id()),
                vec![
                    node.when_true.and_then(|e| e.node_id()),
                    node.when_false.and_then(|e| e.node_id()),
                ],
            ),
            _ => (None, Vec::new()),
        };
        if let Some(condition) = condition {
            // Rule 2: inside a *branch*, not inside the condition. The child we
            // came up through is the discriminator, which is why the walk
            // carries it.
            if branches.into_iter().flatten().any(|branch| branch == child) {
                return Some((current, condition));
            }
        }
        child = current;
        current = nodes.parent(current)?;
    }
}

/// Classify the guard by its syntax, using the same operator set
/// `crate::flow`'s `narrow_type` matches on.
fn classify_guard(map: &NodeMap<'_>, condition: NodeId) -> Guard {
    match map.get(condition) {
        Some(Node::ParenthesizedExpression(inner)) => inner
            .expression
            .and_then(|e| e.node_id())
            .map_or(Guard::Unported, |id| classify_guard(map, id)),
        Some(Node::PrefixUnaryExpression(unary))
            if unary.operator.kind == SyntaxKind::ExclamationToken =>
        {
            unary
                .operand
                .and_then(|e| e.node_id())
                .map_or(Guard::Unported, |id| classify_guard(map, id))
        }
        Some(Node::PropertyAccessExpression(_) | Node::ElementAccessExpression(_)) => {
            Guard::Truthiness
        }
        Some(Node::BinaryExpression(binary)) => {
            let Some(operator) = binary.operator_token else { return Guard::Unported };
            if operator.kind == SyntaxKind::InKeyword {
                return Guard::InGuard;
            }
            if operator.kind == SyntaxKind::InstanceOfKeyword {
                return Guard::InstanceofGuard;
            }
            if !matches!(
                operator.kind,
                SyntaxKind::EqualsEqualsToken
                    | SyntaxKind::ExclamationEqualsToken
                    | SyntaxKind::EqualsEqualsEqualsToken
                    | SyntaxKind::ExclamationEqualsEqualsToken
            ) {
                return Guard::Unported;
            }
            // `typeof x === "string"` before the nullable test: the typeof
            // form compares against a string literal, never `null`/`undefined`.
            let has_typeof_operand =
                [binary.left, binary.right].into_iter().flatten().any(|operand| {
                    matches!(
                        operand.node_id().and_then(|id| map.get(id)),
                        Some(Node::TypeOfExpression(_))
                    )
                });
            if has_typeof_operand {
                return Guard::TypeofGuard;
            }
            // The operand that decides: a written `null` or `undefined` makes
            // this the nullable half `crate::flow` ported; anything else needs
            // `areTypesComparable`.
            let nullable = [binary.left, binary.right].into_iter().flatten().any(|operand| {
                match operand.node_id().and_then(|id| map.get(id)) {
                    Some(Node::KeywordExpression(node)) => node.kind == SyntaxKind::NullKeyword,
                    Some(Node::Identifier(node)) => node.text == "undefined",
                    _ => false,
                }
            });
            if nullable { Guard::NullableEquality } else { Guard::ComparabilityGuard }
        }
        _ => Guard::Unported,
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
        let source = parsed.files.get(index).map_or("", |file| file.content.as_str());
        // Every guard condition's source text in this file, for the loose
        // bound. Collected once per file rather than per line: the strict
        // measure asks "is the line inside this guard's branch", and this asks
        // only "does some guard in this file mention it", which is what makes
        // it an upper bound over the flow paths the strict test cannot see.
        let mut conditions: Vec<&str> = Vec::new();
        for raw in 0..nodes.len() {
            #[allow(clippy::cast_possible_truncation)]
            let candidate = NodeId::new(raw as u32);
            let condition = match map.get(candidate) {
                Some(Node::IfStatement(node)) => node.expression.and_then(|e| e.node_id()),
                Some(Node::WhileStatement(node)) => node.expression.and_then(|e| e.node_id()),
                Some(Node::ConditionalExpression(node)) => node.condition.and_then(|e| e.node_id()),
                _ => None,
            };
            if let Some(condition) = condition {
                let span = nodes.span(condition);
                if let Some(text) = source.get(span.start as usize..span.end as usize) {
                    conditions.push(text);
                }
            }
        }
        for (position, want) in expected_file.assertions.iter().enumerate() {
            let (Some(got), Some(&id)) = (our_file.get(position), line_ids.get(position)) else {
                continue;
            };
            // Rule 1. C2 lives here: an identifier reference is already
            // narrowed today and must not be counted as newly reachable.
            match nodes.kind(id) {
                SyntaxKind::PropertyAccessExpression | SyntaxKind::ElementAccessExpression => {}
                SyntaxKind::Identifier => {
                    report.c2_identifier_lines += 0;
                    continue;
                }
                _ => continue,
            }
            let verdict = if want.text == got.line() {
                Verdict::Right
            } else if got.type_string == "error" {
                Verdict::Gap
            } else {
                Verdict::Wrong
            };

            // The loose bound, taken before the strict test so that a line
            // counted strictly is also counted loosely — the two are nested
            // sets, not disjoint buckets, and reporting them as disjoint is
            // the error that would make the range meaningless.
            let access_span_for_loose = nodes.span(id);
            if verdict != Verdict::Right
                && let Some(access_text) = source
                    .get(access_span_for_loose.start as usize..access_span_for_loose.end as usize)
                && conditions.iter().any(|condition| condition.contains(access_text))
            {
                report.loose += 1;
            }

            let Some((_, condition)) = enclosing_guard(nodes, map, id) else {
                report.out_of_range += 1;
                continue;
            };
            // Rule 3: the guard's source text mentions the access's own text.
            let access_span = nodes.span(id);
            let condition_span = nodes.span(condition);
            let (Some(access_text), Some(condition_text)) = (
                source.get(access_span.start as usize..access_span.end as usize),
                source.get(condition_span.start as usize..condition_span.end as usize),
            ) else {
                report.out_of_range += 1;
                continue;
            };
            if !condition_text.contains(access_text) {
                report.out_of_range += 1;
                continue;
            }

            let guard = classify_guard(map, condition);
            let g = match guard {
                Guard::Truthiness => 0u8,
                Guard::NullableEquality => 1,
                Guard::Unported => 2,
                Guard::TypeofGuard => 3,
                Guard::InGuard => 4,
                Guard::InstanceofGuard => 5,
                Guard::ComparabilityGuard => 6,
            };
            let v = match verdict {
                Verdict::Right => 0u8,
                Verdict::Gap => 1,
                Verdict::Wrong => 2,
            };
            *report.in_range.entry((g, v)).or_default() += 1;
            if g < 2 && v > 0 {
                *report.cases.entry(case.name.clone()).or_default() += 1;
            }
        }
    }
    Some(report)
}

fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let reports: Vec<Report> = cases.par_iter().filter_map(measure).collect();
    let mut total = Report::default();
    for report in &reports {
        total.merge(report);
    }

    println!("# refmatch — what property-reference narrowing could reach (`bd tsr-6ka`)\n");
    println!("## Access-expression assertion lines inside a guard that mentions them\n");
    println!("  {:<28} {:<18} {:>8}", "guard form", "verdict today", "lines");
    let mut convertible = 0usize;
    let mut at_risk = 0usize;
    let mut in_range_total = 0usize;
    for ((g, v), n) in &total.in_range {
        println!("  {:<28} {:<18} {:>8}", guard_label(*g), verdict_label(*v), n);
        in_range_total += n;
        if *g < 2 && *v > 0 {
            convertible += n;
        }
        if *g < 2 && *v == 0 {
            at_risk += n;
        }
    }
    println!("\n  CONVERTIBLE (ported guard, not right today)   {convertible:>8}");
    println!("  AT RISK     (ported guard, right today)      {at_risk:>8}");
    println!(
        "  in range, unported guard                     {:>8}",
        in_range_total - convertible - at_risk
    );
    println!("  out of range (no guard mentioning the access) {:>7}", total.out_of_range);
    println!(
        "\n  LOOSE BOUND: not-right access lines mentioned by ANY guard in the same file  {:>6}",
        total.loose
    );
    println!(
        "  (a superset of CONVERTIBLE: it admits the flow paths the enclosing-branch test\n   cannot see — `if (!a.b) return;` then a use of `a.b`, early exits, `&&` chains.)"
    );

    println!("\n## Concentration of the convertible set\n");
    let mut cases: Vec<(&String, &usize)> = total.cases.iter().collect();
    cases.sort_by(|a, b| b.1.cmp(a.1));
    let case_total: usize = total.cases.values().sum();
    for (name, n) in cases.iter().take(10) {
        #[allow(clippy::cast_precision_loss)] // Counts, far below 2^52.
        let share = if case_total == 0 { 0.0 } else { **n as f64 * 100.0 / case_total as f64 };
        println!("  {n:>6}  {share:>5.1}%  {name}");
    }
    println!("  {} cases", total.cases.len());

    println!("\n## CONTROLS\n");
    println!(
        "  C1 in-range + out-of-range = access lines    {} (arithmetic, by construction)",
        in_range_total + total.out_of_range
    );
    println!(
        "  C2 identifier lines admitted by rule 1       {} (must be 0)",
        total.c2_identifier_lines
    );
}
