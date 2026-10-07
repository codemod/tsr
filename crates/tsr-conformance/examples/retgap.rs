//! Sizing probe for `bd tsr-4sc.9` — the multi-distinct-type return
//! aggregate of `getReturnTypeFromBody` (`checker.go:20259`), the leg
//! `signatures.rs` refuses with "Two or more distinct types".
//!
//! `depend.rs` at `4b81458` reads the `FunctionDeclaration` decl-name row at
//! ~3,312 lines (2.8% want-any) plus ~1,504 propagated, and
//! `checker-notes-callres.md` §8.2 counts 797 blocked callees. What none of
//! them split is *why* each function's return type gaps: the aggregate leg
//! (buildable now that `Relation::Subtype` landed at `e7a65fb`), the
//! strictness leg (bare `return;` beside a valued one), async/generator, or
//! a return expression that itself gaps (downstream, not this item's).
//!
//! For every gap line whose node is an `Identifier` that is (a) the name of
//! a `FunctionDeclaration`, or (b) a reference resolving to a symbol whose
//! value declaration is one, classify that function:
//!
//! Controls: C1 every classified line answers `errorType` today (expect 0);
//! C2 buckets sum to classified.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Node, NodeId, NodeMap, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

#[derive(Default)]
struct Report {
    classified: usize,
    forms: BTreeMap<String, usize>,
    wants_any: BTreeMap<String, usize>,
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
        for (k, n) in &other.cases {
            *self.cases.entry(k.clone()).or_default() += n;
        }
    }
}

/// Return expressions of `body`, skipping nested function-likes — the same
/// walk `signatures.rs::return_expressions_of` makes, reproduced here because
/// that seam is private and a probe must not widen crate surface for a
/// measurement.
fn return_expressions<'a>(map: &NodeMap<'a>, body: NodeId) -> Vec<Option<tsr_ast::Expression<'a>>> {
    let Some(root) = map.get(body) else { return Vec::new() };
    let mut found = Vec::new();
    let mut stack = vec![root];
    let mut children = Vec::new();
    while let Some(node) = stack.pop() {
        match node {
            Node::FunctionDeclaration(_)
            | Node::FunctionExpression(_)
            | Node::ArrowFunction(_)
            | Node::MethodDeclaration(_)
            | Node::GetAccessorDeclaration(_)
            | Node::SetAccessorDeclaration(_)
                if node.node_id() != map.get(body).and_then(|b| b.node_id()) =>
            {
                continue;
            }
            Node::ReturnStatement(statement) => {
                found.push(statement.expression);
                continue;
            }
            _ => {}
        }
        children.clear();
        tsr_ast::push_children(node, &mut children);
        stack.extend(children.iter().copied());
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
        types_producer::assertions_for_case_with_ids(&arena, &parsed, &parsed.files.as_slice());
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
            let Some(Node::Identifier(identifier)) = map.get(id) else { continue };
            // (a) the declaration name of a FunctionDeclaration, or (b) a
            // reference resolving to one.
            let function = if let Some(parent) = nodes.parent(id)
                && matches!(map.get(parent), Some(Node::FunctionDeclaration(_)))
                && map.get(parent).and_then(|p| p.name_id()) == Some(id)
            {
                Some(parent)
            } else if nodes.parent(id).map(|p| map.get(p).and_then(|n| n.name_id()))
                != Some(Some(id))
            {
                bound
                    .resolve_name(nodes, map, id, identifier.text, SymbolFlags::VALUE)
                    .and_then(|symbol| bound.symbols().get(symbol).value_declaration)
                    .filter(|&d| nodes.kind(d) == SyntaxKind::FunctionDeclaration)
            } else {
                None
            };
            let Some(function) = function else { continue };
            let Some(Node::FunctionDeclaration(declaration)) = map.get(function) else { continue };

            report.classified += 1;
            if types_producer::type_id_at_location(&mut checker, bound, nodes, map, id) != error {
                report.c1_not_gap += 1;
            }

            let form = if let Some(annotation) = declaration.r#type {
                // The largest bucket of the first run (1,699) — split by why.
                if checker.get_type_from_type_node(annotation) == error {
                    "ANNOTATED: the return annotation itself GAPS".to_string()
                } else {
                    let mut pattern_parameter = false;
                    let mut parameter_annotation_gaps = false;
                    let mut parameter_unannotated = false;
                    for parameter in declaration.parameters {
                        if !matches!(
                            parameter.name,
                            Some(tsr_ast::BindingName::Identifier(_))
                        ) {
                            pattern_parameter = true;
                        }
                        match parameter.r#type {
                            Some(t) => {
                                if checker.get_type_from_type_node(t) == error {
                                    parameter_annotation_gaps = true;
                                }
                            }
                            None => parameter_unannotated = true,
                        }
                    }
                    if pattern_parameter {
                        "ANNOTATED: a PATTERN parameter".to_string()
                    } else if parameter_annotation_gaps {
                        "ANNOTATED: a parameter annotation gaps".to_string()
                    } else if parameter_unannotated {
                        "ANNOTATED: an unannotated parameter".to_string()
                    } else {
                        "ANNOTATED: everything types — print/other".to_string()
                    }
                }
            } else if declaration.asterisk_token.is_some() {
                "generator".to_string()
            } else if declaration.modifiers.iter().any(|m| {
                matches!(m, tsr_ast::ModifierLike::Token(t) if t.kind == SyntaxKind::AsyncKeyword)
            }) {
                "async".to_string()
            } else if let Some(body) = declaration.body.and_then(|b| b.node_id()) {
                let returns = return_expressions(map, body);
                let mut bare = false;
                let mut gapped_return = false;
                let mut distinct: Vec<tsr_checker::TypeId> = Vec::new();
                for expression in &returns {
                    match expression {
                        None => bare = true,
                        Some(expression) => {
                            let t = checker.check_expression(*expression);
                            if t == error {
                                gapped_return = true;
                            } else if !distinct.contains(&t) {
                                distinct.push(t);
                            }
                        }
                    }
                }
                if gapped_return {
                    "a return expression gaps (downstream)".to_string()
                } else if distinct.len() >= 2 && bare {
                    "multi-distinct + bare return (strictness too)".to_string()
                } else if distinct.len() >= 2 {
                    format!("MULTI-DISTINCT, all type — the target ({} types)", distinct.len().min(4))
                } else if distinct.len() == 1 && bare {
                    "single + bare return (the strictness leg)".to_string()
                } else {
                    // 0 or 1 distinct types: the ported legs. A gap line here
                    // means something else blocks — parameter gaps, contextual
                    // entanglement — worth seeing raw.
                    "0/1 distinct types — ported leg, gap is elsewhere".to_string()
                }
            } else {
                "no body (ambient/overload)".to_string()
            };
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

    println!("# retgap — function-declaration gap lines, by return-aggregate shape\n");
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
    let mut cases: Vec<_> = report.cases.iter().collect();
    cases.sort_by(|a, b| b.1.cmp(a.1));
    println!("\n## Top cases\n");
    for (case, n) in cases.into_iter().take(10) {
        println!("  {n:>6}  {case}");
    }
}
