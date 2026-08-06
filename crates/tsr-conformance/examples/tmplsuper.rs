//! `TemplateExpression` and `SuperKeyword` — the two purest terminal rows.
//!
//! # Populations, named before the rule, syntactically pinned
//!
//! > **M is every rendered `.types` line whose node kind is `TemplateExpression`.**
//! > **S is every rendered `.types` line whose node kind is `SuperKeyword`.**
//!
//! Both are functions of the tree alone, so `|M|` and `|S|` are invariant under
//! any checker change — the control this workstream has now carried four times
//! (`|P|` 7,430, `|Q|` 4,913, `|G|` 2,082, `|T|` 128).
//!
//! Note `TemplateExpression` is **not** `NoSubstitutionTemplateLiteral`: a
//! template with no `${}` is a different node kind and a different rule. A first
//! survey of the baselines conflated them and made the constant-folding tail look
//! twice its size.
//!
//! # The rules, registered before the numbers exist
//!
//! - **RM-1 / RS-1 (size).** Ship a row only if the counterfactual converts
//!   **≥25% of that row's gap lines** to exact baseline matches.
//! - **RM-2 / RS-2 (match).** And **≥70% of the lines that stop gapping match
//!   exactly.**
//!
//! Thresholds identical to `RC2`, `RA` and `RT`, on purpose. **Scored per row**,
//! not pooled: `SuperKeyword` and `TemplateExpression` are different mechanisms
//! and pooling them would let one carry the other.
//!
//! # Forecasts, legs labelled, bar on the predicted leg
//!
//! ## `TemplateExpression`
//!
//! `checkTemplateExpression` (`checker.go:7976`) has three exits, in order:
//!
//! 1. **constant-folding leg.** `c.evaluate(node, node).Value` non-nil — every
//!    span is a compile-time constant — answers a **fresh string literal type**.
//!    Needs a constant evaluator this port does not have.
//! 2. **template-literal leg.** A `const` context, an element-access argument, or
//!    a template-literal contextual type answers `getTemplateLiteralType`. Needs
//!    contextual typing, which `checker-notes-fnexpr.md` §10 measured as 86%
//!    entangled.
//! 3. **`string` leg — the predicted one.** Everything else. **The bar goes
//!    here.**
//!
//! **A span that is not constant makes leg 1 unreachable by construction**, so a
//! conservative port answers `string` only when at least one span's type is
//! *not* a unit type, and gaps otherwise. That is the safe direction: it can
//! only decline lines leg 1 would have taken, never claim them.
//!
//! **Forecast: 55–80% of M's gap converts; point 65%.** The bars are wide
//! because the same over-caution that cost `bd tsr-a5d` 77 of its 95 lines
//! applies here in reverse — the exclusion is cheap to state and its size is
//! unknown until this probe prints it.
//!
//! ## `SuperKeyword`
//!
//! `checkSuperExpression` (`checker.go:7854`) answers the base class's
//! **instance** type in an instance member and its **static** side in a static
//! one. The corpus-wide `>super :` answers are `any` 189, `typeof Base` 126,
//! `A` 118, `typeof C` 60 — so:
//!
//! - **base-class leg — the predicted one.** The container's class has a written
//!   `extends` clause naming a class this port can resolve. **The bar goes
//!   here.**
//! - **`any` leg. Excluded by construction, forecast 0.** 189 corpus-wide
//!   `>super :` lines print `any`, and `checker-notes-rank.md` §6 forbids banking
//!   on it. Where the base does not resolve, this must gap.
//!
//! **Forecast: 45–75% of S's gap converts; point 60%.** Lower than the template
//! row because the static/instance split and the `any` exclusion both bite, and
//! neither is sized yet.
//!
//! # What this probe prints, and why each column decides something
//!
//! For each row: the outcome split, the baseline's own right-hand side for the
//! gap lines (the **match**-test evidence, never a shape test — `3f140c2`), and
//! the one syntactic split that separates the predicted leg from the excluded
//! one. For M that is *"is every span a unit type"*; for S it is *"does the
//! enclosing class write an `extends` clause"*.
//!
//! Run: `cargo run --release -p tsr-conformance --example tmplsuper`

use std::collections::BTreeMap;

use rayon::prelude::*;
use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Outcome {
    Right,
    Gap,
    Wrong,
}

#[derive(Default)]
struct Row {
    outcomes: BTreeMap<Outcome, usize>,
    gap_rhs: BTreeMap<(bool, String), usize>,
    gap_cases: BTreeMap<String, usize>,
    /// The syntactic split that separates the predicted leg from the excluded
    /// one. `true` is the predicted leg.
    split: BTreeMap<(bool, Outcome), usize>,
    wrong_pairs: BTreeMap<(String, String), usize>,
}

impl Row {
    fn merge(&mut self, other: &Self) {
        for (key, n) in &other.outcomes {
            *self.outcomes.entry(*key).or_default() += n;
        }
        for (key, n) in &other.gap_rhs {
            *self.gap_rhs.entry(key.clone()).or_default() += n;
        }
        for (key, n) in &other.gap_cases {
            *self.gap_cases.entry(key.clone()).or_default() += n;
        }
        for (key, n) in &other.split {
            *self.split.entry(*key).or_default() += n;
        }
        for (key, n) in &other.wrong_pairs {
            *self.wrong_pairs.entry(key.clone()).or_default() += n;
        }
    }

    fn get(&self, outcome: Outcome) -> usize {
        self.outcomes.get(&outcome).copied().unwrap_or(0)
    }
}

#[derive(Default)]
struct Report {
    template: Row,
    super_keyword: Row,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        self.template.merge(&other.template);
        self.super_keyword.merge(&other.super_keyword);
    }
}

/// Whether every span of a template expression has a **unit** type, which is the
/// necessary condition for upstream's `evaluate` to have folded it
/// (`checker.go:7991`). `false` means leg 1 is unreachable and `string` is the
/// answer.
fn every_span_is_a_unit(
    checker: &mut tsr_checker::Checker<'_, '_>,
    map: &tsr_ast::NodeMap<'_>,
    id: NodeId,
) -> bool {
    let Some(Node::TemplateExpression(node)) = map.get(id) else { return true };
    node.template_spans.iter().all(|span| {
        span.expression.is_none_or(|expression| {
            let type_id = checker.check_expression(expression);
            checker.type_of(type_id).flags.intersects(tsr_checker::TypeFlags::UNIT)
        })
    })
}

/// Whether the class enclosing this `super` writes an `extends` clause. The
/// predicted leg for S.
fn class_writes_extends(
    nodes: &tsr_ast::NodeTable,
    map: &tsr_ast::NodeMap<'_>,
    id: NodeId,
) -> bool {
    let mut current = nodes.parent(id);
    while let Some(node) = current {
        if matches!(nodes.kind(node), SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression) {
            let heritage = match map.get(node) {
                Some(Node::ClassDeclaration(class)) => class.heritage_clauses,
                Some(Node::ClassExpression(class)) => class.heritage_clauses,
                _ => return false,
            };
            return heritage.iter().any(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword);
        }
        current = nodes.parent(node);
    }
    false
}

fn main() {
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    assert!(corpus.is_available(), "corpus missing; run git submodule update --init --recursive");
    let cases = corpus.discover().expect("discovering cases");
    let reports: Vec<Report> = cases.par_iter().filter_map(measure).collect();
    let mut total = Report::default();
    for report in reports {
        total.merge(&report);
    }
    print("TemplateExpression", "every span is a unit type", &total.template);
    print("SuperKeyword", "the class writes `extends`", &total.super_keyword);
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

    let mut report = Report::default();
    let name = &case.name;

    for (index, expected_file) in expected.iter().enumerate() {
        let (Some(our_file), Some(line_ids)) = (ours.get(index), ids.get(index)) else { continue };
        if our_file.len() != line_ids.len() {
            continue;
        }
        for (position, assertion) in our_file.iter().enumerate() {
            let id = line_ids[position];
            let kind = nodes.kind(id);
            if !matches!(kind, SyntaxKind::TemplateExpression | SyntaxKind::SuperKeyword) {
                continue;
            }
            let baseline = expected_file.assertions.get(position);
            // Baseline first (`9b10272`, `bd tsr-zlo`).
            let outcome = if baseline.is_some_and(|b| b.text == assertion.line()) {
                Outcome::Right
            } else if assertion.type_string == "error" {
                Outcome::Gap
            } else {
                Outcome::Wrong
            };
            // The predicted leg's syntactic test. For M, "not every span is a
            // unit" is the predicted leg, so it is negated here.
            let predicted = if kind == SyntaxKind::TemplateExpression {
                !every_span_is_a_unit(&mut checker, map, id)
            } else {
                class_writes_extends(nodes, map, id)
            };
            let row = if kind == SyntaxKind::TemplateExpression {
                &mut report.template
            } else {
                &mut report.super_keyword
            };
            *row.outcomes.entry(outcome).or_default() += 1;
            *row.split.entry((predicted, outcome)).or_default() += 1;
            if outcome == Outcome::Wrong
                && let Some(rhs) =
                    baseline.and_then(|b| b.text.strip_prefix(&format!("{} : ", assertion.text)))
            {
                *row.wrong_pairs
                    .entry((assertion.type_string.clone(), rhs.to_string()))
                    .or_default() += 1;
            }
            if outcome == Outcome::Gap {
                *row.gap_cases.entry(name.clone()).or_default() += 1;
                if let Some(rhs) =
                    baseline.and_then(|b| b.text.strip_prefix(&format!("{} : ", assertion.text)))
                {
                    *row.gap_rhs.entry((predicted, rhs.to_string())).or_default() += 1;
                }
            }
        }
    }
    Some(report)
}

#[allow(clippy::cast_precision_loss)]
fn print(name: &str, split_label: &str, row: &Row) {
    let (right, gap, wrong) =
        (row.get(Outcome::Right), row.get(Outcome::Gap), row.get(Outcome::Wrong));
    println!("\n# {name}\n");
    println!("|{name}| = {} lines: right {right} + gap {gap} + wrong {wrong}", right + gap + wrong);
    println!("  the rule fires at >= {} converted lines (25% of the gap)", gap / 4);

    let mut cases: Vec<_> = row.gap_cases.iter().collect();
    cases.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    let top10: usize = cases.iter().take(10).map(|(_, n)| **n).sum();
    println!(
        "  gap concentration: {} cases, top-1 {:.1}%, top-10 {:.1}%",
        cases.len(),
        cases.first().map_or(0.0, |(_, n)| **n as f64 / gap.max(1) as f64 * 100.0),
        top10 as f64 / gap.max(1) as f64 * 100.0
    );
    for (case, n) in cases.iter().take(4) {
        println!("      {n:>5}  {case}");
    }

    println!("\n  the predicted leg — {split_label}:");
    for predicted in [true, false] {
        let get = |o: Outcome| row.split.get(&(predicted, o)).copied().unwrap_or(0);
        println!(
            "      {:<5} right {:>5}  gap {:>5}  wrong {:>5}",
            if predicted { "yes" } else { "no" },
            get(Outcome::Right),
            get(Outcome::Gap),
            get(Outcome::Wrong)
        );
    }

    if !row.wrong_pairs.is_empty() {
        println!("\n  WRONG: ours against upstream");
        let mut pairs: Vec<_> = row.wrong_pairs.iter().collect();
        pairs.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
        for ((ours, them), n) in pairs.iter().take(10) {
            println!("      {n:>5}  ours {ours:<28} them {them}");
        }
    }
    // The decisive cross-tab: what the baseline wants **inside the predicted
    // leg**. The whole-row histogram cannot decide a build, because a line the
    // predicted leg excludes contributes to it and would never have converted.
    for predicted in [true, false] {
        let mut rhs: Vec<_> = row.gap_rhs.iter().filter(|((p, _), _)| *p == predicted).collect();
        rhs.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
        let with_rhs: usize = rhs.iter().map(|(_, n)| **n).sum();
        if with_rhs == 0 {
            continue;
        }
        println!(
            "\n  baseline for the gap lines, predicted leg = {predicted} ({with_rhs} with a comparable RHS):"
        );
        for ((_, text), n) in rhs.iter().take(10) {
            println!("      {:>5}  {:>5.1}%  {}", n, **n as f64 / with_rhs as f64 * 100.0, text);
        }
    }
}
