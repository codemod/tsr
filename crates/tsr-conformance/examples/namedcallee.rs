//! Counterfactual for `bd tsr-4sa` — a `Named` callee never reaches signature
//! lookup (`docs/architecture/checker-notes-namedcallee.md`).
//!
//! `callgate.rs` sized two gates at 2,791 assertion lines — 1,663 on the `new`
//! side (`new: callee is not an object type`) and 1,128 on the call side
//! (`call: identifier types as a non-object`). A population is a ceiling, and
//! `docs/conventions.md` requires the **match** test rather than the shape
//! test, so this probe does not count the row: it **computes the string the arm
//! would print and compares it to the baseline, character for character**.
//!
//! # Why the at-risk column is computed in the same pass
//!
//! `docs/conventions.md`: *"when a mechanism fires on a position rather than on
//! a defect, compute the at-risk population in the same pass that computes the
//! target one"*. This mechanism fires on a **position** — every call whose
//! callee is a `Named` type — and `checker-notes-callres.md` §5 already
//! measured that it can manufacture wrong answers: 264 `Symbol()` lines want
//! `unique symbol` and this port would print `symbol`, and much of the row
//! wants a generic instantiation (`Set<number>`) this port has no members for.
//!
//! So the forecast is bucketed three ways and the three are reported together:
//!
//! - **CONVERTS** — the forecast equals the baseline's right-hand side exactly;
//! - **WOULD PRINT WRONG** — the forecast is a type, and it is not the
//!   baseline's. This is a gap turned into a wrong line, and it is the column
//!   that decides;
//! - **STAYS A GAP** — the arm declines, with the reason it declines for.
//!
//! # What the forecast models
//!
//! The arm being sized reads the callee type's `Named { members }` symbol,
//! walks its interface declarations' members for `CallSignature` /
//! `ConstructSignature` elements, and answers the signature's **written return
//! annotation** resolved through `get_type_from_type_node` — which is the
//! public entry point the real arm would reach through
//! `get_signature_from_declaration`. Reading the annotation directly keeps the
//! counterfactual independent of the plumbing it is sizing.
//!
//! Declining rules modelled, each one a refusal rather than a guess:
//!
//! - **any candidate is generic** (`new <T>(...): Set<T>`) — that is inference,
//!   `callgate.rs`'s own largest gate, and the family §5 names;
//! - **the candidates disagree about the return type** — that is overload
//!   selection by assignability, which this port only has over primitives;
//! - **the return annotation itself gaps**;
//! - **no signature member of the wanted kind**.
//!
//! An overload set whose candidates **agree** about the return type needs no
//! selection to answer — `DateConstructor`'s four construct signatures all
//! return `Date` — so it is forecast rather than declined. That is measured as
//! its own bucket so the decision is visible.
//!
//! # Controls
//!
//! - **C1** every classified line answers `errorType` today. Expected **0**
//!   violations. It also establishes the at-risk claim: a line that is already
//!   right cannot be broken by an arm that only fires where the answer is
//!   `errorType`, because every consumer of an `errorType` gaps too.
//! - **C2** the buckets sum to the classified count.
//! - **C3** the callee's type is `Named` and its `members` symbol has **no
//!   interface declaration**. Pinned by the construct being claimed rather than
//!   by arithmetic: `TypeData::Named`'s `members` is documented as *"the symbol
//!   whose `members` table this type's properties live in"*, so a class instance
//!   type lands here and an interface does not. It is printed as its own bucket
//!   instead of being folded into "no signature member", which would otherwise
//!   absorb a wrong reading of the data model silently.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Node, NodeId, NodeMap, NodeTable, TypeElement};
use tsr_binder::SymbolFlags;
use tsr_checker::types::TypeData;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// The blocking call node for a gap line, by the two routes
/// `checker-notes-callres.md` §3 describes. Verbatim from `callgate.rs`, which
/// is the instrument this probe is re-sizing — a different reachability rule
/// would make the two populations incomparable.
fn blocking_call(
    binder: &tsr_binder::BindResult<'_>,
    nodes: &NodeTable,
    map: &NodeMap<'_>,
    id: NodeId,
) -> Option<NodeId> {
    if matches!(map.get(id), Some(Node::CallExpression(_) | Node::NewExpression(_))) {
        return Some(id);
    }
    let Some(Node::Identifier(identifier)) = map.get(id) else { return None };
    let is_declaration_name =
        nodes.parent(id).and_then(|p| map.get(p)).and_then(|p| p.name_id()) == Some(id);
    let symbol = if is_declaration_name {
        binder.symbol_of(nodes.parent(id)?)?
    } else {
        binder.resolve_name(nodes, map, id, identifier.text, SymbolFlags::VALUE)?
    };
    let declaration = binder.symbols().get(symbol).value_declaration?;
    let initializer = match map.get(declaration)? {
        Node::VariableDeclaration(node) => node.initializer,
        Node::ParameterDeclaration(node) => node.initializer,
        Node::PropertyDeclaration(node) => node.initializer,
        Node::PropertyAssignment(node) => node.initializer,
        _ => None,
    }?;
    let initializer = Node::from(initializer).node_id()?;
    matches!(map.get(initializer), Some(Node::CallExpression(_) | Node::NewExpression(_)))
        .then_some(initializer)
}

#[derive(Default)]
struct Report {
    classified: usize,
    forms: BTreeMap<String, usize>,
    cases: BTreeMap<String, usize>,
    wrong: BTreeMap<String, usize>,
    converted_by_name: BTreeMap<String, usize>,
    converted_cases: BTreeMap<String, usize>,
    c1_not_gap: usize,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        self.classified += other.classified;
        self.c1_not_gap += other.c1_not_gap;
        for (target, source) in [
            (&mut self.forms, &other.forms),
            (&mut self.cases, &other.cases),
            (&mut self.wrong, &other.wrong),
            (&mut self.converted_by_name, &other.converted_by_name),
            (&mut self.converted_cases, &other.converted_cases),
        ] {
            for (key, n) in source {
                *target.entry(key.clone()).or_default() += n;
            }
        }
    }
}

/// Upstream's `isSymbolOrSymbolForCall` (`checker.go:8381`), syntax only — the
/// global-symbol identity check needs a global table this probe does not build,
/// and the corpus has no shadowing `Symbol` in the classified set.
fn is_symbol_or_symbol_for_call(map: &NodeMap<'_>, call: NodeId) -> bool {
    let Some(Node::CallExpression(node)) = map.get(call) else { return false };
    let Some(mut left) = node.expression else { return false };
    if let tsr_ast::Expression::PropertyAccessExpression(access) = left {
        if access.name.is_some_and(
            |name| matches!(Node::from(name), Node::Identifier(id) if id.text == "for"),
        ) {
            let Some(inner) = access.expression else { return false };
            left = inner;
        }
    }
    matches!(left, tsr_ast::Expression::Identifier(id) if id.text == "Symbol")
}

/// Upstream's `isValidESSymbolDeclaration` (`utilities.go:961`), all three arms,
/// applied to the call's parent after `WalkUpParenthesizedExpressions`
/// (`checker.go:8351`).
fn is_valid_es_symbol_declaration(nodes: &NodeTable, map: &NodeMap<'_>, call: NodeId) -> bool {
    let mut parent = nodes.parent(call);
    while let Some(node) = parent {
        if matches!(map.get(node), Some(Node::ParenthesizedExpression(_))) {
            parent = nodes.parent(node);
        } else {
            break;
        }
    }
    let Some(parent) = parent else { return false };
    let readonly = |modifiers: &[tsr_ast::ModifierLike<'_>]| {
        modifiers.iter().any(|modifier| {
            matches!(modifier, tsr_ast::ModifierLike::Token(token)
                if token.kind == tsr_ast::SyntaxKind::ReadonlyKeyword)
        })
    };
    match map.get(parent) {
        Some(Node::VariableDeclaration(declaration)) => {
            matches!(declaration.name, Some(tsr_ast::BindingName::Identifier(_)))
                // `isVarConst` — the `const` lives on the declaration *list*,
                // which is where `combined_node_flags` reads it from too.
                && nodes
                    .parent(parent)
                    .is_some_and(|list| nodes.flags(list).intersects(tsr_ast::NodeFlags::CONST))
        }
        Some(Node::PropertyDeclaration(property)) => {
            readonly(property.modifiers)
                && property.modifiers.iter().any(|modifier| {
                    matches!(modifier, tsr_ast::ModifierLike::Token(token)
                        if token.kind == tsr_ast::SyntaxKind::StaticKeyword)
                })
        }
        Some(Node::PropertySignatureDeclaration(property)) => readonly(property.modifiers),
        _ => false,
    }
}

/// Whether a type's name would need a namespace qualifier — `Intl.NumberFormat`
/// where this port prints `NumberFormat`. `STATUS.md` §5 refuses that family
/// (`bd tsr-93f`, 2.7 wrong per right), so the arm must decline rather than
/// print the bare name.
fn is_namespace_qualified(
    binder: &tsr_binder::BindResult<'_>,
    nodes: &NodeTable,
    map: &NodeMap<'_>,
    symbol: Option<tsr_binder::SymbolId>,
) -> bool {
    let Some(symbol) = symbol else { return false };
    binder.symbols().get(symbol).declarations.iter().any(|&declaration| {
        let mut current = nodes.parent(declaration);
        while let Some(node) = current {
            if matches!(map.get(node), Some(Node::ModuleDeclaration(_))) {
                return true;
            }
            current = nodes.parent(node);
        }
        false
    })
}

/// One signature element of the kind a `new` or a call wants.
struct Candidate<'a> {
    generic: bool,
    r#type: Option<tsr_ast::TypeNode<'a>>,
}

fn candidates_of<'a>(element: TypeElement<'a>, construct: bool) -> Option<Candidate<'a>> {
    match (element, construct) {
        (TypeElement::ConstructSignatureDeclaration(node), true) => {
            Some(Candidate { generic: !node.type_parameters.is_empty(), r#type: node.r#type })
        }
        (TypeElement::CallSignatureDeclaration(node), false) => {
            Some(Candidate { generic: !node.type_parameters.is_empty(), r#type: node.r#type })
        }
        _ => None,
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
            let Some(call) = blocking_call(bound, nodes, map, id) else { continue };
            let (callee, construct) = match map.get(call) {
                Some(Node::CallExpression(node)) => (node.expression, false),
                Some(Node::NewExpression(node)) => (node.expression, true),
                _ => (None, false),
            };
            let Some(callee) = callee else { continue };
            let callee_type = checker.check_expression(callee);
            if callee_type == error {
                continue;
            }
            // The admitted population: the callee's type is a **name**. That is
            // exactly the two gates `callgate.rs` attributes to `bd tsr-4sa` —
            // `calls.rs` and `expressions.rs` both destructure
            // `TypeData::Anonymous` and turn everything else back.
            let TypeData::Named { members, text: callee_text } = &checker.type_of(callee_type).data
            else {
                continue;
            };
            let members = *members;
            let callee_text = callee_text.clone();

            report.classified += 1;
            if types_producer::type_id_at_location(&mut checker, bound, nodes, map, id) != error {
                report.c1_not_gap += 1;
            }
            let wanted = want.text.rsplit_once(" : ").map_or("", |(_, a)| a).to_string();
            let side = if construct { "new" } else { "call" };

            let form = 'form: {
                let Some(members) = members else {
                    break 'form format!(
                        "STAYS A GAP — {side}: the name carries no members symbol"
                    );
                };
                let declarations: Vec<NodeId> =
                    bound.symbols().get(members).declarations.iter().copied().collect();
                let mut elements = Vec::new();
                let mut saw_interface = false;
                let mut heritage = false;
                for declaration in declarations {
                    if let Some(Node::InterfaceDeclaration(interface)) = map.get(declaration) {
                        saw_interface = true;
                        heritage |= !interface.heritage_clauses.is_empty();
                        elements.extend(interface.members.iter().copied());
                    }
                }
                if !saw_interface {
                    break 'form format!(
                        "C3 — {side}: the members symbol has no interface declaration"
                    );
                }
                let found: Vec<Candidate<'_>> =
                    elements.into_iter().filter_map(|e| candidates_of(e, construct)).collect();
                if found.is_empty() {
                    break 'form format!(
                        "STAYS A GAP — {side}: the interface declares no {side} signature"
                    );
                }
                if found.iter().any(|candidate| candidate.generic) {
                    break 'form format!(
                        "STAYS A GAP — {side}: a generic candidate — inference, refused"
                    );
                }
                let overloaded = found.len() > 1;
                let mut printed: Vec<String> = Vec::new();
                let mut gapped = false;
                for candidate in &found {
                    let Some(annotation) = candidate.r#type else {
                        gapped = true;
                        break;
                    };
                    let resolved = checker.get_type_from_type_node(annotation);
                    if resolved == error {
                        gapped = true;
                        break;
                    }
                    printed.push(checker.type_to_string(resolved));
                }
                if gapped {
                    break 'form format!("STAYS A GAP — {side}: the return annotation itself gaps");
                }
                if printed.iter().any(|p| *p != printed[0]) {
                    break 'form format!(
                        "STAYS A GAP — {side}: an overload set whose candidates disagree"
                    );
                }
                let forecast = printed[0].clone();
                let outcome = if forecast == wanted {
                    "would have CONVERTED"
                } else {
                    "prevents a WRONG line"
                };
                // Refusal A — the `unique symbol` family, `checker-notes-callres.md` §5.
                // Upstream's `resolveCallExpression` (`checker.go:8350`) turns an
                // ES-symbol-like return into a *fresh unique symbol* when the call
                // is `Symbol()`/`Symbol.for()` in a const-variable position. This
                // port has no `UniqueESSymbolType`, so the position gaps rather
                // than printing `symbol`. Outside that position upstream answers
                // `symbol` too (`getESSymbolLikeTypeForNode`'s fallthrough), which
                // is why the refusal is positional and not on the return type.
                let return_flags =
                    checker.get_type_from_type_node(found[0].r#type.expect("checked above"));
                if checker
                    .type_of(return_flags)
                    .flags
                    .intersects(tsr_checker::TypeFlags::ES_SYMBOL_LIKE)
                    && is_symbol_or_symbol_for_call(map, call)
                    && is_valid_es_symbol_declaration(nodes, map, call)
                {
                    break 'form format!("REFUSED (unique symbol) — {outcome}");
                }
                // Refusal B — namespace-qualified naming, `STATUS.md` §5.
                let answer_members = match &checker.type_of(return_flags).data {
                    TypeData::Named { members, .. } => *members,
                    _ => None,
                };
                if is_namespace_qualified(bound, nodes, map, answer_members) {
                    break 'form format!("REFUSED (namespace-qualified) — {outcome}");
                }
                // Refusal C — the return annotation resolves to a **type
                // parameter**. On an instantiated callee (`callable2<number>`)
                // that has to be substituted through `bd tsr-4qx`'s seam, which
                // is built for properties and not for signatures, so printing
                // `T` where `number` is wanted is a wrong line. Refusing on the
                // *return type* rather than on the callee being instantiated
                // keeps the eleven instantiated callees whose return mentions
                // no parameter — measured, both ways.
                if checker
                    .type_of(return_flags)
                    .flags
                    .intersects(tsr_checker::TypeFlags::TYPE_PARAMETER)
                {
                    break 'form format!("REFUSED (return is a type parameter) — {outcome}");
                }
                // Refusal D — the interface **extends** something. Upstream's
                // `resolveDeclaredMembers` folds the base types' call and
                // construct signatures in (`getBaseTypes`), and this walk sees
                // only the direct members — so what looks like a single
                // candidate here is one arm of an inherited overload set.
                if heritage {
                    break 'form format!("REFUSED (interface has a heritage clause) — {outcome}");
                }
                if forecast == wanted {
                    *report.converted_by_name.entry(callee_text.clone()).or_default() += 1;
                    *report.converted_cases.entry(case.name.clone()).or_default() += 1;
                    let how = if overloaded { " (agreeing overload set)" } else { "" };
                    break 'form format!("CONVERTS — {side}{how}");
                }
                if wanted == "any" {
                    break 'form format!("want `any` (ADR-0038/0039 forbid it) — {side}");
                }
                *report
                    .wrong
                    .entry(format!(
                        "{side}  want `{wanted}`, forecast `{forecast}`  [callee {callee_text}]  {}",
                        case.name
                    ))
                    .or_default() += 1;
                format!("WOULD PRINT WRONG — {side}")
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

    println!("# namedcallee — the `bd tsr-4sa` counterfactual, forecast against the baseline\n");
    println!("classified (gap line, callee types as a `Named`): {}\n", report.classified);
    let mut rows: Vec<_> = report.forms.iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    let mut sum = 0;
    let (mut converts, mut breaks) = (0usize, 0usize);
    for (form, n) in rows {
        sum += n;
        if form.starts_with("CONVERTS") {
            converts += n;
        }
        if form.starts_with("WOULD PRINT WRONG") {
            breaks += n;
        }
        println!("  {n:>6}  {form}");
    }
    println!("\n  CONVERTS {converts} | WOULD PRINT WRONG {breaks} | rest stays a gap");
    println!("  C1 classified-but-not-gap: {}  (expect 0)", report.c1_not_gap);
    println!("  C2 buckets sum {sum} vs classified {}", report.classified);

    println!("\n## The would-be-wrong lines, verbatim — the at-risk column\n");
    let mut wrong: Vec<_> = report.wrong.iter().collect();
    wrong.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (line, n) in wrong.into_iter().take(25) {
        println!("  {n:>5}  {line}");
    }

    println!("\n## What converts, by callee type\n");
    let mut names: Vec<_> = report.converted_by_name.iter().collect();
    names.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (name, n) in names.into_iter().take(20) {
        println!("  {n:>6}  {name}");
    }

    println!("\n## Top converting cases (the falsifier's input)\n");
    let mut converted: Vec<_> = report.converted_cases.iter().collect();
    converted.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (case, n) in converted.into_iter().take(10) {
        println!("  {n:>6}  {case}");
    }

    println!("\n## Top classified cases\n");
    let mut cases: Vec<_> = report.cases.iter().collect();
    cases.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (case, n) in cases.into_iter().take(10) {
        println!("  {n:>6}  {case}");
    }
}
