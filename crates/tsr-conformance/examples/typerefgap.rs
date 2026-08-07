//! `STATUS.md` §4.3's registered probe for the `TypeReference` row:
//! *"3,855 — `TypeReference`, no further dependency — but top-1 is 51.1%
//! (`resolvingClassDeclarationWhenInBaseTypeResolution`), so ~1,988 is one case
//! and the real row is ~1,867 of unknown cause. Split by case, then by why the
//! reference resolves to nothing."*
//!
//! # What defines the row
//!
//! The row is a `depend.rs` bucket, so it is defined by `depend.rs`'s walk and
//! not by anything this file invents: a gap line whose dependency chain
//! terminates on a `TypeReferenceNode` with the ending `no further dependency`.
//! [`gaps`], [`step`] and [`has_step_arm`] below are **copies** of that walk.
//! They are copied rather than approximated because a probe that re-derives a
//! published row must use the row's own definition — a re-implementation that
//! drifts by one edge measures a different population and reports it under the
//! old number.
//!
//! `no further dependency` at a `TypeReferenceNode` means `depend.rs`'s step
//! returned `None`, which happens when the name has no `node_id`, when
//! `resolve_name(.., TYPE)` fails, when the resolved symbol has no declarations,
//! or when its first declaration has no name. That is a **property of the walk**
//! as much as of the compiler, so this probe reports the other ending
//! (`the dependency types — the root is here`) beside it rather than pretending
//! the row is the whole `TypeReferenceNode` population.
//!
//! # The mechanism split
//!
//! Every bucket is a branch of `Checker::get_type_from_type_reference`
//! (`declared.rs`), which is the port of `getTypeReferenceType`
//! (`checker.go:23146`). The branches are taken in the checker's own order, so
//! a line is attributed to the mechanism that actually decided its answer:
//!
//! - a **qualified** name whose leftmost identifier resolves as a namespace is
//!   refused deliberately — upstream resolves it through `resolveQualifiedName`
//!   (`checker.go:15828`), which this port has not built;
//! - a name that does **not** resolve is *not* a gap by itself: upstream mints a
//!   synthetic unresolved symbol and renders the written text
//!   (`getUnresolvedSymbolForEntityName`, `checker.go:23102`), ported at
//!   `d356450` for +4,645 lines. Such a reference reaches this row only when one
//!   of its type **arguments** gaps;
//! - type arguments written on a non-generic type answer `errorType` per
//!   `checkNoTypeArguments` (`checker.go:23157`);
//! - an argument count that does not equal the parameter count needs
//!   `fillMissingTypeArguments` and the arity check at `checker.go:23189`.
//!
//! # Downstream first
//!
//! Three branches are **not** this row's item: the name resolves and the
//! referenced declaration's own declared type gaps; the reference instantiates
//! correctly but a type argument gaps; the name is unresolved and an argument
//! gaps. Each is a line that would still gap after every mechanism here was
//! built. They are counted and reported **before** the mechanism table, on
//! `docs/conventions.md`'s rule from the object-literal row — where the
//! equivalent bucket was 77%.
//!
//! Because the qualified-name branch answers `errorType` *before* it looks at
//! its arguments, "a type argument also gaps" cannot be a bucket for it without
//! double-attributing. It is reported as an orthogonal flag column instead.
//!
//! # Controls, with their expected values fixed before the run
//!
//! - **C1, construction.** Every classified line's root answers `errorType`.
//!   Expect **0** violations.
//! - **C2, arithmetic.** The buckets sum to the classified total.
//! - **C3, pinned by upstream.** `name does NOT resolve, every argument types`
//!   must be **0**. Its value comes from `getUnresolvedSymbolForEntityName`
//!   (`checker.go:23102`), not from any arithmetic over this partition: upstream
//!   renders the written name for an unresolvable entity name and this port
//!   ported that arm, so such a reference does not answer `errorType` at all. A
//!   non-zero reading says the ported arm is not reached at this position — a
//!   finding about the port, and it would invalidate the mechanism map rather
//!   than merely move a count.
//! - **C4, pinned by upstream.** The same for the dotted spelling:
//!   `qualified, root does NOT resolve, every argument types` must be **0**,
//!   because `resolveQualifiedName` (`checker.go:15828`) resolves the left as a
//!   namespace and returns nil when that fails, so upstream falls through to the
//!   same unresolved-symbol path and prints the dotted text.
//! - **C5, pinned by the corpus source.** Every dotted root in the top-1 case
//!   `compiler/resolvingClassDeclarationWhenInBaseTypeResolution` is a namespace
//!   **declared in the same file** — the case declares 29 distinct namespaces
//!   and uses 28 distinct dotted roots, and set subtraction leaves none
//!   undeclared. So that case must contribute **0** lines to any unresolved
//!   bucket, and its lines must land in the qualified-namespace refusal.
//! - **C6, construction.** `name resolves and the reference computes a type`
//!   must be **0** in both the generic and the non-generic arm: the line gaps by
//!   C1, so no branch that returns a non-`errorType` can hold one.

use std::collections::{BTreeMap, HashSet};

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{EntityName, Node, NodeId, NodeMap, NodeTable, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// `depend.rs`'s cap, copied with it.
const MAX_DEPTH: usize = 16;

/// The case `STATUS.md` §4.3 names as 51.1% of the row.
const TOP_CASE: &str = "compiler/resolvingClassDeclarationWhenInBaseTypeResolution";

/// The ending that defines the row.
const ROW_ENDING: &str = "no further dependency";

/// Copied from `depend.rs`. Does this node's own answer gap?
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

/// Copied from `depend.rs`. One step toward what this node's answer depends on.
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
                EntityName::Identifier(identifier) => identifier.text,
                EntityName::QualifiedName(qualified) => qualified.right?.text,
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

/// Copied from `depend.rs`.
fn has_step_arm(node: Node<'_>, is_declaration_name: bool) -> bool {
    is_declaration_name
        || matches!(
            node,
            Node::PropertyAccessExpression(_)
                | Node::ElementAccessExpression(_)
                | Node::CallExpression(_)
                | Node::NewExpression(_)
                | Node::ParenthesizedExpression(_)
                | Node::AsExpression(_)
                | Node::NonNullExpression(_)
                | Node::TypeReferenceNode(_)
                | Node::Identifier(_)
                | Node::BinaryExpression(_)
        )
}

/// The type parameters a symbol's own declaration writes.
///
/// Replicates `Checker::local_type_parameters_of` (`declared.rs`), which is
/// private and whose count is the discriminator between the non-generic and the
/// instantiating branch of `get_type_from_type_reference`. Replicated rather
/// than exported: this is a measurement and it must not change checker code.
fn local_type_parameter_count<'a>(
    binder: &tsr_binder::BindResult<'a>,
    map: &NodeMap<'a>,
    symbol: tsr_binder::SymbolId,
) -> usize {
    let Some(declaration) = binder.symbols().get(symbol).declarations.first().copied() else {
        return 0;
    };
    match map.get(declaration) {
        Some(Node::ClassDeclaration(node)) => node.type_parameters.len(),
        Some(Node::ClassExpression(node)) => node.type_parameters.len(),
        Some(Node::InterfaceDeclaration(node)) => node.type_parameters.len(),
        Some(Node::TypeAliasDeclaration(node)) => node.type_parameters.len(),
        _ => 0,
    }
}

/// The mechanism that decided this reference's answer, in the checker's own
/// branch order. Returns the bucket label and whether a type argument also gaps.
fn classify<'a>(
    checker: &mut tsr_checker::Checker<'a, '_>,
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    reference: &tsr_ast::TypeReferenceNode<'a>,
) -> (&'static str, bool) {
    let error = checker.intrinsics().error;
    let argument_gaps = reference
        .type_arguments
        .iter()
        .any(|argument| checker.get_type_from_type_node(*argument) == error);

    let name = match reference.type_name {
        None => return ("no type name — a parse shape, not a resolution failure", argument_gaps),
        Some(EntityName::QualifiedName(_)) => {
            // The leftmost identifier, which is what `resolveQualifiedName`
            // (`checker.go:15828`) resolves as a namespace before anything else.
            let mut root = reference.type_name.expect("matched Some");
            while let EntityName::QualifiedName(inner) = root {
                let Some(left) = inner.left else {
                    return ("malformed qualified name — no left", argument_gaps);
                };
                root = left;
            }
            let EntityName::Identifier(root) = root else {
                return ("malformed qualified name — root is not an identifier", argument_gaps);
            };
            let Some(root_id) = root.node_id else {
                return ("malformed qualified name — root has no node id", argument_gaps);
            };
            let resolves = binder
                .resolve_name(nodes, map, root_id, root.text, SymbolFlags::NAMESPACE)
                .is_some();
            return if resolves {
                (
                    "qualified name, leftmost RESOLVES as a namespace — refused (resolveEntityName)",
                    argument_gaps,
                )
            } else if argument_gaps {
                ("DOWNSTREAM — qualified, root unresolved, a type ARGUMENT gaps", true)
            } else {
                ("C4 VIOLATION — qualified, root unresolved, every argument types", false)
            };
        }
        Some(EntityName::Identifier(name)) => name,
    };

    let Some(id) = name.node_id else {
        return ("the name has no node id — a parse shape", argument_gaps);
    };
    let Some(symbol) = binder.resolve_name(nodes, map, id, name.text, SymbolFlags::TYPE) else {
        return if argument_gaps {
            ("DOWNSTREAM — name unresolved, a type ARGUMENT gaps (the name itself prints)", true)
        } else {
            ("C3 VIOLATION — name does NOT resolve, every argument types", false)
        };
    };

    let parameters = local_type_parameter_count(binder, map, symbol);
    if parameters == 0 {
        if !reference.type_arguments.is_empty() {
            return (
                "arguments written on a non-generic type (checkNoTypeArguments)",
                argument_gaps,
            );
        }
        return if checker.get_declared_type_of_symbol(symbol) == error {
            ("DOWNSTREAM — name resolves, non-generic, the DECLARED TYPE gaps", false)
        } else {
            ("C6 VIOLATION — name resolves, non-generic, declared type is not error", false)
        };
    }
    if reference.type_arguments.len() < parameters {
        return (
            "generic, FEWER arguments than parameters (fillMissingTypeArguments)",
            argument_gaps,
        );
    }
    if reference.type_arguments.len() > parameters {
        return ("generic, MORE arguments than parameters (arity)", argument_gaps);
    }
    if argument_gaps {
        return ("DOWNSTREAM — name resolves, generic, arity ok, a type ARGUMENT gaps", true);
    }
    ("C6 VIOLATION — name resolves, generic, arity ok, every argument types", false)
}

/// Buckets whose gap survives every mechanism on this page — reported before the
/// mechanism table so they cannot inflate it.
fn is_downstream(form: &str) -> bool {
    form.starts_with("DOWNSTREAM")
}

#[derive(Default)]
struct Report {
    gap: usize,
    /// Every `TypeReferenceNode` root, by how its chain ended — so the row can be
    /// read against the rest of the kind rather than alone.
    endings: BTreeMap<&'static str, usize>,
    /// The mechanism split across every ending, not just the row's.
    ending_forms: BTreeMap<(&'static str, &'static str), usize>,
    /// The own mechanism of each **gapping type argument** of a row line.
    argument_forms: BTreeMap<String, usize>,
    classified: usize,
    forms: BTreeMap<&'static str, usize>,
    wants_any: BTreeMap<&'static str, usize>,
    /// Orthogonal to the mechanism: does a written type argument also gap?
    argument_also_gaps: BTreeMap<&'static str, usize>,
    form_cases: BTreeMap<&'static str, BTreeMap<String, usize>>,
    /// The same table restricted to the top-1 case, for C5 and for the
    /// with/without split `STATUS.md` §4.3 asks for.
    top_case_forms: BTreeMap<&'static str, usize>,
    top_case_wants_any: usize,
    cases: BTreeMap<String, usize>,
    c1_root_does_not_gap: usize,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        self.gap += other.gap;
        self.classified += other.classified;
        self.top_case_wants_any += other.top_case_wants_any;
        self.c1_root_does_not_gap += other.c1_root_does_not_gap;
        for (k, n) in &other.endings {
            *self.endings.entry(k).or_default() += n;
        }
        for (k, n) in &other.ending_forms {
            *self.ending_forms.entry(*k).or_default() += n;
        }
        for (k, n) in &other.argument_forms {
            *self.argument_forms.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.forms {
            *self.forms.entry(k).or_default() += n;
        }
        for (k, n) in &other.wants_any {
            *self.wants_any.entry(k).or_default() += n;
        }
        for (k, n) in &other.argument_also_gaps {
            *self.argument_also_gaps.entry(k).or_default() += n;
        }
        for (k, n) in &other.top_case_forms {
            *self.top_case_forms.entry(k).or_default() += n;
        }
        for (k, cases) in &other.form_cases {
            let mine = self.form_cases.entry(k).or_default();
            for (case, n) in cases {
                *mine.entry(case.clone()).or_default() += n;
            }
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
            report.gap += 1;

            // `depend.rs`'s walk, verbatim.
            let mut current = line_ids[position];
            let mut visited: HashSet<NodeId> = HashSet::new();
            visited.insert(current);
            let mut depth = 0usize;
            let ending = loop {
                let Some(next) = step(&mut checker, bound, nodes, map, current) else {
                    break "no further dependency";
                };
                if !gaps(&mut checker, bound, nodes, map, next) {
                    break "the dependency types — the root is here";
                }
                if !visited.insert(next) {
                    break "cycle";
                }
                depth += 1;
                if depth >= MAX_DEPTH {
                    break "depth cap";
                }
                current = next;
            };
            if nodes.kind(current) != SyntaxKind::TypeReference {
                continue;
            }
            let is_declaration_name = nodes
                .parent(current)
                .and_then(|p| map.get(p))
                .is_some_and(|p| p.name_id() == Some(current));
            let ending = if ending == ROW_ENDING
                && !map.get(current).is_some_and(|n| has_step_arm(n, is_declaration_name))
            {
                "NO STEP ARM for this kind — not a finding"
            } else {
                ending
            };
            *report.endings.entry(ending).or_default() += 1;
            let Some(Node::TypeReferenceNode(reference)) = map.get(current) else { continue };
            let (form, argument_gaps) = classify(&mut checker, bound, nodes, map, reference);
            // The same mechanism split across *every* ending, so the row can be
            // read against the rest of the kind rather than in isolation.
            *report.ending_forms.entry((ending, form)).or_default() += 1;
            if ending != ROW_ENDING {
                continue;
            }

            report.classified += 1;
            if !gaps(&mut checker, bound, nodes, map, current) {
                report.c1_root_does_not_gap += 1;
            }
            // A gapping type ARGUMENT has an owner of its own, and the `arg-gaps`
            // column is worthless without it: if the arguments are themselves
            // qualified references then the same mechanism owns both, and if they
            // are not then part of this row belongs elsewhere. Measured, not
            // assumed.
            if argument_gaps {
                let error = checker.intrinsics().error;
                for argument in reference.type_arguments {
                    if checker.get_type_from_type_node(*argument) != error {
                        continue;
                    }
                    let label = match argument {
                        tsr_ast::TypeNode::TypeReferenceNode(inner) => {
                            classify(&mut checker, bound, nodes, map, inner).0.to_owned()
                        }
                        other => other.node_id().map_or_else(
                            || "<no node id>".to_owned(),
                            |id| format!("not a TypeReference: {:?}", nodes.kind(id)),
                        ),
                    };
                    *report.argument_forms.entry(label).or_default() += 1;
                }
            }
            let wants_any = want.text.strip_prefix(&format!("{} : ", got.text)) == Some("any");

            *report.forms.entry(form).or_default() += 1;
            if wants_any {
                *report.wants_any.entry(form).or_default() += 1;
            }
            if argument_gaps {
                *report.argument_also_gaps.entry(form).or_default() += 1;
            }
            *report.form_cases.entry(form).or_default().entry(case.name.clone()).or_default() += 1;
            *report.cases.entry(case.name.clone()).or_default() += 1;
            if case.name == TOP_CASE {
                *report.top_case_forms.entry(form).or_default() += 1;
                if wants_any {
                    report.top_case_wants_any += 1;
                }
            }
        }
    }
    Some(report)
}

#[allow(clippy::cast_precision_loss)]
fn share(part: usize, whole: usize) -> f64 {
    part as f64 / whole.max(1) as f64 * 100.0
}

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

    println!("# typerefgap — the `TypeReference` row, split by why the reference has no type\n");
    println!("gap lines walked: {}", report.gap);

    println!("\n## Every `TypeReferenceNode` root, by how its chain ended\n");
    let mut endings: Vec<_> = report.endings.iter().collect();
    endings.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    let all_roots: usize = report.endings.values().sum();
    for (ending, n) in endings {
        println!("  {n:>6}  {:>5.1}%  {ending}", share(*n, all_roots));
    }
    println!("\n  the row is the `{ROW_ENDING}` line: {}", report.classified);

    // Downstream first, on `docs/conventions.md`'s rule: a bucket that would
    // still gap after every mechanism here was built cannot be allowed to
    // inflate the mechanisms.
    let downstream: usize =
        report.forms.iter().filter(|(f, _)| is_downstream(f)).map(|(_, n)| *n).sum();
    let downstream_any: usize =
        report.wants_any.iter().filter(|(f, _)| is_downstream(f)).map(|(_, n)| *n).sum();
    println!("\n## Downstream first — the gap is not in this reference's own mechanism\n");
    println!(
        "  {downstream:>6}  {:>5.1}% of the row   want-any {downstream_any}",
        share(downstream, report.classified)
    );
    println!(
        "  {:>6}  {:>5.1}% of the row   a mechanism on this page decided the answer",
        report.classified - downstream,
        share(report.classified - downstream, report.classified)
    );

    println!("\n## The row, by mechanism\n");
    println!("  {:>6} {:>8} {:>7}  {:>8}  mechanism", "lines", "want-any", "share", "arg-gaps");
    let mut rows: Vec<_> = report.forms.iter().collect();
    rows.sort_by_key(|(form, n)| (!is_downstream(form), std::cmp::Reverse(**n)));
    let mut sum = 0;
    for (form, n) in &rows {
        let any = report.wants_any.get(*form).copied().unwrap_or(0);
        let args = report.argument_also_gaps.get(*form).copied().unwrap_or(0);
        sum += **n;
        println!("  {n:>6} {any:>8} {:>6.1}%  {args:>8}  {form}", share(any, **n));
    }

    println!("\n## The gapping type ARGUMENTS of row lines, by their own mechanism\n");
    let mut arguments: Vec<_> = report.argument_forms.iter().collect();
    arguments.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    let argument_total: usize = report.argument_forms.values().sum();
    for (label, n) in arguments {
        println!("  {n:>6}  {:>5.1}%  {label}", share(*n, argument_total));
    }

    println!("\n## Every `TypeReferenceNode` root, ending x mechanism\n");
    let mut pairs: Vec<_> = report.ending_forms.iter().collect();
    pairs.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for ((ending, form), n) in pairs {
        println!("  {n:>6}  {ending:<38}  {form}");
    }

    let top_total: usize = report.top_case_forms.values().sum();
    println!("\n## With and without the top-1 case ({TOP_CASE})\n");
    println!(
        "  with    {:>6} lines, want-any {:>5}",
        report.classified,
        report.wants_any.values().sum::<usize>()
    );
    println!(
        "  top-1   {top_total:>6} lines ({:>5.1}% of the row), want-any {:>5}",
        share(top_total, report.classified),
        report.top_case_wants_any
    );
    println!(
        "  without {:>6} lines, want-any {:>5}",
        report.classified - top_total,
        report.wants_any.values().sum::<usize>() - report.top_case_wants_any
    );
    println!("\n  the top-1 case's own mechanism split:");
    let mut top_rows: Vec<_> = report.top_case_forms.iter().collect();
    top_rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (form, n) in top_rows {
        println!("    {n:>6}  {form}");
    }

    println!("\n## Head cases per mechanism — top 6 each\n");
    for (form, n) in &rows {
        println!("  {form}  ({n} lines)");
        let empty = BTreeMap::new();
        let cases = report.form_cases.get(*form).unwrap_or(&empty);
        let mut ranked: Vec<_> = cases.iter().collect();
        ranked.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
        for (case, count) in ranked.into_iter().take(6) {
            println!("      {count:>6}  {case}");
        }
    }

    println!("\n## The row's head cases overall — top 15\n");
    let mut ranked: Vec<_> = report.cases.iter().collect();
    ranked.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (case, n) in ranked.into_iter().take(15) {
        println!("  {n:>6}  {:>5.1}%  {case}", share(*n, report.classified));
    }

    println!("\n## Controls\n");
    println!(
        "  C1 construction: roots that do not gap                     {}  (expect 0)",
        report.c1_root_does_not_gap
    );
    println!("  C2 arithmetic:   buckets sum to {sum}, classified {}", report.classified);
    let c3 = report
        .forms
        .iter()
        .filter(|(f, _)| f.starts_with("C3 VIOLATION"))
        .map(|(_, n)| *n)
        .sum::<usize>();
    let c4 = report
        .forms
        .iter()
        .filter(|(f, _)| f.starts_with("C4 VIOLATION"))
        .map(|(_, n)| *n)
        .sum::<usize>();
    let c6 = report
        .forms
        .iter()
        .filter(|(f, _)| f.starts_with("C6 VIOLATION"))
        .map(|(_, n)| *n)
        .sum::<usize>();
    println!("  C3 upstream:     unresolved name, every argument types      {c3}  (expect 0)");
    println!("  C4 upstream:     qualified, root unresolved, args all type  {c4}  (expect 0)");
    let c5: usize = report
        .top_case_forms
        .iter()
        .filter(|(f, _)| f.contains("unresolved") || f.starts_with("C3") || f.starts_with("C4"))
        .map(|(_, n)| *n)
        .sum();
    println!("  C5 corpus:       top-1 case lines in an unresolved bucket   {c5}  (expect 0)");
    println!("  C6 construction: a gap line whose reference computes a type {c6}  (expect 0)");
}
