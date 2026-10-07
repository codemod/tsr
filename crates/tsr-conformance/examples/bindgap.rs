//! Sizing probe for destructuring / binding patterns — STATUS.md §4.2's
//! 649-score row (`depend.rs`: "decl name, neither: `BindingElement` cycle",
//! 3,728 lines at `acdeed5`, 23.8% want-any).
//!
//! For every gap line whose node is an `Identifier` that is the *name* of a
//! `BindingElement`, classify by what `getBindingElementTypeFromParentType`
//! (`checker.go:17707`) would need:
//!
//! - pattern kind — `ObjectBindingPattern` (property lookup) vs
//!   `ArrayBindingPattern` (numeric indexed access / iterated type);
//! - element modifiers — rest (`getRestType`, Omit machinery), default
//!   initializer (`UnionReductionSubtype` on the annotation-less path,
//!   `checker.go:17789` — §5 refused that reduction for `||`/`??`), renamed
//!   (`{ p: x }`), computed property name;
//! - nesting — the pattern hangs off a `VariableDeclaration`, a `Parameter`,
//!   or another `BindingElement` (recursive parent type);
//! - the outer holder's type source, and whether that source **types today**:
//!   an annotation that resolves, an initializer that types, a parameter
//!   needing a contextual type (unported for patterns), or nothing.
//!
//! Controls: C1 every classified line's node answers `errorType` today
//! (expect 0 violations); C2 buckets sum to the classified total.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Node, NodeId, SyntaxKind};
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
            let Some(Node::Identifier(_)) = map.get(id) else { continue };
            // The name of a BindingElement, matching depend.rs's row.
            let Some(element_id) = nodes.parent(id) else { continue };
            let Some(Node::BindingElement(element)) = map.get(element_id) else { continue };
            if map.get(element_id).and_then(|p| p.name_id()) != Some(id) {
                continue;
            }
            report.classified += 1;
            if types_producer::type_id_at_location(&mut checker, bound, nodes, map, id) != error {
                report.c1_not_gap += 1;
            }

            // The pattern this element sits in.
            let pattern_id = nodes.parent(element_id);
            let pattern = pattern_id.map_or("NO PATTERN PARENT", |p| match nodes.kind(p) {
                SyntaxKind::ObjectBindingPattern => "object",
                SyntaxKind::ArrayBindingPattern => "array",
                _ => "OTHER PARENT KIND",
            });

            // Element modifiers, each of which upstream handles with separate
            // machinery.
            let mut modifiers = String::new();
            if element.dot_dot_dot_token.is_some() {
                modifiers.push_str(" rest");
            }
            if element.initializer.is_some() {
                modifiers.push_str(" default");
            }
            match element.property_name {
                Some(tsr_ast::PropertyName::ComputedPropertyName(_)) => {
                    modifiers.push_str(" computed-name");
                }
                Some(_) => modifiers.push_str(" renamed"),
                None => {}
            }
            if modifiers.is_empty() {
                modifiers.push_str(" plain");
            }

            // Walk up to the holder: pattern -> (BindingElement -> pattern)* ->
            // VariableDeclaration | Parameter.
            let mut nested = false;
            let mut holder: Option<NodeId> = None;
            let mut current = pattern_id;
            while let Some(node) = current {
                match nodes.kind(node) {
                    SyntaxKind::ObjectBindingPattern | SyntaxKind::ArrayBindingPattern => {
                        current = nodes.parent(node);
                    }
                    SyntaxKind::BindingElement => {
                        nested = true;
                        current = nodes.parent(node);
                    }
                    _ => {
                        holder = Some(node);
                        break;
                    }
                }
            }

            // The holder's type source, and whether it types today.
            let source = match holder.map(|h| (h, map.get(h))) {
                Some((h, Some(Node::VariableDeclaration(declaration)))) => {
                    let _ = h;
                    match (declaration.r#type, declaration.initializer) {
                        (Some(annotation), _) => {
                            if checker.get_type_from_type_node(annotation) == error {
                                "var: annotation GAPS"
                            } else {
                                "var: annotation types"
                            }
                        }
                        (None, Some(initializer)) => {
                            if checker.check_expression(initializer) == error {
                                "var: initializer GAPS"
                            } else {
                                "var: initializer types"
                            }
                        }
                        (None, None) => "var: no source (implicit any)",
                    }
                }
                Some((_, Some(Node::ParameterDeclaration(parameter)))) => match parameter.r#type {
                    Some(annotation) => {
                        if checker.get_type_from_type_node(annotation) == error {
                            "param: annotation GAPS"
                        } else {
                            "param: annotation types"
                        }
                    }
                    None => "param: contextual (unported for patterns)",
                },
                Some((_, _)) => "OTHER HOLDER KIND",
                None => "NO HOLDER",
            };

            let nesting = if nested { "nested" } else { "top" };
            let form = format!("{pattern} {nesting} | {source} |{modifiers}");
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

    println!("# bindgap — binding-element gap lines, by what the arm would need\n");
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
    for (case, n) in cases.into_iter().take(12) {
        println!("  {n:>6}  {case}");
    }
}
