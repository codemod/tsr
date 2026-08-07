//! Why an un-annotated `function f` gaps on its OWN name — `cyclegap.rs`'s
//! largest unowned row, split by whether the pieces of the signature compute.
//!
//! # The population
//!
//! `cyclegap.rs` showed `depend.rs`'s `cycle` ending is 98.6% a **length-1
//! self-loop**, and that its largest row is 2,943 lines rooted on the name of a
//! `FunctionDeclaration` with **neither a return annotation nor an
//! initialiser** — want-any 1.8%, 611 cases, and 1,894 of them wanting a
//! **signature / arrow**. The board never attributed it because a self-loop is
//! reported as a cycle rather than as a missing step arm.
//!
//! # The question this probe exists to answer
//!
//! The line gaps because `get_type_of_symbol` on the function's symbol answers
//! `error`. Two very different mechanisms produce that, and the fix for one is
//! not the fix for the other:
//!
//! - **wiring** — the signature's own pieces all compute (every parameter has a
//!   type, the body's returns type), and nothing assembles them into the
//!   function's type at this node. Cheap, and the whole row is reachable.
//! - **downstream** — a parameter or a returned expression gaps, so the
//!   signature could not be built whatever assembled it. The row then belongs to
//!   its inputs, exactly as `retgap.rs` concluded for the *other*
//!   `FunctionDeclaration` row (`checker-notes-callres.md` §12), and it is not
//!   an item.
//!
//! `STATUS.md`'s own rule is that a population identified by the shape of the
//! answer is not thereby attributed to a mechanism. This is that split, taken
//! before anything is costed.
//!
//! # Controls
//!
//! - **C1, frozen.** The population must reproduce `cyclegap.rs`'s 2,943. It is
//!   selected by the same walk; a different number means the copy drifted.
//! - **C2, construction.** The buckets are ordered **downstream-first**: a line
//!   with any gapping parameter or any gapping return expression is counted
//!   there and never reaches the wiring bucket. This is the direction that
//!   *under*-reports the cheap answer, which is the safe direction for a probe
//!   whose flattering result would invent work.
//! - **C3.** `want-any` is reported per bucket — ADR-0038 lines are unreachable
//!   whatever the split says.

use std::collections::{BTreeMap, HashMap, HashSet};

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Node, NodeId, NodeMap, NodeTable, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// Does this node's own answer gap? Copied from `depend.rs`.
fn gaps<'a>(
    checker: &mut tsr_checker::Checker<'a, '_>,
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    id: NodeId,
) -> bool {
    let error = checker.intrinsics().error;
    if let Some(node) = map.get(id)
        && let Ok(type_node) = tsr_ast::TypeNode::try_from(node)
    {
        return checker.get_type_from_type_node(type_node) == error;
    }
    types_producer::type_id_at_location(checker, binder, nodes, map, id) == error
}

/// One step toward what this node's answer depends on. Copied from `depend.rs`
/// so the population is the same one `cyclegap.rs` reports — see C1.
fn step<'a>(
    checker: &mut tsr_checker::Checker<'a, '_>,
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    id: NodeId,
) -> Option<NodeId> {
    let node = map.get(id)?;

    if let Some(parent) = nodes.parent(id)
        && let Some(parent_node) = map.get(parent)
        && parent_node.name_id() == Some(id)
        && !matches!(parent_node, Node::PropertyAccessExpression(_) | Node::QualifiedName(_))
    {
        if let Some(annotation) = parent_node.type_id() {
            return Some(annotation);
        }
        if let Some(initializer) = parent_node.initializer_id() {
            return Some(initializer);
        }
    }

    if let Some(parent) = nodes.parent(id)
        && let Some(parent_node) = map.get(parent)
        && parent_node.name_id() == Some(id)
        && matches!(parent_node, Node::PropertyAccessExpression(_) | Node::QualifiedName(_))
    {
        return parent_node.expression_id().or_else(|| match parent_node {
            Node::QualifiedName(qualified) => qualified.left.and_then(|l| l.node_id()),
            _ => None,
        });
    }

    match node {
        Node::PropertyAccessExpression(_)
        | Node::ElementAccessExpression(_)
        | Node::CallExpression(_)
        | Node::NewExpression(_)
        | Node::ParenthesizedExpression(_)
        | Node::AsExpression(_)
        | Node::NonNullExpression(_) => node.expression_id(),
        Node::TypeReferenceNode(reference) => {
            let name = reference.type_name?;
            let text = match name {
                tsr_ast::EntityName::Identifier(identifier) => identifier.text,
                tsr_ast::EntityName::QualifiedName(qualified) => qualified.right?.text,
            };
            let symbol = binder.resolve_name(nodes, map, id, text, SymbolFlags::TYPE)?;
            let declaration = binder.symbols().get(symbol).declarations.first().copied()?;
            map.get(declaration)?.name_id()
        }
        Node::Identifier(identifier) => {
            let symbol =
                binder.resolve_name(nodes, map, id, identifier.text, SymbolFlags::VALUE)?;
            if checker.get_type_of_symbol(symbol) != checker.intrinsics().error {
                return None;
            }
            let declaration = binder.symbols().get(symbol).value_declaration?;
            map.get(declaration)?.name_id()
        }
        Node::BinaryExpression(binary) => {
            let left = binary.left.and_then(|e| e.node_id());
            let right = binary.right.and_then(|e| e.node_id());
            for operand in [left, right].into_iter().flatten() {
                if types_producer::type_id_at_location(checker, binder, nodes, map, operand)
                    == checker.intrinsics().error
                {
                    return Some(operand);
                }
            }
            None
        }
        _ => None,
    }
}

/// The self-loop test, stated positively: this identifier is a declaration name,
/// its declaration carries neither annotation nor initialiser, and resolving it
/// as a value lands back on the same node.
fn is_self_loop<'a>(
    checker: &mut tsr_checker::Checker<'a, '_>,
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    id: NodeId,
) -> bool {
    let Some(Node::Identifier(identifier)) = map.get(id) else { return false };
    let Some(parent) = nodes.parent(id) else { return false };
    let Some(parent_node) = map.get(parent) else { return false };
    if parent_node.name_id() != Some(id) {
        return false;
    }
    if parent_node.type_id().is_some() || parent_node.initializer_id().is_some() {
        return false;
    }
    let Some(symbol) = binder.resolve_name(nodes, map, id, identifier.text, SymbolFlags::VALUE)
    else {
        return false;
    };
    if checker.get_type_of_symbol(symbol) != checker.intrinsics().error {
        return false;
    }
    binder
        .symbols()
        .get(symbol)
        .value_declaration
        .and_then(|d| map.get(d))
        .and_then(|d| d.name_id())
        == Some(id)
}

#[derive(Default)]
struct Report {
    population: usize,
    /// `bucket -> lines`, downstream-first (C2).
    buckets: BTreeMap<&'static str, usize>,
    bucket_any: BTreeMap<&'static str, usize>,
    bucket_cases: BTreeMap<&'static str, HashMap<String, usize>>,
    /// For the wiring bucket, a sample so the claim is checkable by hand.
    samples: Vec<String>,
    /// Parameter count profile of the wiring bucket — a zero-parameter function
    /// whose body returns a literal is a different build from a generic one.
    wiring_params: BTreeMap<usize, usize>,
    /// 0 = the line's own node is the root, 1 = it arrived after ≥1 steps.
    depths: BTreeMap<usize, usize>,
    /// The wiring bucket by function form. An `async` function's inferred return
    /// is `Promise<T>` and a generator's is `Generator<…>`; both are separate
    /// mechanisms from returning the body's type, and the first sample dump was
    /// full of `Promise<void>`.
    wiring_forms: BTreeMap<&'static str, usize>,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        self.population += other.population;
        for (k, n) in &other.buckets {
            *self.buckets.entry(k).or_default() += n;
        }
        for (k, n) in &other.bucket_any {
            *self.bucket_any.entry(k).or_default() += n;
        }
        for (k, cases) in &other.bucket_cases {
            let mine = self.bucket_cases.entry(k).or_default();
            for (case, n) in cases {
                *mine.entry(case.clone()).or_default() += n;
            }
        }
        for (k, n) in &other.wiring_params {
            *self.wiring_params.entry(*k).or_default() += n;
        }
        for (k, n) in &other.depths {
            *self.depths.entry(*k).or_default() += n;
        }
        for (k, n) in &other.wiring_forms {
            *self.wiring_forms.entry(k).or_default() += n;
        }
        for sample in &other.samples {
            if self.samples.len() < 40 {
                self.samples.push(sample.clone());
            }
        }
    }
}

/// Every `return` expression inside this function, not descending into nested
/// functions — a nested function's returns belong to *its* signature.
fn return_expressions(nodes: &NodeTable, map: &NodeMap<'_>, function: NodeId) -> Vec<NodeId> {
    let mut found = Vec::new();
    let mut stack = vec![function];
    let mut first = true;
    while let Some(id) = stack.pop() {
        if !first
            && matches!(
                nodes.kind(id),
                SyntaxKind::FunctionDeclaration
                    | SyntaxKind::FunctionExpression
                    | SyntaxKind::ArrowFunction
                    | SyntaxKind::MethodDeclaration
            )
        {
            continue;
        }
        first = false;
        if nodes.kind(id) == SyntaxKind::ReturnStatement
            && let Some(node) = map.get(id)
            && let Some(expression) = node.expression_id()
        {
            found.push(expression);
        }
        if let Some(node) = map.get(id) {
            tsr_ast::for_each_child_id(node, |child| stack.push(child));
        }
    }
    found
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
            let Some(want_type) = want.text.strip_prefix(&format!("{} : ", got.text)) else {
                continue;
            };
            if got.type_string != "error" {
                continue;
            }
            // C1 fired on the first run at 1,475 against `cyclegap.rs`'s 2,943,
            // and the diagnosis was this probe's SELECTION, not the row: it
            // tested the line's own node, while `cyclegap.rs` reports where the
            // line's dependency walk *ends*. Half of the row arrives after one
            // or more steps. The walk is now the same one, and the own-node
            // subset is reported as a split instead of standing in for the row.
            let mut current = line_ids[position];
            let mut visited: HashSet<NodeId> = HashSet::new();
            visited.insert(current);
            let mut depth = 0usize;
            let root = loop {
                let Some(next) = step(&mut checker, bound, nodes, map, current) else {
                    break current;
                };
                if !gaps(&mut checker, bound, nodes, map, next) {
                    break current;
                }
                if !visited.insert(next) {
                    break current;
                }
                depth += 1;
                if depth >= 16 {
                    break current;
                }
                current = next;
            };
            let id = root;
            let Some(parent) = nodes.parent(id) else { continue };
            if nodes.kind(parent) != SyntaxKind::FunctionDeclaration {
                continue;
            }
            if !is_self_loop(&mut checker, bound, nodes, map, id) {
                continue;
            }
            report.population += 1;
            *report.depths.entry(usize::from(depth > 0)).or_default() += 1;

            // C2: downstream first. A parameter that gaps, or a returned
            // expression that gaps, disqualifies the line from `wiring`.
            let parameters: Vec<NodeId> = match map.get(parent) {
                Some(Node::FunctionDeclaration(function)) => function
                    .parameters
                    .iter()
                    .filter_map(|p| p.name.and_then(|n| n.node_id()))
                    .collect(),
                _ => Vec::new(),
            };
            let gapping_parameters =
                parameters.iter().filter(|p| gaps(&mut checker, bound, nodes, map, **p)).count();

            let returns = return_expressions(nodes, map, parent);
            let gapping_returns =
                returns.iter().filter(|r| gaps(&mut checker, bound, nodes, map, **r)).count();

            let bucket = if gapping_parameters > 0 && gapping_returns > 0 {
                "downstream: a parameter AND a return expression gap"
            } else if gapping_parameters > 0 {
                "downstream: a parameter gaps"
            } else if gapping_returns > 0 {
                "downstream: a returned expression gaps"
            } else if returns.is_empty() {
                "WIRING: no return statement at all — the answer is `void`"
            } else {
                "WIRING: every parameter and every return expression types"
            };

            if bucket.starts_with("WIRING") {
                let (is_async, is_generator) = match map.get(parent) {
                    Some(Node::FunctionDeclaration(function)) => (
                        function.modifiers.iter().any(|m| {
                            m.node_id().is_some_and(|i| nodes.kind(i) == SyntaxKind::AsyncKeyword)
                        }),
                        function.asterisk_token.is_some(),
                    ),
                    _ => (false, false),
                };
                let form = match (is_async, is_generator) {
                    (true, true) => "async generator — wants AsyncGenerator<…>",
                    (true, false) => "async — wants Promise<T>",
                    (false, true) => "generator — wants Generator<…>",
                    (false, false) => "plain — wants the body's type",
                };
                *report.wiring_forms.entry(form).or_default() += 1;
                report
                    .wiring_params
                    .entry(parameters.len().min(4))
                    .and_modify(|n| *n += 1)
                    .or_insert(1);
                if report.samples.len() < 40 {
                    report.samples.push(format!(
                        "{:<46} want {:<44} params {} returns {}",
                        case.name,
                        &want_type[..want_type.len().min(44)],
                        parameters.len(),
                        returns.len()
                    ));
                }
            }
            *report.buckets.entry(bucket).or_default() += 1;
            if want_type == "any" {
                *report.bucket_any.entry(bucket).or_default() += 1;
            }
            *report
                .bucket_cases
                .entry(bucket)
                .or_default()
                .entry(case.name.clone())
                .or_default() += 1;
        }
    }
    Some(report)
}

#[allow(clippy::cast_precision_loss)]
fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    assert!(corpus.is_available(), "corpus missing");
    let cases = corpus.discover().expect("cases");
    let report = cases
        .par_iter()
        .filter_map(measure)
        .fold(Report::default, |mut a, r| {
            a.merge(&r);
            a
        })
        .reduce(Report::default, |mut a, b| {
            a.merge(&b);
            a
        });

    println!("# fnsiggap — why an un-annotated `function f` gaps on its own name\n");
    println!("population: {} (C1: cyclegap.rs reads 2,943)\n", report.population);

    println!("## Split, downstream-first (C2)\n");
    let mut rows: Vec<_> = report.buckets.iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (bucket, n) in &rows {
        let any = report.bucket_any.get(*bucket).copied().unwrap_or(0);
        let empty = HashMap::new();
        let cases = report.bucket_cases.get(*bucket).unwrap_or(&empty);
        let (top, top_n) =
            cases.iter().max_by_key(|(_, n)| **n).map_or((" ", &0), |(c, n)| (c.as_str(), n));
        println!(
            "  {n:>5}  {:>5.1}%  want-any {any:>4}  net {:>5}  {:>4} cases, top-1 {:>5.1}% {top}\n         {bucket}",
            **n as f64 / report.population.max(1) as f64 * 100.0,
            **n - any,
            cases.len(),
            *top_n as f64 / (**n).max(1) as f64 * 100.0,
        );
    }

    println!("\n## Where the line sits relative to the root\n");
    for (depth, n) in &report.depths {
        let label = if *depth == 0 { "the line's OWN node" } else { "reached after >=1 step" };
        println!("  {label:<24} {n:>5}");
    }

    println!("\n## The WIRING bucket by function form — these are different builds\n");
    let mut forms: Vec<_> = report.wiring_forms.iter().collect();
    forms.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (form, n) in &forms {
        println!("  {n:>5}  {form}");
    }

    println!("\n## The wiring bucket by parameter count (4 = 4 or more)\n");
    for (count, n) in &report.wiring_params {
        println!("  {count} params  {n:>5}");
    }

    println!("\n## Wiring samples — checkable by hand\n");
    for sample in report.samples.iter().take(25) {
        println!("  {sample}");
    }
}
