//! Counterfactual for the `ConstructorTypeNode` arm — `new (x: T) => U` in type
//! position (`checker-notes-ctortype.md`).
//!
//! `depend.rs` measures the `ConstructorType` root at 1,195 gap lines. A root
//! population is a **ceiling**, and `docs/conventions.md` requires the *match*
//! test rather than the shape test, so this probe does not count the row: it
//! **computes the string the arm would print and compares it to the baseline,
//! character for character**.
//!
//! # What the forecast is
//!
//! The arm being costed is: fold `ConstructorTypeNode` into
//! `signature_parts_of`, add a construct flag to `Signature`, and have
//! `signature_to_string` emit `new ` — or `abstract new ` when the declaration
//! carries the `abstract` modifier. So the forecast here reproduces
//! `signature_to_string` from public checker API over the same AST parts
//! `signature_parts_of` would hand it: type parameters (name, `extends`
//! constraint, `= default`), parameters (`...`, name, `?: `/`: `, the
//! annotation's type — or its **written** text for a `typeof` annotation, the
//! one node-reuse rule `Parameter::written_text` carries), then `) => ` and the
//! return annotation.
//!
//! Every form the real `get_signature_from_declaration` refuses is refused here
//! too, and reported as its own bucket rather than as a conversion: a binding
//! pattern parameter, a type parameter carrying a modifier, a parameter or
//! return annotation whose own type gaps.
//!
//! # Which lines the forecast is allowed to claim
//!
//! A root population includes lines reached through steps that do **not**
//! preserve the printed type — `type C = new () => X; var v: C` prints `C`, not
//! the body. So the walk records its step kinds and only lines reached by
//! **type-preserving** steps (a declaration name to its annotation, an
//! identifier reference to its declaration's name) are forecast. Everything
//! else is reported as downstream and claimed as nothing.
//!
//! # Controls
//!
//! - **C1** every classified line answers `errorType` today (expect 0
//!   violations). Arithmetic-free: it re-asks the checker.
//! - **C2** buckets sum to classified.
//! - **C3, pinned to the port's own refusal rather than to my summary of it:
//!   `get_type_from_type_node` on every classified root must answer `error`
//!   today** (expect 0 violations). If a constructor type node already types,
//!   the refusal this item is built on does not exist and the population is
//!   not what it says.

use std::collections::{BTreeMap, HashSet};

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{ConstructorTypeNode, Node, NodeId, NodeMap, NodeTable, SyntaxKind, TypeNode};
use tsr_binder::SymbolFlags;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// Mirrors `depend.rs`'s cap, for the same reason: an unanticipated shape is
/// counted rather than hung on.
const MAX_DEPTH: usize = 16;

/// Whether a dependency step keeps the printed type identical.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    /// A declaration's name to its annotation, or a reference to its
    /// declaration's name. `getTypeOfSymbol` returns the annotation's type
    /// unchanged, so the line prints what the annotation prints.
    Preserving,
    /// Anything else — a receiver, a callee, an initialiser, a type reference
    /// (which prints the alias's *name*).
    Lossy,
}

#[derive(Default)]
struct Report {
    classified: usize,
    forms: BTreeMap<String, usize>,
    cases: BTreeMap<String, usize>,
    misses: BTreeMap<String, usize>,
    spellings: BTreeMap<&'static str, usize>,
    c1_not_gap: usize,
    c3_root_types: usize,
    cycles: usize,
    too_deep: usize,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        self.classified += other.classified;
        self.c1_not_gap += other.c1_not_gap;
        self.c3_root_types += other.c3_root_types;
        self.cycles += other.cycles;
        self.too_deep += other.too_deep;
        for (k, n) in &other.forms {
            *self.forms.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.cases {
            *self.cases.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.misses {
            *self.misses.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.spellings {
            *self.spellings.entry(k).or_default() += n;
        }
    }
}

/// Does this node's own answer gap? Copied from `depend.rs` so the walk lands
/// on the same roots that sized the row.
fn gaps<'a>(
    checker: &mut tsr_checker::Checker<'a, '_>,
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    id: NodeId,
) -> bool {
    let error = checker.intrinsics().error;
    if let Some(node) = map.get(id)
        && let Ok(type_node) = TypeNode::try_from(node)
    {
        return checker.get_type_from_type_node(type_node) == error;
    }
    types_producer::type_id_at_location(checker, binder, nodes, map, id) == error
}

/// One step toward what this node's answer depends on, with whether the step
/// preserves the printed type. The arms are `depend.rs`'s, unchanged; only the
/// classification is new.
fn step<'a>(
    checker: &mut tsr_checker::Checker<'a, '_>,
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    id: NodeId,
) -> Option<(NodeId, Step)> {
    let node = map.get(id)?;

    if let Some(parent) = nodes.parent(id)
        && let Some(parent_node) = map.get(parent)
        && parent_node.name_id() == Some(id)
        && !matches!(parent_node, Node::PropertyAccessExpression(_) | Node::QualifiedName(_))
    {
        if let Some(annotation) = parent_node.type_id() {
            return Some((annotation, Step::Preserving));
        }
        if let Some(initializer) = parent_node.initializer_id() {
            return Some((initializer, Step::Lossy));
        }
    }

    if let Some(parent) = nodes.parent(id)
        && let Some(parent_node) = map.get(parent)
        && parent_node.name_id() == Some(id)
        && matches!(parent_node, Node::PropertyAccessExpression(_) | Node::QualifiedName(_))
    {
        return parent_node
            .expression_id()
            .or_else(|| match parent_node {
                Node::QualifiedName(qualified) => qualified.left.and_then(|l| l.node_id()),
                _ => None,
            })
            .map(|next| (next, Step::Lossy));
    }

    match node {
        Node::PropertyAccessExpression(_)
        | Node::ElementAccessExpression(_)
        | Node::CallExpression(_)
        | Node::NewExpression(_)
        | Node::AsExpression(_)
        | Node::NonNullExpression(_) => node.expression_id().map(|next| (next, Step::Lossy)),
        // `(e)` prints what `e` prints.
        Node::ParenthesizedExpression(_) => {
            node.expression_id().map(|next| (next, Step::Preserving))
        }
        Node::TypeReferenceNode(reference) => {
            let name = reference.type_name?;
            let text = match name {
                tsr_ast::EntityName::Identifier(identifier) => identifier.text,
                tsr_ast::EntityName::QualifiedName(qualified) => qualified.right?.text,
            };
            let symbol = binder.resolve_name(nodes, map, id, text, SymbolFlags::TYPE)?;
            let declaration = binder.symbols().get(symbol).declarations.first().copied()?;
            // Lossy: an aliased constructor type prints the alias's name.
            map.get(declaration)?.name_id().map(|next| (next, Step::Lossy))
        }
        Node::Identifier(identifier) => {
            let symbol =
                binder.resolve_name(nodes, map, id, identifier.text, SymbolFlags::VALUE)?;
            if checker.get_type_of_symbol(symbol) != checker.intrinsics().error {
                return None;
            }
            let declaration = binder.symbols().get(symbol).value_declaration?;
            map.get(declaration)?.name_id().map(|next| (next, Step::Preserving))
        }
        _ => None,
    }
}

/// `typeof a` — the one annotation whose node the builder reuses, mirrored from
/// `Checker::type_query_written_text` (`signatures.rs`).
fn type_query_written_text(annotation: TypeNode<'_>) -> Option<String> {
    let TypeNode::TypeQueryNode(query) = annotation else { return None };
    if !query.type_arguments.is_empty() {
        return None;
    }
    let mut segments = Vec::new();
    let mut current = query.expr_name?;
    loop {
        match current {
            tsr_ast::EntityName::Identifier(identifier) => {
                segments.push(identifier.text);
                break;
            }
            tsr_ast::EntityName::QualifiedName(qualified) => {
                segments.push(qualified.right?.text);
                current = qualified.left?;
            }
        }
    }
    segments.reverse();
    Some(format!("typeof {}", segments.join(".")))
}

/// The string the arm would print for this constructor type node, or the name
/// of the form that would still gap.
fn forecast<'a>(
    checker: &mut tsr_checker::Checker<'a, '_>,
    binder: &tsr_binder::BindResult<'a>,
    node: &'a ConstructorTypeNode<'a>,
) -> Result<String, &'static str> {
    let error = checker.intrinsics().error;
    let is_abstract = node.modifiers.iter().any(|modifier| {
        matches!(modifier, tsr_ast::ModifierLike::Token(t) if t.kind == SyntaxKind::AbstractKeyword)
    });
    let mut out = if is_abstract { "abstract new ".to_string() } else { "new ".to_string() };

    if !node.type_parameters.is_empty() {
        out.push('<');
        for (index, parameter) in node.type_parameters.iter().enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            if !parameter.modifiers.is_empty() {
                return Err("a type parameter carries a modifier (const/in/out)");
            }
            let Some(name) = parameter.name else {
                return Err("a type parameter has no name");
            };
            out.push_str(name.text);
            if let Some(constraint) = parameter.constraint {
                let id = checker.get_type_from_type_node(constraint);
                if id == error {
                    return Err("a type parameter constraint itself gaps");
                }
                out.push_str(" extends ");
                match type_query_written_text(constraint) {
                    Some(written) => out.push_str(&written),
                    None => out.push_str(&checker.type_to_string(id)),
                }
            }
            if let Some(default) = parameter.default_type {
                let id = checker.get_type_from_type_node(default);
                if id == error {
                    return Err("a type parameter default itself gaps");
                }
                out.push_str(" = ");
                out.push_str(&checker.type_to_string(id));
            }
        }
        out.push('>');
    }

    out.push('(');
    for (index, parameter) in node.parameters.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        let Some(tsr_ast::BindingName::Identifier(name)) = parameter.name else {
            return Err("a parameter is a binding pattern");
        };
        if parameter.initializer.is_some() {
            return Err("a parameter has an initialiser");
        }
        if parameter.dot_dot_dot_token.is_some() {
            out.push_str("...");
        }
        out.push_str(name.text);
        out.push_str(if parameter.question_token.is_some() { "?: " } else { ": " });
        if let Some(annotation) = parameter.r#type {
            let id = checker.get_type_from_type_node(annotation);
            if id == error {
                return Err("a parameter annotation itself gaps");
            }
            match type_query_written_text(annotation) {
                Some(written) => out.push_str(&written),
                None => out.push_str(&checker.type_to_string(id)),
            }
        } else {
            // `parameter_of` falls back to the symbol's type — the implicit
            // `any` — for an unannotated parameter.
            let Some(id) = parameter.node_id.and_then(|id| binder.symbol_of(id)) else {
                return Err("an unannotated parameter has no symbol");
            };
            let t = checker.get_type_of_symbol(id);
            if t == error {
                return Err("an unannotated parameter's type gaps");
            }
            out.push_str(&checker.type_to_string(t));
        }
    }
    out.push_str(") => ");

    let Some(annotation) = node.r#type else {
        return Err("no return annotation");
    };
    let id = checker.get_type_from_type_node(annotation);
    if id == error {
        return Err("the return annotation itself gaps");
    }
    match type_query_written_text(annotation) {
        Some(written) => out.push_str(&written),
        None => out.push_str(&checker.type_to_string(id)),
    }
    Ok(out)
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
            let Some(wanted) = want.text.strip_prefix(&format!("{} : ", got.text)) else {
                continue;
            };
            if got.type_string != "error" {
                continue;
            }

            // The walk, `depend.rs`'s, with the step kinds retained.
            let mut current = line_ids[position];
            let mut visited: HashSet<NodeId> = HashSet::new();
            visited.insert(current);
            let mut depth = 0usize;
            let mut preserving = true;
            while let Some((next, kind)) = step(&mut checker, bound, nodes, map, current) {
                if !gaps(&mut checker, bound, nodes, map, next) {
                    break;
                }
                if !visited.insert(next) {
                    report.cycles += 1;
                    break;
                }
                if kind == Step::Lossy {
                    preserving = false;
                }
                depth += 1;
                if depth >= MAX_DEPTH {
                    report.too_deep += 1;
                    break;
                }
                current = next;
            }

            if nodes.kind(current) != SyntaxKind::ConstructorType {
                continue;
            }
            let Some(Node::ConstructorTypeNode(constructor)) = map.get(current) else { continue };

            report.classified += 1;
            if types_producer::type_id_at_location(
                &mut checker,
                bound,
                nodes,
                map,
                line_ids[position],
            ) != error
            {
                report.c1_not_gap += 1;
            }
            if checker.get_type_from_type_node(TypeNode::ConstructorTypeNode(constructor)) != error
            {
                report.c3_root_types += 1;
            }
            *report
                .spellings
                .entry(if constructor.modifiers.is_empty() { "new" } else { "abstract new" })
                .or_default() += 1;

            let form = if preserving {
                match forecast(&mut checker, bound, constructor) {
                    Err(reason) => format!("still a gap: {reason}"),
                    Ok(forecast) if forecast == wanted => {
                        "CONVERTS — forecast matches the baseline exactly".to_string()
                    }
                    Ok(_) if wanted == "any" => "want `any` (ceiling)".to_string(),
                    Ok(forecast) => {
                        *report
                            .misses
                            .entry(format!("want `{wanted}`, forecast `{forecast}`"))
                            .or_default() += 1;
                        "MISS — forecast differs".to_string()
                    }
                }
            } else {
                "downstream — a lossy step lies between the line and the node".to_string()
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
    let report = cases
        .par_iter()
        .filter_map(measure)
        .fold(Report::default, |mut a, r| {
            a.merge(&r);
            a
        })
        .reduce(Report::default, |mut a, r| {
            a.merge(&r);
            a
        });

    println!("# ctorgen — the `new (x: T) => U` counterfactual, forecast against the baseline\n");
    println!(
        "classified (gap line whose dependency root is a ConstructorType node): {}\n",
        report.classified
    );
    let mut rows: Vec<_> = report.forms.iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    let mut sum = 0;
    for (form, n) in rows {
        sum += n;
        println!("  {n:>6}  {form}");
    }
    println!("\n  C1 classified-but-not-gap: {}  (expect 0)", report.c1_not_gap);
    println!("  C2 buckets sum {sum} vs classified {}", report.classified);
    println!("  C3 root already types: {}  (expect 0)", report.c3_root_types);
    println!("  cycles {} | depth-capped {}", report.cycles, report.too_deep);

    println!("\n## Spellings\n");
    for (spelling, n) in &report.spellings {
        println!("  {n:>6}  {spelling}");
    }

    println!("\n## The misses, verbatim — what the forecast got wrong\n");
    let mut misses: Vec<_> = report.misses.iter().collect();
    misses.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (miss, n) in misses.into_iter().take(20) {
        println!("  {n:>5}  {miss}");
    }

    let mut cases: Vec<_> = report.cases.iter().collect();
    cases.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    println!("\n## Top cases\n");
    for (case, n) in cases.into_iter().take(10) {
        println!("  {n:>6}  {case}");
    }
}
