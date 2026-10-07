//! Sizing probe for `typeof x` in type position — `bd tsr-4sc.10`.
//!
//! `depend.rs` reports the `TypeQuery` root row at 1,728 gap lines. That is a
//! *row*, and STATUS.md's fourth rule forbids quoting it as work: this probe
//! measures the **mechanism's own population** instead. For every gap line
//! whose declaration-edge walk (same `step`/`gaps` as `depend.rs`) ends at a
//! `TypeQueryNode`, classify by what `getTypeFromTypeQueryNode`
//! (`checker.go:24102`) would have to do:
//!
//! - the `ExprName` form — `this`, `Identifier`, `QualifiedName` (with depth);
//! - whether type arguments are present (an *instantiation expression*,
//!   `getInstantiationExpressionType`, `checker.go:10660` — unported, and the
//!   whole construct will refuse rather than approximate);
//! - for an `Identifier` exprName: whether `check_expression` on it answers
//!   non-error **today** — the half the arm can convert without any other
//!   build — and for a `QualifiedName`: whether the leftmost base identifier
//!   types today, which bounds what a `checkQualifiedName` twin could reach;
//! - the `want-any` share, which ADR-0038 makes unmatchable.
//!
//! Controls:
//! - C1 (construction): every classified line's root is a `TypeQueryNode`
//!   whose own type gaps — expect 0 violations.
//! - C2 (arithmetic): the form buckets sum to the classified total.
//! - C3 (frozen, pinned to the sibling instrument): the classified total is
//!   compared against `depend.rs`'s published 1,728 for the same tree.

use std::collections::{BTreeMap, HashSet};

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Expression, Node, NodeId, NodeMap, NodeTable};
use tsr_binder::SymbolFlags;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

const MAX_DEPTH: usize = 16;

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

/// One declaration-edge step — a copy of `depend.rs`'s `step`, kept in sync by
/// eye rather than shared, because `depend.rs` is a published instrument whose
/// output STATUS.md quotes and this probe must not be able to change it.
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
        _ => None,
    }
}

#[derive(Default)]
struct Report {
    classified: usize,
    forms: BTreeMap<String, usize>,
    wants_any: BTreeMap<String, usize>,
    c1_root_not_type_query_gap: usize,
    cases: BTreeMap<String, usize>,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        self.classified += other.classified;
        self.c1_root_not_type_query_gap += other.c1_root_not_type_query_gap;
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

/// The base of an entity name: the `a` of `a.b.c`, or the identifier itself.
fn entity_base(name: tsr_ast::EntityName<'_>) -> Option<&tsr_ast::Identifier<'_>> {
    let mut current = name;
    loop {
        match current {
            tsr_ast::EntityName::Identifier(identifier) => return Some(identifier),
            tsr_ast::EntityName::QualifiedName(qualified) => current = qualified.left?,
        }
    }
}

fn entity_depth(name: tsr_ast::EntityName<'_>) -> usize {
    let mut depth = 0;
    let mut current = name;
    while let tsr_ast::EntityName::QualifiedName(qualified) = current {
        depth += 1;
        match qualified.left {
            Some(left) => current = left,
            None => break,
        }
    }
    depth
}

fn classify<'a>(
    checker: &mut tsr_checker::Checker<'a, '_>,
    query: &tsr_ast::TypeQueryNode<'a>,
) -> String {
    let has_arguments = !query.type_arguments.is_empty();
    let Some(name) = query.expr_name else { return "no exprName (parse recovery)".into() };
    if has_arguments {
        return "instantiation expression (type args) — refuses whole-construct".into();
    }
    match name {
        tsr_ast::EntityName::Identifier(identifier) => {
            if identifier.text == "this" {
                return "typeof this — refuses".into();
            }
            let answered = checker.check_expression(Expression::Identifier(identifier))
                != checker.intrinsics().error;
            if answered {
                "identifier, name types TODAY".into()
            } else {
                "identifier, name still gaps".into()
            }
        }
        qualified @ tsr_ast::EntityName::QualifiedName(_) => {
            let depth = entity_depth(qualified);
            let base_types = entity_base(qualified).is_some_and(|base| {
                checker.check_expression(Expression::Identifier(base)) != checker.intrinsics().error
            });
            format!(
                "qualified depth {depth}, base {}",
                if base_types { "types TODAY" } else { "still gaps" }
            )
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
        types_producer::assertions_for_case_with_ids(&arena, &parsed, &parsed.files.as_slice());
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
            if want.text.strip_prefix(&format!("{} : ", got.text)).is_none() {
                continue;
            }
            if got.type_string != "error" {
                continue;
            }

            let mut current = line_ids[position];
            let mut visited: HashSet<NodeId> = HashSet::new();
            visited.insert(current);
            let mut depth = 0usize;
            while let Some(next) = step(&mut checker, bound, nodes, map, current) {
                if !gaps(&mut checker, bound, nodes, map, next) {
                    break;
                }
                if !visited.insert(next) {
                    break;
                }
                depth += 1;
                if depth >= MAX_DEPTH {
                    break;
                }
                current = next;
            }

            let Some(Node::TypeQueryNode(query)) = map.get(current) else { continue };
            report.classified += 1;
            if !gaps(&mut checker, bound, nodes, map, current) {
                report.c1_root_not_type_query_gap += 1;
            }
            let form = classify(&mut checker, query);
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

    println!("# tquery — the TypeQuery root row, by mechanism form (bd tsr-4sc.10)\n");
    println!("classified gap lines whose root is a TypeQueryNode: {}\n", report.classified);
    println!("## Form buckets (C2: must sum to the total)\n");
    let mut sum = 0usize;
    let mut rows: Vec<_> = report.forms.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1));
    for (form, n) in rows {
        let any = report.wants_any.get(form).copied().unwrap_or(0);
        sum += n;
        println!("  {n:>6}  want-any {any:>5}  {form}");
    }
    println!("\n  C2 arithmetic: buckets sum {sum} vs classified {}", report.classified);
    println!(
        "  C1 construction: classified roots that do not gap {}  (expect 0)",
        report.c1_root_not_type_query_gap
    );
    println!("  C3 frozen: depend.rs published 1,728 TypeQuery root lines for this tree");
    let mut cases: Vec<_> = report.cases.iter().collect();
    cases.sort_by(|a, b| b.1.cmp(a.1));
    println!("\n## Top cases\n");
    for (case, n) in cases.into_iter().take(10) {
        println!("  {n:>6}  {case}");
    }
}
