//! Counterfactual for the type-predicate return arm (`checker-notes-typepred.md`).
//!
//! `depend.rs` measures a `TypePredicate` root at 754 gap lines with **want-any
//! 0**. A population is a ceiling (`STATUS.md` §1, fourth rule), and
//! `docs/conventions.md` requires the *match* test rather than the shape test,
//! so this probe does not count the row. It computes the line the arm would
//! print and compares it to the baseline **string for string**.
//!
//! # How the forecast is produced without the arm existing
//!
//! The arm has two halves and only one of them is new:
//!
//! - `getTypeFromTypeNode`'s `ast.KindTypePredicate` case (`checker.go:22858`)
//!   answers `voidType` under an `asserts` modifier and `booleanType`
//!   otherwise — a *type*, so the signature stops gapping.
//! - `typePredicateToTypePredicateNodeHelper` (`nodebuilderimpl.go:1765`)
//!   emits the predicate **in return position** instead of that type, so the
//!   printed line reads `(x: unknown) => x is string` and not
//!   `(x: unknown) => boolean`.
//!
//! Everything else about the printed line — the parameter list, the type
//! parameters, the enclosing `{ … }` of a type literal — is machinery this port
//! already has and this probe must not re-implement, because re-implementing it
//! would forecast the probe's spelling rather than the compiler's.
//!
//! So the forecast is taken from the compiler itself, by **de-predication**:
//! every `TypePredicateNode` span in the case's own units is replaced with a
//! fresh unresolved type name, the case is re-checked, and the port prints the
//! whole line for real — `(x: unknown) => Tsrpredmark0`, because an unresolved
//! type reference prints the name that was written (`d356450`, `bd tsr-eep`).
//! Substituting the forecast predicate text back for the marker gives exactly
//! the string the arm would print, with every other part of the line produced
//! by the code that will produce it after the build.
//!
//! # Controls
//!
//! - **C1** every classified line answers `errorType` today (expect 0).
//! - **C2** buckets sum to classified.
//! - **C3** a line reported as converting must have carried a marker in the
//!   de-predicated run — otherwise the rewrite never reached it and the match
//!   is a coincidence (expect 0).
//! - **C4** the de-predicated run must produce the same number of assertion
//!   lines per file as the original; a case where it does not is skipped and
//!   counted, because positional comparison would be meaningless there.
//! - **C5** pinned to the upstream construct rather than to a summary of it:
//!   `getTypeFromTypeNode` answers `boolean`/`void` for a predicate, never
//!   `errorType`, so **no classified line may want `any`** for the predicate's
//!   own sake. `depend.rs` reports want-any 0 for this root; this recomputes it
//!   here rather than carrying it.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Node, NodeId, NodeMap, NodeTable};
use tsr_binder::SymbolFlags;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// How far a dependency chain may run before it is abandoned. Copied from
/// `depend.rs` so this probe's classification is the same walk that produced
/// the 754 it is sizing.
const MAX_DEPTH: usize = 16;

/// The stem of the name substituted for a predicate in the de-predicated run.
/// Deliberately not a plausible user identifier: a collision would make a
/// marker resolve, and a resolved marker prints its target instead of itself.
const MARKER: &str = "Tsrpredmark";

#[derive(Default)]
struct Report {
    classified: usize,
    forms: BTreeMap<String, usize>,
    roots: BTreeMap<String, usize>,
    misses: BTreeMap<String, usize>,
    cases: BTreeMap<String, usize>,
    c1_not_gap: usize,
    c3_no_marker: usize,
    c4_skipped_cases: usize,
    c5_want_any: usize,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        self.classified += other.classified;
        self.c1_not_gap += other.c1_not_gap;
        self.c3_no_marker += other.c3_no_marker;
        self.c4_skipped_cases += other.c4_skipped_cases;
        self.c5_want_any += other.c5_want_any;
        for (map, mine) in [
            (&other.forms, &mut self.forms),
            (&other.roots, &mut self.roots),
            (&other.misses, &mut self.misses),
            (&other.cases, &mut self.cases),
        ] {
            for (k, n) in map {
                *mine.entry(k.clone()).or_default() += n;
            }
        }
    }
}

/// Does this node's own answer gap? (`depend.rs`)
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

/// One step toward what this node's answer depends on. (`depend.rs`)
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

/// The root of `id`'s gap: the first node in the chain that gaps without a
/// gapping dependency of its own.
fn root_of<'a>(
    checker: &mut tsr_checker::Checker<'a, '_>,
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    start: NodeId,
) -> Option<NodeId> {
    let mut current = start;
    for _ in 0..MAX_DEPTH {
        let Some(next) = step(checker, binder, nodes, map, current) else { return Some(current) };
        if !gaps(checker, binder, nodes, map, next) {
            return Some(current);
        }
        current = next;
    }
    None
}

/// What the arm would print in return position for this predicate node.
///
/// Ported ahead of the code from `typePredicateToTypePredicateNodeHelper`
/// (`nodebuilderimpl.go:1765`) plus the printer's `emitTypePredicate`: the
/// `asserts` modifier, then the parameter name or `this`, then ` is ` and the
/// predicate's type — which upstream renders through `typeToTypeNode`, i.e.
/// the *computed* type and not the written node.
///
/// `None` where the arm itself would gap: a predicate whose own type node this
/// port cannot resolve is a gap, not an `any`.
fn forecast<'a>(
    checker: &mut tsr_checker::Checker<'a, '_>,
    predicate: &'a tsr_ast::TypePredicateNode<'a>,
) -> Option<String> {
    let mut out = String::new();
    if predicate.asserts_modifier.is_some() {
        out.push_str("asserts ");
    }
    match predicate.parameter_name? {
        tsr_ast::TypePredicateParameterName::Identifier(name) => out.push_str(name.text),
        tsr_ast::TypePredicateParameterName::ThisTypeNode(_) => out.push_str("this"),
    }
    if let Some(node) = predicate.r#type {
        let id = checker.get_type_from_type_node(node);
        if id == checker.intrinsics().error {
            return None;
        }
        out.push_str(" is ");
        out.push_str(&checker.type_to_string(id));
    }
    Some(out)
}

/// The predicate's syntactic family, for the refusal rule: a sub-form needing
/// unported machinery refuses the whole construct rather than approximating it.
fn family(predicate: &tsr_ast::TypePredicateNode<'_>) -> &'static str {
    let asserts = predicate.asserts_modifier.is_some();
    let this = matches!(
        predicate.parameter_name,
        Some(tsr_ast::TypePredicateParameterName::ThisTypeNode(_))
    );
    match (asserts, this, predicate.r#type.is_some()) {
        (false, false, _) => "x is T",
        (false, true, _) => "this is T",
        (true, _, false) => "asserts x",
        (true, false, true) => "asserts x is T",
        (true, true, true) => "asserts this is T",
    }
}

/// What the arm answers for a predicate node used as a **type**: `voidType`
/// under an `asserts` modifier, `booleanType` otherwise
/// (`getTypeFromTypeNode`, `checker.go:22858`).
fn predicate_type_text(predicate: &tsr_ast::TypePredicateNode<'_>) -> &'static str {
    if predicate.asserts_modifier.is_some() { "void" } else { "boolean" }
}

/// What a marker stands for, in the de-predicated run.
struct Marker {
    /// The predicate text the arm would print in a signature's return
    /// position. `None` where the predicate's own type node gaps.
    predicate: Option<String>,
    /// The text of the type the arm answers everywhere else.
    r#type: &'static str,
}

/// Substitute the arm's output back into a de-predicated line.
///
/// **The two halves of the arm are two different substitutions, and which one
/// applies is decided by position.** A marker standing in a signature's return
/// slot becomes the predicate; a marker anywhere else — most commonly the type
/// of a *call* to the predicate function — becomes `boolean` or `void`, which
/// is what `getTypeFromTypeNode` answers for the node. The first version of
/// this probe substituted the predicate everywhere and reported 204 misses
/// whose head was `isFunction(x) : boolean` against a forecast of
/// `isFunction(x) : x is Function`: the probe, not the arm, was wrong.
///
/// Return position is `") => "` (the `FunctionTypeNode` spelling,
/// `Checker::signature_to_string`) or `"): "` (the member spelling,
/// `objects::signature_member_text`) — the two renderers this port has, and the
/// same split upstream makes by emitting `ast.KindFunctionType` versus a
/// signature member.
///
/// `None` when a marker the line needs has no forecast.
fn substitute(line: &str, markers: &BTreeMap<String, Marker>) -> Option<String> {
    let mut out = String::new();
    let mut rest = line;
    while let Some(at) = rest.find(MARKER) {
        out.push_str(&rest[..at]);
        let tail = &rest[at + MARKER.len()..];
        let digits = tail.len() - tail.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        let name = format!("{MARKER}{}", &tail[..digits]);
        let marker = markers.get(&name)?;
        let in_return_position = out.ends_with(") => ") || out.ends_with("): ");
        if in_return_position {
            out.push_str(marker.predicate.as_deref()?);
        } else {
            out.push_str(marker.r#type);
        }
        rest = &tail[digits..];
    }
    out.push_str(rest);
    Some(out)
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

    let mut report = Report::default();

    // Pass 1: the real case. Classify the gap lines whose root is a predicate
    // node, and forecast each predicate the case's own units carry.
    //
    // `marker_of` is keyed by the predicate node's id in *this* program; the
    // de-predicated run is a different program with different ids, so the two
    // are joined through the marker text and not through the id.
    let arena = tsr_core::Arena::new();
    let (program, ours, ids) =
        types_producer::assertions_for_case_with_ids(&arena, &parsed, &parsed.files.as_slice());
    let nodes = program.nodes();
    let map = program.node_map();
    let bound = program.binder();
    let mut checker = tsr_checker::Checker::with_module_host(bound, nodes, map, Some(&program));

    // Every predicate node in a unit the case owns, with the span to rewrite.
    // Lib files cannot be rewritten — they are not in `case.files` — so a line
    // blocked by a lib predicate reports as blocked, never as converting.
    let mut rewrites: BTreeMap<String, Vec<(usize, usize, String)>> = BTreeMap::new();
    let mut marker_of: BTreeMap<NodeId, String> = BTreeMap::new();
    let mut markers: BTreeMap<String, Marker> = BTreeMap::new();
    let mut next_marker = 0usize;
    for file in program.root_and_referenced_files() {
        let Some(unit) = parsed.files.iter().find(|u| file.file_name().ends_with(&u.name)) else {
            continue;
        };
        let range = file.node_range();
        for raw in range.clone() {
            let id = NodeId::new(raw);
            let Some(Node::TypePredicateNode(predicate)) = map.get(id) else { continue };
            let span = nodes.span(id);
            let marker = format!("{MARKER}{next_marker}");
            next_marker += 1;
            let text = forecast(&mut checker, predicate);
            rewrites.entry(unit.name.clone()).or_default().push((
                span.start as usize,
                span.end as usize,
                marker.clone(),
            ));
            markers.insert(
                marker.clone(),
                Marker { predicate: text, r#type: predicate_type_text(predicate) },
            );
            marker_of.insert(id, marker);
        }
    }

    // The classified lines, as (file index, position, root predicate id).
    let mut classified: Vec<(usize, usize, NodeId)> = Vec::new();
    for (index, expected_file) in expected.iter().enumerate() {
        let (Some(our_file), Some(line_ids)) = (ours.get(index), ids.get(index)) else { continue };
        if our_file.len() != line_ids.len() {
            continue;
        }
        for (position, want) in expected_file.assertions.iter().enumerate() {
            let Some(got) = our_file.get(position) else { continue };
            if want.text == got.line() || got.type_string != "error" {
                continue;
            }
            if want.text.strip_prefix(&format!("{} : ", got.text)).is_none() {
                continue;
            }
            let id = line_ids[position];
            let Some(root) = root_of(&mut checker, bound, nodes, map, id) else { continue };
            if !matches!(map.get(root), Some(Node::TypePredicateNode(_))) {
                continue;
            }
            report.classified += 1;
            if types_producer::type_id_at_location(&mut checker, bound, nodes, map, id)
                != checker.intrinsics().error
            {
                report.c1_not_gap += 1;
            }
            let wanted = want.text.rsplit_once(" : ").map_or("", |(_, a)| a);
            if wanted == "any" {
                report.c5_want_any += 1;
            }
            let Some(Node::TypePredicateNode(predicate)) = map.get(root) else { continue };
            *report.forms.entry(family(predicate).to_string()).or_default() += 1;
            let parent = nodes
                .parent(root)
                .map_or_else(|| "<root>".to_owned(), |p| format!("{:?}", nodes.kind(p)));
            *report.roots.entry(parent).or_default() += 1;
            classified.push((index, position, root));
        }
    }
    if classified.is_empty() {
        return Some(report);
    }

    // Pass 2: the de-predicated case. Spans are rewritten back-to-front so an
    // earlier replacement cannot move a later one's offsets.
    let mut rewritten = parsed.clone();
    for unit in &mut rewritten.files {
        let Some(spans) = rewrites.get(&unit.name) else { continue };
        let mut spans = spans.clone();
        spans.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
        for (start, end, marker) in spans {
            if start > end || end > unit.content.len() {
                continue;
            }
            unit.content.replace_range(start..end, &marker);
        }
    }
    let arena2 = tsr_core::Arena::new();
    let (_program2, theirs, _ids2) =
        types_producer::assertions_for_case_with_ids(&arena2, &rewritten, &rewritten.files.as_slice());

    // C4: positional comparison is only meaningful if the rewrite changed no
    // line count anywhere.
    let aligned = ours.len() == theirs.len()
        && ours.iter().zip(theirs.iter()).all(|(a, b)| a.len() == b.len());
    if !aligned {
        report.c4_skipped_cases += 1;
        for _ in &classified {
            *report
                .cases
                .entry(format!(
                    "SKIPPED — the de-predicated run does not line up (C4) :: {}",
                    case.name
                ))
                .or_default() += 1;
        }
        return Some(report);
    }

    for (index, position, root) in classified {
        let want = &expected[index].assertions[position];
        let Some(marker) = marker_of.get(&root) else {
            *report
                .cases
                .entry(format!("the root predicate is in a lib file :: {}", case.name))
                .or_default() += 1;
            continue;
        };
        if markers.get(marker).is_none_or(|m| m.predicate.is_none()) {
            *report
                .cases
                .entry(format!("the root predicate's own type gaps :: {}", case.name))
                .or_default() += 1;
            continue;
        }
        let Some(after) = theirs.get(index).and_then(|f| f.get(position)) else { continue };
        let line = after.line();
        if after.type_string != "error" && !line.contains(MARKER) {
            report.c3_no_marker += 1;
        }
        // Every marker in the line becomes its own substitution; a signature
        // can carry more than one predicate (a type literal with several
        // members, an overload set).
        let forecasted = substitute(&line, &markers);
        let bucket = if after.type_string == "error" {
            "still gaps once de-predicated — blocked elsewhere"
        } else if forecasted.is_none() {
            "another predicate on the same line has a gapping type"
        } else if forecasted.as_deref() == Some(want.text.as_str()) {
            "CONVERTS — forecast matches the baseline exactly"
        } else {
            *report
                .misses
                .entry(format!(
                    "want `{}`\n         got  `{}`",
                    want.text,
                    forecasted.unwrap_or_default()
                ))
                .or_default() += 1;
            "MISS — forecast differs"
        };
        *report.cases.entry(format!("{bucket} :: {}", case.name)).or_default() += 1;
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

    println!("# predgap — the type-predicate counterfactual, forecast against the baseline\n");
    println!("classified (gap line whose root is a TypePredicateNode): {}\n", report.classified);

    println!("## The predicate family the root carries\n");
    let mut forms: Vec<_> = report.forms.iter().collect();
    forms.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    let mut sum = 0;
    for (form, n) in forms {
        sum += n;
        println!("  {n:>6}  {form}");
    }
    println!("\n  C2 families sum {sum} vs classified {}", report.classified);

    println!("\n## Where the root predicate sits — its parent kind\n");
    let mut roots: Vec<_> = report.roots.iter().collect();
    roots.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (kind, n) in roots {
        println!("  {n:>6}  {kind}");
    }

    println!("\n## The counterfactual verdict\n");
    let mut buckets: BTreeMap<&str, usize> = BTreeMap::new();
    for (key, n) in &report.cases {
        let bucket = key.split(" :: ").next().unwrap_or(key);
        *buckets.entry(bucket).or_default() += n;
    }
    let mut rows: Vec<_> = buckets.iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    let mut verdict_sum = 0;
    for (bucket, n) in rows {
        verdict_sum += n;
        println!("  {n:>6}  {bucket}");
    }
    println!("\n  C2 verdicts sum {verdict_sum} vs classified {}", report.classified);
    println!("  C1 classified-but-not-gap:        {}  (expect 0)", report.c1_not_gap);
    println!("  C3 converting without a marker:   {}  (expect 0)", report.c3_no_marker);
    println!("  C4 cases skipped for misalignment:{}", report.c4_skipped_cases);
    println!("  C5 classified lines wanting `any`:{}  (expect 0)", report.c5_want_any);

    println!("\n## The misses, verbatim — what a forecast got wrong\n");
    let mut misses: Vec<_> = report.misses.iter().collect();
    misses.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (miss, n) in misses.into_iter().take(20) {
        println!("  {n:>5}  {miss}");
    }

    println!("\n## Top cases, by verdict\n");
    let mut cases: Vec<_> = report.cases.iter().collect();
    cases.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (case, n) in cases.into_iter().take(25) {
        println!("  {n:>6}  {case}");
    }
}
