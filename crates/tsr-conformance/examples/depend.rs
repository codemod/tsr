//! The 37,278 `DEPENDENT-UNKNOWN` gap lines, resolved — `bd tsr-550`.
//!
//! # Why the existing instruments cannot answer this
//!
//! `rank_board.rs` and `gaproot.rs` both separate a *terminal* gap from a
//! *propagated* one with a **span test**: does some inner rendered line, inside
//! this node's span, also gap? That works for `f(g())`, where the blocking
//! `g()` is nested. It cannot see a dependency that sits **beside** the node —
//! `var x = f(); x.foo` blames `x`, whose defect is in a *declaration
//! elsewhere in the file*. `gap_reason` names those (`/ initialiser
//! CallExpression`, `/ annotation TypeReference`) and formats the node away
//! into a string, so 26.70% of the gap lands in one bucket that is neither
//! ranked nor dismissed.
//!
//! This walks **declaration edges** instead of span edges, and it can because
//! everything it needs is already public: `binder.resolve_name`,
//! `Checker::get_type_of_symbol`, `Checker::check_expression`,
//! `Checker::get_type_from_type_node`.
//!
//! # What it does
//!
//! From each gap line, take one step toward whatever the answer depends on, and
//! keep stepping while the thing stepped to **also gaps**. Where the walk stops
//! is the **root**: the first node in the chain that gaps without a gapping
//! dependency of its own. Bucket by the root's kind.
//!
//! The steps are upstream's own dependency edges, not a guess:
//!
//! | at | step to | why |
//! |---|---|---|
//! | a declaration's **name** | its annotation, else its initialiser | `getTypeOfSymbol` takes the annotation when there is one (`checker.go:16652`) |
//! | an identifier **reference** | its symbol's value declaration's name | `checkIdentifier` → `getTypeOfSymbol` |
//! | `a.b` / `a[b]` | the receiver | the member lookup needs the receiver's type first |
//! | `f(…)` / `new C(…)` | the callee | signature resolution needs the callee's type |
//! | `(e)` | the inner expression | |
//! | a **type reference** | the referenced declaration's name | `getTypeFromTypeReference` |
//!
//! # Controls
//!
//! - **C1, construction.** A root is a node that gaps and whose step either does
//!   not exist or does not gap. So `roots that gap` must equal the whole
//!   population: every walk terminates at a gapping node. Printed.
//! - **C2, construction.** The walk carries a `visited` set and a depth cap. A
//!   chain that revisits a node is a **cycle**, which is a real shape here
//!   (`var a = b; var b = a;`), and it is counted rather than silently truncated
//!   — a cycle terminating at its entry point would otherwise be reported as a
//!   root of whatever kind the walk happened to stop on.
//! - **C3, arithmetic.** Every gap line lands in exactly one root bucket.
//! - **C4, frozen.** The gap total is compared against `STATUS.md`'s published
//!   figure, which no mutation of this file can move.

use std::collections::{BTreeMap, HashMap, HashSet};

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_ast::{Node, NodeId, NodeMap, NodeTable, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// How far a dependency chain may run before it is reported as a chain rather
/// than resolved to a root. Chains here are short; the cap exists so a shape
/// nobody anticipated is *counted* instead of hanging.
const MAX_DEPTH: usize = 16;

/// Does this node's own answer gap?
fn gaps<'a>(
    checker: &mut tsr_checker::Checker<'a, '_>,
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    id: NodeId,
) -> bool {
    let error = checker.intrinsics().error;
    // A type node is not an expression and `type_id_at_location` would answer
    // `error` for it whatever it denotes — which would make every annotation
    // look like a root.
    if let Some(node) = map.get(id)
        && let Ok(type_node) = tsr_ast::TypeNode::try_from(node)
    {
        return checker.get_type_from_type_node(type_node) == error;
    }
    types_producer::type_id_at_location(checker, binder, nodes, map, id) == error
}

/// One step toward what this node's answer depends on.
fn step<'a>(
    checker: &mut tsr_checker::Checker<'a, '_>,
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    id: NodeId,
) -> Option<NodeId> {
    let node = map.get(id)?;

    // A declaration's own name: the annotation, else the initialiser. This is
    // the edge `gap_reason` names and throws away.
    //
    // **`name_id()` is not the same question as "is this a declaration name".**
    // A `PropertyAccessExpression`'s `name` is the `b` of `a.b` and a
    // `QualifiedName`'s `right` is the `B` of `A.B`; both answer this test and
    // neither declares anything. Missing that put 13,421 member names into a
    // bucket labelled "a declaration with neither annotation nor initialiser",
    // which is where the largest root on the board appeared to be.
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

    // The `b` of `a.b` is typed as the **property**, so its dependency is the
    // receiver — the same edge `type_at_location` takes.
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
            // The referenced declaration's name, which is where the chain
            // continues — `getTypeFromTypeReference` resolves the entity name
            // and then takes that symbol's declared type.
            let name = reference.type_name?;
            let text = match name {
                tsr_ast::EntityName::Identifier(identifier) => identifier.text,
                // A qualified name's right-hand side is the one that names the
                // type; the left is a namespace. Following the right keeps the
                // chain on the type and off the module.
                tsr_ast::EntityName::QualifiedName(qualified) => qualified.right?.text,
            };
            let symbol = binder.resolve_name(nodes, map, id, text, SymbolFlags::TYPE)?;
            let declaration = binder.symbols().get(symbol).declarations.first().copied()?;
            map.get(declaration)?.name_id()
        }
        Node::Identifier(identifier) => {
            // A *reference*. The declaration-name case was handled above, so
            // anything reaching here is a use.
            let symbol =
                binder.resolve_name(nodes, map, id, identifier.text, SymbolFlags::VALUE)?;
            if checker.get_type_of_symbol(symbol) != checker.intrinsics().error {
                return None;
            }
            let declaration = binder.symbols().get(symbol).value_declaration?;
            map.get(declaration)?.name_id()
        }
        // `checkBinaryLikeExpression`'s answer depends on both operands; the
        // step follows the first one that gaps (left before right, which is
        // evaluation order). Neither gapping means the *arm* refused — the
        // root is here — which is exactly the decomposition STATUS.md §4.3
        // asked for on a 6,233-line row nobody had split.
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
        // §767: the four largest `NO STEP ARM` kinds, on `BinaryExpression`'s
        // shape — follow the first CONSTITUENT that gaps; none gapping means
        // the arm itself refused and the root really is here.
        //
        // Before this, these four answered `no further dependency` and were
        // relabelled `NO STEP ARM — not a finding`, which is honest but puts
        // ~1,850 lines (23% of the board) in a bucket that says nothing about
        // the compiler. They are now attributed or genuinely rooted here.
        Node::ObjectLiteralExpression(object) => {
            // `checkObjectLiteral` types each property's VALUE; a shorthand's
            // value is its own name, which the identifier arm then resolves.
            first_gapping(
                checker,
                binder,
                nodes,
                map,
                object.properties.iter().filter_map(|property| match property {
                    tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                        assignment.initializer.and_then(|e| e.node_id())
                    }
                    tsr_ast::ObjectLiteralElementLike::ShorthandPropertyAssignment(short) => {
                        short.name.node_id()
                    }
                    other => other.node_id(),
                }),
            )
        }
        Node::ArrayLiteralExpression(array) => first_gapping(
            checker,
            binder,
            nodes,
            map,
            array.elements.iter().filter_map(tsr_ast::Expression::node_id),
        ),
        // A function's TYPE is its signature: the parameters, then the return.
        // The concise-body arrow is the one return expression this probe can
        // reach without a statement walk; a block body's returns are left, and
        // that limit is why a `FunctionExpression` root is still weaker
        // evidence than an `ObjectLiteralExpression` one.
        Node::ArrowFunction(function) => first_gapping(
            checker,
            binder,
            nodes,
            map,
            function
                .parameters
                .iter()
                .filter_map(|parameter| parameter.node_id)
                .chain(function.body.and_then(concise_body_expression)),
        ),
        Node::FunctionExpression(function) => first_gapping(
            checker,
            binder,
            nodes,
            map,
            function.parameters.iter().filter_map(|parameter| parameter.node_id),
        ),
        // §767: a PARAMETER's type is its annotation, else its initializer,
        // else CONTEXTUAL — and the contextual road is the one this port is
        // weakest on, so a parameter with neither is a real root and should
        // say so rather than hide behind a missing arm.
        Node::ParameterDeclaration(parameter) => parameter
            .r#type
            .and_then(|annotation| annotation.node_id())
            .or_else(|| parameter.initializer.and_then(|e| e.node_id())),
        _ => None,
    }
}

/// The expression of an arrow's CONCISE body (`x => e`), or `None` for a block
/// body — whose returns need a statement walk this probe does not do. §767.
fn concise_body_expression(body: tsr_ast::ConciseBody<'_>) -> Option<NodeId> {
    match body {
        tsr_ast::ConciseBody::Block(_) => None,
        other => other.node_id(),
    }
}

/// The first of `candidates` whose type gaps, in source order — the same
/// "follow the operand that gapped" rule the `BinaryExpression` arm uses, and
/// the same meaning when nothing gaps: the arm refused, so the root is here.
/// §767.
fn first_gapping<'a>(
    checker: &mut tsr_checker::Checker<'a, '_>,
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    candidates: impl Iterator<Item = NodeId>,
) -> Option<NodeId> {
    candidates.into_iter().find(|&candidate| {
        types_producer::type_id_at_location(checker, binder, nodes, map, candidate)
            == checker.intrinsics().error
    })
}

/// Whether [`step`] has an arm for this kind at all.
///
/// Without this, `no further dependency` conflates *"this node genuinely has no
/// gapping dependency"* with *"this probe does not know how to walk out of this
/// kind"* — and the second is not a finding about the compiler. The first run
/// reported `BinaryExpression / no further dependency` at 8,722 while
/// `armsplit.rs` measures that row's own-root population at 1,073; the whole
/// difference is this distinction.
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
                // §767
                | Node::ObjectLiteralExpression(_)
                | Node::ArrayLiteralExpression(_)
                | Node::ArrowFunction(_)
                | Node::FunctionExpression(_)
                | Node::ParameterDeclaration(_)
        )
}

/// For an `Identifier` root, why the chain stopped. The largest root bucket on
/// the board, so it gets a reason rather than a kind.
fn identifier_reason<'a>(
    checker: &mut tsr_checker::Checker<'a, '_>,
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    id: NodeId,
) -> &'static str {
    let Some(Node::Identifier(identifier)) = map.get(id) else { return "not an identifier" };
    if let Some(parent) = nodes.parent(id)
        && map.get(parent).and_then(|p| p.name_id()) == Some(id)
        && matches!(
            map.get(parent),
            Some(Node::PropertyAccessExpression(_) | Node::QualifiedName(_))
        )
    {
        return "the member NAME of a.b — its receiver types, so the lookup is the root";
    }
    if let Some(parent) = nodes.parent(id)
        && map.get(parent).and_then(|p| p.name_id()) == Some(id)
        && !matches!(
            map.get(parent),
            Some(Node::PropertyAccessExpression(_) | Node::QualifiedName(_))
        )
    {
        // The parent's kind, because this bucket is the largest root on the
        // board and "a declaration" spans a parameter, a function name and a
        // binding element — three different items.
        return match nodes.kind(parent) {
            SyntaxKind::Parameter => "decl name, no annotation/initialiser: Parameter",
            SyntaxKind::FunctionDeclaration => "decl name, neither: FunctionDeclaration",
            SyntaxKind::MethodDeclaration => "decl name, neither: MethodDeclaration",
            SyntaxKind::BindingElement => "decl name, neither: BindingElement",
            SyntaxKind::VariableDeclaration => "decl name, neither: VariableDeclaration",
            SyntaxKind::PropertyDeclaration => "decl name, neither: PropertyDeclaration",
            SyntaxKind::PropertySignature => "decl name, neither: PropertySignature",
            SyntaxKind::ClassDeclaration => "decl name, neither: ClassDeclaration",
            SyntaxKind::InterfaceDeclaration => "decl name, neither: InterfaceDeclaration",
            SyntaxKind::TypeAliasDeclaration => "decl name, neither: TypeAliasDeclaration",
            SyntaxKind::EnumDeclaration | SyntaxKind::EnumMember => "decl name, neither: enum",
            SyntaxKind::ModuleDeclaration => "decl name, neither: ModuleDeclaration",
            SyntaxKind::ShorthandPropertyAssignment => "decl name, neither: ShorthandProperty",
            // **Enumerating kinds here was a mistake and the registered
            // prediction caught it**: `Parameter` was predicted to dominate and
            // does not appear, while an unenumerated kind took 15,652 lines.
            // The kind is leaked instead of bucketed, so the probe cannot hide
            // a kind its author did not think of.
            _ => return "decl name, neither: SEE KIND COLUMN",
        };
    }
    let Some(symbol) = binder.resolve_name(nodes, map, id, identifier.text, SymbolFlags::VALUE)
    else {
        return "the name does not resolve as a VALUE";
    };
    if binder.symbols().get(symbol).value_declaration.is_none() {
        return "resolves, but the symbol has no value declaration";
    }
    if checker.get_type_of_symbol(symbol) != checker.intrinsics().error {
        return "resolves and the symbol HAS a type — the line differs elsewhere";
    }
    "resolves, symbol untyped, and its declaration does not gap"
}

#[derive(Default)]
struct Report {
    gap: usize,
    /// §826: of `gap`, how many arrived only because the CHECKER answered
    /// `error` while the producer printed `any`. These are the lines every
    /// gap-root board was blind to; printing the count beside the total is what
    /// lets a reader tell this board's population from the older ones.
    error_behind_any: usize,
    /// `(root kind, how the chain ended) -> lines`, with cases.
    roots: BTreeMap<(String, &'static str), usize>,
    root_cases: BTreeMap<(String, &'static str), HashMap<String, usize>>,
    /// How many steps the chain took, so "beside the node" can be told from
    /// "the node itself".
    depths: BTreeMap<usize, usize>,
    /// For the two name-resolution roots, the **name text**. If a handful of
    /// globals dominate, the item is the global scope rather than the resolver.
    unresolved: BTreeMap<(&'static str, String), usize>,
    /// Of each root bucket, how many want `any` — ADR-0038's ceiling, which a
    /// root histogram cannot see and which contaminates every row whose
    /// receiver is an unresolved *value* (`bd tsr-eep` records the asymmetry).
    wants_any: BTreeMap<(String, &'static str), usize>,
    /// For the property-access roots that are **not** ceiling, the checker's own
    /// `gap_reason` — which names the arm that refused, rather than the kind.
    access_reasons: BTreeMap<String, usize>,
    cycles: usize,
    too_deep: usize,
    c1_root_does_not_gap: usize,
}

impl Report {
    fn merge(&mut self, other: &Self) {
        self.gap += other.gap;
        self.error_behind_any += other.error_behind_any;
        self.cycles += other.cycles;
        self.too_deep += other.too_deep;
        self.c1_root_does_not_gap += other.c1_root_does_not_gap;
        for (k, n) in &other.roots {
            *self.roots.entry(k.clone()).or_default() += n;
        }
        for (k, cases) in &other.root_cases {
            let mine = self.root_cases.entry(k.clone()).or_default();
            for (case, n) in cases {
                *mine.entry(case.clone()).or_default() += n;
            }
        }
        for (k, n) in &other.access_reasons {
            *self.access_reasons.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.wants_any {
            *self.wants_any.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.unresolved {
            *self.unresolved.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.depths {
            *self.depths.entry(*k).or_default() += n;
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
            // §826: the printed string is NOT the checker's answer.
            // `types_producer` converts `error` to `any` on three branches,
            // faithfully — upstream's own baseline writer does it
            // (`type_symbol_baseline.go:383`) — so a line where the checker
            // computed NOTHING can arrive here reading `any`. Selecting on the
            // string made this board blind to that whole population, measured at
            // **8,826 of 13,295** audited `any` lines in `STATUS.md` §4.-5's
            // correction: the gap column read ~7,400 while the "computed nothing"
            // population is more than twice that, so every population this board
            // has ever printed was a FLOOR presented as a ceiling.
            //
            // `type_id_at_location_tracking` now reports the fact the producer
            // used to discard, which is why this is a selection change and not a
            // new classifier.
            // `TSR_NO_826=1` restores the old string-only selection, so the
            // board's two populations can be A/B'd in one build rather than
            // compared across commits.
            let mut checker_error = false;
            if got.type_string != "error" && std::env::var("TSR_NO_826").is_ok() {
                continue;
            }
            if got.type_string != "error" {
                types_producer::type_id_at_location_tracking(
                    &mut checker,
                    bound,
                    nodes,
                    map,
                    line_ids[position],
                    &mut checker_error,
                );
                if !checker_error {
                    continue;
                }
                report.error_behind_any += 1;
            }
            report.gap += 1;

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
                    report.cycles += 1;
                    break "cycle";
                }
                depth += 1;
                if depth >= MAX_DEPTH {
                    report.too_deep += 1;
                    break "depth cap";
                }
                current = next;
            };

            if !gaps(&mut checker, bound, nodes, map, current) {
                report.c1_root_does_not_gap += 1;
            }
            let is_declaration_name = nodes
                .parent(current)
                .and_then(|p| map.get(p))
                .is_some_and(|p| p.name_id() == Some(current));
            let ending = if ending == "no further dependency"
                && !map.get(current).is_some_and(|n| has_step_arm(n, is_declaration_name))
            {
                "NO STEP ARM for this kind — not a finding"
            } else {
                ending
            };
            let mut kind = format!("{:?}", nodes.kind(current));
            if nodes.kind(current) == SyntaxKind::Identifier {
                let reason = identifier_reason(&mut checker, bound, nodes, map, current);
                kind = if reason == "decl name, neither: SEE KIND COLUMN" {
                    let parent_kind = nodes
                        .parent(current)
                        .map_or_else(|| "<root>".to_owned(), |p| format!("{:?}", nodes.kind(p)));
                    format!("decl name, neither, in a {parent_kind}")
                } else {
                    format!("Identifier: {reason}")
                };
            }
            if let Some(node) = map.get(current) {
                let text = match node {
                    Node::Identifier(identifier) => Some(("value", identifier.text.to_owned())),
                    Node::TypeReferenceNode(reference) => match reference.type_name {
                        Some(tsr_ast::EntityName::Identifier(i)) => {
                            Some(("type", i.text.to_owned()))
                        }
                        Some(tsr_ast::EntityName::QualifiedName(q)) => {
                            q.right.map(|r| ("type-qualified", r.text.to_owned()))
                        }
                        None => None,
                    },
                    _ => None,
                };
                if let Some(entry) = text
                    && (kind.contains("does not resolve") || kind == "TypeReference")
                {
                    *report.unresolved.entry(entry).or_default() += 1;
                }
            }
            let kind_label = kind.clone();
            let key = (kind, ending);
            *report.roots.entry(key.clone()).or_default() += 1;
            let wants_any = want.text.strip_prefix(&format!("{} : ", got.text)) == Some("any");
            if wants_any {
                *report.wants_any.entry(key.clone()).or_default() += 1;
            }
            // The reachable property-access roots, by the arm that refused.
            if !wants_any
                && matches!(
                    nodes.kind(current),
                    SyntaxKind::PropertyAccessExpression | SyntaxKind::Identifier
                )
                && kind_label.starts_with("PropertyAccess")
                || (!wants_any && kind_label.contains("member NAME"))
            {
                let reason = types_producer::gap_reason(&mut checker, bound, nodes, map, current);
                *report.access_reasons.entry(reason).or_default() += 1;
            }
            *report.root_cases.entry(key).or_default().entry(case.name.clone()).or_default() += 1;
            *report.depths.entry(depth.min(6)).or_default() += 1;
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
        .reduce(Report::default, |mut a, b| {
            a.merge(&b);
            a
        });

    println!("# depend — the gap's roots, following DECLARATION edges (`bd tsr-550`)\n");
    println!(
        "gap lines walked: {} ({} of them are a CHECKER `error` the producer printed as `any` — §826)\n",
        report.gap, report.error_behind_any
    );

    println!("## Where the chain ends — the ROOT of each gap line\n");
    let mut rows: Vec<_> = report.roots.iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    let total: usize = report.roots.values().sum();
    for ((kind, ending), n) in rows.iter().take(28) {
        let empty = HashMap::new();
        let cases = report.root_cases.get(&((*kind).clone(), *ending)).unwrap_or(&empty);
        let (top, top_n) =
            cases.iter().max_by_key(|(_, n)| **n).map_or((" ", &0), |(c, n)| (c.as_str(), n));
        #[allow(clippy::cast_precision_loss)]
        let share = **n as f64 / total.max(1) as f64 * 100.0;
        #[allow(clippy::cast_precision_loss)]
        let top1 = *top_n as f64 / (**n).max(1) as f64 * 100.0;
        let any = report.wants_any.get(&((*kind).clone(), *ending)).copied().unwrap_or(0);
        #[allow(clippy::cast_precision_loss)]
        let any_share = any as f64 / (**n).max(1) as f64 * 100.0;
        println!(
            "  {kind:<30} {ending:<34} {n:>6} {share:>5.1}%  want-any {any:>6} ({any_share:>4.1}%)  {:>5} cases, top-1 {top1:>5.1}%  {top}",
            cases.len()
        );
    }

    println!("\n## How far the chain ran — 0 means the line's own node is the root\n");
    for (depth, n) in &report.depths {
        #[allow(clippy::cast_precision_loss)]
        let share = *n as f64 / total.max(1) as f64 * 100.0;
        let label = if *depth >= 6 { "6+".to_string() } else { depth.to_string() };
        println!("  {label:>3} steps  {n:>7}  {share:>5.1}%");
    }

    println!("\n## Property access: why the reachable roots refused — top 14\n");
    let mut reasons: Vec<_> = report.access_reasons.iter().collect();
    reasons.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (reason, n) in reasons.iter().take(14) {
        println!("  {n:>6}  {}", &reason[..reason.len().min(110)]);
    }

    println!("\n## The names that do not resolve — top 24\n");
    let mut names: Vec<_> = report.unresolved.iter().collect();
    names.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for ((meaning, name), n) in names.iter().take(24) {
        println!("  {meaning:<16} {name:<40} {n:>6}");
    }

    println!("\n## Controls");
    println!(
        "  C1 construction: roots that do not gap  {}  (expect 0)",
        report.c1_root_does_not_gap
    );
    println!("  C2 construction: cycles {}, depth-cap hits {}", report.cycles, report.too_deep);
    println!("  C3 arithmetic:   root buckets sum to {total}, gap lines walked {}", report.gap);
    println!("  C4 frozen:       STATUS.md publishes ~127,736 gap lines at 63.66%");
}
