//! Counterfactual for `getApparentType`'s **instantiable head**
//! (`checker.go:21729`–`:21736`):
//!
//! ```go
//! originalType := t
//! if t.flags&TypeFlagsInstantiable != 0 {
//!     t = c.getBaseConstraintOfType(t)
//!     if t == nil { t = c.unknownType }
//! }
//! ```
//!
//! `crates/tsr-checker/src/members.rs`'s `apparent_type` ports the five
//! primitive arms of the switch below it and **not** this head, so a member
//! read off a type parameter or a `this` type finds nothing. `depend.rs` at
//! HEAD reports those as `the receiver has no such property: T` (132 + 88 on
//! the member-name twin) and `: this` (144 + 81) — ~445 lines.
//!
//! Both shapes are a `Named` type carrying `TypeFlags::TYPE_PARAMETER`
//! (`declared.rs:856` for a type parameter, `expressions.rs:465` for `this`),
//! and they differ only in what the constraint is:
//!
//! - a **type parameter**'s constraint is the `extends` clause on its
//!   declaration;
//! - a **`this` type**'s constraint is the declared type of the class symbol
//!   it carries.
//!
//! This is a counterfactual, not a count: it resolves the constraint, performs
//! the lookup the arm would perform, and compares the printed result to the
//! baseline string for string.
//!
//! Controls: C1 every classified line answers `errorType` today (expect 0).
//! C2 buckets sum to classified.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{MemberName, Node};
use tsr_binder::SymbolFlags;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

#[derive(Default)]
struct Report {
    classified: usize,
    forms: BTreeMap<String, usize>,
    cases: BTreeMap<String, usize>,
    misses: BTreeMap<String, usize>,
    c1_not_gap: usize,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        self.classified += other.classified;
        self.c1_not_gap += other.c1_not_gap;
        for (k, n) in &other.forms {
            *self.forms.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.cases {
            *self.cases.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.misses {
            *self.misses.entry(k.clone()).or_default() += n;
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
            // The access node, or the member-name twin that renders the same
            // lookup on its own line.
            let access = match map.get(id) {
                Some(Node::PropertyAccessExpression(a)) => Some(a),
                Some(Node::Identifier(_)) => match nodes.parent(id).and_then(|p| map.get(p)) {
                    Some(Node::PropertyAccessExpression(a)) => Some(a),
                    _ => None,
                },
                _ => None,
            };
            let Some(access) = access else { continue };
            let (Some(receiver), Some(MemberName::Identifier(name))) =
                (access.expression, access.name)
            else {
                continue;
            };
            let receiver_type = checker.check_expression(receiver);
            if receiver_type == error {
                continue;
            }
            // Identify the two instantiable shapes WITHOUT reaching into the
            // checker's private surface — a probe must not widen crate API for
            // a measurement. `this` is recognised syntactically; a type
            // parameter by resolving its printed name as a TYPE at the access
            // site and checking the symbol's flag.
            let printed = checker.type_to_string(receiver_type);
            let receiver_is_this = matches!(map.get(receiver.node_id().unwrap_or(id)),
                Some(Node::Token(_)) | None) && printed == "this";
            let symbol = if receiver_is_this || printed == "this" {
                // Walk out to the enclosing class.
                let mut current = nodes.parent(id);
                let mut found = None;
                while let Some(node) = current {
                    if matches!(nodes.kind(node), tsr_ast::SyntaxKind::ClassDeclaration
                        | tsr_ast::SyntaxKind::ClassExpression)
                    {
                        found = bound.symbol_of(node);
                        break;
                    }
                    current = nodes.parent(node);
                }
                match found { Some(s) => s, None => continue }
            } else {
                match bound.resolve_name(nodes, map, id, &printed, SymbolFlags::TYPE) {
                    Some(s) if bound.symbols().get(s).flags.intersects(SymbolFlags::TYPE_PARAMETER) => s,
                    _ => continue,
                }
            };
            report.classified += 1;
            if types_producer::type_id_at_location(&mut checker, bound, nodes, map, id) != error {
                report.c1_not_gap += 1;
            }
            let wanted = want.text.rsplit_once(" : ").map_or("", |(_, a)| a).to_string();

            // The constraint, by shape. A `this` type carries the CLASS symbol;
            // a type parameter carries its own.
            let entry_flags = bound.symbols().get(symbol).flags;
            let constraint = if printed == "this" {
                Some(checker.get_declared_type_of_symbol(symbol))
            } else {
                // The `extends` clause on the type-parameter declaration.
                bound
                    .symbols()
                    .get(symbol)
                    .declarations
                    .first()
                    .copied()
                    .and_then(|d| match map.get(d) {
                        Some(Node::TypeParameterDeclaration(p)) => p.constraint,
                        _ => None,
                    })
                    .map(|c| checker.get_type_from_type_node(c))
            };

            let _ = entry_flags;
            let kind = if printed == "this" { "this" } else { "T" };
            let form = match constraint {
                None => format!("{kind}: no constraint — apparent type is `unknown`, stays a gap"),
                Some(c) if c == error => format!("{kind}: the constraint itself GAPS"),
                Some(c) => match checker.get_type_of_property_of_type(c, name.text) {
                    Some(found) if found == error => {
                        format!("{kind}: member found, its own type gaps (downstream)")
                    }
                    Some(found) => {
                        let forecast = checker.type_to_string(found);
                        if forecast == wanted {
                            format!("{kind}: CONVERTS — forecast matches exactly")
                        } else if wanted == "any" {
                            format!("{kind}: want `any` (ceiling)")
                        } else {
                            *report
                                .misses
                                .entry(format!("{kind}: want `{wanted}`, forecast `{forecast}`"))
                                .or_default() += 1;
                            format!("{kind}: MISS — forecast differs")
                        }
                    }
                    None => format!("{kind}: the constraint has no such member"),
                },
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
    let mut report = Report::default();
    for partial in cases.par_iter().filter_map(measure).collect::<Vec<_>>() {
        report.merge(&partial);
    }

    println!("# apparent — getApparentType's instantiable head, forecast\n");
    println!("classified: {}\n", report.classified);
    let mut rows: Vec<_> = report.forms.iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    let mut sum = 0;
    for (form, n) in rows {
        sum += n;
        println!("  {n:>6}  {form}");
    }
    println!("\n  C1 classified-but-not-gap: {}  (expect 0)", report.c1_not_gap);
    println!("  C2 buckets sum {sum} vs classified {}", report.classified);

    println!("\n## Misses, verbatim\n");
    let mut misses: Vec<_> = report.misses.iter().collect();
    misses.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (miss, n) in misses.into_iter().take(12) {
        println!("  {n:>5}  {miss}");
    }

    let mut cases: Vec<_> = report.cases.iter().collect();
    cases.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    println!("\n## Top cases\n");
    for (case, n) in cases.into_iter().take(8) {
        println!("  {n:>6}  {case}");
    }
}
