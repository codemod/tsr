//! What the gap board does **not** attribute — the `cycle` and `depth cap`
//! endings, and what every large row actually *wants*.
//!
//! # Why this is not `depend.rs`
//!
//! `depend.rs` buckets a gap line by the **kind of the node its walk stopped
//! on**. Two of its endings are not roots at all:
//!
//! - `cycle` (5,394 lines at `7299a14`) — the walk revisited a node, so it
//!   stopped on whichever member of the loop it happened to re-enter. The kind
//!   printed for those rows (`FunctionDeclaration cycle` 2,943,
//!   `BindingElement cycle` 2,363) names *an arbitrary member of a cycle*, not a
//!   cause. `STATUS.md` §4.0 lists both among head rows described as "all
//!   owned"; neither has an owner, a probe, or a `bd` issue anywhere in `docs/`.
//! - `depth cap` (1,330) — same problem, plus a known single pathological case.
//!
//! A kind histogram also cannot say what the *answer* was supposed to be. A row
//! of 2,120 lines is a different item depending on whether the baseline wants a
//! signature, a union, or a string literal — and that is recoverable for free,
//! because the baseline text is already parsed to decide the line gaps.
//!
//! # What it adds
//!
//! 1. Every gap line's **want shape** — the syntactic form of the type upstream
//!    prints — cross-tabbed against the board's rows.
//! 2. For the cycle ending: the **kind sequence of the loop itself**, so a real
//!    recursive shape (`var a = b; var b = a;`) can be told from a probe artefact.
//!
//! # Controls
//!
//! - **C1, arithmetic.** Every walked line lands in exactly one ending bucket
//!   and one want-shape bucket; both sums are printed against the walked total.
//! - **C2, frozen.** The cycle and depth-cap counts must reproduce `depend.rs`'s
//!   5,394 / 1,330 at the same commit. The walk is copied from it verbatim; a
//!   disagreement means the copy drifted and nothing else here is readable.
//! - **C3, construction.** `want-any` is reported per row, because ADR-0038 puts
//!   those lines beyond reach whatever the shape says.

use std::collections::{BTreeMap, HashMap, HashSet};

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Node, NodeId, NodeMap, NodeTable, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

const MAX_DEPTH: usize = 16;

/// Does this node's own answer gap? Copied from `depend.rs` — see C2.
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

/// One step toward what this node's answer depends on. Copied from `depend.rs`.
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

/// Whether [`step`] has an arm for this kind at all. Copied from `depend.rs`.
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

/// The syntactic form of the type the baseline prints.
///
/// The order of the tests is load-bearing and is **outside-in**: a union of
/// signatures is a union, an array of unions is an array. Classifying by the
/// first thing found instead would put `(() => void)[]` under "signature", which
/// is the wrong owner — the missing mechanism is the array, not the signature.
fn want_shape(want: &str) -> &'static str {
    let want = want.trim();
    if want.is_empty() {
        return "<empty>";
    }
    match want {
        "any" => return "any (ADR-0038)",
        "error" => return "error",
        "never" | "unknown" | "void" | "undefined" | "null" | "string" | "number" | "boolean"
        | "bigint" | "symbol" | "object" | "this" => return "primitive",
        _ => {}
    }
    // Top-level splits first: scan at bracket depth 0 so a nested `|` inside a
    // type argument does not read as a union.
    let bytes = want.as_bytes();
    let (mut depth, mut top_union, mut top_intersection, mut top_arrow) =
        (0i32, false, false, false);
    for (index, byte) in bytes.iter().enumerate() {
        match byte {
            b'(' | b'[' | b'{' | b'<' => depth += 1,
            b')' | b']' | b'}' | b'>' => {
                // `=>` closes nothing; without this every arrow unbalances the
                // depth counter and the rest of the string reads as nested.
                if *byte == b'>' && index > 0 && bytes[index - 1] == b'=' {
                    if depth == 0 {
                        top_arrow = true;
                    }
                } else {
                    depth -= 1;
                }
            }
            b'|' if depth == 0 => top_union = true,
            b'&' if depth == 0 => top_intersection = true,
            _ => {}
        }
    }
    if top_union {
        return "union";
    }
    if top_intersection {
        return "intersection";
    }
    if want.ends_with("[]") && depth == 0 {
        return "array";
    }
    if top_arrow || want.starts_with('(') {
        return "signature / arrow";
    }
    if want.starts_with('{') {
        return "anonymous object";
    }
    if want.starts_with('[') {
        return "tuple";
    }
    if want.starts_with('"') || want.starts_with('\'') || want.starts_with('`') {
        return "string literal";
    }
    if want.starts_with("typeof ") {
        return "typeof query";
    }
    if want.starts_with("import(") {
        return "import(…)";
    }
    if want.contains('.') {
        return "qualified name";
    }
    if want.contains('<') {
        return "generic reference";
    }
    if want.bytes().all(|b| b.is_ascii_digit() || b == b'.' || b == b'-') {
        return "numeric literal";
    }
    "bare name"
}

#[derive(Default)]
struct Report {
    walked: usize,
    /// `(ending, root kind) -> lines`
    rows: BTreeMap<(&'static str, String), usize>,
    row_any: BTreeMap<(&'static str, String), usize>,
    row_shapes: BTreeMap<(&'static str, String), BTreeMap<&'static str, usize>>,
    row_cases: BTreeMap<(&'static str, String), HashMap<String, usize>>,
    /// Ending totals, for C1.
    endings: BTreeMap<&'static str, usize>,
    /// The kind sequence of the loop, for the cycle ending only.
    cycle_shapes: BTreeMap<String, usize>,
    /// For a **length-1** cycle — a node whose step is itself — what the node
    /// declares and whether it has an annotation or an initialiser. A self-loop
    /// is not the recursive shape `depend.rs`'s C2 documents; it is the walk
    /// having no edge to take, which is the `NO STEP ARM` finding wearing the
    /// `cycle` label.
    self_loops: BTreeMap<String, usize>,
    /// For a `NO STEP ARM` type-node root, whether a **child type node** gaps —
    /// i.e. whether an arm would have somewhere to go.
    unwalked_children: BTreeMap<(String, bool), usize>,
    /// Whole-corpus want-shape histogram over the gap.
    shapes: BTreeMap<&'static str, usize>,
    cycles: usize,
    too_deep: usize,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        self.walked += other.walked;
        self.cycles += other.cycles;
        self.too_deep += other.too_deep;
        for (k, n) in &other.rows {
            *self.rows.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.row_any {
            *self.row_any.entry(k.clone()).or_default() += n;
        }
        for (k, shapes) in &other.row_shapes {
            let mine = self.row_shapes.entry(k.clone()).or_default();
            for (shape, n) in shapes {
                *mine.entry(shape).or_default() += n;
            }
        }
        for (k, cases) in &other.row_cases {
            let mine = self.row_cases.entry(k.clone()).or_default();
            for (case, n) in cases {
                *mine.entry(case.clone()).or_default() += n;
            }
        }
        for (k, n) in &other.endings {
            *self.endings.entry(k).or_default() += n;
        }
        for (k, n) in &other.cycle_shapes {
            *self.cycle_shapes.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.self_loops {
            *self.self_loops.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.unwalked_children {
            *self.unwalked_children.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.shapes {
            *self.shapes.entry(k).or_default() += n;
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
            let Some(want_type) = want.text.strip_prefix(&format!("{} : ", got.text)) else {
                continue;
            };
            if got.type_string != "error" {
                continue;
            }
            report.walked += 1;

            let mut current = line_ids[position];
            let mut chain: Vec<NodeId> = vec![current];
            let mut visited: HashSet<NodeId> = HashSet::new();
            visited.insert(current);
            let mut depth = 0usize;
            let mut loop_start = None;
            let ending = loop {
                let Some(next) = step(&mut checker, bound, nodes, map, current) else {
                    break "no further dependency";
                };
                if !gaps(&mut checker, bound, nodes, map, next) {
                    break "the dependency types — the root is here";
                }
                if !visited.insert(next) {
                    report.cycles += 1;
                    loop_start = chain.iter().position(|id| *id == next);
                    break "cycle";
                }
                chain.push(next);
                depth += 1;
                if depth >= MAX_DEPTH {
                    report.too_deep += 1;
                    break "depth cap";
                }
                current = next;
            };

            let is_declaration_name = nodes
                .parent(current)
                .and_then(|p| map.get(p))
                .is_some_and(|p| p.name_id() == Some(current));
            let ending = if ending == "no further dependency"
                && !map.get(current).is_some_and(|n| has_step_arm(n, is_declaration_name))
            {
                "NO STEP ARM"
            } else {
                ending
            };

            let kind = if nodes.kind(current) == SyntaxKind::Identifier {
                let parent = nodes
                    .parent(current)
                    .map_or_else(|| "<root>".to_owned(), |p| format!("{:?}", nodes.kind(p)));
                format!("Identifier in {parent}")
            } else {
                format!("{:?}", nodes.kind(current))
            };

            if ending == "cycle"
                && loop_start == Some(chain.len() - 1)
                && let Some(node) = map.get(current)
            {
                let parent_kind = nodes
                    .parent(current)
                    .map_or_else(|| "<root>".to_owned(), |p| format!("{:?}", nodes.kind(p)));
                let parent_node = nodes.parent(current).and_then(|p| map.get(p));
                let annotation = parent_node.and_then(|p| p.type_id()).is_some();
                let initialiser = parent_node.and_then(|p| p.initializer_id()).is_some();
                let _ = node;
                *report
                    .self_loops
                    .entry(format!(
                        "{parent_kind:<24} annotation {annotation:<5} initialiser {initialiser}"
                    ))
                    .or_default() += 1;
            }

            if ending == "NO STEP ARM"
                && let Some(node) = map.get(current)
                && tsr_ast::TypeNode::try_from(node).is_ok()
            {
                let mut child_gaps = false;
                tsr_ast::for_each_child_id(node, |child| {
                    if map.get(child).is_some_and(|c| tsr_ast::TypeNode::try_from(c).is_ok())
                        && gaps(&mut checker, bound, nodes, map, child)
                    {
                        child_gaps = true;
                    }
                });
                *report
                    .unwalked_children
                    .entry((format!("{:?}", nodes.kind(current)), child_gaps))
                    .or_default() += 1;
            }

            if ending == "cycle"
                && let Some(start) = loop_start
            {
                let shape = chain[start..]
                    .iter()
                    .map(|id| format!("{:?}", nodes.kind(*id)))
                    .collect::<Vec<_>>()
                    .join(" -> ");
                *report.cycle_shapes.entry(shape).or_default() += 1;
            }

            let shape = want_shape(want_type);
            let key = (ending, kind);
            *report.rows.entry(key.clone()).or_default() += 1;
            *report.row_shapes.entry(key.clone()).or_default().entry(shape).or_default() += 1;
            *report
                .row_cases
                .entry(key.clone())
                .or_default()
                .entry(case.name.clone())
                .or_default() += 1;
            if want_type == "any" {
                *report.row_any.entry(key).or_default() += 1;
            }
            *report.endings.entry(ending).or_default() += 1;
            *report.shapes.entry(shape).or_default() += 1;
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

    println!("# cyclegap — what the board does not attribute, and what the gap WANTS\n");
    println!("gap lines walked: {}\n", report.walked);

    println!("## The gap by what upstream's answer LOOKS LIKE\n");
    let mut shapes: Vec<_> = report.shapes.iter().collect();
    shapes.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (shape, n) in &shapes {
        let share = **n as f64 / report.walked.max(1) as f64 * 100.0;
        println!("  {shape:<20} {n:>6}  {share:>5.1}%");
    }

    println!("\n## Endings\n");
    let mut endings: Vec<_> = report.endings.iter().collect();
    endings.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (ending, n) in &endings {
        let share = **n as f64 / report.walked.max(1) as f64 * 100.0;
        println!("  {ending:<40} {n:>6}  {share:>5.1}%");
    }

    println!("\n## The UNATTRIBUTED endings — cycle and depth cap, by node kind\n");
    let mut rows: Vec<_> =
        report.rows.iter().filter(|((e, _), _)| *e == "cycle" || *e == "depth cap").collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for ((ending, kind), n) in rows.iter().take(14) {
        let empty = HashMap::new();
        let cases = report.row_cases.get(&((*ending), (*kind).clone())).unwrap_or(&empty);
        let (top, top_n) =
            cases.iter().max_by_key(|(_, n)| **n).map_or((" ", &0), |(c, n)| (c.as_str(), n));
        let any = report.row_any.get(&((*ending), (*kind).clone())).copied().unwrap_or(0);
        let empty_shapes = BTreeMap::new();
        let shapes = report.row_shapes.get(&((*ending), (*kind).clone())).unwrap_or(&empty_shapes);
        let mut top_shapes: Vec<_> = shapes.iter().collect();
        top_shapes.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
        let shape_text = top_shapes
            .iter()
            .take(3)
            .map(|(s, n)| format!("{s} {n}"))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "  {ending:<11} {kind:<34} {n:>5}  want-any {any:>5} ({:>4.1}%)  {:>4} cases, top-1 {:>5.1}% {top}\n              wants: {shape_text}",
            any as f64 / (**n).max(1) as f64 * 100.0,
            cases.len(),
            *top_n as f64 / (**n).max(1) as f64 * 100.0,
        );
    }

    println!("\n## The cycles themselves — the kind sequence of the loop, top 16\n");
    let mut cycle_shapes: Vec<_> = report.cycle_shapes.iter().collect();
    cycle_shapes.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (shape, n) in cycle_shapes.iter().take(16) {
        println!("  {n:>5}  {}", &shape[..shape.len().min(120)]);
    }

    println!("\n## Length-1 cycles — a node whose step is ITSELF, by what it declares\n");
    let mut self_loops: Vec<_> = report.self_loops.iter().collect();
    self_loops.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (shape, n) in self_loops.iter().take(12) {
        println!("  {n:>5}  {shape}");
    }

    println!("\n## `NO STEP ARM` TYPE-NODE roots — would an arm have anywhere to go?\n");
    let mut unwalked: Vec<_> = report.unwalked_children.iter().collect();
    unwalked.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for ((kind, child_gaps), n) in unwalked.iter().take(20) {
        let label = if *child_gaps { "a child TYPE NODE gaps" } else { "no child type node gaps" };
        println!("  {n:>5}  {kind:<24} {label}");
    }
    let propagating: usize =
        report.unwalked_children.iter().filter(|((_, c), _)| *c).map(|(_, n)| n).sum();
    let terminal: usize =
        report.unwalked_children.iter().filter(|((_, c), _)| !*c).map(|(_, n)| n).sum();
    println!(
        "\n  TOTAL type-node roots with no arm: {} — {propagating} PROPAGATING (an arm would step \
         inward), {terminal} terminal",
        propagating + terminal
    );

    println!("\n## Every row above 700 lines, with what it wants\n");
    let mut rows: Vec<_> = report.rows.iter().filter(|(_, n)| **n >= 700).collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for ((ending, kind), n) in &rows {
        let any = report.row_any.get(&((*ending), (*kind).clone())).copied().unwrap_or(0);
        let empty_shapes = BTreeMap::new();
        let shapes = report.row_shapes.get(&((*ending), (*kind).clone())).unwrap_or(&empty_shapes);
        let mut top_shapes: Vec<_> = shapes.iter().collect();
        top_shapes.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
        let shape_text = top_shapes
            .iter()
            .take(4)
            .map(|(s, n)| format!("{s} {n}"))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "  {n:>6}  net {:>6}  {kind:<36} {ending:<36}\n            wants: {shape_text}",
            **n - any,
        );
    }

    println!("\n## Controls");
    let ending_total: usize = report.endings.values().sum();
    let shape_total: usize = report.shapes.values().sum();
    println!(
        "  C1 arithmetic: endings sum {ending_total}, shapes sum {shape_total}, walked {}",
        report.walked
    );
    println!(
        "  C2 frozen:     cycles {} (depend.rs at 7299a14: 5394), depth-cap {} (1330)",
        report.cycles, report.too_deep
    );
    println!(
        "  C3 want-any is reported per row — ADR-0038 lines are out of reach whatever the shape"
    );
}
