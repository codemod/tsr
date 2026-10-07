//! `this` where the container declares an explicit `this` parameter.
//!
//! # The population, named before the rule
//!
//! > **T is every rendered `.types` assertion line whose node kind is
//! > `ThisKeyword` and whose nearest **this-container** declares an explicit
//! > `this` parameter carrying a written type annotation.**
//!
//! The this-container is `getThisContainer(node, includeArrowFunctions: false,
//! …)` (`checker.go:12188`): the nearest enclosing function-like node **skipping
//! arrows**, which is what makes an arrow transparent to `this`. T is a function
//! of the tree alone, so `|T|` is invariant under any checker change — the
//! control this workstream has leaned on three times (`|P|` 7,430, `|Q|` 4,913,
//! `|G|` 2,082 across a week and four agents).
//!
//! # The rule, registered before the number was known
//!
//! - **RT-1 (size).** Ship only if the counterfactual converts **≥25% of T's gap
//!   lines** to exact baseline matches.
//! - **RT-2 (match).** And **≥70% of the lines that stop gapping match exactly.**
//!
//! Thresholds identical to `RC2` and `RA` on purpose, so the three are
//! comparable.
//!
//! # The forecast, with legs labelled and a bar on the inferred one
//!
//! - **Annotated leg.** The `this` parameter carries a written type node, so the
//!   answer is `get_type_from_type_node` of it and the renderer is already
//!   exercised. **Forecast 60–85% of T's gap converts.**
//! - **Inferred leg. Excluded by construction, forecast 0.** An unannotated
//!   `this` parameter takes its type from `get_type_of_symbol`, which answers the
//!   implicit `any` — and `checker-notes-rank.md` §6 forbids banking on `any`.
//!   T's definition requires the annotation, so nothing here can reach it.
//! - **Point forecast: 70 lines converted, error bars 45–95.** The bars are wide
//!   because of `bd tsr-a5d`: that source was ranked at 95 lines and the
//!   counterfactual reached 18, since **reachability inside a source is a second
//!   filter nobody had costed.** Here the filter is whether the annotation
//!   resolves — a class name, a keyword and a type literal do; a generic
//!   instantiation may not.
//!
//! Run: `cargo run --release -p tsr-conformance --example thisparam`

use std::collections::BTreeMap;

use rayon::prelude::*;
use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// The node kinds `getThisContainer` stops at with `includeArrowFunctions:
/// false` — an arrow is **not** one, which is what keeps it transparent.
const THIS_CONTAINER: [SyntaxKind; 6] = [
    SyntaxKind::FunctionDeclaration,
    SyntaxKind::FunctionExpression,
    SyntaxKind::MethodDeclaration,
    SyntaxKind::GetAccessor,
    SyntaxKind::SetAccessor,
    SyntaxKind::Constructor,
];

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Outcome {
    Right,
    Gap,
    Wrong,
}

#[derive(Default)]
struct Report {
    /// Every `ThisKeyword` line, by outcome.
    all: BTreeMap<Outcome, usize>,
    /// T — the container declares an annotated `this` parameter.
    t: BTreeMap<Outcome, usize>,
    /// T's gap lines by case, for concentration.
    t_gap_cases: BTreeMap<String, usize>,
    /// What the baseline wants for T's gap lines.
    t_rhs: BTreeMap<String, usize>,
    /// The container kind behind each T line, so the arm ordering is visible.
    t_container: BTreeMap<String, usize>,
    /// Control: a `this` parameter with **no** annotation. Excluded from T by
    /// construction, and printed so the exclusion is a number rather than a
    /// claim.
    unannotated_this_parameter: usize,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        for (key, n) in &other.all {
            *self.all.entry(*key).or_default() += n;
        }
        for (key, n) in &other.t {
            *self.t.entry(*key).or_default() += n;
        }
        for (key, n) in &other.t_gap_cases {
            *self.t_gap_cases.entry(key.clone()).or_default() += n;
        }
        for (key, n) in &other.t_rhs {
            *self.t_rhs.entry(key.clone()).or_default() += n;
        }
        for (key, n) in &other.t_container {
            *self.t_container.entry(key.clone()).or_default() += n;
        }
        self.unannotated_this_parameter += other.unannotated_this_parameter;
    }
}

/// The nearest this-container, and its `this` parameter's annotation if it has
/// one. Mirrors `getThisContainer` (`checker.go:12188`) with
/// `includeArrowFunctions: false`.
fn this_container(
    nodes: &tsr_ast::NodeTable,
    map: &tsr_ast::NodeMap<'_>,
    node: NodeId,
) -> Option<(NodeId, Option<bool>)> {
    let mut current = nodes.parent(node);
    while let Some(id) = current {
        if THIS_CONTAINER.contains(&nodes.kind(id)) {
            let parameters = match map.get(id) {
                Some(Node::FunctionDeclaration(n)) => n.parameters,
                Some(Node::FunctionExpression(n)) => n.parameters,
                Some(Node::MethodDeclaration(n)) => n.parameters,
                Some(Node::GetAccessorDeclaration(n)) => n.parameters,
                Some(Node::SetAccessorDeclaration(n)) => n.parameters,
                Some(Node::ConstructorDeclaration(n)) => n.parameters,
                _ => return Some((id, None)),
            };
            let annotated = parameters
                .first()
                .filter(|p| {
                    matches!(p.name, Some(tsr_ast::BindingName::Identifier(name))
                        if name.text == "this")
                })
                .map(|p| p.r#type.is_some());
            return Some((id, annotated));
        }
        current = nodes.parent(id);
    }
    None
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
    print(&total);
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

    let mut report = Report::default();
    let name = &case.name;

    for (index, expected_file) in expected.iter().enumerate() {
        let (Some(our_file), Some(line_ids)) = (ours.get(index), ids.get(index)) else { continue };
        if our_file.len() != line_ids.len() {
            continue;
        }
        for (position, assertion) in our_file.iter().enumerate() {
            let id = line_ids[position];
            if nodes.kind(id) != SyntaxKind::ThisKeyword {
                continue;
            }
            let baseline = expected_file.assertions.get(position);
            // The baseline is asked first: a line where we answer `error` and
            // upstream's baseline also says `error` is a right answer
            // (`9b10272`, `bd tsr-zlo`).
            let outcome = if baseline.is_some_and(|b| b.text == assertion.line()) {
                Outcome::Right
            } else if assertion.type_string == "error" {
                Outcome::Gap
            } else {
                Outcome::Wrong
            };
            *report.all.entry(outcome).or_default() += 1;

            let Some((container, annotated)) = this_container(nodes, map, id) else { continue };
            match annotated {
                Some(true) => {}
                Some(false) => {
                    report.unannotated_this_parameter += 1;
                    continue;
                }
                None => continue,
            }
            *report.t.entry(outcome).or_default() += 1;
            *report.t_container.entry(format!("{:?}", nodes.kind(container))).or_default() += 1;
            if outcome == Outcome::Gap {
                *report.t_gap_cases.entry(name.clone()).or_default() += 1;
                if let Some(rhs) =
                    baseline.and_then(|b| b.text.strip_prefix(&format!("{} : ", assertion.text)))
                {
                    *report.t_rhs.entry(rhs.to_string()).or_default() += 1;
                }
            }
        }
    }
    Some(report)
}

#[allow(clippy::cast_precision_loss)]
fn print(report: &Report) {
    let get = |m: &BTreeMap<Outcome, usize>, o: Outcome| m.get(&o).copied().unwrap_or(0);
    println!("# thisparam — `this` where the container annotates a `this` parameter\n");
    println!(
        "every ThisKeyword line: right {} + gap {} + wrong {} = {}",
        get(&report.all, Outcome::Right),
        get(&report.all, Outcome::Gap),
        get(&report.all, Outcome::Wrong),
        get(&report.all, Outcome::Right)
            + get(&report.all, Outcome::Gap)
            + get(&report.all, Outcome::Wrong)
    );
    let (tr, tg, tw) = (
        get(&report.t, Outcome::Right),
        get(&report.t, Outcome::Gap),
        get(&report.t, Outcome::Wrong),
    );
    println!(
        "\n|T| = {} lines: right {tr} + gap {tg} + wrong {tw}   <- invariant across runs",
        tr + tg + tw
    );
    println!("  RT-1 fires at >= {} converted lines (25% of T's gap)\n", tg / 4);

    let mut cases: Vec<_> = report.t_gap_cases.iter().collect();
    cases.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    let top10: usize = cases.iter().take(10).map(|(_, n)| **n).sum();
    println!(
        "T's gap concentration: {} cases, top-1 {:.1}%, top-10 {:.1}%",
        cases.len(),
        cases.first().map_or(0.0, |(_, n)| **n as f64 / tg.max(1) as f64 * 100.0),
        top10 as f64 / tg.max(1) as f64 * 100.0
    );
    for (case, n) in cases.iter().take(5) {
        println!("      {n:>4}  {case}");
    }

    println!("\nT's containers:");
    for (kind, n) in &report.t_container {
        println!("      {n:>4}  {kind}");
    }

    println!("\nwhat the baseline wants for T's gap lines:");
    let mut rhs: Vec<_> = report.t_rhs.iter().collect();
    rhs.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (text, n) in rhs.iter().take(15) {
        println!("      {n:>4}  {text}");
    }

    println!(
        "\ncontrol: `this` parameters with NO annotation, excluded from T by construction = {}",
        report.unannotated_this_parameter
    );
}
